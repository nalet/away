//! Channels that a machine opens while a connection runs (MS-RDPEDYC), of which Away
//! takes one: the one a client says over how its screen is now, so that the desktop
//! follows a window that changes its size (MS-RDPEDISP). They travel on a further
//! channel of their own, and each has a number the machine gives it.

use crate::Error;

/// The further channel that the channels opened later travel on.
pub const CHANNEL: &str = "drdynvc";
/// The channel a client says over how its screen is now.
pub const SCREEN: &str = "Microsoft::Windows::RDS::DisplayControl";

/// The version of these channels the client speaks: the first, in which nothing is
/// compressed.
const VERSION: u16 = 1;
/// What the client answers a channel it does not take with: that it failed.
const NOT_TAKEN: u32 = 0x8000_4005;
/// The longest message the client takes on a channel that was opened later.
const LONGEST: usize = 64 * 1024;

const CAN_DO: u8 = 5;
const OPENS: u8 = 1;
const FIRST_PART: u8 = 2;
const PART: u8 = 3;
const CLOSES: u8 = 4;

/// What the machine says of a client's screen: what it takes.
const SCREEN_TAKES: u32 = 5;
/// What the client says of its screen: how it is laid out.
const SCREEN_IS: u32 = 2;

/// The smallest and the largest side a machine takes for a screen, in dots.
const SMALLEST_SIDE: u16 = 200;
const LARGEST_SIDE: u16 = 8192;

/// The channels that were opened later, as far as the machine and the client are with them.
#[derive(Debug, Default)]
pub struct Later {
    /// The number the machine gave the channel of the screen, once it opened it.
    screen: Option<u32>,
    /// How many dots a screen has at most for the machine, once it said so.
    most_dots: Option<u64>,
    /// A message that came in more than one part and is not whole yet: the channel it
    /// is on, how long it is, and what is there of it.
    parts: Option<(u32, usize, Vec<u8>)>,
}

/// A number of one, two or four bytes, as the head of a message says how long it is.
fn number(message: &[u8], at: usize, size: u8) -> Option<(u32, usize)> {
    let bytes = match size {
        0 => 1,
        1 => 2,
        2 => 4,
        _ => return None,
    };
    let carried = message.get(at..at + bytes)?;
    let mut number = [0u8; 4];
    number[..bytes].copy_from_slice(carried);
    Some((u32::from_le_bytes(number), at + bytes))
}

/// A channel's number in as few bytes as it takes, and how the head names that size.
fn written(id: u32) -> (u8, Vec<u8>) {
    match id {
        0..=0xff => (0, vec![id as u8]),
        0x100..=0xffff => (1, (id as u16).to_le_bytes().to_vec()),
        _ => (2, id.to_le_bytes().to_vec()),
    }
}

