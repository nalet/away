//! The licence that a machine for many people hands every client (MS-RDPELE).
//!
//! A Windows machine that serves desktops or programs to many people asks every client
//! to take a licence before it shows anything. The exchange has three steps: the machine
//! asks and shows a key; the client answers with a secret sealed to that key, from which
//! both sides make the keys of the exchange; the machine sends a challenge, the client
//! answers it, and the machine hands over the licence.
//!
//! The steps are the protocol's own, from the time before TLS: a secret sealed with RSA
//! as it is, a cipher and two hashes that are used for nothing else today. They protect
//! nothing here that the secured connection doesn't protect already. The arithmetic, the
//! cipher and the hashes are libraries', as everywhere in Away.

use md5::{Digest, Md5};
use num_bigint::BigUint;
use rc4::{KeyInit, Rc4, StreamCipher};
use sha1::Sha1;

use crate::der::{self, Reader};
use crate::Error;

const WRONG: Error = Error::NotRdp("the machine's word on a licence is not built as one");

/// The client's platform, as every client says it that is of neither of the two makers
/// the protocol names: a Windows of the newer line.
const PLATFORM: u32 = 0x0401_0000;

/// What a machine says about licences.
#[derive(Debug, PartialEq, Eq)]
pub enum Said {
    /// The client is taken as it is: there is nothing to take.
    Valid,
    /// The machine asks the client to take a licence.
    Asks(Asked),
    /// The machine challenges the client, sealed with the keys of the exchange.
    Challenges(Vec<u8>, [u8; 16]),
    /// The machine hands over the licence, sealed with the keys of the exchange.
    Hands(Vec<u8>, [u8; 16]),
    /// The machine refuses the client, with its own number for why.
    Refuses(u32),
}

/// What a machine shows when it asks the client to take a licence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asked {
    /// The machine's random bytes for this exchange.
    pub random: [u8; 32],
    /// Who made what the licence is for.
    pub company: String,
    /// What the licence is for.
    pub product: String,
    /// The key the client seals its secret to: its modulus and its exponent, highest
    /// byte first.
    pub modulus: Vec<u8>,
    /// The key's exponent.
    pub exponent: Vec<u8>,
}

/// What an exchange needs that has to be new each time, and what the client is known by:
/// taken from the system's random bytes, and from the installation, by whoever holds the
/// connection.
#[derive(Clone)]
pub struct New {
    /// The client's random bytes for this exchange.
    pub random: [u8; 32],
    /// The secret the keys of the exchange are made from.
    pub secret: [u8; 48],
    /// What the client is known by to whoever hands out licences: the same for every
    /// connection of one installation, so that it is handed the same licence again.
    pub known_by: [u8; 16],
}

/// A licence that was taken.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Taken {
    /// Who made what the licence is for.
    pub company: String,
    /// What the licence is for.
    pub product: String,
    /// Whose licences these are.
    pub scope: String,
    /// The licence itself, as the machine wrote it.
    pub licence: Vec<u8>,
}

fn number16(bytes: &[u8], at: usize) -> Result<usize, Error> {
    bytes.get(at..at + 2).map(|pair| usize::from(u16::from_le_bytes([pair[0], pair[1]]))).ok_or(WRONG)
}

fn number32(bytes: &[u8], at: usize) -> Result<usize, Error> {
    bytes.get(at..at + 4).map(|four| u32::from_le_bytes([four[0], four[1], four[2], four[3]]) as usize).ok_or(WRONG)
}

/// A piece with its kind and its length before it. Gives the piece, and what follows it.
fn piece(bytes: &[u8]) -> Result<(&[u8], &[u8]), Error> {
    let length = number16(bytes, 2)?;
    let holds = bytes.get(4..4 + length).ok_or(WRONG)?;
    Ok((holds, &bytes[4 + length..]))
}

fn write_piece(kind: u16, holds: &[u8]) -> Vec<u8> {
    let mut piece = Vec::with_capacity(holds.len() + 4);
    piece.extend(kind.to_le_bytes());
    piece.extend((holds.len() as u16).to_le_bytes());
    piece.extend_from_slice(holds);
    piece
}

/// A message of the exchange: that it is about a licence, its kind, its length.
pub(crate) fn message(kind: u8, holds: &[u8]) -> Vec<u8> {
    let mut message = vec![0x80, 0, 0, 0, kind, 0x03];
    message.extend(((holds.len() + 4) as u16).to_le_bytes());
    message.extend_from_slice(holds);
    message
}

