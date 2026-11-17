//! A connection to a machine that speaks VNC, from the machine's first word to its open
//! desktop (RFC 6143).
//!
//! The machine speaks first: the version it speaks. Then it says how it signs people in,
//! which is with a password of its own or not at all, asks for the password where it has
//! one, and says how large its desktop is. From there on it draws: whenever the client
//! asks, it sends the rectangles that changed since the last time, and the client asks
//! again once it has them. Keys and the pointer go the other way, and text that was
//! copied goes either way.

use away_secret::Secret;
use flate2::{Decompress, FlushDecompress, Status};

use crate::picture::{Area, Picture};
use crate::{password, zrle, Error};

/// What happened, for whoever holds the connection to show or to act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Happened {
    /// The machine said which version of VNC it speaks.
    Speaks(String),
    /// The machine asks for its password.
    Asks,
    /// The machine took the password, or asked for none.
    SignedIn,
    /// The desktop is open: how large it is, and what the machine calls it.
    Open {
        /// How wide, in pixels.
        width: u16,
        /// How high.
        height: u16,
        /// What the machine calls its desktop.
        name: String,
    },
    /// The desktop has another size now, and all of it is drawn anew.
    Resized {
        /// How wide, in pixels.
        width: u16,
        /// How high.
        height: u16,
    },
    /// The machine drew: these areas of the picture are new.
    Drawn(Vec<Area>),
    /// This text was copied on the machine.
    Copied(String),
}

/// A button of the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    /// The main one.
    Left,
    /// The one in the middle.
    Middle,
    /// The one that opens a menu.
    Right,
    /// The one that goes back.
    Back,
}

/// One thing a person does with the keys or the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    /// The pointer moved to this place of the desktop.
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
    /// The wheel turned at this place: 120 is one notch away from the person, or to the
    /// right, and a trackpad turns it by less at a time.
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
    /// A key went down or came up: the key by what it writes or does, as X11 numbers it.
    Key {
        /// The key's number.
        sym: u32,
        /// Down, or up again.
        down: bool,
    },
}

/// Which ways text that was copied passes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Passes {
    /// What the person copied can be pasted on the machine.
    pub to_the_machine: bool,
    /// What is copied on the machine reaches the person.
    pub from_the_machine: bool,
}

/// The ways a machine writes a rectangle that Away reads, and the one word on the
/// desktop itself: that it has another size.
const AS_IT_IS: i32 = 0;
const COPIED_FROM_THE_PICTURE: i32 = 1;
const PACKED: i32 = 16;
const ANOTHER_SIZE: i32 = -223;

/// The most pixels a desktop has: those of two screens of 5120 by 2880. Every one of
/// them is kept, compared and sent.
const MOST_PIXELS: usize = 2 * 5120 * 2880;
/// The most a text, a name or a packed rectangle holds.
const LONGEST_TEXT: usize = 1 << 20;
const LARGEST_PACKED: usize = 64 << 20;
/// One notch of a wheel, as the person's side counts it.
const A_NOTCH: i32 = 120;
/// How many steps of the wheel one event of the person's is turned into, at most.
const MOST_STEPS: i32 = 600;

/// The versions of the protocol, which differ in how the sign-in is agreed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Version {
    /// The machine decides how it signs people in, and says so.
    Decides,
    /// The machine offers ways, and the client chooses.
    Offers,
    /// The same, and the machine says why where it refuses.
    OffersAndSaysWhy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    AwaitsVersion,
    AwaitsWays,
    AwaitsWay,
    AwaitsQuestion,
    AwaitsPassword,
    AwaitsResult,
    AwaitsWhy { password: bool },
    AwaitsDesktop,
    Open,
}

/// A connection to a machine. It takes the bytes that arrive, and gives the bytes to
/// send and what happened.
pub struct Connection {
    state: State,
    version: Version,
    /// Whether the machine is a Mac, which says so with the version it speaks.
    a_mac: bool,
    /// What arrived and is not read yet, and how far it is read.
    arrived: Vec<u8>,
    read: usize,
    password: Option<Secret>,
    asked: Option<[u8; 16]>,
    picture: Picture,
    /// How many rectangles of the update that is arriving are still to come.
    rectangles: u16,
    /// Whether the update that is arriving said that the desktop has another size.
    another_size: bool,
    /// The stream every packed rectangle of the connection is part of.
    unpacks: Decompress,
    passes: Passes,
    /// The buttons that are down, and where the pointer is.
    buttons: u8,
    pointer: (u16, u16),
    /// What the wheel turned and is no whole step yet: up and down, and sideways.
    turned: (i32, i32),
}

fn number16(bytes: &[u8]) -> u16 {
    u16::from_be_bytes([bytes[0], bytes[1]])
}

fn number32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

/// A text of the machine's, which the protocol has in the western alphabet's eight bits.
fn text(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| char::from(*byte)).collect()
}

impl Connection {
    /// A connection that begins: the machine speaks first. With a password, the
    /// machine's question for it is answered at once; without one, whoever holds the
    /// connection is told that the machine asks, and gives it then.
    pub fn begin(password: Option<Secret>, passes: Passes) -> Connection {
        Connection {
            state: State::AwaitsVersion,
            version: Version::OffersAndSaysWhy,
            a_mac: false,
            arrived: Vec::new(),
            read: 0,
            password,
            asked: None,
            picture: Picture::new(0, 0),
            rectangles: 0,
            another_size: false,
            unpacks: Decompress::new(true),
            passes,
            buttons: 0,
            pointer: (0, 0),
            turned: (0, 0),
        }
    }

