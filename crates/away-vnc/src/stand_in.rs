//! The machine's side of a connection, for tests and for whatever stands in for a
//! machine: it speaks a version, signs a client in with a password or without one,
//! shows a picture in each way a machine writes rectangles, and draws a mark where the
//! left button goes down, as a machine draws what a person does.

use away_secret::Secret;
use flate2::{Compress, Compression, FlushCompress};

use crate::picture::{Area, Picture};
use crate::{password, zrle, Error};

const WRONG: Error = Error::NotVnc("the client says something a machine doesn't take");

/// How wide and how high the mark is that the stand-in draws for a button or a key.
pub const MARK: usize = 12;
/// The colour of the mark for the left button: blue, green, red.
pub const CLICKED: [u8; 3] = [0x20, 0x40, 0xe0];

/// The colour of the bar that says how far the wheel was turned towards the person: a
/// row for each step, down the right edge. Blue, green, red.
pub const SCROLLED: [u8; 3] = [0x10, 0xd0, 0x30];
/// How wide that bar is.
pub const BAR: usize = 4;

/// The colour of the mark for a key that went down: it says which key it was.
pub fn typed(sym: u32) -> [u8; 3] {
    [sym as u8, 0xff - sym as u8, 0x55]
}

/// The question the stand-in asks for its password with.
const QUESTION: [u8; 16] = [0x21, 0x43, 0x65, 0x87, 0xa9, 0xcb, 0xed, 0x0f, 0x10, 0x32, 0x54, 0x76, 0x98, 0xba, 0xdc, 0xfe];

/// What the stand-in saw of its client, for a test to look at.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Seen {
    /// The version the client answered with, as it wrote it.
    pub answered: Option<String>,
    /// The way of signing in the client chose, where it was offered ways.
    pub chose: Option<u8>,
    /// Whether the client lets others see the desktop too.
    pub shared: Option<bool>,
    /// The pixels the client asked for, as the protocol writes them, without the filling.
    pub pixels: Option<[u8; 16]>,
    /// The ways of writing a rectangle the client reads, the one it likes best first.
    pub reads: Vec<i32>,
    /// What the client asked to be drawn: only what changed or all of it, and how much of the desktop.
    pub asked_for: Vec<(bool, u16, u16)>,
    /// The keys: each with whether it went down.
    pub keys: Vec<(u32, bool)>,
    /// The pointer: the buttons that are down, and where it is.
    pub pointer: Vec<(u8, u16, u16)>,
    /// The texts the client said were copied.
    pub pasted: Vec<String>,
}

enum State {
    AwaitsVersion,
    AwaitsChoice,
    AwaitsAnswer,
    AwaitsShared,
    Open,
    Ended,
}

/// A stand-in for a machine's desktop.
pub struct Machine {
    state: State,
    speaks: &'static str,
    offers: Vec<u8>,
    password: Option<String>,
    takes_nobody: Option<&'static str>,
    picture: Picture,
    arrived: Vec<u8>,
    seen: Seen,
    packs: Compress,
    /// Whether the client asked for what changes, and nothing changed since.
    waits: bool,
    /// What is to be said once the client asks: areas that changed, and whether the
    /// desktop has another size.
    changed: Vec<Area>,
    another_size: bool,
    /// Whether the control key is down.
    control: bool,
    /// How many steps the wheel was turned towards the person, less those away from them.
    scrolled: usize,
}

impl Machine {
    /// A stand-in that speaks the current version, asks for no password, and shows this picture.
    pub fn new(picture: Picture) -> Machine {
        Machine {
            state: State::AwaitsVersion,
            speaks: "003.008",
            offers: vec![1],
            password: None,
            takes_nobody: None,
            picture,
            arrived: Vec::new(),
            seen: Seen::default(),
            packs: Compress::new(Compression::fast(), true),
            waits: false,
            changed: Vec::new(),
            another_size: false,
            control: false,
            scrolled: 0,
        }
    }

    /// The same, asking for this password.
    pub fn with_the_password(self, password: &str) -> Machine {
        Machine { password: Some(password.to_string()), offers: vec![2], ..self }
    }

    /// The same, speaking another version: `003.003`, `003.007`, `003.008`, or one a
    /// machine makes up.
    pub fn speaking(self, version: &'static str) -> Machine {
        Machine { speaks: version, ..self }
    }

    /// The same, as a Mac's own screen sharing: it speaks a version of its own, and
    /// offers ways of signing in of its own around the password.
    pub fn as_a_mac(self) -> Machine {
        let offers = if self.password.is_some() { vec![30, 33, 2, 35] } else { vec![30, 33, 35] };
        Machine { speaks: "003.889", offers, ..self }
    }

