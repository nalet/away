//! Just enough of DER to read a certificate and to read and write the messages of the
//! sign-in before the desktop, which are written in the same encoding.
//!
//! A value is one byte that says what kind it is, its length, and what it holds. Only
//! lengths that are written out are read, which is all that either has.

use crate::Error;

/// A list of values, in order.
pub const SEQUENCE: u8 = 0x30;
/// A whole number.
pub const INTEGER: u8 = 0x02;
/// A string of bits.
pub const BIT_STRING: u8 = 0x03;
/// A string of bytes.
pub const OCTET_STRING: u8 = 0x04;
/// The name of a kind of thing, as numbers.
pub const OBJECT_NAME: u8 = 0x06;
/// A text in UTF-16.
pub const BMP_STRING: u8 = 0x1e;
/// A time with a year of two digits.
pub const UTC_TIME: u8 = 0x17;
/// A time with a year of four digits.
pub const GENERALIZED_TIME: u8 = 0x18;
/// The first of the values a kind of record numbers itself.
pub const FIRST_OF_ITS_OWN: u8 = 0xa0;

/// The kind of the value a record numbers itself: its first is 0.
pub const fn of_its_own(number: u8) -> u8 {
    FIRST_OF_ITS_OWN | number
}

/// A value of this kind that holds these bytes, written out.
pub fn write(kind: u8, holds: &[u8]) -> Vec<u8> {
    let mut written = Vec::with_capacity(holds.len() + 6);
    written.push(kind);
    match holds.len() {
        length @ 0..0x80 => written.push(length as u8),
        length => {
            let bytes = length.to_be_bytes();
            let bytes = &bytes[bytes.iter().position(|byte| *byte != 0).unwrap_or(bytes.len() - 1)..];
            written.push(0x80 | bytes.len() as u8);
            written.extend_from_slice(bytes);
        }
    }
    written.extend_from_slice(holds);
    written
}

/// A whole number that is not negative, written out.
pub fn write_number(number: u32) -> Vec<u8> {
    let bytes = number.to_be_bytes();
    let mut holds = bytes[bytes.iter().position(|byte| *byte != 0).unwrap_or(3)..].to_vec();
    // a first bit that is set would make it negative
    if holds[0] & 0x80 != 0 {
        holds.insert(0, 0);
    }
    write(INTEGER, &holds)
}

/// The whole number a value of that kind holds, when it fits in 32 bits. A negative one
/// is read as the same bits.
pub fn read_number(holds: &[u8]) -> Result<u32, Error> {
    let holds = match holds {
        [0, rest @ ..] if !rest.is_empty() => rest,
        other => other,
    };
    if holds.is_empty() || holds.len() > 4 {
        return Err(Error::SignIn("a number in it is longer than this code reads"));
    }
    Ok(holds.iter().fold(0u32, |number, byte| (number << 8) | u32::from(*byte)))
}

/// How long the value is that these bytes begin with, its kind and its length included,
/// once enough of it has arrived to say.
pub fn whole(bytes: &[u8]) -> Result<Option<usize>, Error> {
    let [_, first, rest @ ..] = bytes else {
        return Ok(None);
    };
    if first & 0x80 == 0 {
        return Ok(Some(2 + usize::from(*first)));
    }
    let count = usize::from(first & 0x7f);
    if count == 0 || count > 3 {
        return Err(Error::SignIn("a message's length is written in a way this code doesn't read"));
    }
    if rest.len() < count {
        return Ok(None);
    }
    Ok(Some(2 + count + rest[..count].iter().fold(0usize, |length, byte| (length << 8) | usize::from(*byte))))
}

/// One value: what kind it is, and what it holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Value<'a> {
    /// What kind of value it is.
    pub kind: u8,
    /// What it holds.
    pub holds: &'a [u8],
}

/// Reads values one after the other.
pub struct Reader<'a> {
    rest: &'a [u8],
}

impl<'a> Reader<'a> {
    /// A reader at the beginning of some bytes.
    pub fn new(bytes: &'a [u8]) -> Self {
        Reader { rest: bytes }
    }

    /// Whether every value was read.
    pub fn is_empty(&self) -> bool {
        self.rest.is_empty()
    }

