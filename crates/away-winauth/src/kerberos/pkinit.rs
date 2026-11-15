//! Proving who asks for a ticket with a certificate in place of a password (RFC 4556,
//! MS-PKCA). The client signs what it asks for with the certificate's key and sends a
//! number of an exchange of keys; the domain controller signs its own number in turn.
//! What both then know, and nobody who listened, is the key the answer is sealed with.

use num_bigint::BigUint;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha384, Sha512};

use crate::certificate::{self, Certificate};
use crate::der::{self, Reader, Value};
use crate::Error;

use super::crypto::{Key, Kind};
use super::messages::{Extra, WITH_A_CERTIFICATE};

const WRONG: Error = Error::NotKerberos("the domain controller's proof with a certificate is not built as one");

const SIGNED: &[u32] = &[1, 2, 840, 113549, 1, 7, 2];
const WHAT_THE_CLIENT_SIGNS: &[u32] = &[1, 3, 6, 1, 5, 2, 3, 1];
const WHAT_THE_CONTROLLER_SIGNS: &[u32] = &[1, 3, 6, 1, 5, 2, 3, 2];
const KIND_OF_WHAT_IS_SIGNED: &[u32] = &[1, 2, 840, 113549, 1, 9, 3];
const HASH_OF_WHAT_IS_SIGNED: &[u32] = &[1, 2, 840, 113549, 1, 9, 4];
const SHA1: &[u32] = &[1, 3, 14, 3, 2, 26];
const SHA256: &[u32] = &[2, 16, 840, 1, 101, 3, 4, 2, 1];
const SHA384: &[u32] = &[2, 16, 840, 1, 101, 3, 4, 2, 2];
const SHA512: &[u32] = &[2, 16, 840, 1, 101, 3, 4, 2, 3];
const RSA: &[u32] = &[1, 2, 840, 113549, 1, 1, 1];
const EXCHANGE_OF_KEYS: &[u32] = &[1, 2, 840, 10046, 2, 1];

/// The numbers both sides do the exchange of keys with (RFC 3526, group 14): a prime of
/// 2048 bits, and the number 2.
const PRIME: &str = "\
FFFFFFFFFFFFFFFFC90FDAA22168C234C4C6628B80DC1CD129024E088A67CC74020BBEA63B139B22514A08798E3404DD\
EF9519B3CD3A431B302B0A6DF25F14374FE1356D6D51C245E485B576625E7EC6F44C42E9A637ED6B0BFF5CB6F406B7ED\
EE386BFB5A899FA5AE9F24117C4B1FE649286651ECE45B3DC2007CB8A163BF0598DA48361C55D39A69163FA8FD24CF5F\
83655D23DCA3AD961C62F356208552BB9ED529077096966D670C354E4ABC9804F1746C08CA18217C32905E462E36CE3B\
E39E772C180E86039B2783A2EC07A28FB5C55DF06F4C52C9DE2BCBF6955817183995497CEA956AE515D2261898FA0510\
15728E5A8AACAA68FFFFFFFFFFFFFFFF";

fn prime() -> BigUint {
    BigUint::parse_bytes(PRIME.as_bytes(), 16).expect("the prime, as it is written")
}

/// What holds a certificate's key, and signs with it.
pub trait Signs {
    /// The certificate.
    fn certificate(&self) -> &[u8];
    /// Signs a message with the certificate's key: RSA, with SHA-256.
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error>;
}

/// A certificate with its key, as files hold them.
pub struct Held {
    certificate: Vec<u8>,
    key: ring::rsa::KeyPair,
}

impl Held {
    /// A certificate and its RSA key. The key is read as either of the two ways such a
    /// key is written.
    pub fn new(certificate: &[u8], key: &[u8]) -> Result<Held, Error> {
        Certificate::read(certificate)?;
        let key = ring::rsa::KeyPair::from_pkcs8(key)
            .or_else(|_| ring::rsa::KeyPair::from_der(key))
            .map_err(|_| Error::NotACertificate("its key is no RSA key that can be read"))?;
        Ok(Held { certificate: certificate.to_vec(), key })
    }
}

