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
use away_rdp::picture::Picture;
use away_rdp::secret::Secret;
use away_rdp::stand_in::desktop::{Desktop, Seen};
use away_rdp::stand_in::StandIn;
use away_rdp::tpkt::Frames;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection};

const CERTIFICATE: &[u8] = include_bytes!("fixtures/machine.der");
const KEY: &[u8] = include_bytes!("fixtures/machine.key.der");

/// What a Windows machine answers when it chooses the sign-in before the desktop.
const CHOSEN: [u8; 19] = [0x03, 0x00, 0x00, 0x13, 0x0e, 0xd0, 0x00, 0x00, 0x12, 0x34, 0x00, 0x02, 0x3f, 0x08, 0x00, 0x08, 0x00, 0x00, 0x00];
/// What a machine answers that secures the connection and signs the account in itself.
const CHOSEN_TLS: [u8; 19] =
    [0x03, 0x00, 0x00, 0x13, 0x0e, 0xd0, 0x00, 0x00, 0x12, 0x34, 0x00, 0x02, 0x00, 0x08, 0x00, 0x01, 0x00, 0x00, 0x00];
/// What a machine answers that takes no connection over TLS.
const REFUSED: [u8; 19] =
    [0x03, 0x00, 0x00, 0x13, 0x0e, 0xd0, 0x00, 0x00, 0x12, 0x34, 0x00, 0x03, 0x00, 0x08, 0x00, 0x02, 0x00, 0x00, 0x00];

/// The account the stand-in knows, and its password. Neither exists anywhere else.
const KNOWN: &str = "CORP\\alex.urban";
const PASSWORD: &str = "a password that exists for these tests alone";

/// The stand-in's verdict on whether the account may come in: it may, or access is refused.
const MAY_COME_IN: [u8; 4] = [0, 0, 0, 0];
const MAY_NOT: [u8; 4] = [5, 0, 0, 0];

/// The desktop the stand-in draws: flat parts, slopes, noise and stripes, so that its
/// rectangles hold everything a machine's do. Its size is no multiple of a rectangle's.
fn desktop() -> Picture {
    let mut picture = Picture::new(200, 120);
    let mut noise = 1u32;
    for row in 0..picture.height {
        for column in 0..picture.width {
            noise = noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let pixel = match column / 50 {
                0 => [0xf3, 0xf2, 0xf1],
                1 => [(row * 2) as u8, column as u8, (row + column) as u8],
                2 => [(noise >> 8) as u8, (noise >> 16) as u8, (noise >> 24) as u8],
                _ => [if row % 2 == 0 { 0 } else { 255 }; 3],
            };
            picture.pixels[(row * picture.width + column) * 4..][..3].copy_from_slice(&pixel);
        }
    }
    picture
}

/// What a stand-in gives when it ends: the first request it read and, where the
/// connection went that far, what it saw of the client on the way to the desktop.
struct Ended {
    request: Vec<u8>,
    seen: Option<Seen>,
}

/// How a stand-in goes on after its first answer.
#[derive(Clone, Copy)]
enum Then {
    /// It ends there.
    Ends,
    /// It secures the connection, takes the sign-in before the desktop, says this
    /// verdict, and draws its desktop.
    SignsInFirst([u8; 4]),
    /// It secures the connection and draws its desktop for the account the client names.
    SignsInItself,
    /// It secures the connection, takes the sign-in before the desktop, and goes on as
    /// a Windows Server for many people does: it asks the client to take a licence,
    /// hands one over or doesn't, and draws its desktop at 16 bits a pixel.
    AsAServerForMany { hands_a_licence_over: bool },
}

/// A stand-in for a machine: it takes one connection, reads the first request, answers
/// with these bytes, and goes on as it is told to.
fn machine(answer: &'static [u8], secures: bool) -> (String, JoinHandle<Ended>) {
    machine_then(answer, if secures { Then::SignsInFirst(MAY_COME_IN) } else { Then::Ends })
}

