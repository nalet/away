//! The encoding that Kerberos, certificates and what is signed with them are written in:
//! a value is one byte that says what kind it is, its length, and what it holds. Only
//! what those three use of it is here.

use crate::Error;

/// A list of values, in order.
pub const SEQUENCE: u8 = 0x30;
/// A set of values.
pub const SET: u8 = 0x31;
/// A whole number.
pub const INTEGER: u8 = 0x02;
/// A string of bits.
pub const BIT_STRING: u8 = 0x03;
/// A string of bytes.
pub const OCTET_STRING: u8 = 0x04;
/// Nothing.
pub const NULL: u8 = 0x05;
/// The name of a kind of thing, as numbers.
pub const OBJECT_NAME: u8 = 0x06;
/// A text in UTF-8.
pub const UTF8_STRING: u8 = 0x0c;
/// A text as Kerberos writes its names.
pub const GENERAL_STRING: u8 = 0x1b;
/// A time with a year of four digits.
pub const GENERALIZED_TIME: u8 = 0x18;
/// Yes or no.
pub const BOOLEAN: u8 = 0x01;

/// The kind of the value a record numbers itself, which wraps the value it is.
pub const fn numbered(number: u8) -> u8 {
    0xa0 | number
}

/// The kind of the value a record numbers itself, where the number takes the place of
/// the value's own kind and the value holds no further values.
pub const fn numbered_plain(number: u8) -> u8 {
    0x80 | number
}

/// The kind of a message of Kerberos, which each have a number of their own.
pub const fn message(number: u8) -> u8 {
    0x60 | number
}

const WRONG: Error = Error::NotKerberos("a value in it is not written as values are");

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

/// A list of values, each written out already.
pub fn sequence(values: &[Vec<u8>]) -> Vec<u8> {
    write(SEQUENCE, &values.concat())
}

/// A value under the number a record gives it.
pub fn under(number: u8, value: &[u8]) -> Vec<u8> {
    write(numbered(number), value)
}

/// A whole number.
pub fn number(number: i64) -> Vec<u8> {
    let bytes = number.to_be_bytes();
    // as few bytes as say it: no byte that only repeats the sign
    let mut from = 0;
    while from < 7 && ((bytes[from] == 0 && bytes[from + 1] & 0x80 == 0) || (bytes[from] == 0xff && bytes[from + 1] & 0x80 != 0)) {
        from += 1;
    }
    write(INTEGER, &bytes[from..])
}

/// A whole number that is not negative, from its bytes with the first the largest: a
/// number too large for any other way of holding it.
pub fn large_number(bytes: &[u8]) -> Vec<u8> {
    let from = bytes.iter().position(|byte| *byte != 0).unwrap_or(bytes.len().saturating_sub(1));
    let mut holds = bytes[from..].to_vec();
    if holds.first().is_none_or(|first| first & 0x80 != 0) {
        holds.insert(0, 0);
    }
    write(INTEGER, &holds)
}

/// A string of bytes.
pub fn bytes(holds: &[u8]) -> Vec<u8> {
    write(OCTET_STRING, holds)
}

/// A string of bits that are whole bytes.
pub fn bits(holds: &[u8]) -> Vec<u8> {
    let mut with_none_unused = vec![0];
    with_none_unused.extend_from_slice(holds);
    write(BIT_STRING, &with_none_unused)
}

/// A text as Kerberos writes its names.
pub fn text(text: &str) -> Vec<u8> {
    write(GENERAL_STRING, text.as_bytes())
}

/// The name of a kind of thing, from its numbers.
pub fn name(numbers: &[u32]) -> Vec<u8> {
    let mut holds = Vec::new();
    if let [first, second, rest @ ..] = numbers {
        holds.push((first * 40 + second) as u8);
        for number in rest {
            // seven bits a byte, the largest first, every byte but the last marked
            let mut pieces = vec![(number & 0x7f) as u8];
            let mut left = number >> 7;
            while left != 0 {
                pieces.push((left & 0x7f) as u8 | 0x80);
                left >>= 7;
            }
            holds.extend(pieces.iter().rev());
        }
    }
    write(OBJECT_NAME, &holds)
}

/// A time: seconds since the beginning of 1970, written as the year, the month, the
/// day, the hour, the minute and the second, in the time of Greenwich.
pub fn time(seconds: u64) -> Vec<u8> {
    let (days, of_day) = (seconds / 86_400, seconds % 86_400);
    // the calendar, counted from the first of March of the year 0
    let shifted = days + 719_468;
    let era = shifted / 146_097;
    let day_of_era = shifted % 146_097;
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 { month_from_march + 3 } else { month_from_march - 9 };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    let written = format!("{year:04}{month:02}{day:02}{:02}{:02}{:02}Z", of_day / 3600, of_day % 3600 / 60, of_day % 60);
    write(GENERALIZED_TIME, written.as_bytes())
}

