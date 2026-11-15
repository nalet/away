//! The try program's way with an organisation's authority: it asks pages that stand in
//! for an authority's, signed in with a ticket of a real Kerberos server where
//! `AWAY_TEST_KERBEROS` names one (`tests/machines/kerberos/real.sh` starts it).
//!
//! The pages open the ticket with the key of their service, as an authority's pages
//! do, read the request as an authority reads one, and issue with an authority that
//! exists for the tests alone.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use away_issuer::authority::stand_in::{between_dashes, Issued, Pages, With};
use away_winauth::certificate::{Certificate, FOR_SIGNING_IN};
use away_winauth::kerberos::crypto::{Key, Kind};
use away_winauth::request;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../away-winauth/tests/fixtures");
/// The account that asks, as the tests' Kerberos server knows it.
const PASSWORD: &str = "a password that exists for these tests alone";

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{FIXTURES}/{name}")).expect("a fixture")
}

/// Pages that stand in for an organisation's authority, as the tests' Kerberos server
/// knows them: the service `HTTP/localhost`, whose key is made from a password, and one
/// account that may ask. They run for as long as what is given back is kept.
fn pages() -> (tokio::runtime::Runtime, Pages) {
    let runs = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().expect("a runtime");
    let pages = runs.block_on(Pages::open(With {
        shows: (fixture("controller.der"), fixture("controller.key.der")),
        of_the_service: Key::from_password(
            Kind::Aes256,
            "the password of pages that exist for these tests alone",
            b"CORP.EXAMPLEHTTPlocalhost",
            4096,
        ),
        issues_with: (fixture("issuing.der"), fixture("issuing.key.der")),
        asked_as: "pat.typed".to_string(),
        domain: "CORP".to_string(),
        realm: "CORP.EXAMPLE".to_string(),
        for_signing_in: "CorpSignIn".to_string(),
        for_an_agent: "CorpAgent".to_string(),
        never_for: vec!["sam.other".to_string()],
        good_for: 300,
    }));
    (runs, pages)
}

fn server() -> Option<String> {
    let at = std::env::var("AWAY_TEST_KERBEROS").ok().filter(|at| !at.is_empty());
    if at.is_none() {
        assert!(std::env::var_os("CI").is_none(), "the pipeline's tests need a Kerberos server: AWAY_TEST_KERBEROS is not set");
        eprintln!("no Kerberos server to test against: AWAY_TEST_KERBEROS is not set");
    }
    at
}

