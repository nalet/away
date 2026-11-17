//! One machine's desktop over VNC, as the gateway holds it: reached, signed in with the
//! desktop's own password where it has one, and then drawn into a picture of the
//! gateway's own ([ADR-0004](../../../spec/architecture/decisions/0004-the-session-and-its-picture.md)).
//!
//! VNC has nothing a machine shows of itself, so there is no step of checking who it
//! is; and what travels between the gateway and the machine is not secured by the
//! protocol. A machine is reached over VNC on a network that is trusted with that.

use away_secret::Secret;
use away_vnc::connection::{Button, Connection, Happened, Input};
use away_wire::Did;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::machine::{self, failure, Area, Failure, Passes, Step, Why, WITHIN};
use crate::shown::{Pixels, Shown};

/// The connection to a machine, and what it said so far.
struct Line {
    machine: TcpStream,
    open: Connection,
    scratch: Vec<u8>,
    /// Whether the machine asks for its password.
    asks: bool,
    /// The areas that were drawn and not yet taken.
    drawn: Vec<Area>,
    /// Whether the desktop got another size since that was last asked.
    resized: bool,
    /// The text that was last copied on the machine, until it is taken.
    copied: Option<String>,
}

/// What went wrong, as a step that failed: a password the machine didn't take is the
/// sign-in's, whatever step it was noticed at.
fn failed(step: Step, error: away_vnc::Error) -> Failure {
    match &error {
        away_vnc::Error::PasswordRefused(_) => failure(Step::SigningIn, Why::SignInRefused, error),
        away_vnc::Error::Refused(_) => failure(step, Why::Refused, error),
        _ => failure(step, Why::Broken, error),
    }
}

impl Line {
    async fn write(&mut self, bytes: &[u8], step: Step) -> Result<(), Failure> {
        if bytes.is_empty() {
            return Ok(());
        }
        let broke = |error: std::io::Error| failure(step, Why::Broken, format!("the connection to the machine broke: {error}"));
        self.machine.write_all(bytes).await.map_err(broke)
    }

    /// Takes in what arrived from the machine, and answers it where it wants an answer.
    async fn takes(&mut self, arrived: &[u8], step: Step) -> Result<(), Failure> {
        let (send, happened) = self.open.receive(arrived).map_err(|error| failed(step, error))?;
        self.write(&send, step).await?;
        for happened in happened {
            match happened {
                Happened::Asks => self.asks = true,
                // a desktop of another size is drawn anew: what was drawn on the one before is of no use
                Happened::Resized { .. } => {
                    self.resized = true;
                    self.drawn.clear();
                }
                Happened::Drawn(areas) => self.drawn.extend(areas.iter().map(|area| Area {
                    left: area.left,
                    top: area.top,
                    width: area.width,
                    height: area.height,
                })),
                // of what was copied on the machine, the newest is for the person
                Happened::Copied(text) => self.copied = Some(text),
                Happened::Speaks(_) | Happened::SignedIn | Happened::Open { .. } => {}
            }
        }
        Ok(())
    }

    /// Waits for the machine to say something, and takes it in.
    async fn hears(&mut self, step: Step) -> Result<(), Failure> {
        let broke = |error: std::io::Error| failure(step, Why::Broken, format!("the connection to the machine broke: {error}"));
        let mut scratch = std::mem::take(&mut self.scratch);
        let heard = match self.machine.read(&mut scratch).await.map_err(broke) {
            Ok(0) => Err(failure(step, Why::Refused, "the machine closed the connection")),
            Ok(count) => self.takes(&scratch[..count], step).await,
            Err(failed) => Err(failed),
        };
        self.scratch = scratch;
        heard
    }
}

/// A machine that answered over VNC, and said whether it asks for its password.
pub struct Reached {
    line: Line,
}

/// Finds a machine at its address, connects to it, and hears whether it asks for a
/// password. The clipboard passes these ways on the desktop that is opened over it.
pub async fn reach(host: &str, port: u16, clipboard: Passes) -> Result<Reached, Failure> {
    let reaching = async {
        let machine = machine::connect(host, port).await?;
        let passes =
            away_vnc::connection::Passes { to_the_machine: clipboard.to_the_machine, from_the_machine: clipboard.from_the_machine };
        let mut line = Line {
            machine,
            open: Connection::begin(None, passes),
            scratch: vec![0; 64 * 1024],
            asks: false,
            drawn: Vec::new(),
            resized: false,
            copied: None,
        };
        // the machine speaks first: until it asks for its password, or took the client without one
        while !line.asks && !line.open.is_open() {
            line.hears(Step::Reaching).await?;
        }
        Ok(Reached { line })
    };
    match tokio::time::timeout(WITHIN, reaching).await {
        Ok(reached) => reached,
        Err(_) => Err(failure(Step::Reaching, Why::NoAnswer, format!("no answer within {} seconds", WITHIN.as_secs()))),
    }
}

