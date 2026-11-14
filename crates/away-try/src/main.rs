//! Tries a machine with Away's own protocol code, as far as that code goes, and says what
//! happened at each step:
//!
//! ```text
//! away-try rdp fin-app-02.example.com
//! ```
//!
//! It is for the people who build Away: the proof that the protocol code works against
//! real machines, with no browser and nothing else of Away around it. It is not in the
//! image.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use away_rdp::identity::Identity;
use away_rdp::negotiation::{self, Security};
use away_rdp::tpkt::Frames;
use away_rdp::{der, sign_in};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, ClientConnection, DigitallySignedStruct, SignatureScheme};

const USAGE: &str = "\
away-try rdp <address>

  Reaches a machine over RDP with Away's own code and says what it shows, step by step.
  The address is a name or a number, with a port after a colon where it is not 3389.
";

/// How long a machine gets to answer, at each step.
const WITHIN: Duration = Duration::from_secs(10);

fn main() -> ExitCode {
    let words: Vec<String> = std::env::args().skip(1).collect();
    match words.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["rdp", address] => match rdp(address) {
            Ok(()) => ExitCode::SUCCESS,
            Err(why) => {
                println!("   {why}");
                ExitCode::FAILURE
            }
        },
        ["help" | "--help" | "-h"] => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        _ => {
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// The steps of a connection over RDP, as far as Away's code goes.
fn rdp(address: &str) -> Result<(), String> {
    let (host, port) = match address.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => (host, port.parse::<u16>().map_err(|_| format!("{port} is no port."))?),
        _ => (address, 3389),
    };

    println!("Reaching {host}, on port {port}");
    let began = Instant::now();
    let to = (host, port)
        .to_socket_addrs()
        .map_err(|_| format!("The name {host} is not known."))?
        .next()
        .ok_or_else(|| format!("The name {host} is not known."))?;
    let mut connection = TcpStream::connect_timeout(&to, WITHIN).map_err(|error| match error.kind() {
        std::io::ErrorKind::TimedOut => {
            format!("It didn't answer within {} seconds. It may be switched off, or its address may have changed.", WITHIN.as_secs())
        }
        std::io::ErrorKind::ConnectionRefused => "It takes no connection on this port.".to_string(),
        _ => format!("It couldn't be reached: {error}."),
    })?;
    connection
        .set_read_timeout(Some(WITHIN))
        .and_then(|()| connection.set_write_timeout(Some(WITHIN)))
        .map_err(|error| error.to_string())?;
    println!("   It answered in {} ms.", began.elapsed().as_millis());

    println!("Asking which security it takes");
    connection.write_all(&negotiation::request()).map_err(|error| format!("The request couldn't be sent: {error}."))?;
    let mut frames = Frames::new();
    let carried = loop {
        if let Some(carried) = frames.next_frame().map_err(|error| error.to_string())? {
            break carried;
        }
        let mut bytes = [0u8; 512];
        match connection.read(&mut bytes) {
            Ok(0) => return Err("It closed the connection without an answer.".to_string()),
            Ok(count) => frames.push(&bytes[..count]),
            Err(_) => return Err(format!("It gave no answer within {} seconds.", WITHIN.as_secs())),
        }
    };
    let answer = negotiation::read_answer(&carried).map_err(|error| error.to_string())?;
    println!(
        "   It chose {}.",
        match answer.security {
            Security::Tls => "TLS, with the sign-in on its own screen",
            Security::SignInFirst { tells_result: false } => "the sign-in before the desktop",
            Security::SignInFirst { tells_result: true } =>
                "the sign-in before the desktop, and says whether one may come in before it goes on",
        }
    );
    let also: Vec<&str> = [
        (answer.graphics_pipeline, "the graphics pipeline"),
        (answer.extended_client_data, "the longer form of what a client says of itself"),
        (answer.restricted_admin, "a sign-in that leaves no credential on it"),
        (answer.redirected_sign_in, "a sign-in that the person's own device answers for"),
    ]
    .into_iter()
    .filter_map(|(has, what)| has.then_some(what))
    .collect();
    if !also.is_empty() {
        println!("   It also takes: {}.", also.join("; "));
    }

    println!("Securing the connection");
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .map_err(|error| error.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AnyCertificate(provider)))
        .with_no_client_auth();
    let name = ServerName::try_from(host.to_string()).map_err(|_| format!("{host} is no name a certificate could be for."))?;
    let mut secured = ClientConnection::new(Arc::new(config), name).map_err(|error| error.to_string())?;
    while secured.is_handshaking() {
        secured.complete_io(&mut connection).map_err(|error| format!("The connection couldn't be secured: {error}."))?;
    }
    let version = secured.protocol_version().map(|version| format!("{version:?}")).unwrap_or_default();
    let suite = secured.negotiated_cipher_suite().map(|suite| format!("{:?}", suite.suite())).unwrap_or_default();
    println!("   {}, with {suite}.", version.replace("TLSv1_", "TLS 1."));

    println!("What it shows of itself");
    let shown = secured.peer_certificates().and_then(|all| all.first()).ok_or("It showed no certificate.")?;
    let identity = Identity::read(shown.as_ref()).map_err(|error| error.to_string())?;
    let name = identity.name.as_deref().unwrap_or("no name");
    match (&identity.issued_by, identity.issued_by_itself) {
        (_, true) => println!("   A certificate for {name}, which it issued itself."),
        (Some(issuer), false) => println!("   A certificate for {name}, issued by {issuer}."),
        (None, false) => println!("   A certificate for {name}, issued by an authority without a name."),
    }
    println!("   Valid from {} until {}.", identity.valid_from, identity.valid_until);
    println!("   Its fingerprint (SHA-256): {}", identity.fingerprint_text());

    if answer.security == Security::Tls {
        println!("The next step is what a client says of itself, which this version doesn't speak yet.");
        return Ok(());
    }

    println!("Signing in, before the desktop");
    let mut secured = rustls::Stream::new(&mut secured, &mut connection);
    secured.write_all(&sign_in::begin()).map_err(|error| format!("The sign-in couldn't be begun: {error}."))?;
    let mut held = Vec::new();
    let length = loop {
        match der::whole(&held).map_err(|error| error.to_string())? {
            Some(length) if held.len() >= length => break length,
            _ => {}
        }
        let mut bytes = [0u8; 2048];
        match secured.read(&mut bytes) {
            Ok(0) => return Err("It closed the connection in the middle of the sign-in.".to_string()),
            Ok(count) => held.extend_from_slice(&bytes[..count]),
            Err(_) => return Err(format!("It gave no answer to the sign-in within {} seconds.", WITHIN.as_secs())),
        }
    };
    let (version, challenge) = sign_in::read_challenge(&held[..length]).map_err(|error| error.to_string())?;
    let computer = challenge.computer.as_deref().unwrap_or("a machine without a name");
    match (&challenge.domain, &challenge.computer_dns) {
        (Some(domain), _) if domain == computer => println!("   It answers as {computer}, which is in no domain."),
        (Some(domain), Some(full)) => println!("   It answers as {computer} of the domain {domain}, in full {full}."),
        (Some(domain), None) => println!("   It answers as {computer} of the domain {domain}."),
        (None, _) => println!("   It answers as {computer}."),
    }
    if let Some(windows) = challenge.version {
        println!("   It says it runs Windows {}.{}, build {}.", windows.major, windows.minor, windows.build);
    }
    println!("   It speaks version {version} of the sign-in.");
    println!("The next step is proving who one is, with a name and a password, which this version doesn't speak yet.");
    Ok(())
}

/// Takes whatever certificate a machine shows, and still makes the machine prove that it
/// holds the certificate's key. Whether the certificate is the one Away knows the machine
/// by is decided after the connection is secured, from what [`Identity`] reads.
#[derive(Debug)]
struct AnyCertificate(Arc<CryptoProvider>);

impl ServerCertVerifier for AnyCertificate {
    fn verify_server_cert(
        &self,
        _certificate: &CertificateDer<'_>,
        _others: &[CertificateDer<'_>],
        _name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signed: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(message, certificate, signed, &self.0.signature_verification_algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signed: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, certificate, signed, &self.0.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}
