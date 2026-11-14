//! What stands in for a sign-in service in the tests: something that signs tokens as a
//! real one does, with keys that exist for the tests alone, and a service on this
//! machine that signs one person in without asking them anything.
//!
//! It is built only for tests: nothing of it is in a release.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use away_part::address::read_form;
use away_part::base64;
use away_part::http::{answer, carried, plain, Answer, Answers, Listener};
use away_part::json::Json;
use away_part::{time, Lines, Measures, Stopper};
use hyper::header::{HeaderValue, AUTHORIZATION, LOCATION};
use hyper::{Method, StatusCode};
use ring::rand::SystemRandom;
use ring::signature::{self, EcdsaKeyPair, KeyPair, RsaKeyPair, RsaPublicKeyComponents};
use sha2::{Digest, Sha256};

use crate::sign_ins::random_name;

enum Key {
    Curve(EcdsaKeyPair),
    Rsa(RsaKeyPair),
}

/// Signs tokens, and publishes the key to check them with.
pub struct Signer {
    way: &'static str,
    name: String,
    key: Key,
}

impl Signer {
    /// A signer with a new key on a curve.
    pub fn curve() -> Signer {
        let random = SystemRandom::new();
        let made = EcdsaKeyPair::generate_pkcs8(&signature::ECDSA_P256_SHA256_FIXED_SIGNING, &random).expect("a new key");
        let key = EcdsaKeyPair::from_pkcs8(&signature::ECDSA_P256_SHA256_FIXED_SIGNING, made.as_ref(), &random).expect("the key just made");
        Signer { way: "ES256", name: "2026".to_string(), key: Key::Curve(key) }
    }

    /// A signer with the tests' RSA key, that signs this way: `RS256`, `PS256`, `RS512`.
    pub fn rsa(way: &'static str) -> Signer {
        let key = RsaKeyPair::from_pkcs8(include_bytes!("../tests/fixtures/service.key.der")).expect("the tests' key");
        Signer { way, name: "2026".to_string(), key: Key::Rsa(key) }
    }

    /// The same signer, with another name for its key.
    pub fn named(self, name: &str) -> Signer {
        Signer { name: name.to_string(), ..self }
    }

    /// The way it signs.
    pub fn way(&self) -> &'static str {
        self.way
    }

    /// The first part of a token that says it is signed this way with this signer's key.
    pub fn head(&self, way: &str) -> String {
        base64::for_addresses(
            Json::object([("alg", way.into()), ("kid", self.name.as_str().into()), ("typ", "JWT".into())]).write().as_bytes(),
        )
    }

    /// The keys as a sign-in service publishes them.
    pub fn published(&self) -> Json {
        let letters = |bytes: &[u8]| Json::Text(base64::for_addresses(bytes));
        let key = match &self.key {
            Key::Curve(key) => {
                let point = key.public_key().as_ref();
                Json::object([
                    ("kty", "EC".into()),
                    ("crv", "P-256".into()),
                    ("use", "sig".into()),
                    ("kid", self.name.as_str().into()),
                    ("x", letters(&point[1..33])),
                    ("y", letters(&point[33..])),
                ])
            }
            Key::Rsa(key) => {
                let parts: RsaPublicKeyComponents<Vec<u8>> = RsaPublicKeyComponents::from(key.public());
                Json::object([
                    ("kty", "RSA".into()),
                    ("use", "sig".into()),
                    ("kid", self.name.as_str().into()),
                    ("n", letters(&parts.n)),
                    ("e", letters(&parts.e)),
                ])
            }
        };
        Json::object([("keys", Json::List(vec![key]))])
    }

    /// A token that says this, signed.
    pub fn sign(&self, says: &Json) -> String {
        let random = SystemRandom::new();
        let message = format!("{}.{}", self.head(self.way), base64::for_addresses(says.write().as_bytes()));
        let signed = match &self.key {
            Key::Curve(key) => key.sign(&random, message.as_bytes()).expect("a signature").as_ref().to_vec(),
            Key::Rsa(key) => {
                let how: &dyn signature::RsaEncoding = match self.way {
                    "RS256" => &signature::RSA_PKCS1_SHA256,
                    "RS384" => &signature::RSA_PKCS1_SHA384,
                    "RS512" => &signature::RSA_PKCS1_SHA512,
                    "PS256" => &signature::RSA_PSS_SHA256,
                    "PS384" => &signature::RSA_PSS_SHA384,
                    _ => &signature::RSA_PSS_SHA512,
                };
                let mut signed = vec![0; key.public().modulus_len()];
                key.sign(how, &random, message.as_bytes(), &mut signed).expect("a signature");
                signed
            }
        };
        format!("{message}.{}", base64::for_addresses(&signed))
    }
}