    /// Takes what arrived from the machine. Gives what to send, and what happened.
    pub fn receive(&mut self, bytes: &[u8]) -> Result<(Vec<u8>, Vec<Happened>), Error> {
        self.arrived.extend_from_slice(bytes);
        let (mut send, mut happened, mut drawn) = (Vec::new(), Vec::new(), Vec::new());
        let read = loop {
            match self.next(&mut send, &mut happened, &mut drawn) {
                Ok(true) => {}
                Ok(false) => break Ok(()),
                Err(error) => break Err(error),
            }
        };
        // what was read is forgotten, and what is not whole yet waits for the rest
        self.arrived.drain(..self.read);
        self.read = 0;
        read?;
        if !drawn.is_empty() {
            happened.push(Happened::Drawn(drawn));
        }
        Ok((send, happened))
    }

    /// The next bytes that arrived, where that many are there.
    fn ahead(&self, count: usize) -> Option<&[u8]> {
        self.arrived.get(self.read..self.read + count)
    }

    /// Reads the next thing the machine said, where all of it arrived. Whether it did.
    fn next(&mut self, send: &mut Vec<u8>, happened: &mut Vec<Happened>, drawn: &mut Vec<Area>) -> Result<bool, Error> {
        match self.state {
            State::AwaitsVersion => {
                let Some(line) = self.ahead(12) else { return Ok(false) };
                const NO_VERSION: Error = Error::NotVnc("the machine's first word is no version of VNC");
                let said =
                    std::str::from_utf8(line).ok().filter(|said| said.starts_with("RFB ") && said.ends_with('\n')).ok_or(NO_VERSION)?;
                let (major, minor) = said[4..11].split_once('.').ok_or(NO_VERSION)?;
                let (major, minor): (u32, u32) = (major.parse().map_err(|_| NO_VERSION)?, minor.parse().map_err(|_| NO_VERSION)?);
                let speaks = said[4..11].to_string();
                // a Mac's own screen sharing speaks a version no other machine does
                self.a_mac = (major, minor) == (3, 889);
                let (version, answers) = match (major, minor) {
                    (3, 3..=6) => (Version::Decides, "RFB 003.003\n"),
                    (3, 7) => (Version::Offers, "RFB 003.007\n"),
                    (3, 8..) | (4.., _) => (Version::OffersAndSaysWhy, "RFB 003.008\n"),
                    _ => return Err(Error::NotVnc("the machine speaks a version of VNC older than any that is written down")),
                };
                self.version = version;
                self.read += 12;
                send.extend(answers.as_bytes());
                happened.push(Happened::Speaks(speaks));
                self.state = if version == Version::Decides { State::AwaitsWay } else { State::AwaitsWays };
            }
            // the ways the machine signs people in: Away takes the password where the
            // machine has one, and nothing where it asks for nothing
            State::AwaitsWays => {
                let Some(&[count]) = self.ahead(1) else { return Ok(false) };
                if count == 0 {
                    self.read += 1;
                    self.state = State::AwaitsWhy { password: false };
                    return Ok(true);
                }
                let Some(ways) = self.ahead(1 + usize::from(count)) else { return Ok(false) };
                let (with_a_password, with_nothing) = (ways[1..].contains(&2), ways[1..].contains(&1));
                self.read += 1 + usize::from(count);
                if with_a_password {
                    send.push(2);
                    self.state = State::AwaitsQuestion;
                } else if with_nothing {
                    send.push(1);
                    match self.version {
                        Version::OffersAndSaysWhy => self.state = State::AwaitsResult,
                        _ => self.signed_in(send, happened),
                    }
                } else {
                    return Err(Error::NotYet("signs people in only in ways of its own, and with no password of the desktop's"));
                }
            }
            State::AwaitsWay => {
                let Some(way) = self.ahead(4) else { return Ok(false) };
                let way = number32(way);
                self.read += 4;
                match way {
                    0 => self.state = State::AwaitsWhy { password: false },
                    1 => self.signed_in(send, happened),
                    2 => self.state = State::AwaitsQuestion,
                    _ => return Err(Error::NotYet("signs people in only in ways of its own, and with no password of the desktop's")),
                }
            }
            State::AwaitsQuestion => {
                let Some(asked) = self.ahead(16) else { return Ok(false) };
                let asked: [u8; 16] = asked.try_into().expect("sixteen bytes");
                self.read += 16;
                self.asked = Some(asked);
                match self.password.take() {
                    Some(password) => {
                        send.extend(password::answer(&asked, &password));
                        self.state = State::AwaitsResult;
                    }
                    None => {
                        happened.push(Happened::Asks);
                        self.state = State::AwaitsPassword;
                    }
                }
            }
            // nothing is read until the password is given
            State::AwaitsPassword => return Ok(false),
            State::AwaitsResult => {
                let Some(result) = self.ahead(4) else { return Ok(false) };
                let result = number32(result);
                self.read += 4;
                let password = self.asked.is_some();
                match (result, self.version) {
                    (0, _) => self.signed_in(send, happened),
                    (_, Version::OffersAndSaysWhy) => self.state = State::AwaitsWhy { password },
                    (_, _) if password => return Err(Error::PasswordRefused(String::new())),
                    _ => return Err(Error::Refused(String::new())),
                }
            }
            State::AwaitsWhy { password } => {
                let Some(length) = self.ahead(4) else { return Ok(false) };
                let length = number32(length) as usize;
                if length > LONGEST_TEXT {
                    return Err(Error::NotVnc("the machine's reason is longer than a reason is"));
                }
                let Some(why) = self.ahead(4 + length) else { return Ok(false) };
                let why = text(&why[4..]);
                self.read += 4 + length;
                return Err(if password { Error::PasswordRefused(why) } else { Error::Refused(why) });
            }
            State::AwaitsDesktop => {
                let Some(head) = self.ahead(24) else { return Ok(false) };
                let (width, height, length) = (number16(head), number16(&head[2..]), number32(&head[20..]) as usize);
                if length > LONGEST_TEXT {
                    return Err(Error::NotVnc("the desktop's name is longer than a name is"));
                }
                let Some(all) = self.ahead(24 + length) else { return Ok(false) };
                let name = text(&all[24..]);
                self.read += 24 + length;
                self.picture = self.of_size(width, height)?;
                // the pixels as Away keeps them, the ways of writing a rectangle it reads, and
                // then all of the desktop
                send.extend(pixels_as_away_keeps_them());
                send.extend(reads(&[PACKED, COPIED_FROM_THE_PICTURE, AS_IT_IS, ANOTHER_SIZE]));
                send.extend(self.asks_for(false));
                happened.push(Happened::Open { width, height, name });
                self.state = State::Open;
            }
            State::Open => return self.said(send, happened, drawn),
        }
        Ok(true)
    }

