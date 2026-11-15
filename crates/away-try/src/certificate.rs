//! Asks an organisation's own authority for a certificate, the way Away's issuer does,
//! and says what happened at each step.

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;

use away_issuer::authority::Authority;
use away_part::base64;
use away_part::client::Client;
use away_rdp::identity::Identity;
use away_winauth::certificate::Certificate;
use away_winauth::kerberos::pkinit::Held;
use away_winauth::request::{self, Key};

use crate::kerberos::{now, read};

pub const USAGE: &str = "\
away-try certificate <pages> --kerberos <domain controller> --realm <realm> --as <account> --password-file <file> --kind <kind> --save <file> --save-key <file> [--trusts <file>]
away-try certificate <pages> … --for <account> --agent <file> --agent-key <file>

  Asks an organisation's own authority for a certificate, the way Away's issuer does:
  over the pages the authority publishes for that, such as
  https://authority.corp.example/certsrv/, signed in with Kerberos as the account the
  organisation gave Away for it. The account is named without its realm, and its
  password is read from a file. The kind is the name the authority gives the kind of
  certificate that is asked for.

  A key of 2048 bits is made here. The certificate is written to the one file and the
  key to the other, each as text between two lines of dashes; a file that is there
  already is not written over. With --trusts, the authority's pages may show a
  certificate of an authority that file holds.

  Without --for the certificate is the account's own: an agent's, where the kind is
  one that allows asking for others. With --for it is asked for in another account's
  name, written as the domain knows it, CORP\\alex.urban: the request is signed with
  the agent's certificate and its key, and the authority decides whether this agent
  may ask for that account.
";

/// What the program was asked to do.
#[derive(Default)]
struct Asked {
    pages: String,
    controller: Option<String>,
    realm: Option<String>,
    account: Option<String>,
    password_file: Option<String>,
    kind: Option<String>,
    save: Option<String>,
    save_key: Option<String>,
    trusts: Option<String>,
    for_account: Option<String>,
    agent: Option<String>,
    agent_key: Option<String>,
}

fn asked(words: &[String]) -> Option<Asked> {
    let [pages, rest @ ..] = words else { return None };
    if pages.starts_with('-') {
        return None;
    }
    let mut asked = Asked { pages: pages.clone(), ..Asked::default() };
    let mut rest = rest;
    while let [option, value, after @ ..] = rest {
        let into = match option.as_str() {
            "--kerberos" => &mut asked.controller,
            "--realm" => &mut asked.realm,
            "--as" => &mut asked.account,
            "--password-file" => &mut asked.password_file,
            "--kind" => &mut asked.kind,
            "--save" => &mut asked.save,
            "--save-key" => &mut asked.save_key,
            "--trusts" => &mut asked.trusts,
            "--for" => &mut asked.for_account,
            "--agent" => &mut asked.agent,
            "--agent-key" => &mut asked.agent_key,
            _ => return None,
        };
        if into.replace(value.clone()).is_some() {
            return None;
        }
        rest = after;
    }
    let needed = [&asked.controller, &asked.realm, &asked.account, &asked.password_file, &asked.kind, &asked.save, &asked.save_key];
    // in another's name with an agent and its key, or in nobody's and with neither
    let for_another = [&asked.for_account, &asked.agent, &asked.agent_key];
    let whole = for_another.iter().all(|one| one.is_some()) || for_another.iter().all(|one| one.is_none());
    (rest.is_empty() && needed.iter().all(|one| one.is_some()) && whole).then_some(asked)
}

/// Bytes as text between two lines of dashes.
fn between_dashes(what: &str, bytes: &[u8]) -> String {
    let letters = base64::ordinary(bytes);
    let lines: Vec<&str> = letters.as_bytes().chunks(64).map(|line| std::str::from_utf8(line).unwrap_or_default()).collect();
    format!("-----BEGIN {what}-----\n{}\n-----END {what}-----\n", lines.join("\n"))
}

