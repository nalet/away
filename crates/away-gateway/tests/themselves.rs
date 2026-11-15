//! Signing a person in on a machine as themselves, as the gateway does it: against an
//! issuer that stands in, a stand-in for a machine of a domain, and a real Kerberos
//! server where `AWAY_TEST_KERBEROS` names one (`tests/machines/kerberos/real.sh`
//! starts it).

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use away_gateway::machine::{reach, Desktop, Opened, Screen, Step, Who, Why};
use away_gateway::stand_in::{self, Machine, TO_SIGN};
use away_gateway::themselves::{Domain, Issuer};
use away_part::address::{read_form, Address};
use away_part::base64;
use away_part::client::Client;
use away_part::http::{answer, carried, plain, Answers, Listener};
use away_part::json::Json;
use away_part::{stop, Lines, Measures, Stopper};
use away_winauth::kerberos::crypto::{Key, Kind};
use away_winauth::kerberos::pkinit::Held;
use away_winauth::request::stand_in::{issue, For};
use hyper::StatusCode;

const AUTHORITY: &[u8] = include_bytes!("../../away-winauth/tests/fixtures/authority.der");
const ISSUING: &[u8] = include_bytes!("../../away-winauth/tests/fixtures/issuing.der");
const KEY_OF_ISSUING: &[u8] = include_bytes!("../../away-winauth/tests/fixtures/issuing.key.der");

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

/// The key of the service the tests' Kerberos server knows the stand-in machine by,
/// made from the service's password as that server makes it.
fn of_the_service() -> Key {
    Key::from_password(Kind::Aes256, "the password of a machine that exists for these tests alone", b"CORP.EXAMPLETERMSRVlocalhost", 4096)
}

/// An issuer that stands in: it issues what signs `alex.urban` in for every session
/// but the one named `refused`, with the authority of the tests, and keeps what it issued.
struct StandsIn {
    at: Address,
    issued: Arc<Mutex<Vec<Vec<u8>>>>,
    _stopper: Stopper,
}

async fn issuer() -> StandsIn {
    let listener = Listener::bind("an issuer", "127.0.0.1:0").await.expect("an address");
    let at = Address::read(&format!("http://127.0.0.1:{}", listener.address().port())).expect("its address");
    let issued: Arc<Mutex<Vec<Vec<u8>>>> = Arc::default();
    let keeps = issued.clone();
    let answers: Answers = Arc::new(move |asked| {
        let keeps = keeps.clone();
        Box::pin(async move {
            if asked.uri().path() != "/internal/certificate" {
                return plain(StatusCode::NOT_FOUND, "Nothing is at this address.\n");
            }
            let said = carried(asked, 16 * 1024).await.and_then(|sent| read_form(std::str::from_utf8(&sent).ok()?)).unwrap_or_default();
            let said = |name: &str| said.iter().find(|(its, _)| its == name).map(|(_, value)| value.clone()).unwrap_or_default();
            if said("session") == "refused" || said("session") == "never" {
                let says = match said("session").as_str() {
                    "never" => Json::object([("why", "never".into()), ("says", "Away signs nobody in as alex.urban.".into())]),
                    _ => {
                        let says = "The organisation's authority refused to sign CORP\\alex.urban in. It says: Denied by Policy Module";
                        Json::object([("why", "authority-refused".into()), ("says", says.into())])
                    }
                };
                return answer(StatusCode::FORBIDDEN, "application/json", says.write());
            }
            let request =
                base64::read_for_addresses(&said("request").replace('+', "-").replace('/', "_").replace('=', "")).unwrap_or_default();
            let issuing = Held::new(ISSUING, KEY_OF_ISSUING).expect("the authority of the tests");
            let number = keeps.lock().unwrap().len() as u64 + 1;
            let Ok(certificate) = issue(&request, "alex.urban@CORP.EXAMPLE", For::SigningIn, &issuing, number, (now() - 600, now() + 300))
            else {
                return plain(StatusCode::BAD_REQUEST, "What was asked with is no request.\n");
            };
            keeps.lock().unwrap().push(certificate.clone());
            let says = Json::object([
                ("certificate", base64::ordinary(&certificate).into()),
                ("account", "alex.urban@CORP.EXAMPLE".into()),
                ("until", (now() + 300).into()),
            ]);
            answer(StatusCode::OK, "application/json", says.write())
        })
    });
    let (stopper, stop) = stop::pair();
    tokio::spawn(listener.answer_with(answers, stop, Lines::kept_for("an issuer"), Measures::of("an issuer")));
    StandsIn { at, issued, _stopper: stopper }
}

