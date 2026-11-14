//! Just enough of DER, the encoding a certificate is written in, to read one.
//!
//! A value is one byte that says what kind it is, its length, and what it holds. Only
//! lengths that are written out are read, which is all a certificate has.

use crate::Error;

/// A list of values, in order.
pub const SEQUENCE: u8 = 0x30;
/// A whole number.
pub const INTEGER: u8 = 0x02;
/// A string of bits.
pub const BIT_STRING: u8 = 0x03;
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
