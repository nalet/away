//! A smart card that exists in memory alone, for one session: it holds a certificate
//! and the certificate's key, and answers a Windows machine as a card of the generic
//! kind does that Windows reads with what it brings along (the Generic Identity Device
//! Specification, and the files Windows' own card software keeps on such a card).
//!
//! The machine reads the card through the client's reader ([`away_rdp::smartcard`]):
//! it finds the card by the application on it, reads the files that say what is on
//! the card and the certificate, gives the card its PIN, and has it sign what proves
//! the person to the domain. The card signs, and does nothing else with its key: the
//! key is not read, and nothing is deciphered with it.

use std::io::Write;
use std::sync::{Arc, Mutex};

use away_rdp::secret::Secret;
use rsa::pkcs8::DecodePrivateKey;
use rsa::traits::PublicKeyParts;

use crate::certificate::Certificate;
use crate::Error;

/// What the card says of itself when it is given power: a card that speaks in whole
/// blocks. A Windows machine finds out what a card is from the application on it, not
/// from this.
const SAYS_OF_ITSELF: [u8; 17] = [0x3b, 0xf7, 0x18, 0x00, 0x00, 0x80, 0x31, 0xfe, 0x45, 0x73, 0x66, 0x74, 0x65, 0x2d, 0x6e, 0x66, 0xc4];

/// The name of the application on the card, and what the card answers when the
/// application is chosen: its whole name, and how its keeper would prove itself.
const APPLICATION: [u8; 11] = [0xa0, 0x00, 0x00, 0x03, 0x97, 0x42, 0x54, 0x46, 0x59, 0x02, 0x01];
const ABOUT_THE_APPLICATION: [u8; 20] =
    [0x61, 0x12, 0x4f, 0x0b, 0xa0, 0x00, 0x00, 0x03, 0x97, 0x42, 0x54, 0x46, 0x59, 0x02, 0x01, 0x73, 0x03, 0x40, 0x01, 0xc0];
const ABOUT_ITS_FILES: [u8; 10] = [0x62, 0x08, 0x82, 0x01, 0x38, 0x8c, 0x03, 0x03, 0x30, 0x30];

/// The card's one key: the number it goes by, and the file that says so.
const KEY: u8 = 0x81;
/// How many times a wrong PIN is taken before the card takes none any more.
const TRIES: u8 = 3;

// the card's answers that are no data: how a command ended
const DONE: u16 = 0x9000;
const WRONG_LENGTH: u16 = 0x6700;
const NOT_PROVEN: u16 = 0x6982;
const BLOCKED: u16 = 0x6983;
const NOT_NOW: u16 = 0x6985;
const WRONG_DATA: u16 = 0x6a80;
const NO_SUCH_FILE: u16 = 0x6a82;
const NOT_THIS_WAY: u16 = 0x6a86;
const NO_SUCH_THING: u16 = 0x6a88;
const NOT_DONE: u16 = 0x6d00;

/// What the machine did with the card, for whoever says what happens on a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Did {
    /// It found the card to be of the kind it reads.
    Found,
    /// It read the certificate.
    ReadTheCertificate,
    /// It gave the card a PIN, which was the card's or was not.
    GaveThePin {
        /// Whether the PIN was the card's.
        right: bool,
    },
    /// It had the card sign.
    Signed,
    /// It asked the card for something the card does not do: the command's number.
    NotDone(u8),
}

/// What the machine did with the card, as it goes along. Whoever made the card keeps
/// this, and the reader keeps the card.
pub type Told = Arc<Mutex<Vec<Did>>>;

/// The card.
pub struct Card {
    /// The card's files: for each, the file it is in, the number it goes by, and what it holds.
    files: Vec<(u16, u16, Vec<u8>)>,
    key: rsa::PrivateKey,
    pin: Secret,
    told: Told,
    /// Whether the application on the card is chosen.
    chosen: bool,
    /// Whether the PIN was given since the card got its power.
    proven: bool,
    tries: u8,
    /// What the key is to be used for, and how, where the machine said so.
    signs: Option<(u8, u8, u8)>,
    /// What of an answer the machine has not fetched yet.
    left: Vec<u8>,
    /// What of a command in several parts has arrived.
    parts: Vec<u8>,
}

/// One command, taken apart: its kind, its two parameters, what it carries, and how
/// much the machine wants back.
struct Command<'a> {
    in_parts: bool,
    kind: u8,
    parameters: (u8, u8),
    carries: &'a [u8],
    wants: Option<u8>,
}