impl Later {
    /// Takes one message of the further channel. Gives what to answer, each a message
    /// of that channel.
    pub fn receive(&mut self, message: &[u8]) -> Result<Vec<Vec<u8>>, Error> {
        const WRONG: Error = Error::NotRdp("a message about the channels that are opened later is not built as one");
        let head = *message.first().ok_or(WRONG)?;
        let (what, how, size) = (head >> 4, (head >> 2) & 3, head & 3);
        match what {
            // the machine says which version it speaks: the client speaks the first
            CAN_DO => {
                let mut answer = vec![CAN_DO << 4, 0];
                answer.extend(VERSION.to_le_bytes());
                Ok(vec![answer])
            }
            // the machine opens a channel, by its name: the client takes the one of the
            // screen, and no other
            OPENS => {
                let (id, at) = number(message, 1, size).ok_or(WRONG)?;
                let name = message.get(at..).ok_or(WRONG)?;
                let name = name.split(|byte| *byte == 0).next().unwrap_or_default();
                let taken = name == SCREEN.as_bytes();
                if taken {
                    (self.screen, self.most_dots) = (Some(id), None);
                }
                let mut answer = vec![(OPENS << 4) | size];
                answer.extend_from_slice(&message[1..at]);
                answer.extend(if taken { 0 } else { NOT_TAKEN }.to_le_bytes());
                Ok(vec![answer])
            }
            // the first part of a message that is longer than one part, with its length
            FIRST_PART => {
                let (id, at) = number(message, 1, size).ok_or(WRONG)?;
                let (length, at) = number(message, at, how).ok_or(WRONG)?;
                let carried = message.get(at..).ok_or(WRONG)?;
                if length as usize > LONGEST || carried.len() > length as usize {
                    return Err(Error::NotRdp("a message on a channel that was opened later is longer than the client takes"));
                }
                self.parts = Some((id, length as usize, carried.to_vec()));
                self.whole();
                Ok(Vec::new())
            }
            // a message, or a further part of one
            PART => {
                let (id, at) = number(message, 1, size).ok_or(WRONG)?;
                let carried = message.get(at..).ok_or(WRONG)?;
                match &mut self.parts {
                    Some((of, length, there)) if *of == id => {
                        if there.len() + carried.len() > *length {
                            return Err(WRONG);
                        }
                        there.extend_from_slice(carried);
                        self.whole();
                    }
                    _ => self.said(id, carried),
                }
                Ok(Vec::new())
            }
            // the machine closes a channel: the client confirms it
            CLOSES => {
                let (id, at) = number(message, 1, size).ok_or(WRONG)?;
                if self.screen == Some(id) {
                    (self.screen, self.most_dots) = (None, None);
                }
                if self.parts.as_ref().is_some_and(|(of, ..)| *of == id) {
                    self.parts = None;
                }
                Ok(vec![message[..at].to_vec()])
            }
            // what the first version does not have, and what is not Away's
            _ => Ok(Vec::new()),
        }
    }

    /// Takes a message that came in parts once it is whole.
    fn whole(&mut self) {
        if self.parts.as_ref().is_some_and(|(_, length, there)| there.len() == *length) {
            if let Some((id, _, there)) = self.parts.take() {
                self.said(id, &there);
            }
        }
    }

    /// What the machine said on a channel that was opened later.
    fn said(&mut self, id: u32, said: &[u8]) {
        if self.screen != Some(id) {
            return;
        }
        let four = |at: usize| said.get(at..at + 4).map(|four| u32::from_le_bytes([four[0], four[1], four[2], four[3]]));
        // what it takes of a screen: how many screens, and how many dots each has at most
        if let (Some(SCREEN_TAKES), Some(screens), Some(wide), Some(high)) = (four(0), four(8), four(12), four(16)) {
            self.most_dots = Some(u64::from(screens) * u64::from(wide) * u64::from(high));
        }
    }

    /// Whether the machine follows the client's screen as it changes: it opened the
    /// channel for it and said what it takes.
    pub fn follows(&self) -> bool {
        self.screen.is_some() && self.most_dots.is_some()
    }

    /// What tells the machine that the client's screen is now this: one screen of this
    /// many dots, on which the machine draws this much larger than usual, in percent.
    /// Nothing where the machine doesn't follow the screen, or takes no screen like it.
    pub fn screen(&self, width: u16, height: u16, scale: u32) -> Option<Vec<u8>> {
        let (id, most_dots) = (self.screen?, self.most_dots?);
        let sides = SMALLEST_SIDE..=LARGEST_SIDE;
        if !sides.contains(&width)
            || !sides.contains(&height)
            || !width.is_multiple_of(2)
            || u64::from(width) * u64::from(height) > most_dots
        {
            return None;
        }
        let scale = scale.clamp(100, 500);
        let (size, id) = written(id);
        let mut message = vec![(PART << 4) | size];
        message.extend(id);
        // one screen, which is the first one, with its corner where the desktop begins
        for number in [SCREEN_IS, 56, 40, 1, 1, 0, 0, u32::from(width), u32::from(height)] {
            message.extend(number.to_le_bytes());
        }
        message.extend(crate::gcc::millimetres(width, scale).to_le_bytes());
        message.extend(crate::gcc::millimetres(height, scale).to_le_bytes());
        // it lies on its long side; and of a device's own scales, the usual one
        for number in [0, scale, 100] {
            message.extend(number.to_le_bytes());
        }
        Some(message)
    }
}