impl Signs for Held {
    fn certificate(&self) -> &[u8] {
        &self.certificate
    }

    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error> {
        let mut signature = vec![0; self.key.public().modulus_len()];
        self.key
            .sign(&ring::signature::RSA_PKCS1_SHA256, &ring::rand::SystemRandom::new(), message, &mut signature)
            .map_err(|_| Error::NotACertificate("its key did not sign"))?;
        Ok(signature)
    }
}

/// What is new for one proof: the client's secret number of the exchange of keys, and a
/// number used once that goes into the key.
pub struct Fresh {
    /// The secret number.
    pub secret: [u8; 32],
    /// The number used once.
    pub once: [u8; 32],
}

fn kind_of(numbers: &[u32], with_nothing: bool) -> Vec<u8> {
    match with_nothing {
        true => der::sequence(&[der::name(numbers), der::write(der::NULL, &[])]),
        false => der::sequence(&[der::name(numbers)]),
    }
}

/// The proof that goes with the first request: what is asked, the time and a number used
/// once, signed with the certificate's key, and the client's number of the exchange of
/// keys. `asked` is the part of the request that says what is asked.
pub fn proof(signs: &dyn Signs, asked: &[u8], time: (u64, u32), nonce: u32, fresh: &Fresh) -> Result<Extra, Error> {
    let certificate = Certificate::read(signs.certificate())?;
    let prime = prime();
    let public = BigUint::from(2u8).modpow(&BigUint::from_bytes_be(&fresh.secret), &prime);
    let half: BigUint = (&prime - 1u8) >> 1usize;

    // what is asked is bound to the proof twice: with the old hash, and with the one that
    // domain controllers of today ask for (MS-PKCA 2.2.3)
    let second_check = der::sequence(&[der::under(0, &der::bytes(&Sha256::digest(asked))), der::under(1, &kind_of(SHA256, false))]);
    let who_and_when = der::sequence(&[
        der::under(0, &der::number(i64::from(time.1))),
        der::under(1, &der::time(time.0)),
        der::under(2, &der::number(i64::from(nonce))),
        der::under(3, &der::bytes(&Sha1::digest(asked))),
        der::under(5, &second_check),
    ]);
    let numbers = der::sequence(&[der::large_number(&prime.to_bytes_be()), der::number(2), der::large_number(&half.to_bytes_be())]);
    let its_number =
        der::sequence(&[der::sequence(&[der::name(EXCHANGE_OF_KEYS), numbers]), der::bits(&der::large_number(&public.to_bytes_be()))]);
    let signed = der::sequence(&[der::under(0, &who_and_when), der::under(1, &its_number), der::under(3, &der::bytes(&fresh.once))]);

    // what says what is signed and its hash: this is what the key signs
    let about: [Vec<u8>; 2] = [
        der::sequence(&[der::name(KIND_OF_WHAT_IS_SIGNED), der::write(der::SET, &der::name(WHAT_THE_CLIENT_SIGNS))]),
        der::sequence(&[der::name(HASH_OF_WHAT_IS_SIGNED), der::write(der::SET, &der::bytes(&Sha256::digest(&signed)))]),
    ];
    let signature = signs.sign(&der::write(der::SET, &about.concat()))?;
    let signer = der::sequence(&[
        der::number(1),
        der::sequence(&[certificate.issuer.to_vec(), certificate.number.to_vec()]),
        kind_of(SHA256, true),
        der::write(der::numbered(0), &about.concat()),
        kind_of(RSA, true),
        der::bytes(&signature),
    ]);
    let all = der::sequence(&[
        der::number(3),
        der::write(der::SET, &kind_of(SHA256, true)),
        der::sequence(&[der::name(WHAT_THE_CLIENT_SIGNS), der::under(0, &der::bytes(&signed))]),
        der::write(der::numbered(0), certificate.whole),
        der::write(der::SET, &signer),
    ]);
    let wrapped = der::sequence(&[der::name(SIGNED), der::under(0, &all)]);
    Ok(Extra { kind: WITH_A_CERTIFICATE, holds: der::sequence(&[der::write(der::numbered_plain(0), &wrapped)]) })
}

