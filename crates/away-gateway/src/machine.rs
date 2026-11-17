//! One machine's desktop, as the gateway holds it: reached, secured, signed in and
//! opened with Away's own RDP code, and then drawn into a picture of the gateway's own
//! ([ADR-0004](../../../spec/architecture/decisions/0004-the-session-and-its-picture.md)).
//!
//! The steps are the ones a person sees while a machine opens. Each can fail, and says
//! at which step and why.

use std::sync::Arc;
use std::time::Duration;

pub use away_rdp::clipboard::{File as CopiedFile, Passes, Picture as CopiedPicture, LONGEST_PIECE};
use away_rdp::connection::{Connection, Happened};
use away_rdp::gcc::Client;
use away_rdp::identity::Identity;
use away_rdp::input::Input;
use away_rdp::negotiation::{self, Security};
use away_rdp::ntlm::{Account, Fresh};
use away_rdp::picture::Picture;
use away_rdp::rail::{Command, Program};
use away_rdp::redirection::Redirection;
use away_rdp::sign_in::{self, New, Next, SignIn};
use away_rdp::tpkt::Frames;
use away_rdp::{der, licence};
use away_wire::{Did, Does};
use tokio::io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;

use crate::shown::{Pixels, Shown};
use crate::themselves::OnAMachine;
use crate::windows::Windows;

/// How long a machine gets to answer, at each step on the way to its desktop.
pub const WITHIN: Duration = Duration::from_secs(10);
/// How long a machine gets to show its first picture, once it took the sign-in.
pub const FIRST_PICTURE_WITHIN: Duration = Duration::from_secs(30);
/// How long a machine gets to say that a program started, once it took the sign-in: a
/// person's first sign-in on a machine that serves programs takes its time.
pub const PROGRAM_STARTS_WITHIN: Duration = Duration::from_secs(120);

/// The steps of a connection, as a person sees them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Finding the machine at its address and connecting to it.
    Reaching,
    /// Comparing what the machine shows of itself with what Away knows of it.
    Checking,
    /// Signing the person in on the machine.
    SigningIn,
    /// Waiting until the machine shows something.
    FirstPicture,
}

/// Why a step failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// The machine's name leads to no address.
    NoAddress,
    /// It gave no answer in time.
    NoAnswer,
    /// It refused the connection.
    Refused,
    /// It shows another identity than the one Away knows.
    AnotherIdentity,
    /// It did not accept the sign-in.
    SignInRefused,
    /// The person could not be signed in as themselves: Away got nothing that signs
    /// them in, or the domain did not take it.
    NotThemselves,
    /// It took the sign-in and showed nothing in time.
    NoPicture,
    /// A farm's broker sent the connection to a machine the files don't name for it.
    SentElsewhere,
    /// The machine did not start the program.
    ProgramNotStarted,
    /// The machine ended the session because the same account's desktop was opened
    /// over another connection: in another window, or on another device.
    OpenedElsewhere,
    /// The connection broke, or the machine said something Away can't go on with.
    Broken,
}

/// A step that failed: which, why, and the technical reason for whoever is asked for help.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    /// The step.
    pub step: Step,
    /// Why.
    pub why: Why,
    /// The technical reason.
    pub details: String,
}

pub(crate) fn failure(step: Step, why: Why, details: impl ToString) -> Failure {
    Failure { step, why, details: details.to_string() }
}

/// The screen a desktop is opened for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Screen {
    /// How wide the desktop is, in the screen's own dots.
    pub width: u16,
    /// How high.
    pub height: u16,
    /// How much larger than usual the machine draws there, in percent.
    pub scale: u32,
}

/// The most dots a desktop has: those of a screen of 3840 by 2160. Every one of them is
/// read, compared and sent.
const MOST_DOTS: u32 = 3840 * 2160;
/// The most dots a desktop has along one side, which is what a Windows machine takes.
const MOST_A_SIDE: u32 = 8192;

impl Screen {
    /// A screen of this size, on which the machine draws at its usual size.
    pub fn of(width: u16, height: u16) -> Screen {
        Screen { width, height, scale: 100 }
    }

    /// The screen for a window of this size, as a page measures it, on a device that
    /// shows this many dots for a hundred of the page's. The desktop has the device's
    /// dots, and the machine draws that much larger: text keeps the size it has at the
    /// machine, and is as sharp as the device shows it. Where that would be more dots
    /// than a desktop has, the machine draws less large, in steps of a quarter.
    pub fn for_a_window(width: u16, height: u16, dots: u32) -> Screen {
        let (width, height) = (u32::from(width.clamp(640, 4096)), u32::from(height.clamp(480, 2160)));
        let fits = |scale: u32| {
            let (wide, high) = (width * scale / 100, height * scale / 100);
            wide <= MOST_A_SIDE && high <= MOST_A_SIDE && wide * high <= MOST_DOTS
        };
        let mut scale = dots.clamp(100, 500);
        while scale > 100 && !fits(scale) {
            scale = (scale - 1) / 25 * 25;
        }
        let scale = scale.max(100);
        // a desktop's width is a multiple of four
        Screen { width: (width * scale / 100) as u16 & !3, height: (height * scale / 100) as u16, scale }
    }
}

/// Who is signed in on a machine.
pub enum Who {
    /// An account, with a name and a password that somebody typed.
    Account(Account),
    /// A scoped Kerberos service ticket, with no delegated password or smart card.
    RestrictedAdmin(away_winauth::kerberos::exchange::Ticket),
    /// The person themselves: with a ticket of their domain for this machine, and a
    /// smart card that exists in the gateway's memory alone.
    Themselves(Box<OnAMachine>),
}

/// A machine that answered, and said which security it takes.
pub struct Reached {
    host: String,
    connection: TcpStream,
    security: Security,
    restricted: bool,
}

