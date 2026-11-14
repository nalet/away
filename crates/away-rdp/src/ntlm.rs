//! The messages of NTLM, the way of signing in that needs no directory and that a
//! Windows machine takes for an account of its own (MS-NLMP).
//!
//! It takes three messages: the client says what it can do, the machine answers with a
//! challenge and says who it is, and the client proves that it knows the password. What
//! is here so far is the first and the reading of the second.

use crate::Error;

/// How every message begins.
const BEGINS: &[u8; 8] = b"NTLMSSP\0";

// what the client says it can do and wants (the flags of MS-NLMP 2.2.2.5)
const UNICODE: u32 = 0x0000_0001;
const WANTS_TARGET: u32 = 0x0000_0004;
const SIGNS: u32 = 0x0000_0010;
const SEALS: u32 = 0x0000_0020;
const NTLM: u32 = 0x0000_0200;
const ALWAYS_SIGNS: u32 = 0x0000_8000;
const EXTENDED_SECURITY: u32 = 0x0008_0000;
const SAYS_VERSION: u32 = 0x0200_0000;
const KEYS_128: u32 = 0x2000_0000;
const KEY_EXCHANGE: u32 = 0x4000_0000;
const KEYS_56: u32 = 0x8000_0000;

/// What Away says it can do and wants: names in Unicode, the machine's own name, signed
/// and sealed messages with keys of 128 bits that are exchanged, and both versions.
pub const ASKED_FOR: u32 =
    UNICODE | WANTS_TARGET | SIGNS | SEALS | NTLM | ALWAYS_SIGNS | EXTENDED_SECURITY | SAYS_VERSION | KEYS_128 | KEY_EXCHANGE | KEYS_56;

/// The first message: what the client can do. It names no domain and no device.
pub fn negotiate() -> Vec<u8> {
    let mut message = Vec::with_capacity(40);
    message.extend_from_slice(BEGINS);
    message.extend_from_slice(&1u32.to_le_bytes());
    message.extend_from_slice(&ASKED_FOR.to_le_bytes());
    // no domain and no device: two empty fields, each a length twice and where it would be
    for _ in 0..2 {
        message.extend_from_slice(&[0, 0, 0, 0]);
        message.extend_from_slice(&40u32.to_le_bytes());
    }
    // the version field: no system's version, and the revision of NTLM that is current
    message.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 15]);
    message
}

/// The version of Windows a machine says it runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Version {
    /// Its major number: 10 for every Windows since 2015.
    pub major: u8,
    /// Its minor number.
    pub minor: u8,
    /// Its build, which tells one release from another.
    pub build: u16,
}

/// The machine's answer: its challenge, and who it says it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Challenge {
    /// What the machine agreed to of what was asked for.
    pub agreed: u32,
    /// The eight bytes the client's proof is made with.
    pub challenge: [u8; 8],
    /// The name the machine answers as: its domain's, or its own outside one.
    pub answers_as: String,
    /// Its own short name.
    pub computer: Option<String>,
    /// Its domain's short name. Outside a domain it is the machine's own again.
    pub domain: Option<String>,
    /// Its own full name.
    pub computer_dns: Option<String>,
    /// Its domain's full name.
    pub domain_dns: Option<String>,
    /// The machine's clock, in units of a ten-millionth of a second since 1601.
    pub clock: Option<u64>,
    /// The version of Windows it says it runs.
    pub version: Option<Version>,
    /// Everything it said of itself, as it sent it: the client's proof includes these bytes.
    pub said_of_itself: Vec<u8>,
}

/// Reads the machine's answer to the first message.
pub fn read_challenge(message: &[u8]) -> Result<Challenge, Error> {
    const WRONG: Error = Error::SignIn("the machine's challenge is not built as one");
    if message.len() < 48 || &message[..8] != BEGINS || number(message, 8)? != 2 {
        return Err(WRONG);
    }
    let agreed = number(message, 20)?;
    let challenge: [u8; 8] = message[24..32].try_into().map_err(|_| WRONG)?;
    let answers_as = utf16(field(message, 12)?);
    let said_of_itself = field(message, 40)?.to_vec();
    let version = if agreed & SAYS_VERSION != 0 && message.len() >= 56 {
        Some(Version { major: message[48], minor: message[49], build: u16::from_le_bytes([message[50], message[51]]) })
    } else {
        None
    };
    let mut read = Challenge {
        agreed,
        challenge,
        answers_as,
        computer: None,
        domain: None,
        computer_dns: None,
        domain_dns: None,
        clock: None,
        version,
        said_of_itself,
    };
    // what it says of itself: a kind, a length and a value, one after the other, to an end
    let mut rest = &read.said_of_itself[..];
    while let [kind_low, kind_high, length_low, length_high, after @ ..] = rest {
        let kind = u16::from_le_bytes([*kind_low, *kind_high]);
        let length = usize::from(u16::from_le_bytes([*length_low, *length_high]));
        if kind == 0 {
            break;
        }
        if after.len() < length {
            return Err(Error::SignIn("what the machine says of itself ends in the middle"));
        }
        let (value, after) = after.split_at(length);
        match kind {
            1 => read.computer = Some(utf16(value)),
            2 => read.domain = Some(utf16(value)),
            3 => read.computer_dns = Some(utf16(value)),
            4 => read.domain_dns = Some(utf16(value)),
            7 => read.clock = <[u8; 8]>::try_from(value).ok().map(u64::from_le_bytes),
            _ => {}
        }
        rest = after;
    }
    Ok(read)
}

