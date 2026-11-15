//! The sign-in service as Away sees it: where its pages and its keys are, the address a
//! person is sent to, the exchange of what they come back with for signed tokens, and
//! the renewal of a sign-in (OpenID Connect Discovery 1.0 and Core 3.1; RFC 7636 for the
//! proof that the one who began a sign-in ends it; RFC 6749 section 6 for renewals).
//!
//! Away is a client of the service with a secret of its own. Nothing here is kept: what
//! comes back is handed to whoever asked.

use std::sync::{Arc, Mutex};

use away_part::address::Address;
use away_part::client::{self, Client, Got};
use away_part::json::Json;
use away_part::token::{self, Expected, Keys};
use away_secret::Secret;

/// How often at most the keys a service publishes are read anew because a token is
/// signed with one that is not among them: once in this many seconds.
const KEYS_ANEW_AT_MOST_EVERY: u64 = 10;

/// What Away is told about the sign-in service.
#[derive(Clone, Debug)]
pub struct Settings {
    /// The service, as it names itself: `https://sign-in.example.com/realms/staff`.
    pub service: String,
    /// The name the service knows Away by.
    pub client: String,
    /// The secret Away proves itself to the service with.
    pub secret: Secret,
    /// Where the service sends a person back to: Away's own address, and `/sign-in/back`.
    pub back: String,
    /// What Away asks the service to say about a person: `openid profile email`.
    pub asks_for: String,
}

/// Why the sign-in service was of no help.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failed {
    /// It didn't answer.
    NoAnswer(String),
    /// It answered with something Away can't use: the setup is wrong, on one side or
    /// the other.
    Unusable(String),
    /// It refused: what Away handed it is no longer good.
    Refused(String),
}

impl Failed {
    /// The technical reason, for whoever is asked for help.
    pub fn details(&self) -> &str {
        match self {
            Failed::NoAnswer(why) | Failed::Unusable(why) | Failed::Refused(why) => why,
        }
    }
}

impl From<client::Failed> for Failed {
    fn from(failed: client::Failed) -> Failed {
        match failed {
            client::Failed::NotReached(_) | client::Failed::NoAnswerInTime => Failed::NoAnswer(format!("The sign-in service: {failed}.")),
            // a certificate that isn't trusted, or an answer that is none, is a matter of the setup
            other => Failed::Unusable(format!("The sign-in service: {other}.")),
        }
    }
}

/// What the service signed for a person.
#[derive(Clone, Debug)]
pub struct Signed {
    /// What it says of the person, where it said anything: a renewal may come without.
    pub says: Option<Json>,
    /// What the sign-in is renewed with, where the service gave something for that.
    pub renewal: Option<Secret>,
    /// How long what was signed is good for, in seconds from now.
    pub good_for: u64,
}

/// Where the service's parts are, as it says itself.
#[derive(Clone, Debug)]
struct Found {
    sign_in_page: Address,
    tokens: Address,
    keys: Address,
    forgets: Option<Address>,
}

struct Known {
    found: Option<Arc<Found>>,
    keys: Keys,
    keys_read_anew_at: Option<u64>,
}

/// The sign-in service.
pub struct Service {
    settings: Settings,
    client: Client,
    known: Mutex<Known>,
}

impl Service {
    /// The service of these settings. Nothing is asked of it yet.
    pub fn new(settings: Settings, client: Client) -> Service {
        Service { settings, client, known: Mutex::new(Known { found: None, keys: Keys::default(), keys_read_anew_at: None }) }
    }

    /// The settings it was made with.
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Asks the service where its parts are, unless it has said so already.
    async fn found(&self) -> Result<Arc<Found>, Failed> {
        if let Some(found) = self.known.lock().ok().and_then(|known| known.found.clone()) {
            return Ok(found);
        }
        let unusable = |why: String| Failed::Unusable(format!("What the sign-in service says of itself can't be used: {why}."));
        let named = self.settings.service.trim_end_matches('/');
        let asked = Address::read(&format!("{named}/.well-known/openid-configuration"))
            .map_err(|why| Failed::Unusable(format!("The sign-in service's address can't be used: {why}.")))?;
        let says = read(self.client.get(&asked).await?).map_err(unusable)?;
        if says.text_of("issuer").map(|issuer| issuer.trim_end_matches('/')) != Some(named) {
            return Err(unusable(format!("it names itself {}, and Away is told {named}", says.text_of("issuer").unwrap_or("nothing"))));
        }
        let part = |name: &str| -> Result<Address, Failed> {
            let written = says.text_of(name).ok_or_else(|| unusable(format!("it has no {name}")))?;
            let address = Address::read(written).map_err(|why| unusable(format!("its {name} is no address Away can use: {why}")))?;
            // a service that is reached over a secured connection sends Away nowhere that is not
            match asked.secured && !address.secured {
                true => Err(unusable(format!("its {name} is not secured"))),
                false => Ok(address),
            }
        };
        let found = Arc::new(Found {
            sign_in_page: part("authorization_endpoint")?,
            tokens: part("token_endpoint")?,
            keys: part("jwks_uri")?,
            forgets: says.get("revocation_endpoint").map(|_| part("revocation_endpoint")).transpose()?,
        });
        if let Ok(mut known) = self.known.lock() {
            known.found = Some(found.clone());
        }
        Ok(found)
    }

