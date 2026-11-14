//! The machine's side of a connection once the person is signed in, for tests: it gives
//! the channels, takes the client without a licence, says what it can do, and draws one
//! picture, in each of the ways a machine sends rectangles that Away reads.

use crate::picture::Picture;
use crate::{der, mcs, ntlm, pdu, tpkt, Error};

const WRONG: Error = Error::NotRdp("the client's message is not built as one");

/// The channel the stand-in gives the client, the one of the desktop, and the number of
/// the sharing.
const USER: u16 = 1004;
const DESKTOP: u16 = 1003;
const SHARING: u32 = 0x0001_03ea;

/// How wide and how high the rectangles are that the picture is cut into, at most.
const TILE: usize = 64;

/// What the stand-in saw of the client.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Seen {
    /// The size of the desktop the client asked for.
    pub asked: (u16, u16),
    /// The name the client gave itself.
    pub name: String,
    /// The security the client says was chosen.
    pub security: u32,
    /// The domain and the name of the account the desktop is for.
    pub account: (String, String),
    /// The password that travelled with the account's name, where one did.
    pub password: Option<String>,
    /// How many bits a pixel the client takes.
    pub bits: u16,
    /// Whether the client takes drawing from instructions, which Away never does.
    pub takes_instructions: bool,
    /// Whether the client said that it leaves.
    pub left: bool,
}

enum State {
    AwaitsClient,
    AwaitsUser,
    AwaitsAccount,
    Open,
}

/// A stand-in for a machine's desktop.
pub struct Desktop {
    state: State,
    picture: Picture,
    seen: Seen,
}

impl Desktop {
    /// A stand-in that offers a desktop of the picture's size, and draws the picture.
    pub fn new(picture: Picture) -> Desktop {
        Desktop { state: State::AwaitsClient, picture, seen: Seen::default() }
    }

    /// What it saw of the client so far.
    pub fn seen(&self) -> &Seen {
        &self.seen
    }

    /// Answers what one frame of the client's carries.
    pub fn answer(&mut self, carried: &[u8]) -> Result<Vec<u8>, Error> {
        let message = tpkt::data(carried)?;
        match (&self.state, message) {
            (State::AwaitsClient, [0x7f, 0x65, ..]) => {
                let at = message.windows(4).position(|part| part == b"Duca").ok_or(WRONG)? + 4;
                let blocks = match message.get(at..).ok_or(WRONG)? {
                    [first, _, rest @ ..] if first & 0x80 != 0 => rest,
                    [_, rest @ ..] => rest,
                    _ => return Err(WRONG),
                };
                let core = blocks.get(..216).filter(|core| core[..4] == [0x01, 0xc0, 0xd8, 0x00]).ok_or(WRONG)?;
                self.seen.asked = (number16(core, 8)?, number16(core, 10)?);
                self.seen.name = ntlm::utf16(&core[24..56]).trim_end_matches('\0').to_string();
                self.seen.security = u32::from(number16(core, 212)?);
                self.state = State::AwaitsUser;
                Ok(tpkt::data_frame(&connect_answer()))
            }
            (State::AwaitsUser, [0x04, ..]) => Ok(Vec::new()),
            (State::AwaitsUser, [0x28]) => {
                self.state = State::AwaitsAccount;
                let [high, low] = (USER - 1001).to_be_bytes();
                Ok(tpkt::data_frame(&[0x2e, 0, high, low]))
            }
            (State::AwaitsAccount, [0x38, user_high, user_low, high, low]) => {
                Ok(tpkt::data_frame(&[0x3e, 0, *user_high, *user_low, *high, *low, *high, *low]))
            }
            (State::AwaitsAccount | State::Open, [0x64, _, _, high, low, _, rest @ ..]) => {
                let arrived = match rest {
                    [first, second, arrived @ ..] if first & 0x80 != 0 => {
                        arrived.get(..(usize::from(first & 0x7f) << 8) | usize::from(*second)).ok_or(WRONG)?
                    }
                    [first, arrived @ ..] => arrived.get(..usize::from(*first)).ok_or(WRONG)?,
                    _ => return Err(WRONG),
                };
                if u16::from_be_bytes([*high, *low]) != DESKTOP {
                    return Ok(Vec::new());
                }
                match self.state {
                    State::AwaitsAccount => self.account(arrived),
                    _ => self.open(arrived),
                }
            }
            (_, [0x21, ..]) => {
                self.seen.left = true;
                Ok(Vec::new())
            }
            _ => Err(WRONG),
        }
    }