fn wide(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
    String::from_utf16_lossy(&units).trim_end_matches('\0').to_string()
}

/// Reads what a machine says about licences.
pub fn read(message: &[u8]) -> Result<Said, Error> {
    if number16(message, 0)? & 0x0080 == 0 {
        return Err(WRONG);
    }
    let kind = *message.get(4).ok_or(WRONG)?;
    let holds = message.get(8..4 + number16(message, 6)?).ok_or(WRONG)?;
    let sealed = |holds: &[u8]| -> Result<(Vec<u8>, [u8; 16]), Error> {
        let (sealed, rest) = piece(holds)?;
        let check = rest.get(..16).ok_or(WRONG)?;
        Ok((sealed.to_vec(), check.try_into().expect("16 bytes")))
    };
    match kind {
        0xff => match number32(holds, 0)? {
            7 => Ok(Said::Valid),
            why => Ok(Said::Refuses(why as u32)),
        },
        0x01 => {
            let random: [u8; 32] = holds.get(..32).ok_or(WRONG)?.try_into().expect("32 bytes");
            let company_length = number32(holds, 36)?;
            let company = holds.get(40..40 + company_length).ok_or(WRONG)?;
            let at = 40 + company_length;
            let product_length = number32(holds, at)?;
            let product = holds.get(at + 4..at + 4 + product_length).ok_or(WRONG)?;
            let (_ways, rest) = piece(&holds[at + 4 + product_length..])?;
            let (shown, _scopes) = piece(rest)?;
            let (modulus, exponent) = key(shown)?;
            Ok(Said::Asks(Asked { random, company: wide(company), product: wide(product), modulus, exponent }))
        }
        0x02 => sealed(holds.get(4..).ok_or(WRONG)?).map(|(challenge, check)| Said::Challenges(challenge, check)),
        0x03 | 0x04 => sealed(holds).map(|(licence, check)| Said::Hands(licence, check)),
        _ => Err(WRONG),
    }
}

/// The key in what a machine shows: a chain of certificates, of which the last is the
/// machine's own, or a key in a form of the protocol's own.
fn key(shown: &[u8]) -> Result<(Vec<u8>, Vec<u8>), Error> {
    const NO_KEY: Error = Error::NotYet("asks the client to take a licence, and shows no key to take it with");
    const KEY: Error = Error::NotRdp("the key a machine shows for a licence is not built as one");
    if shown.is_empty() {
        return Err(NO_KEY);
    }
    match number32(shown, 0)? & 0x7fff_ffff {
        // the protocol's own form: the key's lengths, its exponent, and its modulus lowest byte first
        1 => {
            let holds = shown.get(16..16 + number16(shown, 14)?).ok_or(KEY)?;
            if holds.get(..4) != Some(b"RSA1") {
                return Err(KEY);
            }
            let length = number32(holds, 4)?.checked_sub(8).ok_or(KEY)?;
            let highest_first = |number: &[u8]| number.iter().rev().skip_while(|byte| **byte == 0).copied().collect::<Vec<u8>>();
            Ok((highest_first(holds.get(20..20 + length).ok_or(KEY)?), highest_first(holds.get(16..20).ok_or(KEY)?)))
        }
        2 => {
            let mut rest = shown.get(8..).ok_or(KEY)?;
            let mut last = None;
            for _ in 0..number32(shown, 4)? {
                let length = number32(rest, 0)?;
                last = Some(rest.get(4..4 + length).ok_or(KEY)?);
                rest = &rest[4 + length..];
            }
            public_key(last.ok_or(NO_KEY)?).map_err(|_| KEY)
        }
        _ => Err(Error::NotYet("shows a key for a licence in a form this version doesn't read")),
    }
}

