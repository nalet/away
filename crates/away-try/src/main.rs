//! Tries a machine with Away's own protocol code, as far as that code goes, and says what
//! happened at each step:
//!
//! ```text
//! away-try rdp fin-app-02.example.com
//! away-try rdp fin-app-02.example.com --as 'CORP\alex.urban' --password-file ~/.config/away/password
//! away-try rdp fin-app-02.example.com --as 'CORP\alex.urban' --password-file ~/.config/away/password --save desktop.bmp
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

use away_rdp::connection::{Connection, Happened};
use away_rdp::der;
use away_rdp::gcc::Client;
use away_rdp::identity::Identity;
use away_rdp::negotiation::{self, Security};
use away_rdp::ntlm::{self, Account, Challenge, Fresh};
use away_rdp::picture::Picture;
use away_rdp::secret::Secret;
use away_rdp::sign_in::{self, Message, New, Next, SignIn};
use away_rdp::tpkt::Frames;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, ClientConnection, DigitallySignedStruct, SignatureScheme};
use sha2::{Digest, Sha256};

const USAGE: &str = "\
away-try rdp <address> [--as <account> --password-file <file> [--save <file>]]

  Reaches a machine over RDP with Away's own code and says what it shows, step by step.
  The address is a name or a number, with a port after a colon where it is not 3389.

  With an account it signs in and opens the desktop. The account is written as people
  write it: CORP\\alex.urban, alex.urban@corp.example.com, or a name alone. Its
  password is read from a file, so that it is on no command line.

  With --save it writes the desktop's picture to a file, as a BMP, once the machine
  has stopped drawing.

  It says the SHA-256 of the picture's pixels: three bytes each (blue, green, red), row
  by row from the top. A screenshot taken on the machine at that moment has the same.
";

/// How long a machine gets to answer, at each step.
const WITHIN: Duration = Duration::from_secs(10);
/// How long a machine has to draw nothing for its picture to count as finished.
const QUIET_FOR: Duration = Duration::from_secs(3);
/// How long what a machine draws is taken at most.
const DRAWS_AT_MOST: Duration = Duration::from_secs(45);

/// What the program was asked to do.
struct Asked {
    address: String,
    account: Option<String>,
    password_file: Option<String>,
    save: Option<String>,
}

/// Reads the words the program was started with. Gives nothing where they make no sense.
fn asked(words: &[String]) -> Option<Asked> {
    let [kind, address, rest @ ..] = words else { return None };
    let mut rest = rest;
    if kind != "rdp" || address.starts_with('-') {
        return None;
    }
    let mut asked = Asked { address: address.clone(), account: None, password_file: None, save: None };
    while let [option, value, after @ ..] = rest {
        let into = match option.as_str() {
            "--as" => &mut asked.account,
            "--password-file" => &mut asked.password_file,
            "--save" => &mut asked.save,
            _ => return None,
        };
        if into.replace(value.clone()).is_some() {
            return None;
        }
        rest = after;
    }
    // an account has a password, and a picture needs an account
    let whole = rest.is_empty() && asked.account.is_some() == asked.password_file.is_some();
    (whole && (asked.save.is_none() || asked.account.is_some())).then_some(asked)
}

fn main() -> ExitCode {
    let words: Vec<String> = std::env::args().skip(1).collect();
    if let [word] = words.as_slice() {
        if matches!(word.as_str(), "help" | "--help" | "-h") {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
    }
    let Some(asked) = asked(&words) else {
        eprint!("{USAGE}");
        return ExitCode::from(2);
    };
    let tried = match (&asked.account, &asked.password_file) {
        (Some(name), Some(file)) => match std::fs::read_to_string(file) {
            Ok(password) => {
                let account = Account::named(name, Secret::new(password.trim_end_matches(['\r', '\n'])));
                rdp(&asked.address, Some(account), asked.save.as_deref())
            }
            Err(error) => Err(format!("The password's file can't be read: {error}.")),
        },
        _ => rdp(&asked.address, None, None),
    };
    match tried {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            println!("   {why}");
            ExitCode::FAILURE
        }
    }
}