struct Inner {
    /// Where it is, whatever it names itself.
    at: String,
    names_itself: String,
    signer: Signer,
    client: (String, String),
    back: String,
    says: Json,
    good_for: u64,
    sends_back_with: Option<String>,
    refuses_renewals: bool,
    codes: HashMap<String, (String, String)>,
    renewals: Vec<String>,
    renewed: u64,
    forgotten: Vec<String>,
}

/// A sign-in service on this machine that signs in one person without asking them
/// anything, the way a real one does once somebody has typed their password.
pub struct SignInService {
    address: String,
    inner: Arc<Mutex<Inner>>,
    stopper: Stopper,
}

impl SignInService {
    /// Starts one that knows Away by this name and this secret, and sends people back
    /// to this address.
    pub async fn start(client: &str, secret: &str, back: &str) -> SignInService {
        SignInService::start_at("127.0.0.1:0", client, secret, back).await
    }

    /// The same, at an address that is chosen: `127.0.0.1:18081`.
    ///
    /// Tests that run in a browser tell it how to behave by sending it a form:
    /// `POST /stand-in?refuses_renewals=true`. The names are `refuses_renewals` (true or
    /// false), `sends_back_with` (an error, or nothing), `signs_for` (seconds), `person`
    /// (`administrator` or `person`) and `reset`, which makes it as it was when it started.
    pub async fn start_at(at: &str, client: &str, secret: &str, back: &str) -> SignInService {
        let listener = Listener::bind("sign-in service", at).await.expect("an address");
        let address = format!("http://127.0.0.1:{}", listener.address().port());
        let inner = Arc::new(Mutex::new(Inner {
            at: address.clone(),
            names_itself: address.clone(),
            signer: Signer::curve(),
            client: (client.to_string(), secret.to_string()),
            back: back.to_string(),
            says: administrator(),
            good_for: 300,
            sends_back_with: None,
            refuses_renewals: false,
            codes: HashMap::new(),
            renewals: Vec::new(),
            renewed: 0,
            forgotten: Vec::new(),
        }));
        let (stopper, stop) = away_part::stop::pair();
        let answers: Answers = {
            let inner = inner.clone();
            Arc::new(move |request| {
                let inner = inner.clone();
                Box::pin(async move {
                    let path = request.uri().path().to_string();
                    let asked = request.uri().query().unwrap_or_default().to_string();
                    let reading = request.method() == Method::GET;
                    let named = request.headers().get(AUTHORIZATION).and_then(|named| named.to_str().ok()).map(str::to_string);
                    let sent = carried(request, 16_384).await.unwrap_or_default();
                    let sent = read_form(std::str::from_utf8(&sent).unwrap_or_default()).unwrap_or_default();
                    let mut inner = inner.lock().expect("the stand-in's state");
                    match (path.as_str(), reading) {
                        ("/.well-known/openid-configuration", true) => inner.says_of_itself(),
                        ("/keys", true) => json(StatusCode::OK, inner.signer.published()),
                        ("/authorize", true) => inner.sign_in_page(&read_form(&asked).unwrap_or_default()),
                        ("/token", false) => inner.tokens(named.as_deref(), &sent),
                        ("/stand-in", false) => {
                            inner.told(&read_form(&asked).unwrap_or_default());
                            json(StatusCode::OK, Json::object([]))
                        }
                        ("/revoke", false) => {
                            inner.forgotten.extend(sent.iter().filter(|(name, _)| name == "token").map(|(_, token)| token.clone()));
                            json(StatusCode::OK, Json::object([]))
                        }
                        _ => plain(StatusCode::NOT_FOUND, "Nothing is at this address.\n"),
                    }
                })
            })
        };
        tokio::spawn(listener.answer_with(answers, stop, Lines::kept_for("sign-in service"), Measures::of("sign-in service")));
        SignInService { address, inner, stopper }
    }

