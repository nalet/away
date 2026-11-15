//! The issuer as a whole: asked as the gateway asks it, with stand-ins for what it rests
//! on. A server answers for sessions, a sign-in service publishes its keys, an
//! organisation's authority issues, and a real Kerberos server gives the tickets where
//! `AWAY_TEST_KERBEROS` names one (`tests/machines/kerberos/real.sh` starts it).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use away_issuer::authority::stand_in::{between_dashes, Pages, With};
use away_issuer::told::{self, Internal, SignInService, Told, Windows};
use away_issuer::Issuer;
use away_part::address::{read_form, Address};
use away_part::client::Client;
use away_part::http::{answer, carried, plain, Answers, Listener};
use away_part::json::Json;
use away_part::stand_in::Signer;
use away_part::{stop, Lines, Measures, Part, Settings, Stopper};
use away_secret::Secret;
use away_winauth::certificate::{Certificate, FOR_SIGNING_IN};
use away_winauth::kerberos::crypto::{Key, Kind};
use away_winauth::request::{self, Key as NewKey};
use hyper::StatusCode;

const AUTHORITY: &[u8] = include_bytes!("../../away-winauth/tests/fixtures/authority.der");
const SHOWN: &[u8] = include_bytes!("../../away-winauth/tests/fixtures/controller.der");
const KEY_OF_IT: &[u8] = include_bytes!("../../away-winauth/tests/fixtures/controller.key.der");
const ISSUING: &[u8] = include_bytes!("../../away-winauth/tests/fixtures/issuing.der");
const KEY_OF_ISSUING: &[u8] = include_bytes!("../../away-winauth/tests/fixtures/issuing.key.der");
const PASSWORD: &str = "a password that exists for these tests alone";