/// Finds a machine at its address, connects to it, and asks which security it takes.
/// A farm is told something first: its broker which collection is wanted, and the
/// machine a broker sent the client on to what the broker gave for it.
pub async fn reach(host: &str, port: u16, says_first: Option<&[u8]>) -> Result<Reached, Failure> {
    reach_mode(host, port, says_first, false).await
}

/// Reaches a direct machine requiring Restricted Admin; no security downgrade.
pub async fn reach_restricted_admin(host: &str, port: u16) -> Result<Reached, Failure> {
    reach_mode(host, port, None, true).await
}

/// Finds a machine at its address and connects to it.
pub(crate) async fn connect(host: &str, port: u16) -> Result<TcpStream, Failure> {
    // a number of the newer kind is written in brackets, which are not part of it
    let written = host.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')).unwrap_or(host);
    let found = tokio::net::lookup_host((written, port))
        .await
        .map_err(|error| failure(Step::Reaching, Why::NoAddress, format!("{host} leads to no address: {error}")))?;
    // a name may lead to several addresses, and the machine may answer on one of them alone
    let mut connection = Err(failure(Step::Reaching, Why::NoAddress, format!("{host} leads to no address")));
    for to in found {
        connection = TcpStream::connect(to).await.map_err(|error| match error.kind() {
            std::io::ErrorKind::ConnectionRefused => failure(Step::Reaching, Why::Refused, format!("{to} refused the connection")),
            std::io::ErrorKind::TimedOut => failure(Step::Reaching, Why::NoAnswer, format!("{to} gave no answer")),
            _ => failure(Step::Reaching, Why::Broken, format!("{to} could not be reached: {error}")),
        });
        if connection.is_ok() {
            break;
        }
    }
    let connection = connection?;
    let _ = connection.set_nodelay(true);
    Ok(connection)
}

async fn reach_mode(host: &str, port: u16, says_first: Option<&[u8]>, restricted: bool) -> Result<Reached, Failure> {
    let reaching = async {
        let mut connection = connect(host, port).await?;
        let broken = |error: std::io::Error| failure(Step::Reaching, Why::Broken, format!("the first request: {error}"));
        let request = if restricted {
            negotiation::request_restricted_admin()
        } else {
            negotiation::request_with(says_first).map_err(|error| failure(Step::Reaching, Why::Broken, error))?
        };
        connection.write_all(&request).await.map_err(broken)?;
        let mut frames = Frames::new();
        let carried = loop {
            let not_rdp = |error: away_rdp::Error| failure(Step::Reaching, Why::Broken, error);
            if let Some(carried) = frames.next_frame().map_err(not_rdp)? {
                break carried;
            }
            let mut bytes = [0u8; 512];
            match connection.read(&mut bytes).await.map_err(broken)? {
                0 => return Err(failure(Step::Reaching, Why::Refused, "it closed the connection without an answer")),
                count => frames.push(&bytes[..count]),
            }
        };
        let answer = negotiation::read_answer(&carried).map_err(|error| failure(Step::Reaching, Why::Refused, error))?;
        if restricted && (!answer.restricted_admin || answer.security == Security::Tls) {
            return Err(failure(Step::SigningIn, Why::SignInRefused, "the machine did not accept Restricted Admin with CredSSP"));
        }
        Ok(Reached { host: host.to_string(), connection, security: answer.security, restricted })
    };
    match tokio::time::timeout(WITHIN, reaching).await {
        Ok(reached) => reached,
        Err(_) => Err(failure(Step::Reaching, Why::NoAnswer, format!("no answer within {} seconds", WITHIN.as_secs()))),
    }
}

/// A machine whose connection is secured, and which showed who it is.
pub struct Secured {
    host: String,
    secured: TlsStream<TcpStream>,
    security: Security,
    restricted: bool,
    identity: Identity,
    /// Which ways the clipboard passes on the desktop that is opened over it.
    clipboard: Passes,
}

impl Reached {
    /// Secures the connection, and reads what the machine shows of itself. A machine is
    /// known by the certificate it shows, not by who signed it: whoever calls compares
    /// it with what Away knows.
    pub async fn secure(self) -> Result<Secured, Failure> {
        let broken = |details: String| failure(Step::Checking, Why::Broken, details);
        let config = away_part::client::encrypting_unchecked(b"").map_err(broken)?;
        // a machine that is reached by a number has no name a certificate could be for
        let name = rustls::pki_types::ServerName::try_from(self.host.clone())
            .map_err(|_| broken(format!("{} is no name a connection can be secured for", self.host)))?;
        let securing = TlsConnector::from(Arc::new(config)).connect(name, self.connection);
        let secured = match tokio::time::timeout(WITHIN, securing).await {
            Ok(Ok(secured)) => secured,
            Ok(Err(error)) => return Err(broken(format!("the connection could not be secured: {error}"))),
            Err(_) => return Err(failure(Step::Checking, Why::NoAnswer, "the connection was not secured in time")),
        };
        let shown = secured
            .get_ref()
            .1
            .peer_certificates()
            .and_then(|all| all.first())
            .ok_or_else(|| broken("it showed no certificate".to_string()))?;
        let identity = Identity::read(shown.as_ref()).map_err(|error| broken(error.to_string()))?;
        Ok(Secured { host: self.host, secured, security: self.security, restricted: self.restricted, identity, clipboard: Passes::NEITHER })
    }
}

impl Secured {
    /// What the machine shows of itself.
    pub fn identity(&self) -> &Identity {
        &self.identity
    }

    /// The same, with the clipboard passing these ways on the desktop that is opened
    /// over it. It passes neither way unless this says so.
    pub fn passing(self, clipboard: Passes) -> Secured {
        Secured { clipboard, ..self }
    }