/// The seconds since the beginning of 1970 that a time says.
pub fn read_time(holds: &[u8]) -> Result<u64, Error> {
    let digits = |from: usize, count: usize| -> Result<u64, Error> {
        let written = holds.get(from..from + count).ok_or(WRONG)?;
        written.iter().try_fold(0u64, |number, digit| digit.is_ascii_digit().then(|| number * 10 + u64::from(digit - b'0')).ok_or(WRONG))
    };
    let (year, month, day) = (digits(0, 4)?, digits(4, 2)?, digits(6, 2)?);
    let (hour, minute, second) = (digits(8, 2)?, digits(10, 2)?, digits(12, 2)?);
    if !(1970..=9999).contains(&year) || !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 60 {
        return Err(WRONG);
    }
    // a fraction of a second may follow, and then the letter that says Greenwich
    if holds.last() != Some(&b'Z') {
        return Err(WRONG);
    }
    let year = if month <= 2 { year - 1 } else { year };
    let era = year / 400;
    let year_of_era = year % 400;
    let month_from_march = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * month_from_march + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    Ok(days * 86_400 + hour * 3600 + minute * 60 + second)
}

/// One value: what kind it is, what it holds, and all of it as it was written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Value<'a> {
    /// What kind of value it is.
    pub kind: u8,
    /// What it holds.
    pub holds: &'a [u8],
    /// The value as it was written, with its kind and its length.
    pub whole: &'a [u8],
}

impl<'a> Value<'a> {
    /// A reader of the values this one holds.
    pub fn inside(&self) -> Reader<'a> {
        Reader::new(self.holds)
    }

    /// The whole number it holds, when it is one that fits.
    pub fn number(&self) -> Result<i64, Error> {
        if self.kind != INTEGER || self.holds.is_empty() || self.holds.len() > 8 {
            return Err(WRONG);
        }
        let negative = self.holds[0] & 0x80 != 0;
        Ok(self.holds.iter().fold(if negative { -1i64 } else { 0 }, |number, byte| (number << 8) | i64::from(*byte)))
    }

    /// The text it holds.
    pub fn text(&self) -> Result<&'a str, Error> {
        std::str::from_utf8(self.holds).map_err(|_| WRONG)
    }
}