/// The modulus and the exponent of a certificate's key. Such a certificate names its
/// key's kind in a way of its own, so nothing but the key is read.
fn public_key(certificate: &[u8]) -> Result<(Vec<u8>, Vec<u8>), Error> {
    let mut whole = Reader::new(Reader::new(certificate).expect(der::SEQUENCE)?);
    let mut body = Reader::new(whole.expect(der::SEQUENCE)?);
    // the version, where it is written, the serial number, how it is signed, who issued
    // it, how long it is valid, and whom it is for
    if body.value()?.kind == der::FIRST_OF_ITS_OWN {
        body.value()?;
    }
    for _ in 0..4 {
        body.expect(der::SEQUENCE)?;
    }
    let mut key = Reader::new(body.expect(der::SEQUENCE)?);
    key.expect(der::SEQUENCE)?;
    let [0, numbers @ ..] = key.expect(der::BIT_STRING)? else {
        return Err(Error::Certificate("its key is not a whole number of bytes"));
    };
    let mut numbers = Reader::new(Reader::new(numbers).expect(der::SEQUENCE)?);
    let unsigned = |number: &[u8]| number.iter().skip_while(|byte| **byte == 0).copied().collect::<Vec<u8>>();
    Ok((unsigned(numbers.expect(der::INTEGER)?), unsigned(numbers.expect(der::INTEGER)?)))
}

/// The keys of one exchange: one that checks what is sent, one that seals it.
#[derive(Clone)]
pub(crate) struct Keys {
    checks: [u8; 16],
    seals: [u8; 16],
}

impl Keys {
    /// Makes the keys from the client's secret and both sides' random bytes.
    pub(crate) fn make(secret: &[u8; 48], client: &[u8; 32], machine: &[u8; 32]) -> Keys {
        let salted = |secret: &[u8], first: &[u8; 32], second: &[u8; 32]| -> Vec<u8> {
            [&b"A"[..], b"BB", b"CCC"]
                .into_iter()
                .flat_map(|letters| {
                    let inner = Sha1::new().chain_update(letters).chain_update(secret).chain_update(first).chain_update(second).finalize();
                    Md5::new().chain_update(secret).chain_update(inner).finalize()
                })
                .collect()
        };
        let master = salted(secret, client, machine);
        let both = salted(&master, machine, client);
        Keys {
            checks: both[..16].try_into().expect("16 bytes"),
            seals: Md5::new().chain_update(&both[16..32]).chain_update(client).chain_update(machine).finalize().into(),
        }
    }

    /// The check over something that is sent.
    pub(crate) fn check(&self, sent: &[u8]) -> [u8; 16] {
        let inner = Sha1::new()
            .chain_update(self.checks)
            .chain_update([0x36; 40])
            .chain_update((sent.len() as u32).to_le_bytes())
            .chain_update(sent)
            .finalize();
        Md5::new().chain_update(self.checks).chain_update([0x5c; 48]).chain_update(inner).finalize().into()
    }

    /// Seals something, or opens what was sealed: the cipher is the same both ways, and
    /// begins anew for every piece.
    pub(crate) fn seal(&self, piece: &[u8]) -> Vec<u8> {
        let mut sealed = piece.to_vec();
        Rc4::new_from_slice(&self.seals).expect("a key of 16 bytes").apply_keystream(&mut sealed);
        sealed
    }
}

/// An exchange that is under way.
pub struct Taking {
    keys: Keys,
    known_by: [u8; 20],
}

impl Taking {
    /// Begins: the client's answer to a machine that asks it to take a licence. The
    /// names are the account's and the client's own.
    pub fn begin(asked: &Asked, new: New, account: &str, client: &str) -> Result<(Taking, Vec<u8>), Error> {
        // the protocol's old step: the secret as a number, lowest byte first, to the power
        // of the key's exponent, by its modulus. The secret has to be smaller than the
        // modulus, and the key of a size and a kind that keys have: whatever a machine
        // shows, the arithmetic is never asked for a number it can't work with
        let (modulus, exponent) = (BigUint::from_bytes_be(&asked.modulus), BigUint::from_bytes_be(&asked.exponent));
        let secret = BigUint::from_bytes_le(&new.secret);
        if asked.modulus.len() > 1024 || asked.exponent.len() > 8 || modulus <= secret || exponent < BigUint::from(3u8) {
            return Err(Error::NotRdp("the key a machine shows for a licence is not built as one"));
        }
        let mut sealed = secret.modpow(&exponent, &modulus).to_bytes_le();
        sealed.resize(asked.modulus.len() + 8, 0);

        let named = |name: &str| -> Vec<u8> {
            name.chars().map(|c| if c.is_ascii() && !c.is_ascii_control() { c as u8 } else { b'?' }).chain([0]).collect()
        };
        let mut holds = Vec::new();
        holds.extend(1u32.to_le_bytes());
        holds.extend(PLATFORM.to_le_bytes());
        holds.extend(new.random);
        holds.extend(write_piece(0x0002, &sealed));
        holds.extend(write_piece(0x000f, &named(account)));
        holds.extend(write_piece(0x0010, &named(client)));

        let mut known_by = [0u8; 20];
        known_by[..4].copy_from_slice(&PLATFORM.to_le_bytes());
        known_by[4..].copy_from_slice(&new.known_by);
        Ok((Taking { keys: Keys::make(&new.secret, &new.random, &asked.random), known_by }, message(0x13, &holds)))
    }

