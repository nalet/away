//! The keys the secrets Away keeps for machines are sealed to, and the opening of them
//! ([ADR-0017](../../../spec/architecture/decisions/0017-away-keeps-the-secrets-of-machines-sealed.md)).
//!
//! The installation gives the issuer one key or more, as PEM: the first is the one new
//! secrets are sealed to, and the others still open what was sealed to them, so that a
//! key can be replaced without a secret being lost. Every issuer of an installation is
//! given the same keys, so that any of them opens any secret.

use away_part::{base64, sealed};
use away_secret::Secret;
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::pkcs8::{DecodePrivateKey, EncodePublicKey};
use rsa::sha2::Sha256;
use rsa::traits::PublicKeyParts;
use rsa::Oaep;

/// The fewest bits a keeping key has.
const FEWEST_BITS: usize = 2048;

/// One key: its name, its private half, and its public half as DER.
struct Key {
    name: [u8; sealed::KEY_NAME],
    private: rsa::PrivateKey,
    public: Vec<u8>,
}

/// The keys the issuer opens sealed secrets with.
pub struct Keeping {
    keys: Vec<Key>,
}

/// Why a sealed secret did not open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotOpened {
    /// It is not built as one.
    NotSealed,
    /// It is sealed to a key the issuer doesn't hold.
    OtherKey,
    /// It doesn't open with the key: changed, or sealed under another name.
    Broken,
}

impl Keeping {
    /// Reads the keys from PEM: RSA keys, as PKCS #1 or PKCS #8, of 2048 bits or more.
    pub fn read(pem: &[u8]) -> Result<Keeping, String> {
        let text = std::str::from_utf8(pem).map_err(|_| "the keys are no text".to_string())?;
        let mut keys = Vec::new();
        let mut rest = text;
        while let Some(begins) = rest.find("-----BEGIN ") {
            let after = &rest[begins + 11..];
            let label_ends = after.find("-----").ok_or("a key's first line is not whole")?;
            let label = &after[..label_ends];
            let ends = format!("-----END {label}-----");
            let body_and_rest = &after[label_ends + 5..];
            let body_ends = body_and_rest.find(&ends).ok_or_else(|| format!("the {label} has no end"))?;
            let letters: String = body_and_rest[..body_ends].chars().filter(|letter| !letter.is_whitespace()).collect();
            let der = base64::read_for_addresses(&letters.replace('+', "-").replace('/', "_").replace('=', ""))
                .ok_or_else(|| format!("the {label} is not written in Base64"))?;
            let private = match label {
                "RSA PRIVATE KEY" => rsa::PrivateKey::from_pkcs1_der(&der).map_err(|_| "a key is not an RSA key as PKCS #1".to_string())?,
                "PRIVATE KEY" => rsa::PrivateKey::from_pkcs8_der(&der).map_err(|_| "a key is not an RSA key as PKCS #8".to_string())?,
                other => return Err(format!("the keys hold a {other}, which is no RSA private key")),
            };
            if private.size() * 8 < FEWEST_BITS {
                return Err(format!("a key has fewer than {FEWEST_BITS} bits"));
            }
            let public = private
                .to_public_key()
                .to_public_key_der()
                .map_err(|_| "a key's public half could not be written".to_string())?
                .as_bytes()
                .to_vec();
            keys.push(Key { name: sealed::key_name(&public), private, public });
            rest = &body_and_rest[body_ends + ends.len()..];
        }
        if keys.is_empty() {
            return Err("there is no key".to_string());
        }
        Ok(Keeping { keys })
    }

    /// The key new secrets are sealed to: its name, and its public half as DER.
    pub fn current(&self) -> ([u8; sealed::KEY_NAME], &[u8]) {
        (self.keys[0].name, &self.keys[0].public)
    }

    /// Opens a sealed secret that was kept under this name.
    pub fn open(&self, sealed: &[u8], name: &str) -> Result<Secret, NotOpened> {
        let parts = sealed::read(sealed).ok_or(NotOpened::NotSealed)?;
        let key = self.keys.iter().find(|key| key.name == parts.key).ok_or(NotOpened::OtherKey)?;
        // blinded, so that how long it takes says nothing of the key
        let mut opened =
            key.private.decrypt_blinded(&mut getrandom::SysRng, Oaep::<Sha256>::new(), parts.wrapped).map_err(|_| NotOpened::Broken)?;
        let unbound = UnboundKey::new(&AES_256_GCM, &opened);
        // the key of this one secret is not kept longer than it is used
        opened.iter_mut().for_each(|byte| *byte = 0);
        let unbound = unbound.map_err(|_| NotOpened::Broken)?;
        let mut held = parts.sealed.to_vec();
        let secret = LessSafeKey::new(unbound)
            .open_in_place(Nonce::assume_unique_for_key(parts.nonce), Aad::from(sealed::bound_to(name)), &mut held)
            .map_err(|_| NotOpened::Broken)?;
        let secret = String::from_utf8(secret.to_vec()).map_err(|_| NotOpened::Broken)?;
        held.iter_mut().for_each(|byte| *byte = 0);
        Ok(Secret::new(secret))
    }
}