impl Reached {
    /// Whether the machine asks for its password.
    pub fn asks(&self) -> bool {
        self.line.asks
    }

    /// Gives the machine its password, where it asks for one, and waits for its desktop.
    pub async fn sign_in(mut self, password: Option<&Secret>) -> Result<Desktop, Failure> {
        let signing_in = async {
            if self.line.asks {
                let password = password
                    .ok_or_else(|| failure(Step::SigningIn, Why::SignInRefused, "the machine asks for a password, and none was given"))?;
                let answer = self.line.open.password(password);
                self.line.write(&answer, Step::SigningIn).await?;
            }
            while !self.line.open.is_open() {
                self.line.hears(Step::SigningIn).await?;
            }
            Ok(Desktop { line: self.line, unread: Vec::new() })
        };
        match tokio::time::timeout(WITHIN, signing_in).await {
            Ok(signed_in) => signed_in,
            Err(_) => Err(failure(Step::SigningIn, Why::NoAnswer, format!("no answer within {} seconds", WITHIN.as_secs()))),
        }
    }
}

/// A desktop that is open over VNC: the gateway's picture of it, and the connection it
/// comes over.
pub struct Desktop {
    line: Line,
    /// What arrived and is not taken in yet.
    unread: Vec<u8>,
}

impl Desktop {
    /// Whether the machine is a Mac, as the version of VNC it speaks says.
    pub fn is_a_mac(&self) -> bool {
        self.line.open.is_a_mac()
    }
}

/// What a person did, as a machine is told of it over VNC. A key by its place on the
/// keyboard is nothing here: such a machine is told of keys by what they write.
fn input(did: &Did) -> Option<Input> {
    Some(match *did {
        Did::Moved { x, y } => Input::Moved { x, y },
        Did::Button { button, down, x, y } => {
            let button = match button {
                away_wire::Button::Main => Button::Left,
                away_wire::Button::Middle => Button::Middle,
                away_wire::Button::Second => Button::Right,
                away_wire::Button::Back => Button::Back,
                away_wire::Button::Forward => return None,
            };
            Input::Button { button, down, x, y }
        }
        Did::Wheel { by, sideways, x, y } => Input::Wheel { by, sideways, x, y },
        Did::Written { sym, down } => Input::Key { sym, down },
        // a character that may have no key: as X11 numbers what is written, the western
        // alphabet's by its own number and every other one above a mark
        Did::Character { unit, down } => {
            let sym = if unit < 0x100 { u32::from(unit) } else { 0x0100_0000 + u32::from(unit) };
            Input::Key { sym, down }
        }
        Did::Key { .. } | Did::Locks { .. } => return None,
    })
}

impl Shown for Desktop {
    fn picture(&self) -> Pixels<'_> {
        let picture = self.line.open.picture();
        Pixels { width: picture.width, height: picture.height, pixels: &picture.pixels }
    }

    fn takes_keys_as_written(&self) -> bool {
        true
    }

    async fn arrives(&mut self) -> Result<(), Failure> {
        let broke =
            |error: std::io::Error| failure(Step::FirstPicture, Why::Broken, format!("the connection to the machine broke: {error}"));
        match self.line.machine.read(&mut self.line.scratch).await.map_err(broke)? {
            0 => Err(failure(Step::FirstPicture, Why::Broken, "the machine closed the connection")),
            count => {
                self.unread.extend_from_slice(&self.line.scratch[..count]);
                Ok(())
            }
        }
    }

    async fn take(&mut self) -> Result<Vec<Area>, Failure> {
        let unread = std::mem::take(&mut self.unread);
        self.line.takes(&unread, Step::FirstPicture).await?;
        Ok(std::mem::take(&mut self.line.drawn))
    }

    fn resized(&mut self) -> bool {
        std::mem::take(&mut self.line.resized)
    }

    fn copied(&mut self) -> Option<String> {
        self.line.copied.take()
    }

    async fn did(&mut self, did: &[Did]) -> Result<(), Failure> {
        let events: Vec<Input> = did.iter().filter_map(input).collect();
        let told = self.line.open.input(&events);
        self.line.write(&told, Step::FirstPicture).await
    }

    async fn offer(&mut self, text: &str) -> Result<(), Failure> {
        let told = self.line.open.copied(text);
        self.line.write(&told, Step::FirstPicture).await
    }

    async fn leave(mut self) {
        // leaving is a courtesy: the machine notices a closed connection as well
        let _ = self.line.machine.shutdown().await;
    }
}
