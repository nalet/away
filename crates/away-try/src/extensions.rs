//! What a machine over VNC knows beyond the protocol's core: its extensions.
//!
//! A machine never lists them. The client names the ones it reads, and a machine that
//! knows one answers in that extension's own way: with a rectangle that carries no
//! pixels, or with a message of a kind the core doesn't have. This names them all to a
//! machine whose desktop is open, reads what it sends, and says which it answered. One
//! that it didn't answer may still be known to it: some say something only when there
//! is something to say.

use std::collections::{BTreeMap, BTreeSet};

/// An extension: its number among the ways of writing a rectangle, the name its
/// description gives it, what it is for, and when a machine that knows it says so.
pub struct Extension {
    pub number: i32,
    pub name: &'static str,
    pub does: &'static str,
    /// Nothing where a machine answers as soon as it is named; otherwise when it does.
    pub only: Option<&'static str>,
}

const ANOTHER_SIZE: i32 = -223;
const LAST: i32 = -224;
const POINTER_PLACE: i32 = -232;
const POINTER: i32 = -239;
const POINTER_OF_TWO_COLOURS: i32 = -240;
const KEYS_BY_PLACE: i32 = -258;
const LIGHTS: i32 = -261;
const DEVICES: i32 = -305;
const NAME: i32 = -307;
const SCREENS: i32 = -308;
const POWER: i32 = -309;
const MARKS: i32 = -312;
const UNASKED: i32 = -313;
const POINTER_SEEN_THROUGH: i32 = -314;
const CLIPBOARD: i32 = 0xc0a1_e5ce_u32 as i32;

/// The extensions that are each a way to send the pointer's shape: a machine that is
/// named several answers in the one it likes best.
pub const POINTERS: [i32; 3] = [POINTER_SEEN_THROUGH, POINTER, POINTER_OF_TWO_COLOURS];

/// The extensions this program names to a machine, and reads the answers of.
pub const KNOWN: [Extension; 15] = [
    Extension { number: CLIPBOARD, name: "ExtendedClipboard", does: "carries a clipboard with more than western text", only: None },
    Extension { number: SCREENS, name: "ExtendedDesktopSize", does: "says its screens, and takes another size when asked", only: None },
    Extension {
        number: ANOTHER_SIZE,
        name: "DesktopSize",
        does: "says that its desktop has another size",
        only: Some("when the size changes"),
    },
    Extension { number: POINTER, name: "Cursor", does: "sends the pointer's shape, to be drawn on one's own device", only: None },
    Extension {
        number: POINTER_SEEN_THROUGH,
        name: "CursorWithAlpha",
        does: "sends the pointer's shape with its see-through parts",
        only: None,
    },
    Extension { number: POINTER_OF_TWO_COLOURS, name: "XCursor", does: "sends the pointer's shape in two colours", only: None },
    Extension {
        number: POINTER_PLACE,
        name: "PointerPos",
        does: "says where the pointer is",
        only: Some("when the machine itself moves the pointer"),
    },
    Extension { number: NAME, name: "DesktopName", does: "says that its desktop has another name", only: Some("when the name changes") },
    Extension {
        number: LAST,
        name: "LastRect",
        does: "ends an update when it is done, without counting its rectangles first",
        only: Some("in an update it doesn't count"),
    },
    Extension { number: MARKS, name: "Fence", does: "marks places in what it sends, by which the line is measured", only: None },
    Extension { number: UNASKED, name: "ContinuousUpdates", does: "sends what changes without being asked each time", only: None },
    Extension { number: KEYS_BY_PLACE, name: "QEMU Extended Key Event", does: "takes keys by their place on the keyboard", only: None },
    Extension { number: LIGHTS, name: "QEMU LED State", does: "says whether caps lock and num lock are on", only: None },
    Extension { number: POWER, name: "xvp", does: "can be shut down and started again by the client", only: None },
    Extension { number: DEVICES, name: "gii", does: "takes devices other than a keyboard and a pointer", only: None },
];

