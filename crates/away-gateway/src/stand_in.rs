//! A machine that stands in for a real one, for tests: it takes connections as a
//! Windows machine of a domain does, secures them with a certificate of its own, signs
//! one account in before the desktop, and then shows a desktop that draws a mark for
//! every click and for every key. It publishes one program, `||ledger`: a client that
//! asks for it is shown the program's window in the place of the desktop.
//!
//! A stand-in that is in a domain knows no password. It takes a ticket for its service,
//! which it opens with the service's key, is handed a smart card, and reads the card
//! over the connection once the desktop begins, with the commands a Windows machine
//! gives one: it has the card sign, as a machine does to sign the person in.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use away_rdp::identity::Identity;
use away_rdp::ntlm::Account;
use away_rdp::picture::Picture;
pub use away_rdp::redirection::Redirection;
use away_rdp::stand_in::desktop::{Desktop, Seen};
use away_rdp::stand_in::StandIn;
use away_rdp::tpkt::{Frame, Frames};
use away_secret::Secret;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;

use away_winauth::kerberos::crypto::Key;

const CERTIFICATE: &[u8] = include_bytes!("../../away-try/tests/fixtures/machine.der");
/// What a stand-in in a domain has the card sign.
pub const TO_SIGN: &[u8] = b"what proves the person to the domain";
const KEY: &[u8] = include_bytes!("../../away-try/tests/fixtures/machine.key.der");

/// What a Windows machine answers when it chooses the sign-in before the desktop, and
/// says whether one may come in before it goes on.
const CHOSEN: [u8; 19] = [0x03, 0x00, 0x00, 0x13, 0x0e, 0xd0, 0x00, 0x00, 0x12, 0x34, 0x00, 0x02, 0x3f, 0x08, 0x00, 0x08, 0x00, 0x00, 0x00];
/// Its verdict on an account that may come in.
const MAY_COME_IN: [u8; 4] = [0, 0, 0, 0];