    /// The keys the service publishes: as they were read, or read anew where a token
    /// was signed with one that is not among them.
    async fn keys(&self, anew: bool, now: u64) -> Result<Keys, Failed> {
        if let Ok(known) = self.known.lock() {
            let recently = known.keys_read_anew_at.is_some_and(|at| now.saturating_sub(at) < KEYS_ANEW_AT_MOST_EVERY);
            if !known.keys.is_empty() && (!anew || recently) {
                return Ok(known.keys.clone());
            }
        }
        let found = self.found().await?;
        let keys = Keys::read(
            &read(self.client.get(&found.keys).await?)
                .map_err(|why| Failed::Unusable(format!("The sign-in service's keys can't be read: {why}.")))?,
        );
        if keys.is_empty() {
            return Err(Failed::Unusable("The sign-in service publishes no key that Away can check a signature with.".to_string()));
        }
        if let Ok(mut known) = self.known.lock() {
            known.keys = keys.clone();
            if anew {
                known.keys_read_anew_at = Some(now);
            }
        }
        Ok(keys)
    }

    /// The address a person is sent to, to sign in. The state comes back with them, the
    /// number comes back signed, and the challenge is what the proof at the exchange is
    /// checked against.
    pub async fn sign_in_page(&self, state: &str, number: &str, challenge: &str) -> Result<String, Failed> {
        let found = self.found().await?;
        Ok(found
            .sign_in_page
            .asking(&[
                ("response_type", "code"),
                ("client_id", &self.settings.client),
                ("redirect_uri", &self.settings.back),
                ("scope", &self.settings.asks_for),
                ("state", state),
                ("nonce", number),
                ("code_challenge", challenge),
                ("code_challenge_method", "S256"),
            ])
            .written())
    }

    /// Exchanges what a person came back with for what the service signed about them.
    pub async fn exchange(&self, code: &str, proof: &Secret, number: &str, now: u64) -> Result<Signed, Failed> {
        let form = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", self.settings.back.as_str()),
            ("code_verifier", proof.expose()),
        ];
        let signed = self.tokens(&form, Some(number), now).await?;
        match signed.says {
            Some(_) => Ok(signed),
            None => Err(Failed::Unusable("The sign-in service's answer says nothing about who signed in.".to_string())),
        }
    }

    /// Renews a sign-in.
    pub async fn renew(&self, renewal: &Secret, now: u64) -> Result<Signed, Failed> {
        self.tokens(&[("grant_type", "refresh_token"), ("refresh_token", renewal.expose())], None, now).await
    }

    /// Tells the service that a sign-in is over, where it takes such word. Nothing
    /// depends on its answer.
    pub async fn forget(&self, renewal: &Secret) {
        if let Ok(Found { forgets: Some(forgets), .. }) = self.found().await.as_deref() {
            let form = [("token", renewal.expose()), ("token_type_hint", "refresh_token")];
            let _ = self.client.send_form(forgets, &form, Some((&self.settings.client, &self.settings.secret))).await;
        }
    }

    async fn tokens(&self, form: &[(&str, &str)], number: Option<&str>, now: u64) -> Result<Signed, Failed> {
        let found = self.found().await?;
        let got = self.client.send_form(&found.tokens, form, Some((&self.settings.client, &self.settings.secret))).await?;
        let status = got.status;
        let answer = read(got).map_err(|why| Failed::Unusable(format!("The sign-in service's answer can't be read: {why}.")));
        if status != 200 {
            // a refusal says why in words of the service's own
            let why = answer.ok().and_then(|answer| {
                let error = answer.text_of("error")?.to_string();
                Some(match answer.text_of("error_description") {
                    Some(more) => format!("{error}: {more}"),
                    None => error,
                })
            });
            return Err(match (status, why) {
                (400 | 401, Some(why)) if why.starts_with("invalid_grant") => {
                    Failed::Refused(format!("The sign-in service refused ({why})."))
                }
                (_, Some(why)) => Failed::Unusable(format!("The sign-in service refused Away itself ({why}).")),
                (status, None) => Failed::Unusable(format!("The sign-in service answered with the status {status}.")),
            });
        }
        let answer = answer?;
        let says = match answer.text_of("id_token") {
            Some(token) => Some(self.check(token, number, now).await?),
            None => None,
        };
        let good_for = match (answer.get("expires_in").and_then(Json::count), &says) {
            (Some(seconds), _) => seconds,
            (None, Some(says)) => says.get("exp").and_then(Json::count).unwrap_or(now).saturating_sub(now),
            (None, None) => return Err(Failed::Unusable("The sign-in service's answer doesn't say how long it is good for.".to_string())),
        };
        Ok(Signed { says, renewal: answer.text_of("refresh_token").map(Secret::new), good_for })
    }

    async fn check(&self, token: &str, number: Option<&str>, now: u64) -> Result<Json, Failed> {
        let expected = Expected { service: &self.settings.service, client: &self.settings.client, number, now };
        let unusable = |wrong: token::Wrong| Failed::Unusable(format!("What the sign-in service signed can't be taken: {wrong}."));
        match token::check(token, &self.keys(false, now).await?, &expected) {
            // the service may have changed its keys: read them once more
            Err(token::Wrong::UnknownKey) => token::check(token, &self.keys(true, now).await?, &expected).map_err(unusable),
            other => other.map_err(unusable),
        }
    }
}

/// Reads an answer that should be JSON.
fn read(got: Got) -> Result<Json, String> {
    let text = std::str::from_utf8(&got.body).map_err(|_| "it is no text".to_string())?;
    Json::read(text).map_err(|wrong| format!("it is no JSON ({wrong})"))
}