/// What is said to a machine whose desktop is open: the ways of writing that are read
/// from now on, which are pixels as they are, copies, every extension, and whatever was
/// named by its number beside them; and a request for a corner of the desktop, so that
/// the machine has an update to put its answers in.
pub fn asks(also: &[i32], width: u16, height: u16) -> Vec<u8> {
    let ways: Vec<i32> = [0, 1].into_iter().chain(KNOWN.iter().map(|known| known.number)).chain(also.iter().copied()).collect();
    let mut says = vec![2, 0];
    says.extend((ways.len() as u16).to_be_bytes());
    says.extend(ways.iter().flat_map(|way| way.to_be_bytes()));
    says.extend([3, 0, 0, 0, 0, 0]);
    says.extend(width.min(16).to_be_bytes());
    says.extend(height.min(16).to_be_bytes());
    says
}

/// The longest thing a machine says in one piece that is still read to its end.
const LONGEST: usize = 256 << 20;

/// What a machine sent after it was named the extensions.
#[derive(Default)]
pub struct Heard {
    arrived: Vec<u8>,
    /// The rectangles of the update that is arriving which are still to come.
    left: Option<u16>,
    /// The extensions it answered, by their numbers, each with what it said.
    pub answered: BTreeMap<i32, String>,
    /// The ways it wrote pixels in.
    pub drew_as: BTreeSet<i32>,
    /// How long the text was that it sent as its clipboard, the plain way. Never the text.
    pub plain_text: Option<usize>,
    /// Why nothing more is read, where the machine sent what this program doesn't read.
    pub stopped: Option<String>,
}

enum Step {
    Read(usize),
    Waits,
    Stops(String),
}

fn number16(bytes: &[u8]) -> usize {
    usize::from(u16::from_be_bytes([bytes[0], bytes[1]]))
}

fn number32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

impl Heard {
    /// Takes what arrived from the machine.
    pub fn receive(&mut self, bytes: &[u8]) {
        if self.stopped.is_some() {
            return;
        }
        let mut arrived = std::mem::take(&mut self.arrived);
        arrived.extend_from_slice(bytes);
        let mut at = 0;
        while at < arrived.len() {
            match self.next(&arrived[at..]) {
                Step::Read(count) => at += count,
                Step::Waits => break,
                Step::Stops(why) => {
                    self.stopped = Some(why);
                    return;
                }
            }
        }
        self.arrived = arrived.split_off(at);
    }

    fn next(&mut self, rest: &[u8]) -> Step {
        if self.left.is_some() {
            return self.rectangle(rest);
        }
        let whole = |count: usize| if rest.len() >= count { Step::Read(count) } else { Step::Waits };
        match rest[0] {
            // an update: its rectangles follow
            0 => {
                let Some(head) = rest.get(..4) else { return Step::Waits };
                self.left = Some(u16::from_be_bytes([head[2], head[3]])).filter(|count| *count > 0);
                Step::Read(4)
            }
            // colours by their numbers, which nobody asked for
            1 => match rest.get(4..6) {
                Some(count) => whole(6 + 6 * number16(count)),
                None => Step::Waits,
            },
            // the bell
            2 => Step::Read(1),
            // the clipboard: the plain way, or the extension's where the length is below nothing
            3 => {
                let Some(length) = rest.get(4..8) else { return Step::Waits };
                let length = number32(length) as i32;
                let count = length.unsigned_abs() as usize;
                if count > LONGEST {
                    return Step::Stops(format!("It sent a clipboard of {count} bytes, which is more than this program reads."));
                }
                let Some(carried) = rest.get(8..8 + count) else { return Step::Waits };
                if length >= 0 {
                    self.plain_text = Some(count);
                } else {
                    self.answered.entry(CLIPBOARD).or_insert_with(|| clipboard(carried));
                }
                Step::Read(8 + count)
            }
            150 => {
                self.answered.entry(UNASKED).or_default();
                Step::Read(1)
            }
            248 => {
                let Some(length) = rest.get(8) else { return Step::Waits };
                self.answered.entry(MARKS).or_default();
                whole(9 + usize::from(*length))
            }
            250 => {
                let Some(said) = rest.get(..4) else { return Step::Waits };
                self.answered.entry(POWER).or_insert_with(|| format!("in its version {}", said[2]));
                Step::Read(4)
            }
            253 => {
                let Some(head) = rest.get(..4) else { return Step::Waits };
                let length = if head[1] & 0x80 != 0 { number16(&head[2..]) } else { usize::from(u16::from_le_bytes([head[2], head[3]])) };
                self.answered.entry(DEVICES).or_default();
                whole(4 + length)
            }
            kind => {
                let shown: String = rest.iter().take(16).map(|byte| format!("{byte:02x}")).collect::<Vec<_>>().join(" ");
                Step::Stops(format!(
                    "It sent a message of kind {kind}, which the protocol's core doesn't have and this program doesn't read. It begins: {shown}"
                ))
            }
        }
    }

