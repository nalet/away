//! The key the secrets Away keeps for machines are sealed to, as the server knows it: the
//! public half of the issuer's key, from a certificate the installation gives the server
//! ([ADR-0017](../../../spec/architecture/decisions/0017-away-keeps-the-secrets-of-machines-sealed.md)).
//!
//! The server never holds the private half, and never learns the key from another part:
//! a part that told the server a key of its own would have new secrets sealed to it.

use away_part::{base64, sealed};

/// The object identifier of an RSA key, as DER.
const RSA_ENCRYPTION: &[u8] = &[0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01];

/// The key new secrets are sealed to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keeping {
    /// Its name, as a sealed secret names it, in hexadecimal.
    pub name: String,
    /// Its public half, as DER (a SubjectPublicKeyInfo).
    pub public: Vec<u8>,
}

/// One part of DER: its tag, what it holds, and the bytes after it.
fn part(bytes: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let tag = *bytes.first()?;
    let first = *bytes.get(1)?;
    let (length, at) = if first < 0x80 {
        (usize::from(first), 2)
    } else {
        let count = usize::from(first & 0x7f);
        if count == 0 || count > 4 {
            return None;
        }
        let length = bytes.get(2..2 + count)?.iter().fold(0usize, |length, byte| (length << 8) | usize::from(*byte));
        (length, 2 + count)
    };
    let holds = bytes.get(at..at.checked_add(length)?)?;
    Some((tag, holds, &bytes[at + length..]))
}

/// Reads the key from the first certificate of PEM: an RSA key.
pub fn read(pem: &[u8]) -> Result<Keeping, String> {
    let text = std::str::from_utf8(pem).map_err(|_| "the certificate is no text".to_string())?;
    let begins = text.find("-----BEGIN CERTIFICATE-----").ok_or("there is no certificate")?;
    let body = &text[begins + 27..];
    let ends = body.find("-----END CERTIFICATE-----").ok_or("the certificate has no end")?;
    let letters: String = body[..ends].chars().filter(|letter| !letter.is_whitespace()).collect();
    let der = base64::read_for_addresses(&letters.replace('+', "-").replace('/', "_").replace('=', ""))
        .ok_or("the certificate is not written in Base64")?;
    let wrong = || "the certificate is not built as one".to_string();
    // the certificate, what is signed of it, and in that, after the version where there
    // is one, the number, the way it is signed, the issuer, the time and the subject: the key
    let (0x30, certificate, _) = part(&der).ok_or_else(wrong)? else { return Err(wrong()) };
    let (0x30, signed, _) = part(certificate).ok_or_else(wrong)? else { return Err(wrong()) };
    let mut rest = signed;
    if rest.first() == Some(&0xa0) {
        rest = part(rest).ok_or_else(wrong)?.2;
    }
    for _ in 0..5 {
        rest = part(rest).ok_or_else(wrong)?.2;
    }
    let (0x30, key, _) = part(rest).ok_or_else(wrong)? else { return Err(wrong()) };
    let public = &rest[..rest.len() - part(rest).ok_or_else(wrong)?.2.len()];
    let (0x30, algorithm, _) = part(key).ok_or_else(wrong)? else { return Err(wrong()) };
    if !algorithm.starts_with(RSA_ENCRYPTION) {
        return Err("the certificate's key is no RSA key".to_string());
    }
    Ok(Keeping { name: sealed::written(&sealed::key_name(public)), public: public.to_vec() })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A certificate of the tests: an RSA key of 2048 bits, signed by itself.
    const CERTIFICATE: &str = include_str!("../tests/fixtures/keeping.pem");

    #[test]
    fn the_key_is_read_from_its_certificate_and_named_by_its_public_half() {
        let keeping = read(CERTIFICATE.as_bytes()).expect("a key");
        assert_eq!(keeping.name.len(), 32);
        assert_eq!(keeping.name, sealed::written(&sealed::key_name(&keeping.public)));
        // the public half begins with the RSA key's object identifier
        assert_eq!(&keeping.public[4..6], &[0x30, 0x0d]);
        assert!(read(b"no certificate").is_err());
        assert!(read(b"-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n").is_err());
    }
}
