//! The session's messages between the browser and the gateway: one connection a session
//! carries them both ways
//! ([ADR-0004](../../../spec/architecture/decisions/0004-the-session-and-its-picture.md)).
//!
//! What is said seldom travels as text, one JSON object a message: the steps of a
//! connection, what the machine asks, how a session ended. What is said many times a
//! second travels as bytes: a rectangle of the picture one way, keys and the pointer the
//! other. The pages read and write the same messages in `web/src/session/wire.ts`, and
//! both sides are tried against the examples in `fixtures/messages.json`.

use away_part::json::Json;
use away_secret::Secret;

/// A message as the connection carries it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Carried {
    /// Text.
    Text(String),
    /// Bytes.
    Bytes(Vec<u8>),
}

/// What is no message of a session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotThis(pub &'static str);

/// What the gateway tells the browser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToBrowser<'a> {
    /// A step of the connection has begun: `reaching`, `checking`, `signing-in` or
    /// `first-picture`.
    Step(&'a str),
    /// The machine signs people in by what they type, and a name and a password are
    /// asked for: again, where it didn't accept the ones before.
    Asks {
        /// Whether it asks again.
        again: bool,
        /// Whether they are asked for in place of signing the person in as themselves,
        /// which Away does for this person on no machine.
        instead: bool,
        /// Whether the machine has a password of its desktop's own and knows no names:
        /// the password alone is asked for.
        password_alone: bool,
    },
    /// The desktop is open, at this size in pixels.
    Desktop {
        /// How wide.
        width: u16,
        /// How high.
        height: u16,
        /// Whether the machine is told of keys by what they write, and not by their
        /// places on the keyboard: it has no layout of the person's to read places with.
        written: bool,
    },
    /// A rectangle of the picture: where it goes, and its pixels as a PNG picture.
    Rectangle {
        /// Its left edge.
        left: u16,
        /// Its top edge.
        top: u16,
        /// How wide it is.
        width: u16,
        /// How high.
        height: u16,
        /// Its pixels.
        png: &'a [u8],
    },
    /// The rectangles sent since the last of these belong together, and have this
    /// number. The browser says when it has drawn them.
    Drawn(u32),
    /// A step failed: which, why as one word, and the technical reason.
    Failed {
        /// The step.
        step: &'a str,
        /// Why.
        why: &'a str,
        /// The technical reason.
        details: &'a str,
    },
    /// The session ended: why, as one word.
    Ended(&'a str),
    /// Which ways the clipboard passes between the person's device and the machine.
    Passes {
        /// What the person copied can be pasted on the machine.
        to_the_machine: bool,
        /// What is copied on the machine reaches the person's device.
        from_the_machine: bool,
        /// Whether pictures pass with the text: a machine may carry text alone.
        pictures: bool,
    },
    /// This text was copied on the machine.
    Copied(&'a str),
    /// The text that was copied on the machine has this formatting, written as a page of
    /// the web is: it comes after the text.
    Formatted(&'a str),
    /// This picture was copied on the machine.
    CopiedPicture {
        /// The picture, as a PNG picture.
        png: &'a [u8],
    },
    /// These files were copied on the machine: which they are, for the person to
    /// choose from, and nothing of what is in them.
    CopiedFiles(&'a [CopiedFile]),
    /// A piece of the file the person asked for: the pieces come in their order.
    PieceOfAFile(&'a [u8]),
    /// What became of the file the person asked for: all of it came, or the machine
    /// gave no more of it.
    File {
        /// Whether all of it came.
        whole: bool,
    },
    /// The machine pastes a file the person copied, and wants a piece of it: the file by
    /// its place among those they copied, the piece by where it begins and how long it is.
    Wants {
        /// The file.
        file: u32,
        /// Where the piece begins.
        from: u64,
        /// How long it is.
        length: u32,
    },
    /// The windows of the programs that are open, from the front one back, and which of
    /// them is the one the person works in.
    Windows {
        /// The windows.
        windows: &'a [Window],
        /// The window in front, where one is.
        in_front: Option<u32>,
    },
    /// The icon of a window of a program, as a PNG picture: the one it shows from now on.
    Icon {
        /// The window.
        window: u32,
        /// The picture.
        png: &'a [u8],
    },
    /// The icons of the tray, in the order they came: those the programs show.
    Tray(&'a [TrayIcon]),
    /// The picture of an icon of the tray, as a PNG picture: the one it shows from now on.
    TrayPicture {
        /// The window the icon is of.
        window: u32,
        /// The icon, among those of its window.
        icon: u32,
        /// The picture.
        png: &'a [u8],
    },
    /// A program the person asked for in the session that runs started, or didn't.
    Started {
        /// The program, by the name Away gives it.
        program: &'a str,
        /// Whether it started.
        started: bool,
        /// Why it didn't, in the machine's words, where the machine said so.
        details: &'a str,
    },
}

/// A window of a program, as the browser is told of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Window {
    /// What names it for as long as it is open.
    pub id: u32,
    /// Its title.
    pub title: String,
    /// How it shows: `as-it-is`, `maximized`, `minimized` or `hidden`.
    pub shown: &'static str,
    /// Whether it is a window of its own, which a list of windows names: not a menu, a
    /// tip or a dialogue that belongs to another window.
    pub listed: bool,
    /// The parts of it that can be seen, each by its left, its top, its right and its
    /// bottom edge on the desktop; the right and the bottom edge are outside it.
    pub seen: Vec<[i32; 4]>,
}

/// A file that was copied on the machine, as the browser is told of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopiedFile {
    /// Its name, after the folders it lies in among what was copied, each followed by a slash.
    pub name: String,
    /// How many bytes it holds.
    pub size: u64,
    /// Whether it is a folder.
    pub folder: bool,
}

/// An icon of the tray, as the browser is told of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayIcon {
    /// The window the icon is of.
    pub window: u32,
    /// The icon, among those of its window.
    pub icon: u32,
    /// What it says when the pointer rests on it.
    pub tip: String,
}

/// What a person does with an icon of the tray.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayDoes {
    /// Chooses it, with the main button or the keyboard.
    Choose,
    /// Asks for its menu.
    Menu,
}

/// What a person does with a window as a whole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Does {
    /// Brings it to the front.
    Front,
    /// Makes it small.
    Minimize,
    /// Makes it as large as it gets.
    Maximize,
    /// Brings it back to the size it had.
    Restore,
    /// Closes it.
    Close,
}

/// The kind of bytes that are a rectangle of the picture.
const RECTANGLE: u8 = 1;
/// The kind of bytes that are the icon of a window.
const ICON: u8 = 2;
/// The kind of bytes that are the picture of an icon of the tray.
const TRAY_PICTURE: u8 = 3;
/// The kind of bytes that are a picture that was copied on the machine.
const COPIED_PICTURE: u8 = 4;
/// The kind of bytes that are a piece of a file the person asked for.
const PIECE_OF_A_FILE: u8 = 5;
/// The kind of bytes of the browser's that are a piece of a picture the person copied.
/// What a person did with the keys and the pointer begins with a lower number.
const THE_PERSONS_PICTURE: u8 = 0x10;
/// The kind of bytes of the browser's that are a piece of what the machine wanted of
/// a file the person copied.
const OF_THE_PERSONS_FILE: u8 = 0x11;
/// The kind of bytes of the browser's that are a piece of the message that says what
/// text the person copied, where that is longer than a message of the browser's.
const THE_PERSONS_TEXT: u8 = 0x12;
/// Which piece of a picture it is: the first, the last, or both.
const FIRST: u8 = 1;
const LAST: u8 = 2;
/// The way a rectangle's pixels are written: as a PNG picture.
const PNG: u8 = 1;

impl ToBrowser<'_> {
    /// The message as it travels.
    pub fn write(&self) -> Carried {
        let object = |held: Json| Carried::Text(held.write());
        match *self {
            ToBrowser::Step(step) => object(Json::object([("step", step.into())])),
            ToBrowser::Asks { again, instead, password_alone } => {
                let mut asks = vec![("again".to_string(), Json::from(again))];
                if instead {
                    asks.push(("instead".to_string(), true.into()));
                }
                if password_alone {
                    asks.push(("only".to_string(), "password".into()));
                }
                object(Json::object([("asks", Json::Object(asks))]))
            }
            ToBrowser::Desktop { width, height, written } => {
                let mut desktop =
                    vec![("width".to_string(), Json::from(u64::from(width))), ("height".to_string(), Json::from(u64::from(height)))];
                if written {
                    desktop.push(("keys".to_string(), "written".into()));
                }
                object(Json::object([("desktop", Json::Object(desktop))]))
            }
            ToBrowser::Rectangle { left, top, width, height, png } => {
                let mut bytes = Vec::with_capacity(10 + png.len());
                bytes.push(RECTANGLE);
                for number in [left, top, width, height] {
                    bytes.extend(number.to_be_bytes());
                }
                bytes.push(PNG);
                bytes.extend_from_slice(png);
                Carried::Bytes(bytes)
            }
            ToBrowser::Drawn(number) => object(Json::object([("drawn", u64::from(number).into())])),
            ToBrowser::Failed { step, why, details } => {
                object(Json::object([("failed", Json::object([("step", step.into()), ("why", why.into()), ("details", details.into())]))]))
            }
            ToBrowser::Ended(why) => object(Json::object([("ended", why.into())])),
            // that pictures pass is said only where they do
            ToBrowser::Passes { to_the_machine, from_the_machine, pictures: false } => {
                object(Json::object([("clipboard", Json::object([("to", to_the_machine.into()), ("from", from_the_machine.into())]))]))
            }
            ToBrowser::Passes { to_the_machine, from_the_machine, pictures: true } => object(Json::object([(
                "clipboard",
                Json::object([("to", to_the_machine.into()), ("from", from_the_machine.into()), ("pictures", true.into())]),
            )])),
            ToBrowser::Copied(text) => object(Json::object([("copied", text.into())])),
            ToBrowser::Formatted(formatted) => object(Json::object([("formatted", formatted.into())])),
            // a folder is said to be one and has no size; a file has its size
            ToBrowser::CopiedFiles(files) => object(Json::object([(
                "files",
                Json::List(
                    files
                        .iter()
                        .map(|file| match file.folder {
                            true => Json::object([("name", file.name.as_str().into()), ("folder", true.into())]),
                            false => Json::object([("name", file.name.as_str().into()), ("size", file.size.into())]),
                        })
                        .collect(),
                ),
            )])),
            ToBrowser::PieceOfAFile(piece) => {
                let mut bytes = Vec::with_capacity(1 + piece.len());
                bytes.push(PIECE_OF_A_FILE);
                bytes.extend_from_slice(piece);
                Carried::Bytes(bytes)
            }
            ToBrowser::File { whole } => object(Json::object([("file", if whole { "whole" } else { "failed" }.into())])),
            ToBrowser::Wants { file, from, length } => object(Json::object([(
                "wants",
                Json::object([("file", u64::from(file).into()), ("from", from.into()), ("length", u64::from(length).into())]),
            )])),
            ToBrowser::CopiedPicture { png } => {
                let mut bytes = Vec::with_capacity(1 + png.len());
                bytes.push(COPIED_PICTURE);
                bytes.extend_from_slice(png);
                Carried::Bytes(bytes)
            }
            ToBrowser::Icon { window, png } => {
                let mut bytes = Vec::with_capacity(5 + png.len());
                bytes.push(ICON);
                bytes.extend(window.to_be_bytes());
                bytes.extend_from_slice(png);
                Carried::Bytes(bytes)
            }
            ToBrowser::Tray(icons) => object(Json::object([(
                "tray",
                Json::List(
                    icons
                        .iter()
                        .map(|icon| {
                            Json::object([
                                ("window", u64::from(icon.window).into()),
                                ("icon", u64::from(icon.icon).into()),
                                ("tip", icon.tip.as_str().into()),
                            ])
                        })
                        .collect(),
                ),
            )])),
            ToBrowser::TrayPicture { window, icon, png } => {
                let mut bytes = Vec::with_capacity(9 + png.len());
                bytes.push(TRAY_PICTURE);
                bytes.extend(window.to_be_bytes());
                bytes.extend(icon.to_be_bytes());
                bytes.extend_from_slice(png);
                Carried::Bytes(bytes)
            }
            ToBrowser::Started { program, started, details } => {
                let said = if details.is_empty() {
                    Json::object([("program", program.into()), ("ok", started.into())])
                } else {
                    Json::object([("program", program.into()), ("ok", started.into()), ("details", details.into())])
                };
                object(Json::object([("started", said)]))
            }
            ToBrowser::Windows { windows, in_front } => {
                let window = |window: &Window| {
                    let part = |part: &[i32; 4]| Json::List(part.iter().map(|edge| Json::Number(f64::from(*edge))).collect());
                    Json::object([
                        ("id", u64::from(window.id).into()),
                        ("title", window.title.as_str().into()),
                        ("shown", window.shown.into()),
                        ("listed", window.listed.into()),
                        ("seen", Json::List(window.seen.iter().map(part).collect())),
                    ])
                };
                object(Json::object([
                    ("windows", Json::List(windows.iter().map(window).collect())),
                    ("in_front", in_front.map_or(Json::Null, |window| u64::from(window).into())),
                ]))
            }
        }
    }
}

