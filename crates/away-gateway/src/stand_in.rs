//! A machine that stands in for a real one, for tests: it takes connections as a
//! Windows machine of a domain does, secures them with a certificate of its own, signs
//! one account in before the desktop, and then shows a desktop that draws a mark for
//! every click and for every key.

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

const CERTIFICATE: &[u8] = include_bytes!("../../away-try/tests/fixtures/machine.der");
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

/// What a stand-in keeps of its connections, for a test to look at.
#[derive(Default)]
struct Kept {
    /// What it saw of each client that came as far as the desktop, the last one last.
    seen: Vec<Seen>,
    /// The desktop's picture as the last connection has it now.
    picture: Option<Picture>,
    /// What each client said first, where it said something: what a farm is told.
    said_first: Vec<Option<Vec<u8>>>,
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

    async fn listen(at: &str, account: &str, password: &str, shows: Picture, sends_on: Option<Redirection>) -> Machine {
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
        let (account, password) = (account.to_string(), password.to_string());
        let keeps = kept.clone();
        let listening = tokio::spawn(async move {
            while let Ok((connection, _)) = listener.accept().await {
                let (secures, keeps, shows, sends_on) = (secures.clone(), keeps.clone(), shows.clone(), sends_on.clone());
                let knows = Account::named(&account, Secret::new(password.clone()));
                tokio::spawn(async move {
                    let _ = serve(connection, secures, knows, shows, sends_on, keeps).await;
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
    knows: Account,
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

    // the sign-in before the desktop: it knows one account, and holds the certificate it showed
    let holds = Identity::read(CERTIFICATE).map_err(std::io::Error::other)?.public_key;
    let mut stand_in = StandIn::new(knows, &holds);
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
        // an account it doesn't know, or a wrong password: it ends the connection
        let answer = stand_in.answer(&message).map_err(std::io::Error::other)?;
        if answer.is_empty() {
            break;
        }
        secured.write_all(&answer).await?;
    }
    secured.write_all(&MAY_COME_IN).await?;

    // then the desktop, for as long as the client stays
    let mut desktop = match sends_on {
        Some(to) => Desktop::new(shows).that_sends_on(to),
        None => Desktop::new(shows),
    };
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