/// What a machine says on these channels, for the stand-ins and the tests.
#[cfg(any(test, feature = "stand-in"))]
pub mod stand_in {
    use super::*;

    /// The machine says which version it speaks.
    pub fn can_do(version: u16) -> Vec<u8> {
        let mut message = vec![CAN_DO << 4, 0];
        message.extend(version.to_le_bytes());
        message
    }

    /// The machine opens a channel of this number and name.
    pub fn opens(id: u32, name: &str) -> Vec<u8> {
        let (size, id) = written(id);
        let mut message = vec![(OPENS << 4) | size];
        message.extend(id);
        message.extend(name.bytes());
        message.push(0);
        message
    }

    /// The machine says on the channel of the screen what it takes: one screen, of
    /// this many dots each way at most.
    pub fn takes(id: u32, wide: u32, high: u32) -> Vec<u8> {
        let (size, id) = written(id);
        let mut message = vec![(PART << 4) | size];
        message.extend(id);
        for number in [SCREEN_TAKES, 20, 1, wide, high] {
            message.extend(number.to_le_bytes());
        }
        message
    }

    /// What a client answered when a channel was opened: its number, and whether the
    /// client took it.
    pub fn read_answer(message: &[u8]) -> Option<(u32, bool)> {
        let head = *message.first()?;
        if head >> 4 != OPENS {
            return None;
        }
        let (id, at) = number(message, 1, head & 3)?;
        let status = message.get(at..at + 4)?;
        Some((id, status == [0, 0, 0, 0]))
    }

