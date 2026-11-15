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
use away_winauth::kerberos::crypto::{Key, Kind};
use away_winauth::sign_in::stand_in::Machine;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection};

/// The tests' certificates: an authority, a domain controller's, and an account's with its key.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../away-winauth/tests/fixtures");
const ACCOUNTS_CERTIFICATE: &[u8] = include_bytes!("../../away-winauth/tests/fixtures/account.der");
/// What a stand-in that reads a card has the card sign: the hash of this.
const TO_SIGN: &[u8] = b"what proves the person to the domain";

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
    told: Told,
}

/// What a stand-in that takes tickets was told in the sign-in before the desktop: who
/// the ticket is for, and what the client handed over.
#[derive(Default)]
struct Told {
    account: Option<(String, String)>,
    handed_over: Option<Vec<u8>>,
}

/// The key of the service the tests' Kerberos server knows the stand-in by, made from
/// the service's password as that server makes it (`tests/machines/kerberos/real.sh`).
fn of_the_service() -> Key {
    Key::from_password(Kind::Aes256, "the password of a machine that exists for these tests alone", b"CORP.EXAMPLETERMSRV127.0.0.1", 4096)
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
    /// It secures the connection, takes the sign-in before the desktop, and publishes
    /// one program: a client that asks for it is shown the program's window.
    PublishesAProgram,
    /// It secures the connection, takes the sign-in before the desktop with a ticket,
    /// as a machine of a domain does, and draws its desktop. It holds the key of its
    /// service, or another machine's.
    TakesATicket { holds_the_services_key: bool },
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
        let mut told = Told::default();
        let seen = match then {
            Then::Ends => None,
            Then::SignsInFirst(verdict) => secure(&mut connection, Some(verdict), Desktop::new(desktop())),
            Then::SignsInItself => secure(&mut connection, None, Desktop::new(desktop())),
            Then::AsAServerForMany { hands_a_licence_over: true } => {
                secure(&mut connection, Some(MAY_COME_IN), Desktop::new(desktop()).at_16_bits().that_hands_out_licences())
            }
            Then::PublishesAProgram => secure(&mut connection, Some(MAY_COME_IN), Desktop::new(desktop()).that_publishes(&["||ledger"])),
            Then::TakesATicket { holds_the_services_key } => {
                let key = if holds_the_services_key { of_the_service() } else { Key::new(Kind::Aes256, &[0x17; 32]).expect("a key") };
                secure_with(&mut connection, Some(MAY_COME_IN), Desktop::new(desktop()), Some((key, &mut told)))
            }
            Then::AsAServerForMany { hands_a_licence_over: false } => secure(
                &mut connection,
                Some(MAY_COME_IN),
                Desktop::new(desktop()).at_16_bits().that_asks_for_a_licence_and_hands_none_over(),
            ),
        };
        Ended { request: request.to_vec(), seen, told }
    });
    (address, running)
}

/// Secures the connection, takes the sign-in before the desktop where a verdict is given
/// to end it with, and lets this desktop answer for as long as the client stays.
fn secure(connection: &mut TcpStream, verdict: Option<[u8; 4]>, desktop: Desktop) -> Option<Seen> {
    secure_with(connection, verdict, desktop, None)
}