/// Seals a secret to a public key, as an administrator's browser does: for the tests.
#[cfg(any(test, feature = "stand-in"))]
pub fn seal(public: &[u8], name: &str, secret: &str) -> Vec<u8> {
    use ring::aead::NONCE_LEN;
    use rsa::pkcs8::DecodePublicKey;
    let key = rsa::PublicKey::from_public_key_der(public).expect("a public key");
    let mut once = [0u8; 32 + NONCE_LEN];
    getrandom::fill(&mut once).expect("random bytes");
    let wrapped = key.encrypt(&mut getrandom::SysRng, Oaep::<Sha256>::new(), &once[..32]).expect("the key wrapped");
    let sealing = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, &once[..32]).expect("a key"));
    let nonce: [u8; NONCE_LEN] = once[32..].try_into().expect("12 bytes");
    let mut held = secret.as_bytes().to_vec();
    sealing.seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::from(sealed::bound_to(name)), &mut held).expect("sealed");
    sealed::write(&sealed::Sealed { key: sealed::key_name(public), wrapped: &wrapped, nonce, sealed: &held })
}

/// A new keeping key, as PEM (PKCS #8): for the tests.
#[cfg(any(test, feature = "stand-in"))]
pub fn new_key(bits: usize) -> String {
    use rsa::pkcs8::EncodePrivateKey;
    let key = rsa::PrivateKey::new(&mut getrandom::SysRng, bits).expect("a key");
    let der = key.to_pkcs8_der().expect("written");
    let letters = base64::ordinary(der.as_bytes());
    let lines: Vec<&str> = letters.as_bytes().chunks(64).map(|line| std::str::from_utf8(line).expect("letters")).collect();
    format!("-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_sealed_to_the_issuers_key_opens_with_it_under_its_name_alone() {
        let (first, second) = (new_key(2048), new_key(2048));
        let keeping = Keeping::read(format!("{first}{second}").as_bytes()).expect("two keys");
        let (name, public) = keeping.current();
        assert_eq!(name, sealed::key_name(public));
        let kept = seal(public, "mac-01", "the desktop's password");
        assert_eq!(keeping.open(&kept, "mac-01").expect("it opens").expose(), "the desktop's password");
        // under another name, changed, or not sealed at all, it doesn't open
        assert_eq!(keeping.open(&kept, "mac-02").err(), Some(NotOpened::Broken));
        let mut changed = kept.clone();
        *changed.last_mut().unwrap() ^= 1;
        assert_eq!(keeping.open(&changed, "mac-01").err(), Some(NotOpened::Broken));
        assert_eq!(keeping.open(b"the password itself", "mac-01").err(), Some(NotOpened::NotSealed));
        // a secret sealed to the second key opens too: the key it was sealed to is kept on
        let older = Keeping::read(second.as_bytes()).unwrap();
        let kept_before = seal(older.current().1, "mac-01", "an older password");
        assert_eq!(keeping.open(&kept_before, "mac-01").expect("it opens").expose(), "an older password");
        // and one sealed to a key the issuer doesn't hold doesn't
        let other = Keeping::read(new_key(2048).as_bytes()).unwrap();
        assert_eq!(keeping.open(&seal(other.current().1, "mac-01", "x"), "mac-01").err(), Some(NotOpened::OtherKey));
        // what holds no key, or no RSA key, or one too small, is refused
        assert!(Keeping::read(b"").is_err());
        assert!(Keeping::read(b"-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n").is_err());
        assert!(Keeping::read(new_key(1024).as_bytes()).is_err());
    }

    #[test]
    fn what_a_browser_sealed_with_its_own_cryptography_opens_here() {
        // sealed as Away's page seals a password, with the WebCrypto of a browser, to the
        // key the tests keep beside the server's fixtures
        let keeping = Keeping::read(include_bytes!("../../away-server/tests/fixtures/keeping.key.pem")).expect("the tests' key");
        let sealed = include_bytes!("../tests/fixtures/sealed-by-a-browser.bin");
        assert_eq!(keeping.open(sealed, "mac-02").expect("it opens").expose(), "a desktop, sealed by a browser");
        assert_eq!(keeping.open(sealed, "mac-01").err(), Some(NotOpened::Broken));
    }
}
