//! Asking an authority for a certificate (PKCS #10, RFC 5652, MS-WCCE): a key that is
//! new, the request that carries its public half, and the same request signed by an
//! agent, which asks in another account's name.
//!
//! The key is made here and stays with whoever made it. The request holds its public
//! half alone, and the authority writes the account's name into the certificate itself,
//! from its directory: what the request says of a name is not what counts.

use rsa::pkcs8::{DecodePrivateKey, EncodePrivateKey, EncodePublicKey};
use rsa::traits::PublicKeyParts;
use sha2::{Digest, Sha256};

use crate::certificate::Certificate;
use crate::der;
use crate::kerberos::pkinit::Signs;
use crate::Error;

const SIGNED: &[u32] = &[1, 2, 840, 113549, 1, 7, 2];
const PLAIN: &[u32] = &[1, 2, 840, 113549, 1, 7, 1];
const KIND_OF_WHAT_IS_SIGNED: &[u32] = &[1, 2, 840, 113549, 1, 9, 3];
const HASH_OF_WHAT_IS_SIGNED: &[u32] = &[1, 2, 840, 113549, 1, 9, 4];
/// A name and a value that go with a request, as Windows' authority reads them.
const NAME_AND_VALUE: &[u32] = &[1, 3, 6, 1, 4, 1, 311, 13, 2, 1];
const SHA256: &[u32] = &[2, 16, 840, 1, 101, 3, 4, 2, 1];
const RSA_WITH_SHA256: &[u32] = &[1, 2, 840, 113549, 1, 1, 11];
const COMMON_NAME: &[u32] = &[2, 5, 4, 3];
/// A text of sixteen bits a letter, the largest byte first.
const WIDE_TEXT: u8 = 0x1e;
/// What a SHA-256 hash is written behind when RSA signs it (RFC 8017, 9.2).
const A_SHA256_HASH: [u8; 19] =
    [0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01, 0x05, 0x00, 0x04, 0x20];

/// How many bits a key has: what a Windows machine reads from a smart card.
pub const BITS: usize = 2048;

fn kind_of(numbers: &[u32]) -> Vec<u8> {
    der::sequence(&[der::name(numbers), der::write(der::NULL, &[])])
}

/// An RSA key of 2048 bits, with its private half.
pub struct Key {
    key: rsa::PrivateKey,
}

impl std::fmt::Debug for Key {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str("Key(…)")
    }
}

impl Key {
    /// A key that is new, from the system's random numbers.
    pub fn new() -> Result<Key, Error> {
        let key = rsa::PrivateKey::new(&mut getrandom::SysRng, BITS).map_err(|_| Error::NotACertificate("a new key could not be made"))?;
        Ok(Key { key })
    }

    /// A key that was written before, as PKCS #8.
    pub fn read(written: &[u8]) -> Result<Key, Error> {
        let key =
            rsa::PrivateKey::from_pkcs8_der(written).map_err(|_| Error::NotACertificate("the key is not an RSA key written as PKCS #8"))?;
        if key.n().to_be_bytes().len() * 8 != BITS {
            return Err(Error::NotACertificate("a key for signing in has 2048 bits, and this one has not"));
        }
        Ok(Key { key })
    }

    /// The key with its private half, written as PKCS #8: what a card is made with.
    pub fn written(&self) -> Result<Vec<u8>, Error> {
        Ok(self.key.to_pkcs8_der().map_err(|_| Error::NotACertificate("the key could not be written"))?.as_bytes().to_vec())
    }

    /// The public half, written the way a certificate holds a key.
    fn public(&self) -> Result<Vec<u8>, Error> {
        let written = self.key.to_public_key().to_public_key_der().map_err(|_| Error::NotACertificate("the key could not be written"))?;
        Ok(written.as_bytes().to_vec())
    }

    /// Signs a message: RSA, with SHA-256.
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error> {
        let hashed = [&A_SHA256_HASH[..], &Sha256::digest(message)].concat();
        self.key.sign(rsa::Pkcs1v15Sign::new_unprefixed(), &hashed).map_err(|_| Error::NotACertificate("the key did not sign"))
    }
}

/// The request for a certificate for this key: its public half, and the proof that
/// whoever asks holds the private one. `name` is what the request calls its subject.
pub fn asked(key: &Key, name: &str) -> Result<Vec<u8>, Error> {
    let subject =
        der::sequence(&[der::write(der::SET, &der::sequence(&[der::name(COMMON_NAME), der::write(der::UTF8_STRING, name.as_bytes())]))]);
    // nothing further is asked for: what the certificate holds is the authority's to say
    let what = der::sequence(&[der::number(0), subject, key.public()?, der::write(der::numbered(0), &[])]);
    let signature = key.sign(&what)?;
    Ok(der::sequence(&[what, kind_of(RSA_WITH_SHA256), der::bits(&signature)]))
}