/// What the client trusts when a domain controller proves itself.
pub struct Trusts<'a> {
    /// The authorities whose certificates a domain controller's has to lead to.
    pub authorities: &'a [&'a [u8]],
    /// The name of the domain controller that was asked: its certificate has to be for it.
    pub controller: &'a str,
}

fn hash(kind: &[u8], message: &[u8]) -> Option<(Vec<u8>, u32)> {
    let named = |numbers: &[u32]| der::name(numbers)[2..].to_vec();
    Some(match kind {
        kind if kind == named(SHA256) => (Sha256::digest(message).to_vec(), 11),
        kind if kind == named(SHA384) => (Sha384::digest(message).to_vec(), 12),
        kind if kind == named(SHA512) => (Sha512::digest(message).to_vec(), 13),
        kind if kind == named(SHA1) => (Sha1::digest(message).to_vec(), 5),
        _ => return None,
    })
}

/// What a domain controller signed in its answer: the bytes, once the signature, the
/// certificate it was made with and what that certificate is for have been checked.
fn signed_by_a_controller<'a>(wrapped: &'a [u8], trusts: &Trusts, now: u64) -> Result<&'a [u8], Error> {
    let not_trusted = |why: &'static str| Error::NotTrusted(why);
    let mut outside = Reader::new(wrapped).expect(der::SEQUENCE)?.inside();
    if outside.expect(der::OBJECT_NAME)?.holds != &der::name(SIGNED)[2..] {
        return Err(WRONG);
    }
    let mut all = outside.expect(der::numbered(0))?.inside().expect(der::SEQUENCE)?.inside();
    all.expect(der::INTEGER)?;
    all.expect(der::SET)?;
    let mut what = all.expect(der::SEQUENCE)?.inside();
    if what.expect(der::OBJECT_NAME)?.holds != &der::name(WHAT_THE_CONTROLLER_SIGNS)[2..] {
        return Err(WRONG);
    }
    let signed = what.expect(der::numbered(0))?.inside().expect(der::OCTET_STRING)?.holds;
    // the certificates that came with it
    let mut came_with = Vec::new();
    if let Some(listed) = all.optional(der::numbered(0))? {
        let mut listed = listed.inside();
        while !listed.is_empty() {
            let one = listed.value()?;
            // what is no certificate in the list is passed over
            if let Ok(certificate) = Certificate::read(one.whole) {
                came_with.push(certificate);
            }
        }
    }
    all.optional(der::numbered(1))?;
    let mut signer = all.expect(der::SET)?.inside().expect(der::SEQUENCE)?.inside();
    signer.expect(der::INTEGER)?;
    let by = signer.expect(der::SEQUENCE)?;
    let hashed_how = signer.expect(der::SEQUENCE)?.inside().expect(der::OBJECT_NAME)?.holds;
    let about = signer.optional(der::numbered(0))?;
    let signed_how = signer.expect(der::SEQUENCE)?.inside().expect(der::OBJECT_NAME)?.holds;
    let signature = signer.expect(der::OCTET_STRING)?.holds;

    // whose signature it is: the certificate that its issuer and its number name
    let certificate = came_with
        .iter()
        .find(|certificate| by.holds == [certificate.issuer, certificate.number].concat())
        .ok_or(not_trusted("the certificate it signed with did not come with it"))?;
    let key = certificate.key.ok_or(not_trusted("its certificate has no RSA key"))?;
    let (digest, with_rsa) = hash(hashed_how, signed).ok_or(not_trusted("it hashed what it signed in a way that is not spoken here"))?;
    // the signature names RSA alone, and the hash apart, or both in one
    let how = match signed_how == &der::name(RSA)[2..] {
        true => der::name(&[1, 2, 840, 113549, 1, 1, with_rsa])[2..].to_vec(),
        false => signed_how.to_vec(),
    };
    let message = match about {
        // what is signed is what says the hash of the bytes: the hash has to be theirs
        Some(about) => {
            let mut says_the_hash = false;
            let mut attributes = about.inside();
            while !attributes.is_empty() {
                let mut attribute = attributes.expect(der::SEQUENCE)?.inside();
                if attribute.expect(der::OBJECT_NAME)?.holds == &der::name(HASH_OF_WHAT_IS_SIGNED)[2..] {
                    says_the_hash = attribute.expect(der::SET)?.inside().expect(der::OCTET_STRING)?.holds == digest;
                }
            }
            if !says_the_hash {
                return Err(not_trusted("what it signed is not what it sent"));
            }
            der::write(der::SET, about.holds)
        }
        None => signed.to_vec(),
    };
    if !certificate::signed_with(key, &how, &message, signature) {
        return Err(not_trusted("its signature does not hold"));
    }

    let trusted: Vec<Certificate> = trusts.authorities.iter().filter_map(|authority| Certificate::read(authority).ok()).collect();
    if !certificate::leads_to(certificate, &came_with, &trusted, now) {
        return Err(not_trusted("its certificate leads to no authority that is trusted, or is good no longer"));
    }
    if !certificate.is_for(certificate::FOR_A_DOMAIN_CONTROLLER) {
        return Err(not_trusted("its certificate is not one for a domain controller"));
    }
    if !certificate.machines.iter().any(|machine| machine.eq_ignore_ascii_case(trusts.controller)) {
        return Err(not_trusted("its certificate is for another machine than the one that was asked"));
    }
    Ok(signed)
}

