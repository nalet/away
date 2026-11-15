//! The try program's way with a domain controller: what it says for each step, against a
//! real Kerberos server where `AWAY_TEST_KERBEROS` names one
//! (`tests/machines/kerberos/real.sh` starts it).

use std::process::{Command, Output};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../away-winauth/tests/fixtures");

fn server() -> Option<String> {
    let at = std::env::var("AWAY_TEST_KERBEROS").ok().filter(|at| !at.is_empty());
    if at.is_none() {
        assert!(std::env::var_os("CI").is_none(), "the pipeline's tests need a Kerberos server: AWAY_TEST_KERBEROS is not set");
        eprintln!("no Kerberos server to test against: AWAY_TEST_KERBEROS is not set");
    }
    at
}

fn try_kerberos(words: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_away-try")).arg("kerberos").args(words).output().expect("the test program runs")
}

#[test]
fn it_says_what_it_takes() {
    for words in [
        &[][..],
        &["dc-01.corp.example"],
        &["dc-01.corp.example", "--realm", "CORP.EXAMPLE", "--as", "alex.urban"],
        // a certificate without its key and the authority, and a password beside a certificate
        &["dc-01.corp.example", "--realm", "CORP.EXAMPLE", "--as", "alex.urban", "--certificate", "c"],
        &[
            "dc-01.corp.example",
            "--realm",
            "CORP.EXAMPLE",
            "--as",
            "alex.urban",
            "--password-file",
            "p",
            "--certificate",
            "c",
            "--key",
            "k",
            "--authority",
            "a",
        ],
        &["dc-01.corp.example", "--realm", "CORP.EXAMPLE", "--as", "alex.urban", "--password-file", "p", "--anything", "else"],
    ] {
        let refused = try_kerberos(words);
        assert_eq!(refused.status.code(), Some(2), "{words:?}");
        assert!(String::from_utf8_lossy(&refused.stderr).starts_with("away-try kerberos <domain controller> --realm <realm>"), "{words:?}");
    }
    // a file that is not there, and a port nothing listens on
    let missing = try_kerberos(&["127.0.0.1:1", "--realm", "CORP.EXAMPLE", "--as", "alex.urban", "--password-file", "/nowhere/password"]);
    assert_eq!(missing.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&missing.stdout).contains("The password's file can't be read"));
}

#[test]
fn it_asks_a_real_server_for_tickets_with_a_certificate_and_with_a_password() {
    let Some(at) = server() else { return };
    let file = |name: &str| format!("{FIXTURES}/{name}");
    let with_a_certificate = try_kerberos(&[
        &at,
        "--realm",
        "CORP.EXAMPLE",
        "--as",
        "alex.urban",
        "--certificate",
        &file("account.der"),
        "--key",
        &file("account.key.der"),
        "--authority",
        &file("authority.der"),
        "--for",
        "TERMSRV/desk-01.corp.example",
    ]);
    let said = String::from_utf8(with_a_certificate.stdout).expect("text");
    assert!(with_a_certificate.status.success(), "{said}");
    let lines: Vec<&str> = said.lines().collect();
    assert_eq!(lines.len(), 7, "{said}");
    assert!(lines[0].ends_with("for a first ticket for alex.urban@CORP.EXAMPLE, with the certificate"), "{said}");
    assert!(lines[1].starts_with("   It proved itself with a certificate of the authority, for "), "{said}");
    assert_eq!(lines[2], "   It gave a ticket for krbtgt/CORP.EXAMPLE, to alex.urban@CORP.EXAMPLE.");
    assert_eq!(lines[4], "Asking for a ticket for TERMSRV/desk-01.corp.example, with the first ticket");
    assert_eq!(lines[5], "   It gave a ticket for TERMSRV/desk-01.corp.example, to alex.urban@CORP.EXAMPLE.");

    // with a password, read from a file
    let password = std::env::temp_dir().join(format!("away-try-kerberos-{}", std::process::id()));
    std::fs::write(&password, "a password that exists for these tests alone\n").expect("a file for the password");
    let with_a_password =
        try_kerberos(&[&at, "--realm", "CORP.EXAMPLE", "--as", "pat.typed", "--password-file", password.to_str().expect("a path")]);
    let said = String::from_utf8(with_a_password.stdout).expect("text");
    assert!(with_a_password.status.success(), "{said}");
    assert!(said.contains("   It gave a ticket for krbtgt/CORP.EXAMPLE, to pat.typed@CORP.EXAMPLE.\n"), "{said}");
    // a wrong one is refused, in words
    std::fs::write(&password, "another password\n").expect("a file for the password");
    let refused =
        try_kerberos(&[&at, "--realm", "CORP.EXAMPLE", "--as", "pat.typed", "--password-file", password.to_str().expect("a path")]);
    let _ = std::fs::remove_file(&password);
    let said = String::from_utf8(refused.stdout).expect("text");
    assert_eq!(refused.status.code(), Some(1), "{said}");
    assert!(said.contains("The domain controller refused: the proof of who asks did not hold (its own code 24)"), "{said}");

    // and an authority that is not the server's is not believed
    let disbelieved = try_kerberos(&[
        &at,
        "--realm",
        "CORP.EXAMPLE",
        "--as",
        "alex.urban",
        "--certificate",
        &file("account.der"),
        "--key",
        &file("account.key.der"),
        "--authority",
        &file("stranger.der"),
    ]);
    let said = String::from_utf8(disbelieved.stdout).expect("text");
    assert_eq!(disbelieved.status.code(), Some(1), "{said}");
    assert!(
        said.ends_with("   It did not prove who it is: its certificate leads to no authority that is trusted, or is good no longer.\n"),
        "{said}"
    );
}