/// Reads values one after the other.
#[derive(Clone)]
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

    /// The kind of the next value, where there is one.
    pub fn next_kind(&self) -> Option<u8> {
        self.rest.first().copied()
    }

    /// The next value.
    pub fn value(&mut self) -> Result<Value<'a>, Error> {
        let before = self.rest;
        let [kind, first, rest @ ..] = self.rest else { return Err(WRONG) };
        // a kind that takes more than one byte is used by none of the three
        if kind & 0x1f == 0x1f {
            return Err(WRONG);
        }
        let (length, rest) = if first & 0x80 == 0 {
            (usize::from(*first), rest)
        } else {
            let count = usize::from(first & 0x7f);
            if count == 0 || count > 4 || rest.len() < count {
                return Err(WRONG);
            }
            (rest[..count].iter().fold(0usize, |length, byte| (length << 8) | usize::from(*byte)), &rest[count..])
        };
        if rest.len() < length {
            return Err(WRONG);
        }
        let (holds, rest) = rest.split_at(length);
        self.rest = rest;
        Ok(Value { kind: *kind, holds, whole: &before[..before.len() - rest.len()] })
    }

    /// The next value, when it is of this kind.
    pub fn expect(&mut self, kind: u8) -> Result<Value<'a>, Error> {
        let value = self.value()?;
        if value.kind == kind {
            Ok(value)
        } else {
            Err(WRONG)
        }
    }

    /// The next value when it is of this kind, and nothing when another follows or none.
    pub fn optional(&mut self, kind: u8) -> Result<Option<Value<'a>>, Error> {
        match self.next_kind() {
            Some(next) if next == kind => self.value().map(Some),
            _ => Ok(None),
        }
    }

    /// The value under this number of a record, when it is the next: the one value that
    /// the number wraps.
    pub fn under(&mut self, number: u8) -> Result<Option<Value<'a>>, Error> {
        match self.optional(numbered(number))? {
            Some(wrapped) => {
                let mut inside = wrapped.inside();
                let value = inside.value()?;
                if inside.is_empty() {
                    Ok(Some(value))
                } else {
                    Err(WRONG)
                }
            }
            None => Ok(None),
        }
    }

    /// The same, where the record always has it.
    pub fn needed(&mut self, number: u8) -> Result<Value<'a>, Error> {
        self.under(number)?.ok_or(WRONG)
    }

    /// Passes over the values under numbers that come before this one.
    pub fn skip_to(&mut self, number: u8) -> Result<(), Error> {
        while let Some(kind) = self.next_kind() {
            if kind & 0xe0 != 0xa0 || kind & 0x1f >= number {
                break;
            }
            self.value()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_written_as_the_encoding_writes_them() {
        assert_eq!(write(OCTET_STRING, &[1, 2]), [0x04, 2, 1, 2]);
        assert_eq!(&write(OCTET_STRING, &[7; 200])[..3], &[0x04, 0x81, 200]);
        assert_eq!(&write(OCTET_STRING, &[7; 300])[..4], &[0x04, 0x82, 1, 44]);
        assert_eq!(sequence(&[number(5), bytes(&[9])]), [0x30, 6, 0x02, 1, 5, 0x04, 1, 9]);
        assert_eq!(under(3, &number(5)), [0xa3, 3, 0x02, 1, 5]);
        assert_eq!((numbered_plain(1), message(10)), (0x81, 0x6a));
        // a number takes as few bytes as say it, with its sign
        for (given, written) in [
            (0i64, &[0x02, 1, 0][..]),
            (127, &[0x02, 1, 0x7f]),
            (128, &[0x02, 2, 0, 0x80]),
            (-1, &[0x02, 1, 0xff]),
            (-128, &[0x02, 1, 0x80]),
            (-129, &[0x02, 2, 0xff, 0x7f]),
            (0x1234_5678, &[0x02, 4, 0x12, 0x34, 0x56, 0x78]),
            (i64::from(u32::MAX), &[0x02, 5, 0, 0xff, 0xff, 0xff, 0xff]),
        ] {
            assert_eq!(number(given), written, "{given}");
            assert_eq!(Reader::new(written).value().unwrap().number(), Ok(given));
        }
        assert_eq!(large_number(&[0, 0, 0x80, 1]), [0x02, 3, 0, 0x80, 1]);
        assert_eq!(large_number(&[0x7f, 1]), [0x02, 2, 0x7f, 1]);
        assert_eq!(large_number(&[0, 0]), [0x02, 1, 0]);
        assert_eq!(bits(&[0xa0]), [0x03, 2, 0, 0xa0]);
        assert_eq!(text("krbtgt"), [0x1b, 6, b'k', b'r', b'b', b't', b'g', b't']);
        // the name of the way to sign in with a certificate, and one with a large number in it
        assert_eq!(name(&[1, 3, 6, 1, 5, 2, 3, 1]), [0x06, 7, 0x2b, 6, 1, 5, 2, 3, 1]);
        assert_eq!(name(&[1, 2, 840, 113549, 1, 1, 11]), [0x06, 9, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 1, 1, 11]);
    }

    #[test]
    fn a_time_is_written_and_read_as_the_calendar_has_it() {
        for (seconds, written) in [
            (0u64, "19700101000000Z"),
            (951_782_400, "20000229000000Z"),
            (1_794_700_800, "20261115000000Z"),
            (1_794_787_199, "20261115235959Z"),
            (4_102_444_800, "21000101000000Z"),
        ] {
            assert_eq!(time(seconds), write(GENERALIZED_TIME, written.as_bytes()), "{written}");
            assert_eq!(read_time(written.as_bytes()), Ok(seconds), "{written}");
        }
        for wrong in ["", "2026111500000", "20261115000000", "20261315000000Z", "2026111500000xZ", "19691231235959Z"] {
            assert!(read_time(wrong.as_bytes()).is_err(), "{wrong}");
        }
    }

    #[test]
    fn values_are_read_one_after_the_other_and_under_their_numbers() {
        let record = sequence(&[under(0, &number(5)), under(2, &text("a")), under(5, &bytes(&[1, 2, 3]))]);
        let mut outside = Reader::new(&record);
        let list = outside.expect(SEQUENCE).unwrap();
        assert!(outside.is_empty());
        assert_eq!(list.whole, &record[..]);
        let mut inside = list.inside();
        assert_eq!(inside.needed(0).unwrap().number(), Ok(5));
        // one that is not there, one that is, and one after others that are passed over
        assert_eq!(inside.under(1), Ok(None));
        assert_eq!(inside.clone().under(2).unwrap().unwrap().text(), Ok("a"));
        inside.skip_to(5).unwrap();
        assert_eq!(inside.needed(5).unwrap().holds, [1, 2, 3]);
        assert!(inside.is_empty() && inside.needed(6).is_err());

        // cut anywhere, a value is refused and never read past its end
        for cut in 1..record.len() {
            let mut reader = Reader::new(&record[..cut]);
            assert!(reader.value().is_err(), "cut at {cut}");
        }
        // a value under a number wraps exactly one value
        let two = under(0, &[number(1), number(2)].concat());
        assert!(Reader::new(&two).under(0).is_err());
        assert!(Reader::new(&[0x1f, 1, 0]).value().is_err());
        assert!(Reader::new(&[0x04, 0x85, 0, 0, 0, 0, 1, 0]).value().is_err());
    }
}