/// A button of the pointer, as a browser numbers them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    /// The main one.
    Main,
    /// The one in the middle.
    Middle,
    /// The one that opens a menu.
    Second,
    /// The one that goes back.
    Back,
    /// The one that goes forward.
    Forward,
}

/// One thing a person did with the keys or the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Did {
    /// The pointer moved to this place of the desktop, in its pixels.
    Moved {
        /// From the left.
        x: u16,
        /// From the top.
        y: u16,
    },
    /// A button went down or came up at this place.
    Button {
        /// Which.
        button: Button,
        /// Down, or up again.
        down: bool,
        /// From the left.
        x: u16,
        /// From the top.
        y: u16,
    },
    /// The wheel turned at this place: 120 is one notch away from the person, or to the right.
    Wheel {
        /// By how much.
        by: i16,
        /// Sideways, and not up and down.
        sideways: bool,
        /// From the left.
        x: u16,
        /// From the top.
        y: u16,
    },
    /// A key went down or came up: the key by its place on the keyboard.
    Key {
        /// Its code, in the first set of scan codes.
        code: u8,
        /// Whether it is one of the keys that were added later.
        extended: bool,
        /// Down, or up again.
        down: bool,
    },
    /// A character that may have no key, as one unit of UTF-16.
    Character {
        /// The unit.
        unit: u16,
        /// Down, or up again.
        down: bool,
    },
    /// Which locks are on.
    Locks {
        /// The scroll lock.
        scroll: bool,
        /// The lock of the number keys.
        num: bool,
        /// The lock of the capitals.
        caps: bool,
    },
    /// A key went down or came up: the key by what it writes or does, as X11 numbers
    /// such keys. For a machine that is told of keys so.
    Written {
        /// The key's number.
        sym: u32,
        /// Down, or up again.
        down: bool,
    },
}

