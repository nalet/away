//! The machine's question for its password, and the answer (RFC 6143, 7.2.2): the machine
//! gives sixteen bytes it made up, and the client gives them back enciphered with the
//! password. The password itself doesn't travel.
//!
//! The cipher is DES, with the password's first eight bytes as the key, each byte's bits
//! in the reverse order: that is how the protocol is written, and what every machine
//! expects. A longer password is cut by the machine too.

use away_secret::Secret;
use des::cipher::{BlockCipherEncrypt, KeyInit};

/// The answer to the machine's question, with this password.
pub fn answer(asked: &[u8; 16], password: &Secret) -> [u8; 16] {
    let mut key = [0u8; 8];
    for (to, from) in key.iter_mut().zip(password.expose().bytes()) {
        *to = from.reverse_bits();
    }
    let cipher = des::Des::new_from_slice(&key).expect("a key of eight bytes");
    let mut answer = *asked;
    for block in answer.as_chunks_mut::<8>().0 {
        cipher.encrypt_block(block.into());
    }
    answer
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_answer_is_what_other_programs_that_speak_vnc_give() {
        // a password, a question, and the answer worked out with another program's DES:
        // the key is the password's bytes, each with its bits in the reverse order
        let asked = [0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f];
        let given = answer(&asked, &Secret::new("password"));
        assert_eq!(given, [0xb8, 0x66, 0x92, 0x41, 0x25, 0xc8, 0xee, 0xbb, 0x9d, 0xeb, 0xc1, 0xdb, 0x61, 0xc5, 0x38, 0xe2]);
        // the password is cut after eight bytes, and a shorter one is filled up with nothing
        assert_eq!(answer(&asked, &Secret::new("password and more")), given);
        assert_ne!(answer(&asked, &Secret::new("passwor")), given);
        // both halves are enciphered alike: the same eight bytes give the same answer
        let twice = answer(&[7; 16], &Secret::new("password"));
        assert_eq!(twice[..8], twice[8..]);
    }
}