    /// The machine took the sign-in: the client says that others may see the desktop
    /// too, which leaves whoever sits at the machine where they are.
    fn signed_in(&mut self, send: &mut Vec<u8>, happened: &mut Vec<Happened>) {
        send.push(1);
        happened.push(Happened::SignedIn);
        self.state = State::AwaitsDesktop;
    }

    /// A black picture for a desktop of this size, where Away keeps one that large.
    fn of_size(&self, width: u16, height: u16) -> Result<Picture, Error> {
        let (width, height) = (usize::from(width), usize::from(height));
        if width == 0 || height == 0 {
            return Err(Error::NotVnc("the desktop has no size"));
        }
        if width * height > MOST_PIXELS {
            return Err(Error::NotYet("has a desktop of more pixels than two screens of 5120 by 2880"));
        }
        Ok(Picture::new(width, height))
    }

    /// Asks the machine to draw: all of the desktop, or what changed since it last drew.
    fn asks_for(&self, only_what_changed: bool) -> Vec<u8> {
        let mut asks = vec![3, u8::from(only_what_changed), 0, 0, 0, 0];
        asks.extend((self.picture.width as u16).to_be_bytes());
        asks.extend((self.picture.height as u16).to_be_bytes());
        asks
    }

    /// Reads what the machine says once the desktop is open.
    fn said(&mut self, send: &mut Vec<u8>, happened: &mut Vec<Happened>, drawn: &mut Vec<Area>) -> Result<bool, Error> {
        if self.rectangles > 0 {
            if !self.rectangle(happened, drawn)? {
                return Ok(false);
            }
            self.rectangles -= 1;
            if self.rectangles == 0 {
                self.drew(send);
            }
            return Ok(true);
        }
        let Some(&[kind]) = self.ahead(1) else { return Ok(false) };
        match kind {
            // it draws: how many rectangles follow
            0 => {
                let Some(head) = self.ahead(4) else { return Ok(false) };
                self.rectangles = number16(&head[2..]);
                self.read += 4;
                if self.rectangles == 0 {
                    self.drew(send);
                }
            }
            // colours for pixels that are numbers of a table: Away asks for pixels that are colours
            1 => {
                let Some(head) = self.ahead(6) else { return Ok(false) };
                let length = 6 + usize::from(number16(&head[4..])) * 6;
                if self.ahead(length).is_none() {
                    return Ok(false);
                }
                self.read += length;
            }
            // a bell
            2 => self.read += 1,
            // text that was copied on the machine
            3 => {
                let Some(head) = self.ahead(8) else { return Ok(false) };
                let length = number32(&head[4..]) as usize;
                if length > LONGEST_TEXT {
                    return Err(Error::NotVnc("a text that was copied is longer than Away takes one"));
                }
                let Some(all) = self.ahead(8 + length) else { return Ok(false) };
                if self.passes.from_the_machine {
                    happened.push(Happened::Copied(text(&all[8..])));
                }
                self.read += 8 + length;
            }
            _ => return Err(Error::NotVnc("the machine says something of a kind that VNC doesn't have")),
        }
        Ok(true)
    }

    /// The machine drew everything it had: it is asked again, for what changes from now
    /// on, or for all of a desktop that has another size.
    fn drew(&mut self, send: &mut Vec<u8>) {
        let only_what_changed = !std::mem::take(&mut self.another_size);
        send.extend(self.asks_for(only_what_changed));
    }

