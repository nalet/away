//! The try program's way with an organisation's authority: it asks pages that stand in
//! for an authority's, signed in with a ticket of a real Kerberos server where
//! `AWAY_TEST_KERBEROS` names one (`tests/machines/kerberos/real.sh` starts it).
//!
//! The pages are the tests' own: they open the ticket with the key of their service,
//! as an authority's pages do, read the request as an authority reads one, and answer
//! with pages built as a real authority's are.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::Arc;
use std::thread::JoinHandle;

use away_part::address::read_form;
use away_part::base64;
use away_winauth::kerberos::crypto::{Key, Kind};
use away_winauth::kerberos::gss::stand_in::{answer, key_in_the_ticket, Ends};
use away_winauth::kerberos::gss::Bound;
use away_winauth::request;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../away-winauth/tests/fixtures");
/// The authority the pages' certificate is of, the certificate the pages show, which is
/// for `localhost`, and its key.
const AUTHORITY: &[u8] = include_bytes!("../../away-winauth/tests/fixtures/authority.der");
const SHOWN: &[u8] = include_bytes!("../../away-winauth/tests/fixtures/controller.der");
const KEY_OF_IT: &[u8] = include_bytes!("../../away-winauth/tests/fixtures/controller.key.der");
/// The certificate the pages hand out, whatever was asked for: one for an account, for
/// signing in.
const HANDED_OUT: &[u8] = include_bytes!("../../away-winauth/tests/fixtures/account.der");
/// The account that asks, as the tests' Kerberos server knows it.
const PASSWORD: &str = "a password that exists for these tests alone";

/// The key of the service the tests' Kerberos server knows the pages by, made from the
/// service's password as that server makes it (`tests/machines/kerberos/real.sh`).
fn of_the_pages() -> Key {
    Key::from_password(Kind::Aes256, "the password of pages that exist for these tests alone", b"CORP.EXAMPLEHTTPlocalhost", 4096)
}

/// What the pages were asked, and by whom.
#[derive(Default, Debug)]
struct Seen {
    /// What was asked for, in the order it was asked.
    asked: Vec<String>,
    /// The account each request was signed in as, and whether its proof was bound to
    /// the certificate the pages showed.
    signed_in: Vec<(String, bool)>,
    /// The kind of certificate that was asked for.
    kind: Option<String>,
    /// The account the request is in the name of, where an agent signed it, and
    /// whether the agent's certificate is the one the tests gave.
    in_the_name_of: Option<(String, bool)>,
    /// What the request calls its subject.
    name: Option<String>,
}

fn page(status: &str, kind: &str, holds: &str) -> Vec<u8> {
    format!("HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\n\r\n{holds}", holds.len()).into_bytes()
}