fn expect(value: Value, kind: u8) -> Result<Value, Error> {
    if value.kind == kind {
        Ok(value)
    } else {
        Err(WRONG)
    }
}

/// The key the answer is sealed with, from the domain controller's part of the proof:
/// its number of the exchange of keys, which it signed with its certificate. `holds` is
/// what goes with the answer under [`super::messages::WITH_A_CERTIFICATE_ANSWER`].
pub fn key(holds: &[u8], kind: Kind, nonce: u32, fresh: &Fresh, trusts: &Trusts, now: u64) -> Result<Key, Error> {
    let mut outside = Reader::new(holds);
    let Some(with_numbers) = outside.optional(der::numbered(0))? else {
        // the other way, in which the key itself travels sealed, is one a client asks for
        return Err(Error::NotKerberos("the domain controller answered in a way the client did not ask for"));
    };
    let mut parts = with_numbers.inside().expect(der::SEQUENCE)?.inside();
    let wrapped = parts.expect(der::numbered_plain(0))?.holds;
    let theirs_once = parts.under(1)?.map(|once| expect(once, der::OCTET_STRING).map(|once| once.holds)).transpose()?;

    let signed = signed_by_a_controller(wrapped, trusts, now)?;
    let mut signed = Reader::new(signed).expect(der::SEQUENCE)?.inside();
    let its_number = expect(signed.needed(0)?, der::BIT_STRING)?.holds.get(1..).ok_or(WRONG)?;
    let its_number = BigUint::from_bytes_be(Reader::new(its_number).expect(der::INTEGER)?.holds);
    if signed.needed(1)?.number()? != i64::from(nonce) {
        return Err(Error::NotTrusted("it answered another request than the one that was made"));
    }
    // a number that would give the key away is none
    let prime = prime();
    if its_number < BigUint::from(2u8) || its_number > &prime - 2u8 {
        return Err(Error::NotTrusted("its number of the exchange of keys is none"));
    }
    let shared = its_number.modpow(&BigUint::from_bytes_be(&fresh.secret), &prime).to_bytes_be();
    // as long as the prime is, with noughts in front
    let mut from = vec![0u8; 256 - shared.len()];
    from.extend(shared);
    // with the numbers used once of both sides, where the domain controller sent its own
    if let Some(theirs_once) = theirs_once {
        from.extend_from_slice(&fresh.once);
        from.extend_from_slice(theirs_once);
    }
    Ok(Key::from_shared(kind, &from))
}

