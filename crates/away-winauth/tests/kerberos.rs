//! Away's own Kerberos against a real Kerberos server: with a certificate, a first
//! ticket; with that ticket, one for a machine; and that ticket shown to a machine that
//! takes it with the Kerberos library's own code. `AWAY_TEST_KERBEROS` names the server
//! and `AWAY_TEST_KERBEROS_MACHINE` the machine (`tests/machines/kerberos/real.sh` starts
//! both). Without them these tests say so and pass.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use away_winauth::kerberos::exchange::{framed, unframed, First, Later, Ticket, WithAPassword};
use away_winauth::kerberos::gss;
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

/// Where the machine is that takes the tickets the server gives.
fn machine() -> Option<String> {
    let at = std::env::var("AWAY_TEST_KERBEROS_MACHINE").ok().filter(|at| !at.is_empty());
    if at.is_none() {
        assert!(
            std::env::var_os("CI").is_none(),
            "the pipeline's tests need a machine that takes tickets: AWAY_TEST_KERBEROS_MACHINE is not set"
        );
        eprintln!("no machine to show a ticket to: AWAY_TEST_KERBEROS_MACHINE is not set");
    }
    at
}

/// A connection to the machine, over which whole messages go back and forth.
struct Line(TcpStream);

impl Line {
    fn to(at: &str) -> Line {
        let connection = TcpStream::connect(at).expect("the machine takes connections");
        connection.set_read_timeout(Some(Duration::from_secs(10))).expect("a time limit");
        Line(connection)
    }

    fn send(&mut self, message: &[u8]) {
        self.0.write_all(&framed(message)).expect("the message goes out");
    }

    /// The machine's next message, or nothing where it ended the connection.
    fn take(&mut self) -> Option<Vec<u8>> {
        let mut arrived = Vec::new();
        loop {
            if let Some(message) = unframed(&arrived).expect("a whole message") {
                return Some(message.to_vec());
            }
            let mut bytes = [0u8; 4096];
            match self.0.read(&mut bytes) {
                Ok(0) | Err(_) => return None,
                Ok(count) => arrived.extend_from_slice(&bytes[..count]),
            }
        }
    }
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
fn a_real_server_gives_a_first_ticket_for_a_password_and_none_for_a_wrong_one() {
    let Some(at) = server() else { return };
    let with = |password: &str| -> Result<Ticket, Error> {
        let (mut asking, request) = WithAPassword::begin("pat.typed", "CORP.EXAMPLE", now(), 0x0bad_cafe);
        let proof = asking.prove(&ask(&at, &request), password, now(), [9; 16])?;
        asking.receive(&ask(&at, &proof))
    };
    let first = with("a password that exists for these tests alone").expect("a first ticket");
    assert_eq!((first.realm.as_str(), &first.account), ("CORP.EXAMPLE", &Name::new(NameKind::Account, &["pat.typed"])));
    // and with it a ticket for a machine, as with a certificate
    let machine = Name::new(NameKind::Service, &["TERMSRV", "desk-01.corp.example"]);
    let (later, request) = Later::begin(&first, machine.clone(), now(), 3, [7; 16]);
    assert_eq!(later.receive(&ask(&at, &request)).expect("a ticket for the machine").service, machine);

    // a wrong password proves nothing, and an account that is not there is none
    let wrong = with("another password").expect_err("a wrong password");
    assert!(matches!(wrong, Error::Refused { code: 24, .. }), "{wrong}");
    let (mut asking, request) = WithAPassword::begin("nobody.known", "CORP.EXAMPLE", now(), 1);
    let unknown = asking.prove(&ask(&at, &request), "any", now(), [9; 16]).expect_err("no such account");
    assert!(matches!(unknown, Error::Refused { code: 6, .. }), "{unknown}");
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

#[test]
fn a_real_machine_takes_the_ticket_proves_itself_and_both_seal_for_each_other() {
    let (Some(at), Some(machine_at)) = (server(), machine()) else { return };
    let first = first_ticket(&at, "alex.urban", ACCOUNT, ACCOUNT_KEY, AUTHORITY).expect("a first ticket");
    let service = Name::new(NameKind::Service, &["TERMSRV", "desk-01.corp.example"]);
    let (later, request) = Later::begin(&first, service, now(), 0x1122_3344, [7; 16]);
    let ticket = later.receive(&ask(&at, &request)).expect("a ticket for the machine");
    let fresh = || gss::Fresh { key: [0x21; 32], begins_with: [0x22; 16], counts_from: 0x1234_5678 };

    // the ticket is shown, and the machine answers with its own proof
    let (shown, token) = gss::show(&ticket, now(), fresh());
    let mut line = Line::to(&machine_at);
    line.send(&token);
    let answer = line.take().expect("the machine answers the ticket");
    let (mut sealing, last) = shown.receive(&answer).expect("the machine proves itself");
    if let Some(last) = last {
        line.send(&last);
        line.take().expect("the machine takes the client's check on what was offered");
    }
    // what the client seals, the machine opens, and what the machine seals, the client
    // opens: it says who the ticket is for, as it read it in the ticket
    for (number, says) in [&b"what the client says"[..], b"and what it says next", b""].into_iter().enumerate() {
        line.send(&sealing.seal(says, [0x30 + number as u8; 16]));
        let sealed = line.take().expect("the machine answers what it could open");
        assert_eq!(sealing.open(&sealed).expect("what the machine sealed"), [b"alex.urban@CORP.EXAMPLE heard: ", says].concat());
    }
    // a message that was changed on the way is one the machine ends the connection at
    let mut changed = sealing.seal(b"changed on the way", [0x40; 16]);
    let at_its_end = changed.len() - 1;
    changed[at_its_end] ^= 1;
    line.send(&changed);
    assert_eq!(line.take(), None, "the machine opens nothing that was changed");

    // a ticket that was changed, and one shown by who doesn't have the key that goes
    // with it: the machine says that it refuses, and the client reads that as a refusal
    let mut changed = ticket.clone();
    let in_the_ticket = changed.bytes.len() - 1;
    changed.bytes[in_the_ticket] ^= 1;
    let stolen = Ticket { key: first.key.clone(), ..ticket.clone() };
    for (other, what) in [(changed, "a ticket that was changed"), (stolen, "a ticket without the key that goes with it")] {
        let (shown, token) = gss::show(&other, now(), fresh());
        let mut line = Line::to(&machine_at);
        line.send(&token);
        let refusal = line.take().expect("the machine says that it refuses");
        assert_eq!(shown.receive(&refusal).unwrap_err(), Error::NotTrusted("the machine refused the ticket"), "{what}");
    }
}