    /// One rectangle of an update: pixels, or an extension's answer.
    fn rectangle(&mut self, rest: &[u8]) -> Step {
        let Some(head) = rest.get(..12) else { return Step::Waits };
        let (width, height) = (number16(&head[4..]), number16(&head[6..]));
        let way = number32(&head[8..]) as i32;
        let after = &rest[12..];
        // how many bytes follow, and what the rectangle said where it is an answer
        let (follow, said): (usize, Option<String>) = match way {
            0 => (width * height * 4, None),
            1 => (4, None),
            16 => match after.get(..4) {
                Some(length) => (4 + number32(length) as usize, None),
                None => return Step::Waits,
            },
            ANOTHER_SIZE => (0, Some(format!("{width} by {height} pixels"))),
            LAST | POINTER_PLACE | KEYS_BY_PLACE => (0, Some(String::new())),
            POINTER => (width * height * 4 + width.div_ceil(8) * height, Some(format!("a pointer of {width} by {height} pixels"))),
            POINTER_OF_TWO_COLOURS => (
                if width * height > 0 { 6 + 2 * width.div_ceil(8) * height } else { 0 },
                Some(format!("a pointer of {width} by {height} pixels")),
            ),
            POINTER_SEEN_THROUGH => match after.get(..4) {
                Some(inner) if number32(inner) == 0 => (4 + width * height * 4, Some(format!("a pointer of {width} by {height} pixels"))),
                Some(inner) => {
                    return Step::Stops(format!(
                        "It wrote the pointer's shape in a way numbered {}, which this program doesn't read.",
                        number32(inner) as i32
                    ));
                }
                None => return Step::Waits,
            },
            NAME => match after.get(..4) {
                Some(length) => (4 + number32(length) as usize, Some(String::new())),
                None => return Step::Waits,
            },
            SCREENS => match after.first() {
                Some(screens) => (
                    4 + 16 * usize::from(*screens),
                    Some(format!("{screens} screen{}, {width} by {height} pixels in all", if *screens == 1 { "" } else { "s" })),
                ),
                None => return Step::Waits,
            },
            LIGHTS => (1, Some(String::new())),
            way => return Step::Stops(format!("It wrote a rectangle in a way numbered {way}, which this program doesn't read.")),
        };
        if follow > LONGEST {
            return Step::Stops(format!("It sent a rectangle of {follow} bytes, which is more than this program reads."));
        }
        if after.len() < follow {
            return Step::Waits;
        }
        match said {
            Some(said) => {
                self.answered.entry(way).or_insert(said);
            }
            None => {
                self.drew_as.insert(way);
            }
        }
        // an update that doesn't count its rectangles ends with the one that says so
        self.left = match (self.left, way) {
            (_, LAST) => None,
            (Some(u16::MAX), _) => Some(u16::MAX),
            (Some(left), _) => Some(left - 1).filter(|left| *left > 0),
            (None, _) => None,
        };
        Step::Read(12 + follow)
    }
}

