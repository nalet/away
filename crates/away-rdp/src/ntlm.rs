//! The messages of NTLM, the way of signing in that needs no directory and that a
//! Windows machine takes for an account of its own (MS-NLMP).
//!
//! It takes three messages: the client says what it can do, the machine answers with a
//! challenge and says who it is, and the client proves that it knows the password. From
//! the proof both sides have a key, with which the messages after it are signed and
//! sealed.
//!
//! The password never leaves the client. What is sent is a proof made with it, good for
//! this one challenge.
//!
//! The mathematics is not this crate's: the hashes and the cipher come from libraries.
//! NTLM is old, and so are they (MD4, MD5, RC4). It is what a Windows machine takes for
//! an account of its own, and it travels inside TLS.

use hmac::{Hmac, KeyInit, Mac};
use md4::{Digest, Md4};
use md5::Md5;
use rc4::{Rc4, StreamCipher};

use crate::secret::Secret;
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

/// Who signs in: an account's name, its domain where the name doesn't hold it, and its
/// password.
#[derive(Clone, Debug)]
pub struct Account {
    /// The account's name, as the machine knows it.
    pub name: String,
    /// Its domain's short name, or the machine's own for an account of the machine.
    /// Empty where the name holds the domain, as `alex.urban@corp.example.com` does.
    pub domain: String,
    /// Its password.
    pub password: Secret,
}

impl Account {
    /// An account from the name as people write it: `CORP\alex.urban`,
    /// `alex.urban@corp.example.com`, or `alex.urban` alone.
    pub fn named(written: &str, password: Secret) -> Account {
        match written.split_once('\\') {
            Some((domain, name)) => Account { name: name.to_string(), domain: domain.to_string(), password },
            None => Account { name: written.to_string(), domain: String::new(), password },
        }
    }
}

/// What the proof needs that has to be new each time: taken from the system's random
/// bytes by whoever holds the connection.
#[derive(Clone)]
pub struct Fresh {
    /// The client's own challenge.
    pub challenge: [u8; 8],
    /// The key the messages after the proof are signed and sealed with.
    pub key: [u8; 16],
}

type Proof = Hmac<Md5>;

pub(crate) fn hmac(key: &[u8], parts: &[&[u8]]) -> [u8; 16] {
    let mut proof = <Proof as KeyInit>::new_from_slice(key).expect("a key of any length is taken");
    for part in parts {
        proof.update(part);
    }
    proof.finalize().into_bytes().into()
}