    /// Whether the machine signs people in itself, on its own screen, once the desktop
    /// is open: it then takes any name and asks there.
    pub fn signs_in_itself(&self) -> bool {
        self.security == Security::Tls
    }

    /// Signs somebody in, and opens the desktop for this screen, for a keyboard of this
    /// layout, as Windows numbers them. On the machine a farm's broker sent the client
    /// on to, the session the broker named is said. A broker opens nothing: it says
    /// where the connection belongs. With a program, no desktop is opened: the machine
    /// is asked to start the program, and shows its windows.
    pub async fn sign_in(
        mut self,
        who: Who,
        screen: Screen,
        layout: u32,
        sent_on_to: Option<u32>,
        program: Option<Program>,
    ) -> Result<Opened, Failure> {
        if self.restricted != matches!(&who, Who::RestrictedAdmin(_)) {
            return Err(failure(Step::SigningIn, Why::SignInRefused, "the sign-in does not match the negotiated Restricted Admin mode"));
        }
        let signing_in = async {
            let random =
                |step: Step| move |error: getrandom::Error| failure(step, Why::Broken, format!("the system gave no random bytes: {error}"));
            let refused = |details: String| failure(Step::SigningIn, Why::SignInRefused, details);
            let broken = |error: std::io::Error| failure(Step::SigningIn, Why::Broken, error);
            let (domain, name, password, reader) = match who {
                // the machine signs the account in itself, with what the client tells it
                Who::Account(account) if self.security == Security::Tls => (account.domain, account.name, Some(account.password), None),
                Who::Themselves(_) if self.security == Security::Tls => {
                    return Err(failure(
                        Step::SigningIn,
                        Why::NotThemselves,
                        "the machine takes no sign-in before the desktop, which signing a person in as themselves goes through",
                    ))
                }
                Who::Themselves(theirs) => {
                    // the ticket is shown in place of a proof of a password, and the card is
                    // handed over in place of the password itself, sealed
                    let OnAMachine { ticket, card, reader } = *theirs;
                    let new = away_winauth::sign_in::New {
                        for_the_ticket: away_winauth::kerberos::gss::Fresh {
                            key: random_bytes(Step::SigningIn)?,
                            begins_with: random_bytes(Step::SigningIn)?,
                            counts_from: u32::from_be_bytes(random_bytes(Step::SigningIn)?) & 0x3fff_ffff,
                        },
                        number: random_bytes(Step::SigningIn)?,
                        begin_with: [random_bytes(Step::SigningIn)?, random_bytes(Step::SigningIn)?],
                    };
                    let now = away_part::time::now();
                    let (mut signing_in, first) = away_winauth::sign_in::SignIn::begin_with_a_card(
                        &ticket,
                        &self.identity.public_key,
                        &card,
                        (now.as_secs(), now.subsec_micros()),
                        new,
                    );
                    let not = |details: String| failure(Step::SigningIn, Why::NotThemselves, details);
                    self.secured.write_all(&first).await.map_err(broken)?;
                    loop {
                        let message =
                            message(&mut self.secured).await.map_err(|error| not(format!("the machine ended the sign-in: {error}")))?;
                        match signing_in.receive(&message).map_err(|error| not(error.to_string()))? {
                            Next::Send(bytes) => self.secured.write_all(&bytes).await.map_err(broken)?,
                            Next::SendLast(bytes) => {
                                self.secured.write_all(&bytes).await.map_err(broken)?;
                                break;
                            }
                        }
                    }
                    if self.security == (Security::SignInFirst { tells_result: true }) {
                        let mut verdict = [0u8; 4];
                        self.secured.read_exact(&mut verdict).await.map_err(|_| {
                            not("the machine closed the connection where it says whether the person may come in".to_string())
                        })?;
                        sign_in::read_verdict(verdict).map_err(|error| not(error.to_string()))?;
                    }
                    (card.domain, card.name, None, Some(reader))
                }
                Who::RestrictedAdmin(ticket) => {
                    // Only a ticket is presented; every credential field is empty.

                    let new = away_winauth::sign_in::New {
                        for_the_ticket: away_winauth::kerberos::gss::Fresh {
                            key: random_bytes(Step::SigningIn)?,
                            begins_with: random_bytes(Step::SigningIn)?,
                            counts_from: u32::from_be_bytes(random_bytes(Step::SigningIn)?) & 0x3fff_ffff,
                        },
                        number: random_bytes(Step::SigningIn)?,
                        begin_with: [random_bytes(Step::SigningIn)?, random_bytes(Step::SigningIn)?],
                    };
                    let now = away_part::time::now();
                    let (mut signing_in, first) = away_winauth::sign_in::SignIn::begin_restricted_admin(
                        &ticket,
                        &self.identity.public_key,
                        (now.as_secs(), now.subsec_micros()),
                        new,
                    );
                    let not = |details: String| failure(Step::SigningIn, Why::NotThemselves, details);
                    self.secured.write_all(&first).await.map_err(broken)?;
                    loop {
                        let message =
                            message(&mut self.secured).await.map_err(|error| not(format!("the machine ended the sign-in: {error}")))?;
                        match signing_in.receive(&message).map_err(|error| not(error.to_string()))? {
                            Next::Send(bytes) => self.secured.write_all(&bytes).await.map_err(broken)?,
                            Next::SendLast(bytes) => {
                                self.secured.write_all(&bytes).await.map_err(broken)?;
                                break;
                            }
                        }
                    }
                    if self.security == (Security::SignInFirst { tells_result: true }) {
                        let mut verdict = [0u8; 4];
                        self.secured.read_exact(&mut verdict).await.map_err(|_| {
                            not("the machine closed the connection where it says whether the person may come in".to_string())
                        })?;
                        sign_in::read_verdict(verdict).map_err(|error| not(error.to_string()))?;
                    }
                    (String::new(), String::new(), None, None)
                }
                Who::Account(account) => {
                    let (domain, name) = (account.domain.clone(), account.name.clone());
                    let mut fresh = [0u8; 56];
                    getrandom::fill(&mut fresh).map_err(random(Step::SigningIn))?;
                    let new = New {
                        for_the_proof: Fresh {
                            challenge: fresh[..8].try_into().expect("8 bytes"),
                            key: fresh[8..24].try_into().expect("16 bytes"),
                        },
                        number: fresh[24..].try_into().expect("32 bytes"),
                    };
                    let (mut signing_in, first) = SignIn::begin(account, &self.host, &self.identity.public_key, new, clock());
                    self.secured.write_all(&first).await.map_err(broken)?;
                    loop {
                        // a machine that doesn't take the name or the password ends the connection here
                        let message =
                            message(&mut self.secured).await.map_err(|error| refused(format!("the machine ended the sign-in: {error}")))?;
                        match signing_in.receive(&message).map_err(|error| refused(error.to_string()))? {
                            Next::Send(bytes) => self.secured.write_all(&bytes).await.map_err(broken)?,
                            Next::SendLast(bytes) => {
                                self.secured.write_all(&bytes).await.map_err(broken)?;
                                break;
                            }
                        }
                    }
                    if self.security == (Security::SignInFirst { tells_result: true }) {
                        let mut verdict = [0u8; 4];
                        self.secured.read_exact(&mut verdict).await.map_err(|_| {
                            refused("the machine closed the connection where it says whether the account may come in".to_string())
                        })?;
                        sign_in::read_verdict(verdict).map_err(|error| refused(error.to_string()))?;
                    }
                    (domain, name, None, None)
                }
            };

            // what Away is, the channels, the licence, and what each side can do
            let client = Client {
                width: screen.width,
                height: screen.height,
                scale: screen.scale,
                security: self.security.number(),
                name: "away".to_string(),
                layout,
                sent_on_to,
                program,
                clipboard: self.clipboard,
            };
            let mut fresh = [0u8; 80];
            getrandom::fill(&mut fresh).map_err(random(Step::SigningIn))?;
            let for_a_licence = licence::New {
                random: fresh[..32].try_into().expect("32 bytes"),
                secret: fresh[32..].try_into().expect("48 bytes"),
                known_by: known_by(),
            };
            let reads_a_card = reader.is_some();
            let (open, first) = Connection::begin(client, &domain, &name, password, for_a_licence, reader);
            self.secured.write_all(&first).await.map_err(broken)?;
            let (reader, writer) = tokio::io::split(self.secured);
            let mut desktop = Desktop {
                reader,
                writer,
                frames: Frames::new(),
                open,
                picture: None,
                is_open: false,
                sent_on: None,
                drawn: Vec::new(),
                windows: Windows::default(),
                windows_changed: false,
                started: false,
                started_later: Vec::new(),
                scratch: vec![0u8; 32 * 1024],
                reads_a_card,
                ticked: tokio::time::Instant::now(),
                resized: false,
                copied: None,
                copied_picture: None,
                copied_files: None,
                piece: None,
            };
            while !desktop.is_open {
                // a machine that reads a card waits for the reader's word as time passes
                let arrived = tokio::time::timeout(TICKS_EVERY, desktop.reader.read(&mut desktop.scratch)).await;
                desktop.tick().await.map_err(|failed| Failure { step: Step::SigningIn, ..failed })?;
                match arrived {
                    Err(_) => continue,
                    // a machine that refuses the account once the desktop is asked for ends the connection
                    Ok(Ok(0)) => return Err(refused("the machine closed the connection before its desktop was open".to_string())),
                    Ok(Ok(count)) => desktop.frames.push(&desktop.scratch[..count]),
                    Ok(Err(error)) => return Err(broken(error)),
                }
                // what the machine draws along with its last word is kept for whoever takes it first
                desktop.draw().await.map_err(|failed| Failure { step: Step::SigningIn, ..failed })?;
                if let Some(to) = desktop.sent_on.take() {
                    return Ok(Opened::SentOn(to));
                }
            }
            Ok(Opened::Desktop(Box::new(desktop)))
        };
        match tokio::time::timeout(WITHIN * 3, signing_in).await {
            Ok(desktop) => desktop,
            Err(_) => Err(failure(Step::SigningIn, Why::NoAnswer, "the sign-in was not answered in time")),
        }
    }
}