/// The desktop a stand-in draws unless it is given another: flat parts, slopes, noise
/// and stripes, so that its rectangles hold everything a machine's do. Its size is no
/// multiple of a rectangle's.
pub fn desktop() -> Picture {
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

/// The program a stand-in publishes, as a client asks for it.
pub const PROGRAM: &str = "||ledger";

/// What a stand-in keeps of its connections, for a test to look at.
#[derive(Default)]
struct Kept {
    /// What it saw of each client that came as far as the desktop, the last one last.
    seen: Vec<Seen>,
    /// The desktop's picture as the last connection has it now.
    picture: Option<Picture>,
    /// What each client said first, where it said something: what a farm is told.
    said_first: Vec<Option<Vec<u8>>>,
    /// Whom each ticket was for that a client showed: the account's name and its realm.
    shown: Vec<(String, String)>,
}

/// How a stand-in signs people in before the desktop.
#[derive(Clone)]
enum Takes {
    /// One account, with its name and its password.
    Account(String, String),
    /// A ticket for its service, which it opens with the service's key.
    Tickets(Key),
    /// Either of them, as a machine of a domain does: a ticket for its service, or the
    /// one account it knows, with its name and its password.
    Either(Key, String, String),
}

/// A stand-in for a machine, listening.
pub struct Machine {
    address: SocketAddr,
    kept: Arc<Mutex<Kept>>,
    listening: tokio::task::JoinHandle<()>,
}

impl Machine {
    /// Starts a stand-in at a port of this machine that is free. It knows one account,
    /// and shows this desktop to whoever signs in as it.
    pub async fn start(account: &str, password: &str, shows: Picture) -> Machine {
        Machine::start_at("127.0.0.1:0", account, password, shows).await
    }

    /// A stand-in for a farm's broker: it signs the account in, and sends whoever comes
    /// on to another machine.
    pub async fn start_broker(account: &str, password: &str, sends_on: Redirection) -> Machine {
        Machine::listen("127.0.0.1:0", account, password, desktop(), Some(sends_on)).await
    }

    /// The same, at this address.
    pub async fn start_at(at: &str, account: &str, password: &str, shows: Picture) -> Machine {
        Machine::listen(at, account, password, shows, None).await
    }

    /// A stand-in that is in a domain: it knows no password, takes tickets for its
    /// service, which this is the key of, and reads the card it is handed.
    pub async fn start_in_a_domain(of_the_service: Key, shows: Picture) -> Machine {
        Machine::listening("127.0.0.1:0", Takes::Tickets(of_the_service), shows, None).await
    }

    /// A stand-in that is in a domain and knows an account besides, as a machine of a
    /// domain does: it takes a ticket for its service and reads the card it is handed,
    /// or signs that account in with its name and its password.
    pub async fn start_in_a_domain_that_knows(of_the_service: Key, account: &str, password: &str, shows: Picture) -> Machine {
        let takes = Takes::Either(of_the_service, account.to_string(), password.to_string());
        Machine::listening("127.0.0.1:0", takes, shows, None).await
    }

    /// A stand-in for a farm's broker that is in a domain: it takes a ticket, and sends
    /// whoever comes on to another machine.
    pub async fn start_broker_in_a_domain(of_the_service: Key, sends_on: Redirection) -> Machine {
        Machine::listening("127.0.0.1:0", Takes::Tickets(of_the_service), desktop(), Some(sends_on)).await
    }

    async fn listen(at: &str, account: &str, password: &str, shows: Picture, sends_on: Option<Redirection>) -> Machine {
        Machine::listening(at, Takes::Account(account.to_string(), password.to_string()), shows, sends_on).await
    }

    async fn listening(at: &str, takes: Takes, shows: Picture, sends_on: Option<Redirection>) -> Machine {
        let listener = TcpListener::bind(at).await.expect("an address for the stand-in machine");
        let address = listener.local_addr().expect("its address");
        let kept = Arc::new(Mutex::new(Kept::default()));
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .expect("versions")
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(CERTIFICATE.to_vec())],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(KEY.to_vec())),
            )
            .expect("the stand-in's certificate");
        let secures = TlsAcceptor::from(Arc::new(config));
        let keeps = kept.clone();
        let listening = tokio::spawn(async move {
            while let Ok((connection, _)) = listener.accept().await {
                let (secures, keeps, shows, sends_on, takes) =
                    (secures.clone(), keeps.clone(), shows.clone(), sends_on.clone(), takes.clone());
                tokio::spawn(async move {
                    let _ = serve(connection, secures, takes, shows, sends_on, keeps).await;
                });
            }
        });
        Machine { address, kept, listening }
    }

    /// Where it listens: a name of this machine, and its port.
    pub fn address(&self) -> (String, u16) {
        ("localhost".to_string(), self.address.port())
    }

    /// The fingerprint of the certificate it shows, as Away writes one.
    pub fn fingerprint(&self) -> String {
        Identity::read(CERTIFICATE).expect("the stand-in's certificate").fingerprint_text()
    }

    /// What it saw of each client that came as far as the desktop, the last one last.
    pub fn seen(&self) -> Vec<Seen> {
        self.kept.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).seen.clone()
    }

    /// The desktop's picture as the last connection has it now, with the marks it drew.
    pub fn picture(&self) -> Option<Picture> {
        self.kept.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).picture.clone()
    }

    /// What each client said first, where it said something, the last one last.
    pub fn said_first(&self) -> Vec<Option<Vec<u8>>> {
        self.kept.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).said_first.clone()
    }

    /// Whom each ticket was for that a client showed: the account's name and its realm.
    pub fn shown(&self) -> Vec<(String, String)> {
        self.kept.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).shown.clone()
    }

    /// Stops listening.
    pub fn stop(&self) {
        self.listening.abort();
    }
}

impl Drop for Machine {
    fn drop(&mut self) {
        self.listening.abort();
    }
}