    /// Reads the next rectangle of the update that is arriving, and puts it into the
    /// picture, where all of it arrived. Whether it did.
    fn rectangle(&mut self, happened: &mut Vec<Happened>, drawn: &mut Vec<Area>) -> Result<bool, Error> {
        const OUTSIDE: Error = Error::NotVnc("a rectangle lies outside the desktop");
        let Some(head) = self.ahead(12) else { return Ok(false) };
        let area = Area {
            left: usize::from(number16(head)),
            top: usize::from(number16(&head[2..])),
            width: usize::from(number16(&head[4..])),
            height: usize::from(number16(&head[6..])),
        };
        let written = number32(&head[8..]) as i32;
        match written {
            AS_IT_IS => {
                if !self.picture.holds(area) {
                    return Err(OUTSIDE);
                }
                let length = area.width * area.height * 4;
                let Some(all) = self.arrived.get(self.read + 12..self.read + 12 + length) else { return Ok(false) };
                for (row, pixels) in all.chunks_exact((area.width * 4).max(1)).enumerate() {
                    let at = ((area.top + row) * self.picture.width + area.left) * 4;
                    let to = &mut self.picture.pixels[at..at + area.width * 4];
                    to.copy_from_slice(pixels);
                    to.as_chunks_mut::<4>().0.iter_mut().for_each(|pixel| pixel[3] = 0xff);
                }
                self.read += 12 + length;
            }
            COPIED_FROM_THE_PICTURE => {
                let Some(all) = self.ahead(16) else { return Ok(false) };
                let from = (usize::from(number16(&all[12..])), usize::from(number16(&all[14..])));
                if !self.picture.holds(area) || !self.picture.holds(Area { left: from.0, top: from.1, ..area }) {
                    return Err(OUTSIDE);
                }
                self.picture.copy(from, area);
                self.read += 16;
            }
            PACKED => {
                let Some(all) = self.ahead(16) else { return Ok(false) };
                let length = number32(&all[12..]) as usize;
                if length > LARGEST_PACKED {
                    return Err(Error::NotVnc("a packed rectangle is larger than one is"));
                }
                if !self.picture.holds(area) {
                    return Err(OUTSIDE);
                }
                let Some(packed) = self.arrived.get(self.read + 16..self.read + 16 + length) else { return Ok(false) };
                // a tile holds a byte for its kind, and three bytes a pixel at most: with colours before them
                let tiles = area.width.div_ceil(zrle::TILE) * area.height.div_ceil(zrle::TILE);
                let most = area.width * area.height * 3 + tiles * (1 + 127 * 3) + 64;
                let unpacked = unpack(&mut self.unpacks, packed, most)?;
                zrle::draw(&unpacked, area, &mut self.picture)?;
                self.read += 16 + length;
            }
            // the desktop has another size: its picture begins anew, and all of it is asked for
            ANOTHER_SIZE => {
                let (width, height) = (area.width as u16, area.height as u16);
                self.picture = self.of_size(width, height)?;
                self.another_size = true;
                drawn.clear();
                happened.retain(|happened| !matches!(happened, Happened::Drawn(_)));
                happened.push(Happened::Resized { width, height });
                self.read += 12;
                return Ok(true);
            }
            _ => return Err(Error::NotYet("writes what it draws in a way Away did not ask for")),
        }
        if area.width > 0 && area.height > 0 {
            drawn.push(area);
        }
        Ok(true)
    }

    /// The answer to the machine's question for its password, once somebody gave it.
    /// Nothing where the machine did not ask.
    pub fn password(&mut self, password: &Secret) -> Vec<u8> {
        match (self.state, self.asked) {
            (State::AwaitsPassword, Some(asked)) => {
                self.state = State::AwaitsResult;
                password::answer(&asked, password).to_vec()
            }
            _ => Vec::new(),
        }
    }

    /// The desktop's picture, as far as the machine has drawn it.
    pub fn picture(&self) -> &Picture {
        &self.picture
    }

    /// Whether the desktop is open.
    pub fn is_open(&self) -> bool {
        self.state == State::Open
    }

    /// Whether the machine is a Mac, as the version it speaks says.
    pub fn is_a_mac(&self) -> bool {
        self.a_mac
    }

    /// What to send to tell the machine of keys and of the pointer. Nothing before the
    /// desktop is open.
    pub fn input(&mut self, events: &[Input]) -> Vec<u8> {
        if self.state != State::Open {
            return Vec::new();
        }
        let mut send = Vec::new();
        for event in events {
            match *event {
                Input::Moved { x, y } => {
                    self.at(x, y);
                    self.points(&mut send, self.buttons);
                }
                Input::Button { button, down, x, y } => {
                    let bit = match button {
                        Button::Left => 0x01,
                        Button::Middle => 0x02,
                        Button::Right => 0x04,
                        Button::Back => 0x80,
                    };
                    self.buttons = if down { self.buttons | bit } else { self.buttons & !bit };
                    self.at(x, y);
                    self.points(&mut send, self.buttons);
                }
                Input::Wheel { by, sideways, x, y } => {
                    self.at(x, y);
                    let steps = self.steps(by, sideways);
                    // a step of the wheel is a button of its own that goes down and comes up
                    let bit = match (sideways, steps > 0) {
                        (false, true) => 0x08,
                        (false, false) => 0x10,
                        (true, false) => 0x20,
                        (true, true) => 0x40,
                    };
                    for _ in 0..steps.unsigned_abs() {
                        self.points(&mut send, self.buttons | bit);
                        self.points(&mut send, self.buttons);
                    }
                }
                Input::Key { sym, down } => {
                    send.extend([4, u8::from(down), 0, 0]);
                    send.extend(sym.to_be_bytes());
                }
            }
        }
        send
    }

    /// The pointer is at this place, or at the desktop's edge where that is beyond it.
    fn at(&mut self, x: u16, y: u16) {
        let within = |place: u16, size: usize| place.min(size.saturating_sub(1) as u16);
        self.pointer = (within(x, self.picture.width), within(y, self.picture.height));
    }

    fn points(&self, send: &mut Vec<u8>, buttons: u8) {
        send.extend([5, buttons]);
        send.extend(self.pointer.0.to_be_bytes());
        send.extend(self.pointer.1.to_be_bytes());
    }