fn machine_then(answer: &'static [u8], then: Then) -> (String, JoinHandle<Ended>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("an address");
    let address = listener.local_addr().expect("its address").to_string();
    let running = std::thread::spawn(move || {
        let (mut connection, _) = listener.accept().expect("a connection");
        let mut request = [0u8; 19];
        connection.read_exact(&mut request).expect("the first request");
        connection.write_all(answer).expect("the answer goes out");
        let seen = match then {
            Then::Ends => None,
            Then::SignsInFirst(verdict) => secure(&mut connection, Some(verdict), Desktop::new(desktop())),
            Then::SignsInItself => secure(&mut connection, None, Desktop::new(desktop())),
            Then::AsAServerForMany { hands_a_licence_over: true } => {
                secure(&mut connection, Some(MAY_COME_IN), Desktop::new(desktop()).at_16_bits().that_hands_out_licences())
            }
            Then::AsAServerForMany { hands_a_licence_over: false } => secure(
                &mut connection,
                Some(MAY_COME_IN),
                Desktop::new(desktop()).at_16_bits().that_asks_for_a_licence_and_hands_none_over(),
            ),
        };
        Ended { request: request.to_vec(), seen }
    });
    (address, running)
}

/// Secures the connection, takes the sign-in before the desktop where a verdict is given
/// to end it with, and lets this desktop answer for as long as the client stays.
fn secure(connection: &mut TcpStream, verdict: Option<[u8; 4]>, mut desktop: Desktop) -> Option<Seen> {
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
            return None;
        }
    }
    // the sign-in before the desktop, answered as a Windows machine of a domain answers:
    // it knows one account, and holds the certificate it showed
    let mut secured = rustls::Stream::new(&mut secured, connection);
    let holds = Identity::read(CERTIFICATE).expect("the stand-in's certificate").public_key;
    let mut stand_in = StandIn::new(Account::named(KNOWN, Secret::new(PASSWORD)), &holds);
    while verdict.is_some() {
        let mut message = Vec::new();
        loop {
            match away_rdp::der::whole(&message) {
                Ok(Some(length)) if message.len() >= length => break,
                Ok(_) => {}
                Err(_) => return None,
            }
            let mut bytes = [0u8; 4096];
            match secured.read(&mut bytes) {
                Ok(count) if count > 0 => message.extend_from_slice(&bytes[..count]),
                // the client left: it had no account, or it was refused
                _ => return None,
            }
        }
        match stand_in.answer(&message) {
            Ok(answer) if answer.is_empty() => break,
            Ok(answer) => {
                if secured.write_all(&answer).is_err() {
                    return None;
                }
            }
            Err(_) => return None,
        }
    }
    // the name and the password have arrived: say whether the account may come in
    if let Some(verdict) = verdict {
        let _ = secured.write_all(&verdict);
    }
    // then the desktop, for as long as the client stays
    let mut frames = Frames::new();
    loop {
        while let Ok(Some(carried)) = frames.next_frame() {
            let answer = desktop.answer(&carried).expect("the client's messages are built as they are");
            if secured.write_all(&answer).is_err() {
                return Some(desktop.seen().clone());
            }
        }
        let mut bytes = [0u8; 4096];
        match secured.read(&mut bytes) {
            Ok(count) if count > 0 => frames.push(&bytes[..count]),
            _ => return Some(desktop.seen().clone()),
        }
    }
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
    assert_eq!(running.join().expect("the stand-in ends").request, away_rdp::negotiation::request(), "what it asked for");
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
fn it_signs_an_account_in_opens_the_desktop_and_keeps_its_picture() {
    let (address, running) = machine(&CHOSEN, true);
    let picture = std::env::temp_dir().join(format!("away-try-{}-desktop.bmp", std::process::id()));
    let file = password_file(PASSWORD);
    let tried = Command::new(env!("CARGO_BIN_EXE_away-try"))
        .args(["rdp", &address, "--save"])
        .arg(&picture)
        .args(["--as", KNOWN, "--password-file"])
        .arg(&file)
        .output()
        .expect("the test program runs");
    let _ = std::fs::remove_file(file);
    let said = String::from_utf8(tried.stdout).expect("text");
    let seen = running.join().expect("the stand-in ends").seen.expect("the connection went on to the desktop");
    assert!(tried.status.success(), "{said}");

    let drew = desktop();
    let sum: String = {
        use sha2::{Digest, Sha256};
        let mut sum = Sha256::new();
        drew.pixels.as_chunks::<4>().0.iter().for_each(|pixel| sum.update(&pixel[..3]));
        sum.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
    };
    let lines: Vec<&str> = said.lines().skip(11).collect();
    assert_eq!(
        lines[..13],
        [
            "Signing in, before the desktop, as CORP\\alex.urban",
            "   It answers as FIN-APP-02 of the domain CORP, in full FIN-APP-02.corp.example.com.",
            "   It says it runs Windows 10.0, build 26100.",
            "   It took the proof of the password, and proved that it holds the certificate it showed.",
            "   The name and the password were handed over, sealed.",
            "   It says the account may come in.",
            "Saying what Away is, and taking the channels",
            "   It gave the channel 1004 to this client, and 1003 is the desktop's.",
            "   It takes the client as it is, without a licence.",
            "Agreeing on what each side can do",
            "   It offers a desktop of 200 by 120, at 32 bits a pixel.",
            "   Both sides have said what they can do, and it draws from now on.",
            "Taking what it draws, until it has drawn nothing for 3 seconds",
        ],
        "{said}"
    );
    // eight rectangles: two as they are, of 64 by 64 and of 64 by 56 pixels, and six compressed
    assert_eq!(lines[13], "   2 rectangles at 32 bits a pixel, as they are, 30720 bytes in all.", "{said}");
    assert!(lines[14].starts_with("   6 rectangles at 32 bits a pixel, compressed, "), "{said}");
    assert_eq!(lines[15], "The desktop's picture");
    assert_eq!(lines[16], format!("   200 by 120 pixels. Their SHA-256: {sum}"));
    assert_eq!(lines[17], format!("   It is in {}.", picture.display()));
    assert_eq!(lines.len(), 18, "{said}");
    assert!(!said.contains(PASSWORD), "the password is never said");

    // the file holds the desktop the stand-in drew, pixel for pixel
    let kept = std::fs::read(&picture).expect("the picture's file");
    let _ = std::fs::remove_file(&picture);
    assert_eq!(&kept[..2], b"BM");
    assert_eq!(kept.len(), 54 + 200 * 120 * 3);
    assert_eq!(kept[18..26], [200, 0, 0, 0, 120, 0, 0, 0]);
    let rows: Vec<u8> = kept[54..].as_chunks::<600>().0.iter().rev().flatten().copied().collect();
    let drawn: Vec<u8> = drew.pixels.as_chunks::<4>().0.iter().flat_map(|pixel| &pixel[..3]).copied().collect();
    assert!(rows == drawn, "the picture in the file is not the one the stand-in drew");

    // what the stand-in saw of the client
    assert_eq!(
        seen,
        Seen {
            asked: (1280, 800),
            name: "away".to_string(),
            security: 8,
            account: ("CORP".to_string(), "alex.urban".to_string()),
            password: None,
            bits: 32,
            takes_instructions: false,
            left: true,
            licence: None,
        }
    );
}

