//! Programs of a Windows machine that open in windows of their own (MS-RDPERP 2.2.2):
//! the messages of the channel a program is asked for on. The machine says that it is
//! ready, the client says the same, what it is, how much room there is, and which
//! program it wants, and the machine says whether the program started. What the
//! program's windows are travels with what the machine draws ([`crate::window`]).

use crate::Error;

/// The name of the channel these messages travel on.
pub const CHANNEL: &str = "rail";

const START: u16 = 0x0001;
const ACTIVATE: u16 = 0x0002;
const SETTING: u16 = 0x0003;
const COMMAND: u16 = 0x0004;
const NOTIFY: u16 = 0x0006;
const READY: u16 = 0x0005;
const CLIENT: u16 = 0x000b;
const KEYBOARD: u16 = 0x0011;
const READY_WITH_MORE: u16 = 0x0013;
const CLOAK: u16 = 0x0015;
const RESULT: u16 = 0x0080;

/// The setting that says how much room the windows have.
const WORK_AREA: u32 = 0x0000_002f;
/// The client hides windows from the machine's picture, and is told of those the
/// machine hides.
const CLOAKS: u32 = 0x0000_0200;
/// Of what a person types with: a keyboard layout, and not a text service.
const A_LAYOUT: u32 = 0x0000_0002;

const WRONG: Error = Error::NotRdp("a message about programs is not built as one");

/// A program to start: what a farm or a machine publishes it as, and what it is given.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    /// What to run: the name a program is published under, with `||` before it, or the
    /// path of a program on the machine.
    pub run: String,
    /// What the program is given as it starts: a file to open, or nothing.
    pub arguments: String,
}

/// What a person does with a window as a whole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Makes it small.
    Minimize,
    /// Makes it as large as it gets.
    Maximize,
    /// Brings it back to the size it had.
    Restore,
    /// Closes it.
    Close,
}

fn message(kind: u16, holds: &[u8]) -> Vec<u8> {
    let mut message = Vec::with_capacity(holds.len() + 4);
    message.extend(kind.to_le_bytes());
    message.extend(((holds.len() + 4) as u16).to_le_bytes());
    message.extend_from_slice(holds);
    message
}

fn wide(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

/// The client is ready for programs, as the machine said it is.
pub fn ready() -> Vec<u8> {
    // the build of whoever says it, which says nothing of a person's device
    message(READY, &1u32.to_le_bytes())
}

/// What the client is: one that hides windows, and leaves moving a window and its place
/// among the others to the machine. The icons of the windows travel, for the client to
/// show beside their titles.
pub fn client() -> Vec<u8> {
    message(CLIENT, &CLOAKS.to_le_bytes())
}

/// The keyboard the person types on, by its layout as Windows numbers them: the
/// machine gives the program's windows that layout. A session the machine had open
/// before keeps the layout it was begun with until it is told so. Nothing for a layout
/// that is not the one of a language as such.
pub fn keyboard(layout: u32) -> Option<Vec<u8>> {
    let language = u16::try_from(layout).ok()?;
    let mut holds = A_LAYOUT.to_le_bytes().to_vec();
    holds.extend(language.to_le_bytes());
    // a layout is no text service: the two names of one are none
    holds.extend([0; 32]);
    // the layout as the machine has it loaded: the layout over its language
    holds.extend((u32::from(language) << 16 | u32::from(language)).to_le_bytes());
    Some(message(KEYBOARD, &holds))
}

/// How much room the windows have: the whole desktop, which has no bar of the client's.
pub fn work_area(width: u16, height: u16) -> Vec<u8> {
    let mut holds = WORK_AREA.to_le_bytes().to_vec();
    for number in [0, 0, width, height] {
        holds.extend(number.to_le_bytes());
    }
    message(SETTING, &holds)
}

/// Asks the machine to start a program.
pub fn start(program: &Program) -> Result<Vec<u8>, Error> {
    let (run, arguments) = (wide(&program.run), wide(&program.arguments));
    if run.is_empty() || run.len() > 520 || arguments.len() > 16_000 {
        return Err(Error::NotRdp("a program's name is 1 to 260 characters, and what it is given 8000 at most"));
    }
    // nothing of it is a file of the client's, and nothing is to be filled in on the machine
    let mut holds = Vec::with_capacity(run.len() + arguments.len() + 8);
    for number in [0, run.len(), 0, arguments.len()] {
        holds.extend((number as u16).to_le_bytes());
    }
    holds.extend(run);
    holds.extend(arguments);
    Ok(message(START, &holds))
}

/// Tells the machine that a window is the one the person works in now, or no longer is.
pub fn activate(window: u32, active: bool) -> Vec<u8> {
    let mut holds = window.to_le_bytes().to_vec();
    holds.push(u8::from(active));
    message(ACTIVATE, &holds)
}

/// What a person does with an icon of the tray.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayDoes {
    /// Chooses it, with the main button or the keyboard.
    Choose,
    /// Asks for its menu, with the second button or the menu key.
    Menu,
}

