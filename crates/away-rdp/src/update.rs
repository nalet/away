//! What the machine draws: rectangles of its picture, as they arrive (MS-RDPBCGR 2.2.9).
//!
//! They come in frames of the short kind, one or more updates in each, and an update can
//! arrive in pieces. This cuts them apart, puts the pieces together, and gives the
//! rectangles; turning a rectangle's bytes into pixels is the picture's part.

use crate::window::{self, Change};
use crate::Error;

/// One rectangle of the machine's picture, as it arrived.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rectangle {
    /// Where it goes: its left and top edge, in pixels.
    pub left: u16,
    /// Its top edge.
    pub top: u16,
    /// Its last column, which belongs to it.
    pub right: u16,
    /// Its last row, which belongs to it.
    pub bottom: u16,
    /// How wide what it carries is, which may be more than where it goes.
    pub width: u16,
    /// How high what it carries is.
    pub height: u16,
    /// How many bits a pixel has.
    pub bits: u16,
    /// Whether what it carries is compressed.
    pub compressed: bool,
    /// What it carries.
    pub carries: Vec<u8>,
}

/// The largest update the client takes, in bytes. The client says so when it says what
/// it can do, and a machine that sends more is not kept the memory for.
pub const LARGEST: usize = 8 * 1024 * 1024;

/// The updates of the short frames, with the pieces of one that is not whole yet.
#[derive(Default)]
pub struct Updates {
    pieces: Vec<u8>,
    windows: Vec<Change>,
}

impl Updates {
    /// Nothing has arrived yet.
    pub fn new() -> Self {
        Updates::default()
    }

    /// Takes what a short frame carries, and gives the rectangles that are whole.
    pub fn short_frame(&mut self, mut carried: &[u8]) -> Result<Vec<Rectangle>, Error> {
        const WRONG: Error = Error::NotRdp("an update of the picture is not built as one");
        let mut rectangles = Vec::new();
        while let [head, rest @ ..] = carried {
            // its kind, whether it is a piece, and whether it is compressed
            let (kind, piece, compressed) = (head & 0x0f, (head >> 4) & 0x3, head >> 6);
            if compressed != 0 {
                return Err(Error::NotYet("compresses what it draws on the way"));
            }
            let [low, high, rest @ ..] = rest else { return Err(WRONG) };
            let length = usize::from(u16::from_le_bytes([*low, *high]));
            let update = rest.get(..length).ok_or(WRONG)?;
            carried = &rest[length..];
            // a single update, the last piece, the first, or one in between
            let whole = match piece {
                0 => Some(update.to_vec()),
                2 => {
                    self.pieces = update.to_vec();
                    None
                }
                _ if self.pieces.len() + update.len() > LARGEST => {
                    return Err(Error::NotRdp("an update is larger than the client said it takes"));
                }
                3 => {
                    self.pieces.extend_from_slice(update);
                    None
                }
                _ => {
                    let mut whole = std::mem::take(&mut self.pieces);
                    whole.extend_from_slice(update);
                    Some(whole)
                }
            };
            match (kind, whole) {
                (1, Some(whole)) => rectangles.extend(read_rectangles(&whole)?),
                // instructions: how many, and then each. Away asks for none to draw from,
                // so what arrives are the machine's words on the windows of programs
                (0, Some(whole)) => {
                    let [low, high, orders @ ..] = &whole[..] else { return Err(WRONG) };
                    self.windows.extend(window::read(u16::from_le_bytes([*low, *high]), orders)?);
                }
                _ => {}
            }
        }
        Ok(rectangles)
    }

    /// What the machine said of the windows of programs since this was last asked.
    pub fn take_windows(&mut self) -> Vec<Change> {
        std::mem::take(&mut self.windows)
    }
}

/// Reads an update of the long kind that carries instructions, and gives the machine's
/// words on the windows of programs in it.
pub fn read_windows(update: &[u8]) -> Result<Vec<Change>, Error> {
    const WRONG: Error = Error::NotRdp("an update of instructions is not built as one");
    let count = update.get(4..6).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).ok_or(WRONG)?;
    window::read(count, update.get(8..).ok_or(WRONG)?)
}