pub(crate) fn wide(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

/// The key that stands for the password in every proof: made from the password, the
/// account's name in capitals, and its domain.
pub(crate) fn password_key(account: &Account) -> [u8; 16] {
    let known_by: [u8; 16] = Md4::digest(wide(account.password.expose())).into();
    hmac(&known_by, &[&wide(&account.name.to_uppercase()), &wide(&account.domain)])
}

/// The proof for one challenge, and the key both sides then share.
struct Answer {
    /// What is sent: the proof, and everything it was made over.
    response: Vec<u8>,
    /// The key both sides have once the machine has checked the proof.
    key: [u8; 16],
}

fn answer(password_key: &[u8; 16], challenge: &[u8; 8], own_challenge: &[u8; 8], clock: u64, said_of_machine: &[u8]) -> Answer {
    // what the proof is made over: its form, the time, the client's own challenge, and
    // what the machine said of itself
    let mut over = vec![1, 1, 0, 0, 0, 0, 0, 0];
    over.extend_from_slice(&clock.to_le_bytes());
    over.extend_from_slice(own_challenge);
    over.extend_from_slice(&[0; 4]);
    over.extend_from_slice(said_of_machine);
    over.extend_from_slice(&[0; 4]);
    let proof = hmac(password_key, &[challenge, &over]);
    let key = hmac(password_key, &[&proof]);
    let mut response = proof.to_vec();
    response.extend_from_slice(&over);
    Answer { response, key }
}

/// What the machine said of itself, with what the client adds: that its message carries
/// a check over all three messages, which service it means to reach, and no binding to a
/// channel.
fn with_the_clients_own(said: &[u8], serves: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(said.len() + 64);
    let mut flags = 0u32;
    let mut rest = said;
    while let [kind_low, kind_high, length_low, length_high, after @ ..] = rest {
        let kind = u16::from_le_bytes([*kind_low, *kind_high]);
        let length = usize::from(u16::from_le_bytes([*length_low, *length_high]));
        if kind == 0 || after.len() < length {
            break;
        }
        let (value, after) = after.split_at(length);
        match kind {
            6 => flags = <[u8; 4]>::try_from(value).map(u32::from_le_bytes).unwrap_or(0),
            9 | 10 => {}
            _ => out.extend_from_slice(&rest[..4 + length]),
        }
        rest = after;
    }
    let mut add = |kind: u16, value: &[u8]| {
        out.extend_from_slice(&kind.to_le_bytes());
        out.extend_from_slice(&(value.len() as u16).to_le_bytes());
        out.extend_from_slice(value);
    };
    add(6, &(flags | 2).to_le_bytes());
    add(9, &wide(serves));
    add(10, &[0; 16]);
    add(0, &[]);
    out
}

/// The third message: the proof that the client knows the account's password, for this
/// machine's challenge. With it comes what signs and seals the messages after it.
///
/// `first` and `second` are the first two messages as they were sent and received: the
/// third carries a check over all three. `serves` names the service the client means to
/// reach, `TERMSRV/` and the machine's name. `clock` is the time now, used where the
/// machine didn't send its own.
pub fn authenticate(
    first: &[u8],
    second: &[u8],
    challenge: &Challenge,
    account: &Account,
    serves: &str,
    fresh: &Fresh,
    clock: u64,
) -> (Vec<u8>, Session) {
    let said = with_the_clients_own(&challenge.said_of_itself, serves);
    let answer = answer(&password_key(account), &challenge.challenge, &fresh.challenge, challenge.clock.unwrap_or(clock), &said);
    // the key for what follows: a new one, sent sealed with the key the proof gave, where
    // the machine agreed to that
    let (key, sent_key) = if challenge.agreed & KEY_EXCHANGE != 0 {
        let mut sealed = fresh.key;
        Rc4::new_from_slice(&answer.key).expect("a key of 16 bytes").apply_keystream(&mut sealed);
        (fresh.key, sealed.to_vec())
    } else {
        (answer.key, Vec::new())
    };

    let agreed = challenge.agreed & ASKED_FOR;
    let domain = wide(&account.domain);
    let name = wide(&account.name);
    // the older form of the proof is left empty, as it is whenever the machine sent its clock
    let older = [0u8; 24];
    let fields: [&[u8]; 6] = [&older, &answer.response, &domain, &name, &[], &sent_key];
    let mut message = Vec::with_capacity(88 + fields.iter().map(|field| field.len()).sum::<usize>());
    message.extend_from_slice(BEGINS);
    message.extend_from_slice(&3u32.to_le_bytes());
    let mut at = 88u32;
    for field in fields {
        message.extend_from_slice(&(field.len() as u16).to_le_bytes());
        message.extend_from_slice(&(field.len() as u16).to_le_bytes());
        message.extend_from_slice(&at.to_le_bytes());
        at += field.len() as u32;
    }
    message.extend_from_slice(&agreed.to_le_bytes());
    message.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 15]);
    message.extend_from_slice(&[0; 16]);
    for field in fields {
        message.extend_from_slice(field);
    }
    // the check over all three messages, made with the new key
    let check = hmac(&key, &[first, second, &message]);
    message[72..88].copy_from_slice(&check);
    (message, Session::new(&key))
}

/// What signs and seals the messages after the proof, in both directions.
pub struct Session {
    to_machine: Rc4,
    from_machine: Rc4,
    signs: [u8; 16],
    machine_signs: [u8; 16],
    sent: u32,
    received: u32,
}

impl Session {
    /// The machine's side of a session: what the client seals it opens, and the reverse.
    #[cfg(any(test, feature = "stand-in"))]
    pub(crate) fn as_the_machine(key: &[u8; 16]) -> Session {
        let client = Session::new(key);
        Session {
            to_machine: client.from_machine,
            from_machine: client.to_machine,
            signs: client.machine_signs,
            machine_signs: client.signs,
            sent: 0,
            received: 0,
        }
    }

    /// The keys of both directions, made from the one key both sides share.
    pub(crate) fn new(key: &[u8; 16]) -> Session {
        let made = |what: &str| -> [u8; 16] {
            let mut hash = Md5::new();
            hash.update(key);
            hash.update(what.as_bytes());
            hash.update([0]);
            hash.finalize().into()
        };
        Session {
            to_machine: Rc4::new_from_slice(&made("session key to client-to-server sealing key magic constant"))
                .expect("a key of 16 bytes"),
            from_machine: Rc4::new_from_slice(&made("session key to server-to-client sealing key magic constant"))
                .expect("a key of 16 bytes"),
            signs: made("session key to client-to-server signing key magic constant"),
            machine_signs: made("session key to server-to-client signing key magic constant"),
            sent: 0,
            received: 0,
        }
    }