/// Tells the machine what the person does with an icon of the tray, as Windows' own
/// taskbar tells the program: the button going down and coming up, and from the tray's
/// version 3 on the choice it makes, after them.
pub fn tray(window: u32, icon: u32, does: TrayDoes, version: u32) -> Vec<Vec<u8>> {
    let (down, up, then) = match does {
        TrayDoes::Choose => (0x0201u32, 0x0202u32, 0x0400u32),
        TrayDoes::Menu => (0x0204, 0x0205, 0x007b),
    };
    let mut said = vec![down, up];
    if version >= 3 {
        said.push(then);
    }
    said.into_iter()
        .map(|message| {
            let mut holds = window.to_le_bytes().to_vec();
            holds.extend(icon.to_le_bytes());
            holds.extend(message.to_le_bytes());
            self::message(NOTIFY, &holds)
        })
        .collect()
}

/// Asks the machine to hide a window: it is drawn no more, as long as it is open.
pub fn cloak(window: u32) -> Vec<u8> {
    let mut holds = window.to_le_bytes().to_vec();
    holds.push(1);
    message(CLOAK, &holds)
}

/// Tells the machine what the person does with a window as a whole.
pub fn command(window: u32, command: Command) -> Vec<u8> {
    let number: u16 = match command {
        Command::Minimize => 0xf020,
        Command::Maximize => 0xf030,
        Command::Restore => 0xf120,
        Command::Close => 0xf060,
    };
    let mut holds = window.to_le_bytes().to_vec();
    holds.extend(number.to_le_bytes());
    message(COMMAND, &holds)
}

/// What the machine says about programs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Said {
    /// It is ready for programs.
    Ready,
    /// The program started.
    Started,
    /// The program did not start.
    NotStarted {
        /// Why, as RDP numbers the reasons.
        result: u16,
        /// Why, as the machine's own system numbers them.
        raw: u32,
    },
    /// Something else, which the client takes no notice of: the message's kind.
    Other(u16),
}

/// Reads one message of the machine's.
pub fn read(message: &[u8]) -> Result<Said, Error> {
    let number16 = |at: usize| message.get(at..at + 2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).ok_or(WRONG);
    let (kind, length) = (number16(0)?, usize::from(number16(2)?));
    if length < 4 || length > message.len() {
        return Err(WRONG);
    }
    Ok(match kind {
        READY | READY_WITH_MORE => Said::Ready,
        RESULT => {
            // what was asked for comes back with it: the flags, the result, the system's own word
            let result = number16(6)?;
            let raw = message.get(8..12).map(|four| u32::from_le_bytes([four[0], four[1], four[2], four[3]])).ok_or(WRONG)?;
            match result {
                0 => Said::Started,
                _ => Said::NotStarted { result, raw },
            }
        }
        other => Said::Other(other),
    })
}

/// Why a program did not start, in words.
pub fn why(result: u16) -> &'static str {
    match result {
        0x0001 => "the machine does not watch the desktop the program would open on",
        0x0002 => "the machine could not read which program is asked for",
        0x0003 => "the machine does not let this program be started: it is not among those it publishes",
        0x0005 => "the machine does not find the program",
        0x0007 => "the session on the machine is locked",
        _ => "the machine could not start the program",
    }
}

/// The machine's side of these messages, for the tests' stand-in for one.
#[cfg(any(test, feature = "stand-in"))]
pub mod stand_in {
    use super::*;

