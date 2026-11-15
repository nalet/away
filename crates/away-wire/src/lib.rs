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
    },
    /// The desktop is open, at this size in pixels.
    Desktop {
        /// How wide.
        width: u16,
        /// How high.
        height: u16,
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
    /// The windows of the programs that are open, from the front one back, and which of
    /// them is the one the person works in.
    Windows {
        /// The windows.
        windows: &'a [Window],
        /// The window in front, where one is.
        in_front: Option<u32>,
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
/// The way a rectangle's pixels are written: as a PNG picture.
const PNG: u8 = 1;

impl ToBrowser<'_> {
    /// The message as it travels.
    pub fn write(&self) -> Carried {
        let object = |held: Json| Carried::Text(held.write());
        match *self {
            ToBrowser::Step(step) => object(Json::object([("step", step.into())])),
            ToBrowser::Asks { again } => object(Json::object([("asks", Json::object([("again", again.into())]))])),
            ToBrowser::Desktop { width, height } => object(Json::object([(
                "desktop",
                Json::object([("width", u64::from(width).into()), ("height", u64::from(height).into())]),
            )])),
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
            Carried::Bytes(bytes) => read_what_was_done(bytes).map(FromBrowser::Did),
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
            ToBrowser::Asks { again: false }.write(),
            ToBrowser::Asks { again: true }.write(),
            ToBrowser::Desktop { width: 1280, height: 800 }.write(),
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
        assert_eq!(read.len(), 8);
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
        ] {
            assert!(FromBrowser::read(&Carried::Text(text.to_string())).is_err(), "{text}");
        }
        for cut in ["01", "01028001", "02000102800190ff", "0205010280019000", "030078", "04", "041e02", "0607", "0608", "07", "ff"] {
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