impl<'a> Command<'a> {
    /// A command in the short form, which is the only one this card takes.
    fn read(command: &'a [u8]) -> Option<Command<'a>> {
        let (head, rest) = command.split_at_checked(4)?;
        let (carries, wants) = match rest {
            [] => (&[][..], None),
            [wants] => (&[][..], Some(*wants)),
            [length, carried @ ..] => match carried.len().checked_sub(usize::from(*length))? {
                0 => (carried, None),
                1 => (&carried[..carried.len() - 1], carried.last().copied()),
                _ => return None,
            },
        };
        Some(Command { in_parts: head[0] & 0x10 != 0, kind: head[1], parameters: (head[2], head[3]), carries, wants })
    }
}

/// A thing on the card as it travels: the number it goes by, how long it is, and it.
fn thing(number: u16, holds: &[u8]) -> Vec<u8> {
    let mut thing = number.to_be_bytes().to_vec();
    match holds.len() {
        length @ ..0x80 => thing.push(length as u8),
        length @ ..0x100 => thing.extend([0x81, length as u8]),
        length => thing.extend([0x82, (length >> 8) as u8, length as u8]),
    }
    thing.extend_from_slice(holds);
    thing
}

impl Card {
    /// A card that holds this certificate and its key, takes this PIN, and names what
    /// holds its key as it is told to.
    ///
    /// The key is the certificate's, of 2048 bits, written as PKCS #8. The sixteen
    /// bytes are what a machine knows the card by: they are new for every card, because
    /// a machine keeps what it read of a card under them. Gives the card, and where
    /// what a machine does with it is told.
    pub fn new(certificate: &[u8], key: &[u8], pin: Secret, container: &str, known_by: [u8; 16]) -> Result<(Card, Told), Error> {
        let key =
            rsa::PrivateKey::from_pkcs8_der(key).map_err(|_| Error::NotACertificate("the key is not an RSA key written as PKCS #8"))?;
        let modulus = key.n().to_be_bytes();
        if modulus.len() != 256 {
            return Err(Error::NotACertificate("a card's key has 2048 bits, and this one has not"));
        }
        // the key is the certificate's: the certificate holds the same modulus
        let read = Certificate::read(certificate)?;
        if !read.key.is_some_and(|public| public.windows(256).any(|part| part == &modulus[..])) {
            return Err(Error::NotACertificate("the key is not the certificate's"));
        }
        let name: Vec<u16> = container.encode_utf16().collect();
        if name.is_empty() || name.len() > 39 || container.contains('\\') {
            return Err(Error::NotACertificate("what holds the key has a name of 39 characters at most, without a backslash"));
        }
        let length = u16::try_from(certificate.len()).map_err(|_| Error::NotACertificate("the certificate is longer than a card holds"))?;

        // the files a machine's card software keeps on a card, as they are on one that
        // holds one key for exchanging keys and its certificate
        let entry = |directory: &str, file: &str, number: u16, inside: u16| {
            let mut entry = [0u8; 28];
            entry[..directory.len()].copy_from_slice(directory.as_bytes());
            entry[9..9 + file.len()].copy_from_slice(file.as_bytes());
            entry[20..22].copy_from_slice(&number.to_le_bytes());
            entry[24..26].copy_from_slice(&inside.to_le_bytes());
            entry
        };
        let mut which = vec![1];
        for entry in [
            entry("mscp", "", 0, 0xa000),
            entry("", "cardid", 0xdf20, 0xa012),
            entry("", "cardapps", 0xdf21, 0xa010),
            entry("", "cardcf", 0xdf22, 0xa010),
            entry("mscp", "cmapfile", 0xdf23, 0xa010),
            entry("mscp", "kxc00", 0xdf24, 0xa010),
        ] {
            which.extend(entry);
        }
        // what holds the key: its name, that it is there and the one to use, and a key
        // for exchanging keys of 2048 bits
        let mut holds_the_key = vec![0u8; 86];
        for (to, from) in holds_the_key.as_chunks_mut::<2>().0.iter_mut().zip(&name) {
            *to = from.to_le_bytes();
        }
        holds_the_key[80] = 0x03;
        holds_the_key[84..].copy_from_slice(&2048u16.to_le_bytes());
        // the certificate, packed the way a card holds one
        let mut packed = vec![0x01, 0x00];
        packed.extend(length.to_le_bytes());
        let mut packer = flate2::write::ZlibEncoder::new(packed, flate2::Compression::default());
        packer.write_all(certificate).expect("bytes are written into memory");
        let packed = packer.finish().expect("bytes are written into memory");

        let files = vec![
            (0xa000, 0xdf1f, which),
            // the key by its number: in use, of 2048 bits, for exchanging keys
            (0xa000, 0xdf20, vec![0x01, 0x01, 0x00, 0x00, 0x00, 0x07, 0x9a, KEY, 0xb0, 0xff, 0xff, 0x00, 0x00]),
            (0xa012, 0xdf20, known_by.to_vec()),
            (0xa010, 0xdf21, b"mscp\0\0\0\0".to_vec()),
            (0xa010, 0xdf22, vec![0x00, 0x00, 0x01, 0x00, 0x04, 0x00]),
            (0xa010, 0xdf23, holds_the_key),
            (0xa010, 0xdf24, packed),
        ];
        let told = Told::default();
        let card = Card {
            files,
            key,
            pin,
            told: told.clone(),
            chosen: false,
            proven: false,
            tries: TRIES,
            signs: None,
            left: Vec::new(),
            parts: Vec::new(),
        };
        Ok((card, told))
    }

