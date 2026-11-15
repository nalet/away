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

mod certificate;
mod kerberos;

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use away_rdp::connection::{Connection, Happened};
use away_rdp::der;
use away_rdp::gcc::Client;
use away_rdp::identity::Identity;
use away_rdp::licence;
use away_rdp::negotiation::{self, Security};
use away_rdp::ntlm::{self, Account, Challenge, Fresh};
use away_rdp::picture::Picture;
use away_rdp::secret::Secret;
use away_rdp::sign_in::{self, Message, New, Next, SignIn};
use away_rdp::tpkt::Frames;
use away_rdp::window::Change;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, ClientConnection, DigitallySignedStruct, SignatureScheme};
use sha2::{Digest, Sha256};

const USAGE: &str = "\
away-try rdp <address> [--as <account> --password-file <file> [--save <file>]] [--collection <name>] [--program <name>]
                       [--kerberos <domain controller> [--realm <realm>]]
                       [--certificate <file> --key <file> [--authority <file>]]

  Reaches a machine over RDP with Away's own code and says what it shows, step by step.
  The address is a name or a number, with a port after a colon where it is not 3389.

  With an account it signs in and opens the desktop. The account is written as people
  write it: CORP\\alex.urban, alex.urban@corp.example.com, or a name alone. Its
  password is read from a file, so that it is on no command line.

  With --save it writes the desktop's picture to a file, as a BMP, once the machine
  has stopped drawing.

  With --collection the address is a farm's broker, which is asked for that collection
  and says which machine serves it.

  With --kerberos the account signs in before the desktop with a ticket: the domain
  controller is asked for one for this machine, with the account's password, and the
  machine is shown the ticket in place of a proof of the password. The realm is the
  domain's name in capitals; it is taken from an account written as name@domain.

  With --certificate and --key the machine signs the account in with a smart card, in
  place of a name and a password. The card exists in this program's memory alone, and
  holds that certificate and its key. The machine reads it over the connection, gives
  it a PIN that was made for this one connection, and has it sign what proves the
  person to the domain. This needs --kerberos. With --authority there is no password
  at all: the ticket is asked for with the certificate too, and the domain controller
  has to prove itself with a certificate of that authority. Without it the ticket is
  asked for with the password, and the card is still what the machine signs in with.

  With --program it opens no desktop: it asks the machine to start that program, and
  says whether it started and which windows it opens. A program that a farm publishes
  is asked for by the name it is published under, with || before it: '||ledger'.

  It says the SHA-256 of the picture's pixels: three bytes each (blue, green, red), row
  by row from the top. A screenshot taken on the machine at that moment has the same.
  Where a machine offers its desktop at 16 bits a pixel, as a Windows Server does unless
  it is set otherwise, the screenshot has the same once its colours are cut to those bits.
";

/// How long a machine gets to answer, at each step.
const WITHIN: Duration = Duration::from_secs(10);
/// How long a machine has to draw nothing for its picture to count as finished.
const QUIET_FOR: Duration = Duration::from_secs(3);
/// How long what a machine draws is taken at most.
const DRAWS_AT_MOST: Duration = Duration::from_secs(45);

/// What this program is known by to a machine that hands out licences: the same wherever
/// it runs, so that all its runs are one device to whoever counts them.
fn known_by() -> [u8; 16] {
    Sha256::digest(b"away-try")[..16].try_into().expect("16 bytes")
}

/// What the program was asked to do.
struct Asked {
    address: String,
    account: Option<String>,
    password_file: Option<String>,
    save: Option<String>,
    collection: Option<String>,
    program: Option<String>,
    kerberos: Option<String>,
    realm: Option<String>,
    certificate: Option<String>,
    key: Option<String>,
    authority: Option<String>,
}

/// What the smart card holds that the machine signs the account in with: a certificate
/// and its key, and the authority a domain controller proves itself with where the
/// ticket is asked for with the certificate too.
struct OnTheCard {
    certificate: Vec<u8>,
    key: Vec<u8>,
    authority: Option<Vec<u8>>,
}