    /// Answers the machine's challenge.
    pub fn answer(&self, challenge: &[u8], check: &[u8; 16]) -> Result<Vec<u8>, Error> {
        let asked = self.keys.seal(challenge);
        if self.keys.check(&asked) != *check || asked.len() > usize::from(u16::MAX) {
            return Err(Error::NotRdp("the machine's challenge is not sealed with the keys of this exchange"));
        }
        // the answer says what kind of client this is: another kind than those the protocol names
        let mut says = Vec::with_capacity(asked.len() + 8);
        for number in [0x0100u16, 0xff00, 0x0003, asked.len() as u16] {
            says.extend(number.to_le_bytes());
        }
        says.extend(&asked);
        let mut holds = write_piece(0x0009, &self.keys.seal(&says));
        holds.extend(write_piece(0x0009, &self.keys.seal(&self.known_by)));
        holds.extend(self.keys.check(&[&says[..], &self.known_by].concat()));
        Ok(message(0x15, &holds))
    }

    /// Opens the licence the machine hands over.
    pub fn open(&self, sealed: &[u8], check: &[u8; 16]) -> Result<Taken, Error> {
        let holds = self.keys.seal(sealed);
        if self.keys.check(&holds) != *check {
            return Err(Error::NotRdp("the licence is not sealed with the keys of this exchange"));
        }
        // whose licences these are, who made what it is for, what it is for, and the licence
        let mut at = 4;
        let mut next = || -> Result<&[u8], Error> {
            let length = number32(&holds, at)?;
            let piece = holds.get(at + 4..at + 4 + length).ok_or(WRONG)?;
            at += 4 + length;
            Ok(piece)
        };
        let scope = String::from_utf8_lossy(next()?).trim_end_matches('\0').to_string();
        let company = wide(next()?);
        let product = wide(next()?);
        let licence = next()?.to_vec();
        Ok(Taken { company, product, scope, licence })
    }
}

/// The machine's side of the exchange, for the stand-in of the tests: it shows a key
/// that exists for the tests alone.
#[cfg(any(test, feature = "stand-in"))]
pub mod stand_in {
    use super::*;

    const MODULUS: &str =
        "c944f0c3dc6d0cd3c971d7e9f1a31381b22ac6f2add2ef7b46bbde028e986df0401f74d551dc73747384da9f2f0554fad7ececcdaa141914af8a6b884827ce77";
    const OPENS: &str =
        "5c1b98b389a4be420de67391346c14f93db825e3d978b6f00ba6de2afd66f4a8357b39123c1dd1e9ee14166123a402a1068ca52d6f9019918dd711bb0f202e81";
    const CHALLENGE: &[u8] = b"T\0E\0S\0T\0\0\0";

    fn bytes(hex: &str) -> Vec<u8> {
        (0..hex.len()).step_by(2).map(|at| u8::from_str_radix(&hex[at..at + 2], 16).expect("hex")).collect()
    }

    /// What the stand-in saw of the client in the exchange.
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct Seen {
        /// The account's name and the client's own, as the client gave them.
        pub names: (String, String),
        /// What the client is known by.
        pub known_by: Vec<u8>,
    }

    /// A machine that hands out licences.
    pub struct HandsOut {
        random: [u8; 32],
        keys: Option<Keys>,
        /// What it saw of the client.
        pub seen: Seen,
    }

    impl HandsOut {
        /// A machine with these random bytes for the exchange.
        pub fn new(random: [u8; 32]) -> HandsOut {
            HandsOut { random, keys: None, seen: Seen::default() }
        }

