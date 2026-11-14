//! Bytes as letters (RFC 4648): the ordinary way, with its padding, for what goes into
//! a header; and the way made for addresses, without padding, which signed tokens use.
//!
//! Reading is strict: a letter of the other alphabet, padding where none belongs, or
//! bits left over are refused.

const ORDINARY: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const FOR_ADDRESSES: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn write(bytes: &[u8], letters: &[u8; 64], padded: bool) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for three in bytes.chunks(3) {
        let held = [three[0], *three.get(1).unwrap_or(&0), *three.get(2).unwrap_or(&0)];
        let four = [held[0] >> 2, ((held[0] & 0x03) << 4) | (held[1] >> 4), ((held[1] & 0x0f) << 2) | (held[2] >> 6), held[2] & 0x3f];
        for (nth, six) in four.iter().enumerate() {
            if nth <= three.len() {
                out.push(letters[usize::from(*six)] as char);
            } else if padded {
                out.push('=');
            }
        }
    }
    out
}

/// Bytes as letters, the ordinary way, with padding.
pub fn ordinary(bytes: &[u8]) -> String {
    write(bytes, ORDINARY, true)
}

/// Bytes as letters that an address can hold, without padding.
pub fn for_addresses(bytes: &[u8]) -> String {
    write(bytes, FOR_ADDRESSES, false)
}

/// The bytes that letters of the kind made for addresses stand for. Gives nothing where
/// they are not such letters, are padded, or leave bits over.
pub fn read_for_addresses(letters: &str) -> Option<Vec<u8>> {
    let letters = letters.as_bytes();
    if letters.len() % 4 == 1 {
        return None;
    }
    let mut bytes = Vec::with_capacity(letters.len() / 4 * 3 + 2);
    for four in letters.chunks(4) {
        let mut held = 0u32;
        for letter in four {
            let six = match letter {
                b'A'..=b'Z' => letter - b'A',
                b'a'..=b'z' => letter - b'a' + 26,
                b'0'..=b'9' => letter - b'0' + 52,
                b'-' => 62,
                b'_' => 63,
                _ => return None,
            };
            held = (held << 6) | u32::from(six);
        }
        match four.len() {
            4 => bytes.extend_from_slice(&held.to_be_bytes()[1..]),
            // the bits that belong to no byte have to be nothing
            3 if held & 0x3 == 0 => bytes.extend_from_slice(&(held >> 2).to_be_bytes()[2..]),
            2 if held & 0xf == 0 => bytes.push((held >> 4) as u8),
            _ => return None,
        }
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_examples_of_the_standard() {
        for (bytes, letters) in
            [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")]
        {
            assert_eq!(ordinary(bytes.as_bytes()), letters);
            let unpadded = letters.trim_end_matches('=');
            assert_eq!(for_addresses(bytes.as_bytes()), unpadded);
            assert_eq!(read_for_addresses(unpadded).as_deref(), Some(bytes.as_bytes()));
        }
    }

    #[test]
    fn the_two_alphabets_differ_in_two_letters() {
        assert_eq!(ordinary(&[0xfb, 0xff, 0xfe]), "+//+");
        assert_eq!(for_addresses(&[0xfb, 0xff, 0xfe]), "-__-");
        assert_eq!(read_for_addresses("-__-"), Some(vec![0xfb, 0xff, 0xfe]));
        assert_eq!(read_for_addresses("+//+"), None);
    }

    #[test]
    fn every_length_comes_back_as_it_went() {
        let bytes: Vec<u8> = (0..=255).collect();
        for length in 0..bytes.len() {
            assert_eq!(read_for_addresses(&for_addresses(&bytes[..length])).as_deref(), Some(&bytes[..length]));
        }
    }

    #[test]
    fn what_is_padded_cut_or_has_bits_left_over_is_refused() {
        for wrong in ["Zg==", "Zm8=", "Z", "Zm9vY", "Zh", "Zm9", "Zm 9v", "Zm9v\n", "Zm9v=", "Zö"] {
            assert_eq!(read_for_addresses(wrong), None, "{wrong}");
        }
    }
}
