//! The test program against a stand-in for a machine: something that answers the first
//! request as a Windows machine does and secures the connection with a certificate of its
//! own. The pipeline can't reach a Windows machine, so this is what it runs on every
//! change; real machines are tried by hand and by the Windows suite.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Output};
use std::sync::Arc;
use std::thread::JoinHandle;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection};

const CERTIFICATE: &[u8] = include_bytes!("fixtures/machine.der");
const KEY: &[u8] = include_bytes!("fixtures/machine.key.der");

/// What a Windows machine answers when it chooses the sign-in before the desktop.
const CHOSEN: [u8; 19] = [0x03, 0x00, 0x00, 0x13, 0x0e, 0xd0, 0x00, 0x00, 0x12, 0x34, 0x00, 0x02, 0x3f, 0x08, 0x00, 0x08, 0x00, 0x00, 0x00];
/// What a machine answers that takes no connection over TLS.
const REFUSED: [u8; 19] =
    [0x03, 0x00, 0x00, 0x13, 0x0e, 0xd0, 0x00, 0x00, 0x12, 0x34, 0x00, 0x03, 0x00, 0x08, 0x00, 0x02, 0x00, 0x00, 0x00];

/// A stand-in for a machine: it takes one connection, reads the first request, answers
/// with these bytes and, when asked to, secures the connection. It gives the request it
/// read.
fn machine(answer: &'static [u8], secures: bool) -> (String, JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("an address");
    let address = listener.local_addr().expect("its address").to_string();
    let running = std::thread::spawn(move || {
        let (mut connection, _) = listener.accept().expect("a connection");
        let mut request = [0u8; 19];
        connection.read_exact(&mut request).expect("the first request");
        connection.write_all(answer).expect("the answer goes out");
        if secures {
            secure(&mut connection);
        }
        request.to_vec()
    });
    (address, running)
}

fn secure(connection: &mut TcpStream) {
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
    // the sign-in before the desktop: read its first message, and answer as a Windows
    // machine of a domain does, saying who it is
    let mut secured = rustls::Stream::new(&mut secured, connection);
    let mut first = Vec::new();
    loop {
        match away_rdp::der::whole(&first) {
            Ok(Some(length)) if first.len() >= length => break,
            Ok(_) => {}
            Err(_) => return,
        }
        let mut bytes = [0u8; 512];
        match secured.read(&mut bytes) {
            Ok(count) if count > 0 => first.extend_from_slice(&bytes[..count]),
            _ => return,
        }
    }
    assert_eq!(first, away_rdp::sign_in::begin(), "the first message of the sign-in");
    let challenge = away_rdp::ntlm::challenge_for_tests("FIN-APP-02", "CORP", "corp.example.com", 26_100);
    let _ = secured.write_all(&away_rdp::sign_in::request(6, &challenge));
    // the test program has nothing more to say: wait until it leaves
    let mut rest = Vec::new();
    let _ = secured.read_to_end(&mut rest);
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
    assert_eq!(lines[15], "The next step is proving who one is, with a name and a password, which this version doesn't speak yet.");
    assert_eq!(lines.len(), 16);
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
    assert!(String::from_utf8_lossy(&none.stderr).contains("away-try rdp <address>"));
    let wrong = Command::new(env!("CARGO_BIN_EXE_away-try")).args(["rdp", "fin-app-02:port"]).output().expect("the test program runs");
    assert_eq!(wrong.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&wrong.stdout).contains("port is no port."));
}