    /// Seals a message for the machine: its signature, then the message, unreadable.
    pub fn seal(&mut self, message: &[u8]) -> Vec<u8> {
        let mut sealed = message.to_vec();
        self.to_machine.apply_keystream(&mut sealed);
        let mut check = [0u8; 8];
        check.copy_from_slice(&hmac(&self.signs, &[&self.sent.to_le_bytes(), message])[..8]);
        self.to_machine.apply_keystream(&mut check);
        let mut out = Vec::with_capacity(16 + sealed.len());
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&check);
        out.extend_from_slice(&self.sent.to_le_bytes());
        out.extend_from_slice(&sealed);
        self.sent += 1;
        out
    }

    /// Opens a message the machine sealed, and refuses one whose signature is not the
    /// machine's or that is not the next in order.
    pub fn open(&mut self, sealed: &[u8]) -> Result<Vec<u8>, Error> {
        const WRONG: Error = Error::SignIn("a sealed message is not signed by the machine that was challenged");
        let [1, 0, 0, 0, check @ .., c0, c1, c2, c3] = sealed.get(..16).ok_or(WRONG)? else {
            return Err(WRONG);
        };
        if u32::from_le_bytes([*c0, *c1, *c2, *c3]) != self.received {
            return Err(WRONG);
        }
        let mut message = sealed[16..].to_vec();
        self.from_machine.apply_keystream(&mut message);
        let mut check: [u8; 8] = check.try_into().map_err(|_| WRONG)?;
        self.from_machine.apply_keystream(&mut check);
        if check != hmac(&self.machine_signs, &[&self.received.to_le_bytes(), &message])[..8] {
            return Err(WRONG);
        }
        self.received += 1;
        Ok(message)
    }
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
pub(crate) fn field(message: &[u8], at: usize) -> Result<&[u8], Error> {
    let length = message.get(at..at + 2).map(|bytes| usize::from(u16::from_le_bytes([bytes[0], bytes[1]])));
    let from = number(message, at + 4)? as usize;
    length
        .and_then(|length| message.get(from..from.checked_add(length)?))
        .ok_or(Error::SignIn("a field of the sign-in points outside its message"))
}

