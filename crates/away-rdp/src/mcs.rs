//! The layer that gives a connection its channels (MCS, T.125): the client says what it
//! is and asks for channels, the machine answers with their numbers, and from then on
//! every message travels on one of them.

use crate::der::{self, Reader};
use crate::Error;

/// The number a machine's own channel has.
pub const MACHINE: u16 = 1002;

/// How many channels, users and bytes a domain may have: eight numbers, three times.
pub(crate) fn limits(channels: u32, users: u32, tokens: u32, unit: u32) -> Vec<u8> {
    let mut numbers = Vec::new();
    for number in [channels, users, tokens, 1, 0, 1, unit, 2] {
        numbers.extend(der::write_number(number));
    }
    der::write(der::SEQUENCE, &numbers)
}

/// The first message: what the client asks of the domain, and what it says of itself.
pub fn connect(says_of_itself: &[u8]) -> Vec<u8> {
    let mut fields = Vec::new();
    // who calls and who is called, and that this client leads upward
    fields.extend(der::write(der::OCTET_STRING, &[1]));
    fields.extend(der::write(der::OCTET_STRING, &[1]));
    fields.extend([0x01, 0x01, 0xff]);
    // what it would like, the least it takes, and the most
    fields.extend(limits(34, 2, 0, 65_535));
    fields.extend(limits(1, 1, 1, 1_056));
    fields.extend(limits(65_535, 64_535, 65_535, 65_535));
    fields.extend(der::write(der::OCTET_STRING, says_of_itself));
    let mut message = vec![0x7f, 0x65];
    message.extend(&der::write(0, &fields)[1..]);
    message
}

/// Reads the machine's answer to the first message, and gives what it says of itself.
pub fn read_connect_answer(message: &[u8]) -> Result<&[u8], Error> {
    const WRONG: Error = Error::NotRdp("the answer to what the client says of itself is not built as one");
    let wrong = |_| WRONG;
    let [0x7f, 0x66, rest @ ..] = message else {
        return Err(WRONG);
    };
    let holds = after_length(rest).ok_or(WRONG)?;
    let mut fields = Reader::new(holds);
    let result = fields.expect(0x0a).map_err(wrong)?;
    if result != [0] {
        return Err(Error::NotRdp("the machine refused what the client says of itself"));
    }
    fields.expect(der::INTEGER).map_err(wrong)?;
    fields.expect(der::SEQUENCE).map_err(wrong)?;
    fields.expect(der::OCTET_STRING).map_err(wrong)
}

/// What follows a length: as many bytes as the length says.
fn after_length(bytes: &[u8]) -> Option<&[u8]> {
    let (first, rest) = bytes.split_first()?;
    let (length, rest) = if first & 0x80 == 0 {
        (usize::from(*first), rest)
    } else {
        let count = usize::from(first & 0x7f);
        if count == 0 || count > 4 || rest.len() < count {
            return None;
        }
        (rest[..count].iter().fold(0usize, |length, byte| (length << 8) | usize::from(*byte)), &rest[count..])
    };
    rest.get(..length)
}

/// Asks the machine to make the domain, then for a user of its own.
pub fn erect_and_attach() -> [Vec<u8>; 2] {
    [vec![0x04, 0x01, 0x00, 0x01, 0x00], vec![0x28]]
}

/// Reads the machine's answer to the request for a user: the user's channel.
pub fn read_attached(message: &[u8]) -> Result<u16, Error> {
    match message {
        [0x2e, 0, high, low, ..] => Ok(u16::from_be_bytes([*high, *low]) + 1001),
        [0x2e | 0x2c, ..] => Err(Error::NotRdp("the machine gave the client no user of its own")),
        _ => Err(Error::NotRdp("the answer to the request for a user is not built as one")),
    }
}

/// Asks to join a channel.
pub fn join(user: u16, channel: u16) -> Vec<u8> {
    let mut message = vec![0x38];
    message.extend((user - 1001).to_be_bytes());
    message.extend(channel.to_be_bytes());
    message
}

/// Reads the machine's answer to a request to join: the channel that was joined.
pub fn read_joined(message: &[u8]) -> Result<u16, Error> {
    match message {
        [0x3e, 0, _, _, _, _, high, low] => Ok(u16::from_be_bytes([*high, *low])),
        [0x3e, 0, _, _, high, low] => Ok(u16::from_be_bytes([*high, *low])),
        [0x3e | 0x3c, ..] => Err(Error::NotRdp("the machine refused a channel")),
        _ => Err(Error::NotRdp("the answer to a request to join is not built as one")),
    }
}

/// A message for the machine, on a channel.
pub fn send(user: u16, channel: u16, carried: &[u8]) -> Vec<u8> {
    let mut message = vec![0x64];
    message.extend((user - 1001).to_be_bytes());
    message.extend(channel.to_be_bytes());
    message.push(0x70);
    message.extend(((carried.len() as u16) | 0x8000).to_be_bytes());
    message.extend_from_slice(carried);
    message
}

/// Tells the machine that the client leaves, because the person asked it to.
pub fn leave() -> Vec<u8> {
    vec![0x21, 0x80]
}

/// What arrives from the machine once the channels are joined.
#[derive(Debug, PartialEq, Eq)]
pub enum Arrived<'a> {
    /// A message on a channel.
    On(u16, &'a [u8]),
    /// The machine ends the connection.
    Ending,
}