/// The SHA-256 away-try says of the stand-in's desktop when it travels at 16 bits a
/// pixel: of every colour cut to those bits and made whole again.
fn sum_at_16_bits() -> String {
    use away_rdp::interleaved::{from_16_bits, to_16_bits};
    use sha2::{Digest, Sha256};
    let mut sum = Sha256::new();
    desktop().pixels.as_chunks::<4>().0.iter().for_each(|pixel| sum.update(&from_16_bits(to_16_bits(pixel))[..3]));
    sum.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn a_server_for_many_people_hands_out_a_licence_and_the_client_takes_it() {
    let (address, running) = machine_then(&CHOSEN, Then::AsAServerForMany { hands_a_licence_over: true });
    let tried = try_rdp_as(&address, KNOWN, PASSWORD);
    let said = String::from_utf8(tried.stdout).expect("text");
    let seen = running.join().expect("the stand-in ends").seen.expect("the connection went on to the desktop");
    assert!(tried.status.success(), "{said}");
    let lines: Vec<&str> = said.lines().skip(17).collect();
    assert_eq!(
        lines[..9],
        [
            "Saying what Away is, and taking the channels",
            "   It gave the channel 1004 to this client, and 1003 is the desktop's.",
            "   It hands out licences, and asks the client to take one: for A02, of A stand-in.",
            "   It handed over a licence of 23 bytes, from example.com.",
            "   The licence is not kept: the next connection takes one again.",
            "Agreeing on what each side can do",
            "   It offers a desktop of 200 by 120, at 16 bits a pixel.",
            "   Both sides have said what they can do, and it draws from now on.",
            "Taking what it draws, until it has drawn nothing for 3 seconds",
        ],
        "{said}"
    );
    // eight rectangles: two as they are, their rows filled up to whole words, and six in runs
    assert_eq!(lines[9], "   2 rectangles at 16 bits a pixel, as they are, 15840 bytes in all.", "{said}");
    assert!(lines[10].starts_with("   6 rectangles at 16 bits a pixel, compressed, "), "{said}");
    // the desktop behind the licence is the one the stand-in drew, in the colours that travelled
    assert_eq!(lines[12], format!("   200 by 120 pixels. Their SHA-256: {}", sum_at_16_bits()), "{said}");
    // the licence was taken in the account's name, by a client that is the same device on every run
    let licence = seen.licence.expect("the stand-in handed out a licence");
    assert_eq!(licence.names, ("alex.urban".to_string(), "away".to_string()));
    let known_by = {
        use sha2::{Digest, Sha256};
        Sha256::digest(b"away-try")[..16].to_vec()
    };
    assert_eq!(licence.known_by, [&[0, 0, 1, 4][..], &known_by].concat());
    assert_eq!(seen.bits, 16, "the client answers at the bits a pixel the machine offers");
    assert!(seen.left);
    assert!(!said.contains(PASSWORD), "the password is never said");
}

#[test]
fn a_server_that_counts_people_asks_for_a_licence_and_goes_on_without_one() {
    let (address, running) = machine_then(&CHOSEN, Then::AsAServerForMany { hands_a_licence_over: false });
    let tried = try_rdp_as(&address, KNOWN, PASSWORD);
    let said = String::from_utf8(tried.stdout).expect("text");
    let seen = running.join().expect("the stand-in ends").seen.expect("the connection went on to the desktop");
    assert!(tried.status.success(), "{said}");
    let lines: Vec<&str> = said.lines().skip(18).collect();
    assert_eq!(
        lines[..5],
        [
            "   It gave the channel 1004 to this client, and 1003 is the desktop's.",
            "   It hands out licences, and asks the client to take one: for A02, of A stand-in.",
            "   It took the client's answer, handed no licence over, and goes on.",
            "Agreeing on what each side can do",
            "   It offers a desktop of 200 by 120, at 16 bits a pixel.",
        ],
        "{said}"
    );
    assert_eq!(said.lines().rev().nth(1), Some(format!("   200 by 120 pixels. Their SHA-256: {}", sum_at_16_bits()).as_str()), "{said}");
    let licence = seen.licence.expect("the stand-in asked for a licence to be taken");
    assert_eq!(licence.names, ("alex.urban".to_string(), "away".to_string()));
    assert!(licence.known_by.is_empty());
}

#[test]
fn without_a_file_the_picture_is_not_kept_and_its_sum_is_still_said() {
    let (address, running) = machine(&CHOSEN, true);
    let tried = try_rdp_as(&address, KNOWN, PASSWORD);
    let said = String::from_utf8(tried.stdout).expect("text");
    running.join().expect("the stand-in ends");
    assert!(tried.status.success(), "{said}");
    let lines: Vec<&str> = said.lines().collect();
    assert!(lines[lines.len() - 2].starts_with("   200 by 120 pixels. Their SHA-256: "), "{said}");
    assert_eq!(lines[lines.len() - 1], "   To keep it, name a file: --save <file>.");
}

#[test]
fn a_machine_that_signs_in_itself_is_told_the_name_and_the_password_once_the_connection_is_secured() {
    let (address, running) = machine_then(&CHOSEN_TLS, Then::SignsInItself);
    let tried = try_rdp_as(&address, KNOWN, PASSWORD);
    let said = String::from_utf8(tried.stdout).expect("text");
    let seen = running.join().expect("the stand-in ends").seen.expect("the connection went on to the desktop");
    assert!(tried.status.success(), "{said}");
    let lines: Vec<&str> = said.lines().collect();
    assert_eq!(lines[3], "   It chose TLS, with the sign-in on its own screen.");
    assert_eq!(
        lines[10..15],
        [
            "Opening the desktop as CORP\\alex.urban",
            "   The name and the password travel to the machine inside the secured connection.",
            "Saying what Away is, and taking the channels",
            "   It gave the channel 1004 to this client, and 1003 is the desktop's.",
            "   It takes the client as it is, without a licence.",
        ],
        "{said}"
    );
    assert!(lines[lines.len() - 2].starts_with("   200 by 120 pixels. Their SHA-256: "), "{said}");
    assert!(!said.contains(PASSWORD), "the password is never said");
    assert_eq!(seen.account, ("CORP".to_string(), "alex.urban".to_string()));
    assert_eq!(seen.password.as_deref(), Some(PASSWORD));
    assert_eq!(seen.security, 1);
    assert!(seen.left);
}

#[test]
fn a_machine_that_signs_in_itself_is_given_no_password_nobody_named() {
    let (address, running) = machine_then(&CHOSEN_TLS, Then::SignsInItself);
    let tried = try_rdp(&address);
    let said = String::from_utf8(tried.stdout).expect("text");
    assert_eq!(running.join().expect("the stand-in ends").seen, Some(Seen::default()), "it went no further");
    assert!(tried.status.success(), "{said}");
    assert!(said.ends_with("To go on, name an account: --as <account> --password-file <file>.\n"), "{said}");
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
    let (address, running) = machine_then(&CHOSEN, Then::SignsInFirst(MAY_NOT));
    let tried = try_rdp_as(&address, KNOWN, PASSWORD);
    let said = String::from_utf8(tried.stdout).expect("text");
    assert!(running.join().expect("the stand-in ends").seen.is_some_and(|seen| seen == Seen::default()), "it went no further");
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
    assert!(
        String::from_utf8_lossy(&none.stderr).contains("away-try rdp <address> [--as <account> --password-file <file> [--save <file>]]")
    );
    // a picture needs an account, an account its password, and nothing is said twice
    for words in [
        &["rdp", "fin-app-02", "--save", "desktop.bmp"][..],
        &["rdp", "fin-app-02", "--as", KNOWN],
        &["rdp", "fin-app-02", "--as", KNOWN, "--as", KNOWN],
        &["rdp", "fin-app-02", "--anything", "else"],
        &["vnc", "fin-app-02"],
    ] {
        let refused = Command::new(env!("CARGO_BIN_EXE_away-try")).args(words).output().expect("the test program runs");
        assert_eq!(refused.status.code(), Some(2), "{words:?}");
    }
    let wrong = Command::new(env!("CARGO_BIN_EXE_away-try")).args(["rdp", "fin-app-02:port"]).output().expect("the test program runs");
    assert_eq!(wrong.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&wrong.stdout).contains("port is no port."));
}
