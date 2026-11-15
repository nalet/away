//! Tries a domain controller with Away's own Kerberos, and says what happened at each
//! step: a first ticket for an account, with its password or with a certificate, and
//! with that ticket one for a service.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use away_winauth::kerberos::exchange::{framed, unframed, First, Later, Ticket, WithAPassword};
use away_winauth::kerberos::messages::{Name, NameKind};
use away_winauth::kerberos::pkinit::{Fresh, Held, Trusts};

/// How long a domain controller gets to answer.
const WITHIN: Duration = Duration::from_secs(10);

pub const USAGE: &str = "\
away-try kerberos <domain controller> --realm <realm> --as <account> --password-file <file> [--for <service>]
away-try kerberos <domain controller> --realm <realm> --as <account> --certificate <file> --key <file> --authority <file> [--for <service>]

  Asks a domain controller for a first ticket with Away's own Kerberos, and says what it
  answers. The realm is the domain's name in capitals, and the account is named without
  it. With a password, the password is read from a file. With a certificate, the
  account proves itself with the certificate's key and nothing else, and the domain
  controller has to prove itself with a certificate of the authority that is named.
  The files hold a certificate or a key as bytes, or as text between two lines of
  dashes.

  With --for it then asks for a ticket for a service, written as its kind and its
  machine: TERMSRV/fin-app-02.corp.example is a machine's desktop.
";

/// What the program was asked to do.
#[derive(Default)]
struct Asked {
    controller: String,
    realm: Option<String>,
    account: Option<String>,
    password_file: Option<String>,
    certificate: Option<String>,
    key: Option<String>,
    authority: Option<String>,
    service: Option<String>,
}

fn asked(words: &[String]) -> Option<Asked> {
    let [controller, rest @ ..] = words else { return None };
    if controller.starts_with('-') {
        return None;
    }
    let mut asked = Asked { controller: controller.clone(), ..Asked::default() };
    let mut rest = rest;
    while let [option, value, after @ ..] = rest {
        let into = match option.as_str() {
            "--realm" => &mut asked.realm,
            "--as" => &mut asked.account,
            "--password-file" => &mut asked.password_file,
            "--certificate" => &mut asked.certificate,
            "--key" => &mut asked.key,
            "--authority" => &mut asked.authority,
            "--for" => &mut asked.service,
            _ => return None,
        };
        if into.replace(value.clone()).is_some() {
            return None;
        }
        rest = after;
    }
    // a password, or a certificate with its key and the authority, and never both
    let with_a_certificate = asked.certificate.is_some() && asked.key.is_some() && asked.authority.is_some();
    let none_of_it = asked.certificate.is_none() && asked.key.is_none() && asked.authority.is_none();
    let one_way = (asked.password_file.is_some() && none_of_it) || (asked.password_file.is_none() && with_a_certificate);
    (rest.is_empty() && asked.realm.is_some() && asked.account.is_some() && one_way).then_some(asked)
}

/// What a file holds: bytes as they are, or what is written as text between two lines
/// of dashes.
fn read(file: &str, what: &str) -> Result<Vec<u8>, String> {
    let held = std::fs::read(file).map_err(|error| format!("The {what}'s file can't be read: {error}."))?;
    let Some(text) = std::str::from_utf8(&held).ok().filter(|text| text.trim_start().starts_with("-----BEGIN")) else {
        return Ok(held);
    };
    let written: String = text.lines().filter(|line| !line.starts_with("-----")).flat_map(|line| line.trim().chars()).collect();
    let mut bytes = Vec::with_capacity(written.len() * 3 / 4);
    let (mut bits, mut count) = (0u32, 0);
    for letter in written.chars().filter(|letter| *letter != '=') {
        let value = match letter {
            'A'..='Z' => letter as u32 - 'A' as u32,
            'a'..='z' => letter as u32 - 'a' as u32 + 26,
            '0'..='9' => letter as u32 - '0' as u32 + 52,
            '+' => 62,
            '/' => 63,
            _ => return Err(format!("The {what}'s file holds text that is no {what}.")),
        };
        bits = (bits << 6) | value;
        count += 6;
        if count >= 8 {
            count -= 8;
            bytes.push((bits >> count) as u8);
        }
    }
    Ok(bytes)
}

fn now() -> (u64, u32) {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    (now.as_secs(), now.subsec_micros())
}

fn random<const N: usize>() -> Result<[u8; N], String> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).map_err(|error| format!("The system gave no random bytes: {error}."))?;
    Ok(bytes)
}