    fn tells(&self, did: Did) {
        if let Ok(mut told) = self.told.lock() {
            told.push(did);
        }
    }

    /// The next of an answer the machine fetches in parts: 256 bytes at most, and
    /// whether more is left.
    fn next(&mut self, at_most: usize) -> Vec<u8> {
        let count = self.left.len().min(at_most).min(256);
        let mut answer: Vec<u8> = self.left.drain(..count).collect();
        answer.extend(match self.left.len() {
            0 => DONE.to_be_bytes(),
            left @ ..256 => [0x61, left as u8],
            _ => [0x61, 0x00],
        });
        answer
    }

    /// What one command gives, or how it ended where it gives nothing.
    fn does(&mut self, command: &Command) -> Result<Vec<u8>, u16> {
        // a command in several parts is the same command each time
        if command.kind != 0x2a {
            self.parts.clear();
        }
        match (command.kind, command.parameters) {
            // choosing the application by its name, of which the machine sends the beginning
            (0xa4, (0x04, how)) => {
                if command.carries.len() > 16 {
                    return Err(WRONG_LENGTH);
                }
                if command.carries.is_empty() || !APPLICATION.starts_with(command.carries) {
                    return Err(NO_SUCH_FILE);
                }
                self.chosen = true;
                self.tells(Did::Found);
                match how {
                    0x00 => Ok(ABOUT_THE_APPLICATION.to_vec()),
                    0x04 => Ok(ABOUT_ITS_FILES.to_vec()),
                    0x0c => Ok(Vec::new()),
                    _ => Err(NOT_THIS_WAY),
                }
            }
            // choosing the application as the place where the card is
            (0xa4, (0x00, _)) => match command.carries {
                [0x3f, 0xff] if self.chosen => Ok(Vec::new()),
                [_, _] => Err(NO_SUCH_FILE),
                _ => Err(WRONG_LENGTH),
            },
            (0xa4, _) => Err(NOT_THIS_WAY),
            _ if !self.chosen => Err(NOT_NOW),
            // reading a file, or the key's public half
            (0xcb, (high, low)) => {
                let inside = u16::from_be_bytes([high, low]);
                match command.carries {
                    [0x5c, 0x02, high, low] => {
                        let number = u16::from_be_bytes([*high, *low]);
                        let file = self.files.iter().find(|(file, its, _)| *its == number && (inside == 0x3fff || *file == inside));
                        let (_, _, holds) = file.ok_or(NO_SUCH_THING)?;
                        if number == 0xdf24 {
                            self.tells(Did::ReadTheCertificate);
                        }
                        Ok(thing(number, holds))
                    }
                    [0x70, 0x08, 0x84, 0x01, KEY, 0xa5, 0x03, 0x7f, 0x49, 0x80] if inside == 0x3fff => {
                        let modulus = self.key.n().to_be_bytes();
                        let exponent = self.key.e().to_be_bytes();
                        let exponent = &exponent[exponent.iter().position(|byte| *byte != 0).unwrap_or(0)..];
                        let mut public = vec![0x81, 0x82, 0x01, 0x00];
                        public.extend_from_slice(&modulus);
                        public.extend([0x82, exponent.len() as u8]);
                        public.extend_from_slice(exponent);
                        let mut answer = vec![0x7f, 0x49, 0x82, (public.len() >> 8) as u8, public.len() as u8];
                        answer.extend(public);
                        Ok(answer)
                    }
                    _ => Err(WRONG_DATA),
                }
            }
            // the PIN: given, asked about, or forgotten
            (0x20, (0x00, 0x80)) => {
                if command.carries.is_empty() {
                    return if self.proven { Ok(Vec::new()) } else { Err(0x63c0 | u16::from(self.tries)) };
                }
                if self.tries == 0 {
                    return Err(BLOCKED);
                }
                // the PIN as it is, or filled up with zeros; compared without stopping at the first difference
                let pin = self.pin.expose().as_bytes();
                let (given, filled) = command.carries.split_at(command.carries.len().min(pin.len()));
                let differs = given.iter().zip(pin).fold(given.len() ^ pin.len(), |differs, (a, b)| differs | usize::from(a ^ b));
                let right = differs == 0 && filled.iter().all(|byte| *byte == 0);
                self.tells(Did::GaveThePin { right });
                if right {
                    (self.proven, self.tries) = (true, TRIES);
                    Ok(Vec::new())
                } else {
                    self.tries -= 1;
                    self.proven = false;
                    Err(0x63c0 | u16::from(self.tries))
                }
            }
            (0x20, (0x00, 0x82)) => {
                self.proven = false;
                Ok(Vec::new())
            }
            (0x20, _) => Err(NO_SUCH_THING),
            // what the key is to be used for, and how
            (0x22, (0x41, what)) => match command.carries {
                [0x80, 0x01, how, 0x84, 0x01, key] => {
                    self.signs = Some((what, *how, *key));
                    Ok(Vec::new())
                }
                carries if carries.len() != 6 => Err(WRONG_LENGTH),
                _ => Err(WRONG_DATA),
            },
            (0x22, _) => Err(NOT_THIS_WAY),
            // using the key
            (0x2a, parameters) => {
                if command.carries.is_empty() {
                    return Err(WRONG_LENGTH);
                }
                self.parts.extend_from_slice(command.carries);
                if command.in_parts {
                    return Ok(Vec::new());
                }
                let to_sign = std::mem::take(&mut self.parts);
                // the card signs, in the one way a domain takes, and what it signs is a
                // hash with what says which hash it is: it puts the filling around it itself
                let Some((0xb6, how, KEY)) = self.signs else { return Err(if self.signs.is_some() { NOT_PROVEN } else { NOT_NOW }) };
                if parameters != (0x9e, 0x9a) || how & 0xf0 != 0x50 {
                    return Err(NOT_THIS_WAY);
                }
                if !self.proven {
                    return Err(NOT_PROVEN);
                }
                let signed = self.key.sign(rsa::Pkcs1v15Sign::new_unprefixed(), &to_sign).map_err(|_| WRONG_LENGTH)?;
                self.tells(Did::Signed);
                Ok(signed)
            }
            (other, _) => {
                self.tells(Did::NotDone(other));
                Err(NOT_DONE)
            }
        }
    }
}

impl away_rdp::smartcard::Card for Card {
    fn says_of_itself(&self) -> Vec<u8> {
        SAYS_OF_ITSELF.to_vec()
    }