/// Pages that stand in for an authority's, for one connection. Gives their address and
/// what they saw.
fn pages() -> (String, JoinHandle<Seen>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("an address");
    let address = format!("https://localhost:{}/certsrv/", listener.local_addr().expect("its address").port());
    let running = std::thread::spawn(move || {
        let mut seen = Seen::default();
        let Ok((mut connection, _)) = listener.accept() else { return seen };
        let config = ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .expect("versions")
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(SHOWN.to_vec())],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(KEY_OF_IT.to_vec())),
            )
            .expect("the pages' certificate");
        let mut secured = ServerConnection::new(Arc::new(config)).expect("a connection to secure");
        while secured.is_handshaking() {
            if secured.complete_io(&mut connection).is_err() {
                return seen;
            }
        }
        let mut secured = rustls::Stream::new(&mut secured, &mut connection);
        let bound = Bound::to_the_certificate(SHOWN).expect("the pages' certificate");
        let mut arrived = Vec::new();
        loop {
            // one request: its head, and as much as the head says follows
            let (head, sent) = loop {
                if let Some(at) = arrived.windows(4).position(|four| four == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&arrived[..at]).to_string();
                    let length = head
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|length| length.trim().parse::<usize>().unwrap_or(0))
                        })
                        .unwrap_or(0);
                    if arrived.len() >= at + 4 + length {
                        let sent = arrived[at + 4..at + 4 + length].to_vec();
                        arrived.drain(..at + 4 + length);
                        break (head, sent);
                    }
                }
                let mut bytes = [0u8; 8192];
                match secured.read(&mut bytes) {
                    Ok(count) if count > 0 => arrived.extend_from_slice(&bytes[..count]),
                    _ => return seen,
                }
            };
            let asked = head.lines().next().unwrap_or_default().to_string();
            seen.asked.push(asked.clone());
            // whoever asks shows a ticket for the pages' service, which opens with its key
            let token = head
                .lines()
                .find_map(|line| line.strip_prefix("authorization: Negotiate ").or(line.strip_prefix("Authorization: Negotiate ")))
                .and_then(|letters| base64::read_for_addresses(&letters.trim().replace('+', "-").replace('/', "_").replace('=', "")));
            let signed_in = token.and_then(|token| {
                let session = key_in_the_ticket(&token, &of_the_pages()).ok()?;
                answer(&token, &session, None, Ends::Content, [4; 16]).ok().map(|(_, said, _)| said)
            });
            let Some(said) = signed_in else {
                let refused = [&b"HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Negotiate\r\nContent-Length: 0\r\n\r\n"[..]].concat();
                if secured.write_all(&refused).is_err() {
                    return seen;
                }
                continue;
            };
            seen.signed_in.push((said.account.clone(), said.bound == bound));

            let answered = if asked.starts_with("POST /certsrv/certfnsh.asp ") {
                let form = read_form(&String::from_utf8_lossy(&sent)).unwrap_or_default();
                let field = |name: &str| form.iter().find(|(its, _)| its == name).map(|(_, value)| value.clone()).unwrap_or_default();
                let kind = field("CertAttrib").strip_prefix("CertificateTemplate:").unwrap_or_default().to_string();
                let sent = base64::read_for_addresses(&field("CertRequest").replace('+', "-").replace('/', "_").replace('=', ""))
                    .unwrap_or_default();
                // a request an agent signed in another's name, or one of the account's own
                let plain = match request::stand_in::read_in_the_name_of(&sent) {
                    Ok(signed) => {
                        seen.in_the_name_of = Some((signed.account, signed.agent == HANDED_OUT));
                        signed.asked
                    }
                    Err(_) => sent,
                };
                seen.name = request::stand_in::read(&plain).ok().map(|read| read.name);
                seen.kind = Some(kind.clone());
                match (kind.as_str(), &seen.name) {
                    ("NoSuchKind", _) | (_, None) => page("200 OK", "text/html", REFUSED),
                    _ => page("200 OK", "text/html", ISSUED),
                }
            } else if asked.starts_with("GET /certsrv/certnew.cer?ReqID=449&Enc=b64 ") {
                let letters = base64::ordinary(HANDED_OUT);
                page(
                    "200 OK",
                    "application/pkix-cert",
                    &format!("-----BEGIN CERTIFICATE-----\r\n{letters}\r\n-----END CERTIFICATE-----\r\n"),
                )
            } else {
                page("404 Not Found", "text/html", "<html><body>Not found.</body></html>")
            };
            if secured.write_all(&answered).is_err() {
                return seen;
            }
        }
    });
    (address, running)
}

const ISSUED: &str = "<HTML><Body><P ID=locPageTitle> <B> Certificate Issued </B>\
    <P ID=locInfoCertIssued> The certificate you requested was issued to you.</P>\
    <A Href=\"certnew.cer?ReqID=449&amp;Enc=b64\">Download certificate</A></Body></HTML>";

const REFUSED: &str = "<HTML><Body><P ID=locPageTitle> <B> Certificate Request Denied </B>\n\
    <P ID=locDenied> Your certificate request was denied. </P>\n\
    <P ID=locInfoReqIDandReason>\n\nYour Request Id is 452. The disposition message is \"Denied by Policy Module\".\n</P>\n\
    <P ID=locContactAdmin>Contact your administrator for further information.\n</P></Body></HTML>";

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
    let lines = format!("-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n", base64::ordinary(AUTHORITY));
    std::fs::write(folder.join("authority.pem"), lines).expect("the authority's file");
    folder
}

fn try_certificate(pages: &str, server: &str, folder: &std::path::Path, kind: &str, more: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_away-try"))
        .args(["certificate", pages, "--kerberos", server, "--realm", "CORP.EXAMPLE", "--as", "pat.typed", "--kind", kind])
        .arg("--password-file")
        .arg(folder.join("password"))
        .arg("--save")
        .arg(folder.join("certificate.pem"))
        .arg("--save-key")
        .arg(folder.join("key.pem"))
        .args(more)
        .output()
        .expect("the test program runs")
}

fn in_the_name_of_alex(folder: &std::path::Path) -> Vec<String> {
    vec![
        "--for".to_string(),
        "CORP\\alex.urban".to_string(),
        "--agent".to_string(),
        format!("{FIXTURES}/account.der"),
        "--agent-key".to_string(),
        format!("{FIXTURES}/account.key.der"),
        "--trusts".to_string(),
        folder.join("authority.pem").display().to_string(),
    ]
}