fn domain(kerberos: &str, issuer: &StandsIn) -> Domain {
    let lines: Vec<String> =
        base64::ordinary(AUTHORITY).as_bytes().chunks(64).map(|line| String::from_utf8_lossy(line).to_string()).collect();
    let authority = format!("-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n", lines.join("\n"));
    let asks = Issuer::new(Client::new(None).expect("a client"), issuer.at.clone());
    Domain::new("corp.example", &[kerberos.to_string()], authority.as_bytes(), asks).expect("a domain")
}

/// Takes what the machine sends until something is the case, for a few seconds at
/// most, and tells the connection how time passes meanwhile.
async fn until(desktop: &mut Desktop, what: &str, mut is: impl FnMut(&Desktop) -> bool) {
    desktop.take().await.expect("what the machine sent is read");
    let taking = async {
        while !is(desktop) {
            if let Ok(arrived) = tokio::time::timeout(Duration::from_millis(100), desktop.arrives()).await {
                arrived.expect("the machine stays");
            }
            desktop.tick().await.expect("the machine stays");
            desktop.take().await.expect("what the machine sends is read");
        }
    };
    tokio::time::timeout(Duration::from_secs(10), taking).await.unwrap_or_else(|_| panic!("it never happened: {what}"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_person_is_signed_in_as_themselves_with_a_ticket_and_a_card_and_nothing_typed() {
    let Some(kerberos) = kerberos() else { return };
    let issuer = issuer().await;
    let domain = domain(&kerberos, &issuer);
    let machine = Machine::start_in_a_domain(of_the_service(), stand_in::desktop()).await;
    let (host, port) = machine.address();

    // who the person is in the domain: a certificate for a key of the gateway's, and a first ticket
    let who = domain.themselves("5e55").await.expect("the person, as the domain knows them").expect("a person Away signs in so");
    assert_eq!(who.account(), "alex.urban@CORP.EXAMPLE");
    let issued = issuer.issued.lock().unwrap().last().cloned().expect("a certificate was issued");

    // on the machine: a ticket for its desktop, and the card
    let theirs = domain.on(&who, &host).await.expect("what signs them in on the machine");
    let secured = reach(&host, port, None).await.expect("it is reached").secure().await.expect("the connection is secured");
    let Ok(Opened::Desktop(mut desktop)) =
        secured.sign_in(Who::Themselves(Box::new(theirs)), Screen::of(1280, 800), 0x0807, None, None).await
    else {
        panic!("the person is signed in, and the machine shows its desktop")
    };
    let shows = stand_in::desktop();
    until(&mut desktop, "the first picture", |desktop| desktop.picture() == &shows).await;

    // the machine read in the ticket who it is for, took the reader, and read the card to its end
    assert_eq!(machine.shown(), [("alex.urban".to_string(), "CORP.EXAMPLE".to_string())]);
    let read_the_card = async {
        loop {
            if machine.seen().last().and_then(|seen| seen.card.as_ref()).is_some_and(|card| card.closed) {
                break;
            }
            if let Ok(arrived) = tokio::time::timeout(Duration::from_millis(50), desktop.arrives()).await {
                arrived.expect("the machine stays");
            }
            desktop.tick().await.expect("the machine stays");
            desktop.take().await.expect("what the machine sends is read");
        }
    };
    tokio::time::timeout(Duration::from_secs(10), read_the_card).await.expect("the machine reads the card to its end");
    let seen = machine.seen().last().cloned().expect("the client came as far as the desktop");
    assert_eq!(seen.channels, ["drdynvc", "rdpdr", "rdpsnd"]);
    let card = seen.card.expect("the machine read the card");
    assert_eq!(card.reader, "Away 0");
    let read = away_winauth::card::stand_in::read(&card.answers).expect("the answers of a card of this kind");
    assert!(read.certificate == issued, "the certificate on the card is the one the issuer gave");
    assert!(read.took_the_pin && read.container.starts_with("away-"));
    assert!(away_winauth::card::stand_in::is_signed_by(&issued, TO_SIGN, &read.signed), "the card signed with the certificate's key");
    desktop.leave().await;

    // a second machine of the session, as where a farm's broker sends the person on:
    // another ticket, and a card of its own, with the same certificate
    let again = domain.on(&who, &host).await.expect("what signs them in on the next machine");
    let secured = reach(&host, port, None).await.expect("it is reached").secure().await.expect("the connection is secured");
    assert!(matches!(
        secured.sign_in(Who::Themselves(Box::new(again)), Screen::of(1280, 800), 0x0807, None, None).await,
        Ok(Opened::Desktop(_))
    ));
    assert_eq!(issuer.issued.lock().unwrap().len(), 1, "one certificate serves the session's way to its machine");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn what_keeps_a_person_from_being_themselves_is_said_at_the_sign_in() {
    let Some(kerberos) = kerberos() else { return };
    let issuer = issuer().await;
    let domain = domain(&kerberos, &issuer);

    // the issuer refuses: in its own words
    let Err(refused) = domain.themselves("refused").await else { panic!("the issuer signs nobody in for this session") };
    assert_eq!((refused.step, refused.why), (Step::SigningIn, Why::NotThemselves));
    assert_eq!(
        refused.details,
        "The organisation's authority refused to sign CORP\\alex.urban in. It says: Denied by Policy Module (authority-refused)"
    );
    // a person Away signs in as themselves on no machine is no failure: there is
    // nothing that signs them in, and they type as on any machine
    assert!(matches!(domain.themselves("never").await, Ok(None)));

    // a machine the domain knows no desktop of
    let who = domain.themselves("5e55").await.expect("the person, as the domain knows them").expect("a person Away signs in so");
    let Err(unknown) = domain.on(&who, "elsewhere.corp.example").await else { panic!("the domain knows no such machine") };
    assert_eq!((unknown.step, unknown.why), (Step::SigningIn, Why::NotThemselves));
    assert!(unknown.details.contains("asked for a ticket for the desktop of elsewhere.corp.example: "), "{}", unknown.details);

    // a machine that holds another key than the one the domain knows its service by
    let another = Key::new(Kind::Aes256, &[0x17; 32]).expect("a key");
    let machine = Machine::start_in_a_domain(another, stand_in::desktop()).await;
    let (host, port) = machine.address();
    let theirs = domain.on(&who, &host).await.expect("what signs them in on the machine");
    let secured = reach(&host, port, None).await.expect("it is reached").secure().await.expect("the connection is secured");
    let Err(not_taken) = secured.sign_in(Who::Themselves(Box::new(theirs)), Screen::of(1280, 800), 0x0807, None, None).await else {
        panic!("a machine that can't open the ticket signs nobody in")
    };
    assert_eq!((not_taken.step, not_taken.why), (Step::SigningIn, Why::NotThemselves));

    // an issuer that is not there, and a domain controller that is not
    let nowhere = Issuer::new(Client::new(None).unwrap(), Address::read("http://127.0.0.1:1").unwrap());
    let lines: Vec<String> =
        base64::ordinary(AUTHORITY).as_bytes().chunks(64).map(|line| String::from_utf8_lossy(line).to_string()).collect();
    let authority = format!("-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n", lines.join("\n"));
    let without_issuer = Domain::new("corp.example", std::slice::from_ref(&kerberos), authority.as_bytes(), nowhere).unwrap();
    let Err(unreached) = without_issuer.themselves("5e55").await else { panic!("an issuer that is not there issues nothing") };
    assert!(unreached.details.starts_with("the issuer: it couldn't be reached"), "{}", unreached.details);
    let asks = Issuer::new(Client::new(None).unwrap(), issuer.at.clone());
    let without_controller = Domain::new("corp.example", &["127.0.0.1:1".to_string()], authority.as_bytes(), asks).unwrap();
    let Err(unreached) = without_controller.themselves("5e55").await else {
        panic!("a domain controller that is not there gives no ticket")
    };
    assert!(unreached.details.starts_with("the domain controller 127.0.0.1 could not be reached"), "{}", unreached.details);
}