/// What the browser tells the gateway. It holds secrets, so one is not compared with
/// another, and none is printed.
#[derive(Clone, Debug)]
pub enum FromBrowser {
    /// The first message: the ticket the server gave for this session, and the window the
    /// desktop is for.
    Begins {
        /// The ticket.
        ticket: Secret,
        /// How wide the window is, as a page measures it.
        width: u16,
        /// How high.
        height: u16,
        /// The layout of the person's keyboard, as Windows numbers them, where the
        /// browser can tell.
        layout: Option<u32>,
        /// How many dots the device shows for a hundred of the page's: 200 on a screen
        /// with twice the dots each way. Where the browser says.
        scale: Option<u32>,
    },
    /// The name and the password the machine asked for.
    Credentials {
        /// The name.
        name: String,
        /// The password.
        password: Secret,
    },
    /// The rectangles up to this number are drawn.
    Seen(u32),
    /// Keys and the pointer.
    Did(Vec<Did>),
    /// The person leaves.
    Leaves,
    /// The person copied this text on their own device.
    Copied(String),
    /// The person copied this text on their own device, with its formatting, written
    /// as a page of the web is.
    CopiedFormatted {
        /// The text.
        text: String,
        /// The same with its formatting.
        formatted: String,
    },
    /// The person asks for a file that was copied on the machine, by its place among
    /// those that were listed: what is in it comes in pieces.
    File(u32),
    /// The person gives up on the file they asked for.
    NoFile,
    /// The person copied these files on their own device, for the machine to paste:
    /// their names and their sizes. None: they took them back.
    Gives(Vec<CopiedFile>),
    /// A piece of what the machine wanted of a file the person copied: what it wanted
    /// comes in several, as a message of the browser's is short.
    OfTheirFile {
        /// Whether it is the first piece.
        first: bool,
        /// Whether it is the last one.
        last: bool,
        /// The piece.
        piece: Vec<u8>,
    },
    /// The person's device has nothing to give of what the machine wanted.
    NothingOfTheirFile,
    /// A piece of a picture the person copied on their own device, which is a PNG
    /// picture: a message of the browser's is short, so a picture comes in several.
    CopiedPicture {
        /// Whether it is the first piece.
        first: bool,
        /// Whether it is the last one.
        last: bool,
        /// The piece.
        piece: Vec<u8>,
    },
    /// A piece of the message that says what text the person copied on their own
    /// device, where the text is longer than a message of the browser's: the pieces
    /// together are that message, as it is written where the text is short.
    CopiedText {
        /// Whether it is the first piece.
        first: bool,
        /// Whether it is the last one.
        last: bool,
        /// The piece.
        piece: Vec<u8>,
    },
    /// The window has another size now, or is on another screen.
    Sized {
        /// How wide the window is, as a page measures it.
        width: u16,
        /// How high.
        height: u16,
        /// How many dots the device shows for a hundred of the page's, where the browser says.
        scale: Option<u32>,
    },
    /// What the person does with a window of a program as a whole.
    Window {
        /// The window.
        id: u32,
        /// What they do with it.
        does: Does,
    },
    /// The person asks for another program of the machine, in the session that runs: by
    /// the name Away gives it.
    Start(String),
    /// What the person does with an icon of the tray.
    Tray {
        /// The window the icon is of.
        window: u32,
        /// The icon, among those of its window.
        icon: u32,
        /// What they do with it.
        does: TrayDoes,
    },
}

