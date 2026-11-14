//! The test program against a stand-in for a machine: something that answers the first
//! request as a Windows machine does and secures the connection with a certificate of its
//! own. The pipeline can't reach a Windows machine, so this is what it runs on every
//! change; real machines are tried by hand and by the Windows suite.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Output};
use std::sync::Arc;
use std::thread::JoinHandle;

use away_rdp::identity::Identity;
use away_rdp::ntlm::Account;
use away_rdp::secret::Secret;
use away_rdp::stand_in::StandIn;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection};

const CERTIFICATE: &[u8] = include_bytes!("fixtures/machine.der");
const KEY: &[u8] = include_bytes!("fixtures/machine.key.der");

/// What a Windows machine answers when it chooses the sign-in before the desktop.
const CHOSEN: [u8; 19] = [0x03, 0x00, 0x00, 0x13, 0x0e, 0xd0, 0x00, 0x00, 0x12, 0x34, 0x00, 0x02, 0x3f, 0x08, 0x00, 0x08, 0x00, 0x00, 0x00];
/// What a machine answers that takes no connection over TLS.
const REFUSED: [u8; 19] =
    [0x03, 0x00, 0x00, 0x13, 0x0e, 0xd0, 0x00, 0x00, 0x12, 0x34, 0x00, 0x03, 0x00, 0x08, 0x00, 0x02, 0x00, 0x00, 0x00];

/// The account the stand-in knows, and its password. Neither exists anywhere else.
const KNOWN: &str = "CORP\\alex.urban";
const PASSWORD: &str = "a password that exists for these tests alone";

/// The stand-in's verdict on whether the account may come in: it may, or access is refused.
const MAY_COME_IN: [u8; 4] = [0, 0, 0, 0];
const MAY_NOT: [u8; 4] = [5, 0, 0, 0];

/// A stand-in for a machine: it takes one connection, reads the first request, answers
/// with these bytes and, when asked to, secures the connection and takes the sign-in
/// before the desktop, ending with this verdict. It gives the request it read.
fn machine(answer: &'static [u8], secures: bool) -> (String, JoinHandle<Vec<u8>>) {
    machine_with_verdict(answer, secures, MAY_COME_IN)
}

fn machine_with_verdict(answer: &'static [u8], secures: bool, verdict: [u8; 4]) -> (String, JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("an address");
    let address = listener.local_addr().expect("its address").to_string();
    let running = std::thread::spawn(move || {
        let (mut connection, _) = listener.accept().expect("a connection");
        let mut request = [0u8; 19];
        connection.read_exact(&mut request).expect("the first request");
        connection.write_all(answer).expect("the answer goes out");
        if secures {
            secure(&mut connection, verdict);
        }
        request.to_vec()
    });
    (address, running)
}

fn secure(connection: &mut TcpStream, verdict: [u8; 4]) {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("versions")
        .with_no_client_auth()
        .with_single_cert(vec![CertificateDer::from(CERTIFICATE.to_vec())], PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(KEY.to_vec())))
        .expect("the stand-in's certificate");
    let mut secured = ServerConnection::new(Arc::new(config)).expect("a connection to secure");
    while secured.is_handshaking() {
        if secured.complete_io(connection).is_err() {
            return;
        }
    }
    // the sign-in before the desktop, answered as a Windows machine of a domain answers:
    // it knows one account, and holds the certificate it showed
    let mut secured = rustls::Stream::new(&mut secured, connection);
    let holds = Identity::read(CERTIFICATE).expect("the stand-in's certificate").public_key;
    let mut stand_in = StandIn::new(Account::named(KNOWN, Secret::new(PASSWORD)), &holds);
    loop {
        let mut message = Vec::new();
        loop {
            match away_rdp::der::whole(&message) {
                Ok(Some(length)) if message.len() >= length => break,
                Ok(_) => {}
                Err(_) => return,
            }
            let mut bytes = [0u8; 4096];
            match secured.read(&mut bytes) {
                Ok(count) if count > 0 => message.extend_from_slice(&bytes[..count]),
                // the client left: it had no account, or it was refused
                _ => return,
            }
        }
        match stand_in.answer(&message) {
            Ok(answer) if answer.is_empty() => break,
            Ok(answer) => {
                if secured.write_all(&answer).is_err() {
                    return;
                }
            }
            Err(_) => return,
        }
    }
    // the name and the password have arrived: say whether the account may come in
    let _ = secured.write_all(&verdict);
    let mut rest = Vec::new();
    let _ = secured.read_to_end(&mut rest);
}