#[test]
fn it_asks_in_another_accounts_name_signed_in_with_a_ticket_that_is_bound_to_the_connection() {
    let Some(server) = server() else { return };
    let (address, running) = pages();
    let folder = folder("for-another");
    let more = in_the_name_of_alex(&folder);
    let tried = try_certificate(&address, &server, &folder, "CorpSignIn", &more.iter().map(String::as_str).collect::<Vec<_>>());
    let said = String::from_utf8_lossy(&tried.stdout);
    assert!(tried.status.success(), "{said}");
    for line in [
        "Making a key of 2048 bits, and the request for a certificate for it",
        "Signing the request in the name of CORP\\alex.urban, with the agent's certificate",
        "for a certificate of the kind CorpSignIn, signed in as pat.typed@CORP.EXAMPLE",
        "   It issued a certificate for alex.urban@CORP.EXAMPLE, as corp-authority.",
        "   It is one for signing in.",
    ] {
        assert!(said.contains(line), "{line}\n{said}");
    }

    // the pages were asked twice, each time by the account with a proof bound to their certificate
    let seen = running.join().expect("the pages ran");
    assert_eq!(seen.asked, ["POST /certsrv/certfnsh.asp HTTP/1.1", "GET /certsrv/certnew.cer?ReqID=449&Enc=b64 HTTP/1.1"]);
    assert_eq!(seen.signed_in, [("pat.typed".to_string(), true), ("pat.typed".to_string(), true)]);
    assert_eq!(seen.kind.as_deref(), Some("CorpSignIn"));
    // the agent signed for the account, and the request proves that its key is held
    assert_eq!(seen.in_the_name_of, Some(("CORP\\alex.urban".to_string(), true)));
    assert_eq!(seen.name.as_deref(), Some("alex.urban"));

    // the certificate is kept as it came, and the key beside it for its owner alone
    let kept = std::fs::read_to_string(folder.join("certificate.pem")).expect("the certificate's file");
    assert_eq!(kept.lines().next(), Some("-----BEGIN CERTIFICATE-----"));
    let letters: String = kept.lines().filter(|line| !line.starts_with("-----")).collect();
    assert!(base64::ordinary(HANDED_OUT) == letters, "the certificate that is kept is the one the pages gave");
    let key = std::fs::read_to_string(folder.join("key.pem")).expect("the key's file");
    assert_eq!(key.lines().next(), Some("-----BEGIN PRIVATE KEY-----"));
    assert_eq!(std::fs::metadata(folder.join("key.pem")).expect("the key's file").permissions().mode() & 0o777, 0o600);
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn it_asks_for_a_certificate_of_the_accounts_own_where_nobody_else_is_named() {
    let Some(server) = server() else { return };
    let (address, running) = pages();
    let folder = folder("its-own");
    let trusts = folder.join("authority.pem").display().to_string();
    let tried = try_certificate(&address, &server, &folder, "CorpAgent", &["--trusts", &trusts]);
    let said = String::from_utf8_lossy(&tried.stdout);
    assert!(tried.status.success(), "{said}");
    assert!(!said.contains("Signing the request in the name of"), "{said}");
    let seen = running.join().expect("the pages ran");
    assert_eq!((seen.in_the_name_of, seen.name.as_deref(), seen.kind.as_deref()), (None, Some("pat.typed"), Some("CorpAgent")));
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn a_refusal_is_said_in_the_authoritys_own_words_and_nothing_is_kept() {
    let Some(server) = server() else { return };
    let (address, running) = pages();
    let folder = folder("refused");
    let more = in_the_name_of_alex(&folder);
    let tried = try_certificate(&address, &server, &folder, "NoSuchKind", &more.iter().map(String::as_str).collect::<Vec<_>>());
    let said = String::from_utf8_lossy(&tried.stdout);
    assert_eq!(tried.status.code(), Some(1), "{said}");
    assert!(
        said.contains("   The authority refused. It says: Your Request Id is 452. The disposition message is \"Denied by Policy Module\"."),
        "{said}"
    );
    assert_eq!(running.join().expect("the pages ran").asked.len(), 1);
    assert!(!folder.join("certificate.pem").exists() && !folder.join("key.pem").exists());
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn pages_that_show_a_certificate_of_no_authority_that_is_trusted_are_not_asked() {
    let Some(server) = server() else { return };
    let (address, running) = pages();
    let folder = folder("not-trusted");
    let tried = try_certificate(&address, &server, &folder, "CorpAgent", &[]);
    let said = String::from_utf8_lossy(&tried.stdout);
    assert_eq!(tried.status.code(), Some(1), "{said}");
    assert!(said.contains("the connection to it couldn't be secured"), "{said}");
    // nothing reached the pages: no request, and no ticket
    let seen = running.join().expect("the pages ran");
    assert!(seen.asked.is_empty() && seen.signed_in.is_empty(), "{seen:?}");
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn a_file_that_is_there_already_is_not_written_over() {
    let folder = folder("there-already");
    std::fs::write(folder.join("key.pem"), "a key that is somebody's").expect("a file");
    let tried = try_certificate("https://localhost:1/certsrv/", "localhost:1", &folder, "CorpAgent", &[]);
    let said = String::from_utf8_lossy(&tried.stdout);
    assert_eq!(tried.status.code(), Some(1), "{said}");
    assert!(said.contains("is there already, and is not written over."), "{said}");
    assert_eq!(std::fs::read_to_string(folder.join("key.pem")).unwrap(), "a key that is somebody's");
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