/// One connection, for as long as the client stays.
async fn serve(
    mut connection: TcpStream,
    secures: TlsAcceptor,
    takes: Takes,
    shows: Picture,
    sends_on: Option<Redirection>,
    keeps: Arc<Mutex<Kept>>,
) -> std::io::Result<()> {
    // the first request, and the answer of a machine that signs in before the desktop
    let mut frames = Frames::new();
    let request = loop {
        if let Some(request) = frames.next_frame().map_err(std::io::Error::other)? {
            break request;
        }
        let mut bytes = [0u8; 512];
        match connection.read(&mut bytes).await? {
            0 => return Ok(()),
            count => frames.push(&bytes[..count]),
        }
    };
    keeps.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).said_first.push(away_rdp::negotiation::read_token(&request));
    connection.write_all(&CHOSEN).await?;
    let mut secured = secures.accept(connection).await?;

    // the sign-in before the desktop: it knows one account, or takes a ticket, and holds
    // the certificate it showed
    let holds = Identity::read(CERTIFICATE).map_err(std::io::Error::other)?.public_key;
    let (mut with_a_password, mut with_a_ticket) = match takes {
        Takes::Account(account, password) => (Some(StandIn::new(Account::named(&account, Secret::new(password)), &holds)), None),
        Takes::Tickets(of_the_service) => (None, Some(away_winauth::sign_in::stand_in::Machine::new(of_the_service, &holds))),
        Takes::Either(of_the_service, account, password) => (
            Some(StandIn::new(Account::named(&account, Secret::new(password)), &holds)),
            Some(away_winauth::sign_in::stand_in::Machine::new(of_the_service, &holds)),
        ),
    };
    loop {
        let mut message = Vec::new();
        loop {
            match away_rdp::der::whole(&message) {
                Ok(Some(length)) if message.len() >= length => break,
                Ok(_) => {}
                Err(error) => return Err(std::io::Error::other(error)),
            }
            let mut bytes = [0u8; 4096];
            match secured.read(&mut bytes).await? {
                0 => return Ok(()),
                count => message.extend_from_slice(&bytes[..count]),
            }
        }
        // a machine that takes either tells them apart by the first message: the way of
        // signing in that needs no directory names itself in it
        if with_a_password.is_some() && with_a_ticket.is_some() {
            match message.windows(8).any(|part| part == b"NTLMSSP\0") {
                true => with_a_ticket = None,
                false => with_a_password = None,
            }
        }
        // an account it doesn't know, a wrong password, a ticket that is not for its
        // service: it ends the connection
        let answer = match (&mut with_a_password, &mut with_a_ticket) {
            (Some(stand_in), _) => stand_in.answer(&message).map_err(std::io::Error::other)?,
            (_, Some(machine)) => machine.answer(&message).map_err(std::io::Error::other)?,
            _ => return Ok(()),
        };
        if answer.is_empty() {
            break;
        }
        secured.write_all(&answer).await?;
    }
    secured.write_all(&MAY_COME_IN).await?;

    // then the desktop, for as long as the client stays
    let mut desktop = match sends_on {
        Some(to) => Desktop::new(shows).that_publishes(&[PROGRAM]).that_sends_on(to),
        None => Desktop::new(shows).that_publishes(&[PROGRAM]),
    };
    // a machine that was handed a smart card reads it once the desktop begins, as a
    // Windows machine does to sign the person in: with the PIN it was handed
    if let Some(machine) = &with_a_ticket {
        if let Some(shown) = machine.account.clone() {
            keeps.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).shown.push(shown);
        }
        if let Some(card) = machine.handed_over.as_deref().and_then(away_winauth::sign_in::stand_in::on_a_card) {
            let hash = away_winauth::card::stand_in::hash_of(TO_SIGN);
            desktop = desktop.that_reads_a_card(&away_winauth::card::stand_in::commands(card.pin.expose(), &hash));
        }
    }
    let mut frames = Frames::new();
    let nth = {
        let mut kept = keeps.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        kept.seen.push(desktop.seen().clone());
        kept.picture = Some(desktop.picture().clone());
        kept.seen.len() - 1
    };
    loop {
        while let Some(frame) = frames.next_of_either_kind().map_err(std::io::Error::other)? {
            let answer = match frame {
                Frame::Slow(carried) => desktop.answer(&carried),
                Frame::Fast(head, carried) => desktop.input(head, &carried),
            }
            .map_err(std::io::Error::other)?;
            {
                let mut kept = keeps.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                kept.seen[nth] = desktop.seen().clone();
                kept.picture = Some(desktop.picture().clone());
            }
            secured.write_all(&answer).await?;
        }
        let mut bytes = [0u8; 4096];
        match secured.read(&mut bytes).await? {
            0 => return Ok(()),
            count => frames.push(&bytes[..count]),
        }
    }
}