        /// Asks the client to take a licence, and shows its key in the protocol's own form.
        pub fn asks(&self) -> Vec<u8> {
            let wide = |text: &str| -> Vec<u8> { text.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect() };
            let modulus: Vec<u8> = bytes(MODULUS).into_iter().rev().chain([0; 8]).collect();
            let mut key = b"RSA1".to_vec();
            for number in [modulus.len() as u32, 512, 63, 0x0001_0001] {
                key.extend(number.to_le_bytes());
            }
            key.extend(&modulus);
            let mut shown = Vec::new();
            for number in [1u32, 1, 1] {
                shown.extend(number.to_le_bytes());
            }
            shown.extend(write_piece(0x0006, &key));
            shown.extend(write_piece(0x0008, &[0; 72]));

            let mut holds = self.random.to_vec();
            holds.extend(0x000a_0003u32.to_le_bytes());
            for text in ["A stand-in", "A02"] {
                let text = wide(text);
                holds.extend((text.len() as u32).to_le_bytes());
                holds.extend(text);
            }
            holds.extend(write_piece(0x000d, &1u32.to_le_bytes()));
            holds.extend(write_piece(0x0003, &shown));
            holds.extend(1u32.to_le_bytes());
            holds.extend(write_piece(0x000e, b"example.com\0"));
            message(0x01, &holds)
        }

        /// Takes the client's answer, and challenges it.
        pub fn challenges(&mut self, answer: &[u8]) -> Result<Vec<u8>, Error> {
            let holds = answer.get(8..).filter(|_| answer.get(4) == Some(&0x13)).ok_or(WRONG)?;
            let client: [u8; 32] = holds.get(8..40).ok_or(WRONG)?.try_into().expect("32 bytes");
            let (sealed, rest) = piece(&holds[40..])?;
            let (account, rest) = piece(rest)?;
            let (name, _) = piece(rest)?;
            let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).trim_end_matches('\0').to_string();
            self.seen.names = (text(account), text(name));
            // open the client's secret with the key's other half
            let modulus = BigUint::from_bytes_be(&bytes(MODULUS));
            let opened = BigUint::from_bytes_le(&sealed[..64]).modpow(&BigUint::from_bytes_be(&bytes(OPENS)), &modulus).to_bytes_le();
            let mut secret = [0u8; 48];
            secret[..opened.len().min(48)].copy_from_slice(&opened[..opened.len().min(48)]);
            let keys = Keys::make(&secret, &client, &self.random);
            let mut holds = 0u32.to_le_bytes().to_vec();
            holds.extend(write_piece(0x0009, &keys.seal(CHALLENGE)));
            holds.extend(keys.check(CHALLENGE));
            self.keys = Some(keys);
            Ok(message(0x02, &holds))
        }

        /// Takes the client's answer and says that the client is taken as it is, as a
        /// machine does that counts the people who use it and not their devices.
        pub fn goes_on(&mut self, answer: &[u8]) -> Result<Vec<u8>, Error> {
            self.challenges(answer)?;
            self.keys = None;
            Ok(message(0xff, &[&7u32.to_le_bytes()[..], &2u32.to_le_bytes(), &[4, 0, 0, 0]].concat()))
        }