/// A folder of the test's own, with the password's file and the trusted authority's in it.
fn folder(named: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("away-try-certificate-{}-{named}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).expect("a folder");
    std::fs::write(folder.join("password"), format!("{PASSWORD}\n")).expect("the password's file");
    std::fs::write(folder.join("authority.pem"), between_dashes("CERTIFICATE", &fixture("authority.der"))).expect("the authority's file");
    folder
}

/// Asks for a certificate of this kind, kept in the folder under this name, with what
/// else is said.
fn try_certificate(pages: &str, server: &str, folder: &Path, kind: &str, kept_as: &str, more: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_away-try"))
        .args(["certificate", pages, "--kerberos", server, "--realm", "CORP.EXAMPLE", "--as", "pat.typed", "--kind", kind])
        .arg("--password-file")
        .arg(folder.join("password"))
        .arg("--save")
        .arg(folder.join(format!("{kept_as}.pem")))
        .arg("--save-key")
        .arg(folder.join(format!("{kept_as}.key.pem")))
        .args(more)
        .output()
        .expect("the test program runs")
}

/// What a file holds between its two lines of dashes.
fn held(file: &Path) -> Vec<u8> {
    let text = std::fs::read_to_string(file).expect("the file");
    let letters: String = text.lines().filter(|line| !line.starts_with("-----")).collect();
    away_part::base64::read_for_addresses(&letters.replace('+', "-").replace('/', "_").replace('=', "")).expect("bytes between dashes")
}

#[test]
fn it_asks_for_an_agents_certificate_and_with_it_for_one_in_another_accounts_name() {
    let Some(server) = server() else { return };
    let (_runs, pages) = pages();
    let folder = folder("for-another");
    let trusts = folder.join("authority.pem").display().to_string();

    // the account's own certificate, where nobody else is named
    let tried = try_certificate(&pages.address, &server, &folder, "CorpAgent", "agent", &["--trusts", &trusts]);
    let said = String::from_utf8_lossy(&tried.stdout);
    assert!(tried.status.success(), "{said}");
    assert!(!said.contains("Signing the request in the name of"), "{said}");
    assert!(said.contains("   It issued a certificate for pat.typed@CORP.EXAMPLE, as corp-issuing."), "{said}");
    // an agent's certificate lasts thirty days from the moment it was issued, and signs nobody in
    assert!(said.contains("   It is good for 29 days more.") || said.contains("   It is good for 30 days more."), "{said}");
    assert!(!said.contains("It is one for signing in."), "{said}");

    // and with it one in a person's name
    let (agent, agents_key) = (folder.join("agent.pem").display().to_string(), folder.join("agent.key.pem").display().to_string());
    let more = ["--for", "CORP\\alex.urban", "--agent", &agent, "--agent-key", &agents_key, "--trusts", &trusts];
    let tried = try_certificate(&pages.address, &server, &folder, "CorpSignIn", "alex", &more);
    let said = String::from_utf8_lossy(&tried.stdout);
    assert!(tried.status.success(), "{said}");
    for line in [
        "Making a key of 2048 bits, and the request for a certificate for it",
        "Signing the request in the name of CORP\\alex.urban, with the agent's certificate",
        "for a certificate of the kind CorpSignIn, signed in as pat.typed@CORP.EXAMPLE",
        "   It issued a certificate for alex.urban@CORP.EXAMPLE, as corp-issuing.",
        "   It is one for signing in.",
    ] {
        assert!(said.contains(line), "{line}\n{said}");
    }

    // the pages were asked by the account alone, each time with a proof bound to their certificate
    let seen = pages.seen();
    assert_eq!((seen.signed_in.len(), seen.not_signed_in, seen.refused.len()), (4, 0, 0), "{seen:?}");
    assert_eq!(
        seen.issued,
        [
            Issued { number: 449, kind: "CorpAgent".to_string(), account: "pat.typed@CORP.EXAMPLE".to_string(), by_an_agent: false },
            Issued { number: 450, kind: "CorpSignIn".to_string(), account: "alex.urban@CORP.EXAMPLE".to_string(), by_an_agent: true },
        ]
    );

    // the certificate is kept with its key, which is for its owner alone and is the certificate's
    let certificate = held(&folder.join("alex.pem"));
    let read = Certificate::read(&certificate).expect("the certificate that was kept");
    assert!(read.is_for(FOR_SIGNING_IN) && read.account.as_deref() == Some("alex.urban@CORP.EXAMPLE"));
    let key = std::fs::read_to_string(folder.join("alex.key.pem")).expect("the key's file");
    assert_eq!(key.lines().next(), Some("-----BEGIN PRIVATE KEY-----"));
    assert_eq!(std::fs::metadata(folder.join("alex.key.pem")).expect("the key's file").permissions().mode() & 0o777, 0o600);
    let key = request::Key::read(&held(&folder.join("alex.key.pem"))).expect("the key that was kept").written().unwrap();
    let pin = away_rdp::secret::Secret::new("1234");
    assert!(away_winauth::card::Card::new(&certificate, &key, pin, "a key", [1; 16]).is_ok(), "the key is the certificate's");
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn a_refusal_is_said_in_the_authoritys_own_words_and_nothing_is_kept() {
    let Some(server) = server() else { return };
    let (_runs, pages) = pages();
    let folder = folder("refused");
    let trusts = folder.join("authority.pem").display().to_string();
    let tried = try_certificate(&pages.address, &server, &folder, "NoSuchKind", "none", &["--trusts", &trusts]);
    let said = String::from_utf8_lossy(&tried.stdout);
    assert_eq!(tried.status.code(), Some(1), "{said}");
    assert!(
        said.contains(
            "   The authority refused. It says: Your Request Id is 449. The disposition message is \"Denied by Policy Module 0x80094800, \
             The request was for a certificate template that is not supported by the Active Directory Certificate Services policy: NoSuchKind. \"."
        ),
        "{said}"
    );
    assert_eq!((pages.seen().issued.len(), pages.seen().refused.len()), (0, 1));
    assert!(!folder.join("none.pem").exists() && !folder.join("none.key.pem").exists());
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn pages_that_show_a_certificate_of_no_authority_that_is_trusted_are_not_asked() {
    let Some(server) = server() else { return };
    let (_runs, pages) = pages();
    let folder = folder("not-trusted");
    let tried = try_certificate(&pages.address, &server, &folder, "CorpAgent", "agent", &[]);
    let said = String::from_utf8_lossy(&tried.stdout);
    assert_eq!(tried.status.code(), Some(1), "{said}");
    assert!(said.contains("the connection to it couldn't be secured"), "{said}");
    // nothing reached the pages: no request, and no ticket
    assert_eq!(pages.seen(), Default::default());
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn a_file_that_is_there_already_is_not_written_over() {
    let folder = folder("there-already");
    std::fs::write(folder.join("agent.key.pem"), "a key that is somebody's").expect("a file");
    let tried = try_certificate("https://localhost:1/certsrv/", "localhost:1", &folder, "CorpAgent", "agent", &[]);
    let said = String::from_utf8_lossy(&tried.stdout);
    assert_eq!(tried.status.code(), Some(1), "{said}");
    assert!(said.contains("is there already, and is not written over."), "{said}");
    assert_eq!(std::fs::read_to_string(folder.join("agent.key.pem")).unwrap(), "a key that is somebody's");
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn it_says_what_it_takes() {
    for words in [
        &["certificate"][..],
        &["certificate", "https://authority.corp.example/certsrv/"],
        &["certificate", "https://authority.corp.example/certsrv/", "--kerberos", "dc-01", "--realm", "CORP.EXAMPLE", "--as", "svc"],
        // in another's name, an agent's certificate and its key go with it
        &[
            "certificate",
            "https://authority.corp.example/certsrv/",
            "--kerberos",
            "dc-01",
            "--realm",
            "CORP.EXAMPLE",
            "--as",
            "svc",
            "--password-file",
            "p",
            "--kind",
            "k",
            "--save",
            "c",
            "--save-key",
            "k",
            "--for",
            "CORP\\alex.urban",
        ],
    ] {
        let tried = Command::new(env!("CARGO_BIN_EXE_away-try")).args(words).output().expect("the test program runs");
        assert_eq!(tried.status.code(), Some(2), "{words:?}");
        assert!(
            String::from_utf8_lossy(&tried.stderr).contains("away-try certificate <pages> --kerberos <domain controller>"),
            "{words:?}"
        );
    }
    let help = Command::new(env!("CARGO_BIN_EXE_away-try")).arg("help").output().expect("the test program runs");
    assert!(String::from_utf8_lossy(&help.stdout).contains("away-try certificate <pages> --kerberos <domain controller>"));
}