    fn inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("the stand-in's state")
    }

    /// Where it is, which is also what it names itself.
    pub fn address(&self) -> String {
        self.address.clone()
    }

    /// Who it signs in from now on, and says at every renewal.
    pub fn signs_in(&self, says: Json) {
        self.inner().says = says;
    }

    /// How long what it signs is good for, in seconds.
    pub fn signs_for(&self, seconds: u64) {
        self.inner().good_for = seconds;
    }

    /// From now on it sends people back with this error and without signing them in,
    /// as when somebody gives up on the sign-in page.
    pub fn sends_back_with(&self, error: Option<&str>) {
        self.inner().sends_back_with = error.map(str::to_string);
    }

    /// Whether it refuses every renewal from now on, as when an account is closed.
    pub fn refuses_renewals(&self, refuses: bool) {
        self.inner().refuses_renewals = refuses;
    }

    /// From now on it names itself something else than Away is told.
    pub fn names_itself(&self, name: &str) {
        self.inner().names_itself = name.to_string();
    }

    /// It signs with a new key from now on, and publishes that one alone.
    pub fn changes_its_key(&self) {
        self.inner().signer = Signer::curve().named("2027");
    }

    /// How many sign-ins it has renewed.
    pub fn renewed(&self) -> u64 {
        self.inner().renewed
    }

    /// What can renew a sign-in right now.
    pub fn renewals(&self) -> Vec<String> {
        self.inner().renewals.clone()
    }

    /// What it was told to forget.
    pub fn forgotten(&self) -> Vec<String> {
        self.inner().forgotten.clone()
    }

    /// It answers no longer.
    pub fn stop(&self) {
        self.stopper.stop();
    }
}

/// The person the stand-in signs in unless it is told otherwise: an administrator.
pub fn administrator() -> Json {
    Json::object([
        ("sub", "7c1f".into()),
        ("name", "Alex Urban".into()),
        ("email", "alex.urban@example.com".into()),
        ("groups", Json::List(vec!["staff".into(), "away-admins".into()])),
    ])
}

/// A person who administers nothing.
pub fn person() -> Json {
    Json::object([
        ("sub", "9d2e".into()),
        ("name", "Robin Meyer".into()),
        ("email", "robin.meyer@example.com".into()),
        ("groups", Json::List(vec!["staff".into()])),
    ])
}

fn json(status: StatusCode, says: Json) -> Answer {
    answer(status, "application/json", says.write())
}

fn refusal(status: StatusCode, error: &str, more: &str) -> Answer {
    json(status, Json::object([("error", error.into()), ("error_description", more.into())]))
}

impl Inner {
    /// Behaves from now on as a test tells it to.
    fn told(&mut self, told: &[(String, String)]) {
        for (name, value) in told {
            match name.as_str() {
                "reset" => {
                    self.says = administrator();
                    self.good_for = 300;
                    self.sends_back_with = None;
                    self.refuses_renewals = false;
                }
                "refuses_renewals" => self.refuses_renewals = value == "true",
                "sends_back_with" => self.sends_back_with = Some(value.clone()).filter(|error| !error.is_empty()),
                "signs_for" => self.good_for = value.parse().unwrap_or(300),
                "person" => self.says = if value == "person" { person() } else { administrator() },
                _ => {}
            }
        }
    }

    fn says_of_itself(&self) -> Answer {
        let at = |path: &str| Json::Text(format!("{}{path}", self.at));
        json(
            StatusCode::OK,
            Json::object([
                ("issuer", self.names_itself.as_str().into()),
                ("authorization_endpoint", at("/authorize")),
                ("token_endpoint", at("/token")),
                ("jwks_uri", at("/keys")),
                ("revocation_endpoint", at("/revoke")),
            ]),
        )
    }