    /// Who the desktop is for. The answer: no licence is needed, and what the machine can do.
    fn account(&mut self, arrived: &[u8]) -> Result<Vec<u8>, Error> {
        if number16(arrived, 0)? != 0x0040 {
            return Err(WRONG);
        }
        let (domain, name, password) = (usize::from(number16(arrived, 12)?), usize::from(number16(arrived, 14)?), number16(arrived, 16)?);
        let texts = arrived.get(22..).ok_or(WRONG)?;
        self.seen.account =
            (ntlm::utf16(texts.get(..domain).ok_or(WRONG)?), ntlm::utf16(texts.get(domain + 2..domain + 2 + name).ok_or(WRONG)?));
        let travelled = texts.get(domain + name + 4..domain + name + 4 + usize::from(password)).ok_or(WRONG)?;
        self.seen.password = (password != 0).then(|| ntlm::utf16(travelled));
        self.state = State::Open;

        let mut licence = vec![0x80, 0, 0, 0, 0xff, 0x03, 0x10, 0x00];
        licence.extend(7u32.to_le_bytes());
        licence.extend(2u32.to_le_bytes());
        licence.extend([4, 0, 0, 0]);

        let (width, height) = (self.picture.width as u16, self.picture.height as u16);
        let mut sets = pdu::capability(1, &pdu::words(&[1, 3, 0x0200, 0, 0, 0x041d, 0, 0, 0, 0]));
        sets.extend(pdu::capability(2, &pdu::words(&[32, 1, 1, 1, width, height, 0, 1, 1, 0, 1, 0])));
        let mut can = Vec::new();
        can.extend(SHARING.to_le_bytes());
        can.extend(pdu::words(&[4, (sets.len() + 4) as u16]));
        can.extend(b"RDP\0");
        can.extend(pdu::words(&[2, 0]));
        can.extend(sets);
        can.extend(0u32.to_le_bytes());

        let mut send = to_the_client(&licence);
        send.extend(to_the_client(&pdu::share(1, mcs::MACHINE, &can)));
        Ok(send)
    }