/// What signing in on a machine gave.
pub enum Opened {
    /// Its desktop.
    Desktop(Box<Desktop>),
    /// A farm's word: the machine is a broker, and the connection belongs elsewhere.
    SentOn(Redirection),
}

/// An area of the picture that the machine drew anew.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    /// Its left edge, in pixels.
    pub left: usize,
    /// Its top edge.
    pub top: usize,
    /// How wide it is.
    pub width: usize,
    /// How high.
    pub height: usize,
}

/// A desktop that is open: the gateway's picture of it, and the connection it comes over.
pub struct Desktop {
    reader: ReadHalf<TlsStream<TcpStream>>,
    writer: WriteHalf<TlsStream<TcpStream>>,
    frames: Frames,
    open: Connection,
    picture: Option<Picture>,
    is_open: bool,
    /// Where a broker sends the connection, where the machine turned out to be one.
    sent_on: Option<Redirection>,
    /// The areas that were drawn and not yet taken.
    drawn: Vec<Area>,
    /// The windows of the programs, where the session is for a program.
    windows: Windows,
    /// Whether the machine said something of the windows since they were last asked for.
    windows_changed: bool,
    /// Whether the machine said that the program started.
    started: bool,
    /// Whether each program asked for since the first started, in the order they were asked for.
    started_later: Vec<Result<(), String>>,
    scratch: Vec<u8>,
    /// Whether the machine was handed a smart card, which it reads over the connection.
    reads_a_card: bool,
    /// When the connection was last told how much time passed.
    ticked: tokio::time::Instant,
    /// Whether the machine offered its desktop anew, at another size, since that was last asked.
    resized: bool,
    /// The text that was last copied on the machine, until it is taken.
    copied: Option<String>,
    /// The picture that was last copied on the machine, until it is taken.
    copied_picture: Option<CopiedPicture>,
    /// The files that were last copied on the machine, until they are taken.
    copied_files: Option<Vec<CopiedFile>>,
    /// The piece of a file that arrived, until it is taken: none where the machine gave none.
    piece: Option<Option<Vec<u8>>>,
}