    fn sign_in_page(&mut self, asked: &[(String, String)]) -> Answer {
        let asked = |name: &str| asked.iter().find(|(its, _)| its == name).map(|(_, value)| value.as_str()).unwrap_or_default();
        let whole = asked("response_type") == "code"
            && asked("client_id") == self.client.0
            && asked("redirect_uri") == self.back
            && asked("scope").split(' ').any(|scope| scope == "openid")
            && asked("code_challenge_method") == "S256"
            && asked("code_challenge").len() == 43
            && !asked("state").is_empty()
            && !asked("nonce").is_empty();
        if !whole {
            return plain(StatusCode::BAD_REQUEST, "The sign-in page was asked for in a way it doesn't take.\n");
        }
        let came_with = match &self.sends_back_with {
            Some(error) => format!("error={error}&error_description=said+the+stand-in"),
            None => {
                let code = random_name();
                self.codes.insert(code.clone(), (asked("nonce").to_string(), asked("code_challenge").to_string()));
                format!("code={code}")
            }
        };
        let mut answer = plain(StatusCode::SEE_OTHER, "Back to Away.\n");
        let to = format!("{}?{came_with}&state={}", self.back, away_part::address::written_inside(asked("state")));
        answer.headers_mut().insert(LOCATION, HeaderValue::from_str(&to).expect("an address"));
        answer
    }

    fn tokens(&mut self, named: Option<&str>, sent: &[(String, String)]) -> Answer {
        let sent = |name: &str| sent.iter().find(|(its, _)| its == name).map(|(_, value)| value.as_str()).unwrap_or_default();
        // the name and the secret are written as a form writes them before they are joined (RFC 6749, 2.3.1)
        let written =
            format!("{}:{}", away_part::address::written_inside(&self.client.0), away_part::address::written_inside(&self.client.1));
        let expected = format!("Basic {}", base64::ordinary(written.as_bytes()));
        if named != Some(expected.as_str()) {
            return refusal(StatusCode::UNAUTHORIZED, "invalid_client", "The client's name or its secret is wrong.");
        }
        let now = time::now().as_secs();
        let number = match sent("grant_type") {
            "authorization_code" => {
                let Some((number, challenge)) = self.codes.remove(sent("code")) else {
                    return refusal(StatusCode::BAD_REQUEST, "invalid_grant", "Code not valid");
                };
                let proven = base64::for_addresses(&Sha256::digest(sent("code_verifier").as_bytes()));
                if proven != challenge || sent("redirect_uri") != self.back {
                    return refusal(StatusCode::BAD_REQUEST, "invalid_grant", "PKCE verification failed");
                }
                Some(number)
            }
            "refresh_token" => {
                let known = self.renewals.iter().position(|renewal| renewal == sent("refresh_token"));
                match (known, self.refuses_renewals) {
                    (Some(at), false) => {
                        // each renewal is good once
                        self.renewals.remove(at);
                        self.renewed += 1;
                        None
                    }
                    _ => return refusal(StatusCode::BAD_REQUEST, "invalid_grant", "Session not active"),
                }
            }
            _ => return refusal(StatusCode::BAD_REQUEST, "unsupported_grant_type", "The stand-in takes a code or a renewal."),
        };
        let Json::Object(mut says) = self.says.clone() else { unreachable!("the stand-in says an object") };
        says.extend([
            ("iss".to_string(), self.names_itself.as_str().into()),
            ("aud".to_string(), self.client.0.as_str().into()),
            ("iat".to_string(), now.into()),
            ("exp".to_string(), (now + self.good_for).into()),
        ]);
        if let Some(number) = number {
            says.push(("nonce".to_string(), number.into()));
        }
        let renewal = random_name();
        self.renewals.push(renewal.clone());
        json(
            StatusCode::OK,
            Json::object([
                ("token_type", "Bearer".into()),
                ("access_token", random_name().into()),
                ("expires_in", self.good_for.into()),
                ("id_token", self.signer.sign(&Json::Object(says)).into()),
                ("refresh_token", renewal.into()),
            ]),
        )
    }
}