    /// The same, offering these ways of signing in.
    pub fn that_offers(self, ways: &[u8]) -> Machine {
        Machine { offers: ways.to_vec(), ..self }
    }

    /// The same, as a machine that takes nobody right now, and says why.
    pub fn that_takes_nobody(self, why: &'static str) -> Machine {
        Machine { takes_nobody: Some(why), ..self }
    }

    /// The machine's first word: the version it speaks.
    pub fn greets(&self) -> Vec<u8> {
        format!("RFB {}\n", self.speaks).into_bytes()
    }

    /// What it saw of the client so far.
    pub fn seen(&self) -> &Seen {
        &self.seen
    }

    /// The picture as the stand-in has it now, with what it drew for keys and the pointer.
    pub fn picture(&self) -> &Picture {
        &self.picture
    }

    /// Whether the client was signed in and the desktop is open.
    pub fn is_open(&self) -> bool {
        matches!(self.state, State::Open)
    }

    /// Whether the machine decides how it signs people in, as the oldest version has it.
    fn decides(&self) -> bool {
        self.speaks == "003.003"
    }

    /// Why it refuses, as the version it speaks says it.
    fn why(&self, why: &str) -> Vec<u8> {
        [&(why.len() as u32).to_be_bytes()[..], why.as_bytes()].concat()
    }

    /// Answers what the client sent.
    pub fn answer(&mut self, bytes: &[u8]) -> Result<Vec<u8>, Error> {
        self.arrived.extend_from_slice(bytes);
        let mut send = Vec::new();
        while self.next(&mut send)? {}
        // what changed is said once the client asked for it
        if self.waits && (self.another_size || !self.changed.is_empty()) {
            self.waits = false;
            send.extend(self.draws());
        }
        Ok(send)
    }

    fn take(&mut self, count: usize) -> Option<Vec<u8>> {
        (self.arrived.len() >= count).then(|| self.arrived.drain(..count).collect())
    }

    fn next(&mut self, send: &mut Vec<u8>) -> Result<bool, Error> {
        match self.state {
            State::AwaitsVersion => {
                let Some(line) = self.take(12) else { return Ok(false) };
                self.seen.answered = Some(String::from_utf8(line).map_err(|_| WRONG)?);
                if let Some(why) = self.takes_nobody {
                    send.extend(if self.decides() { vec![0, 0, 0, 0] } else { vec![0] });
                    send.extend(self.why(why));
                    self.state = State::Ended;
                } else if self.decides() {
                    // the oldest version: the machine says how it signs people in
                    match self.password {
                        Some(_) => {
                            send.extend([0, 0, 0, 2]);
                            send.extend(QUESTION);
                            self.state = State::AwaitsAnswer;
                        }
                        None => {
                            send.extend([0, 0, 0, 1]);
                            self.state = State::AwaitsShared;
                        }
                    }
                } else {
                    send.push(self.offers.len() as u8);
                    send.extend(&self.offers);
                    self.state = State::AwaitsChoice;
                }
            }
            State::AwaitsChoice => {
                let Some(chose) = self.take(1) else { return Ok(false) };
                self.seen.chose = Some(chose[0]);
                match chose[0] {
                    2 if self.password.is_some() => {
                        send.extend(QUESTION);
                        self.state = State::AwaitsAnswer;
                    }
                    1 if self.offers.contains(&1) => {
                        // the current version says that it took the client, and the one before says nothing
                        if self.speaks != "003.007" {
                            send.extend([0, 0, 0, 0]);
                        }
                        self.state = State::AwaitsShared;
                    }
                    _ => return Err(WRONG),
                }
            }
            State::AwaitsAnswer => {
                let Some(answer) = self.take(16) else { return Ok(false) };
                let right = password::answer(&QUESTION, &Secret::new(self.password.clone().unwrap_or_default()));
                if answer == right {
                    send.extend([0, 0, 0, 0]);
                    self.state = State::AwaitsShared;
                } else {
                    send.extend([0, 0, 0, 1]);
                    if !self.decides() && self.speaks != "003.007" {
                        send.extend(self.why("Authentication failed"));
                    }
                    self.state = State::Ended;
                }
            }
            State::AwaitsShared => {
                let Some(shared) = self.take(1) else { return Ok(false) };
                self.seen.shared = Some(shared[0] != 0);
                // how large the desktop is, the pixels the machine has by itself, and its name
                send.extend((self.picture.width as u16).to_be_bytes());
                send.extend((self.picture.height as u16).to_be_bytes());
                send.extend([32, 24, 1, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0]);
                let name = b"a stand-in's desktop";
                send.extend((name.len() as u32).to_be_bytes());
                send.extend(name);
                self.state = State::Open;
            }
            State::Open => return self.said(send),
            // it ended the conversation: nothing more is taken
            State::Ended if self.arrived.is_empty() => return Ok(false),
            State::Ended => return Err(WRONG),
        }
        Ok(true)
    }