/// A file that holds a password, as the test program takes one.
fn password_file(password: &str) -> std::path::PathBuf {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let file =
        std::env::temp_dir().join(format!("away-try-{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    std::fs::write(&file, format!("{password}\n")).expect("a file for the password");
    file
}

fn try_rdp_as(address: &str, account: &str, password: &str) -> Output {
    let file = password_file(password);
    let tried = Command::new(env!("CARGO_BIN_EXE_away-try"))
        .args(["rdp", address, "--as", account, "--password-file"])
        .arg(&file)
        .output()
        .expect("the test program runs");
    let _ = std::fs::remove_file(file);
    tried
}

fn try_rdp(address: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_away-try")).args(["rdp", address]).output().expect("the test program runs")
}

#[test]
fn it_reaches_a_machine_secures_the_connection_and_says_what_the_machine_shows() {
    let (address, running) = machine(&CHOSEN, true);
    let tried = try_rdp(&address);
    let said = String::from_utf8(tried.stdout).expect("text");
    assert_eq!(running.join().expect("the stand-in ends"), away_rdp::negotiation::request(), "what it asked for");
    assert!(tried.status.success(), "{said}");
    let lines: Vec<&str> = said.lines().collect();
    assert!(lines[0].starts_with("Reaching 127.0.0.1, on port "), "{said}");
    assert!(lines[1].starts_with("   It answered in "), "{said}");
    assert_eq!(lines[2], "Asking which security it takes");
    assert_eq!(lines[3], "   It chose the sign-in before the desktop, and says whether one may come in before it goes on.");
    assert!(lines[4].starts_with("   It also takes: the graphics pipeline; "), "{said}");
    assert_eq!(lines[5], "Securing the connection");
    assert!(lines[6].starts_with("   TLS 1.3, with "), "{said}");
    assert_eq!(lines[7], "What it shows of itself");
    assert_eq!(lines[8], "   A certificate for fin-app-02.example.com, which it issued itself.");
    assert!(lines[9].starts_with("   Valid from 2026-11-14T") && lines[9].ends_with(" until 2126-10-21T12:54:24Z."), "{said}");
    assert_eq!(
        lines[10],
        "   Its fingerprint (SHA-256): 21:1c:8d:21:3d:84:d9:75:86:5d:58:a0:04:0e:0c:10:08:be:f0:af:5e:5c:cd:49:71:1e:46:1d:fc:97:a6:c3"
    );
    assert_eq!(lines[11], "Signing in, before the desktop");
    assert_eq!(lines[12], "   It answers as FIN-APP-02 of the domain CORP, in full FIN-APP-02.corp.example.com.");
    assert_eq!(lines[13], "   It says it runs Windows 10.0, build 26100.");
    assert_eq!(lines[14], "   It speaks version 6 of the sign-in.");
    assert_eq!(lines[15], "To go on, name an account: --as <account> --password-file <file>.");
    assert_eq!(lines.len(), 16);
}

#[test]
fn it_signs_an_account_in_before_the_desktop() {
    let (address, running) = machine(&CHOSEN, true);
    let tried = try_rdp_as(&address, KNOWN, PASSWORD);
    let said = String::from_utf8(tried.stdout).expect("text");
    running.join().expect("the stand-in ends");
    assert!(tried.status.success(), "{said}");
    let lines: Vec<&str> = said.lines().skip(11).collect();
    assert_eq!(
        lines,
        [
            "Signing in, before the desktop, as CORP\\alex.urban",
            "   It answers as FIN-APP-02 of the domain CORP, in full FIN-APP-02.corp.example.com.",
            "   It says it runs Windows 10.0, build 26100.",
            "   It took the proof of the password, and proved that it holds the certificate it showed.",
            "   The name and the password were handed over, sealed.",
            "   It says the account may come in.",
            "The next step is what a client says of itself, which this version doesn't speak yet.",
        ],
        "{said}"
    );
    assert!(!said.contains(PASSWORD), "the password is never said");
}

#[test]
fn a_wrong_password_is_refused_and_said_so() {
    let (address, running) = machine(&CHOSEN, true);
    let tried = try_rdp_as(&address, KNOWN, "another password");
    let said = String::from_utf8(tried.stdout).expect("text");
    running.join().expect("the stand-in ends");
    assert_eq!(tried.status.code(), Some(1));
    assert!(said.ends_with("   It refused the sign-in: the name or the password is wrong (its own code 0xc000006d).\n"), "{said}");
    assert!(!said.contains("another password"));
}

#[test]
fn an_account_that_may_not_come_in_is_told_so() {
    let (address, running) = machine_with_verdict(&CHOSEN, true, MAY_NOT);
    let tried = try_rdp_as(&address, KNOWN, PASSWORD);
    let said = String::from_utf8(tried.stdout).expect("text");
    running.join().expect("the stand-in ends");
    assert_eq!(tried.status.code(), Some(1));
    assert!(said.ends_with("   The name and the password were handed over, sealed.\n   It took the sign-in, and says this account may not come in over RDP here.\n"), "{said}");
}

#[test]
fn a_password_file_that_is_not_there_is_said_to_be_missing() {
    let tried = Command::new(env!("CARGO_BIN_EXE_away-try"))
        .args(["rdp", "fin-app-02.example.com", "--as", KNOWN, "--password-file", "/nowhere/at/all"])
        .output()
        .expect("the test program runs");
    assert_eq!(tried.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&tried.stdout).starts_with("   The password's file can't be read: "));
}

#[test]
fn a_machine_that_refuses_is_said_to_refuse_and_why() {
    let (address, running) = machine(&REFUSED, false);
    let tried = try_rdp(&address);
    let said = String::from_utf8(tried.stdout).expect("text");
    running.join().expect("the stand-in ends");
    assert_eq!(tried.status.code(), Some(1));
    assert!(said.ends_with("Asking which security it takes\n   It takes no connection over TLS.\n"), "{said}");
}

#[test]
fn something_that_is_no_rdp_machine_is_said_to_be_none() {
    let (address, running) = machine(b"HTTP/1.1 400 Bad Request\r\n\r\n", false);
    let tried = try_rdp(&address);
    let said = String::from_utf8(tried.stdout).expect("text");
    running.join().expect("the stand-in ends");
    assert_eq!(tried.status.code(), Some(1));
    assert!(said.ends_with("   What it sent is not RDP: a frame doesn't begin as one.\n"), "{said}");
}

#[test]
fn a_port_nothing_listens_on_is_said_to_take_no_connection() {
    let free = TcpListener::bind("127.0.0.1:0").expect("an address").local_addr().expect("its address").to_string();
    let tried = try_rdp(&free);
    let said = String::from_utf8(tried.stdout).expect("text");
    assert_eq!(tried.status.code(), Some(1));
    assert!(said.ends_with("   It takes no connection on this port.\n"), "{said}");
}

#[test]
fn it_says_what_it_takes() {
    let none = Command::new(env!("CARGO_BIN_EXE_away-try")).output().expect("the test program runs");
    assert_eq!(none.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&none.stderr).contains("away-try rdp <address> [--as <account> --password-file <file>]"));
    let wrong = Command::new(env!("CARGO_BIN_EXE_away-try")).args(["rdp", "fin-app-02:port"]).output().expect("the test program runs");
    assert_eq!(wrong.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&wrong.stdout).contains("port is no port."));
}