    /// What a client said about programs.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum Asked {
        /// It is ready.
        Ready,
        /// What it is: the features it names.
        Client(u32),
        /// How much room the windows have.
        WorkArea(u16, u16),
        /// The keyboard layout to type with, as the machine has it loaded.
        Keyboard(u32),
        /// To start a program.
        Start(Program),
        /// A window is the one the person works in, or no longer is.
        Activate(u32, bool),
        /// What the person does with a window as a whole.
        Command(u32, Command),
        /// To hide a window.
        Cloak(u32),
        /// What the person does with an icon of the tray: its window, the icon, and the
        /// message the program is given.
        Tray(u32, u32, u32),
        /// Something else: the message's kind.
        Other(u16),
    }

    /// Reads one message of a client's.
    pub fn read(message: &[u8]) -> Result<Asked, Error> {
        let number16 = |at: usize| message.get(at..at + 2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).ok_or(WRONG);
        let number32 =
            |at: usize| message.get(at..at + 4).map(|four| u32::from_le_bytes([four[0], four[1], four[2], four[3]])).ok_or(WRONG);
        let text = |at: usize, length: usize| {
            let bytes = message.get(at..at + length).ok_or(WRONG)?;
            let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
            Ok::<String, Error>(String::from_utf16_lossy(&units))
        };
        if usize::from(number16(2)?) != message.len() {
            return Err(WRONG);
        }
        Ok(match number16(0)? {
            READY => Asked::Ready,
            CLIENT => Asked::Client(number32(4)?),
            SETTING if number32(4)? == WORK_AREA => Asked::WorkArea(number16(12)?, number16(14)?),
            KEYBOARD if number32(4)? == A_LAYOUT => Asked::Keyboard(number32(42)?),
            START => {
                let (run, directory, arguments) = (usize::from(number16(6)?), usize::from(number16(8)?), usize::from(number16(10)?));
                Asked::Start(Program { run: text(12, run)?, arguments: text(12 + run + directory, arguments)? })
            }
            ACTIVATE => Asked::Activate(number32(4)?, *message.get(8).ok_or(WRONG)? != 0),
            COMMAND => Asked::Command(
                number32(4)?,
                match number16(8)? {
                    0xf020 => Command::Minimize,
                    0xf030 => Command::Maximize,
                    0xf120 => Command::Restore,
                    0xf060 => Command::Close,
                    _ => return Err(WRONG),
                },
            ),
            CLOAK if message.get(8) == Some(&1) => Asked::Cloak(number32(4)?),
            NOTIFY => Asked::Tray(number32(4)?, number32(8)?, number32(12)?),
            other => Asked::Other(other),
        })
    }

    /// The machine is ready for programs.
    pub fn ready() -> Vec<u8> {
        message(READY, &26_100u32.to_le_bytes())
    }

    /// Whether a program started: what was asked for, and the result.
    pub fn result(program: &Program, result: u16) -> Vec<u8> {
        let run = wide(&program.run);
        let mut holds = Vec::new();
        for number in [0, result] {
            holds.extend(number.to_le_bytes());
        }
        holds.extend(if result == 0 { 0u32 } else { 2 }.to_le_bytes());
        holds.extend(0u16.to_le_bytes());
        holds.extend((run.len() as u16).to_le_bytes());
        holds.extend(run);
        message(RESULT, &holds)
    }
}

#[cfg(test)]
mod tests {
    use super::stand_in::Asked;
    use super::*;