/// A request in another account's name: the request, the account it is for, and an
/// agent's signature over both. `account` is written as the domain knows it,
/// `CORP\alex.urban`; the agent is a certificate that allows asking for others, with
/// its key.
pub fn in_the_name_of(asked: &[u8], account: &str, agent: &dyn Signs) -> Result<Vec<u8>, Error> {
    let certificate = Certificate::read(agent.certificate())?;
    let wide = |text: &str| der::write(WIDE_TEXT, &text.encode_utf16().flat_map(u16::to_be_bytes).collect::<Vec<u8>>());
    let named = der::sequence(&[wide("requestername"), wide(account)]);
    // what says what is signed, its hash, and whom it is for: this is what the agent signs
    let about: [Vec<u8>; 3] = [
        der::sequence(&[der::name(KIND_OF_WHAT_IS_SIGNED), der::write(der::SET, &der::name(PLAIN))]),
        der::sequence(&[der::name(HASH_OF_WHAT_IS_SIGNED), der::write(der::SET, &der::bytes(&Sha256::digest(asked)))]),
        der::sequence(&[der::name(NAME_AND_VALUE), der::write(der::SET, &named)]),
    ];
    let signature = agent.sign(&der::write(der::SET, &about.concat()))?;
    let signer = der::sequence(&[
        der::number(1),
        der::sequence(&[certificate.issuer.to_vec(), certificate.number.to_vec()]),
        kind_of(SHA256),
        der::write(der::numbered(0), &about.concat()),
        kind_of(RSA_WITH_SHA256),
        der::bytes(&signature),
    ]);
    let all = der::sequence(&[
        der::number(3),
        der::write(der::SET, &kind_of(SHA256)),
        der::sequence(&[der::name(PLAIN), der::under(0, &der::bytes(asked))]),
        der::write(der::numbered(0), certificate.whole),
        der::write(der::SET, &signer),
    ]);
    Ok(der::sequence(&[der::name(SIGNED), der::under(0, &all)]))
}

/// What an authority would read in such requests, for tests and for whatever stands in
/// for an authority.
#[cfg(any(test, feature = "stand-in"))]
pub mod stand_in {
    use super::*;
    use crate::certificate::signed_with;
    use crate::der::Reader;

    const WRONG: Error = Error::NotACertificate("the request is not built as one");