/// Reads what arrived from the machine.
pub fn read_arrived(message: &[u8]) -> Result<Arrived<'_>, Error> {
    const WRONG: Error = Error::NotRdp("a message on a channel is not built as one");
    match message {
        // a message: who sent it, on which channel, how it travels, how long it is
        [0x68, _, _, high, low, _, rest @ ..] => {
            let channel = u16::from_be_bytes([*high, *low]);
            let (length, carried) = match rest {
                [first, second, carried @ ..] if first & 0x80 != 0 => ((usize::from(first & 0x7f) << 8) | usize::from(*second), carried),
                [first, carried @ ..] => (usize::from(*first), carried),
                _ => return Err(WRONG),
            };
            carried.get(..length).map(|carried| Arrived::On(channel, carried)).ok_or(WRONG)
        }
        // the end of the connection, with whichever of its reasons: that the machine ends
        // it, as a Windows machine says, or that the other side asked for it
        [first, ..] if first & 0xfc == 0x20 => Ok(Arrived::Ending),
        _ => Err(WRONG),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_message_holds_the_three_limits_and_what_the_client_says() {
        let message = connect(&[0xaa; 300]);
        assert_eq!(&message[..2], &[0x7f, 0x65]);
        // its length in two bytes, then the two selectors and the flag
        assert_eq!(&message[2..5], &[0x82, 0x01, 0x92]);
        assert_eq!(&message[5..14], &[0x04, 0x01, 0x01, 0x04, 0x01, 0x01, 0x01, 0x01, 0xff]);
        // what it would like: 34 channels, 2 users, no tokens, one priority, units of 65 535 bytes
        assert_eq!(
            &message[14..42],
            &[
                0x30, 0x1a, 0x02, 0x01, 0x22, 0x02, 0x01, 0x02, 0x02, 0x01, 0x00, 0x02, 0x01, 0x01, 0x02, 0x01, 0x00, 0x02, 0x01, 0x01,
                0x02, 0x03, 0x00, 0xff, 0xff, 0x02, 0x01, 0x02
            ]
        );
        assert!(message.ends_with(&[0xaa; 300]));
        assert_eq!(message.len(), 5 + 0x192);
    }

    #[test]
    fn the_machines_answer_gives_what_it_says_of_itself() {
        let mut fields = vec![0x0a, 0x01, 0x00, 0x02, 0x01, 0x00];
        fields.extend(limits(34, 3, 0, 65_528));
        fields.extend(der::write(der::OCTET_STRING, b"what the machine says"));
        let mut answer = vec![0x7f, 0x66];
        answer.extend(&der::write(0, &fields)[1..]);
        assert_eq!(read_connect_answer(&answer).unwrap(), b"what the machine says");
        // the result is the sixth byte: anything but 0 is a refusal
        let mut refused = answer.clone();
        refused[5] = 2;
        assert!(read_connect_answer(&refused).is_err());
        for cut in 0..answer.len() {
            assert!(read_connect_answer(&answer[..cut]).is_err(), "cut at {cut}");
        }
    }

    #[test]
    fn a_user_and_its_channels() {
        assert_eq!(erect_and_attach(), [vec![0x04, 0x01, 0x00, 0x01, 0x00], vec![0x28]]);
        assert_eq!(read_attached(&[0x2e, 0x00, 0x00, 0x06]), Ok(1007));
        assert!(read_attached(&[0x2e, 0x01, 0x00, 0x06]).is_err());
        assert_eq!(join(1007, 1003), [0x38, 0x00, 0x06, 0x03, 0xeb]);
        assert_eq!(read_joined(&[0x3e, 0x00, 0x00, 0x06, 0x03, 0xeb, 0x03, 0xeb]), Ok(1003));
        assert!(read_joined(&[0x3e, 0x02, 0x00, 0x06, 0x03, 0xeb]).is_err());
    }

    #[test]
    fn a_message_travels_on_a_channel() {
        assert_eq!(send(1007, 1003, &[1, 2, 3]), [0x64, 0x00, 0x06, 0x03, 0xeb, 0x70, 0x80, 0x03, 1, 2, 3]);
        assert_eq!(read_arrived(&[0x68, 0x00, 0x01, 0x03, 0xeb, 0x70, 0x03, 1, 2, 3]), Ok(Arrived::On(1003, &[1, 2, 3][..])));
        assert_eq!(read_arrived(&[0x68, 0x00, 0x01, 0x03, 0xeb, 0x70, 0x80, 0x03, 1, 2, 3]), Ok(Arrived::On(1003, &[1, 2, 3][..])));
        assert_eq!(read_arrived(&[0x21, 0x80]), Ok(Arrived::Ending));
        // a machine that ends the connection itself says so with another reason
        assert_eq!(read_arrived(&[0x20, 0x80]), Ok(Arrived::Ending));
        assert_eq!(read_arrived(&[0x20, 0x00]), Ok(Arrived::Ending));
        assert!(read_arrived(&[0x24, 0x80]).is_err());
        assert!(read_arrived(&[0x68, 0x00, 0x01, 0x03, 0xeb, 0x70, 0x05, 1, 2]).is_err(), "shorter than it says");
        assert!(read_arrived(&[0x99]).is_err());
    }
}