/// The name of the reader the card is in, and of the machine's own software that reads
/// a card of this kind.
const READER: &str = "Away 0";
const PROVIDER: &str = "Microsoft Base Smart Card Crypto Provider";

/// Reads the words the program was started with. Gives nothing where they make no sense.
fn asked(words: &[String]) -> Option<Asked> {
    let [kind, address, rest @ ..] = words else { return None };
    let mut rest = rest;
    if kind != "rdp" || address.starts_with('-') {
        return None;
    }
    let mut asked = Asked {
        address: address.clone(),
        account: None,
        password_file: None,
        save: None,
        collection: None,
        program: None,
        kerberos: None,
        realm: None,
        certificate: None,
        key: None,
        authority: None,
    };
    while let [option, value, after @ ..] = rest {
        let into = match option.as_str() {
            "--as" => &mut asked.account,
            "--password-file" => &mut asked.password_file,
            "--save" => &mut asked.save,
            "--collection" => &mut asked.collection,
            "--program" => &mut asked.program,
            "--kerberos" => &mut asked.kerberos,
            "--realm" => &mut asked.realm,
            "--certificate" => &mut asked.certificate,
            "--key" => &mut asked.key,
            "--authority" => &mut asked.authority,
            _ => return None,
        };
        if into.replace(value.clone()).is_some() {
            return None;
        }
        rest = after;
    }
    // a card is a certificate with its key, and goes with a ticket; an authority is that of
    // a card, and a realm that of a ticket
    let with_a_card = asked.certificate.is_some();
    let a_card_is_whole = with_a_card == asked.key.is_some() && (!with_a_card || asked.kerberos.is_some());
    let each_has_its_own = (asked.authority.is_none() || with_a_card) && (asked.realm.is_none() || asked.kerberos.is_some());
    // an account proves itself with a password, or with the card's certificate where an
    // authority is named, and never with both
    let proves_itself = match (&asked.account, &asked.password_file) {
        (Some(_), Some(_)) => asked.authority.is_none(),
        (Some(_), None) => asked.authority.is_some(),
        (None, password) => password.is_none(),
    };
    // a picture, a program and a ticket need an account
    let needs_an_account = asked.save.is_some() || asked.program.is_some() || asked.kerberos.is_some();
    (rest.is_empty() && a_card_is_whole && each_has_its_own && proves_itself && (!needs_an_account || asked.account.is_some()))
        .then_some(asked)
}

fn main() -> ExitCode {
    let words: Vec<String> = std::env::args().skip(1).collect();
    if let [word] = words.as_slice() {
        if matches!(word.as_str(), "help" | "--help" | "-h") {
            print!("{USAGE}\n{}\n{}", kerberos::USAGE, certificate::USAGE);
            return ExitCode::SUCCESS;
        }
    }
    // an authority is asked for a certificate
    if words.first().is_some_and(|kind| kind == "certificate") {
        return match certificate::run(&words[1..]) {
            Some(Ok(())) => ExitCode::SUCCESS,
            Some(Err(why)) => {
                println!("   {why}");
                ExitCode::FAILURE
            }
            None => {
                eprint!("{}", certificate::USAGE);
                ExitCode::from(2)
            }
        };
    }
    // a domain controller is tried with Kerberos, a machine with RDP
    if words.first().is_some_and(|kind| kind == "kerberos") {
        return match kerberos::run(&words[1..]) {
            Some(Ok(())) => ExitCode::SUCCESS,
            Some(Err(why)) => {
                println!("   {why}");
                ExitCode::FAILURE
            }
            None => {
                eprint!("{}", kerberos::USAGE);
                ExitCode::from(2)
            }
        };
    }
    let Some(asked) = asked(&words) else {
        eprint!("{USAGE}\n{}\n{}", kerberos::USAGE, certificate::USAGE);
        return ExitCode::from(2);
    };
    let tried = tried(&asked);
    match tried {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            println!("   {why}");
            ExitCode::FAILURE
        }
    }
}

