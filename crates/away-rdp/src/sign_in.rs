//! The sign-in before the desktop: once the connection is secured, the client signs the
//! person in before the machine shows anything (CredSSP, MS-CSSP).
//!
//! Its messages are records that carry the messages of a way of signing in, here NTLM.
//! What is built so far is the beginning: the client's first message, and the machine's
//! answer, in which it says who it is. Proving who the person is comes next.

use crate::der::{self, Reader};
use crate::ntlm::{self, Challenge};
use crate::Error;

/// The version of the sign-in that Away speaks: the one that binds the proof to the
/// connection with a number made for it.
pub const VERSION: u32 = 6;

/// The client's first message: its version, and NTLM's first message inside.
pub fn begin() -> Vec<u8> {
    request(VERSION, &ntlm::negotiate())
}

/// A message of the sign-in that carries one message of the way of signing in.
pub fn request(version: u32, carried: &[u8]) -> Vec<u8> {
    let token = der::write(der::of_its_own(0), &der::write(der::OCTET_STRING, carried));
    let tokens = der::write(der::SEQUENCE, &der::write(der::SEQUENCE, &token));
    let mut fields = der::write(der::of_its_own(0), &der::write_number(version));
    fields.extend(der::write(der::of_its_own(1), &tokens));
    der::write(der::SEQUENCE, &fields)
}

/// What a message of the sign-in holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Message {
    /// The version of the sign-in its sender speaks.
    pub version: u32,
    /// The message of the way of signing in that it carries, if it carries one.
    pub carried: Option<Vec<u8>>,
    /// The machine's own code for why it refuses, if it refuses.
    pub refused: Option<u32>,
}

/// Reads one whole message of the sign-in.
pub fn read(message: &[u8]) -> Result<Message, Error> {
    const WRONG: Error = Error::SignIn("a message of the sign-in is not built as one");
    let as_sign_in = |_| WRONG;
    let mut outer = Reader::new(message);
    let mut fields = Reader::new(outer.expect(der::SEQUENCE).map_err(as_sign_in)?);
    let mut read = Message::default();
    while !fields.is_empty() {
        let field = fields.value().map_err(as_sign_in)?;
        let mut inside = Reader::new(field.holds);
        match field.kind {
            kind if kind == der::of_its_own(0) => read.version = der::read_number(inside.expect(der::INTEGER).map_err(as_sign_in)?)?,
            kind if kind == der::of_its_own(1) => {
                let mut tokens = Reader::new(inside.expect(der::SEQUENCE).map_err(as_sign_in)?);
                let mut token = Reader::new(tokens.expect(der::SEQUENCE).map_err(as_sign_in)?);
                let mut carried = Reader::new(token.expect(der::of_its_own(0)).map_err(as_sign_in)?);
                read.carried = Some(carried.expect(der::OCTET_STRING).map_err(as_sign_in)?.to_vec());
            }
            kind if kind == der::of_its_own(4) => read.refused = Some(der::read_number(inside.expect(der::INTEGER).map_err(as_sign_in)?)?),
            _ => {}
        }
    }
    if read.version == 0 {
        return Err(WRONG);
    }
    Ok(read)
}

/// Reads the machine's answer to the first message: who it says it is.
pub fn read_challenge(message: &[u8]) -> Result<(u32, Challenge), Error> {
    let read = read(message)?;
    if let Some(code) = read.refused {
        return Err(Error::SignInRefused(code));
    }
    let carried = read.carried.ok_or(Error::SignIn("the machine's answer carries no challenge"))?;
    Ok((read.version, ntlm::read_challenge(&carried)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_message_carries_ntlms_first_and_says_its_version() {
        let message = begin();
        assert_eq!(der::whole(&message), Ok(Some(message.len())));
        // a record of two fields: the version 6, and a list of one message
        assert_eq!(&message[..7], &[0x30, 0x37, 0xa0, 0x03, 0x02, 0x01, 0x06]);
        assert_eq!(&message[7..17], &[0xa1, 0x30, 0x30, 0x2e, 0x30, 0x2c, 0xa0, 0x2a, 0x04, 0x28]);
        assert_eq!(&message[17..], &ntlm::negotiate()[..]);
        let read = read(&message).unwrap();
        assert_eq!(read, Message { version: 6, carried: Some(ntlm::negotiate()), refused: None });
    }

    #[test]
    fn the_machines_answer_says_who_it_is() {
        let answer = request(6, &ntlm::challenge_for_tests("FIN-APP-02", "CORP", "corp.example.com", 26_100));
        let (version, challenge) = read_challenge(&answer).unwrap();
        assert_eq!(version, 6);
        assert_eq!(challenge.computer.as_deref(), Some("FIN-APP-02"));
        assert_eq!(challenge.domain_dns.as_deref(), Some("corp.example.com"));
    }

    #[test]
    fn a_machine_that_refuses_says_so_with_its_own_code() {
        // a record of two fields: the version, and the code for "the logon attempt failed"
        let mut fields = der::write(der::of_its_own(0), &der::write_number(6));
        fields.extend(der::write(der::of_its_own(4), &der::write_number(0xc000_006d)));
        let refused = der::write(der::SEQUENCE, &fields);
        assert_eq!(read_challenge(&refused), Err(Error::SignInRefused(0xc000_006d)));
        assert_eq!(Error::SignInRefused(0xc000_006d).to_string(), "It refused the sign-in, with its own code 0xc000006d.");
    }

    #[test]
    fn a_message_that_is_cut_or_changed_is_refused_and_never_read_past_its_end() {
        let whole = request(6, &ntlm::challenge_for_tests("FIN-APP-02", "CORP", "corp.example.com", 26_100));
        for cut in 0..whole.len() {
            assert!(read_challenge(&whole[..cut]).is_err(), "cut at {cut}");
        }
        for at in 0..whole.len() {
            let mut changed = whole.clone();
            changed[at] ^= 0xff;
            let _ = read_challenge(&changed);
        }
        assert!(read(&der::write(der::SEQUENCE, &[])).is_err(), "a record that names no version");
    }
}