impl FromBrowser {
    /// Reads a message as it travelled.
    pub fn read(carried: &Carried) -> Result<FromBrowser, NotThis> {
        match carried {
            Carried::Text(text) => {
                let said = Json::read(text).map_err(|_| NotThis("a message of the browser is no JSON"))?;
                let size = |name: &str| said.get(name).and_then(Json::count).and_then(|size| u16::try_from(size).ok());
                if let Some(ticket) = said.text_of("ticket") {
                    let (width, height) = (size("width"), size("height"));
                    let (Some(width), Some(height)) = (width, height) else {
                        return Err(NotThis("a session begins with the size of its desktop"));
                    };
                    let layout = said.get("layout").and_then(Json::count).and_then(|layout| u32::try_from(layout).ok());
                    let scale = said.get("scale").and_then(Json::count).and_then(|scale| u32::try_from(scale).ok());
                    return Ok(FromBrowser::Begins { ticket: Secret::new(ticket), width, height, layout, scale });
                }
                if let (Some(name), Some(password)) = (said.text_of("name"), said.text_of("password")) {
                    return Ok(FromBrowser::Credentials { name: name.to_string(), password: Secret::new(password) });
                }
                if let Some(seen) = said.get("seen").and_then(Json::count).and_then(|seen| u32::try_from(seen).ok()) {
                    return Ok(FromBrowser::Seen(seen));
                }
                if said.get("leaves").and_then(Json::flag) == Some(true) {
                    return Ok(FromBrowser::Leaves);
                }
                if let Some(text) = said.text_of("copied") {
                    return Ok(match said.text_of("formatted") {
                        Some(formatted) => FromBrowser::CopiedFormatted { text: text.to_string(), formatted: formatted.to_string() },
                        None => FromBrowser::Copied(text.to_string()),
                    });
                }
                if let Some(tray) = said.get("tray") {
                    let number = |name: &str| tray.get(name).and_then(Json::count).and_then(|number| u32::try_from(number).ok());
                    let does = match tray.text_of("does") {
                        Some("choose") => TrayDoes::Choose,
                        Some("menu") => TrayDoes::Menu,
                        _ => return Err(NotThis("an icon of the tray is chosen, or asked for its menu")),
                    };
                    let (Some(window), Some(icon)) = (number("window"), number("icon")) else {
                        return Err(NotThis("an icon of the tray is named by its window and its number"));
                    };
                    return Ok(FromBrowser::Tray { window, icon, does });
                }
                if let Some(gives) = said.get("gives") {
                    if said.text_of("gives") == Some("nothing") {
                        return Ok(FromBrowser::NothingOfTheirFile);
                    }
                    let files = gives.list().ok_or(NotThis("the files a person copied are a list, or nothing is given of one"))?;
                    let read = files.iter().map(|file| {
                        let name = file.text_of("name").filter(|name| !name.is_empty())?;
                        Some(CopiedFile { name: name.to_string(), size: file.get("size").and_then(Json::count)?, folder: false })
                    });
                    return read
                        .collect::<Option<Vec<_>>>()
                        .map(FromBrowser::Gives)
                        .ok_or(NotThis("a file a person copied has a name and a size"));
                }
                if let Some(file) = said.get("file") {
                    return match (file.count().and_then(|file| u32::try_from(file).ok()), said.text_of("file")) {
                        (Some(file), _) => Ok(FromBrowser::File(file)),
                        (None, Some("none")) => Ok(FromBrowser::NoFile),
                        _ => Err(NotThis("a file is asked for by its place among those that were copied, or given up")),
                    };
                }
                if let Some(program) = said.text_of("start") {
                    if program.is_empty() {
                        return Err(NotThis("a program is asked for by its name"));
                    }
                    return Ok(FromBrowser::Start(program.to_string()));
                }
                if let Some(sized) = said.get("sized") {
                    let size = |name: &str| sized.get(name).and_then(Json::count).and_then(|size| u16::try_from(size).ok());
                    let (Some(width), Some(height)) = (size("width"), size("height")) else {
                        return Err(NotThis("a window that has another size says how wide and how high it is"));
                    };
                    let scale = sized.get("scale").and_then(Json::count).and_then(|scale| u32::try_from(scale).ok());
                    return Ok(FromBrowser::Sized { width, height, scale });
                }
                if let Some(id) = said.get("window").and_then(Json::count).and_then(|id| u32::try_from(id).ok()) {
                    let does = match said.text_of("does") {
                        Some("front") => Does::Front,
                        Some("minimize") => Does::Minimize,
                        Some("maximize") => Does::Maximize,
                        Some("restore") => Does::Restore,
                        Some("close") => Does::Close,
                        _ => return Err(NotThis("a window is brought to the front, made small, made large, brought back or closed")),
                    };
                    return Ok(FromBrowser::Window { id, does });
                }
                Err(NotThis("a message of the browser says nothing a session knows"))
            }
            Carried::Bytes(bytes) => match bytes.split_first() {
                Some((&THE_PERSONS_PICTURE, rest)) => match rest.split_first() {
                    Some((which, piece)) if *which & !(FIRST | LAST) == 0 => {
                        Ok(FromBrowser::CopiedPicture { first: which & FIRST != 0, last: which & LAST != 0, piece: piece.to_vec() })
                    }
                    _ => Err(NotThis("a piece of a picture says which piece it is")),
                },
                Some((&OF_THE_PERSONS_FILE, rest)) => match rest.split_first() {
                    Some((which, piece)) if *which & !(FIRST | LAST) == 0 => {
                        Ok(FromBrowser::OfTheirFile { first: which & FIRST != 0, last: which & LAST != 0, piece: piece.to_vec() })
                    }
                    _ => Err(NotThis("a piece of a file says which piece it is")),
                },
                Some((&THE_PERSONS_TEXT, rest)) => match rest.split_first() {
                    Some((which, piece)) if *which & !(FIRST | LAST) == 0 => {
                        Ok(FromBrowser::CopiedText { first: which & FIRST != 0, last: which & LAST != 0, piece: piece.to_vec() })
                    }
                    _ => Err(NotThis("a piece of a long text says which piece it is")),
                },
                _ => read_what_was_done(bytes).map(FromBrowser::Did),
            },
        }
    }
}

