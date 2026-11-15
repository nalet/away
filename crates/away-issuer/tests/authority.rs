//! Asking an organisation's authority, against pages that stand in for one and a real
//! Kerberos server where `AWAY_TEST_KERBEROS` names one
//! (`tests/machines/kerberos/real.sh` starts it): the account signs in at the pages
//! with a ticket of that server, and the pages issue with an authority of the tests.

use std::time::{SystemTime, UNIX_EPOCH};

use away_issuer::authority::stand_in::{between_dashes, Issued, Pages, With};
use away_issuer::authority::{Authority, Why};
use away_part::client::Client;
use away_secret::Secret;
use away_winauth::certificate::{Certificate, FOR_ASKING_FOR_OTHERS, FOR_SIGNING_IN};
use away_winauth::kerberos::crypto::{Key, Kind};
use away_winauth::kerberos::pkinit::Held;
use away_winauth::request::{self, Key as NewKey};

const AUTHORITY: &[u8] = include_bytes!("../../away-winauth/tests/fixtures/authority.der");
const SHOWN: &[u8] = include_bytes!("../../away-winauth/tests/fixtures/controller.der");
const KEY_OF_IT: &[u8] = include_bytes!("../../away-winauth/tests/fixtures/controller.key.der");
const ISSUING: &[u8] = include_bytes!("../../away-winauth/tests/fixtures/issuing.der");
const KEY_OF_ISSUING: &[u8] = include_bytes!("../../away-winauth/tests/fixtures/issuing.key.der");
const PASSWORD: &str = "a password that exists for these tests alone";

