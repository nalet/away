//! One machine's desktop, as the gateway holds it: reached, secured, signed in and
//! opened with Away's own RDP code, and then drawn into a picture of the gateway's own
//! ([ADR-0004](../../../spec/architecture/decisions/0004-the-session-and-its-picture.md)).
//!
//! The steps are the ones a person sees while a machine opens. Each can fail, and says
//! at which step and why.

use std::sync::Arc;
use std::time::Duration;

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
use away_wire::Does;
use tokio::io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;

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

fn failure(step: Step, why: Why, details: impl ToString) -> Failure {
    Failure { step, why, details: details.to_string() }
}

/// Who is signed in on a machine.
pub enum Who {
    /// An account, with a name and a password that somebody typed.
    Account(Account),
    /// The person themselves: with a ticket of their domain for this machine, and a
    /// smart card that exists in the gateway's memory alone.
    Themselves(Box<OnAMachine>),
}

/// A machine that answered, and said which security it takes.
pub struct Reached {
    host: String,
    connection: TcpStream,
    security: Security,
}

/// Finds a machine at its address, connects to it, and asks which security it takes.
/// A farm is told something first: its broker which collection is wanted, and the
/// machine a broker sent the client on to what the broker gave for it.
pub async fn reach(host: &str, port: u16, says_first: Option<&[u8]>) -> Result<Reached, Failure> {
    let reaching = async {
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
        let mut connection = connection?;
        let _ = connection.set_nodelay(true);
        let broken = |error: std::io::Error| failure(Step::Reaching, Why::Broken, format!("the first request: {error}"));
        let request = negotiation::request_with(says_first).map_err(|error| failure(Step::Reaching, Why::Broken, error))?;
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
        Ok(Reached { host: host.to_string(), connection, security: answer.security })
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
    identity: Identity,
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
        Ok(Secured { host: self.host, secured, security: self.security, identity })
    }
}

impl Secured {
    /// What the machine shows of itself.
    pub fn identity(&self) -> &Identity {
        &self.identity
    }

    /// Whether the machine signs people in itself, on its own screen, once the desktop
    /// is open: it then takes any name and asks there.
    pub fn signs_in_itself(&self) -> bool {
        self.security == Security::Tls
    }

    /// Signs somebody in, and opens the desktop at this size, for a keyboard of this
    /// layout, as Windows numbers them. On the machine a farm's broker sent the client
    /// on to, the session the broker named is said. A broker opens nothing: it says
    /// where the connection belongs. With a program, no desktop is opened: the machine
    /// is asked to start the program, and shows its windows.
    pub async fn sign_in(
        mut self,
        who: Who,
        width: u16,
        height: u16,
        layout: u32,
        sent_on_to: Option<u32>,
        program: Option<Program>,
    ) -> Result<Opened, Failure> {
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
            let client = Client { width, height, security: self.security.number(), name: "away".to_string(), layout, sent_on_to, program };
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
                scratch: vec![0u8; 32 * 1024],
                reads_a_card,
                ticked: tokio::time::Instant::now(),
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
    scratch: Vec<u8>,
    /// Whether the machine was handed a smart card, which it reads over the connection.
    reads_a_card: bool,
    /// When the connection was last told how much time passed.
    ticked: tokio::time::Instant,
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
        self.writer
            .write_all(&waited)
            .await
            .map_err(|error| failure(Step::FirstPicture, Why::Broken, format!("the connection to the machine broke: {error}")))
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
                // the machine said why it ends the connection
                away_rdp::Error::Ended(_) | away_rdp::Error::NoLicence(_) => broken(error.to_string()),
                other => broken(other.to_string()),
            })?;
            if !send.is_empty() {
                self.writer.write_all(&send).await.map_err(|error| broken(error.to_string()))?;
            }
            for happened in happened {
                match happened {
                    Happened::Desktop { width, height, .. } => self.picture = Some(Picture::new(usize::from(width), usize::from(height))),
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
                    Happened::Windows(changes) => {
                        self.windows.take(changes);
                        self.windows_changed = true;
                    }
                    Happened::NotStarted { result, raw } => {
                        let details = format!("{} (its own code {raw:#010x})", away_rdp::rail::why(result));
                        return Err(failure(Step::FirstPicture, Why::ProgramNotStarted, details));
                    }
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
        self.writer
            .write_all(&told)
            .await
            .map_err(|error| failure(Step::FirstPicture, Why::Broken, format!("the connection to the machine broke: {error}")))
    }

    /// Tells the machine of keys and of the pointer.
    pub async fn tell(&mut self, events: &[Input]) -> Result<(), Failure> {
        let told = self.open.input(events);
        if told.is_empty() {
            return Ok(());
        }
        self.writer
            .write_all(&told)
            .await
            .map_err(|error| failure(Step::FirstPicture, Why::Broken, format!("the connection to the machine broke: {error}")))
    }

    /// Leaves. The machine keeps what runs on the desktop, for the account to come back to.
    pub async fn leave(mut self) {
        // leaving is a courtesy: the machine notices a closed connection as well
        let _ = self.writer.write_all(&self.open.leave()).await;
        let _ = self.writer.shutdown().await;
    }
}

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