/// What a machine says of its clipboard where it knows the extension: the kinds it
/// carries, how much of each it sends unasked, and what it does with them.
fn clipboard(carried: &[u8]) -> String {
    let Some(flags) = carried.get(..4).map(number32) else { return "and said nothing more".to_string() };
    const KINDS: [(u32, &str); 5] =
        [(0, "text of every alphabet"), (1, "formatted text as RTF"), (2, "formatted text as HTML"), (3, "pictures"), (4, "files")];
    const DOES: [(u32, &str); 4] = [
        (25, "asks for what the other side holds"),
        (26, "asks which kinds the other side holds"),
        (27, "says which kinds it holds"),
        (28, "hands over what it holds"),
    ];
    // where it says what it can do, a size follows for each kind: how much it sends unasked
    let mut sizes = carried[4..].as_chunks::<4>().0.iter().map(|size| u32::from_be_bytes(*size));
    let says_sizes = flags & (1 << 24) != 0;
    let kinds: Vec<String> = KINDS
        .iter()
        .filter(|(bit, _)| flags & (1 << bit) != 0)
        .map(|(_, kind)| match sizes.next().filter(|_| says_sizes) {
            Some(0) => format!("{kind} (only when asked for)"),
            Some(size) => format!("{kind} (up to {size} bytes unasked)"),
            None => (*kind).to_string(),
        })
        .collect();
    let does: Vec<&str> = DOES.iter().filter(|(bit, _)| flags & (1 << bit) != 0).map(|(_, does)| *does).collect();
    let carries = if kinds.is_empty() { "no kind it names".to_string() } else { kinds.join(", ") };
    if does.is_empty() {
        format!("it carries {carries}")
    } else {
        format!("it carries {carries}; it {}", does.join(", "))
    }
}

/// A way of signing in that a machine offers, by the number the protocol gives it.
pub fn way_of_signing_in(way: u32) -> &'static str {
    match way {
        1 => "nothing asked",
        2 => "the desktop's own password",
        5 | 6 | 129 | 130 => "a key pair of the machine's maker",
        16 => "one of several, chosen in a further step",
        18 => "a secured connection first",
        19 => "a secured connection first, then one of several",
        20 => "whatever the machine's own sign-in service takes",
        30 => "a Mac's own: an account of the Mac, by its name and password",
        33 | 35 | 36 => "a Mac's own",
        _ => "one this program has no name for",
    }
}

