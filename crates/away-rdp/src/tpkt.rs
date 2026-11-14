//! The frames RDP travels in. At the start of a connection every frame is four bytes that
//! say how long it is, then what it carries (TPKT, RFC 1006). Once a desktop is drawn, the
//! machine also sends a shorter kind, made for what it draws (the fast path).

use crate::Error;

/// One frame from the machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Frame {
    /// A frame of the ordinary kind, and what it carries.
    Slow(Vec<u8>),
    /// A frame of the shorter kind: its first byte, and what follows its length.
    Fast(u8, Vec<u8>),
}

/// What an ordinary frame carries once the connection is open: a data unit, of which the
/// first three bytes say only that it is one.
pub fn data(carried: &[u8]) -> Result<&[u8], Error> {
    match carried {
        [2, 0xf0, 0x80, rest @ ..] => Ok(rest),
        _ => Err(Error::NotRdp("a frame carries no data unit")),
    }
}

/// A frame around a data unit.
pub fn data_frame(carried: &[u8]) -> Vec<u8> {
    let mut unit = Vec::with_capacity(carried.len() + 3);
    unit.extend_from_slice(&[2, 0xf0, 0x80]);
    unit.extend_from_slice(carried);
    frame(&unit)
}

/// A frame around what it carries.
///
/// # Panics
///
/// When what it carries is longer than a frame can say: 65 531 bytes. Nothing at the
/// start of a connection comes near that.
pub fn frame(carried: &[u8]) -> Vec<u8> {
    let length = u16::try_from(carried.len() + 4).expect("a frame carries at most 65 531 bytes");
    let mut frame = Vec::with_capacity(carried.len() + 4);
    frame.extend_from_slice(&[3, 0]);
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(carried);
    frame
}

/// The bytes that arrived and are not yet a whole frame.
#[derive(Default)]
pub struct Frames {
    held: Vec<u8>,
}

impl Frames {
    /// Nothing has arrived yet.
    pub fn new() -> Self {
        Frames::default()
    }

    /// More bytes arrived.
    pub fn push(&mut self, bytes: &[u8]) {
        self.held.extend_from_slice(bytes);
    }

    /// What the next whole frame carries, if one has arrived.
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>, Error> {
        if self.held.len() < 4 {
            return Ok(None);
        }
        if self.held[0] != 3 {
            return Err(Error::NotRdp("a frame doesn't begin as one"));
        }
        let length = usize::from(u16::from_be_bytes([self.held[2], self.held[3]]));
        if length < 4 {
            return Err(Error::NotRdp("a frame says it is shorter than its own beginning"));
        }
        if self.held.len() < length {
            return Ok(None);
        }
        let carried = self.held[4..length].to_vec();
        self.held.drain(..length);
        Ok(Some(carried))
    }

    /// The next whole frame of either kind, if one has arrived.
    pub fn next_of_either_kind(&mut self) -> Result<Option<Frame>, Error> {
        let [first, second, ..] = self.held[..] else {
            return Ok(None);
        };
        if first == 3 {
            return Ok(self.next_frame()?.map(Frame::Slow));
        }
        if first & 0x3 != 0 {
            return Err(Error::NotRdp("a frame doesn't begin as one"));
        }
        // the shorter kind: its length is one byte, or two when the first has its top bit set
        let (length, begins) = if second & 0x80 == 0 {
            (usize::from(second), 2)
        } else {
            let Some(third) = self.held.get(2) else { return Ok(None) };
            ((usize::from(second & 0x7f) << 8) | usize::from(*third), 3)
        };
        if length < begins {
            return Err(Error::NotRdp("a frame says it is shorter than its own beginning"));
        }
        if self.held.len() < length {
            return Ok(None);
        }
        let carried = self.held[begins..length].to_vec();
        self.held.drain(..length);
        Ok(Some(Frame::Fast(first, carried)))
    }

    /// The bytes that arrived after the last whole frame.
    pub fn left(&self) -> &[u8] {
        &self.held
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_says_its_whole_length() {
        assert_eq!(frame(&[0xaa, 0xbb, 0xcc]), [3, 0, 0, 7, 0xaa, 0xbb, 0xcc]);
    }

    #[test]
    fn frames_come_out_whole_however_the_bytes_arrive() {
        let mut frames = Frames::new();
        let two = [frame(b"first"), frame(b"second")].concat();
        for byte in &two[..two.len() - 1] {
            frames.push(&[*byte]);
        }
        assert_eq!(frames.next_frame().unwrap().unwrap(), b"first");
        assert_eq!(frames.next_frame().unwrap(), None, "the second is one byte short");
        frames.push(&two[two.len() - 1..]);
        assert_eq!(frames.next_frame().unwrap().unwrap(), b"second");
        assert_eq!(frames.next_frame().unwrap(), None);
        assert!(frames.left().is_empty());
    }

    #[test]
    fn both_kinds_of_frame_come_out_of_one_stream() {
        let mut frames = Frames::new();
        frames.push(&data_frame(b"slow"));
        frames.push(&[0x00, 0x05, 1, 2, 3]);
        let mut long = vec![0x00, 0x81, 0x00];
        long.extend(std::iter::repeat_n(7, 253));
        frames.push(&long[..100]);
        assert_eq!(frames.next_of_either_kind().unwrap(), Some(Frame::Slow([&[2, 0xf0, 0x80][..], b"slow"].concat())));
        assert_eq!(frames.next_of_either_kind().unwrap(), Some(Frame::Fast(0, vec![1, 2, 3])));
        assert_eq!(frames.next_of_either_kind().unwrap(), None, "the long one has not all arrived");
        frames.push(&long[100..]);
        assert_eq!(frames.next_of_either_kind().unwrap(), Some(Frame::Fast(0, vec![7; 253])));
        assert_eq!(frames.next_of_either_kind().unwrap(), None);
        assert_eq!(data(&[2, 0xf0, 0x80, 9]).unwrap(), [9]);
        assert!(data(&[2, 0xf0]).is_err());
        let mut wrong = Frames::new();
        wrong.push(&[0x01, 0x05, 0, 0, 0]);
        assert!(wrong.next_of_either_kind().is_err());
        let mut short = Frames::new();
        short.push(&[0x00, 0x01]);
        assert!(short.next_of_either_kind().is_err());
    }

    #[test]
    fn what_is_no_frame_is_refused() {
        let mut frames = Frames::new();
        frames.push(b"HTTP/1.1 400 Bad Request");
        assert_eq!(frames.next_frame(), Err(Error::NotRdp("a frame doesn't begin as one")));
        let mut frames = Frames::new();
        frames.push(&[3, 0, 0, 2]);
        assert_eq!(frames.next_frame(), Err(Error::NotRdp("a frame says it is shorter than its own beginning")));
    }
}