    fn answer(&mut self, command: &[u8]) -> Vec<u8> {
        let Some(command) = Command::read(command) else { return WRONG_LENGTH.to_be_bytes().to_vec() };
        // the rest of an answer that was longer than one answer is
        if command.kind == 0xc0 {
            return match (command.parameters, self.left.is_empty()) {
                ((0, 0), false) => self.next(command.wants.map_or(256, |wants| if wants == 0 { 256 } else { usize::from(wants) })),
                ((0, 0), true) => NOT_NOW.to_be_bytes().to_vec(),
                _ => NOT_THIS_WAY.to_be_bytes().to_vec(),
            };
        }
        // any other command lets go of what was not fetched
        self.left.clear();
        match self.does(&command) {
            Ok(gives) => {
                self.left = gives;
                self.next(256)
            }
            Err(ended) => ended.to_be_bytes().to_vec(),
        }
    }

    fn reset(&mut self) {
        (self.chosen, self.proven, self.signs) = (false, false, None);
        self.left.clear();
        self.parts.clear();
    }
}

/// The machine's side of it, for the tests' stand-in for one: the commands a Windows
/// machine gives a card when it signs a person in with it, and what the card's answers
/// to them hold.
#[cfg(any(test, feature = "stand-in"))]
pub mod stand_in {
    use std::io::Read as _;

    /// What says that a hash is one of SHA-256, before the hash.
    pub const SHA_256: [u8; 19] =
        [0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01, 0x05, 0x00, 0x04, 0x20];

    /// What a machine has a card sign for a message: the message's hash.
    pub fn hash_of(message: &[u8]) -> [u8; 32] {
        use sha2::Digest;
        sha2::Sha256::digest(message).into()
    }

    /// How many times more of the certificate is fetched: enough for one of 2 KiB.
    const FETCHES: usize = 7;

    /// The commands, in their order: the application is chosen, the files that say
    /// what is on the card are read, then the certificate, the PIN is given, and this
    /// hash is signed.
    pub fn commands(pin: &str, hash: &[u8; 32]) -> Vec<Vec<u8>> {
        let read = |inside: u16, number: u16| {
            let ([high, low], [of_high, of_low]) = (inside.to_be_bytes(), number.to_be_bytes());
            vec![0x00, 0xcb, high, low, 0x04, 0x5c, 0x02, of_high, of_low, 0x00]
        };
        let mut commands = vec![vec![0x00, 0xa4, 0x04, 0x00, 0x09, 0xa0, 0x00, 0x00, 0x03, 0x97, 0x42, 0x54, 0x46, 0x59, 0x00]];
        commands.extend([read(0xa000, 0xdf1f), read(0xa012, 0xdf20), read(0xa010, 0xdf22), read(0xa010, 0xdf21), read(0xa010, 0xdf23)]);
        commands.push(read(0xa010, 0xdf24));
        commands.extend(std::iter::repeat_n(vec![0x00, 0xc0, 0x00, 0x00, 0x00], FETCHES));
        commands.push(read(0xa000, 0xdf20));
        commands.push([&[0x00, 0x20, 0x00, 0x80, pin.len() as u8][..], pin.as_bytes()].concat());
        commands.push(vec![0x00, 0x22, 0x41, 0xb6, 0x06, 0x80, 0x01, 0x57, 0x84, 0x01, 0x81]);
        commands.push([&[0x00, 0x2a, 0x9e, 0x9a, 0x33][..], &SHA_256[..], &hash[..], &[0x00]].concat());
        commands
    }