/// Reads what the program needs from its files, and goes through the connection.
fn tried(asked: &Asked) -> Result<(), String> {
    let Some(name) = &asked.account else { return rdp(asked, None, None) };
    // an account whose card gets the ticket has no password
    let password = match &asked.password_file {
        Some(file) => std::fs::read_to_string(file).map_err(|error| format!("The password's file can't be read: {error}."))?,
        None => String::new(),
    };
    let account = Account::named(name, Secret::new(password.trim_end_matches(['\r', '\n'])));
    let on_the_card = match (&asked.certificate, &asked.key) {
        (Some(certificate), Some(key)) => Some(OnTheCard {
            certificate: kerberos::read(certificate, "certificate")?,
            key: kerberos::read(key, "key")?,
            authority: asked.authority.as_ref().map(|authority| kerberos::read(authority, "authority")).transpose()?,
        }),
        _ => None,
    };
    rdp(asked, Some(account), on_the_card)
}

/// The steps of a connection over RDP, as far as Away's code goes.
fn rdp(asked: &Asked, account: Option<Account>, on_the_card: Option<OnTheCard>) -> Result<(), String> {
    let (address, save, collection, program) =
        (asked.address.as_str(), asked.save.as_deref(), asked.collection.as_deref(), asked.program.as_deref());
    let with_a_ticket = asked.kerberos.as_deref().map(|controller| (controller, asked.realm.as_deref()));
    // the reader with the card in it, where the machine signs the account in with one,
    // and where what the machine does with the card is told
    let mut reader: Option<away_rdp::smartcard::Reader> = None;
    let mut told: Option<away_winauth::card::Told> = None;
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
    // a farm's broker is told which collection is wanted, in the words a farm knows
    let wanted = collection.map(|collection| format!("tsv://MS Terminal Services Plugin.1.{collection}"));
    let request = negotiation::request_with(wanted.as_deref().map(str::as_bytes)).map_err(|error| error.to_string())?;
    connection.write_all(&request).map_err(|error| format!("The request couldn't be sent: {error}."))?;
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
        if let Some((controller, realm)) = with_a_ticket {
            // who the ticket is for: the name without its domain, and the realm, which is
            // the domain's name in capitals: said, or the one the account is written with
            let (name, written_with) = match account.name.split_once('@') {
                Some((name, domain)) => (name, Some(domain)),
                None => (account.name.as_str(), Some(account.domain.as_str()).filter(|domain| domain.contains('.'))),
            };
            let realm = match (realm, written_with) {
                (Some(realm), _) => realm.to_string(),
                (None, Some(domain)) => domain.to_uppercase(),
                (None, None) => return Err("Name the realm, the domain's name in capitals: --realm <realm>.".to_string()),
            };
            let service = format!("TERMSRV/{host}");
            let ticket = match on_the_card.as_ref().and_then(|card| card.authority.as_deref().map(|authority| (card, authority))) {
                Some((card, authority)) => {
                    kerberos::ticket_with_a_certificate(controller, &realm, name, (&card.certificate, &card.key, authority), &service)?
                }
                None => kerberos::ticket_with_a_password(controller, &realm, name, account.password.expose(), &service)?,
            };
            println!("Signing in, before the desktop, with the ticket");
            let new = away_winauth::sign_in::New {
                for_the_ticket: away_winauth::kerberos::gss::Fresh {
                    key: kerberos::random()?,
                    begins_with: kerberos::random()?,
                    counts_from: u32::from_be_bytes(kerberos::random()?) & 0x3fff_ffff,
                },
                number: kerberos::random()?,
                begin_with: [kerberos::random()?, kerberos::random()?],
            };
            let (mut signing_in, first) = match &on_the_card {
                Some(holds) => {
                    // a card for this one connection: a PIN nobody is told, a name for what
                    // holds its key, and what a machine knows it by, each made new
                    let hex = |bytes: &[u8]| bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
                    let pin = hex(&kerberos::random::<16>()?);
                    let container = format!("away-{}", hex(&kerberos::random::<12>()?));
                    let (card, tells) =
                        away_winauth::card::Card::new(&holds.certificate, &holds.key, Secret::new(&pin), &container, kerberos::random()?)
                            .map_err(|error| error.to_string())?;
                    reader = Some(away_rdp::smartcard::Reader::new(READER, Box::new(card)));
                    told = Some(tells);
                    let on_a_card = away_winauth::sign_in::OnACard {
                        pin: Secret::new(&pin),
                        name: name.to_string(),
                        domain: realm.clone(),
                        card: String::new(),
                        reader: READER.to_string(),
                        container,
                        provider: PROVIDER.to_string(),
                    };
                    away_winauth::sign_in::SignIn::begin_with_a_card(&ticket, &identity.public_key, &on_a_card, kerberos::now(), new)
                }
                None => away_winauth::sign_in::SignIn::begin(&ticket, &identity.public_key, &account, kerberos::now(), new),
            };
            secured.write_all(&first).map_err(|error| format!("The sign-in couldn't be begun: {error}."))?;
            loop {
                match signing_in.receive(&message(&mut secured)?).map_err(|error| error.to_string())? {
                    Next::Send(bytes) => secured.write_all(&bytes).map_err(|error| format!("The sign-in couldn't go on: {error}."))?,
                    Next::SendLast(bytes) => {
                        secured.write_all(&bytes).map_err(|error| format!("The sign-in couldn't be ended: {error}."))?;
                        break;
                    }
                }
            }
            println!("   It answered the ticket, and proved that it holds the certificate it showed.");
            match reader {
                Some(_) => {
                    println!("   A smart card was handed over in place of a password, sealed: its PIN, and where the machine finds it.")
                }
                None => println!("   The name and the password were handed over, sealed."),
            }
        } else {
            println!("Signing in, before the desktop, as {}", written(&account));
            let mut random = [0u8; 56];
            getrandom::fill(&mut random).map_err(|error| format!("The system gave no random bytes: {error}."))?;
            let new = New {
                for_the_proof: Fresh {
                    challenge: random[..8].try_into().expect("8 bytes"),
                    key: random[8..24].try_into().expect("16 bytes"),
                },
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
        }

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
    let client = Client {
        width: 1280,
        height: 800,
        scale: 100,
        security: answer.security.number(),
        name: "away".to_string(),
        layout: away_rdp::gcc::LAYOUT_US,
        sent_on_to: None,
        program: program.map(|run| away_rdp::rail::Program { run: run.to_string(), arguments: String::new() }),
    };
    let mut random = [0u8; 80];
    getrandom::fill(&mut random).map_err(|error| format!("The system gave no random bytes: {error}."))?;
    let for_a_licence = licence::New {
        random: random[..32].try_into().expect("32 bytes"),
        secret: random[32..].try_into().expect("48 bytes"),
        known_by: known_by(),
    };
    let with_a_reader = reader.is_some();
    let (mut open, first) = Connection::begin(client, &domain, &name, password, for_a_licence, reader);
    let mut asked_the_reader = 0usize;
    let mut ticked = Instant::now();
    secured.write_all(&first).map_err(|error| format!("It couldn't be sent: {error}."))?;
    let mut frames = Frames::new();
    let mut picture: Option<Picture> = None;
    let mut opened: Option<Instant> = None;
    let mut last_drawn: Option<Instant> = None;
    let mut drawn: std::collections::BTreeMap<(u16, bool), (usize, usize)> = std::collections::BTreeMap::new();
    let mut asked_to_take_a_licence = false;
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
                        match asked_to_take_a_licence {
                            false => println!("   It takes the client as it is, without a licence."),
                            true => println!("   It took the client's answer, handed no licence over, and goes on."),
                        }
                        println!("Agreeing on what each side can do");
                    }
                    Happened::AsksToTakeALicence { company, product } => {
                        println!("   It hands out licences, and asks the client to take one: for {product}, of {company}.");
                        asked_to_take_a_licence = true;
                    }
                    Happened::LicenceTaken(taken) => {
                        println!("   It handed over a licence of {} bytes, from {}.", taken.licence.len(), taken.scope);
                        println!("   The licence is not kept: the next connection takes one again.");
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
                    Happened::SentOn(sent) => {
                        println!("   It is a farm's broker, and sends the client on:");
                        println!(
                            "   to the machine {}, at {}, as the session {}.",
                            sent.name.as_deref().unwrap_or("without a name"),
                            sent.address.as_deref().unwrap_or("no address it names"),
                            sent.session
                        );
                        match &sent.token {
                            Some(token) => println!("   The client is to say first there: {}", String::from_utf8_lossy(token).trim_end()),
                            None => println!("   It gives nothing to say first there."),
                        }
                        println!("   This program goes no further: it reaches one machine.");
                        return Ok(());
                    }
                    Happened::AskedForTheProgram => {
                        println!("   It is ready for programs, and was asked for {}.", program.unwrap_or("none"));
                    }
                    Happened::Started => println!("   The program started."),
                    Happened::NotStarted { result, raw } => {
                        return Err(format!("The program did not start: {} (its own code {raw:#010x}).", away_rdp::rail::why(result)));
                    }
                    Happened::Windows(changes) => {
                        for change in changes {
                            match change {
                                Change::Window(window) => {
                                    let what = if window.is_new { "A new window" } else { "A window changed" };
                                    let title = window.title.map(|title| format!(", “{title}”")).unwrap_or_default();
                                    let at = window.at.map(|(left, top)| format!(", at {left}, {top}")).unwrap_or_default();
                                    let size = window.size.map(|(wide, high)| format!(", {wide} by {high}")).unwrap_or_default();
                                    println!("   {what}: number {}{title}{at}{size}.", window.id);
                                }
                                Change::Gone(window) => println!("   A window is gone: number {window}."),
                                Change::InFront(window) => println!("   In front is the window number {window}."),
                                Change::Order(_) => {}
                            }
                        }
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
                    Happened::Reader { taken: true } => println!("   It took this client's smart card reader."),
                    Happened::Reader { taken: false } => println!("   It did not take this client's smart card reader."),
                    // while the machine is busy with the card it draws little: that is no quiet yet
                    Happened::AskedTheReader(_) => {
                        asked_the_reader += 1;
                        if last_drawn.is_some() {
                            last_drawn = Some(Instant::now());
                        }
                    }
                }
            }
        }
        let now = Instant::now();
        // what the machine asked the reader and waited for only so long is answered when the time is over
        let waited = open.elapsed(now.duration_since(ticked).as_millis() as u64);
        ticked = now;
        if !waited.is_empty() {
            secured.write_all(&waited).map_err(|error| format!("It couldn't be sent: {error}."))?;
        }
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
    if with_a_reader {
        println!("What it did with the smart card");
        println!("   It asked the reader {asked_the_reader} times.");
        let did: Vec<away_winauth::card::Did> = told.and_then(|told| told.lock().ok().map(|told| told.clone())).unwrap_or_default();
        let mut said = Vec::new();
        for what in &did {
            if said.contains(what) {
                continue;
            }
            said.push(*what);
            let times = match did.iter().filter(|other| *other == what).count() {
                1 => String::new(),
                times => format!(" ({times} times)"),
            };
            use away_winauth::card::Did;
            match what {
                Did::Found => println!("   It found the card to be of the kind it reads{times}."),
                Did::ReadTheCertificate => println!("   It read the certificate{times}."),
                Did::GaveThePin { right: true } => println!("   It gave the card its PIN{times}."),
                Did::GaveThePin { right: false } => println!("   It gave the card a PIN that is not the card's{times}."),
                Did::Signed => println!("   It had the card sign what proves the person to the domain{times}."),
                Did::NotDone(command) => {
                    println!("   It asked the card for something the card does not do: command {command:#04x}{times}.")
                }
            }
        }
        if did.is_empty() {
            println!("   It gave the card no command.");
        }
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