impl Desktop {
    /// The desktop's picture, as it is on the machine as far as the machine has drawn it.
    pub fn picture(&self) -> &Picture {
        self.picture.as_ref().expect("a desktop that is open has a picture")
    }

    /// Waits for the machine to send something, and takes it in. It can be given up at
    /// any moment without anything being lost: what arrived is kept.
    pub async fn arrives(&mut self) -> Result<(), Failure> {
        match self.reader.read(&mut self.scratch).await {
            Ok(0) => Err(failure(Step::FirstPicture, Why::Broken, "the machine closed the connection")),
            Ok(count) => {
                self.frames.push(&self.scratch[..count]);
                Ok(())
            }
            Err(error) => Err(failure(Step::FirstPicture, Why::Broken, format!("the connection to the machine broke: {error}"))),
        }
    }

    /// Tells the connection how much time passed since it was last told: a machine
    /// that asked the smart card reader to say when something changes gets its answer
    /// when the time it gave is over.
    pub async fn tick(&mut self) -> Result<(), Failure> {
        if !self.reads_a_card {
            return Ok(());
        }
        let now = tokio::time::Instant::now();
        let passed = now.duration_since(self.ticked).as_millis() as u64;
        if passed == 0 {
            return Ok(());
        }
        self.ticked = now;
        let waited = self.open.elapsed(passed);
        if waited.is_empty() {
            return Ok(());
        }
        self.write(&waited).await
    }

    /// Writes to the machine. Where that fails, the machine may have ended the connection
    /// and said why just before: what it sent is read first, so that its reason is not
    /// lost to the write that failed.
    async fn write(&mut self, bytes: &[u8]) -> Result<(), Failure> {
        let Err(error) = self.writer.write_all(bytes).await else { return Ok(()) };
        // what is on its way from the machine, until it has said everything
        while let Ok(Ok(count)) = tokio::time::timeout(LAST_WORDS_WITHIN, self.reader.read(&mut self.scratch)).await {
            if count == 0 {
                break;
            }
            self.frames.push(&self.scratch[..count]);
        }
        while let Ok(Some(frame)) = self.frames.next_of_either_kind() {
            match self.open.receive(frame) {
                Ok(_) => {}
                Err(away_rdp::Error::Ended(OPENED_ELSEWHERE)) => {
                    return Err(failure(Step::FirstPicture, Why::OpenedElsewhere, away_rdp::Error::Ended(OPENED_ELSEWHERE)))
                }
                Err(said) => return Err(failure(Step::FirstPicture, Why::Broken, said)),
            }
        }
        Err(failure(Step::FirstPicture, Why::Broken, format!("the connection to the machine broke: {error}")))
    }

    /// Draws what arrived into the picture, answers the machine where it wants an
    /// answer, and says which areas of the picture are new since this was last asked.
    pub async fn take(&mut self) -> Result<Vec<Area>, Failure> {
        self.draw().await?;
        Ok(std::mem::take(&mut self.drawn))
    }

    /// Draws what arrived into the picture, and answers the machine where it wants an answer.
    async fn draw(&mut self) -> Result<(), Failure> {
        let broken = |details: String| failure(Step::FirstPicture, Why::Broken, details);
        while let Some(frame) = self.frames.next_of_either_kind().map_err(|error| broken(error.to_string()))? {
            let (send, happened) = self.open.receive(frame).map_err(|error| match error {
                // the machine said why it ends the connection: the same desktop was
                // opened over another connection, which a desktop is taken along by
                away_rdp::Error::Ended(OPENED_ELSEWHERE) => failure(Step::FirstPicture, Why::OpenedElsewhere, error),
                other => broken(other.to_string()),
            })?;
            if !send.is_empty() {
                self.write(&send).await?;
            }
            for happened in happened {
                match happened {
                    Happened::Desktop { width, height, .. } => {
                        // a machine that offers its desktop anew draws all of it anew:
                        // what was drawn on the one before is of no use
                        if self.is_open {
                            self.resized = true;
                            self.drawn.clear();
                        }
                        self.picture = Some(Picture::new(usize::from(width), usize::from(height)));
                    }
                    Happened::Open => self.is_open = true,
                    Happened::Drawn(rectangles) => {
                        let picture = self
                            .picture
                            .as_mut()
                            .ok_or_else(|| broken("the machine draws before it said how large its desktop is".to_string()))?;
                        for rectangle in &rectangles {
                            picture.draw(rectangle).map_err(|error| broken(error.to_string()))?;
                            let (left, top) = (usize::from(rectangle.left), usize::from(rectangle.top));
                            let right = (usize::from(rectangle.right) + 1).min(picture.width);
                            let bottom = (usize::from(rectangle.bottom) + 1).min(picture.height);
                            if left < right && top < bottom {
                                self.drawn.push(Area { left, top, width: right - left, height: bottom - top });
                            }
                        }
                    }
                    Happened::SentOn(to) => self.sent_on = Some(to),
                    // of what was copied on the machine, the newest is for the person:
                    // text, or a picture
                    Happened::Copied(text) => {
                        self.copied = Some(text);
                        self.copied_picture = None;
                    }
                    Happened::CopiedPicture(picture) => {
                        self.copied_picture = Some(picture);
                        self.copied = None;
                    }
                    Happened::CopiedFiles(files) => self.copied_files = Some(files),
                    // a piece of the file that is being had, or that the machine gave none
                    Happened::PieceOfAFile(piece) => self.piece = Some(Some(piece)),
                    Happened::NoPieceOfAFile => self.piece = Some(None),
                    Happened::Windows(changes) => {
                        self.windows.take(changes);
                        self.windows_changed = true;
                    }
                    // a program asked for in a session that runs already is said not to
                    // have started, and the session goes on
                    Happened::NotStarted { result, raw } => {
                        let details = format!("{} (its own code {raw:#010x})", away_rdp::rail::why(result));
                        if self.started {
                            self.started_later.push(Err(details));
                        } else {
                            return Err(failure(Step::FirstPicture, Why::ProgramNotStarted, details));
                        }
                    }
                    Happened::Started if self.started => self.started_later.push(Ok(())),
                    Happened::Started => self.started = true,
                    Happened::Channels { .. } | Happened::Licensed | Happened::AsksToTakeALicence { .. } | Happened::LicenceTaken(_) => {}
                    Happened::AskedForTheProgram => {}
                    // what a machine does with the reader is answered by the connection itself
                    Happened::Reader { .. } | Happened::AskedTheReader(_) => {}
                }
            }
        }
        Ok(())
    }

