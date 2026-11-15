//! A further channel of a connection, beside the desktop's (MS-RDPBCGR 2.2.6): a part of
//! RDP of its own travels on one, such as what starts a program. A message travels in
//! pieces of 1600 bytes at most, each with a head that says how long the whole message
//! is, and whether the piece is its first or its last.

use crate::Error;

/// The most a piece carries.
pub const PIECE: usize = 1600;
/// The longest message the client takes on a channel.
pub const LONGEST: usize = 1024 * 1024;

const FIRST: u32 = 0x0000_0001;
const LAST: u32 = 0x0000_0002;
const SHOWS_ITS_HEADS: u32 = 0x0000_0010;
const COMPRESSED: u32 = 0x0020_0000;

/// How a channel is asked for when the connection opens: its name, of seven letters at
/// most, and that it is set up and travels secured as the connection is. Whether what
/// reads the channel on the machine is shown the heads of its pieces depends on the
/// part of RDP that travels on it: the one of programs is, the one of devices is not.
/// Nothing on a channel is compressed.
pub fn asked_for(name: &str, shows_its_heads: bool) -> [u8; 12] {
    let mut asked = [0u8; 12];
    for (to, from) in asked[..7].iter_mut().zip(name.bytes()) {
        *to = from;
    }
    let how: u32 = if shows_its_heads { 0xc020_0000 } else { 0xc000_0000 };
    asked[8..].copy_from_slice(&how.to_le_bytes());
    asked
}

/// A message in the pieces it travels in, each with its head, on a channel that was
/// asked for this way.
pub fn pieces(message: &[u8], shows_its_heads: bool) -> Vec<Vec<u8>> {
    let shown = if shows_its_heads { SHOWS_ITS_HEADS } else { 0 };
    let piece = |flags: u32, carried: &[u8]| {
        let mut piece = Vec::with_capacity(carried.len() + 8);
        piece.extend((message.len() as u32).to_le_bytes());
        piece.extend((flags | shown).to_le_bytes());
        piece.extend_from_slice(carried);
        piece
    };
    if message.is_empty() {
        return vec![piece(FIRST | LAST, &[])];
    }
    let last = (message.len() - 1) / PIECE;
    let flags = |nth: usize| if nth == 0 { FIRST } else { 0 } | if nth == last { LAST } else { 0 };
    message.chunks(PIECE).enumerate().map(|(nth, carried)| piece(flags(nth), carried)).collect()
}

/// The pieces of a channel's messages as they arrive, put together.
#[derive(Default)]
pub struct Pieces {
    whole: Vec<u8>,
    length: usize,
    begun: bool,
}

impl Pieces {
    /// Takes one piece. Gives the message once its last piece is there.
    pub fn take(&mut self, piece: &[u8]) -> Result<Option<Vec<u8>>, Error> {
        const WRONG: Error = Error::NotRdp("a piece of a channel's message is not built as one");
        let number = |at: usize| piece.get(at..at + 4).map(|four| u32::from_le_bytes([four[0], four[1], four[2], four[3]])).ok_or(WRONG);
        let (length, flags) = (number(0)? as usize, number(4)?);
        let carried = &piece[8..];
        if flags & COMPRESSED != 0 {
            return Err(Error::NotYet("compresses what travels on a further channel"));
        }
        if flags & FIRST != 0 {
            if length > LONGEST {
                return Err(Error::NotRdp("a message on a channel is longer than the client takes"));
            }
            self.whole.clear();
            self.length = length;
            self.begun = true;
        }
        // a piece of a message that never began, or more than the message said it is
        if !self.begun || self.whole.len() + carried.len() > self.length {
            return Err(WRONG);
        }
        self.whole.extend_from_slice(carried);
        if flags & LAST == 0 {
            return Ok(None);
        }
        self.begun = false;
        if self.whole.len() != self.length {
            return Err(WRONG);
        }
        Ok(Some(std::mem::take(&mut self.whole)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_travels_in_pieces_and_is_put_together_again() {
        assert_eq!(&asked_for("rail", true), &[b'r', b'a', b'i', b'l', 0, 0, 0, 0, 0x00, 0x00, 0x20, 0xc0]);
        assert_eq!(&asked_for("a-long-name", true)[..8], b"a-long-\0");
        // the channel of devices shows what reads it no heads, in how it is asked for and in each piece
        assert_eq!(&asked_for("rdpdr", false), &[b'r', b'd', b'p', b'd', b'r', 0, 0, 0, 0x00, 0x00, 0x00, 0xc0]);
        assert_eq!(pieces(b"ready", false), [[&[5, 0, 0, 0, 0x03, 0, 0, 0][..], b"ready"].concat()]);

        let short = pieces(b"ready", true);
        assert_eq!(short, [[&[5, 0, 0, 0, 0x13, 0, 0, 0][..], b"ready"].concat()]);
        assert_eq!(pieces(b"", true), [vec![0, 0, 0, 0, 0x13, 0, 0, 0]]);
        let message: Vec<u8> = (0..4000u32).map(|at| at as u8).collect();
        let long = pieces(&message, true);
        assert_eq!(long.iter().map(|piece| (piece.len() - 8, piece[4])).collect::<Vec<_>>(), [(1600, 0x11), (1600, 0x10), (800, 0x12)]);
        assert!(long.iter().all(|piece| piece[..4] == 4000u32.to_le_bytes()));
        // exactly two pieces' worth is two pieces
        assert_eq!(pieces(&message[..3200], true).len(), 2);

        let mut arriving = Pieces::default();
        assert_eq!(arriving.take(&short[0]), Ok(Some(b"ready".to_vec())));
        assert_eq!(arriving.take(&long[0]), Ok(None));
        assert_eq!(arriving.take(&long[1]), Ok(None));
        assert_eq!(arriving.take(&long[2]), Ok(Some(message.clone())));
        // and the next message after it
        assert_eq!(arriving.take(&pieces(b"", true)[0]), Ok(Some(Vec::new())));
    }

    #[test]
    fn pieces_that_do_not_make_a_message_are_refused() {
        let message = vec![7u8; 2000];
        let long = pieces(&message, true);
        // a piece of a message that never began
        assert!(Pieces::default().take(&long[1]).is_err());
        // more than the message said it is, and less
        let mut arriving = Pieces::default();
        assert_eq!(arriving.take(&long[0]), Ok(None));
        let mut more = long[1].clone();
        more.push(0);
        assert!(arriving.take(&more).is_err());
        let mut arriving = Pieces::default();
        assert_eq!(arriving.take(&long[0]), Ok(None));
        assert!(arriving.take(&long[1][..long[1].len() - 1]).is_err());
        // a head that is cut, a message that is too long, and one that is compressed
        assert!(Pieces::default().take(&[1, 0, 0]).is_err());
        let mut huge = long[0].clone();
        huge[..4].copy_from_slice(&(LONGEST as u32 + 1).to_le_bytes());
        assert!(Pieces::default().take(&huge).is_err());
        let mut compressed = pieces(b"ready", true).remove(0);
        compressed[6] = 0x20;
        assert_eq!(Pieces::default().take(&compressed), Err(Error::NotYet("compresses what travels on a further channel")));
    }
}