        /// Takes the client's answer to the challenge, and hands over a licence.
        pub fn hands(&mut self, answer: &[u8]) -> Result<Vec<u8>, Error> {
            const NOT_THE_CLIENTS: Error =
                Error::NotRdp("the client's answer to the challenge is not sealed with the keys of this exchange");
            let keys = self.keys.as_ref().ok_or(WRONG)?;
            let holds = answer.get(8..).filter(|_| answer.get(4) == Some(&0x15)).ok_or(WRONG)?;
            let (said, rest) = piece(holds)?;
            let (known_by, rest) = piece(rest)?;
            let (said, known_by) = (keys.seal(said), keys.seal(known_by));
            if rest.get(..16) != Some(&keys.check(&[&said[..], &known_by].concat())[..]) || said.get(8..) != Some(CHALLENGE) {
                return Err(NOT_THE_CLIENTS);
            }
            self.seen.known_by = known_by;
            let wide = |text: &str| -> Vec<u8> { text.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect() };
            let mut licence = 0x0006_0000u32.to_le_bytes().to_vec();
            for piece in [b"example.com\0".to_vec(), wide("A stand-in"), wide("A02"), b"a licence for the tests".to_vec()] {
                licence.extend((piece.len() as u32).to_le_bytes());
                licence.extend(piece);
            }
            let mut holds = write_piece(0x0009, &keys.seal(&licence));
            holds.extend(keys.check(&licence));
            Ok(message(0x03, &holds))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::stand_in::HandsOut;
    use super::*;

    fn new() -> New {
        New { random: [7; 32], secret: core::array::from_fn(|at| at as u8 + 1), known_by: [9; 16] }
    }

    fn asked(machine: &HandsOut) -> Asked {
        match read(&machine.asks()).unwrap() {
            Said::Asks(asked) => asked,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_client_takes_the_licence_a_machine_hands_out() {
        let mut machine = HandsOut::new([3; 32]);
        let asked = asked(&machine);
        assert_eq!((asked.company.as_str(), asked.product.as_str()), ("A stand-in", "A02"));
        assert_eq!((asked.modulus.len(), asked.exponent.as_slice()), (64, &[1, 0, 1][..]));
        assert_eq!(asked.random, [3; 32]);

        let (taking, answer) = Taking::begin(&asked, new(), "alex.urban", "away").unwrap();
        // the answer says it is about a licence, its kind and its own length
        assert_eq!(&answer[..6], &[0x80, 0, 0, 0, 0x13, 0x03]);
        assert_eq!(usize::from(u16::from_le_bytes([answer[6], answer[7]])), answer.len() - 4);
        // the secret is in it sealed, and never as it is
        assert!(!answer.windows(8).any(|part| part == [1, 2, 3, 4, 5, 6, 7, 8]));

        let Said::Challenges(challenge, check) = read(&machine.challenges(&answer).unwrap()).unwrap() else { panic!("no challenge") };
        let answer = taking.answer(&challenge, &check).unwrap();
        assert_eq!(&answer[..6], &[0x80, 0, 0, 0, 0x15, 0x03]);
        let Said::Hands(licence, check) = read(&machine.hands(&answer).unwrap()).unwrap() else { panic!("no licence") };
        let taken = taking.open(&licence, &check).unwrap();
        assert_eq!(
            taken,
            Taken {
                company: "A stand-in".into(),
                product: "A02".into(),
                scope: "example.com".into(),
                licence: b"a licence for the tests".to_vec()
            }
        );
        // what the machine saw of the client
        assert_eq!(machine.seen.names, ("alex.urban".to_string(), "away".to_string()));
        assert_eq!(machine.seen.known_by, [&[0, 0, 1, 4][..], &[9; 16]].concat(), "the platform, then what the client is known by");
    }

    #[test]
    fn a_machine_that_counts_people_asks_and_then_takes_the_client_as_it_is() {
        let mut machine = HandsOut::new([3; 32]);
        let (_, answer) = Taking::begin(&asked(&machine), new(), "alex.urban", "away").unwrap();
        assert_eq!(read(&machine.goes_on(&answer).unwrap()), Ok(Said::Valid));
        assert_eq!(machine.seen.names, ("alex.urban".to_string(), "away".to_string()));
        assert!(machine.seen.known_by.is_empty(), "what the client is known by travels only with the answer to a challenge");
    }

    #[test]
    fn two_sides_with_other_keys_dont_understand_each_other() {
        let mut machine = HandsOut::new([3; 32]);
        let asked = asked(&machine);
        let (_, answer) = Taking::begin(&asked, new(), "alex.urban", "away").unwrap();
        let Said::Challenges(challenge, check) = read(&machine.challenges(&answer).unwrap()).unwrap() else { panic!("no challenge") };
        // a client that began with another secret can't answer this challenge
        let other = New { secret: [0x55; 48], ..new() };
        let (other, _) = Taking::begin(&asked, other, "alex.urban", "away").unwrap();
        assert!(other.answer(&challenge, &check).is_err());
        // and a challenge that was changed on the way is refused
        let (taking, _) = Taking::begin(&asked, new(), "alex.urban", "away").unwrap();
        let mut changed = challenge.clone();
        changed[0] ^= 1;
        assert!(taking.answer(&changed, &check).is_err());
        assert!(taking.open(&challenge, &check).is_err(), "a challenge is no licence");
    }

    #[test]
    fn the_keys_of_an_exchange_come_from_both_sides() {
        let keys = Keys::make(&[1; 48], &[2; 32], &[3; 32]);
        let same = Keys::make(&[1; 48], &[2; 32], &[3; 32]);
        assert_eq!((keys.checks, keys.seals), (same.checks, same.seals));
        for other in
            [Keys::make(&[9; 48], &[2; 32], &[3; 32]), Keys::make(&[1; 48], &[9; 32], &[3; 32]), Keys::make(&[1; 48], &[2; 32], &[9; 32])]
        {
            assert_ne!(keys.checks, other.checks);
            assert_ne!(keys.seals, other.seals);
        }
        assert_ne!(keys.checks, keys.seals);
        // sealing twice is opening, and a check is over the length as well
        assert_eq!(keys.seal(&keys.seal(b"a piece")), b"a piece");
        assert_ne!(keys.check(b"a piece"), keys.check(b"a piece\0"));
    }

    #[test]
    fn a_machine_that_takes_the_client_as_it_is_or_refuses_it_says_so() {
        let word = |why: u32| message(0xff, &[&why.to_le_bytes()[..], &2u32.to_le_bytes(), &[4, 0, 0, 0]].concat());
        assert_eq!(read(&word(7)), Ok(Said::Valid));
        assert_eq!(read(&word(6)), Ok(Said::Refuses(6)));
        assert!(read(&[0, 0, 0, 0, 0xff, 3, 8, 0]).is_err(), "not about a licence");
        assert!(read(&message(0x55, &[0; 8])).is_err());
    }

    #[test]
    fn what_is_cut_is_refused_and_a_key_of_no_size_is_too() {
        let machine = HandsOut::new([3; 32]);
        let whole = machine.asks();
        // whose licences these are comes last, in 20 bytes, and is not read: a cut there changes nothing
        for cut in 0..whole.len() - 20 {
            let mut cut_short = whole[..cut].to_vec();
            if cut >= 8 {
                // its own length says less, as a cut message would
                let length = (cut - 4) as u16;
                cut_short[6..8].copy_from_slice(&length.to_le_bytes());
            }
            assert!(!matches!(read(&cut_short), Ok(Said::Asks(_))), "cut at {cut}");
        }
        let asked = asked(&machine);
        // a key of nothing, of zeros, smaller than the secret, or larger than keys are
        for modulus in [vec![], vec![0; 64], vec![1; 48], vec![1; 1025]] {
            assert!(Taking::begin(&Asked { modulus, ..asked.clone() }, new(), "a", "b").is_err());
        }
        // an exponent of nothing, or one that would leave the secret as it is
        for exponent in [vec![], vec![0], vec![1], vec![1; 9]] {
            assert!(Taking::begin(&Asked { exponent, ..asked.clone() }, new(), "a", "b").is_err());
        }
    }

    #[test]
    fn a_key_is_read_from_the_last_certificate_of_a_chain() {
        // a chain of one: a certificate with the tests' key, in which the key's kind is named as such certificates name it
        let key = der::write(
            der::SEQUENCE,
            &[der::write(der::INTEGER, &[&[0][..], &stand_in_modulus()].concat()), der::write(der::INTEGER, &[1, 0, 1])].concat(),
        );
        let certificate = der::write(
            der::SEQUENCE,
            &der::write(
                der::SEQUENCE,
                &[
                    der::write(der::INTEGER, &[1]),
                    der::write(der::SEQUENCE, &[]),
                    der::write(der::SEQUENCE, &[]),
                    der::write(der::SEQUENCE, &[]),
                    der::write(der::SEQUENCE, &[]),
                    der::write(
                        der::SEQUENCE,
                        &[der::write(der::SEQUENCE, &[]), der::write(der::BIT_STRING, &[&[0][..], &key].concat())].concat(),
                    ),
                ]
                .concat(),
            ),
        );
        let mut shown = Vec::new();
        for number in [0x8000_0002u32, 1, certificate.len() as u32] {
            shown.extend(number.to_le_bytes());
        }
        shown.extend(&certificate);
        shown.extend([0; 12]);
        assert_eq!(key_of(&shown), Ok((stand_in_modulus(), vec![1, 0, 1])));
        assert_eq!(key_of(&[]), Err(Error::NotYet("asks the client to take a licence, and shows no key to take it with")));
        assert!(key_of(&[9, 0, 0, 0, 0, 0, 0, 0]).is_err());
    }

    fn key_of(shown: &[u8]) -> Result<(Vec<u8>, Vec<u8>), Error> {
        key(shown)
    }

    fn stand_in_modulus() -> Vec<u8> {
        let asked = asked(&HandsOut::new([0; 32]));
        asked.modulus
    }
}