    /// What a request says.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Read {
        /// What it calls its subject.
        pub name: String,
        /// The key it is for, as a certificate holds one.
        pub key: Vec<u8>,
    }

    /// Reads a request, and checks that whoever made it holds the key it is for.
    pub fn read(asked: &[u8]) -> Result<Read, Error> {
        let mut whole = Reader::new(asked).expect(der::SEQUENCE)?.inside();
        let what = whole.value()?;
        let how = whole.expect(der::SEQUENCE)?.inside().expect(der::OBJECT_NAME)?.holds;
        let signature = whole.expect(der::BIT_STRING)?.holds.get(1..).ok_or(WRONG)?;
        let mut inside = what.inside();
        if inside.expect(der::INTEGER)?.number()? != 0 {
            return Err(WRONG);
        }
        let mut named = inside.expect(der::SEQUENCE)?.inside().expect(der::SET)?.inside().expect(der::SEQUENCE)?.inside();
        named.expect(der::OBJECT_NAME)?;
        let name = named.expect(der::UTF8_STRING)?.text()?.to_string();
        let key = inside.value()?;
        // the key itself, as RSA writes it, is what the signature is checked with
        let mut its = key.inside();
        its.expect(der::SEQUENCE)?;
        let as_rsa_writes_it = its.expect(der::BIT_STRING)?.holds.get(1..).ok_or(WRONG)?;
        if !signed_with(as_rsa_writes_it, how, what.whole, signature) {
            return Err(Error::NotTrusted("whoever made the request does not hold the key it is for"));
        }
        Ok(Read { name, key: key.whole.to_vec() })
    }

    /// What a request in another account's name says.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct InTheNameOf {
        /// The request that was signed.
        pub asked: Vec<u8>,
        /// The account it is for.
        pub account: String,
        /// The agent's certificate.
        pub agent: Vec<u8>,
    }

    /// Reads a request in another account's name, and checks the agent's signature
    /// over the request and the account.
    pub fn read_in_the_name_of(signed: &[u8]) -> Result<InTheNameOf, Error> {
        let mut outer = Reader::new(signed).expect(der::SEQUENCE)?.inside();
        if outer.expect(der::OBJECT_NAME)?.whole != der::name(SIGNED) {
            return Err(WRONG);
        }
        let mut all = outer.needed(0)?.inside();
        all.expect(der::INTEGER)?;
        all.expect(der::SET)?;
        let mut held = all.expect(der::SEQUENCE)?.inside();
        held.expect(der::OBJECT_NAME)?;
        let asked = held.needed(0)?.holds;
        let agent = all.expect(der::numbered(0))?.holds;
        let mut signer = all.expect(der::SET)?.inside().expect(der::SEQUENCE)?.inside();
        signer.expect(der::INTEGER)?;
        signer.expect(der::SEQUENCE)?;
        signer.expect(der::SEQUENCE)?;
        let about = signer.expect(der::numbered(0))?.holds;
        let how = signer.expect(der::SEQUENCE)?.inside().expect(der::OBJECT_NAME)?.holds;
        let signature = signer.expect(der::OCTET_STRING)?.holds;

        let certificate = Certificate::read(agent)?;
        let key = certificate.key.ok_or(WRONG)?;
        if !signed_with(key, how, &der::write(der::SET, about), signature) {
            return Err(Error::NotTrusted("the agent did not sign the request"));
        }
        let (mut hash, mut account) = (None, None);
        let mut about = Reader::new(about);
        while !about.is_empty() {
            let mut one = about.expect(der::SEQUENCE)?.inside();
            let kind = one.expect(der::OBJECT_NAME)?.whole;
            let mut values = one.expect(der::SET)?.inside();
            if kind == der::name(HASH_OF_WHAT_IS_SIGNED) {
                hash = Some(values.expect(der::OCTET_STRING)?.holds);
            } else if kind == der::name(NAME_AND_VALUE) {
                let mut pair = values.expect(der::SEQUENCE)?.inside();
                let wide = |holds: &[u8]| {
                    let letters: Vec<u16> = holds.chunks(2).map(|two| u16::from_be_bytes([two[0], *two.get(1).unwrap_or(&0)])).collect();
                    String::from_utf16(&letters).map_err(|_| WRONG)
                };
                if wide(pair.expect(WIDE_TEXT)?.holds)? == "requestername" {
                    account = Some(wide(pair.expect(WIDE_TEXT)?.holds)?);
                }
            }
        }
        if hash != Some(&Sha256::digest(asked)[..]) {
            return Err(Error::NotTrusted("what the agent signed is not this request"));
        }
        Ok(InTheNameOf { asked: asked.to_vec(), account: account.ok_or(WRONG)?, agent: agent.to_vec() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::certificate::tests::{ACCOUNT, ACCOUNT_KEY, STRANGER};
    use crate::kerberos::pkinit::Held;

    /// One key for all the tests here: making one takes a while.
    fn key() -> &'static Key {
        static KEY: std::sync::OnceLock<Key> = std::sync::OnceLock::new();
        KEY.get_or_init(|| Key::new().expect("a new key"))
    }

    #[test]
    fn a_new_key_has_2048_bits_and_is_read_again_as_it_was_written() {
        let written = key().written().expect("the key, written");
        let again = Key::read(&written).expect("the key, read");
        assert!(again.public().unwrap() == key().public().unwrap(), "the key that was read is the one that was written");
        assert_eq!(again.key.n().to_be_bytes().len() * 8, BITS);
        // a card takes the key as it is written here
        assert!(Key::read(ACCOUNT_KEY).is_ok());
        assert_eq!(Key::read(&written[..100]).err(), Some(Error::NotACertificate("the key is not an RSA key written as PKCS #8")));
    }

    #[test]
    fn a_request_carries_the_public_half_and_proves_that_the_private_one_is_held() {
        let asked = asked(key(), "alex.urban").expect("a request");
        let read = stand_in::read(&asked).expect("the request, read");
        assert_eq!(read.name, "alex.urban");
        assert!(read.key == key().public().unwrap(), "the key in the request is the public half");
        // a request that was changed on the way proves nothing
        let mut changed = asked.clone();
        let at = changed.len() / 2;
        changed[at] ^= 1;
        assert!(stand_in::read(&changed).is_err());
    }

    #[test]
    fn an_agent_signs_a_request_in_another_accounts_name() {
        let agent = Held::new(ACCOUNT, ACCOUNT_KEY).expect("an agent");
        let asked = asked(key(), "alex.urban").expect("a request");
        let signed = in_the_name_of(&asked, "CORP\\alex.urban", &agent).expect("a request in another's name");
        let read = stand_in::read_in_the_name_of(&signed).expect("the request, read");
        assert!(read.asked == asked, "the request that was signed is the one that was made");
        assert_eq!(read.account, "CORP\\alex.urban");
        assert!(read.agent == ACCOUNT, "the agent's certificate goes with it");

        // whoever changes the account on the way is noticed
        let was: Vec<u8> = "CORP\\alex.urban".encode_utf16().flat_map(u16::to_be_bytes).collect();
        let now: Vec<u8> = "CORP\\root.urban".encode_utf16().flat_map(u16::to_be_bytes).collect();
        let at = signed.windows(was.len()).position(|part| part == was).expect("the account is in it");
        let mut changed = signed.clone();
        changed[at..at + now.len()].copy_from_slice(&now);
        assert_eq!(stand_in::read_in_the_name_of(&changed).err(), Some(Error::NotTrusted("the agent did not sign the request")));
    }

    #[test]
    fn an_agent_needs_a_certificate_that_can_be_read() {
        struct Nobody;
        impl Signs for Nobody {
            fn certificate(&self) -> &[u8] {
                &STRANGER[..40]
            }
            fn sign(&self, _: &[u8]) -> Result<Vec<u8>, Error> {
                Ok(Vec::new())
            }
        }
        assert!(in_the_name_of(b"a request", "CORP\\alex.urban", &Nobody).is_err());
    }
}