/// The ways of signing in a machine offered, read from its first bytes: its version,
/// then one way where the machine decides, or their count and the ways themselves.
pub fn ways_of_signing_in(first: &[u8]) -> Option<Vec<u32>> {
    let version = std::str::from_utf8(first.get(..12)?).ok()?;
    let (major, minor) = version.get(4..11)?.split_once('.')?;
    let (major, minor): (u32, u32) = (major.parse().ok()?, minor.parse().ok()?);
    if (major, minor) < (3, 7) {
        return Some(vec![number32(first.get(12..16)?)]);
    }
    let count = usize::from(*first.get(12)?);
    Some(first.get(13..13 + count)?.iter().map(|way| u32::from(*way)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rectangle(width: u16, height: u16, way: i32, carried: &[u8]) -> Vec<u8> {
        let mut says = vec![0, 0, 0, 0];
        says.extend(width.to_be_bytes());
        says.extend(height.to_be_bytes());
        says.extend(way.to_be_bytes());
        says.extend(carried);
        says
    }

    fn update(rectangles: &[Vec<u8>]) -> Vec<u8> {
        let mut says = vec![0, 0];
        says.extend((rectangles.len() as u16).to_be_bytes());
        says.extend(rectangles.concat());
        says
    }

    /// What a machine that knows much sends: an update with its screens, a pointer and
    /// a corner of pixels; what its clipboard carries; a mark; and that it sends unasked.
    fn of_a_machine_that_knows_much() -> Vec<u8> {
        let mut says = update(&[
            rectangle(1920, 1080, SCREENS, &[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 7, 128, 4, 56, 0, 0, 0, 0]),
            rectangle(2, 2, POINTER, &[9; 2 * 2 * 4 + 2]),
            rectangle(2, 1, 0, &[7; 8]),
        ]);
        // the clipboard: what it can do, text and pictures, with a size for each
        says.extend([3, 0, 0, 0]);
        says.extend((-12i32).to_be_bytes());
        says.extend((1u32 << 24 | 1 << 25 | 1 << 27 | 1 << 28 | 1 << 3 | 1).to_be_bytes());
        says.extend(20_000_000u32.to_be_bytes());
        says.extend(0u32.to_be_bytes());
        // a mark of four bytes
        says.extend([248, 0, 0, 0, 0, 0, 0, 1, 4, 1, 2, 3, 4]);
        says.push(150);
        says
    }

    #[test]
    fn it_names_every_extension_once_and_asks_for_a_corner() {
        let says = asks(&[1011], 1920, 8);
        assert_eq!(&says[..4], &[2, 0, 0, 18]);
        let named: Vec<i32> = says[4..4 + 18 * 4].as_chunks::<4>().0.iter().map(|way| i32::from_be_bytes(*way)).collect();
        assert_eq!(&named[..3], &[0, 1, CLIPBOARD]);
        assert_eq!(named.last(), Some(&1011));
        assert_eq!(named.iter().collect::<BTreeSet<_>>().len(), named.len());
        assert_eq!(&says[4 + 18 * 4..], &[3, 0, 0, 0, 0, 0, 0, 16, 0, 8]);
    }

    #[test]
    fn what_a_machine_answers_is_read_however_it_arrives() {
        let says = of_a_machine_that_knows_much();
        let mut at_once = Heard::default();
        at_once.receive(&says);
        let mut byte_by_byte = Heard::default();
        says.iter().for_each(|byte| byte_by_byte.receive(&[*byte]));
        for heard in [at_once, byte_by_byte] {
            assert_eq!(heard.stopped, None);
            assert_eq!(heard.answered.keys().copied().collect::<Vec<_>>(), [CLIPBOARD, UNASKED, MARKS, SCREENS, POINTER]);
            assert_eq!(heard.answered[&SCREENS], "1 screen, 1920 by 1080 pixels in all");
            assert_eq!(heard.answered[&POINTER], "a pointer of 2 by 2 pixels");
            assert_eq!(
                heard.answered[&CLIPBOARD],
                "it carries text of every alphabet (up to 20000000 bytes unasked), pictures (only when asked for); \
                 it asks for what the other side holds, says which kinds it holds, hands over what it holds"
            );
            assert_eq!(heard.drew_as.iter().copied().collect::<Vec<_>>(), [0]);
            assert_eq!(heard.plain_text, None);
        }
    }

    #[test]
    fn a_plain_clipboard_is_counted_and_what_is_not_read_ends_the_reading() {
        let mut heard = Heard::default();
        // five bytes of text the plain way, an update that ends by saying so, then a kind nobody reads
        heard.receive(&[3, 0, 0, 0, 0, 0, 0, 5, b'h', b'e', b'l', b'l', b'o']);
        heard.receive(&[0, 0, 0xff, 0xff]);
        heard.receive(&rectangle(1, 1, 16, &[0, 0, 0, 2, 5, 6]));
        heard.receive(&rectangle(0, 0, LAST, &[]));
        assert_eq!((heard.plain_text, heard.stopped.clone()), (Some(5), None));
        assert_eq!(heard.drew_as.iter().copied().collect::<Vec<_>>(), [16]);
        assert!(heard.answered.contains_key(&LAST));
        heard.receive(&[0x1f, 1, 2, 3]);
        assert!(heard.stopped.as_deref().is_some_and(|why| why.contains("of kind 31") && why.ends_with("1f 01 02 03")));
        // and nothing after it is taken
        heard.receive(&[150]);
        assert!(!heard.answered.contains_key(&UNASKED));

        let mut other = Heard::default();
        other.receive(&update(&[rectangle(4, 4, 1011, &[])]));
        assert!(other.stopped.as_deref().is_some_and(|why| why.contains("numbered 1011")));
    }

    #[test]
    fn the_ways_of_signing_in_are_read_from_a_machines_first_bytes() {
        assert_eq!(ways_of_signing_in(b"RFB 003.889\n\x04\x1e\x21\x02\x23"), Some(vec![30, 33, 2, 35]));
        assert_eq!(ways_of_signing_in(b"RFB 003.003\n\x00\x00\x00\x02"), Some(vec![2]));
        assert_eq!(ways_of_signing_in(b"RFB 003.008\n\x02\x01"), None);
        assert_eq!(way_of_signing_in(2), "the desktop's own password");
    }
}