    /// The windows of the programs from the front one back, and the one in front, where
    /// the machine said something of them since this was last asked.
    pub fn windows(&mut self) -> Option<(Vec<away_wire::Window>, Option<u32>)> {
        std::mem::take(&mut self.windows_changed).then(|| self.windows.told())
    }

    /// Whether the machine said that the program started, where the session is for one.
    pub fn started(&self) -> bool {
        self.started
    }

    /// Asks the machine for another program, in the session that runs one.
    pub async fn start(&mut self, program: &Program) -> Result<(), Failure> {
        let told = self.open.start(program).map_err(|error| failure(Step::FirstPicture, Why::Broken, error.to_string()))?;
        self.write(&told).await
    }

    /// Whether each program asked for since the first started, in the order they were
    /// asked for, where the machine said so since this was last asked.
    pub fn started_later(&mut self) -> Vec<Result<(), String>> {
        std::mem::take(&mut self.started_later)
    }

    /// The icons of windows that changed since this was last asked, as PNG pictures.
    pub fn icons(&mut self) -> Vec<(u32, Vec<u8>)> {
        self.windows.icons()
    }

    /// The icons of the tray, where it changed since this was last asked.
    pub fn tray(&mut self) -> Option<Vec<away_wire::TrayIcon>> {
        self.windows.tray()
    }

    /// The pictures of icons of the tray that changed since this was last asked.
    pub fn tray_pictures(&mut self) -> Vec<(u32, u32, Vec<u8>)> {
        self.windows.tray_pictures()
    }

    /// Tells the machine what the person does with an icon of the tray. The pointer goes
    /// to the lower left corner first, beside where the page shows the tray: a menu the
    /// program opens where the pointer is opens there. An icon that is gone is nothing.
    pub async fn use_tray(&mut self, window: u32, icon: u32, does: away_wire::TrayDoes) -> Result<(), Failure> {
        let Some(version) = self.windows.tray_version(window, icon) else { return Ok(()) };
        let does = match does {
            away_wire::TrayDoes::Choose => away_rdp::rail::TrayDoes::Choose,
            away_wire::TrayDoes::Menu => away_rdp::rail::TrayDoes::Menu,
        };
        let (wide, high) = (self.picture().width, self.picture().height);
        let corner = Input::Moved { x: 8.min(wide.saturating_sub(1)) as u16, y: high.saturating_sub(8) as u16 };
        let mut told = self.open.input(&[corner]);
        told.extend(self.open.tray(window, icon, does, version));
        if told.is_empty() {
            return Ok(());
        }
        self.write(&told).await
    }

    /// How many windows of their own are open.
    pub fn windows_listed(&self) -> usize {
        self.windows.listed()
    }

    /// Tells the machine what the person does with a window as a whole.
    pub async fn window(&mut self, id: u32, does: Does) -> Result<(), Failure> {
        let told = match does {
            Does::Front => self.open.activate(id, true),
            Does::Minimize => self.open.command(id, Command::Minimize),
            Does::Maximize => self.open.command(id, Command::Maximize),
            Does::Restore => self.open.command(id, Command::Restore),
            Does::Close => self.open.command(id, Command::Close),
        };
        self.write(&told).await
    }

    /// Tells the machine of keys and of the pointer.
    pub async fn tell(&mut self, events: &[Input]) -> Result<(), Failure> {
        let told = self.open.input(events);
        if told.is_empty() {
            return Ok(());
        }
        self.write(&told).await
    }

    /// Tells the machine that the person's screen is now this, so that it offers its
    /// desktop anew at that size. Whether it was told: a machine that doesn't follow a
    /// screen that changes, or takes no screen like this one, is not.
    pub async fn resize(&mut self, screen: Screen) -> Result<bool, Failure> {
        let told = self.open.screen(screen.width, screen.height, screen.scale);
        if told.is_empty() {
            return Ok(false);
        }
        self.write(&told).await.map(|()| true)
    }

    /// The text that was copied on the machine since this was last asked, where the
    /// clipboard passes from the machine.
    pub fn copied(&mut self) -> Option<String> {
        self.copied.take()
    }

    /// Tells the machine that the person copied this text, so that it can be pasted
    /// there: where the clipboard passes to the machine.
    pub async fn offer(&mut self, text: &str) -> Result<(), Failure> {
        let told = self.open.copied(text);
        if told.is_empty() {
            return Ok(());
        }
        self.write(&told).await
    }