/// The domain controller's side of the proof, for the tests' stand-in for one.
#[cfg(test)]
pub(crate) mod stand_in {
    use super::*;

    /// What a client's proof says, once its signature has been checked with the
    /// certificate that came with it.
    pub(crate) struct Proven<'a> {
        pub certificate: Certificate<'a>,
        pub time: (u64, u32),
        pub nonce: u32,
        pub check: &'a [u8],
        pub second_check: &'a [u8],
        pub its_number: BigUint,
        pub once: &'a [u8],
    }

    pub(crate) fn read_proof(extra: &Extra) -> Result<Proven<'_>, Error> {
        let wrapped = Reader::new(&extra.holds).expect(der::SEQUENCE)?.inside().expect(der::numbered_plain(0))?.holds;
        let mut outside = Reader::new(wrapped).expect(der::SEQUENCE)?.inside();
        outside.expect(der::OBJECT_NAME)?;
        let mut all = outside.expect(der::numbered(0))?.inside().expect(der::SEQUENCE)?.inside();
        all.expect(der::INTEGER)?;
        all.expect(der::SET)?;
        let mut what = all.expect(der::SEQUENCE)?.inside();
        what.expect(der::OBJECT_NAME)?;
        let signed = what.expect(der::numbered(0))?.inside().expect(der::OCTET_STRING)?.holds;
        let certificate = Certificate::read(all.expect(der::numbered(0))?.holds)?;
        let mut signer = all.expect(der::SET)?.inside().expect(der::SEQUENCE)?.inside();
        signer.expect(der::INTEGER)?;
        signer.expect(der::SEQUENCE)?;
        signer.expect(der::SEQUENCE)?;
        let about = signer.expect(der::numbered(0))?;
        signer.expect(der::SEQUENCE)?;
        let signature = signer.expect(der::OCTET_STRING)?.holds;
        let how = der::name(&[1, 2, 840, 113549, 1, 1, 11])[2..].to_vec();
        let key = certificate.key.ok_or(WRONG)?;
        if !certificate::signed_with(key, &how, &der::write(der::SET, about.holds), signature)
            || !about.holds.windows(32).any(|part| part == Sha256::digest(signed).as_slice())
        {
            return Err(Error::NotTrusted("the client's signature does not hold"));
        }

        let mut signed = Reader::new(signed).expect(der::SEQUENCE)?.inside();
        let mut who_and_when = signed.needed(0)?.inside();
        let millionths = who_and_when.needed(0)?.number()? as u32;
        let seconds = der::read_time(who_and_when.needed(1)?.holds)?;
        let nonce = who_and_when.needed(2)?.number()? as u32;
        let check = who_and_when.needed(3)?.holds;
        let second_check = who_and_when.needed(5)?.inside().needed(0)?.holds;
        let mut its_number = signed.needed(1)?.inside();
        its_number.expect(der::SEQUENCE)?;
        let its_number = its_number.expect(der::BIT_STRING)?.holds.get(1..).ok_or(WRONG)?;
        let its_number = BigUint::from_bytes_be(Reader::new(its_number).expect(der::INTEGER)?.holds);
        let once = signed.needed(3)?.holds;
        Ok(Proven { certificate, time: (seconds, millionths), nonce, check, second_check, its_number, once })
    }

    /// A domain controller's part of the proof: its number of the exchange of keys and
    /// the request's number, signed with its certificate; and the key that follows.
    pub(crate) fn answer(
        proven: &Proven,
        controller: &Held,
        secret: &[u8; 32],
        once: Option<&[u8; 32]>,
        kind: Kind,
        hash_alone: bool,
    ) -> (Vec<u8>, Key) {
        let prime = prime();
        let public = BigUint::from(2u8).modpow(&BigUint::from_bytes_be(secret), &prime);
        let signed = der::sequence(&[
            der::under(0, &der::bits(&der::large_number(&public.to_bytes_be()))),
            der::under(1, &der::number(i64::from(proven.nonce))),
        ]);
        let certificate = Certificate::read(controller.certificate()).expect("the controller's certificate");
        let about: [Vec<u8>; 2] = [
            der::sequence(&[der::name(KIND_OF_WHAT_IS_SIGNED), der::write(der::SET, &der::name(WHAT_THE_CONTROLLER_SIGNS))]),
            der::sequence(&[der::name(HASH_OF_WHAT_IS_SIGNED), der::write(der::SET, &der::bytes(&Sha256::digest(&signed)))]),
        ];
        // signed as a whole, or with what says its hash in between
        let (about_written, message) = match hash_alone {
            true => (Vec::new(), signed.clone()),
            false => (der::write(der::numbered(0), &about.concat()), der::write(der::SET, &about.concat())),
        };
        let signer = der::sequence(&[
            der::number(1),
            der::sequence(&[certificate.issuer.to_vec(), certificate.number.to_vec()]),
            kind_of(SHA256, true),
            about_written,
            kind_of(if hash_alone { &[1, 2, 840, 113549, 1, 1, 11] } else { RSA }, true),
            der::bytes(&controller.sign(&message).expect("the controller signs")),
        ]);
        let all = der::sequence(&[
            der::number(3),
            der::write(der::SET, &kind_of(SHA256, true)),
            der::sequence(&[der::name(WHAT_THE_CONTROLLER_SIGNS), der::under(0, &der::bytes(&signed))]),
            der::write(der::numbered(0), certificate.whole),
            der::write(der::SET, &signer),
        ]);
        let wrapped = der::sequence(&[der::name(SIGNED), der::under(0, &all)]);
        let mut parts = vec![der::write(der::numbered_plain(0), &wrapped)];
        parts.extend(once.map(|once| der::under(1, &der::bytes(once))));
        let shared = proven.its_number.modpow(&BigUint::from_bytes_be(secret), &prime).to_bytes_be();
        let mut from = vec![0u8; 256 - shared.len()];
        from.extend(shared);
        if let Some(once) = once {
            from.extend_from_slice(proven.once);
            from.extend_from_slice(once);
        }
        (der::under(0, &der::sequence(&parts)), Key::from_shared(kind, &from))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::certificate::tests::{ACCOUNT, ACCOUNT_KEY, AUTHORITY, CONTROLLER, NOW, STRANGER};

    const CONTROLLER_KEY: &[u8] = include_bytes!("../../tests/fixtures/controller.key.der");

    fn fresh() -> Fresh {
        Fresh { secret: [0x42; 32], once: [0x17; 32] }
    }

    #[test]
    fn the_client_signs_what_it_asks_for_and_both_sides_come_to_the_same_key() {
        let account = Held::new(ACCOUNT, ACCOUNT_KEY).unwrap();
        let controller = Held::new(CONTROLLER, CONTROLLER_KEY).unwrap();
        let asked = b"what is asked, as it is written in the request";
        let extra = proof(&account, asked, (NOW, 654_321), 0x1234_5678, &fresh()).unwrap();
        assert_eq!(extra.kind, 16);

        // a domain controller reads it: who signed, when, for which request, and bound to what is asked
        let proven = stand_in::read_proof(&extra).unwrap();
        assert_eq!(proven.certificate.account.as_deref(), Some("alex.urban@CORP.EXAMPLE"));
        assert_eq!((proven.time, proven.nonce, proven.once), ((NOW, 654_321), 0x1234_5678, &[0x17; 32][..]));
        assert_eq!(proven.check, Sha1::digest(asked).as_slice());
        assert_eq!(proven.second_check, Sha256::digest(asked).as_slice());

        let trusts = Trusts { authorities: &[AUTHORITY], controller: "DC-01.corp.example" };
        for (once, hash_alone, kind) in
            [(Some(&[0x99; 32]), false, Kind::Aes256), (None, false, Kind::Aes128), (Some(&[0x99; 32]), true, Kind::Aes256)]
        {
            let (answer, theirs) = stand_in::answer(&proven, &controller, &[0x24; 32], once, kind, hash_alone);
            let ours = key(&answer, kind, 0x1234_5678, &fresh(), &trusts, NOW).unwrap();
            assert_eq!(ours, theirs, "with a number of its own: {}, signed as a whole: {hash_alone}", once.is_some());
            // another secret gives another key, which opens nothing of theirs
            let other = key(&answer, kind, 0x1234_5678, &Fresh { secret: [0x43; 32], once: [0x17; 32] }, &trusts, NOW).unwrap();
            assert_ne!(other, theirs);
        }
    }

    #[test]
    fn a_domain_controller_that_does_not_prove_itself_is_not_believed() {
        let account = Held::new(ACCOUNT, ACCOUNT_KEY).unwrap();
        let controller = Held::new(CONTROLLER, CONTROLLER_KEY).unwrap();
        let extra = proof(&account, b"asked", (NOW, 0), 7, &fresh()).unwrap();
        let proven = stand_in::read_proof(&extra).unwrap();
        let (answer, _) = stand_in::answer(&proven, &controller, &[0x24; 32], None, Kind::Aes256, false);
        let with = |authorities: &[&[u8]], name: &str, nonce: u32, now: u64, answer: &[u8]| {
            key(answer, Kind::Aes256, nonce, &fresh(), &Trusts { authorities, controller: name }, now)
        };
        assert!(with(&[AUTHORITY], "localhost", 7, NOW, &answer).is_ok());
        let refused: [(Result<Key, Error>, &str); 6] = [
            (
                with(&[STRANGER], "localhost", 7, NOW, &answer),
                "its certificate leads to no authority that is trusted, or is good no longer",
            ),
            (with(&[], "localhost", 7, NOW, &answer), "its certificate leads to no authority that is trusted, or is good no longer"),
            (
                with(&[AUTHORITY], "localhost", 7, 1_000_000_000, &answer),
                "its certificate leads to no authority that is trusted, or is good no longer",
            ),
            (
                with(&[AUTHORITY], "dc-02.corp.example", 7, NOW, &answer),
                "its certificate is for another machine than the one that was asked",
            ),
            (with(&[AUTHORITY], "localhost", 8, NOW, &answer), "it answered another request than the one that was made"),
            // signed by somebody who is no domain controller: the account's own certificate
            (
                with(&[AUTHORITY], "localhost", 7, NOW, &stand_in::answer(&proven, &account, &[0x24; 32], None, Kind::Aes256, false).0),
                "its certificate is not one for a domain controller",
            ),
        ];
        for (result, why) in refused {
            assert_eq!(result, Err(Error::NotTrusted(why)));
        }
        // a byte changed in what it signed, or in its signature
        let at = answer.windows(4).position(|part| part == [0x02, 0x01, 0x07, 0xa0] || part[..3] == [0x02, 0x01, 0x07]).unwrap();
        let mut changed = answer.clone();
        changed[at + 2] = 9;
        assert_eq!(with(&[AUTHORITY], "localhost", 7, NOW, &changed), Err(Error::NotTrusted("what it signed is not what it sent")));
        let mut changed = answer.clone();
        let last = changed.len() - 1;
        changed[last] ^= 1;
        assert_eq!(with(&[AUTHORITY], "localhost", 7, NOW, &changed), Err(Error::NotTrusted("its signature does not hold")));
        // cut anywhere, it is refused
        for cut in 0..answer.len() {
            assert!(with(&[AUTHORITY], "localhost", 7, NOW, &answer[..cut]).is_err(), "cut at {cut}");
        }
        // and a certificate that is no certificate, or a key that is none, holds nothing
        assert!(Held::new(b"nothing", ACCOUNT_KEY).is_err() && Held::new(ACCOUNT, b"nothing").is_err());
    }
}