/// The same, and with a key the sign-in before the desktop is one with a ticket: the
/// stand-in holds that key as the key of its service, and says what it was told.
fn secure_with(
    connection: &mut TcpStream,
    verdict: Option<[u8; 4]>,
    mut desktop: Desktop,
    takes_tickets: Option<(Key, &mut Told)>,
) -> Option<Seen> {
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
    let mut takes_tickets = takes_tickets.map(|(key, told)| (Machine::new(key, &holds), told));
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
        let answered = match &mut takes_tickets {
            Some((machine, told)) => {
                let answered = machine.answer(&message).ok();
                (told.account, told.handed_over) = (machine.account.clone(), machine.handed_over.clone());
                answered
            }
            None => stand_in.answer(&message).ok(),
        };
        match answered {
            Some(answer) if answer.is_empty() => break,
            Some(answer) => {
                if secured.write_all(&answer).is_err() {
                    return None;
                }
            }
            None => return None,
        }
    }
    // a machine that was handed a smart card reads it once the desktop begins, as a
    // Windows machine does to sign the person in: with the PIN it was handed
    if let Some(card) =
        takes_tickets.as_ref().and_then(|(machine, _)| machine.handed_over.as_deref()).and_then(away_winauth::sign_in::stand_in::on_a_card)
    {
        use sha2::{Digest, Sha256};
        let hash: [u8; 32] = Sha256::digest(TO_SIGN).into();
        desktop = desktop.that_reads_a_card(&away_winauth::card::stand_in::commands(card.pin.expose(), &hash));
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
            input: Vec::new(),
            sent_on_to: None,
            for_a_program: false,
            about_programs: Vec::new(),
            // without a card, the client asks for no further channel and names no device
            channels: Vec::new(),
            devices: Vec::new(),
            card: None,
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

fn try_a_program(address: &str, program: &str) -> Output {
    let file = password_file(PASSWORD);
    let tried = Command::new(env!("CARGO_BIN_EXE_away-try"))
        .args(["rdp", address, "--as", KNOWN, "--program", program, "--password-file"])
        .arg(&file)
        .output()
        .expect("the test program runs");
    let _ = std::fs::remove_file(file);
    tried
}

#[test]
fn a_program_is_asked_for_and_its_window_is_said_and_drawn() {
    let (address, running) = machine_then(&CHOSEN, Then::PublishesAProgram);
    let tried = try_a_program(&address, "||ledger");
    let said = String::from_utf8(tried.stdout).expect("text");
    let ended = running.join().expect("the stand-in ends");
    assert!(tried.status.success(), "{said}");
    let lines: Vec<&str> = said.lines().collect();
    let at = lines.iter().position(|line| *line == "   It is ready for programs, and was asked for ||ledger.").expect(&said);
    assert_eq!(
        lines[at + 1..at + 4],
        [
            "   The program started.",
            "   A new window: number 65537, “ledger”, at 20, 10, 160 by 100.",
            "   In front is the window number 65537."
        ],
        "{said}"
    );
    // what was drawn is the window, at its place on a desktop that is otherwise empty
    assert_eq!(
        lines[lines.len() - 5..lines.len() - 3],
        [
            "   2 rectangles at 32 bits a pixel, as they are, 17408 bytes in all.",
            "   4 rectangles at 32 bits a pixel, compressed, 20585 bytes in all."
        ],
        "{said}"
    );
    assert!(lines[lines.len() - 2].starts_with("   200 by 120 pixels. Their SHA-256: "), "{said}");
    let seen = ended.seen.expect("the client came as far as the desktop");
    assert!(seen.for_a_program);
    assert_eq!(seen.about_programs.len(), 4);
}

#[test]
fn a_program_the_machine_does_not_publish_is_said_not_to_start() {
    let (address, running) = machine_then(&CHOSEN, Then::PublishesAProgram);
    let tried = try_a_program(&address, "||payroll");
    let said = String::from_utf8(tried.stdout).expect("text");
    running.join().expect("the stand-in ends");
    assert_eq!(tried.status.code(), Some(1), "{said}");
    assert!(
        said.ends_with(
            "   The program did not start: the machine does not let this program be started: it is not among those it publishes (its own code 0x00000002).\n"
        ),
        "{said}"
    );
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

/// Where the tests' Kerberos server is (`tests/machines/kerberos/real.sh` starts one).
fn kerberos_server() -> Option<String> {
    let at = std::env::var("AWAY_TEST_KERBEROS").ok().filter(|at| !at.is_empty());
    if at.is_none() {
        assert!(std::env::var_os("CI").is_none(), "the pipeline's tests need a Kerberos server: AWAY_TEST_KERBEROS is not set");
        eprintln!("no Kerberos server to test against: AWAY_TEST_KERBEROS is not set");
    }
    at
}

fn try_rdp_with_a_ticket(address: &str, account: &str, more: &[&str]) -> Output {
    let file = password_file(PASSWORD);
    let tried = Command::new(env!("CARGO_BIN_EXE_away-try"))
        .args(["rdp", address, "--as", account, "--password-file"])
        .arg(&file)
        .args(more)
        .output()
        .expect("the test program runs");
    let _ = std::fs::remove_file(file);
    tried
}

#[test]
fn it_signs_in_with_a_ticket_of_a_real_kerberos_server_in_place_of_a_proof_of_the_password() {
    let Some(server) = kerberos_server() else { return };
    // the account is one the server knows, with this password, and the stand-in is the
    // machine the server knows a service of: the realm is read in the account's name
    let (address, running) = machine_then(&CHOSEN, Then::TakesATicket { holds_the_services_key: true });
    let tried = try_rdp_with_a_ticket(&address, "pat.typed@corp.example", &["--kerberos", &server]);
    let said = String::from_utf8(tried.stdout).expect("text");
    let ended = running.join().expect("the stand-in ends");
    assert!(tried.status.success(), "{said}");
    let lines: Vec<&str> = said.lines().skip(11).collect();
    assert!(lines[0].ends_with(", how the key of pat.typed@CORP.EXAMPLE is made"), "{said}");
    assert_eq!(
        (lines[1], lines[2], lines[3]),
        (
            "   It asks for a proof, and says how the key is made from the password.",
            "Asking for a first ticket, with the time sealed with that key",
            "   It gave a ticket for krbtgt/CORP.EXAMPLE, to pat.typed@CORP.EXAMPLE.",
        ),
        "{said}"
    );
    assert_eq!(
        lines[5..7],
        [
            "Asking for a ticket for TERMSRV/127.0.0.1, with the first ticket",
            "   It gave a ticket for TERMSRV/127.0.0.1, to pat.typed@CORP.EXAMPLE."
        ],
        "{said}"
    );
    assert_eq!(
        lines[8..13],
        [
            "Signing in, before the desktop, with the ticket",
            "   It answered the ticket, and proved that it holds the certificate it showed.",
            "   The name and the password were handed over, sealed.",
            "   It says the account may come in.",
            "Saying what Away is, and taking the channels",
        ],
        "{said}"
    );
    assert!(said.contains("\nThe desktop's picture\n   200 by 120 pixels. Their SHA-256: "), "{said}");
    assert!(!said.contains(PASSWORD), "the password is never said");
    // the machine read in the ticket who it is for, and was handed what it signs the person in with
    assert!(ended.seen.is_some(), "the connection went on to the desktop");
    assert_eq!(ended.told.account, Some(("pat.typed".to_string(), "CORP.EXAMPLE".to_string())));
    let account = Account::named("pat.typed@corp.example", Secret::new(PASSWORD));
    assert_eq!(ended.told.handed_over, Some(away_rdp::sign_in::credentials(&account)));

    // a machine that is not the one the ticket is for can't open it, and nothing is handed over to it
    let (address, running) = machine_then(&CHOSEN, Then::TakesATicket { holds_the_services_key: false });
    let tried = try_rdp_with_a_ticket(&address, "CORP\\pat.typed", &["--kerberos", &server, "--realm", "CORP.EXAMPLE"]);
    let said = String::from_utf8(tried.stdout).expect("text");
    let ended = running.join().expect("the stand-in ends");
    assert_eq!(tried.status.code(), Some(1), "{said}");
    assert!(said.contains("Signing in, before the desktop, with the ticket\n") && !said.contains("handed over"), "{said}");
    assert!(ended.told.handed_over.is_none() && ended.seen.is_none());

    // a wrong password gets no ticket, and the machine is shown none
    let (address, running) = machine_then(&CHOSEN, Then::TakesATicket { holds_the_services_key: true });
    let file = password_file("another password");
    let tried = Command::new(env!("CARGO_BIN_EXE_away-try"))
        .args(["rdp", &address, "--as", "pat.typed@corp.example", "--kerberos", &server, "--password-file"])
        .arg(&file)
        .output()
        .expect("the test program runs");
    let _ = std::fs::remove_file(file);
    let said = String::from_utf8(tried.stdout).expect("text");
    assert_eq!(tried.status.code(), Some(1), "{said}");
    assert!(said.contains("The domain controller refused: ") && !said.contains("Signing in, before the desktop"), "{said}");
    assert!(running.join().expect("the stand-in ends").told.account.is_none());
}

/// What the try program says of a smart card the machine read to its end.
const WITH_THE_CARD: [&str; 5] = [
    "What it did with the smart card",
    "   It found the card to be of the kind it reads.",
    "   It read the certificate.",
    "   It gave the card its PIN.",
    "   It had the card sign what proves the person to the domain.",
];

/// Checks what a stand-in that took a ticket and read the card saw: the card that was
/// handed over, and on it the account's certificate and a signature of its key.
fn the_machine_read_the_card(ended: &Ended, said: &str, name: &str) {
    let handed = away_winauth::sign_in::stand_in::on_a_card(ended.told.handed_over.as_deref().expect("something was handed over"))
        .expect("a smart card");
    assert_eq!(
        (handed.name.as_str(), handed.domain.as_str(), handed.reader.as_str(), handed.card.as_str()),
        (name, "CORP.EXAMPLE", "Away 0", "")
    );
    assert_eq!(handed.provider, "Microsoft Base Smart Card Crypto Provider");
    // a PIN and a name for what holds the key that were made for this connection, and the PIN is said nowhere
    let pin = handed.pin.expose();
    assert!(pin.len() == 32 && pin.chars().all(|digit| digit.is_ascii_hexdigit()), "a PIN of 32 digits");
    assert!(handed.container.starts_with("away-") && !said.contains(pin));
    let seen = ended.seen.as_ref().expect("the connection went on to the desktop");
    assert_eq!(seen.channels, ["rdpdr", "rdpsnd"]);
    let card = seen.card.as_ref().expect("the machine read the card");
    assert!(card.closed && card.reader == "Away 0", "the machine went through all of it, with the reader it was told of");
    let read = away_winauth::card::stand_in::read(&card.answers).expect("the answers of a card of this kind");
    assert!(read.certificate == ACCOUNTS_CERTIFICATE, "the certificate on the card is the account's");
    assert_eq!((read.container.as_str(), read.took_the_pin), (handed.container.as_str(), true));
    assert!(
        away_winauth::card::stand_in::is_signed_by(ACCOUNTS_CERTIFICATE, TO_SIGN, &read.signed),
        "the card signed with the certificate's key"
    );
}

#[test]
fn it_signs_in_with_a_smart_card_and_a_certificate_and_no_password_is_in_it() {
    let Some(server) = kerberos_server() else { return };
    let (address, running) = machine_then(&CHOSEN, Then::TakesATicket { holds_the_services_key: true });
    let files = [format!("{FIXTURES}/account.der"), format!("{FIXTURES}/account.key.der"), format!("{FIXTURES}/authority.der")];
    let tried = Command::new(env!("CARGO_BIN_EXE_away-try"))
        .args(["rdp", &address, "--as", "alex.urban@corp.example", "--kerberos", &server])
        .args(["--certificate", &files[0], "--key", &files[1], "--authority", &files[2]])
        .output()
        .expect("the test program runs");
    let said = String::from_utf8(tried.stdout).expect("text");
    let ended = running.join().expect("the stand-in ends");
    assert!(tried.status.success(), "{said}");
    let lines: Vec<&str> = said.lines().skip(11).collect();
    // the ticket is asked for with the certificate, and the domain controller proves itself
    assert!(lines[0].ends_with(", for a first ticket for alex.urban@CORP.EXAMPLE, with the certificate"), "{said}");
    assert!(lines[1].starts_with("   It proved itself with a certificate of the authority, for "), "{said}");
    assert_eq!(lines[2], "   It gave a ticket for krbtgt/CORP.EXAMPLE, to alex.urban@CORP.EXAMPLE.", "{said}");
    assert_eq!(lines[5], "   It gave a ticket for TERMSRV/127.0.0.1, to alex.urban@CORP.EXAMPLE.", "{said}");
    assert_eq!(
        lines[7..11],
        [
            "Signing in, before the desktop, with the ticket",
            "   It answered the ticket, and proved that it holds the certificate it showed.",
            "   A smart card was handed over in place of a password, sealed: its PIN, and where the machine finds it.",
            "   It says the account may come in.",
        ],
        "{said}"
    );
    assert!(said.contains("\n   It took this client's smart card reader.\n"), "{said}");
    let at = lines.iter().position(|line| *line == WITH_THE_CARD[0]).expect("what it did with the card");
    assert!(lines[at + 1].starts_with("   It asked the reader ") && lines[at + 2..at + 6] == WITH_THE_CARD[1..], "{said}");
    assert!(said.contains("\nThe desktop's picture\n   200 by 120 pixels. Their SHA-256: "), "{said}");
    // the machine read in the ticket who it is for, and on the card the account's certificate
    assert_eq!(ended.told.account, Some(("alex.urban".to_string(), "CORP.EXAMPLE".to_string())));
    the_machine_read_the_card(&ended, &said, "alex.urban");
}

#[test]
fn it_hands_a_smart_card_over_where_the_ticket_is_asked_for_with_the_password() {
    let Some(server) = kerberos_server() else { return };
    let (address, running) = machine_then(&CHOSEN, Then::TakesATicket { holds_the_services_key: true });
    let files = [format!("{FIXTURES}/account.der"), format!("{FIXTURES}/account.key.der")];
    let tried =
        try_rdp_with_a_ticket(&address, "pat.typed@corp.example", &["--kerberos", &server, "--certificate", &files[0], "--key", &files[1]]);
    let said = String::from_utf8(tried.stdout).expect("text");
    let ended = running.join().expect("the stand-in ends");
    assert!(tried.status.success(), "{said}");
    assert!(said.contains(", how the key of pat.typed@CORP.EXAMPLE is made\n") && !said.contains("with the certificate"), "{said}");
    assert!(
        said.contains("   A smart card was handed over in place of a password, sealed: its PIN, and where the machine finds it.\n"),
        "{said}"
    );
    assert!(said.contains(&WITH_THE_CARD[1..].join("\n")), "{said}");
    assert!(!said.contains(PASSWORD), "the password is never said");
    the_machine_read_the_card(&ended, &said, "pat.typed");

    // a key that is not the certificate's makes no card
    let (address, running) = machine_then(&CHOSEN, Then::TakesATicket { holds_the_services_key: true });
    let anothers = format!("{FIXTURES}/controller.key.der");
    let tried =
        try_rdp_with_a_ticket(&address, "pat.typed@corp.example", &["--kerberos", &server, "--certificate", &files[0], "--key", &anothers]);
    let said = String::from_utf8(tried.stdout).expect("text");
    assert_eq!(tried.status.code(), Some(1), "{said}");
    assert!(said.ends_with("   The certificate can't be read: the key is not the certificate's.\n"), "{said}");
    assert!(running.join().expect("the stand-in ends").told.handed_over.is_none());
}

#[test]
fn a_ticket_needs_a_realm_which_is_named_or_read_in_the_accounts_name() {
    let (address, running) = machine(&CHOSEN, true);
    let tried = try_rdp_with_a_ticket(&address, KNOWN, &["--kerberos", "127.0.0.1:1"]);
    let said = String::from_utf8(tried.stdout).expect("text");
    let _ = running.join();
    assert_eq!(tried.status.code(), Some(1), "{said}");
    assert!(said.ends_with("Name the realm, the domain's name in capitals: --realm <realm>.\n"), "{said}");
}

#[test]
fn it_says_what_it_takes() {
    let none = Command::new(env!("CARGO_BIN_EXE_away-try")).output().expect("the test program runs");
    assert_eq!(none.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&none.stderr).contains("away-try rdp <address> [--as <account> --password-file <file> [--save <file>]]")
    );
    // a picture, a program and a ticket need an account, a realm a ticket, an account its
    // password, and nothing is said twice
    for words in [
        &["rdp", "fin-app-02", "--save", "desktop.bmp"][..],
        &["rdp", "fin-app-02", "--program", "||ledger"],
        &["rdp", "fin-app-02", "--kerberos", "dc-01.corp.example"],
        &["rdp", "fin-app-02", "--realm", "CORP.EXAMPLE"],
        // a card is a certificate with its key and goes with a ticket; an authority is a card's, and in place of a password
        &["rdp", "fin-app-02", "--as", KNOWN, "--password-file", "f", "--kerberos", "dc-01", "--certificate", "c"],
        &["rdp", "fin-app-02", "--as", KNOWN, "--password-file", "f", "--certificate", "c", "--key", "k"],
        &["rdp", "fin-app-02", "--as", KNOWN, "--password-file", "f", "--kerberos", "dc-01", "--authority", "a"],
        &[
            "rdp",
            "fin-app-02",
            "--as",
            KNOWN,
            "--password-file",
            "f",
            "--kerberos",
            "dc-01",
            "--certificate",
            "c",
            "--key",
            "k",
            "--authority",
            "a",
        ],
        &["rdp", "fin-app-02", "--as", KNOWN, "--kerberos", "dc-01", "--certificate", "c", "--key", "k"],
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