/// Reads an update that carries rectangles: its kind, how many, then each.
pub fn read_rectangles(update: &[u8]) -> Result<Vec<Rectangle>, Error> {
    const WRONG: Error = Error::NotRdp("a rectangle of the picture is not built as one");
    let number = |bytes: &[u8], at: usize| bytes.get(at..at + 2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).ok_or(WRONG);
    if number(update, 0)? != 1 {
        return Err(WRONG);
    }
    let count = number(update, 2)?;
    let mut rest = update.get(4..).ok_or(WRONG)?;
    let mut rectangles = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        let flags = number(rest, 14)?;
        let compressed = flags & 0x0001 != 0;
        let mut length = usize::from(number(rest, 16)?);
        let mut carries = rest.get(18..).ok_or(WRONG)?;
        // a compressed rectangle has a head of its own unless it says it has none
        if compressed && flags & 0x0400 == 0 {
            carries = carries.get(8..).ok_or(WRONG)?;
            length = length.checked_sub(8).ok_or(WRONG)?;
        }
        let holds = carries.get(..length).ok_or(WRONG)?;
        rectangles.push(Rectangle {
            left: number(rest, 0)?,
            top: number(rest, 2)?,
            right: number(rest, 4)?,
            bottom: number(rest, 6)?,
            width: number(rest, 8)?,
            height: number(rest, 10)?,
            bits: number(rest, 12)?,
            compressed,
            carries: holds.to_vec(),
        });
        rest = &carries[length..];
    }
    Ok(rectangles)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rectangle(flags: u16, carries: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for number in [10u16, 20, 13, 21, 4, 2, 32, flags, carries.len() as u16] {
            bytes.extend(number.to_le_bytes());
        }
        bytes.extend_from_slice(carries);
        bytes
    }

    fn update(rectangles: &[Vec<u8>]) -> Vec<u8> {
        let mut bytes = vec![1, 0, rectangles.len() as u8, 0];
        bytes.extend(rectangles.concat());
        bytes
    }

    fn short(head: u8, update: &[u8]) -> Vec<u8> {
        let mut bytes = vec![head];
        bytes.extend((update.len() as u16).to_le_bytes());
        bytes.extend_from_slice(update);
        bytes
    }

    #[test]
    fn rectangles_are_read_with_or_without_a_head_of_their_own() {
        let plain = rectangle(0, &[1; 32]);
        let without_head = rectangle(0x0401, &[2; 10]);
        let with_head = rectangle(0x0001, &[[9; 8].as_slice(), &[3; 5]].concat());
        let read = read_rectangles(&update(&[plain, without_head, with_head])).unwrap();
        assert_eq!(read.len(), 3);
        assert_eq!(
            (read[0].left, read[0].top, read[0].right, read[0].bottom, read[0].width, read[0].height, read[0].bits),
            (10, 20, 13, 21, 4, 2, 32)
        );
        assert!(!read[0].compressed);
        assert_eq!(read[0].carries, [1; 32]);
        assert!(read[1].compressed);
        assert_eq!(read[1].carries, [2; 10]);
        assert_eq!(read[2].carries, [3; 5], "its own head is left out");
    }

    #[test]
    fn an_update_in_pieces_is_put_together() {
        let whole = update(&[rectangle(0, &[7; 40])]);
        let mut updates = Updates::new();
        // a pointer update, then the first piece, one in between, and the last
        let mut frame = short(0x05, &[]);
        frame.extend(short(0x01 | 0x20, &whole[..10]));
        assert_eq!(updates.short_frame(&frame).unwrap(), []);
        assert_eq!(updates.short_frame(&short(0x01 | 0x30, &whole[10..30])).unwrap(), []);
        let read = updates.short_frame(&short(0x01 | 0x10, &whole[30..])).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].carries, [7; 40]);
        // and a single one after it
        assert_eq!(updates.short_frame(&short(0x01, &whole)).unwrap().len(), 1);
    }

    #[test]
    fn pieces_without_an_end_are_refused_once_they_are_more_than_the_client_takes() {
        let mut updates = Updates::new();
        let piece = short(0x01 | 0x30, &[0; 60_000]);
        let mut taken = 0;
        let refused = loop {
            match updates.short_frame(&piece) {
                Ok(_) => taken += 60_000,
                Err(error) => break error,
            }
        };
        assert_eq!(refused, Error::NotRdp("an update is larger than the client said it takes"));
        assert!(taken <= LARGEST && taken + 60_000 > LARGEST);
    }

    #[test]
    fn what_is_cut_or_compressed_on_the_way_is_refused() {
        let whole = short(0x01, &update(&[rectangle(0, &[7; 40])]));
        for cut in 1..whole.len() {
            assert!(Updates::new().short_frame(&whole[..cut]).is_err(), "cut at {cut}");
        }
        assert_eq!(Updates::new().short_frame(&[0x81, 0, 0, 0]), Err(Error::NotYet("compresses what it draws on the way")));
    }
}