/// The steps of a connection over RDP, as far as Away's code goes.
fn rdp(address: &str, account: Option<Account>, save: Option<&str>) -> Result<(), String> {
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
    // the same connection once more, to change how long it waits while it is in use
    let waits = connection.try_clone().map_err(|error| error.to_string())?;

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

    let mut secured = rustls::Stream::new(&mut secured, &mut connection);
    let written = |account: &Account| match account.domain.as_str() {
        "" => account.name.clone(),
        domain => format!("{domain}\\{}", account.name),
    };
    let (domain, name, password) = if answer.security == Security::Tls {
        // the machine signs the account in itself, with what the client tells it
        let Some(account) = account else {
            println!("To go on, name an account: --as <account> --password-file <file>.");
            return Ok(());
        };
        println!("Opening the desktop as {}", written(&account));
        println!("   The name and the password travel to the machine inside the secured connection.");
        (account.domain, account.name, Some(account.password))
    } else {
        let Some(account) = account else {
            // with no account, the first exchange alone: the machine says who it is
            println!("Signing in, before the desktop");
            let first = Message { version: sign_in::VERSION, carried: Some(ntlm::negotiate()), ..Message::default() };
            secured.write_all(&first.write()).map_err(|error| format!("The sign-in couldn't be begun: {error}."))?;
            let answer = Message::read(&message(&mut secured)?).map_err(|error| error.to_string())?;
            let challenge = ntlm::read_challenge(&answer.carried.unwrap_or_default()).map_err(|error| error.to_string())?;
            who_it_is(&challenge);
            println!("   It speaks version {} of the sign-in.", answer.version);
            println!("To go on, name an account: --as <account> --password-file <file>.");
            return Ok(());
        };

        let (domain, name) = (account.domain.clone(), account.name.clone());
        println!("Signing in, before the desktop, as {}", written(&account));
        let mut random = [0u8; 56];
        getrandom::fill(&mut random).map_err(|error| format!("The system gave no random bytes: {error}."))?;
        let new = New {
            for_the_proof: Fresh { challenge: random[..8].try_into().expect("8 bytes"), key: random[8..24].try_into().expect("16 bytes") },
            number: random[24..].try_into().expect("32 bytes"),
        };
        let (mut signing_in, first) = SignIn::begin(account, host, &identity.public_key, new, clock());
        secured.write_all(&first).map_err(|error| format!("The sign-in couldn't be begun: {error}."))?;
        let mut said_who = false;
        loop {
            let next = signing_in.receive(&message(&mut secured)?).map_err(|error| error.to_string())?;
            if let (false, Some(challenge)) = (said_who, signing_in.machine()) {
                who_it_is(challenge);
                said_who = true;
            }
            match next {
                Next::Send(bytes) => secured.write_all(&bytes).map_err(|error| format!("The sign-in couldn't go on: {error}."))?,
                Next::SendLast(bytes) => {
                    secured.write_all(&bytes).map_err(|error| format!("The sign-in couldn't be ended: {error}."))?;
                    break;
                }
            }
        }
        println!("   It took the proof of the password, and proved that it holds the certificate it showed.");
        println!("   The name and the password were handed over, sealed.");

        if answer.security == (Security::SignInFirst { tells_result: true }) {
            let mut verdict = [0u8; 4];
            secured.read_exact(&mut verdict).map_err(|_| {
                "It closed the connection where it should say whether the account may come in: the name or the password may be wrong."
                    .to_string()
            })?;
            sign_in::read_verdict(verdict).map_err(|error| error.to_string())?;
            println!("   It says the account may come in.");
        }
        (domain, name, None)
    };

    println!("Saying what Away is, and taking the channels");
    let client = Client { width: 1280, height: 800, security: answer.security.number(), name: "away".to_string() };
    let (mut open, first) = Connection::begin(client, &domain, &name, password);
    secured.write_all(&first).map_err(|error| format!("It couldn't be sent: {error}."))?;
    let mut frames = Frames::new();
    let mut picture: Option<Picture> = None;
    let mut opened: Option<Instant> = None;
    let mut last_drawn: Option<Instant> = None;
    let mut drawn: std::collections::BTreeMap<(u16, bool), (usize, usize)> = std::collections::BTreeMap::new();
    loop {
        while let Some(frame) = frames.next_of_either_kind().map_err(|error| error.to_string())? {
            let (send, happened) = open.receive(frame).map_err(|error| error.to_string())?;
            if !send.is_empty() {
                secured.write_all(&send).map_err(|error| format!("It couldn't be sent: {error}."))?;
            }
            for happened in happened {
                match happened {
                    Happened::Channels { user, desktop } => {
                        println!("   It gave the channel {user} to this client, and {desktop} is the desktop's.")
                    }
                    Happened::Licensed => {
                        println!("   It takes the client as it is, without a licence.");
                        println!("Agreeing on what each side can do");
                    }
                    Happened::Desktop { width, height, bits } => {
                        println!("   It offers a desktop of {width} by {height}, at {bits} bits a pixel.");
                        picture = Some(Picture::new(usize::from(width), usize::from(height)));
                    }
                    Happened::Open => {
                        println!("   Both sides have said what they can do, and it draws from now on.");
                        println!("Taking what it draws, until it has drawn nothing for {} seconds", QUIET_FOR.as_secs());
                        opened = Some(Instant::now());
                        waits.set_read_timeout(Some(Duration::from_millis(250))).map_err(|error| error.to_string())?;
                    }
                    Happened::Drawn(rectangles) => {
                        for rectangle in &rectangles {
                            let of_a_kind = drawn.entry((rectangle.bits, rectangle.compressed)).or_default();
                            of_a_kind.0 += 1;
                            of_a_kind.1 += rectangle.carries.len();
                            if let Some(picture) = &mut picture {
                                picture.draw(rectangle).map_err(|error| error.to_string())?;
                            }
                        }
                        last_drawn = Some(Instant::now());
                    }
                }
            }
        }
        let now = Instant::now();
        let quiet = last_drawn.is_some_and(|last| now.duration_since(last) >= QUIET_FOR);
        if opened.is_some_and(|opened| quiet || now.duration_since(opened) >= DRAWS_AT_MOST) {
            if !quiet {
                println!("   It still drew after {} seconds, which is where this stops.", DRAWS_AT_MOST.as_secs());
            }
            break;
        }
        let mut bytes = [0u8; 16_384];
        match secured.read(&mut bytes) {
            Ok(0) => return Err("It closed the connection.".to_string()),
            Ok(count) => frames.push(&bytes[..count]),
            Err(error) if matches!(error.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {
                if opened.is_none() {
                    return Err(format!("It gave no answer within {} seconds.", WITHIN.as_secs()));
                }
            }
            Err(error) => return Err(format!("The connection broke: {error}.")),
        }
    }
    for ((bits, compressed), (count, bytes)) in &drawn {
        println!(
            "   {count} rectangles at {bits} bits a pixel, {}, {bytes} bytes in all.",
            if *compressed { "compressed" } else { "as they are" }
        );
    }
    if drawn.is_empty() {
        println!("   It drew nothing in that time.");
    }
    // leaving is a courtesy: the machine notices a closed connection as well
    let _ = secured.write_all(&open.leave());
    let Some(picture) = picture else { return Ok(()) };
    println!("The desktop's picture");
    let mut sum = Sha256::new();
    picture.pixels.as_chunks::<4>().0.iter().for_each(|pixel| sum.update(&pixel[..3]));
    let sum: String = sum.finalize().iter().map(|byte| format!("{byte:02x}")).collect();
    println!("   {} by {} pixels. Their SHA-256: {sum}", picture.width, picture.height);
    match save {
        Some(file) => {
            std::fs::write(file, bmp(&picture)).map_err(|error| format!("The picture couldn't be written to {file}: {error}."))?;
            println!("   It is in {file}.");
        }
        None => println!("   To keep it, name a file: --save <file>."),
    }
    Ok(())
}

/// A picture as a BMP file: 24 bits a pixel, and rows from the bottom up, as that format
/// has them.
fn bmp(picture: &Picture) -> Vec<u8> {
    let row = (picture.width * 3).next_multiple_of(4);
    let size = 54 + row * picture.height;
    let mut file = Vec::with_capacity(size);
    file.extend_from_slice(b"BM");
    file.extend_from_slice(&(size as u32).to_le_bytes());
    file.extend_from_slice(&[0; 4]);
    file.extend_from_slice(&54u32.to_le_bytes());
    file.extend_from_slice(&40u32.to_le_bytes());
    file.extend_from_slice(&(picture.width as u32).to_le_bytes());
    file.extend_from_slice(&(picture.height as u32).to_le_bytes());
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&24u16.to_le_bytes());
    file.extend_from_slice(&[0; 24]);
    for line in picture.pixels.chunks_exact(picture.width * 4).rev() {
        for pixel in line.as_chunks::<4>().0 {
            file.extend_from_slice(&pixel[..3]);
        }
        file.resize(file.len() + row - picture.width * 3, 0);
    }
    file
}

/// One whole message of the sign-in, read from the secured connection.
fn message(secured: &mut impl Read) -> Result<Vec<u8>, String> {
    let mut held = Vec::new();
    loop {
        match der::whole(&held).map_err(|error| error.to_string())? {
            Some(length) if held.len() >= length => {
                held.truncate(length);
                return Ok(held);
            }
            _ => {}
        }
        let mut bytes = [0u8; 4096];
        match secured.read(&mut bytes) {
            Ok(0) => return Err("It closed the connection in the middle of the sign-in.".to_string()),
            Ok(count) => held.extend_from_slice(&bytes[..count]),
            Err(_) => return Err(format!("It gave no answer to the sign-in within {} seconds.", WITHIN.as_secs())),
        }
    }
}

/// Says who a machine says it is.
fn who_it_is(challenge: &Challenge) {
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
}

/// The time now as Windows counts it: in ten-millionths of a second since 1601.
fn clock() -> u64 {
    let since_1970 = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    (since_1970.as_secs() + 11_644_473_600) * 10_000_000 + u64::from(since_1970.subsec_nanos() / 100)
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
