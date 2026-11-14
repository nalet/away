//! The frames RDP travels in at the start of a connection: four bytes that say how long
//! a frame is, then the frame (TPKT, RFC 1006).

use crate::Error;

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
    fn what_is_no_frame_is_refused() {
        let mut frames = Frames::new();
        frames.push(b"HTTP/1.1 400 Bad Request");
        assert_eq!(frames.next_frame(), Err(Error::NotRdp("a frame doesn't begin as one")));
        let mut frames = Frames::new();
        frames.push(&[3, 0, 0, 2]);
        assert_eq!(frames.next_frame(), Err(Error::NotRdp("a frame says it is shorter than its own beginning")));
    }
}