    /// What a client said its screen is: the channel it said it on, how wide and how
    /// high the screen is, and how much larger than usual the machine is to draw.
    pub fn read_screen(message: &[u8]) -> Option<(u32, u16, u16, u32)> {
        let head = *message.first()?;
        if head >> 4 != PART {
            return None;
        }
        let (id, at) = number(message, 1, head & 3)?;
        let said = message.get(at..)?;
        let four = |at: usize| said.get(at..at + 4).map(|four| u32::from_le_bytes([four[0], four[1], four[2], four[3]]));
        if (four(0)?, four(4)?, four(8)?, four(12)?) != (SCREEN_IS, 56, 40, 1) {
            return None;
        }
        Some((id, u16::try_from(four(28)?).ok()?, u16::try_from(four(32)?).ok()?, four(48)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_client_takes_the_channel_of_the_screen_and_no_other() {
        let mut later = Later::default();
        // the machine speaks the third version; the client answers with the first
        assert_eq!(later.receive(&stand_in::can_do(3)).unwrap(), [vec![0x50, 0, 1, 0]]);
        assert!(!later.follows());
        // a channel that is not Away's is answered with a failure
        let other = later.receive(&stand_in::opens(7, "Microsoft::Windows::RDS::Geometry::v08.01")).unwrap();
        assert_eq!(other, [vec![0x10, 7, 0x05, 0x40, 0x00, 0x80]]);
        assert_eq!(stand_in::read_answer(&other[0]), Some((7, false)));
        // the one of the screen is taken, under the number the machine gave it
        let taken = later.receive(&stand_in::opens(0x1234, SCREEN)).unwrap();
        assert_eq!(taken, [vec![0x11, 0x34, 0x12, 0, 0, 0, 0]]);
        assert_eq!(stand_in::read_answer(&taken[0]), Some((0x1234, true)));
        // the machine has not said what it takes: nothing is said of the screen yet
        assert!(!later.follows());
        assert_eq!(later.screen(1280, 800, 100), None);
        assert_eq!(later.receive(&stand_in::takes(0x1234, 8192, 8192)).unwrap(), Vec::<Vec<u8>>::new());
        assert!(later.follows());

        // what the machine says on another channel is not what it takes of a screen
        let mut elsewhere = Later::default();
        elsewhere.receive(&stand_in::opens(9, SCREEN)).unwrap();
        elsewhere.receive(&stand_in::takes(8, 8192, 8192)).unwrap();
        assert!(!elsewhere.follows());
    }

    #[test]
    fn the_client_says_how_its_screen_is_now() {
        let mut later = Later::default();
        later.receive(&stand_in::opens(3, SCREEN)).unwrap();
        later.receive(&stand_in::takes(3, 8192, 8192)).unwrap();
        let said = later.screen(2560, 1600, 200).expect("a screen");
        // a message of the channel 3: the kind, its length, one screen of forty bytes
        assert_eq!(&said[..2], &[0x30, 3]);
        assert_eq!(&said[2..18], &[2, 0, 0, 0, 56, 0, 0, 0, 40, 0, 0, 0, 1, 0, 0, 0]);
        // the first screen, at the desktop's corner, 2560 by 1600
        assert_eq!(&said[18..38], &[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x00, 0x0a, 0, 0, 0x40, 0x06, 0, 0]);
        // 338 by 211 millimetres, on its long side, drawn twice as large
        assert_eq!(&said[38..58], &[0x52, 0x01, 0, 0, 0xd3, 0, 0, 0, 0, 0, 0, 0, 200, 0, 0, 0, 100, 0, 0, 0]);
        assert_eq!(said.len(), 58);
        assert_eq!(stand_in::read_screen(&said), Some((3, 2560, 1600, 200)));

        // a screen no machine takes: too small, too large, of an odd width, or of more
        // dots than this machine said it takes
        for (width, height) in [(198, 800), (1280, 100), (8194, 800), (1281, 800)] {
            assert_eq!(later.screen(width, height, 100), None, "{width} by {height}");
        }
        let mut small = Later::default();
        small.receive(&stand_in::opens(3, SCREEN)).unwrap();
        small.receive(&stand_in::takes(3, 1920, 1080)).unwrap();
        assert!(small.screen(1920, 1080, 100).is_some());
        assert_eq!(small.screen(2560, 1600, 100), None);

        // the machine closes the channel: the client confirms, and says nothing of its screen any more
        assert_eq!(later.receive(&[0x40, 3]).unwrap(), [vec![0x40, 3]]);
        assert!(!later.follows());
        assert_eq!(later.screen(1280, 800, 100), None);
    }

    #[test]
    fn a_message_in_parts_is_put_together_and_what_is_not_built_as_one_is_refused() {
        let mut later = Later::default();
        later.receive(&stand_in::opens(3, SCREEN)).unwrap();
        // what the machine takes, in two parts: the first says how long the whole is
        let whole = stand_in::takes(3, 4096, 2160);
        let said = &whole[2..];
        let mut first = vec![0x20, 3, said.len() as u8];
        first.extend_from_slice(&said[..8]);
        let mut second = vec![0x30, 3];
        second.extend_from_slice(&said[8..]);
        assert_eq!(later.receive(&first).unwrap(), Vec::<Vec<u8>>::new());
        assert!(!later.follows());
        assert_eq!(later.receive(&second).unwrap(), Vec::<Vec<u8>>::new());
        assert!(later.follows());
        assert!(later.screen(4096, 2160, 100).is_some());
        assert_eq!(later.screen(4096, 2162, 100), None);

        // more than the first part said, a head that is cut off, and nothing at all
        let mut again = Later::default();
        again.receive(&stand_in::opens(3, SCREEN)).unwrap();
        again.receive(&[0x20, 3, 4, 1, 2]).unwrap();
        assert!(again.receive(&[0x30, 3, 3, 4, 5]).is_err());
        assert!(Later::default().receive(&[]).is_err());
        assert!(Later::default().receive(&[0x12, 1]).is_err());
        assert!(Later::default().receive(&[0x13, 1, 2, 3, 4]).is_err());
        // what the first version does not have is passed over
        assert_eq!(Later::default().receive(&[0x80, 0, 0, 0]).unwrap(), Vec::<Vec<u8>>::new());
        for cut in 0..whole.len() {
            let _ = Later::default().receive(&whole[..cut]);
        }
    }
}