    #[test]
    fn the_client_says_it_is_ready_what_it_is_and_which_program_it_wants() {
        assert_eq!(ready(), [0x05, 0x00, 0x08, 0x00, 1, 0, 0, 0]);
        assert_eq!(client(), [0x0b, 0x00, 0x08, 0x00, 0x00, 0x02, 0, 0]);
        let swiss = keyboard(0x0807).expect("a language's own layout");
        assert_eq!((&swiss[..10], &swiss[42..]), (&[0x11, 0x00, 46, 0x00, 2, 0, 0, 0, 0x07, 0x08][..], &[0x07, 0x08, 0x07, 0x08][..]));
        assert!(swiss[10..42].iter().all(|byte| *byte == 0));
        assert_eq!(stand_in::read(&swiss), Ok(stand_in::Asked::Keyboard(0x0807_0807)));
        assert_eq!(keyboard(0x0001_0409), None);
        assert_eq!(work_area(1280, 800), [0x03, 0x00, 0x10, 0x00, 0x2f, 0, 0, 0, 0, 0, 0, 0, 0x00, 0x05, 0x20, 0x03]);
        let program = Program { run: "||ledger".to_string(), arguments: "a b".to_string() };
        let asks = start(&program).unwrap();
        assert_eq!(&asks[..12], &[0x01, 0x00, 34, 0x00, 0, 0, 16, 0, 0, 0, 6, 0]);
        assert_eq!(&asks[12..28], &[b'|', 0, b'|', 0, b'l', 0, b'e', 0, b'd', 0, b'g', 0, b'e', 0, b'r', 0]);
        assert_eq!(&asks[28..], &[b'a', 0, b' ', 0, b'b', 0]);
        assert_eq!(activate(0x1_0001, true), [0x02, 0x00, 0x09, 0x00, 0x01, 0x00, 0x01, 0x00, 1]);
        assert_eq!(command(7, Command::Close), [0x04, 0x00, 0x0a, 0x00, 7, 0, 0, 0, 0x60, 0xf0]);
        assert_eq!(cloak(0x1_0001), [0x15, 0x00, 0x09, 0x00, 0x01, 0x00, 0x01, 0x00, 1]);
        assert_eq!(stand_in::read(&cloak(0x1_0001)), Ok(Asked::Cloak(0x1_0001)));
        // a choice and a menu of an icon of the tray, as Windows' taskbar tells them
        let told = |does, version| tray(0x1_0001, 7, does, version).iter().map(|said| stand_in::read(said)).collect::<Result<Vec<_>, _>>();
        assert_eq!(
            told(TrayDoes::Choose, 4),
            Ok(vec![Asked::Tray(0x1_0001, 7, 0x201), Asked::Tray(0x1_0001, 7, 0x202), Asked::Tray(0x1_0001, 7, 0x400)])
        );
        assert_eq!(told(TrayDoes::Menu, 0), Ok(vec![Asked::Tray(0x1_0001, 7, 0x204), Asked::Tray(0x1_0001, 7, 0x205)]));
        assert_eq!(&tray(0x1_0001, 7, TrayDoes::Menu, 4)[2][..4], &[0x06, 0x00, 0x10, 0x00]);

        // each is read as what it is
        assert_eq!(stand_in::read(&ready()), Ok(Asked::Ready));
        assert_eq!(stand_in::read(&client()), Ok(Asked::Client(0x200)));
        assert_eq!(stand_in::read(&work_area(1280, 800)), Ok(Asked::WorkArea(1280, 800)));
        assert_eq!(stand_in::read(&asks), Ok(Asked::Start(program)));
        assert_eq!(stand_in::read(&activate(9, false)), Ok(Asked::Activate(9, false)));
        for does in [Command::Minimize, Command::Maximize, Command::Restore, Command::Close] {
            assert_eq!(stand_in::read(&command(3, does)), Ok(Asked::Command(3, does)));
        }

        // a program without a name, or with one that is too long, is not asked for
        for run in [String::new(), "a".repeat(261)] {
            assert!(start(&Program { run, arguments: String::new() }).is_err());
        }
        assert!(start(&Program { run: "a".repeat(260), arguments: "b".repeat(8000) }).is_ok());
        assert!(start(&Program { run: "a".to_string(), arguments: "b".repeat(8001) }).is_err());
    }

    #[test]
    fn the_machine_says_that_it_is_ready_and_whether_the_program_started() {
        let program = Program { run: "||ledger".to_string(), arguments: String::new() };
        assert_eq!(read(&stand_in::ready()), Ok(Said::Ready));
        // the newer way of saying it, which carries more
        assert_eq!(read(&[0x13, 0x00, 0x0c, 0x00, 1, 0, 0, 0, 7, 0, 0, 0]), Ok(Said::Ready));
        assert_eq!(read(&stand_in::result(&program, 0)), Ok(Said::Started));
        assert_eq!(read(&stand_in::result(&program, 3)), Ok(Said::NotStarted { result: 3, raw: 2 }));
        assert!(why(3).contains("not among those it publishes"));
        // what the client takes no notice of: the machine's own settings
        assert_eq!(read(&[0x03, 0x00, 0x09, 0x00, 0x11, 0, 0, 0, 1]), Ok(Said::Other(3)));
        // cut, or a length that is more than what is there
        let result = stand_in::result(&program, 0);
        for cut in [0, 3, 7, 11] {
            assert_eq!(read(&result[..cut]), Err(WRONG), "cut at {cut}");
        }
        assert_eq!(read(&[0x05, 0x00, 0x09, 0x00, 1, 0, 0, 0]), Err(WRONG));
    }
}