fn server() -> Option<String> {
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

/// Pages as the tests' Kerberos server knows them: the service `HTTP/localhost`, whose
/// key is made from a password, and one account that may ask.
async fn pages() -> Pages {
    Pages::open(With {
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
        known_as: Vec::new(),
        good_for: 300,
    })
    .await
}

fn authority(pages: &Pages, server: &str, password: &str) -> Authority {
    let client = Client::new(Some(between_dashes("CERTIFICATE", AUTHORITY).as_bytes())).expect("a client that trusts the tests' authority");
    Authority::new(client, &pages.address, &[server.to_string()], "CORP.EXAMPLE", "pat.typed", Secret::new(password)).expect("an authority")
}

#[tokio::test]
async fn the_account_gets_an_agents_certificate_and_with_it_one_in_a_persons_name() {
    let Some(server) = server() else { return };
    let pages = pages().await;
    let authority = authority(&pages, &server, PASSWORD);
    let issuing = Certificate::read(ISSUING).unwrap();

    // first the account's own certificate, which allows asking for others
    let agents_key = NewKey::new().expect("a key");
    let asked = request::asked(&agents_key, "pat.typed").expect("a request");
    let agents = authority.ask(&asked, "CorpAgent").await.expect("an agent's certificate");
    let read = Certificate::read(&agents).expect("the agent's certificate, read");
    assert_eq!(read.account.as_deref(), Some("pat.typed@CORP.EXAMPLE"));
    assert!(read.is_for(FOR_ASKING_FOR_OTHERS) && !read.is_for(FOR_SIGNING_IN) && read.is_signed_by(&issuing));
    let agent = Held::new(&agents, &agents_key.written().unwrap()).expect("the agent, with its key");

    // then a certificate for a key of a person's, asked for in their name
    let key = NewKey::new().expect("a key");
    let asked = request::asked(&key, "alex.urban").expect("a request");
    let signed = request::in_the_name_of(&asked, "CORP\\alex.urban", &agent).expect("the request, signed by the agent");
    let issued = authority.ask(&signed, "CorpSignIn").await.expect("a certificate for signing in");
    let read = Certificate::read(&issued).expect("the certificate, read");
    assert_eq!(read.account.as_deref(), Some("alex.urban@CORP.EXAMPLE"));
    assert!(read.is_for(FOR_SIGNING_IN) && read.is_signed_by(&issuing) && read.is_good_at(now()));
    // it lasts minutes, and began before it was asked for, as a Windows authority's does
    assert!(read.until <= now() + 300 && read.until > now() + 240 && read.from < now() - 500, "{} to {}", read.from, read.until);
    // and it is for the key that was made here: a card takes the two together
    let pin = away_rdp::secret::Secret::new("1234");
    assert!(away_winauth::card::Card::new(&issued, &key.written().unwrap(), pin, "a key", [1; 16]).is_ok());

    // the pages were asked by the account alone, each time with a ticket bound to their connection
    let seen = pages.seen();
    assert_eq!((seen.signed_in.len(), seen.not_signed_in), (4, 0), "{seen:?}");
    assert!(seen.signed_in.iter().all(|account| account == "pat.typed"));
    assert_eq!(
        seen.issued,
        [
            Issued { number: 449, kind: "CorpAgent".to_string(), account: "pat.typed@CORP.EXAMPLE".to_string(), by_an_agent: false },
            Issued { number: 450, kind: "CorpSignIn".to_string(), account: "alex.urban@CORP.EXAMPLE".to_string(), by_an_agent: true },
        ]
    );
}

#[tokio::test]
async fn what_the_authority_refuses_is_said_in_its_own_words() {
    let Some(server) = server() else { return };
    let pages = pages().await;
    let authority = authority(&pages, &server, PASSWORD);
    let agents_key = NewKey::new().expect("a key");
    let agents = authority.ask(&request::asked(&agents_key, "pat.typed").unwrap(), "CorpAgent").await.expect("an agent's certificate");
    let agent = Held::new(&agents, &agents_key.written().unwrap()).expect("the agent, with its key");
    let key = NewKey::new().expect("a key");
    let asked = request::asked(&key, "somebody").expect("a request");

    // an account the authority issues nothing for
    let signed = request::in_the_name_of(&asked, "CORP\\sam.other", &agent).unwrap();
    assert_eq!(
        authority.ask(&signed, "CorpSignIn").await,
        Err(Why::Refused("Your Request Id is 450. The disposition message is \"Denied by Policy Module\".".to_string()))
    );
    // a kind the authority doesn't know
    let signed = request::in_the_name_of(&asked, "CORP\\alex.urban", &agent).unwrap();
    let Err(Why::Refused(said)) = authority.ask(&signed, "NoSuchKind").await else { panic!("a kind that isn't there is refused") };
    assert!(said.contains("0x80094800") && said.ends_with("NoSuchKind. \"."), "{said}");
    // a request for signing in that no agent signed, and one an agent signed that is the agent's own kind
    assert!(matches!(authority.ask(&asked, "CorpSignIn").await, Err(Why::Refused(_))));
    assert!(matches!(authority.ask(&signed, "CorpAgent").await, Err(Why::Refused(_))));
    // an agent the authority didn't issue: the tests' account certificate is of another authority
    let stranger = Held::new(
        include_bytes!("../../away-winauth/tests/fixtures/account.der"),
        include_bytes!("../../away-winauth/tests/fixtures/account.key.der"),
    )
    .unwrap();
    let signed = request::in_the_name_of(&asked, "CORP\\alex.urban", &stranger).unwrap();
    assert!(matches!(authority.ask(&signed, "CorpSignIn").await, Err(Why::Refused(_))));

    let seen = pages.seen();
    assert_eq!((seen.issued.len(), seen.refused.len()), (1, 5), "{seen:?}");
}

#[tokio::test]
async fn an_account_the_domain_controller_does_not_take_asks_nothing() {
    let Some(server) = server() else { return };
    let pages = pages().await;
    let authority = authority(&pages, &server, "another password");
    let key = NewKey::new().expect("a key");
    let Err(Why::NotAsked(why)) = authority.ask(&request::asked(&key, "pat.typed").unwrap(), "CorpAgent").await else {
        panic!("a wrong password gets no ticket")
    };
    assert!(why.starts_with("The domain controller, asked for a ticket for the account Away asks the authority as:"), "{why}");
    // nothing reached the pages
    assert_eq!(pages.seen(), Default::default());
}

#[tokio::test]
async fn pages_of_an_authority_that_is_not_trusted_are_not_asked() {
    let Some(server) = server() else { return };
    let pages = pages().await;
    let authority = Authority::new(
        Client::new(None).unwrap(),
        &pages.address,
        std::slice::from_ref(&server),
        "CORP.EXAMPLE",
        "pat.typed",
        Secret::new(PASSWORD),
    )
    .expect("an authority");
    let key = NewKey::new().expect("a key");
    let Err(Why::NotAsked(why)) = authority.ask(&request::asked(&key, "pat.typed").unwrap(), "CorpAgent").await else {
        panic!("pages whose certificate leads nowhere are not asked")
    };
    assert!(why.contains("couldn't be secured"), "{why}");
    assert_eq!(pages.seen(), Default::default());
}