/// A number of four bytes at a place in a message.
fn number(message: &[u8], at: usize) -> Result<u32, Error> {
    message
        .get(at..at + 4)
        .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
        .map(u32::from_le_bytes)
        .ok_or(Error::SignIn("a message of the sign-in is too short"))
}

/// What a field of a message points at: its length, its length again, and where it is.
fn field(message: &[u8], at: usize) -> Result<&[u8], Error> {
    let length = message.get(at..at + 2).map(|bytes| usize::from(u16::from_le_bytes([bytes[0], bytes[1]])));
    let from = number(message, at + 4)? as usize;
    length
        .and_then(|length| message.get(from..from.checked_add(length)?))
        .ok_or(Error::SignIn("a field of the sign-in points outside its message"))
}

/// A text in the encoding Windows uses, of which an odd last byte is left out.
fn utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
    String::from_utf16_lossy(&units)
}

/// A machine's challenge, built the way a Windows machine builds one. For tests, here
/// and in whatever stands in for a machine.
pub fn challenge_for_tests(computer: &str, domain: &str, dns_domain: &str, build: u16) -> Vec<u8> {
    let wide = |text: &str| text.encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<u8>>();
    let mut said = Vec::new();
    for (kind, value) in [
        (2u16, wide(domain)),
        (1, wide(computer)),
        (4, wide(dns_domain)),
        (3, wide(&format!("{computer}.{dns_domain}"))),
        (7, 134_079_408_000_000_000u64.to_le_bytes().to_vec()),
    ] {
        said.extend_from_slice(&kind.to_le_bytes());
        said.extend_from_slice(&(value.len() as u16).to_le_bytes());
        said.extend_from_slice(&value);
    }
    said.extend_from_slice(&[0, 0, 0, 0]);
    let name = wide(domain);
    let mut message = Vec::new();
    message.extend_from_slice(BEGINS);
    message.extend_from_slice(&2u32.to_le_bytes());
    message.extend_from_slice(&(name.len() as u16).to_le_bytes());
    message.extend_from_slice(&(name.len() as u16).to_le_bytes());
    message.extend_from_slice(&56u32.to_le_bytes());
    message.extend_from_slice(&ASKED_FOR.to_le_bytes());
    message.extend_from_slice(&[0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef]);
    message.extend_from_slice(&[0; 8]);
    message.extend_from_slice(&(said.len() as u16).to_le_bytes());
    message.extend_from_slice(&(said.len() as u16).to_le_bytes());
    message.extend_from_slice(&((56 + name.len()) as u32).to_le_bytes());
    message.extend_from_slice(&[10, 0]);
    message.extend_from_slice(&build.to_le_bytes());
    message.extend_from_slice(&[0, 0, 0, 15]);
    message.extend_from_slice(&name);
    message.extend_from_slice(&said);
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_message_says_what_away_can_do_and_names_nobody() {
        let message = negotiate();
        assert_eq!(message.len(), 40);
        assert_eq!(&message[..12], b"NTLMSSP\0\x01\0\0\0");
        assert_eq!(u32::from_le_bytes(message[12..16].try_into().unwrap()), 0xe208_8235);
        assert_eq!(&message[16..32], &[0, 0, 0, 0, 40, 0, 0, 0, 0, 0, 0, 0, 40, 0, 0, 0]);
        assert_eq!(&message[32..], &[0, 0, 0, 0, 0, 0, 0, 15]);
    }

    #[test]
    fn a_machines_challenge_says_who_it_is() {
        let read = read_challenge(&challenge_for_tests("FIN-APP-02", "CORP", "corp.example.com", 26_100)).unwrap();
        assert_eq!(read.challenge, [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef]);
        assert_eq!(read.answers_as, "CORP");
        assert_eq!(read.computer.as_deref(), Some("FIN-APP-02"));
        assert_eq!(read.domain.as_deref(), Some("CORP"));
        assert_eq!(read.computer_dns.as_deref(), Some("FIN-APP-02.corp.example.com"));
        assert_eq!(read.domain_dns.as_deref(), Some("corp.example.com"));
        assert_eq!(read.version, Some(Version { major: 10, minor: 0, build: 26_100 }));
        assert_eq!(read.clock, Some(134_079_408_000_000_000));
        assert_eq!(read.agreed, ASKED_FOR);
        assert!(read.said_of_itself.ends_with(&[0, 0, 0, 0]));
    }

    #[test]
    fn a_challenge_that_is_cut_or_changed_is_refused_and_never_read_past_its_end() {
        let whole = challenge_for_tests("FIN-APP-02", "CORP", "corp.example.com", 26_100);
        for cut in 0..whole.len() {
            assert!(read_challenge(&whole[..cut]).is_err(), "cut at {cut}");
        }
        for at in 0..whole.len() {
            let mut changed = whole.clone();
            changed[at] ^= 0xff;
            let _ = read_challenge(&changed);
        }
        let mut first = whole.clone();
        first[8] = 1;
        assert!(read_challenge(&first).is_err(), "a first message is no challenge");
        // a field that points past the end of the message
        let mut outside = whole.clone();
        outside[44..48].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(read_challenge(&outside), Err(Error::SignIn("a field of the sign-in points outside its message")));
    }
}