/// Writes a file that is not there yet, for its owner alone.
fn write(file: &str, what: &str, text: &str) -> Result<(), String> {
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(file)
        .map_err(|error| format!("The {what}'s file can't be written: {error}."))?;
    out.write_all(text.as_bytes()).map_err(|error| format!("The {what}'s file can't be written: {error}."))
}

/// The steps of asking. Gives nothing where the words made no sense.
pub fn run(words: &[String]) -> Option<Result<(), String>> {
    let asked = asked(words)?;
    Some(tried(&asked))
}

fn tried(asked: &Asked) -> Result<(), String> {
    let given = |one: &Option<String>| one.clone().unwrap_or_default();
    let (realm, account, kind) = (given(&asked.realm), given(&asked.account), given(&asked.kind));
    let (save, save_key) = (given(&asked.save), given(&asked.save_key));
    for file in [&save, &save_key] {
        if std::path::Path::new(file).exists() {
            return Err(format!("{file} is there already, and is not written over."));
        }
    }
    let password =
        std::fs::read_to_string(given(&asked.password_file)).map_err(|error| format!("The password's file can't be read: {error}."))?;
    let password = away_secret::Secret::new(password.trim_end_matches(['\r', '\n']));
    let trusts = match &asked.trusts {
        Some(file) => {
            Some(std::fs::read(file).map_err(|error| format!("The file of the authority that is trusted can't be read: {error}."))?)
        }
        None => None,
    };

    println!("Making a key of {} bits, and the request for a certificate for it", request::BITS);
    let key = Key::new().map_err(|error| error.to_string())?;
    let name = asked.for_account.as_deref().map_or(account.as_str(), |account| account.rsplit('\\').next().unwrap_or(account));
    let mut request = request::asked(&key, name).map_err(|error| error.to_string())?;
    println!("   The request is {} bytes, and proves that the key is held here.", request.len());
    if let Some(for_account) = &asked.for_account {
        println!("Signing the request in the name of {for_account}, with the agent's certificate");
        let agent = Held::new(&read(&given(&asked.agent), "agent's certificate")?, &read(&given(&asked.agent_key), "agent's key")?)
            .map_err(|error| error.to_string())?;
        request = request::in_the_name_of(&request, for_account, &agent).map_err(|error| error.to_string())?;
        println!("   Signed, {} bytes with the agent's certificate.", request.len());
    }

    let client = Client::new(trusts.as_deref())?;
    let authority = Authority::new(client, &asked.pages, &[given(&asked.controller)], &realm, &account, password)?;
    println!("Asking {} for a certificate of the kind {kind}, signed in as {account}@{realm}", asked.pages);
    let runs = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|error| error.to_string())?;
    let certificate = runs.block_on(authority.ask(&request, &kind)).map_err(|why| why.to_string())?;

    let read = Certificate::read(&certificate).map_err(|error| error.to_string())?;
    let identity = Identity::read(&certificate).map_err(|error| error.to_string())?;
    let whose = read.account.clone().or(identity.name.clone()).unwrap_or_else(|| "no name".to_string());
    match &identity.issued_by {
        Some(issuer) => println!("   It issued a certificate for {whose}, as {issuer}."),
        None => println!("   It issued a certificate for {whose}."),
    }
    println!("   Valid from {} until {}.", identity.valid_from, identity.valid_until);
    let left = read.until.saturating_sub(now().0);
    match left {
        0 => println!("   It has run out already."),
        seconds if seconds < 7200 => println!("   It is good for {} minutes and {} seconds more.", seconds / 60, seconds % 60),
        seconds => println!("   It is good for {} days more.", seconds / 86400),
    }
    if read.is_for(away_winauth::certificate::FOR_SIGNING_IN) {
        println!("   It is one for signing in.");
    }

    write(&save, "certificate", &between_dashes("CERTIFICATE", &certificate))?;
    write(&save_key, "key", &between_dashes("PRIVATE KEY", &key.written().map_err(|error| error.to_string())?))?;
    println!("The certificate is in {save}, and its key in {save_key}.");
    Ok(())
}