    /// How many steps of the wheel a turn is, and which way. A step is what VNC has of
    /// a wheel, and machines make different things of one:
    ///
    /// - most scroll by a few lines for a step, as for a notch of a wheel. A trackpad
    ///   turns by less than a notch at a time, and what is left adds up to the next step;
    /// - a Mac's own screen sharing scrolls by one pixel for a step. A turn is as many
    ///   steps as the pixels it stands for, which is as smooth and as far as on the
    ///   person's own device: a notch is a hundred of them.
    fn steps(&mut self, by: i16, sideways: bool) -> i32 {
        // a notch is 120, and stands for 100 pixels: a pixel is 6 of 5 parts
        let (times, a_step) = if self.a_mac { (5, 6) } else { (1, A_NOTCH) };
        let turned = if sideways { &mut self.turned.1 } else { &mut self.turned.0 };
        // a turn the other way begins anew: what was left of the way before is forgotten
        if (*turned > 0) != (by > 0) {
            *turned = 0;
        }
        *turned += i32::from(by) * times;
        let steps = (*turned / a_step).clamp(-MOST_STEPS, MOST_STEPS);
        *turned %= a_step;
        steps
    }

    /// What to send to tell the machine that the person copied this text, so that it can
    /// be pasted there. Nothing where the clipboard doesn't pass to the machine, or the
    /// desktop is not open. The protocol has the western alphabet's eight bits, and a
    /// character beyond them travels as a question mark.
    pub fn copied(&self, text: &str) -> Vec<u8> {
        if self.state != State::Open || !self.passes.to_the_machine {
            return Vec::new();
        }
        let bytes: Vec<u8> = text.chars().map(|character| u8::try_from(u32::from(character)).unwrap_or(b'?')).take(LONGEST_TEXT).collect();
        let mut send = vec![6, 0, 0, 0];
        send.extend((bytes.len() as u32).to_be_bytes());
        send.extend(bytes);
        send
    }
}

/// The pixels as Away keeps them: 32 bits a pixel of which 24 are colour, the lowest byte
/// first, and in it blue, then green, then red.
fn pixels_as_away_keeps_them() -> Vec<u8> {
    let mut says = vec![0, 0, 0, 0, 32, 24, 0, 1];
    for most in [255u16, 255, 255] {
        says.extend(most.to_be_bytes());
    }
    says.extend([16, 8, 0, 0, 0, 0]);
    says
}

/// The ways of writing a rectangle that the client reads, the one it likes best first.
fn reads(ways: &[i32]) -> Vec<u8> {
    let mut says = vec![2, 0];
    says.extend((ways.len() as u16).to_be_bytes());
    says.extend(ways.iter().flat_map(|way| way.to_be_bytes()));
    says
}