/// The most events one message carries.
const MOST: usize = 256;

fn read_what_was_done(mut left: &[u8]) -> Result<Vec<Did>, NotThis> {
    const CUT: NotThis = NotThis("keys and pointer of the browser end in the middle");
    let mut did = Vec::new();
    while let Some((kind, rest)) = left.split_first() {
        left = rest;
        if did.len() == MOST {
            return Err(NotThis("a message of the browser carries more keys and pointer than one may"));
        }
        let mut take = |length: usize| -> Result<&[u8], NotThis> {
            let taken = left.get(..length).ok_or(CUT)?;
            left = &left[length..];
            Ok(taken)
        };
        let number = |pair: &[u8]| u16::from_be_bytes([pair[0], pair[1]]);
        let flag = |byte: u8| match byte {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(NotThis("a yes or no of the browser is neither")),
        };
        did.push(match kind {
            1 => {
                let at = take(4)?;
                Did::Moved { x: number(at), y: number(&at[2..]) }
            }
            2 => {
                let told = take(6)?;
                let button = match told[0] {
                    0 => Button::Main,
                    1 => Button::Middle,
                    2 => Button::Second,
                    3 => Button::Back,
                    4 => Button::Forward,
                    _ => return Err(NotThis("a button of the browser is none a pointer has")),
                };
                Did::Button { button, down: flag(told[1])?, x: number(&told[2..]), y: number(&told[4..]) }
            }
            3 => {
                let told = take(7)?;
                Did::Wheel { by: number(told) as i16, sideways: flag(told[2])?, x: number(&told[3..]), y: number(&told[5..]) }
            }
            4 => {
                let told = take(3)?;
                Did::Key { code: told[0], extended: flag(told[1])?, down: flag(told[2])? }
            }
            5 => {
                let told = take(3)?;
                Did::Character { unit: number(told), down: flag(told[2])? }
            }
            6 => {
                let locks = take(1)?[0];
                if locks > 7 {
                    return Err(NotThis("the locks of the browser are more than there are"));
                }
                Did::Locks { scroll: locks & 1 != 0, num: locks & 2 != 0, caps: locks & 4 != 0 }
            }
            7 => {
                let told = take(5)?;
                Did::Written { sym: u32::from_be_bytes([told[0], told[1], told[2], told[3]]), down: flag(told[4])? }
            }
            _ => return Err(NotThis("keys and pointer of the browser are of no kind a session knows")),
        });
    }
    Ok(did)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn bytes(hex: &str) -> Vec<u8> {
        (0..hex.len()).step_by(2).map(|at| u8::from_str_radix(&hex[at..at + 2], 16).expect("hex")).collect()
    }

    /// The examples both sides are tried against: each a message as it travels, and what
    /// it says, written the way the pages' test reads it too.
    const EXAMPLES: &str = include_str!("../fixtures/messages.json");

    #[test]
    fn the_gateways_messages_travel_as_the_examples_say() {
        let examples = Json::read(EXAMPLES).expect("the examples are JSON");
        let to_browser = examples.get("to_browser").and_then(Json::list).expect("examples of what the gateway says");
        let written: Vec<Carried> = vec![
            ToBrowser::Step("reaching").write(),
            ToBrowser::Step("first-picture").write(),
            ToBrowser::Asks { again: false, instead: false, password_alone: false }.write(),
            ToBrowser::Asks { again: true, instead: false, password_alone: false }.write(),
            ToBrowser::Desktop { width: 1280, height: 800, written: false }.write(),
            ToBrowser::Rectangle { left: 64, top: 128, width: 200, height: 120, png: &[0x89, 0x50, 0x4e, 0x47] }.write(),
            ToBrowser::Drawn(7).write(),
            ToBrowser::Failed { step: "reaching", why: "no-answer", details: "no answer within 10 seconds" }.write(),
            ToBrowser::Ended("left").write(),
            ToBrowser::Windows {
                windows: &[Window {
                    id: 65537,
                    title: "Ledger".to_string(),
                    shown: "as-it-is",
                    listed: true,
                    seen: vec![[20, 10, 180, 110]],
                }],
                in_front: Some(65537),
            }
            .write(),
            ToBrowser::Asks { again: false, instead: true, password_alone: false }.write(),
            ToBrowser::Passes { to_the_machine: true, from_the_machine: false, pictures: false }.write(),
            ToBrowser::Copied("two\nlines").write(),
            ToBrowser::Icon { window: 65537, png: &[0x89, 0x50, 0x4e, 0x47] }.write(),
            ToBrowser::Started {
                program: "Wages",
                started: false,
                details: "the machine does not find the program (its own code 0x00000003)",
            }
            .write(),
            ToBrowser::Tray(&[TrayIcon { window: 65537, icon: 1, tip: "Ledger – 2 reminders".to_string() }]).write(),
            ToBrowser::TrayPicture { window: 65537, icon: 1, png: &[0x89, 0x50, 0x4e, 0x47] }.write(),
            ToBrowser::Asks { again: true, instead: false, password_alone: true }.write(),
            ToBrowser::Desktop { width: 3024, height: 1964, written: true }.write(),
            ToBrowser::CopiedPicture { png: &[0x89, 0x50, 0x4e, 0x47] }.write(),
            ToBrowser::Passes { to_the_machine: true, from_the_machine: true, pictures: true }.write(),
            ToBrowser::CopiedFiles(&[
                CopiedFile { name: "notes.txt".to_string(), size: 11, folder: false },
                CopiedFile { name: "folder".to_string(), size: 0, folder: true },
                CopiedFile { name: "folder/data.bin".to_string(), size: 5_000_000_000, folder: false },
            ])
            .write(),
            ToBrowser::PieceOfAFile(b"hello").write(),
            ToBrowser::File { whole: true }.write(),
            ToBrowser::File { whole: false }.write(),
            ToBrowser::Wants { file: 1, from: 5_000_000_000, length: 100_000 }.write(),
            ToBrowser::Formatted("<b>two</b>\nlines").write(),
        ];
        assert_eq!(written.len(), to_browser.len());
        for (written, example) in written.iter().zip(to_browser) {
            match written {
                Carried::Text(text) => assert_eq!(Some(text.as_str()), example.text_of("text"), "{}", example.write()),
                Carried::Bytes(bytes) => assert_eq!(Some(hex(bytes).as_str()), example.text_of("bytes"), "{}", example.write()),
            }
        }
    }

    #[test]
    fn the_browsers_messages_are_read_as_the_examples_say() {
        let examples = Json::read(EXAMPLES).expect("the examples are JSON");
        let from_browser = examples.get("from_browser").and_then(Json::list).expect("examples of what the browser says");
        let read: Vec<FromBrowser> = from_browser
            .iter()
            .map(|example| {
                let carried = match (example.text_of("text"), example.text_of("bytes")) {
                    (Some(text), _) => Carried::Text(text.to_string()),
                    (_, Some(hex)) => Carried::Bytes(bytes(hex)),
                    _ => panic!("an example travels as text or as bytes"),
                };
                FromBrowser::read(&carried).unwrap_or_else(|NotThis(why)| panic!("{}: {why}", example.write()))
            })
            .collect();
        assert_eq!(read.len(), 20);
        // text that is longer than a message comes as the message that says it, in pieces
        assert!(matches!(&read[19], FromBrowser::CopiedText { first: true, last: true, piece } if piece[..] == *br#"{"copied":"long"}"#));
        assert!(matches!(
            FromBrowser::read(&Carried::Bytes(vec![0x12, 0, b'a'])),
            Ok(FromBrowser::CopiedText { first: false, last: false, .. })
        ));
        assert!(FromBrowser::read(&Carried::Bytes(vec![0x12])).is_err());
        assert!(FromBrowser::read(&Carried::Bytes(vec![0x12, 4, b'a'])).is_err());
        assert!(matches!(&read[18], FromBrowser::CopiedFormatted { text, formatted } if text == "bold" && formatted == "<b>bold</b>"));
        assert!(matches!(&read[15], FromBrowser::Gives(files) if files[..] == [
            CopiedFile { name: "report.txt".to_string(), size: 8, folder: false },
            CopiedFile { name: "data.bin".to_string(), size: 5_000_000_000, folder: false },
        ]));
        assert!(matches!(&read[16], FromBrowser::OfTheirFile { first: true, last: true, piece } if piece[..] == *b"hello"));
        assert!(matches!(read[17], FromBrowser::NothingOfTheirFile));
        assert!(
            matches!(FromBrowser::read(&Carried::Text(r#"{"gives":[]}"#.to_string())), Ok(FromBrowser::Gives(files)) if files.is_empty())
        );
        assert!(FromBrowser::read(&Carried::Text(r#"{"gives":[{"name":"","size":1}]}"#.to_string())).is_err());
        assert!(FromBrowser::read(&Carried::Text(r#"{"gives":[{"name":"a"}]}"#.to_string())).is_err());
        assert!(FromBrowser::read(&Carried::Text(r#"{"gives":7}"#.to_string())).is_err());
        assert!(FromBrowser::read(&Carried::Bytes(vec![0x11, 8, 1])).is_err());
        assert!(matches!(read[13], FromBrowser::File(2)));
        assert!(matches!(read[14], FromBrowser::NoFile));
        assert!(FromBrowser::read(&Carried::Text(r#"{"file":"another"}"#.to_string())).is_err());
        assert!(FromBrowser::read(&Carried::Text(r#"{"file":-1}"#.to_string())).is_err());
        assert!(
            matches!(&read[12], FromBrowser::CopiedPicture { first: true, last: true, piece } if piece[..] == [0x89, 0x50, 0x4e, 0x47])
        );
        // a piece in the middle, and one that doesn't say which it is
        assert!(matches!(
            FromBrowser::read(&Carried::Bytes(vec![0x10, 0, 7])),
            Ok(FromBrowser::CopiedPicture { first: false, last: false, .. })
        ));
        assert!(FromBrowser::read(&Carried::Bytes(vec![0x10])).is_err());
        assert!(FromBrowser::read(&Carried::Bytes(vec![0x10, 4, 7])).is_err());
        assert!(
            matches!(&read[11], FromBrowser::Did(did) if did[..] == [Did::Written { sym: 0xffeb, down: true }, Did::Written { sym: 0x0100_20ac, down: false }])
        );
        assert!(matches!(read[10], FromBrowser::Tray { window: 65537, icon: 1, does: TrayDoes::Menu }));
        assert!(matches!(&read[9], FromBrowser::Start(program) if program == "Wages"));
        assert!(matches!(&read[8], FromBrowser::Copied(text) if text == "a text\nof two lines"));
        assert!(matches!(read[7], FromBrowser::Sized { width: 1440, height: 900, scale: Some(200) }));
        assert!(matches!(read[6], FromBrowser::Window { id: 65537, does: Does::Close }));
        assert!(
            matches!(&read[0], FromBrowser::Begins { ticket, width: 1280, height: 800, layout: Some(0x0807), scale: Some(200) } if ticket.expose() == "7f3a")
        );
        assert!(
            matches!(&read[1], FromBrowser::Credentials { name, password } if name == "CORP\\alex.urban" && password.expose() == "a \"password\"")
        );
        assert!(matches!(read[2], FromBrowser::Seen(7)));
        assert!(matches!(read[3], FromBrowser::Leaves));
        let did = |read: &FromBrowser| match read {
            FromBrowser::Did(did) => did.clone(),
            other => panic!("keys and pointer, and not {other:?}"),
        };
        assert_eq!(did(&read[4]), [Did::Moved { x: 640, y: 400 }]);
        assert_eq!(
            did(&read[5]),
            [
                Did::Button { button: Button::Main, down: true, x: 640, y: 400 },
                Did::Button { button: Button::Second, down: false, x: 1, y: 2 },
                Did::Wheel { by: -120, sideways: false, x: 3, y: 4 },
                Did::Wheel { by: 120, sideways: true, x: 3, y: 4 },
                Did::Key { code: 0x1e, extended: false, down: true },
                Did::Key { code: 0x4d, extended: true, down: false },
                Did::Character { unit: 0x20ac, down: true },
                Did::Locks { scroll: false, num: true, caps: true },
            ]
        );
    }

    #[test]
    fn what_is_no_message_of_a_session_is_refused() {
        for text in [
            "",
            "[]",
            "{}",
            r#"{"ticket":"7f3a"}"#,
            r#"{"ticket":"7f3a","width":70000,"height":800}"#,
            r#"{"name":"alex"}"#,
            r#"{"seen":-1}"#,
            r#"{"leaves":false}"#,
            r#"{"window":65537}"#,
            r#"{"window":65537,"does":"shake"}"#,
            r#"{"window":-1,"does":"close"}"#,
            r#"{"start":""}"#,
            r#"{"tray":{"window":65537,"icon":1}}"#,
            r#"{"tray":{"window":65537,"does":"choose"}}"#,
            r#"{"start":7}"#,
        ] {
            assert!(FromBrowser::read(&Carried::Text(text.to_string())).is_err(), "{text}");
        }
        for cut in [
            "01",
            "01028001",
            "02000102800190ff",
            "0205010280019000",
            "030078",
            "04",
            "041e02",
            "0607",
            "0608",
            "07",
            "070000ffeb",
            "070000ffeb02",
            "ff",
        ] {
            let carried = Carried::Bytes(bytes(cut));
            // "0607" is the three locks, all on: the one of these that is a message
            assert_eq!(FromBrowser::read(&carried).is_err(), cut != "0607", "{cut}");
        }
        // nothing done is a message that says nothing
        assert!(matches!(FromBrowser::read(&Carried::Bytes(Vec::new())), Ok(FromBrowser::Did(did)) if did.is_empty()));
        // and no message carries more than one may
        assert!(FromBrowser::read(&Carried::Bytes([4, 0x1e, 0, 1].repeat(257))).is_err());
        assert!(FromBrowser::read(&Carried::Bytes([4, 0x1e, 0, 1].repeat(256))).is_ok());
    }
}