    /// The picture that was copied on the machine since this was last asked, where the
    /// clipboard passes from the machine.
    pub fn copied_picture(&mut self) -> Option<CopiedPicture> {
        self.copied_picture.take()
    }

    /// The files that were copied on the machine since this was last asked: which they
    /// are, and nothing of what is in them.
    pub fn copied_files(&mut self) -> Option<Vec<CopiedFile>> {
        self.copied_files.take()
    }

    /// The piece of a file that arrived since this was last asked: nothing in it where
    /// the file ends before there, and none where the machine gave none.
    pub fn piece(&mut self) -> Option<Option<Vec<u8>>> {
        self.piece.take()
    }

    /// Asks the machine for the next piece of a file it listed, from this place on.
    /// Whether it was asked: it is not where no files are listed, or a piece is on its way.
    pub async fn ask_for_a_piece(&mut self, file: u32, from: u64) -> Result<bool, Failure> {
        let asks = self.open.ask_for_a_piece(file, from, LONGEST_PIECE);
        if asks.is_empty() {
            return Ok(false);
        }
        self.write(&asks).await.map(|()| true)
    }

    /// Tells the machine that the person copied this picture, so that it can be pasted
    /// there: where the clipboard passes to the machine.
    pub async fn offer_picture(&mut self, picture: CopiedPicture) -> Result<(), Failure> {
        let told = self.open.copied_picture(picture);
        if told.is_empty() {
            return Ok(());
        }
        self.write(&told).await
    }

    /// Whether the machine offered its desktop anew, at another size, since this was
    /// last asked: the picture is then a new one, of that size.
    pub fn resized(&mut self) -> bool {
        std::mem::take(&mut self.resized)
    }

    /// Leaves. The machine keeps what runs on the desktop, for the account to come back to.
    pub async fn leave(mut self) {
        // leaving is a courtesy: the machine notices a closed connection as well
        let _ = self.writer.write_all(&self.open.leave()).await;
        let _ = self.writer.shutdown().await;
        // what the machine was still saying is read to its end, for a moment: a connection
        // that is closed over words nobody read is reset, and a reset that overtakes the
        // leaving takes it away before the machine has read it
        let read_out = async { while matches!(self.reader.read(&mut self.scratch).await, Ok(count) if count > 0) {} };
        let _ = tokio::time::timeout(LAST_WORDS_WITHIN, read_out).await;
    }
}

/// What a person did, as a Windows machine is told of it: a place beyond the desktop is
/// its edge. Nothing for a key that is told by what it writes, which is another
/// machine's way.
fn input(did: &Did, wide: usize, high: usize) -> Option<Input> {
    use away_rdp::input::Button;
    let within = |x: u16, y: u16| (x.min(wide.saturating_sub(1) as u16), y.min(high.saturating_sub(1) as u16));
    Some(match *did {
        Did::Moved { x, y } => {
            let (x, y) = within(x, y);
            Input::Moved { x, y }
        }
        Did::Button { button, down, x, y } => {
            let (x, y) = within(x, y);
            let button = match button {
                away_wire::Button::Main => Button::Left,
                away_wire::Button::Middle => Button::Middle,
                away_wire::Button::Second => Button::Right,
                away_wire::Button::Back => Button::Back,
                away_wire::Button::Forward => Button::Forward,
            };
            Input::Button { button, down, x, y }
        }
        Did::Wheel { by, sideways, x, y } => {
            let (x, y) = within(x, y);
            Input::Wheel { by, sideways, x, y }
        }
        Did::Key { code, extended, down } => Input::Key { code, extended, down },
        Did::Character { unit, down } => Input::Character { unit, down },
        Did::Locks { scroll, num, caps } => Input::Locks { scroll, num, caps },
        Did::Written { .. } => return None,
    })
}

impl Shown for Desktop {
    fn picture(&self) -> Pixels<'_> {
        let picture = Desktop::picture(self);
        Pixels { width: picture.width, height: picture.height, pixels: &picture.pixels }
    }

    async fn arrives(&mut self) -> Result<(), Failure> {
        Desktop::arrives(self).await
    }

    async fn take(&mut self) -> Result<Vec<Area>, Failure> {
        Desktop::take(self).await
    }

    async fn tick(&mut self) -> Result<(), Failure> {
        Desktop::tick(self).await
    }

    fn resized(&mut self) -> bool {
        Desktop::resized(self)
    }

    fn copied(&mut self) -> Option<String> {
        Desktop::copied(self)
    }

    fn carries_pictures(&self) -> bool {
        true
    }

    fn copied_picture(&mut self) -> Option<CopiedPicture> {
        Desktop::copied_picture(self)
    }

    async fn offer_picture(&mut self, picture: CopiedPicture) -> Result<(), Failure> {
        Desktop::offer_picture(self, picture).await
    }

    fn copied_files(&mut self) -> Option<Vec<CopiedFile>> {
        Desktop::copied_files(self)
    }

    fn piece(&mut self) -> Option<Option<Vec<u8>>> {
        Desktop::piece(self)
    }

    async fn ask_for_a_piece(&mut self, file: u32, from: u64) -> Result<bool, Failure> {
        Desktop::ask_for_a_piece(self, file, from).await
    }

    async fn did(&mut self, did: &[Did]) -> Result<(), Failure> {
        let (wide, high) = (Desktop::picture(self).width, Desktop::picture(self).height);
        let events: Vec<Input> = did.iter().filter_map(|did| input(did, wide, high)).collect();
        self.tell(&events).await
    }

    async fn offer(&mut self, text: &str) -> Result<(), Failure> {
        Desktop::offer(self, text).await
    }

    async fn resize(&mut self, screen: Screen) -> Result<bool, Failure> {
        Desktop::resize(self, screen).await
    }

    fn windows(&mut self) -> Option<(Vec<away_wire::Window>, Option<u32>)> {
        Desktop::windows(self)
    }

    fn windows_listed(&self) -> usize {
        Desktop::windows_listed(self)
    }

    fn icons(&mut self) -> Vec<(u32, Vec<u8>)> {
        Desktop::icons(self)
    }

    fn tray(&mut self) -> Option<Vec<away_wire::TrayIcon>> {
        Desktop::tray(self)
    }

    fn tray_pictures(&mut self) -> Vec<(u32, u32, Vec<u8>)> {
        Desktop::tray_pictures(self)
    }

    fn started_later(&mut self) -> Vec<Result<(), String>> {
        Desktop::started_later(self)
    }

    async fn window(&mut self, id: u32, does: Does) -> Result<(), Failure> {
        Desktop::window(self, id, does).await
    }

    async fn use_tray(&mut self, window: u32, icon: u32, does: away_wire::TrayDoes) -> Result<(), Failure> {
        Desktop::use_tray(self, window, icon, does).await
    }

    async fn start(&mut self, program: &Program) -> Result<(), Failure> {
        Desktop::start(self, program).await
    }

    async fn leave(self) {
        Desktop::leave(self).await;
    }
}