/// Unpacks the next part of the connection's stream: at most this many bytes.
fn unpack(unpacks: &mut Decompress, packed: &[u8], most: usize) -> Result<Vec<u8>, Error> {
    const WRONG: Error = Error::NotVnc("a packed rectangle does not unpack");
    let mut unpacked = Vec::with_capacity((packed.len() * 4).clamp(1024, most.max(1024)));
    let mut read = 0;
    loop {
        let before = unpacks.total_in();
        let status = unpacks.decompress_vec(&packed[read..], &mut unpacked, FlushDecompress::Sync).map_err(|_| WRONG)?;
        read += (unpacks.total_in() - before) as usize;
        // all of it is read, and what it unpacks to had room
        if (read == packed.len() && unpacked.len() < unpacked.capacity()) || status == Status::StreamEnd {
            return Ok(unpacked);
        }
        if unpacked.len() > most {
            return Err(Error::NotVnc("a packed rectangle unpacks to more than its size holds"));
        }
        unpacked.reserve(unpacked.capacity().max(4096));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stand_in::{Machine, CLICKED, MARK};

    /// A picture with flat parts, slopes, noise and stripes, of a size that is no
    /// multiple of a tile.
    fn picture() -> Picture {
        let mut picture = Picture::new(200, 120);
        let mut noise = 1u32;
        for top in 0..picture.height {
            for left in 0..picture.width {
                noise = noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let pixel = match left / 50 {
                    0 => [0xf3, 0xf2, 0xf1],
                    1 => [(top * 2) as u8, left as u8, (top + left) as u8],
                    2 => [(noise >> 8) as u8, (noise >> 16) as u8, (noise >> 24) as u8],
                    _ => [if top % 2 == 0 { 0 } else { 255 }; 3],
                };
                picture.set(left, top, pixel);
            }
        }
        picture
    }

    /// Passes what one side says to the other, and back, until both are silent.
    fn talk(connection: &mut Connection, machine: &mut Machine, mut to_machine: Vec<u8>) -> Result<Vec<Happened>, Error> {
        let mut happened = Vec::new();
        while !to_machine.is_empty() {
            let to_client = machine.answer(&to_machine).expect("the machine takes what the client says");
            // in pieces of an odd size, as a connection carries them
            to_machine = Vec::new();
            for piece in to_client.chunks(1021) {
                let (send, more) = connection.receive(piece)?;
                to_machine.extend(send);
                happened.extend(more);
            }
        }
        Ok(happened)
    }

    /// Opens a connection to a stand-in, from its first word on.
    fn open(password: Option<&str>, mut machine: Machine) -> (Result<Vec<Happened>, Error>, Connection, Machine) {
        let passes = Passes { to_the_machine: true, from_the_machine: true };
        let mut connection = Connection::begin(password.map(Secret::new), passes);
        let greeted = connection.receive(&machine.greets());
        let happened = greeted.and_then(|(send, mut happened)| {
            happened.extend(talk(&mut connection, &mut machine, send)?);
            Ok(happened)
        });
        (happened, connection, machine)
    }

    #[test]
    fn a_desktop_opens_with_its_password_and_its_picture_arrives_pixel_for_pixel() {
        let (happened, connection, machine) = open(Some("a password"), Machine::new(picture()).with_the_password("a password"));
        let happened = happened.expect("the desktop opens");
        assert_eq!(
            happened[..3],
            [
                Happened::Speaks("003.008".to_string()),
                Happened::SignedIn,
                Happened::Open { width: 200, height: 120, name: "a stand-in's desktop".to_string() },
            ]
        );
        assert!(connection.is_open() && !connection.is_a_mac());
        assert!(*connection.picture() == *machine.picture(), "the picture is the machine's");
        // every pixel was said to be drawn, and none twice
        let drawn: usize = happened
            .iter()
            .filter_map(|happened| if let Happened::Drawn(areas) = happened { Some(areas) } else { None })
            .flatten()
            .map(|area| area.width * area.height)
            .sum();
        assert_eq!(drawn, 200 * 120);

        // what the machine saw of the client: the version, the password's way, that others
        // may see the desktop too, the pixels Away keeps, and the ways it reads
        let seen = machine.seen();
        assert_eq!((seen.answered.as_deref(), seen.chose, seen.shared), (Some("RFB 003.008\n"), Some(2), Some(true)));
        assert_eq!(seen.pixels, Some([32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0]));
        assert_eq!(seen.reads, [16, 1, 0, -223]);
        // all of the desktop was asked for once, and then what changes
        assert_eq!(seen.asked_for, [(false, 200, 120), (true, 200, 120)]);
    }

    #[test]
    fn a_machine_that_asks_is_given_the_password_later_and_a_wrong_one_is_refused() {
        // without a password at hand, whoever holds the connection is told that the machine asks
        let (happened, mut connection, mut machine) = open(None, Machine::new(picture()).with_the_password("a password"));
        assert_eq!(happened, Ok(vec![Happened::Speaks("003.008".to_string()), Happened::Asks]));
        assert!(connection.input(&[Input::Key { sym: 0x61, down: true }]).is_empty());
        let answer = connection.password(&Secret::new("a password"));
        assert_eq!(answer.len(), 16);
        let happened = talk(&mut connection, &mut machine, answer).expect("the desktop opens");
        assert!(happened.contains(&Happened::SignedIn) && connection.is_open());
        // and not twice
        assert!(connection.password(&Secret::new("a password")).is_empty());

        // a wrong password is refused, with the machine's reason where it speaks the version that gives one
        let (happened, _, _) = open(Some("another"), Machine::new(picture()).with_the_password("a password"));
        assert_eq!(happened, Err(Error::PasswordRefused("Authentication failed".to_string())));
        for older in ["003.007", "003.003"] {
            let (happened, _, _) = open(Some("another"), Machine::new(picture()).with_the_password("a password").speaking(older));
            assert_eq!(happened, Err(Error::PasswordRefused(String::new())), "{older}");
        }
        // a machine that takes nobody says why
        let (happened, _, _) = open(Some("a password"), Machine::new(picture()).that_takes_nobody("Too many security failures"));
        assert_eq!(happened, Err(Error::Refused("Too many security failures".to_string())));
    }

    #[test]
    fn every_version_and_a_machine_without_a_password_open_the_same_desktop() {
        for (speaks, answered) in
            [("003.003", "RFB 003.003\n"), ("003.007", "RFB 003.007\n"), ("003.008", "RFB 003.008\n"), ("004.001", "RFB 003.008\n")]
        {
            for password in [Some("a password"), None] {
                let machine = Machine::new(picture()).speaking(speaks);
                let machine = match password {
                    Some(password) => machine.with_the_password(password),
                    None => machine,
                };
                let (happened, connection, machine) = open(password, machine);
                let happened = happened.unwrap_or_else(|error| panic!("{speaks}, {password:?}: {error}"));
                assert_eq!(happened[0], Happened::Speaks(speaks.to_string()));
                assert!(happened.contains(&Happened::SignedIn) && !happened.contains(&Happened::Asks));
                assert!(*connection.picture() == *machine.picture(), "{speaks}, {password:?}");
                assert_eq!(machine.seen().answered.as_deref(), Some(answered));
                assert_eq!(machine.seen().chose, if speaks == "003.003" { None } else { Some(if password.is_some() { 2 } else { 1 }) });
            }
        }
        // a Mac speaks a version of its own, offers ways of its own beside the password,
        // and is taken with the password
        let (happened, connection, machine) = open(Some("a password"), Machine::new(picture()).with_the_password("a password").as_a_mac());
        assert_eq!(happened.unwrap()[0], Happened::Speaks("003.889".to_string()));
        assert!(connection.is_a_mac());
        assert_eq!((machine.seen().answered.as_deref(), machine.seen().chose), (Some("RFB 003.008\n"), Some(2)));
        // one that has only ways of its own is not signed in on
        let (happened, _, _) = open(Some("a password"), Machine::new(picture()).as_a_mac().that_offers(&[30, 33, 35]));
        assert!(matches!(happened, Err(Error::NotYet(_))));
        // and what is no VNC is refused at its first word
        let mut connection = Connection::begin(None, Passes::default());
        assert!(matches!(connection.receive(b"SSH-2.0-open\r\n"), Err(Error::NotVnc(_))));
    }

    #[test]
    fn what_changes_on_the_machine_arrives_and_a_part_that_moved_is_copied() {
        let (_, mut connection, mut machine) = open(None, Machine::new(picture()));
        // the left button goes down: the machine draws a mark there, and the client is told that area alone
        let pressed = connection.input(&[Input::Button { button: Button::Left, down: true, x: 50, y: 60 }]);
        let happened = talk(&mut connection, &mut machine, pressed).unwrap();
        assert_eq!(happened, [Happened::Drawn(vec![Area { left: 50, top: 60, width: MARK, height: MARK }])]);
        assert_eq!(&connection.picture().pixels[(65 * 200 + 55) * 4..][..4], &[CLICKED[0], CLICKED[1], CLICKED[2], 0xff]);
        assert!(*connection.picture() == *machine.picture());

        // a part of the screen moves: the client copies it within its own picture
        let moved = machine.moves(Area { left: 40, top: 50, width: 60, height: 40 }, (100, 70));
        let (send, happened) = connection.receive(&moved).unwrap();
        assert_eq!(happened, [Happened::Drawn(vec![Area { left: 100, top: 70, width: 60, height: 40 }])]);
        assert_eq!(moved.len(), 4 + 16, "a part that moved travels as where it was");
        talk(&mut connection, &mut machine, send).unwrap();
        assert!(*connection.picture() == *machine.picture());

        // the desktop gets another size: the picture begins anew, and all of it arrives
        let larger = {
            let mut larger = Picture::new(260, 150);
            (0..260 * 150).for_each(|at| larger.set(at % 260, at / 260, [(at % 251) as u8, (at / 260) as u8, 9]));
            larger
        };
        let said = machine.shows(larger.clone());
        let (send, happened) = connection.receive(&said).unwrap();
        assert_eq!(happened, [Happened::Resized { width: 260, height: 150 }]);
        let happened = talk(&mut connection, &mut machine, send).unwrap();
        assert!(!happened.is_empty() && happened.iter().all(|happened| matches!(happened, Happened::Drawn(_))));
        assert!(*connection.picture() == larger);
        assert_eq!(machine.seen().asked_for.last(), Some(&(true, 260, 150)));
        assert!(machine.seen().asked_for.contains(&(false, 260, 150)));
    }

    #[test]
    fn keys_the_pointer_and_the_wheel_are_told_as_vnc_has_them() {
        let (_, mut connection, mut machine) = open(None, Machine::new(picture()));
        let told = connection.input(&[
            Input::Moved { x: 10, y: 20 },
            Input::Button { button: Button::Right, down: true, x: 10, y: 20 },
            Input::Button { button: Button::Middle, down: true, x: 11, y: 20 },
            Input::Button { button: Button::Right, down: false, x: 12, y: 20 },
            Input::Button { button: Button::Middle, down: false, x: 12, y: 20 },
            // beyond the desktop is its edge
            Input::Moved { x: 900, y: 900 },
            Input::Key { sym: 0xffe1, down: true },
            Input::Key { sym: 0x41, down: true },
            Input::Key { sym: 0x41, down: false },
            Input::Key { sym: 0x0100_20ac, down: true },
        ]);
        talk(&mut connection, &mut machine, told).unwrap();
        let seen = machine.seen();
        assert_eq!(seen.pointer, [(0, 10, 20), (4, 10, 20), (6, 11, 20), (2, 12, 20), (0, 12, 20), (0, 199, 119)]);
        assert_eq!(seen.keys, [(0xffe1, true), (0x41, true), (0x41, false), (0x0100_20ac, true)]);

        // a notch of the wheel is one step: a button of its own that goes down and comes up.
        // away from the person, towards them, to the right, to the left
        let wheel = |by: i16, sideways: bool| Input::Wheel { by, sideways, x: 30, y: 40 };
        let told = connection.input(&[wheel(120, false), wheel(-240, false), wheel(120, true), wheel(-120, true)]);
        talk(&mut connection, &mut machine, told).unwrap();
        let steps: Vec<u8> = machine.seen().pointer[6..].iter().map(|(buttons, ..)| *buttons).collect();
        assert_eq!(steps, [0x08, 0, 0x10, 0, 0x10, 0, 0x40, 0, 0x20, 0]);
        assert!(machine.seen().pointer[6..].iter().all(|(_, x, y)| (*x, *y) == (30, 40)));
        // a trackpad turns by less than a notch at a time: it adds up to a step, and a turn
        // the other way begins anew
        let before = machine.seen().pointer.len();
        let told = connection.input(&[wheel(-50, false), wheel(-50, false), wheel(-50, false), wheel(60, false), wheel(-100, false)]);
        talk(&mut connection, &mut machine, told).unwrap();
        let steps: Vec<u8> = machine.seen().pointer[before..].iter().map(|(buttons, ..)| *buttons).collect();
        assert_eq!(steps, [0x10, 0]);
        // a wheel while a button is held keeps the button down
        let before = machine.seen().pointer.len();
        let told =
            connection.input(&[Input::Button { button: Button::Left, down: true, x: 150, y: 100 }, wheel(120, false), wheel(20, false)]);
        talk(&mut connection, &mut machine, told).unwrap();
        let steps: Vec<u8> = machine.seen().pointer[before..].iter().map(|(buttons, ..)| *buttons).collect();
        assert_eq!(steps, [0x01, 0x09, 0x01]);
    }

    #[test]
    fn a_mac_scrolls_by_a_pixel_for_a_step_so_a_turn_is_as_many_steps_as_its_pixels() {
        let (_, mut connection, mut machine) = open(Some("a password"), Machine::new(picture()).with_the_password("a password").as_a_mac());
        let wheel = |by: i16| Input::Wheel { by, sideways: false, x: 30, y: 40 };
        let steps = |machine: &Machine, from: usize| {
            let said = &machine.seen().pointer[from..];
            // each step is the button down and up again
            assert!(said.chunks(2).all(|step| step.len() == 2 && step[1].0 == 0 && step[0].0 == said[0].0));
            (said.len() / 2, said.first().map_or(0, |(buttons, ..)| *buttons))
        };
        // a notch of a wheel stands for a hundred pixels, towards the person
        let told = connection.input(&[wheel(-120)]);
        talk(&mut connection, &mut machine, told).unwrap();
        assert_eq!(steps(&machine, 0), (100, 0x10));
        // a trackpad's small turns: four pixels, then one, then less than one, which adds up
        let before = machine.seen().pointer.len();
        let told = connection.input(&[wheel(5), wheel(1), wheel(1)]);
        talk(&mut connection, &mut machine, told).unwrap();
        assert_eq!(steps(&machine, before), (5, 0x08));
        // however far a turn is, it is no more steps than a machine takes at once
        let before = machine.seen().pointer.len();
        let told = connection.input(&[wheel(i16::MIN)]);
        talk(&mut connection, &mut machine, told).unwrap();
        assert_eq!(steps(&machine, before), (600, 0x10));
    }

    #[test]
    fn text_that_is_copied_passes_each_way_where_it_is_let() {
        let (_, mut connection, mut machine) = open(None, Machine::new(picture()));
        // what the person copied is told to the machine: beyond the western alphabet as a question mark
        let told = connection.copied("Zürich, 30 €");
        talk(&mut connection, &mut machine, told).unwrap();
        assert_eq!(machine.seen().pasted, ["Zürich, 30 ?"]);
        // Ctrl + C on the stand-in copies what it was handed, in capitals: that comes back
        let pressed = connection.input(&[Input::Key { sym: 0xffe3, down: true }, Input::Key { sym: 0x63, down: true }]);
        let happened = talk(&mut connection, &mut machine, pressed).unwrap();
        assert!(happened.contains(&Happened::Copied("ZÜRICH, 30 ?".to_string())));

        // where the clipboard passes neither way, nothing is said and nothing is taken
        let mut machine = Machine::new(picture());
        let mut closed = Connection::begin(None, Passes::default());
        let (send, _) = closed.receive(&machine.greets()).unwrap();
        talk(&mut closed, &mut machine, send).unwrap();
        assert!(closed.is_open() && closed.copied("nothing passes").is_empty());
        let pressed = closed.input(&[Input::Key { sym: 0xffe3, down: true }, Input::Key { sym: 0x63, down: true }]);
        let happened = talk(&mut closed, &mut machine, pressed).unwrap();
        assert!(!happened.iter().any(|happened| matches!(happened, Happened::Copied(_))));
        // and before the desktop is open nothing is told
        assert!(Connection::begin(None, Passes { to_the_machine: true, from_the_machine: true }).copied("early").is_empty());
    }

    #[test]
    fn what_a_machine_says_wrongly_is_refused_and_never_read_past_its_end() {
        let opened = || {
            let (_, connection, _) = open(None, Machine::new(picture()));
            connection
        };
        let update = |rectangle: &[u8]| [&[0u8, 0, 0, 1][..], rectangle].concat();
        let head = |left: u16, top: u16, width: u16, height: u16, written: i32| {
            [left.to_be_bytes(), top.to_be_bytes(), width.to_be_bytes(), height.to_be_bytes()]
                .concat()
                .into_iter()
                .chain(written.to_be_bytes())
                .collect::<Vec<u8>>()
        };
        // a rectangle outside the desktop, as it is, copied or packed
        for written in [0, 1, 16] {
            let said = update(&[head(190, 0, 20, 10, written), vec![0; 16]].concat());
            assert_eq!(opened().receive(&said), Err(Error::NotVnc("a rectangle lies outside the desktop")), "{written}");
        }
        let said = update(&[head(0, 0, 10, 10, 1), vec![0, 195, 0, 0]].concat());
        assert_eq!(opened().receive(&said), Err(Error::NotVnc("a rectangle lies outside the desktop")));
        // a way of writing that was not asked for, a message of no kind, a desktop too large
        assert!(matches!(opened().receive(&update(&head(0, 0, 10, 10, 5))), Err(Error::NotYet(_))));
        assert!(matches!(opened().receive(&[9]), Err(Error::NotVnc(_))));
        assert!(matches!(opened().receive(&update(&head(0, 0, 20000, 20000, -223))), Err(Error::NotYet(_))));
        // what does not unpack
        let said = update(&[head(0, 0, 10, 10, 16), vec![0, 0, 0, 4, 1, 2, 3, 4]].concat());
        assert!(matches!(opened().receive(&said), Err(Error::NotVnc(_))));
        // an update that arrives byte by byte is read as one that arrives whole
        let mut whole = opened();
        let mut by_bytes = opened();
        let said = update(&[head(3, 4, 2, 2, 0), vec![1, 2, 3, 0, 4, 5, 6, 0, 7, 8, 9, 0, 10, 11, 12, 0]].concat());
        let (send, happened) = whole.receive(&said).unwrap();
        assert_eq!(happened, [Happened::Drawn(vec![Area { left: 3, top: 4, width: 2, height: 2 }])]);
        let mut sent = Vec::new();
        let mut all = Vec::new();
        for byte in &said {
            let (send, happened) = by_bytes.receive(std::slice::from_ref(byte)).unwrap();
            sent.extend(send);
            all.extend(happened);
        }
        assert_eq!((sent, all), (send, happened));
        assert!(*whole.picture() == *by_bytes.picture());
        assert_eq!(&whole.picture().pixels[(4 * 200 + 3) * 4..][..8], &[1, 2, 3, 0xff, 4, 5, 6, 0xff]);
        // colours of a table and a bell are passed over
        let mut connection = opened();
        let (send, happened) = connection.receive(&[1, 0, 0, 0, 0, 1, 0, 1, 0, 2, 0, 3, 2]).unwrap();
        assert!(send.is_empty() && happened.is_empty());
    }
}