fn kerberos() -> Option<String> {
    let at = std::env::var("AWAY_TEST_KERBEROS").ok().filter(|at| !at.is_empty());
    if at.is_none() {
        assert!(std::env::var_os("CI").is_none(), "the pipeline's tests need a Kerberos server: AWAY_TEST_KERBEROS is not set");
        eprintln!("no Kerberos server to test against: AWAY_TEST_KERBEROS is not set");
    }
    at
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

/// What the server was told at once: names and their values.
type Said = Vec<(String, String)>;

/// What a session's stand-in server answers when it is asked to answer for it.
#[derive(Clone)]
enum Vouches {
    /// The session runs: whose it is, on which machine, and what the service signed.
    Runs(String, String, String),
    /// The person's sign-in has run out.
    SignedOut,
}

/// A server that stands in: it answers for the sessions it is given, and keeps what it
/// is told of sign-ins that were made.
struct Server {
    at: String,
    sessions: Arc<Mutex<HashMap<String, Vouches>>>,
    told: Arc<Mutex<Vec<Said>>>,
    keeps: Arc<AtomicBool>,
    _stopper: Stopper,
}

impl Server {
    async fn start() -> Server {
        let listener = Listener::bind("a server", "127.0.0.1:0").await.expect("an address");
        let at = format!("http://127.0.0.1:{}", listener.address().port());
        let sessions: Arc<Mutex<HashMap<String, Vouches>>> = Arc::default();
        let told: Arc<Mutex<Vec<Said>>> = Arc::default();
        let keeps = Arc::new(AtomicBool::new(true));
        let (its_sessions, its_told, it_keeps) = (sessions.clone(), told.clone(), keeps.clone());
        let answers: Answers = Arc::new(move |asked| {
            let (sessions, told, keeps) = (its_sessions.clone(), its_told.clone(), it_keeps.clone());
            Box::pin(async move {
                let path = asked.uri().path().to_string();
                let said = carried(asked, 16 * 1024).await.and_then(|sent| read_form(std::str::from_utf8(&sent).ok()?)).unwrap_or_default();
                let named = said.iter().find(|(name, _)| name == "session").map(|(_, session)| session.clone()).unwrap_or_default();
                let known = sessions.lock().unwrap().get(&named).cloned();
                match (path.as_str(), known) {
                    (_, None) => plain(StatusCode::NOT_FOUND, "No session of that name runs.\n"),
                    ("/internal/vouch", Some(Vouches::SignedOut)) => plain(StatusCode::FORBIDDEN, "The person's sign-in has run out.\n"),
                    ("/internal/vouch", Some(Vouches::Runs(subject, machine, signed))) => {
                        let says = Json::object([
                            ("session", named.as_str().into()),
                            ("subject", subject.as_str().into()),
                            ("machine", machine.as_str().into()),
                            ("token", signed.as_str().into()),
                        ]);
                        answer(StatusCode::OK, "application/json", says.write())
                    }
                    ("/internal/told", Some(_)) if keeps.load(Ordering::SeqCst) => {
                        told.lock().unwrap().push(said);
                        answer(StatusCode::OK, "application/json", "{}")
                    }
                    ("/internal/told", Some(_)) => {
                        plain(StatusCode::SERVICE_UNAVAILABLE, "Away keeps nothing here: it is given no database.\n")
                    }
                    _ => plain(StatusCode::NOT_FOUND, "Nothing is at this address.\n"),
                }
            })
        });
        let (stopper, stop) = stop::pair();
        tokio::spawn(listener.answer_with(answers, stop, Lines::kept_for("a server"), Measures::of("a server")));
        Server { at, sessions, told, keeps, _stopper: stopper }
    }

    fn runs(&self, session: &str, answers: Vouches) {
        self.sessions.lock().unwrap().insert(session.to_string(), answers);
    }
}

/// A sign-in service that stands in: it says where its keys are, and publishes them.
struct Service {
    at: String,
    signer: Signer,
    _stopper: Stopper,
}

impl Service {
    async fn start() -> Service {
        let listener = Listener::bind("a sign-in service", "127.0.0.1:0").await.expect("an address");
        let at = format!("http://127.0.0.1:{}/realms/staff", listener.address().port());
        let signer = Signer::curve();
        let (named, published) = (at.clone(), signer.published().write());
        let answers: Answers = Arc::new(move |asked| {
            let (named, published) = (named.clone(), published.clone());
            Box::pin(async move {
                match asked.uri().path() {
                    "/realms/staff/.well-known/openid-configuration" => {
                        let says = Json::object([("issuer", named.as_str().into()), ("jwks_uri", format!("{named}/keys").into())]);
                        answer(StatusCode::OK, "application/json", says.write())
                    }
                    "/realms/staff/keys" => answer(StatusCode::OK, "application/json", published),
                    _ => plain(StatusCode::NOT_FOUND, "Nothing is at this address.\n"),
                }
            })
        });
        let (stopper, stop) = stop::pair();
        tokio::spawn(listener.answer_with(answers, stop, Lines::kept_for("a sign-in service"), Measures::of("a sign-in service")));
        Service { at, signer, _stopper: stopper }
    }

    /// What the service signs about a person now: their account in the domain, and
    /// their groups.
    fn says(&self, subject: &str, account: &str, groups: &[&str]) -> Json {
        Json::object([
            ("iss", self.at.as_str().into()),
            ("aud", "away".into()),
            ("sub", subject.into()),
            ("exp", (now() + 300).into()),
            ("iat", now().into()),
            ("preferred_username", account.into()),
            ("groups", Json::List(groups.iter().map(|group| (*group).into()).collect())),
        ])
    }

    fn signs(&self, subject: &str, account: &str, groups: &[&str]) -> String {
        self.signer.sign(&self.says(subject, account, groups))
    }
}

/// Everything an issuer rests on, and the issuer.
struct World {
    server: Server,
    service: Service,
    pages: Pages,
    issuer: String,
    lines: Lines,
    _stopper: Stopper,
}

async fn world_with(kerberos: &str, good_for: u64) -> World {
    let server = Server::start().await;
    let service = Service::start().await;
    let pages = Pages::open(With {
        shows: (SHOWN.to_vec(), KEY_OF_IT.to_vec()),
        of_the_service: Key::from_password(
            Kind::Aes256,
            "the password of pages that exist for these tests alone",
            b"CORP.EXAMPLEHTTPlocalhost",
            4096,
        ),
        issues_with: (ISSUING.to_vec(), KEY_OF_ISSUING.to_vec()),
        asked_as: "pat.typed".to_string(),
        domain: "CORP".to_string(),
        realm: "CORP.EXAMPLE".to_string(),
        for_signing_in: "CorpSignIn".to_string(),
        for_an_agent: "CorpAgent".to_string(),
        never_for: vec!["sam.other".to_string()],
        good_for,
    })
    .await;
    let told = Told {
        internal: Some(Internal { address: "127.0.0.1:0".to_string(), certificate: None }),
        windows: Some(Windows {
            domain: "corp.example".to_string(),
            short_name: "CORP".to_string(),
            controllers: vec![kerberos.to_string()],
            pages: pages.address.clone(),
            authorities: Some(between_dashes("CERTIFICATE", AUTHORITY).into_bytes()),
            account: "pat.typed".to_string(),
            password: Secret::new(PASSWORD),
            agent_kind: "CorpAgent".to_string(),
            sign_in_kind: "CorpSignIn".to_string(),
            account_entry: told::ACCOUNT_ENTRY.to_string(),
            groups_entry: told::GROUPS_ENTRY.to_string(),
            never_as: told::NEVER_AS.iter().map(|never| never.to_string()).collect(),
            never_of: told::NEVER_OF.iter().map(|never| never.to_string()).collect(),
        }),
        sign_in: Some(SignInService { service: service.at.clone(), client: "away".to_string(), authorities: None }),
        server: Some(told::Server { at: Address::read(&server.at).expect("the server's address"), authorities: None }),
    };
    let (stopper, stop) = stop::pair();
    let lines = Lines::kept_for("issuer");
    let settings = Settings { address: String::new(), health_address: "127.0.0.1:0".to_string() };
    let issuer = Issuer::bind(&settings, told, Part::new("issuer", lines.clone(), stop)).await.expect("the issuer binds");
    let at = format!("http://127.0.0.1:{}/internal/certificate", issuer.bound().address.expect("where the gateway asks").port());
    tokio::spawn(issuer.run());
    World { server, service, pages, issuer: at, lines, _stopper: stopper }
}

async fn world(kerberos: &str) -> World {
    world_with(kerberos, 300).await
}

/// Asks the issuer as the gateway does: for a session, with a request for a certificate.
async fn ask(world: &World, session: &str, asked: &[u8]) -> (u16, Json) {
    let at = Address::read(&world.issuer).expect("the issuer's address");
    let request = away_part::base64::ordinary(asked);
    let got = Client::new(None).unwrap().send_form(&at, &[("session", session), ("request", &request)], None).await.expect("an answer");
    (got.status, Json::read(&String::from_utf8_lossy(&got.body)).unwrap_or(Json::Null))
}

/// What the issuer refuses with: the status, the reason as one word, and whether its
/// words begin as they should.
async fn refused(world: &World, session: &str, asked: &[u8]) -> (u16, String, String) {
    let (status, says) = ask(world, session, asked).await;
    (status, says.text_of("why").unwrap_or_default().to_string(), says.text_of("says").unwrap_or_default().to_string())
}

fn a_request() -> (NewKey, Vec<u8>) {
    let key = NewKey::new().expect("a key");
    let asked = request::asked(&key, "a session's key").expect("a request");
    (key, asked)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_person_of_a_session_that_runs_gets_what_signs_them_in_and_the_record_comes_first() {
    let Some(kerberos) = kerberos() else { return };
    let world = world(&kerberos).await;
    world
        .server
        .runs("5e55", Vouches::Runs("7c1f".to_string(), "desk-01".to_string(), world.service.signs("7c1f", "alex.urban", &["staff"])));
    let (key, asked) = a_request();

    let (status, says) = ask(&world, "5e55", &asked).await;
    assert_eq!(status, 200, "{}", says.write());
    let certificate = says
        .text_of("certificate")
        .and_then(|letters| away_part::base64::read_for_addresses(&letters.replace('+', "-").replace('/', "_").replace('=', "")));
    let certificate = certificate.expect("a certificate");
    let read = Certificate::read(&certificate).expect("the certificate, read");
    assert_eq!((says.text_of("account"), read.account.as_deref()), (Some("alex.urban@CORP.EXAMPLE"), Some("alex.urban@CORP.EXAMPLE")));
    assert!(read.is_for(FOR_SIGNING_IN) && read.is_good_at(now()) && read.until <= now() + 300);
    assert_eq!(says.get("until").and_then(Json::count), Some(read.until));
    // it is for the key that was made by whoever asked: a card takes the two together
    let pin = away_rdp::secret::Secret::new("1234");
    assert!(away_winauth::card::Card::new(&certificate, &key.written().unwrap(), pin, "a key", [1; 16]).is_ok());

    // the server was told of the sign-in, with the account and the certificate's number
    let told = world.server.told.lock().unwrap().clone();
    assert_eq!(told.len(), 1);
    let said = |name: &str| told[0].iter().find(|(its, _)| its == name).map(|(_, value)| value.as_str());
    assert_eq!((said("session"), said("what"), said("as")), (Some("5e55"), Some("signed-in"), Some("alex.urban@CORP.EXAMPLE")));
    assert_eq!((said("number"), said("until")), (Some("01c2"), Some(read.until.to_string().as_str())));
    // and it is a line of the issuer's own: who, as whom, on which machine, the number, until when
    let line = world.lines.kept().into_iter().find(|line| line.contains("signed a person in")).expect("a line");
    for fact in ["\"who\":\"7c1f\"", "\"as\":\"alex.urban@CORP.EXAMPLE\"", "\"machine\":\"desk-01\"", "\"number\":\"01c2\""] {
        assert!(line.contains(fact), "{fact} in {line}");
    }

    // asked again for the same session, as when a desktop locked: another certificate,
    // with the certificate the issuer asks for others with that it has already
    let (again, asked_again) = a_request();
    let (status, says) = ask(&world, "5e55", &asked_again).await;
    assert_eq!(status, 200, "{}", says.write());
    drop(again);
    let kinds: Vec<String> = world.pages.seen().issued.iter().map(|issued| issued.kind.clone()).collect();
    assert_eq!(kinds, ["CorpAgent", "CorpSignIn", "CorpSignIn"]);
    assert_eq!(world.server.told.lock().unwrap().len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nobody_is_signed_in_without_a_session_a_sign_in_of_their_own_and_an_account_away_signs_in_as() {
    let Some(kerberos) = kerberos() else { return };
    let world = world(&kerberos).await;
    let (_key, asked) = a_request();
    let runs = |signed: String| Vouches::Runs("7c1f".to_string(), "desk-01".to_string(), signed);

    // no session of that name, and a person whose sign-in has run out
    assert_eq!(refused(&world, "none", &asked).await, (404, "no-session".to_string(), "No session of that name runs.".to_string()));
    world.server.runs("out", Vouches::SignedOut);
    assert_eq!(refused(&world, "out", &asked).await, (403, "signed-out".to_string(), "The person's sign-in has run out.".to_string()));

    // what the sign-in service is said to have signed: by somebody else, about somebody
    // else, or too long ago
    let other = Signer::curve();
    world.server.runs("forged", runs(other.sign(&world.service.says("7c1f", "alex.urban", &[]))));
    let (status, why, says) = refused(&world, "forged", &asked).await;
    assert_eq!((status, why.as_str()), (403, "sign-in"), "{says}");
    assert!(says.starts_with("What the sign-in service is said to have signed can't be taken: "), "{says}");
    world.server.runs("another", runs(world.service.signs("9d2e", "alex.urban", &[])));
    assert_eq!(
        refused(&world, "another", &asked).await,
        (403, "sign-in".to_string(), "What the sign-in service signed is about another person than the one the session is of.".to_string())
    );
    let Json::Object(mut old) = world.service.says("7c1f", "alex.urban", &[]) else { unreachable!() };
    old.retain(|(name, _)| name != "iat");
    old.push(("iat".to_string(), (now() - 200).into()));
    world.server.runs("old", runs(world.service.signer.sign(&Json::Object(old))));
    assert_eq!(
        refused(&world, "old", &asked).await,
        (403, "sign-in".to_string(), "The sign-in service signed who this person is too long ago, or doesn't say when.".to_string())
    );

    // an account nobody is signed in as, a group whose people are not, and no account at all
    world.server.runs("never", runs(world.service.signs("7c1f", "Administrator", &["staff"])));
    assert_eq!(refused(&world, "never", &asked).await, (403, "never".to_string(), "Away signs nobody in as Administrator.".to_string()));
    world.server.runs("of-them", runs(world.service.signs("7c1f", "alex.urban", &["staff", "/Domain Admins"])));
    let (status, why, says) = refused(&world, "of-them", &asked).await;
    assert_eq!((status, why.as_str()), (403, "never"), "{says}");
    world.server.runs("nameless", runs(world.service.signs("7c1f", "alex.urban@corp.example", &[])));
    assert_eq!(refused(&world, "nameless", &asked).await.1, "no-account");

    // up to here the authority was asked nothing: not even for the issuer's own certificate
    assert_eq!(world.pages.seen(), Default::default());
    assert!(world.server.told.lock().unwrap().is_empty());

    // what is no request for a certificate, and a request whose key is not held
    world.server.runs("5e55", runs(world.service.signs("7c1f", "alex.urban", &["staff"])));
    assert_eq!(refused(&world, "5e55", b"no request").await.0, 400);
    let mut changed = asked.clone();
    let at = changed.len() - 1;
    changed[at] ^= 1;
    assert_eq!(refused(&world, "5e55", &changed).await.1, "request");
    assert_eq!(world.pages.seen(), Default::default());

    // an account the organisation's authority issues nothing for: its refusal, in its own words
    world.server.runs("refused", runs(world.service.signs("7c1f", "sam.other", &["staff"])));
    let (status, why, says) = refused(&world, "refused", &asked).await;
    assert_eq!((status, why.as_str()), (403, "authority-refused"), "{says}");
    assert_eq!(
        says,
        "The organisation's authority refused to sign CORP\\sam.other in. It says: Your Request Id is 450. The disposition message is \"Denied by Policy Module\"."
    );
    assert!(world.server.told.lock().unwrap().is_empty());

    // where the record can't be kept, what signs the person in is not handed out
    world.server.keeps.store(false, Ordering::SeqCst);
    let (status, why, says) = refused(&world, "5e55", &asked).await;
    assert_eq!((status, why.as_str()), (503, "not-recorded"), "{says}");
    assert!(says.starts_with("The server kept no record of the sign-in: it answered with the status 503"), "{says}");
    // each of them is a line of the issuer's
    let lines = world.lines.kept();
    assert_eq!(lines.iter().filter(|line| line.contains("signed nobody in")).count(), 12, "{lines:#?}");
    assert!(!lines.iter().any(|line| line.contains("signed a person in")));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn what_an_authority_issues_for_longer_than_five_minutes_signs_nobody_in() {
    let Some(kerberos) = kerberos() else { return };
    let world = world_with(&kerberos, 3600).await;
    world.server.runs("5e55", Vouches::Runs("7c1f".to_string(), "desk-01".to_string(), world.service.signs("7c1f", "alex.urban", &[])));
    let (_key, asked) = a_request();
    assert_eq!(
        refused(&world, "5e55", &asked).await,
        (
            403,
            "lasts-too-long".to_string(),
            "The organisation's authority issues CorpSignIn for 60 minutes from now, and what signs a person in lasts five at most."
                .to_string()
        )
    );
    assert!(world.server.told.lock().unwrap().is_empty());
}

#[tokio::test]
async fn an_issuer_of_an_installation_without_a_domain_signs_nobody_in() {
    let told = Told { internal: Some(Internal { address: "127.0.0.1:0".to_string(), certificate: None }), ..Told::default() };
    let (_stopper, stop) = stop::pair();
    let lines = Lines::kept_for("issuer");
    let settings = Settings { address: String::new(), health_address: "127.0.0.1:0".to_string() };
    let issuer = Issuer::bind(&settings, told, Part::new("issuer", lines.clone(), stop)).await.expect("the issuer binds");
    let at = Address::read(&format!("http://127.0.0.1:{}/internal/certificate", issuer.bound().address.unwrap().port())).unwrap();
    tokio::spawn(issuer.run());
    let client = Client::new(None).unwrap();
    let got = client.send_form(&at, &[("session", "5e55"), ("request", "AAAA")], None).await.expect("an answer");
    assert_eq!(got.status, 404);
    assert_eq!(Json::read(&String::from_utf8_lossy(&got.body)).unwrap().text_of("why"), Some("no-domain"));
    // what is asked without a session or a request, and what is asked elsewhere
    assert_eq!(client.send_form(&at, &[("session", "5e55")], None).await.unwrap().status, 400);
    let elsewhere = Address { path: "/internal/other".to_string(), ..at.clone() };
    assert_eq!(client.send_form(&elsewhere, &[("session", "5e55")], None).await.unwrap().status, 404);
    assert_eq!(client.get(&at).await.unwrap().status, 405);
    assert!(lines.kept().iter().any(|line| line.contains("no domain is named, so nobody is signed in on a machine as themselves")));
}
