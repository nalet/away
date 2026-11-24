//! A secret sealed to the issuer, as Away keeps it: what an administrator's browser
//! made of a machine's password before it left the browser. Only the issuer, which
//! holds the key it is sealed to, opens it; the server and the database keep it as it
//! is ([ADR-0017](../../../spec/architecture/decisions/0017-away-keeps-the-secrets-of-machines-sealed.md)).
//!
//! It is written as: the mark `AKS1`; the name of the issuer's key it is sealed to, 16
//! bytes; the length of the next part as two bytes, the largest first; a key of this one
//! secret, as the issuer's key encrypts it (RSA-OAEP with SHA-256); twelve bytes used
//! once; and the secret, as that key encrypts it (AES-256-GCM, with its tag at the end),
//! bound to the name the secret is kept under, so that it opens under no other.

use ring::digest::{digest, SHA256};

/// What a sealed secret begins with.
pub const MARK: &[u8; 4] = b"AKS1";
/// How long the name of a key is.
pub const KEY_NAME: usize = 16;
/// How many bytes are used once.
pub const NONCE: usize = 12;
/// How long the tag at the end is.
pub const TAG: usize = 16;
/// The most a sealed secret may be.
pub const LONGEST: usize = 64 * 1024;

/// The parts of a sealed secret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sealed<'a> {
    /// The name of the issuer's key it is sealed to.
    pub key: [u8; KEY_NAME],
    /// The key of this secret, as the issuer's key encrypts it.
    pub wrapped: &'a [u8],
    /// The bytes used once.
    pub nonce: [u8; NONCE],
    /// The secret, encrypted, with its tag.
    pub sealed: &'a [u8],
}

/// Reads a sealed secret's parts. Nothing where it is not built as one: of a key of
/// 2048 to 8192 bits, and with something sealed.
pub fn read(bytes: &[u8]) -> Option<Sealed<'_>> {
    if bytes.len() > LONGEST || !bytes.starts_with(MARK) {
        return None;
    }
    let key: [u8; KEY_NAME] = bytes.get(4..4 + KEY_NAME)?.try_into().ok()?;
    let at = 4 + KEY_NAME;
    let length = usize::from(u16::from_be_bytes(bytes.get(at..at + 2)?.try_into().ok()?));
    if !(256..=1024).contains(&length) {
        return None;
    }
    let wrapped = bytes.get(at + 2..at + 2 + length)?;
    let at = at + 2 + length;
    let nonce: [u8; NONCE] = bytes.get(at..at + NONCE)?.try_into().ok()?;
    let sealed = bytes.get(at + NONCE..)?;
    (sealed.len() > TAG).then_some(Sealed { key, wrapped, nonce, sealed })
}

/// Writes a sealed secret from its parts.
pub fn write(parts: &Sealed<'_>) -> Vec<u8> {
    let mut bytes = MARK.to_vec();
    bytes.extend(parts.key);
    bytes.extend((parts.wrapped.len() as u16).to_be_bytes());
    bytes.extend(parts.wrapped);
    bytes.extend(parts.nonce);
    bytes.extend(parts.sealed);
    bytes
}

/// The name of a key: the first sixteen bytes of the SHA-256 of its public half, as
/// DER (a SubjectPublicKeyInfo).
pub fn key_name(public_key: &[u8]) -> [u8; KEY_NAME] {
    digest(&SHA256, public_key).as_ref()[..KEY_NAME].try_into().expect("16 bytes")
}

/// The name of a key, written as letters: in hexadecimal.
pub fn written(key: &[u8; KEY_NAME]) -> String {
    key.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// What the sealed secret is bound to: the name it is kept under.
pub fn bound_to(name: &str) -> Vec<u8> {
    format!("away kept secret\n{name}").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sealed_secret_is_read_as_it_is_written_and_what_is_built_otherwise_is_none() {
        let wrapped = [7u8; 384];
        let parts = Sealed { key: [1; KEY_NAME], wrapped: &wrapped, nonce: [2; NONCE], sealed: &[3; 40] };
        let bytes = write(&parts);
        assert_eq!(bytes.len(), 4 + 16 + 2 + 384 + 12 + 40);
        assert_eq!(read(&bytes), Some(parts));
        // another mark, a wrapped key of no RSA size, nothing sealed, or too much: none
        assert_eq!(read(&[b"AKS2".as_slice(), &bytes[4..]].concat()), None);
        let short = Sealed { key: [1; KEY_NAME], wrapped: &[7; 100], nonce: [2; NONCE], sealed: &[3; 40] };
        assert_eq!(read(&write(&short)), None);
        assert_eq!(read(&bytes[..bytes.len() - 40 + TAG]), None);
        assert_eq!(read(&[bytes.as_slice(), &vec![0; LONGEST]].concat()), None);
        assert_eq!(read(&bytes[..10]), None);
        // a key is named by its public half, and the name is written in hexadecimal
        assert_eq!(written(&key_name(b"a public key")).len(), 32);
        assert_eq!(bound_to("mac-01"), b"away kept secret\nmac-01");
    }
}