/// A machine's own reason for ending a connection: another connection took the session.
const OPENED_ELSEWHERE: u32 = 5;
/// How long a machine whose connection broke gets to have said why.
const LAST_WORDS_WITHIN: Duration = Duration::from_millis(250);

/// How often at most a connection is told how much time passed.
pub const TICKS_EVERY: Duration = Duration::from_millis(250);

fn random_bytes<const N: usize>(step: Step) -> Result<[u8; N], Failure> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).map_err(|error| failure(step, Why::Broken, format!("the system gave no random bytes: {error}")))?;
    Ok(bytes)
}

/// One message of the sign-in before the desktop: read until it is whole.
async fn message(secured: &mut TlsStream<TcpStream>) -> std::io::Result<Vec<u8>> {
    let mut message = Vec::new();
    loop {
        match der::whole(&message) {
            Ok(Some(length)) if message.len() >= length => return Ok(message),
            Ok(_) => {}
            Err(error) => return Err(std::io::Error::other(error.to_string())),
        }
        let mut bytes = [0u8; 4096];
        match secured.read(&mut bytes).await? {
            0 => return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "it closed the connection")),
            count => message.extend_from_slice(&bytes[..count]),
        }
    }
}

/// The time now, as Windows counts it: in ten-millionths of a second since 1601.
fn clock() -> u64 {
    (away_part::time::now().as_secs() + 11_644_473_600) * 10_000_000
}

/// What a machine that hands out licences knows this client by: the same for every
/// gateway of Away, so that a machine that counts devices counts Away once.
fn known_by() -> [u8; 16] {
    *b"away-the-gateway"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_desktop_has_the_dots_of_the_screen_its_window_is_on() {
        // a window on a screen that shows a dot for a dot, and one on a screen with
        // twice the dots each way: the machine draws twice as large there
        assert_eq!(Screen::for_a_window(1280, 800, 100), Screen { width: 1280, height: 800, scale: 100 });
        assert_eq!(Screen::for_a_window(1280, 800, 200), Screen { width: 2560, height: 1600, scale: 200 });
        assert_eq!(Screen::for_a_window(1728, 1117, 200), Screen { width: 3456, height: 2234, scale: 200 });
        assert_eq!(Screen::for_a_window(1440, 900, 150), Screen { width: 2160, height: 1350, scale: 150 });
        // a desktop's width is a multiple of four
        assert_eq!(Screen::for_a_window(1283, 801, 100), Screen { width: 1280, height: 801, scale: 100 });
        assert_eq!(Screen::for_a_window(1283, 801, 125), Screen { width: 1600, height: 1001, scale: 125 });
        // a browser that says nothing of the kind, or something no screen is
        assert_eq!(Screen::for_a_window(1280, 800, 0), Screen::of(1280, 800));
        assert_eq!(Screen::for_a_window(640, 480, 900), Screen { width: 3200, height: 2400, scale: 500 });
        // a window too small or too large for a desktop is brought to one that isn't
        assert_eq!(Screen::for_a_window(200, 100, 100), Screen::of(640, 480));
        assert_eq!(Screen::for_a_window(9000, 9000, 100), Screen::of(4096, 2160));
    }

    #[test]
    fn where_a_screens_dots_are_more_than_a_desktop_has_the_machine_draws_less_large() {
        // a large window on a dense screen: a quarter less at a time, until it fits
        assert_eq!(Screen::for_a_window(2560, 1440, 200), Screen { width: 3840, height: 2160, scale: 150 });
        assert_eq!(Screen::for_a_window(1920, 1080, 300), Screen { width: 3840, height: 2160, scale: 200 });
        assert_eq!(Screen::for_a_window(2560, 1440, 250), Screen { width: 3840, height: 2160, scale: 150 });
        // a scale between the quarters goes down to the next quarter
        assert_eq!(Screen::for_a_window(2560, 1440, 160), Screen { width: 3840, height: 2160, scale: 150 });
        // and never below the size the window has
        assert_eq!(Screen::for_a_window(4096, 2160, 200), Screen { width: 4096, height: 2160, scale: 100 });
        for (width, height, dots) in [(640, 480, 100), (1366, 768, 125), (3840, 2160, 300), (4096, 2160, 500), (800, 2160, 400)] {
            let screen = Screen::for_a_window(width, height, dots);
            assert!((100..=500).contains(&screen.scale), "{screen:?}");
            assert!(u32::from(screen.width) * u32::from(screen.height) <= 4096 * 2160, "{screen:?}");
            assert_eq!(screen.width % 4, 0, "{screen:?}");
        }
    }
}