    /// Reads the next thing the client says once the desktop is open.
    fn said(&mut self, send: &mut Vec<u8>) -> Result<bool, Error> {
        let Some(&kind) = self.arrived.first() else { return Ok(false) };
        match kind {
            0 => {
                let Some(says) = self.take(20) else { return Ok(false) };
                let pixels: [u8; 16] = says[4..].try_into().expect("sixteen bytes");
                // the stand-in writes pixels one way: the way Away asks for them
                if pixels[..4] != [32, 24, 0, 1] || pixels[10..13] != [16, 8, 0] {
                    return Err(Error::NotYet("is asked for pixels the stand-in doesn't write"));
                }
                self.seen.pixels = Some(pixels);
            }
            2 => {
                let Some(head) = self.arrived.get(..4) else { return Ok(false) };
                let count = usize::from(u16::from_be_bytes([head[2], head[3]]));
                let Some(says) = self.take(4 + count * 4) else { return Ok(false) };
                self.seen.reads = says[4..].as_chunks::<4>().0.iter().map(|way| i32::from_be_bytes(*way)).collect();
            }
            3 => {
                let Some(says) = self.take(10) else { return Ok(false) };
                let only_what_changed = says[1] != 0;
                self.seen.asked_for.push((
                    only_what_changed,
                    u16::from_be_bytes([says[6], says[7]]),
                    u16::from_be_bytes([says[8], says[9]]),
                ));
                if only_what_changed {
                    self.waits = true;
                } else {
                    // all of it, whatever was to be said of its parts
                    self.changed.clear();
                    self.another_size = false;
                    let whole = self.whole();
                    send.extend(self.draws_areas(&whole));
                }
            }
            4 => {
                let Some(says) = self.take(8) else { return Ok(false) };
                let (down, sym) = (says[1] != 0, u32::from_be_bytes([says[4], says[5], says[6], says[7]]));
                self.seen.keys.push((sym, down));
                if sym == 0xffe3 {
                    self.control = down;
                }
                // Ctrl + C copies: the machine's clipboard holds what it was handed, in capitals
                if (sym, down, self.control) == (0x63, true, true) {
                    let copied: Vec<u8> = self
                        .seen
                        .pasted
                        .last()
                        .map(|text| text.to_uppercase().chars().map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?')).collect())
                        .unwrap_or_default();
                    send.extend([3, 0, 0, 0]);
                    send.extend((copied.len() as u32).to_be_bytes());
                    send.extend(copied);
                } else if down && sym < 0xff00 {
                    // a key that writes something leaves a mark in the corner, which says which key it was
                    self.marks(0, 0, typed(sym));
                }
            }
            5 => {
                let Some(says) = self.take(6) else { return Ok(false) };
                let (buttons, x, y) = (says[1], u16::from_be_bytes([says[2], says[3]]), u16::from_be_bytes([says[4], says[5]]));
                let before = self.seen.pointer.last().map_or(0, |(before, ..)| *before);
                self.seen.pointer.push((buttons, x, y));
                let newly = |bit: u8| buttons & bit != 0 && before & bit == 0;
                if newly(0x01) {
                    self.marks(usize::from(x), usize::from(y), CLICKED);
                }
                // a step of the wheel scrolls by a row: towards the person the bar at the
                // right edge grows by one, and away from them it shrinks again
                let row = |top: usize, picture: &Picture| Area {
                    left: picture.width.saturating_sub(BAR),
                    top,
                    width: BAR.min(picture.width),
                    height: 1,
                };
                if newly(0x10) && self.scrolled < self.picture.height {
                    let area = row(self.scrolled, &self.picture);
                    self.picture.fill(area, SCROLLED);
                    self.changed.push(area);
                    self.scrolled += 1;
                } else if newly(0x08) && self.scrolled > 0 {
                    self.scrolled -= 1;
                    let area = row(self.scrolled, &self.picture);
                    self.picture.fill(area, [0, 0, 0]);
                    self.changed.push(area);
                }
            }
            6 => {
                let Some(head) = self.arrived.get(..8) else { return Ok(false) };
                let length = u32::from_be_bytes([head[4], head[5], head[6], head[7]]) as usize;
                let Some(says) = self.take(8 + length) else { return Ok(false) };
                self.seen.pasted.push(says[8..].iter().map(|byte| char::from(*byte)).collect());
            }
            _ => return Err(WRONG),
        }
        Ok(true)
    }

    /// Draws a mark at a place of the picture, cut at the picture's edges.
    fn marks(&mut self, left: usize, top: usize, colour: [u8; 3]) {
        let (left, top) = (left.min(self.picture.width - 1), top.min(self.picture.height - 1));
        let area = Area { left, top, width: MARK.min(self.picture.width - left), height: MARK.min(self.picture.height - top) };
        self.picture.fill(area, colour);
        self.changed.push(area);
    }

    /// The whole desktop, cut into bands, so that an update holds several rectangles.
    fn whole(&self) -> Vec<Area> {
        (0..self.picture.height)
            .step_by(50)
            .map(|top| Area { left: 0, top, width: self.picture.width, height: 50.min(self.picture.height - top) })
            .collect()
    }

    /// What is to be said: that the desktop has another size, or the areas that changed.
    fn draws(&mut self) -> Vec<u8> {
        if std::mem::take(&mut self.another_size) {
            self.changed.clear();
            let mut says = vec![0, 0, 0, 1, 0, 0, 0, 0];
            says.extend((self.picture.width as u16).to_be_bytes());
            says.extend((self.picture.height as u16).to_be_bytes());
            says.extend((-223i32).to_be_bytes());
            return says;
        }
        let areas = std::mem::take(&mut self.changed);
        self.draws_areas(&areas)
    }

    /// An update with these areas of the picture: packed and as they are in turn, where
    /// the client reads what is packed.
    fn draws_areas(&mut self, areas: &[Area]) -> Vec<u8> {
        let mut says = vec![0, 0];
        says.extend((areas.len() as u16).to_be_bytes());
        for (nth, area) in areas.iter().enumerate() {
            for number in [area.left, area.top, area.width, area.height] {
                says.extend((number as u16).to_be_bytes());
            }
            if self.seen.reads.contains(&16) && nth % 2 == 0 {
                says.extend(16i32.to_be_bytes());
                let packed = self.pack(&zrle::stand_in::tiles(&self.picture, *area));
                says.extend((packed.len() as u32).to_be_bytes());
                says.extend(packed);
            } else {
                says.extend(0i32.to_be_bytes());
                for row in area.top..area.top + area.height {
                    let from = (row * self.picture.width + area.left) * 4;
                    for pixel in self.picture.pixels[from..from + area.width * 4].as_chunks::<4>().0 {
                        says.extend([pixel[0], pixel[1], pixel[2], 0]);
                    }
                }
            }
        }
        says
    }

    /// Packs the next part of the connection's stream.
    fn pack(&mut self, tiles: &[u8]) -> Vec<u8> {
        let mut packed = Vec::with_capacity(tiles.len() + tiles.len() / 2 + 64);
        let mut read = 0;
        loop {
            let before = self.packs.total_in();
            self.packs.compress_vec(&tiles[read..], &mut packed, FlushCompress::Sync).expect("packing never fails");
            read += (self.packs.total_in() - before) as usize;
            if read == tiles.len() && packed.len() < packed.capacity() {
                return packed;
            }
            packed.reserve(packed.capacity().max(4096));
        }
    }

    /// A part of the screen moves to another place, as a window that is dragged: the
    /// client is told where the part was, where it reads that, once it asks.
    pub fn moves(&mut self, from: Area, to: (usize, usize)) -> Vec<u8> {
        let area = Area { left: to.0, top: to.1, ..from };
        self.picture.copy((from.left, from.top), area);
        if !self.waits || !self.seen.reads.contains(&1) {
            self.changed.push(area);
            return Vec::new();
        }
        self.waits = false;
        let mut says = vec![0, 0, 0, 1];
        for number in [area.left, area.top, area.width, area.height] {
            says.extend((number as u16).to_be_bytes());
        }
        says.extend(1i32.to_be_bytes());
        says.extend((from.left as u16).to_be_bytes());
        says.extend((from.top as u16).to_be_bytes());
        says
    }

    /// The machine shows another picture from now on, of this size or another: the
    /// client is told once it asks.
    pub fn shows(&mut self, picture: Picture) -> Vec<u8> {
        let another_size = (picture.width, picture.height) != (self.picture.width, self.picture.height);
        self.picture = picture;
        self.another_size = another_size && self.seen.reads.contains(&-223);
        self.changed = self.whole();
        if !self.waits {
            return Vec::new();
        }
        self.waits = false;
        self.draws()
    }
}
