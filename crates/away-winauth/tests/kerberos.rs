//! Away's own Kerberos against a real Kerberos server: with a certificate, a first
//! ticket; with that ticket, one for a machine. `AWAY_TEST_KERBEROS` names the server
//! (`tests/machines/kerberos/real.sh` starts one). Without it these tests say so and pass.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use away_winauth::kerberos::exchange::{framed, unframed, First, Later, Ticket};
use away_winauth::kerberos::messages::{Name, NameKind};
use away_winauth::kerberos::pkinit::{Fresh, Held, Trusts};
use away_winauth::Error;

const AUTHORITY: &[u8] = include_bytes!("fixtures/authority.der");
const ACCOUNT: &[u8] = include_bytes!("fixtures/account.der");
const ACCOUNT_KEY: &[u8] = include_bytes!("fixtures/account.key.der");
const CONTROLLER_KEY: &[u8] = include_bytes!("fixtures/controller.key.der");
const CONTROLLER: &[u8] = include_bytes!("fixtures/controller.der");
const STRANGER: &[u8] = include_bytes!("fixtures/stranger.der");

/// Where the tests' Kerberos server is.
fn server() -> Option<String> {
    let at = std::env::var("AWAY_TEST_KERBEROS").ok().filter(|at| !at.is_empty());
    if at.is_none() {
        // the pipeline always has one: there, a test that finds none has not tested anything
        assert!(std::env::var_os("CI").is_none(), "the pipeline's tests need a Kerberos server: AWAY_TEST_KERBEROS is not set");
        eprintln!("no Kerberos server to test against: AWAY_TEST_KERBEROS is not set");
    }
    at
}

/// Sends a request, and gives what came back.
fn ask(at: &str, request: &[u8]) -> Vec<u8> {
    let mut connection = TcpStream::connect(at).expect("the server takes connections");
    connection.set_read_timeout(Some(Duration::from_secs(10))).expect("a time limit");
    connection.write_all(&framed(request)).expect("the request goes out");
    let mut arrived = Vec::new();
    loop {
        if let Some(reply) = unframed(&arrived).expect("an answer") {
            return reply.to_vec();
        }
        let mut bytes = [0u8; 4096];
        match connection.read(&mut bytes).expect("an answer comes back") {
            0 => panic!("the server closed the connection before it had answered"),
            count => arrived.extend_from_slice(&bytes[..count]),
        }
    }
}

fn now() -> (u64, u32) {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("a clock");
    (now.as_secs(), now.subsec_micros())
}

fn fresh(seed: u8) -> Fresh {
    Fresh { secret: [seed; 32], once: [seed.wrapping_add(1); 32] }
}

/// The first ticket, for an account, with a certificate and its key.
fn first_ticket(at: &str, account: &str, certificate: &[u8], key: &[u8], authority: &[u8]) -> Result<Ticket, Error> {
    let held = Held::new(certificate, key).expect("a certificate and its key");
    let (first, request) = First::begin(&held, account, "CORP.EXAMPLE", now(), 0x1357_9bdf, fresh(0x42))?;
    first.receive(&ask(at, &request), &Trusts { authorities: &[authority], controller: "localhost" }, now().0)
}

#[test]
fn a_real_server_gives_a_first_ticket_for_a_certificate_and_with_it_one_for_a_machine() {
    let Some(at) = server() else { return };
    let first = first_ticket(&at, "alex.urban", ACCOUNT, ACCOUNT_KEY, AUTHORITY).expect("a first ticket");
    assert_eq!((first.realm.as_str(), &first.account), ("CORP.EXAMPLE", &Name::new(NameKind::Account, &["alex.urban"])));
    assert_eq!(first.service, Name::new(NameKind::Service, &["krbtgt", "CORP.EXAMPLE"]));
    assert!(first.until > now().0 && first.until <= now().0 + 601, "good for ten minutes at most");

    // with the first ticket, one for a machine's desktop
    let machine = Name::new(NameKind::Service, &["TERMSRV", "desk-01.corp.example"]);
    let (later, request) = Later::begin(&first, machine.clone(), now(), 0x2468_ace0, [7; 16]);
    let for_the_machine = later.receive(&ask(&at, &request)).expect("a ticket for the machine");
    assert_eq!((for_the_machine.service, &for_the_machine.account), (machine, &first.account));
    assert_ne!(for_the_machine.key, first.key);
    assert_ne!(for_the_machine.bytes, first.bytes);

    // a service the server doesn't know is refused, and the refusal says why
    let nothing = Name::new(NameKind::Service, &["TERMSRV", "nowhere.corp.example"]);
    let (later, request) = Later::begin(&first, nothing, now(), 5, [8; 16]);
    let refused = later.receive(&ask(&at, &request)).expect_err("no such service");
    assert!(matches!(refused, Error::Refused { code: 7, .. }), "{refused}");
    assert!(refused.to_string().starts_with("The domain controller refused: it knows no such service (its own code 7)"), "{refused}");
}

#[test]
fn a_real_server_refuses_who_is_not_the_certificates_account_and_is_itself_not_believed_without_proof() {
    let Some(at) = server() else { return };
    // an account the server doesn't know, and one that is another's than the certificate's
    let unknown = first_ticket(&at, "nobody.known", ACCOUNT, ACCOUNT_KEY, AUTHORITY).expect_err("no such account");
    assert!(matches!(unknown, Error::Refused { code: 6, .. }), "{unknown}");
    let another = first_ticket(&at, "sam.other", ACCOUNT, ACCOUNT_KEY, AUTHORITY).expect_err("another's account");
    assert!(matches!(another, Error::Refused { code: 75, .. }), "{another}");
    // a certificate of an authority the server doesn't trust: the domain controller's own key signs here,
    // under a certificate nobody issued but itself
    let untrusted = first_ticket(&at, "alex.urban", STRANGER, CONTROLLER_KEY, AUTHORITY);
    assert!(untrusted.is_err(), "a certificate nobody trusted is refused");

    // the server's own proof is checked: with another authority, the client doesn't believe it
    let disbelieved = first_ticket(&at, "alex.urban", ACCOUNT, ACCOUNT_KEY, STRANGER).expect_err("an authority that is not the server's");
    assert_eq!(disbelieved, Error::NotTrusted("its certificate leads to no authority that is trusted, or is good no longer"));
    let _ = CONTROLLER;
}