/// A text in the encoding Windows uses, of which an odd last byte is left out.
pub(crate) fn utf16(bytes: &[u8]) -> String {
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

    // the worked example of the published description of NTLM (MS-NLMP 4.2.4): the
    // account User of the domain Domain with the password Password, at the machine Server
    fn example() -> (Account, [u8; 8], [u8; 8], Vec<u8>) {
        let account = Account::named("Domain\\User", Secret::new("Password"));
        let said = [
            0x02, 0x00, 0x0c, 0x00, b'D', 0, b'o', 0, b'm', 0, b'a', 0, b'i', 0, b'n', 0, // its domain
            0x01, 0x00, 0x0c, 0x00, b'S', 0, b'e', 0, b'r', 0, b'v', 0, b'e', 0, b'r', 0, // its own name
            0x00, 0x00, 0x00, 0x00,
        ];
        (account, [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef], [0xaa; 8], said.to_vec())
    }

    #[test]
    fn the_password_becomes_the_key_the_published_example_gives() {
        let (account, _, _, _) = example();
        assert_eq!(account.name, "User");
        assert_eq!(account.domain, "Domain");
        assert_eq!(
            password_key(&account),
            [0x0c, 0x86, 0x8a, 0x40, 0x3b, 0xfd, 0x7a, 0x93, 0xa3, 0x00, 0x1e, 0xf2, 0x2e, 0xf0, 0x2e, 0x3f]
        );
    }

    #[test]
    fn the_proof_and_the_shared_key_are_the_published_examples() {
        let (account, challenge, own, said) = example();
        let answer = answer(&password_key(&account), &challenge, &own, 0, &said);
        assert_eq!(
            &answer.response[..16],
            &[0x68, 0xcd, 0x0a, 0xb8, 0x51, 0xe5, 0x1c, 0x96, 0xaa, 0xbc, 0x92, 0x7b, 0xeb, 0xef, 0x6a, 0x1c]
        );
        assert_eq!(answer.key, [0x8d, 0xe4, 0x0c, 0xca, 0xdb, 0xc1, 0x4a, 0x82, 0xf1, 0x5c, 0xb0, 0xad, 0x0d, 0xe9, 0x5c, 0xa3]);
        // the new key, 16 times 0x55 in the example, as it is sent
        let mut sent = [0x55u8; 16];
        Rc4::new_from_slice(&answer.key).unwrap().apply_keystream(&mut sent);
        assert_eq!(sent, [0xc5, 0xda, 0xd2, 0x54, 0x4f, 0xc9, 0x79, 0x90, 0x94, 0xce, 0x1c, 0xe9, 0x0b, 0xc9, 0xd0, 0x3e]);
    }

    #[test]
    fn a_message_is_sealed_as_in_the_published_example() {
        let mut session = Session::new(&[0x55; 16]);
        assert_eq!(session.signs, [0x47, 0x88, 0xdc, 0x86, 0x1b, 0x47, 0x82, 0xf3, 0x5d, 0x43, 0xfd, 0x98, 0xfe, 0x1a, 0x2d, 0x39]);
        let sealed = session.seal(&wide("Plaintext"));
        assert_eq!(&sealed[..16], &[0x01, 0x00, 0x00, 0x00, 0x7f, 0xb3, 0x8e, 0xc5, 0xc5, 0x5d, 0x49, 0x76, 0x00, 0x00, 0x00, 0x00]);
        assert_eq!(
            &sealed[16..],
            &[0x54, 0xe5, 0x01, 0x65, 0xbf, 0x19, 0x36, 0xdc, 0x99, 0x60, 0x20, 0xc1, 0x81, 0x1b, 0x0f, 0x06, 0xfb, 0x5f]
        );
    }

    #[test]
    fn what_one_side_seals_the_other_opens_in_order_and_nothing_else() {
        let mut client = Session::new(&[7; 16]);
        let mut machine = Session::as_the_machine(&[7; 16]);
        let first = machine.seal(b"the first");
        let second = machine.seal(b"the second");
        // out of order, changed, cut: refused
        assert!(Session::new(&[7; 16]).open(&second).is_err());
        let mut changed = first.clone();
        *changed.last_mut().unwrap() ^= 1;
        assert!(Session::new(&[7; 16]).open(&changed).is_err());
        assert!(Session::new(&[7; 16]).open(&first[..10]).is_err());
        assert!(Session::new(&[8; 16]).open(&first).is_err(), "another key");
        // in order: opened
        assert_eq!(client.open(&first).unwrap(), b"the first");
        assert_eq!(client.open(&second).unwrap(), b"the second");
        assert_eq!(machine.open(&client.seal(b"an answer")).unwrap(), b"an answer");
    }

    #[test]
    fn the_third_message_holds_the_proof_the_names_and_a_check_over_all_three() {
        let second = challenge_for_tests("FIN-APP-02", "CORP", "corp.example.com", 26_100);
        let challenge = read_challenge(&second).unwrap();
        let account = Account::named("CORP\\alex.urban", Secret::new("a password"));
        let fresh = Fresh { challenge: [0xaa; 8], key: [0x55; 16] };
        let first = negotiate();
        let (third, _) = authenticate(&first, &second, &challenge, &account, "TERMSRV/fin-app-02.corp.example.com", &fresh, 0);
        assert_eq!(&third[..12], b"NTLMSSP\0\x03\0\0\0");
        assert_eq!(u32::from_le_bytes(third[60..64].try_into().unwrap()), ASKED_FOR);
        let field = |at: usize| field(&third, at).unwrap().to_vec();
        assert_eq!(field(12), [0; 24], "the older form of the proof is empty");
        assert_eq!(field(28), wide("CORP"));
        assert_eq!(field(36), wide("alex.urban"));
        assert_eq!(field(52).len(), 16, "the new key, sealed");
        // the proof is made over what the machine said of itself and what the client adds
        let proof = field(20);
        let said = &proof[16 + 28..proof.len() - 4];
        assert!(said.starts_with(&challenge.said_of_itself[..challenge.said_of_itself.len() - 4]));
        let added = &said[challenge.said_of_itself.len() - 4..];
        assert_eq!(&added[..8], &[6, 0, 4, 0, 2, 0, 0, 0], "its message carries a check");
        assert_eq!(&added[8..12], &[9, 0, 70, 0], "the service it means to reach");
        assert!(added.ends_with(&[10, 0, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]));
        assert_eq!(&proof[16 + 8..16 + 16], &134_079_408_000_000_000u64.to_le_bytes(), "the machine's own clock");
        // the check covers all three messages, and is made with the new key
        let mut unchecked = third.clone();
        unchecked[72..88].fill(0);
        assert_eq!(&third[72..88], &hmac(&[0x55; 16], &[&first, &second, &unchecked]));
        // the password is nowhere in it
        assert!(!third.windows(wide("a password").len()).any(|part| part == wide("a password")));
    }

    #[test]
    fn a_name_is_taken_as_people_write_it() {
        let named = |written| Account::named(written, Secret::new(""));
        assert_eq!((named("CORP\\alex.urban").domain.as_str(), named("CORP\\alex.urban").name.as_str()), ("CORP", "alex.urban"));
        assert_eq!(
            (named("alex.urban@corp.example.com").domain.as_str(), named("alex.urban@corp.example.com").name.as_str()),
            ("", "alex.urban@corp.example.com")
        );
        assert_eq!(named("alex.urban").domain, "");
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