    /// Whether a signature over a message is that of a certificate's key, as a domain
    /// checks the one a card made over a hash of SHA-256.
    pub fn is_signed_by(certificate: &[u8], message: &[u8], signature: &[u8]) -> bool {
        let Ok(Some(key)) = crate::certificate::Certificate::read(certificate).map(|certificate| certificate.key) else { return false };
        ring::signature::UnparsedPublicKey::new(&ring::signature::RSA_PKCS1_2048_8192_SHA256, key).verify(message, signature).is_ok()
    }

    /// What a machine read on the card with those commands.
    #[derive(Debug, PartialEq, Eq)]
    pub struct Read {
        /// What the card is known by.
        pub known_by: Vec<u8>,
        /// The name of what holds the key.
        pub container: String,
        /// The certificate, unpacked.
        pub certificate: Vec<u8>,
        /// Whether the card took the PIN.
        pub took_the_pin: bool,
        /// What the card signed the hash with: the signature, or nothing where it did not sign.
        pub signed: Vec<u8>,
    }

    /// Reads the card's answers to the commands. Gives nothing where they are not what
    /// a card of this kind answers.
    pub fn read(answers: &[Vec<u8>]) -> Option<Read> {
        let well = |answer: &Vec<u8>| answer.strip_suffix(&[0x90, 0x00]).map(<[u8]>::to_vec);
        let known_by = well(answers.get(2)?)?.get(3..)?.to_vec();
        let holds = well(answers.get(5)?)?;
        let name: Vec<u16> =
            holds.get(3..83)?.as_chunks::<2>().0.iter().map(|two| u16::from_le_bytes(*two)).take_while(|unit| *unit != 0).collect();
        // the certificate, in as many answers as it took
        let mut packed = Vec::new();
        for answer in answers.get(6..7 + FETCHES)? {
            let (holds, ended) = answer.split_at_checked(answer.len().checked_sub(2)?)?;
            packed.extend_from_slice(holds);
            if ended == [0x90, 0x00] {
                break;
            }
        }
        let inside = match packed.get(2)? {
            0x82 => packed.get(5..)?,
            0x81 => packed.get(4..)?,
            _ => packed.get(3..)?,
        };
        let mut certificate = Vec::new();
        flate2::read::ZlibDecoder::new(inside.get(4..)?).read_to_end(&mut certificate).ok()?;
        if inside[..2] != [0x01, 0x00] || usize::from(u16::from_le_bytes([inside[2], inside[3]])) != certificate.len() {
            return None;
        }
        let took_the_pin = answers.get(8 + FETCHES)? == &[0x90, 0x00];
        let signed = well(answers.get(10 + FETCHES)?).unwrap_or_default();
        Some(Read { known_by, container: String::from_utf16(&name).ok()?, certificate, took_the_pin, signed })
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;
    use away_rdp::smartcard::Card as _;
    use sha2::{Digest, Sha256};

    const CERTIFICATE: &[u8] = include_bytes!("../tests/fixtures/account.der");
    const KEY_OF_IT: &[u8] = include_bytes!("../tests/fixtures/account.key.der");
    const ANOTHERS_KEY: &[u8] = include_bytes!("../tests/fixtures/controller.key.der");
    const PIN: &str = "a PIN made for one session";

    const CHOOSE: [u8; 15] = [0x00, 0xa4, 0x04, 0x00, 0x09, 0xa0, 0x00, 0x00, 0x03, 0x97, 0x42, 0x54, 0x46, 0x59, 0x00];
    const SIGNS_THIS_WAY: [u8; 11] = [0x00, 0x22, 0x41, 0xb6, 0x06, 0x80, 0x01, 0x57, 0x84, 0x01, 0x81];
    /// What says that a hash is one of SHA-256, before the hash.
    const SHA_256: [u8; 19] =
        [0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01, 0x05, 0x00, 0x04, 0x20];

    fn card() -> (Card, Told) {
        Card::new(CERTIFICATE, KEY_OF_IT, Secret::new(PIN), "a key for one session", [0x5c; 16]).expect("a card")
    }

    /// A command with what it carries, as a machine sends one that wants an answer.
    fn command(head: [u8; 4], carries: &[u8]) -> Vec<u8> {
        [&head[..], &[carries.len() as u8], carries, &[0]].concat()
    }

    /// A file read whole, as a machine reads one: the first answer, and the rest of it
    /// for as long as the card says that more is left.
    fn read(card: &mut Card, inside: u16, number: u16) -> Vec<u8> {
        let [high, low] = inside.to_be_bytes();
        let mut answer = card.answer(&command([0x00, 0xcb, high, low], &[0x5c, 0x02, (number >> 8) as u8, number as u8]));
        let mut whole = Vec::new();
        loop {
            let (holds, ended) = answer.split_at(answer.len() - 2);
            whole.extend_from_slice(holds);
            match ended {
                [0x90, 0x00] => return whole,
                [0x61, left] => answer = card.answer(&[0x00, 0xc0, 0x00, 0x00, *left]),
                ended => panic!("the read ended with {ended:02x?}"),
            }
        }
    }

    #[test]
    fn a_machine_finds_the_card_by_its_application_and_reads_what_is_on_it() {
        let (mut card, told) = card();
        assert_eq!(card.says_of_itself().len(), 17);
        // nothing is read before the application is chosen, and another application is not on the card
        assert_eq!(card.answer(&command([0x00, 0xcb, 0xa0, 0x12], &[0x5c, 0x02, 0xdf, 0x20])), [0x69, 0x85]);
        assert_eq!(card.answer(&[0x00, 0xa4, 0x04, 0x00, 0x09, 0xa0, 0x00, 0x00, 0x03, 0x08, 0x00, 0x00, 0x10, 0x00]), [0x6a, 0x82]);
        assert_eq!(card.answer(&[0x00, 0xa4, 0x00, 0x0c, 0x02, 0x3f, 0xff]), [0x6a, 0x82]);
        assert_eq!(card.answer(&CHOOSE), [&ABOUT_THE_APPLICATION[..], &[0x90, 0x00]].concat());
        assert_eq!(card.answer(&[0x00, 0xa4, 0x00, 0x0c, 0x02, 0x3f, 0xff]), [0x90, 0x00]);

        // which files there are: the card's own list, of six entries of 28 bytes
        let which = read(&mut card, 0xa000, 0xdf1f);
        assert_eq!((&which[..5], which.len()), (&[0xdf, 0x1f, 0x81, 0xa9, 0x01][..], 4 + 1 + 6 * 28));
        assert_eq!(&which[5 + 5 * 28..][..28], b"mscp\0\0\0\0\0kxc00\0\0\0\0\0\0\x24\xdf\0\0\x10\xa0\0\0");
        // what the card is known by, what is on it, and the key by its number
        assert_eq!(read(&mut card, 0xa012, 0xdf20), [&[0xdf, 0x20, 0x10][..], &[0x5c; 16]].concat());
        assert_eq!(read(&mut card, 0xa010, 0xdf21), [0xdf, 0x21, 0x08, 0x6d, 0x73, 0x63, 0x70, 0, 0, 0, 0]);
        assert_eq!(read(&mut card, 0xa010, 0xdf22), [0xdf, 0x22, 0x06, 0x00, 0x00, 0x01, 0x00, 0x04, 0x00]);
        assert_eq!(read(&mut card, 0xa000, 0xdf20), [0xdf, 0x20, 0x0d, 0x01, 0x01, 0, 0, 0, 0x07, 0x9a, 0x81, 0xb0, 0xff, 0xff, 0, 0]);
        // what holds the key: its name in wide characters, and that it is the one to use
        let holds = read(&mut card, 0xa010, 0xdf23);
        let name: Vec<u8> = "a key for one session".encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(
            (&holds[..3], &holds[3..3 + name.len()], &holds[83..]),
            (&[0xdf, 0x23, 0x56][..], &name[..], &[0x03, 0, 0, 0, 0x00, 0x08][..])
        );
        // a file that is not there, in the file it is asked in, and anywhere
        assert_eq!(card.answer(&command([0x00, 0xcb, 0xa0, 0x10], &[0x5c, 0x02, 0xdf, 0x20])), [0x6a, 0x88]);
        assert_eq!(card.answer(&command([0x00, 0xcb, 0x3f, 0xff], &[0x5c, 0x02, 0x7f, 0x73])), [0x6a, 0x88]);
        assert_eq!(read(&mut card, 0x3fff, 0xdf21).len(), 11);

        // the certificate comes in several answers, packed, and unpacks to the certificate
        let packed = read(&mut card, 0xa010, 0xdf24);
        assert_eq!(&packed[..3], &[0xdf, 0x24, 0x82]);
        assert_eq!(usize::from(u16::from_be_bytes([packed[3], packed[4]])), packed.len() - 5);
        let holds = &packed[5..];
        assert_eq!((&holds[..2], usize::from(u16::from_le_bytes([holds[2], holds[3]]))), (&[0x01, 0x00][..], CERTIFICATE.len()));
        let mut unpacked = Vec::new();
        flate2::read::ZlibDecoder::new(&holds[4..]).read_to_end(&mut unpacked).expect("what is packed unpacks");
        assert_eq!(unpacked, CERTIFICATE);
        assert_eq!(*told.lock().unwrap(), [Did::Found, Did::ReadTheCertificate]);

        // the key's public half is the certificate's
        let asked = [0x00, 0xcb, 0x3f, 0xff, 0x0a, 0x70, 0x08, 0x84, 0x01, 0x81, 0xa5, 0x03, 0x7f, 0x49, 0x80, 0x00];
        let first = card.answer(&asked);
        assert_eq!((first.len(), &first[256..]), (258, &[0x61, 0x0e][..]));
        let rest = card.answer(&[0x00, 0xc0, 0x00, 0x00, 0x0e]);
        let public = [&first[..256], &rest[..14]].concat();
        assert_eq!(
            (&public[..9], &public[265..], &rest[14..]),
            (&[0x7f, 0x49, 0x82, 0x01, 0x09, 0x81, 0x82, 0x01, 0x00][..], &[0x82, 0x03, 1, 0, 1][..], &[0x90, 0x00][..])
        );
        let of_the_certificate = Certificate::read(CERTIFICATE).unwrap().key.unwrap();
        assert!(of_the_certificate.windows(256).any(|part| part == &public[9..265]));
        // nothing is left to fetch, and another command lets go of what was not fetched
        assert_eq!(card.answer(&[0x00, 0xc0, 0x00, 0x00, 0x00]), [0x69, 0x85]);
        card.answer(&asked);
        assert_eq!(card.answer(&CHOOSE).len(), 22);
        assert_eq!(card.answer(&[0x00, 0xc0, 0x00, 0x00, 0x0e]), [0x69, 0x85]);
    }

    #[test]
    fn the_card_signs_for_who_gave_its_pin_and_for_nobody_else() {
        let (mut card, told) = card();
        card.answer(&CHOOSE);
        let hash = Sha256::digest(b"what proves the person to the domain");
        let to_sign = command([0x00, 0x2a, 0x9e, 0x9a], &[&SHA_256[..], &hash[..]].concat());
        // without saying what the key is for, and without the PIN, nothing is signed
        assert_eq!(card.answer(&to_sign), [0x69, 0x85]);
        assert_eq!(card.answer(&SIGNS_THIS_WAY), [0x90, 0x00]);
        assert_eq!(card.answer(&to_sign), [0x69, 0x82]);
        // whether the PIN was given is asked without giving one, and a wrong PIN counts
        assert_eq!(card.answer(&[0x00, 0x20, 0x00, 0x80]), [0x63, 0xc3]);
        assert_eq!(card.answer(&command([0x00, 0x20, 0x00, 0x80], b"another PIN")[..16]), [0x63, 0xc2]);
        assert_eq!(card.answer(&command([0x00, 0x20, 0x00, 0x80], &PIN.as_bytes()[..10])[..15]), [0x63, 0xc1]);
        // the right one, as it is or filled up with zeros, is taken, and the count begins again
        let given = command([0x00, 0x20, 0x00, 0x80], PIN.as_bytes());
        assert_eq!(card.answer(&given[..given.len() - 1]), [0x90, 0x00]);
        assert_eq!(card.answer(&[0x00, 0x20, 0x00, 0x80]), [0x90, 0x00]);
        let filled = [PIN.as_bytes(), &[0; 6]].concat();
        assert_eq!(card.answer(&command([0x00, 0x20, 0x00, 0x80], &filled)[..5 + filled.len()]), [0x90, 0x00]);

        // now it signs: the signature is the key's, over the hash, as a domain checks one
        let signed = card.answer(&to_sign);
        assert_eq!((signed.len(), &signed[256..]), (258, &[0x90, 0x00][..]));
        let public = Certificate::read(CERTIFICATE).unwrap().key.unwrap();
        ring::signature::UnparsedPublicKey::new(&ring::signature::RSA_PKCS1_2048_8192_SHA256, public)
            .verify(b"what proves the person to the domain", &signed[..256])
            .expect("the signature holds");
        // what is to be signed may come in two parts
        let whole = [&SHA_256[..], &hash[..]].concat();
        assert_eq!(card.answer(&[&[0x10, 0x2a, 0x9e, 0x9a, 0x13][..], &whole[..19]].concat()), [0x90, 0x00]);
        assert_eq!(card.answer(&command([0x00, 0x2a, 0x9e, 0x9a], &whole[19..]))[..256], signed[..256]);

        // nothing is deciphered, nothing is signed without the card's own filling, and the key is not written
        assert_eq!(card.answer(&[0x00, 0x22, 0x41, 0xb8, 0x06, 0x80, 0x01, 0x47, 0x84, 0x01, 0x81]), [0x90, 0x00]);
        assert_eq!(card.answer(&command([0x00, 0x2a, 0x80, 0x86], &[0x55; 64])), [0x69, 0x82]);
        assert_eq!(card.answer(&[0x00, 0x22, 0x41, 0xb6, 0x06, 0x80, 0x01, 0x17, 0x84, 0x01, 0x81]), [0x90, 0x00]);
        assert_eq!(card.answer(&to_sign), [0x6a, 0x86]);
        assert_eq!(card.answer(&command([0x00, 0xdb, 0xa0, 0x10], &[0xdf, 0x22, 0x00])), [0x6d, 0x00]);
        // the PIN is forgotten when the machine says so, and when the card loses its power
        card.answer(&SIGNS_THIS_WAY);
        assert_eq!(card.answer(&[0x00, 0x20, 0x00, 0x82]), [0x90, 0x00]);
        assert_eq!(card.answer(&to_sign), [0x69, 0x82]);
        card.answer(&given[..given.len() - 1]);
        card.reset();
        card.answer(&CHOOSE);
        card.answer(&SIGNS_THIS_WAY);
        assert_eq!(card.answer(&to_sign), [0x69, 0x82]);
        use Did::{Found, GaveThePin, NotDone, Signed};
        assert_eq!(
            *told.lock().unwrap(),
            [
                Found,
                GaveThePin { right: false },
                GaveThePin { right: false },
                GaveThePin { right: true },
                GaveThePin { right: true },
                Signed,
                Signed,
                NotDone(0xdb),
                GaveThePin { right: true },
                Found
            ]
        );
    }

    #[test]
    fn the_card_answers_the_commands_of_a_machine_that_signs_a_person_in_with_it() {
        let (mut card, told) = card();
        let hash: [u8; 32] = Sha256::digest(b"what proves the person to the domain").into();
        let answers: Vec<Vec<u8>> = stand_in::commands(PIN, &hash).iter().map(|command| card.answer(command)).collect();
        let read = stand_in::read(&answers).expect("the answers of a card of this kind");
        assert_eq!((&read.known_by[..], read.container.as_str(), read.took_the_pin), (&[0x5c; 16][..], "a key for one session", true));
        assert!(read.certificate == CERTIFICATE, "the certificate is read whole");
        assert!(stand_in::is_signed_by(CERTIFICATE, b"what proves the person to the domain", &read.signed), "the signature holds");
        assert!(!stand_in::is_signed_by(CERTIFICATE, b"something else", &read.signed));
        assert_eq!(*told.lock().unwrap(), [Did::Found, Did::ReadTheCertificate, Did::GaveThePin { right: true }, Did::Signed]);
        // with another PIN the card signs nothing
        let (mut card, _) = self::card();
        let answers: Vec<Vec<u8>> = stand_in::commands("another PIN", &hash).iter().map(|command| card.answer(command)).collect();
        let read = stand_in::read(&answers).expect("the answers of a card of this kind");
        assert_eq!((read.took_the_pin, read.signed.len()), (false, 0));
    }

    #[test]
    fn three_wrong_pins_and_the_card_takes_none_any_more() {
        let (mut card, _) = card();
        card.answer(&CHOOSE);
        let wrong = command([0x00, 0x20, 0x00, 0x80], b"another PIN");
        for left in [2, 1, 0] {
            assert_eq!(card.answer(&wrong[..wrong.len() - 1]), [0x63, 0xc0 | left]);
        }
        let right = command([0x00, 0x20, 0x00, 0x80], PIN.as_bytes());
        assert_eq!(card.answer(&right[..right.len() - 1]), [0x69, 0x83]);
        // a command that is cut anywhere is answered, and nothing breaks
        let asked = [0x00, 0xcb, 0x3f, 0xff, 0x0a, 0x70, 0x08, 0x84, 0x01, 0x81, 0xa5, 0x03, 0x7f, 0x49, 0x80, 0x00];
        for cut in 0..asked.len() {
            assert!(card.answer(&asked[..cut]).len() >= 2, "cut at {cut}");
        }
    }

    #[test]
    fn a_card_is_made_only_with_the_certificates_own_key() {
        assert_eq!(
            Card::new(CERTIFICATE, ANOTHERS_KEY, Secret::new(PIN), "a key", [0; 16]).err(),
            Some(Error::NotACertificate("the key is not the certificate's"))
        );
        assert!(Card::new(CERTIFICATE, &KEY_OF_IT[..100], Secret::new(PIN), "a key", [0; 16]).is_err());
        assert!(Card::new(&CERTIFICATE[..100], KEY_OF_IT, Secret::new(PIN), "a key", [0; 16]).is_err());
        assert!(Card::new(CERTIFICATE, KEY_OF_IT, Secret::new(PIN), "a\\key", [0; 16]).is_err());
        assert!(Card::new(CERTIFICATE, KEY_OF_IT, Secret::new(PIN), &"k".repeat(40), [0; 16]).is_err());
    }
}