/// Sends one request, and gives what came back.
fn ask(host: &str, port: u16, request: &[u8]) -> Result<Vec<u8>, String> {
    // a name may lead to several addresses, and a domain controller may answer on one of them alone
    let mut connection = Err(format!("The name {host} leads to no address."));
    for to in (host, port).to_socket_addrs().map_err(|_| format!("The name {host} is not known."))? {
        connection = TcpStream::connect_timeout(&to, WITHIN).map_err(|error| match error.kind() {
            std::io::ErrorKind::ConnectionRefused => "It takes no connection on this port.".to_string(),
            std::io::ErrorKind::TimedOut => format!("It gave no answer within {} seconds.", WITHIN.as_secs()),
            _ => format!("It could not be reached: {error}."),
        });
        if connection.is_ok() {
            break;
        }
    }
    let mut connection = connection?;
    connection.set_read_timeout(Some(WITHIN)).map_err(|error| error.to_string())?;
    connection.write_all(&framed(request)).map_err(|error| format!("The request couldn't be sent: {error}."))?;
    let mut arrived = Vec::new();
    loop {
        if let Some(reply) = unframed(&arrived).map_err(|error| error.to_string())? {
            return Ok(reply.to_vec());
        }
        let mut bytes = [0u8; 4096];
        match connection.read(&mut bytes) {
            Ok(0) => return Err("It closed the connection without an answer.".to_string()),
            Ok(count) => arrived.extend_from_slice(&bytes[..count]),
            Err(_) => return Err(format!("It gave no answer within {} seconds.", WITHIN.as_secs())),
        }
    }
}

fn says(ticket: &Ticket) {
    let good_for = ticket.until.saturating_sub(now().0);
    println!("   It gave a ticket for {}, to {}@{}.", ticket.service.parts.join("/"), ticket.account.parts.join("/"), ticket.realm);
    println!("   It is sealed with {:?}, and good for {} minutes more.", ticket.key.kind(), good_for / 60);
}

/// The steps of the exchange. Gives the exit code where the words made no sense.
pub fn run(words: &[String]) -> Option<Result<(), String>> {
    let asked = asked(words)?;
    Some(tried(&asked))
}

fn tried(asked: &Asked) -> Result<(), String> {
    let (host, port) = match asked.controller.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => (host, port.parse::<u16>().map_err(|_| format!("{port} is no port."))?),
        _ => (asked.controller.as_str(), 88),
    };
    let (realm, account) = (asked.realm.as_deref().unwrap_or_default(), asked.account.as_deref().unwrap_or_default());
    let nonce = u32::from_be_bytes(random()?) & 0x7fff_ffff;

    let first = match &asked.password_file {
        Some(file) => {
            let password = std::fs::read_to_string(file).map_err(|error| format!("The password's file can't be read: {error}."))?;
            println!("Asking {host}, on port {port}, how the key of {account}@{realm} is made");
            let (mut asking, request) = WithAPassword::begin(account, realm, now(), nonce);
            let reply = ask(host, port, &request)?;
            let proof =
                asking.prove(&reply, password.trim_end_matches(['\r', '\n']), now(), random()?).map_err(|error| error.to_string())?;
            println!("   It asks for a proof, and says how the key is made from the password.");
            println!("Asking for a first ticket, with the time sealed with that key");
            asking.receive(&ask(host, port, &proof)?).map_err(|error| error.to_string())?
        }
        None => {
            let held = Held::new(
                &read(asked.certificate.as_deref().unwrap_or_default(), "certificate")?,
                &read(asked.key.as_deref().unwrap_or_default(), "key")?,
            )
            .map_err(|error| error.to_string())?;
            let authority = read(asked.authority.as_deref().unwrap_or_default(), "authority")?;
            println!("Asking {host}, on port {port}, for a first ticket for {account}@{realm}, with the certificate");
            let fresh = Fresh { secret: random()?, once: random()? };
            let (first, request) = First::begin(&held, account, realm, now(), nonce, fresh).map_err(|error| error.to_string())?;
            let reply = ask(host, port, &request)?;
            let ticket = first
                .receive(&reply, &Trusts { authorities: &[&authority], controller: host }, now().0)
                .map_err(|error| error.to_string())?;
            println!("   It proved itself with a certificate of the authority, for {host}.");
            ticket
        }
    };
    says(&first);

    if let Some(service) = &asked.service {
        println!("Asking for a ticket for {service}, with the first ticket");
        let parts: Vec<&str> = service.split('/').collect();
        let (later, request) = Later::begin(&first, Name::new(NameKind::Service, &parts), now(), nonce ^ 0x5555_5555, random()?);
        says(&later.receive(&ask(host, port, &request)?).map_err(|error| error.to_string())?);
    }
    Ok(())
}