    /// The messages of the open connection: each of the client's last steps is answered,
    /// and after the last the picture is drawn.
    fn open(&mut self, mut arrived: &[u8]) -> Result<Vec<u8>, Error> {
        let mut send = Vec::new();
        let data = |kind: u8, holds: &[u8]| to_the_client(&pdu::share_data(mcs::MACHINE, SHARING, kind, holds));
        while !arrived.is_empty() {
            let message = arrived.get(..usize::from(number16(arrived, 0)?)).filter(|message| message.len() >= 6).ok_or(WRONG)?;
            arrived = &arrived[message.len()..];
            let holds = &message[6..];
            match number16(message, 2)? & 0x0f {
                // what the client can do
                3 => {
                    let named = usize::from(number16(holds, 6)?);
                    let count = number16(holds, 10 + named)?;
                    let mut sets = holds.get(14 + named..).ok_or(WRONG)?;
                    for _ in 0..count {
                        let set = sets.get(..usize::from(number16(sets, 2)?)).filter(|set| set.len() >= 4).ok_or(WRONG)?;
                        match number16(set, 0)? {
                            2 => self.seen.bits = number16(set, 4)?,
                            // the 32 instructions it could take, after its description and what it keeps of the desktop
                            3 => self.seen.takes_instructions = set.get(36..68).ok_or(WRONG)?.iter().any(|takes| *takes != 0),
                            _ => {}
                        }
                        sets = &sets[set.len()..];
                    }
                }
                7 => match (holds.get(8).ok_or(WRONG)?, holds.get(12..).ok_or(WRONG)?) {
                    (0x1f, _) => send.extend(data(0x1f, &pdu::words(&[1, mcs::MACHINE]))),
                    (0x14, [4, 0, ..]) => send.extend(data(0x14, &pdu::words(&[4, 0, 0, 0]))),
                    (0x14, [1, 0, ..]) => send.extend(data(0x14, &pdu::words(&[2, USER, 0x03ea, 0]))),
                    (0x27, _) => {
                        send.extend(data(0x28, &pdu::words(&[0, 0, 3, 4])));
                        send.extend(self.draw());
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        Ok(send)
    }

    /// The picture, cut into rectangles and sent in each way a machine sends them.
    fn draw(&self) -> Vec<u8> {
        let (width, height) = (self.picture.width, self.picture.height);
        let mut rectangles = Vec::new();
        for top in (0..height).step_by(TILE) {
            for left in (0..width).step_by(TILE) {
                rectangles.push(self.rectangle(left, top, TILE.min(width - left), TILE.min(height - top), rectangles.len()));
            }
        }
        let update = |rectangles: &[Vec<u8>]| {
            let mut update = vec![1, 0];
            update.extend((rectangles.len() as u16).to_le_bytes());
            update.extend(rectangles.concat());
            update
        };
        let mut send = Vec::new();
        // the last rectangle in a frame of the ordinary kind
        let last = rectangles.pop();
        // the first half, each in a short frame of its own
        let half = rectangles.len() / 2;
        for rectangle in &rectangles[..half] {
            send.extend(short_frame(0x01, &update(std::slice::from_ref(rectangle))));
        }
        // the rest as one update that travels in three pieces: the first, one in between, the last
        let together = update(&rectangles[half..]);
        let pieces: Vec<&[u8]> = together.chunks(together.len() / 3 + 1).collect();
        for (nth, piece) in pieces.iter().enumerate() {
            let which = match nth {
                _ if pieces.len() == 1 => 0x0,
                0 => 0x2,
                _ if nth + 1 == pieces.len() => 0x1,
                _ => 0x3,
            };
            send.extend(short_frame(0x01 | (which << 4), piece));
        }
        if let Some(last) = last {
            send.extend(to_the_client(&pdu::share_data(mcs::MACHINE, SHARING, 0x02, &update(&[last]))));
        }
        send
    }

    /// One rectangle of the picture, in the way its number chooses.
    fn rectangle(&self, left: usize, top: usize, wide: usize, high: usize, nth: usize) -> Vec<u8> {
        // every fourth carries more columns than it has room for, as a machine does to
        // keep its rows at whole words
        let carried = if nth % 4 == 3 { wide + 4 } else { wide };
        let mut pixels = Vec::with_capacity(carried * high * 4);
        for row in top..top + high {
            let from = (row * self.picture.width + left) * 4;
            pixels.extend_from_slice(&self.picture.pixels[from..from + wide * 4]);
            // what is beyond the rectangle's room is never to be seen
            pixels.extend((0..(carried - wide) * 4).map(|at| (at * 37 + row) as u8));
        }
        let (flags, carries): (u16, Vec<u8>) = match nth % 4 {
            0 => (0x0401, pack(&pixels, carried, high, true)),
            1 => (0x0401, pack(&pixels, carried, high, false)),
            2 => (0, pixels.chunks_exact(carried * 4).rev().flatten().copied().collect()),
            // with a head of its own before what it carries
            _ => (0x0001, [&[0; 8][..], &pack(&pixels, carried, high, true)].concat()),
        };
        let numbers =
            [left, top, left + wide - 1, top + high - 1, carried, high, 32, usize::from(flags), carries.len()].map(|number| number as u16);
        let mut rectangle = pdu::words(&numbers);
        rectangle.extend(carries);
        rectangle
    }
}

fn number16(bytes: &[u8], at: usize) -> Result<u16, Error> {
    bytes.get(at..at + 2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).ok_or(WRONG)
}

/// A length as the conference's messages write it: one byte, or two with the top bit set.
fn length(of: usize) -> Vec<u8> {
    if of < 0x80 {
        vec![of as u8]
    } else {
        (0x8000 | of as u16).to_be_bytes().to_vec()
    }
}

/// The machine's answer to what the client says of itself: the desktop's channel, and no
/// encryption of RDP's own.
fn connect_answer() -> Vec<u8> {
    let block = |kind: u16, holds: &[u8]| {
        let mut block = pdu::words(&[kind, (holds.len() + 4) as u16]);
        block.extend_from_slice(holds);
        block
    };
    let mut blocks = block(0x0c01, &[0x04, 0x00, 0x08, 0x00, 0x0b, 0, 0, 0, 0x01, 0, 0, 0]);
    blocks.extend(block(0x0c02, &[0; 8]));
    blocks.extend(block(0x0c03, &pdu::words(&[DESKTOP, 0])));
    let mut says = vec![0x00, 0x05, 0x00, 0x14, 0x7c, 0x00, 0x01, 0x2a, 0x14, 0x76, 0x0a, 0x01, 0x01, 0x00, 0x01, 0xc0, 0x00];
    says.extend(b"McDn");
    says.extend(length(blocks.len()));
    says.extend(blocks);
    let mut fields = vec![0x0a, 0x01, 0x00, 0x02, 0x01, 0x00];
    fields.extend(mcs::limits(34, 3, 0, 65_528));
    fields.extend(der::write(der::OCTET_STRING, &says));
    let mut answer = vec![0x7f, 0x66];
    answer.extend(&der::write(0, &fields)[1..]);
    answer
}

/// A message for the client, on the desktop's channel, in a frame of the ordinary kind.
fn to_the_client(carried: &[u8]) -> Vec<u8> {
    let [high, low] = DESKTOP.to_be_bytes();
    let mut message = vec![0x68, 0x00, 0x01, high, low, 0x70];
    message.extend(length(carried.len()));
    message.extend_from_slice(carried);
    tpkt::data_frame(&message)
}

/// A frame of the short kind around one update, or one piece of one.
fn short_frame(head: u8, update: &[u8]) -> Vec<u8> {
    let mut carried = vec![head];
    carried.extend((update.len() as u16).to_le_bytes());
    carried.extend_from_slice(update);
    let whole = carried.len() + if carried.len() + 2 < 0x80 { 2 } else { 3 };
    let mut frame = vec![0x00];
    frame.extend(length(whole));
    frame.extend(carried);
    frame
}

/// Packs pixels the way a machine does when nothing may be left out: each colour as a
/// plane of its own, as it is or as runs. The pixels come from the top row down, four
/// bytes each: blue, green, red, and a fourth that is left out.
pub fn pack(pixels: &[u8], width: usize, height: usize, in_runs: bool) -> Vec<u8> {
    let mut packed = vec![if in_runs { 0x30 } else { 0x20 }];
    for colour in [2, 1, 0] {
        // a plane's rows travel from the bottom up
        let plane: Vec<u8> =
            (0..height).rev().flat_map(|row| (0..width).map(move |column| pixels[(row * width + column) * 4 + colour])).collect();
        if in_runs {
            for row in 0..height {
                let line = &plane[row * width..(row + 1) * width];
                // the first row as it is, every later one as what changed against the row before
                let written: Vec<u8> = match row {
                    0 => line.to_vec(),
                    _ => line
                        .iter()
                        .zip(&plane[(row - 1) * width..])
                        .map(|(now, before)| match now.wrapping_sub(*before) as i8 {
                            change @ 0.. => (change as u8) << 1,
                            change => (((-i16::from(change) - 1) as u8) << 1) | 1,
                        })
                        .collect(),
                };
                packed.extend(runs(&written));
            }
        } else {
            packed.extend(plane);
        }
    }
    if !in_runs {
        packed.push(0);
    }
    packed
}

/// One row as pieces: values written out, each piece followed by a run of its last value.
fn runs(row: &[u8]) -> Vec<u8> {
    let mut pieces = Vec::new();
    let (mut at, mut last) = (0, 0u8);
    while at < row.len() {
        // values are written out until at least three in a row repeat the one before them
        let mut written = at;
        let run = loop {
            let before = if written == at { last } else { row[written - 1] };
            let run = row[written..].iter().take_while(|value| **value == before).count();
            if run >= 3 || written == row.len() {
                break run;
            }
            written += 1;
        };
        let value = if written == at { last } else { row[written - 1] };
        let mut values = &row[at..written];
        while values.len() > 15 {
            pieces.push(0xf0);
            pieces.extend_from_slice(&values[..15]);
            values = &values[15..];
        }
        let mut left = run;
        if left < 16 {
            // a short run shares its first byte with the values before it
            if !values.is_empty() || left > 0 {
                pieces.push(((values.len() as u8) << 4) | left as u8);
                pieces.extend_from_slice(values);
            }
            left = 0;
        } else if !values.is_empty() {
            pieces.push((values.len() as u8) << 4);
            pieces.extend_from_slice(values);
        }
        // a long run has bytes of its own: 32 to 47 at a time, or 16 to 31
        while left >= 16 {
            let now = left.min(47);
            pieces.push(if now >= 32 { (((now - 32) as u8) << 4) | 2 } else { (((now - 16) as u8) << 4) | 1 });
            left -= now;
        }
        match left {
            0 => {}
            // too short to be a run: written out
            1 | 2 => {
                pieces.push((left as u8) << 4);
                pieces.extend(std::iter::repeat_n(value, left));
            }
            _ => pieces.push(left as u8),
        }
        last = value;
        at = written + run;
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planar;

    /// Pixels that hold flat parts, slopes and noise, so that every kind of piece occurs.
    fn pixels(width: usize, height: usize, seed: u32) -> Vec<u8> {
        let mut state = seed;
        let mut pixels = Vec::with_capacity(width * height * 4);
        for row in 0..height {
            for column in 0..width {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let pixel = match (column * 4 / width.max(1) + seed as usize) % 4 {
                    0 => [0xf3, 0xf2, 0xf1],
                    1 => [(row * 3) as u8, column as u8, (row + column) as u8],
                    2 => [(state >> 8) as u8, (state >> 16) as u8, (state >> 24) as u8],
                    _ => [if row % 2 == 0 { 0 } else { 255 }; 3],
                };
                pixels.extend(pixel);
                pixels.push(0xff);
            }
        }
        pixels
    }

    #[test]
    fn what_is_packed_unpacks_to_the_same_pixels() {
        for (width, height) in [(1, 1), (1, 9), (2, 2), (3, 5), (16, 4), (17, 3), (47, 2), (48, 6), (64, 64), (100, 7), (131, 33)] {
            for seed in 0..4 {
                let pixels = pixels(width, height, seed);
                for in_runs in [true, false] {
                    let packed = pack(&pixels, width, height, in_runs);
                    assert_eq!(planar::unpack(&packed, width, height).unwrap(), pixels, "{width} by {height}, runs: {in_runs}");
                }
            }
        }
        // one colour all over: a few bytes a row
        let flat = [7, 8, 9, 0xff].repeat(64 * 64);
        assert!(pack(&flat, 64, 64, true).len() < 64 * 3 * 3);
        assert_eq!(planar::unpack(&pack(&flat, 64, 64, true), 64, 64).unwrap(), flat);
    }

    #[test]
    fn runs_are_written_as_a_machine_writes_them() {
        assert_eq!(runs(&[7, 9, 9, 9, 9, 9]), [0x24, 7, 9]);
        assert_eq!(runs(&[0; 6]), [0x06]);
        assert_eq!(runs(&[5; 20]), [0x10, 5, 0x31]);
        assert_eq!(runs(&[5; 40]), [0x10, 5, 0x72]);
        // two alike are no run
        assert_eq!(runs(&[1, 2, 2, 3]), [0x40, 1, 2, 2, 3]);
        assert_eq!(runs(&[5; 18]), [0x10, 5, 0x11]);
        // 48 left after one value: 47 as a long run, and the last, too short for a run, written out
        assert_eq!(runs(&[5; 49]), [0x10, 5, 0xf2, 0x10, 5]);
    }
}