    /// The next value.
    pub fn value(&mut self) -> Result<Value<'a>, Error> {
        let [kind, first, rest @ ..] = self.rest else {
            return Err(Error::Certificate("it ends in the middle of a value"));
        };
        if kind & 0x1f == 0x1f {
            return Err(Error::Certificate("it holds a kind of value this code doesn't read"));
        }
        let (length, rest) = if first & 0x80 == 0 {
            (usize::from(*first), rest)
        } else {
            let bytes = usize::from(first & 0x7f);
            if bytes == 0 || bytes > 4 || rest.len() < bytes {
                return Err(Error::Certificate("a value's length is written in a way this code doesn't read"));
            }
            let length = rest[..bytes].iter().fold(0usize, |length, byte| (length << 8) | usize::from(*byte));
            (length, &rest[bytes..])
        };
        if rest.len() < length {
            return Err(Error::Certificate("a value says it is longer than what holds it"));
        }
        let (holds, rest) = rest.split_at(length);
        self.rest = rest;
        Ok(Value { kind: *kind, holds })
    }

    /// What the next value holds, when it is of this kind.
    pub fn expect(&mut self, kind: u8) -> Result<&'a [u8], Error> {
        let value = self.value()?;
        if value.kind == kind {
            Ok(value.holds)
        } else {
            Err(Error::Certificate("it is not built as a certificate is"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_read_one_after_the_other() {
        let mut reader = Reader::new(&[0x02, 0x01, 0x05, 0x30, 0x03, 0x02, 0x01, 0x07]);
        assert_eq!(reader.value(), Ok(Value { kind: INTEGER, holds: &[5] }));
        assert_eq!(reader.expect(SEQUENCE), Ok(&[0x02, 0x01, 0x07][..]));
        assert!(reader.is_empty());
        assert!(reader.value().is_err());
    }

    #[test]
    fn a_long_value_says_its_length_in_several_bytes() {
        let mut long = vec![0x04, 0x82, 0x01, 0x00];
        long.extend(std::iter::repeat_n(0xab, 256));
        let value = Reader::new(&long).value().unwrap();
        assert_eq!(value.holds.len(), 256);
    }

    #[test]
    fn what_is_written_is_read_back() {
        for length in [0usize, 1, 127, 128, 255, 256, 70_000] {
            let holds = vec![0x5a; length];
            let written = write(OCTET_STRING, &holds);
            assert_eq!(whole(&written), Ok(Some(written.len())), "{length}");
            assert_eq!(Reader::new(&written).expect(OCTET_STRING), Ok(&holds[..]), "{length}");
        }
        for number in [0u32, 6, 127, 128, 255, 256, 0x7fff_ffff, 0x8009_0302, u32::MAX] {
            let written = write_number(number);
            assert_eq!(read_number(Reader::new(&written).expect(INTEGER).unwrap()), Ok(number), "{number}");
        }
        assert_eq!(write_number(6), [0x02, 0x01, 0x06]);
        assert_eq!(write_number(128), [0x02, 0x02, 0x00, 0x80]);
        assert_eq!(of_its_own(1), 0xa1);
    }

    #[test]
    fn a_message_is_whole_once_its_length_can_be_read_and_has_arrived() {
        let message = write(SEQUENCE, &[7; 300]);
        assert_eq!(whole(&message[..1]), Ok(None));
        assert_eq!(whole(&message[..3]), Ok(None), "its length is not all there yet");
        assert_eq!(whole(&message[..4]), Ok(Some(304)));
        assert!(whole(&[0x30, 0x80]).is_err());
        assert!(whole(&[0x30, 0x84, 1, 0, 0, 0]).is_err());
    }

    #[test]
    fn nothing_is_read_past_the_end() {
        for broken in [
            &[0x30][..],
            &[0x30, 0x05, 0x01],
            &[0x30, 0x80, 0x00, 0x00],
            &[0x30, 0x85, 1, 1, 1, 1, 1],
            &[0x1f, 0x01, 0x00],
            &[0x30, 0x82, 0x01],
        ] {
            assert!(Reader::new(broken).value().is_err(), "{broken:x?}");
        }
        assert!(Reader::new(&[0x02, 0x01, 0x05]).expect(SEQUENCE).is_err());
    }
}
