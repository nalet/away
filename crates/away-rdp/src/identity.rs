//! What a machine shows of itself when the connection is secured: its certificate.
//!
//! Away knows a machine by this. Either the certificate comes from an authority Away
//! trusts, or the machine is known by the fingerprint of what it showed when it was
//! first tested. Whoever holds the connection decides which; this reads the facts.

use sha2::{Digest, Sha256};

use crate::der::{self, Reader, Value};
use crate::Error;

/// What a certificate says of the machine that showed it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    /// The SHA-256 hash of the whole certificate: what a machine is known by when no
    /// authority speaks for it.
    pub fingerprint: [u8; 32],
    /// The name the certificate is for, where it has one.
    pub name: Option<String>,
    /// The name of whoever issued it, where it has one.
    pub issued_by: Option<String>,
    /// Whether the machine issued it itself, as a Windows machine does unless it is
    /// given another.
    pub issued_by_itself: bool,
    /// From when it is valid, as `2026-11-14T12:54:24Z`.
    pub valid_from: String,
    /// Until when it is valid.
    pub valid_until: String,
    /// The public key it holds, as the certificate writes it. The sign-in before the
    /// desktop binds itself to this key.
    pub public_key: Vec<u8>,
}

impl Identity {
    /// Reads a certificate, as TLS hands it over.
    pub fn read(certificate: &[u8]) -> Result<Identity, Error> {
        let mut outer = Reader::new(certificate);
        let mut whole = Reader::new(outer.expect(der::SEQUENCE)?);
        if !outer.is_empty() {
            return Err(Error::Certificate("something follows it"));
        }
        let mut body = Reader::new(whole.expect(der::SEQUENCE)?);
        // the version, where it is written, then the serial number
        let mut first = body.value()?;
        if first.kind == der::FIRST_OF_ITS_OWN {
            first = body.value()?;
        }
        if first.kind != der::INTEGER {
            return Err(Error::Certificate("it is not built as a certificate is"));
        }
        body.expect(der::SEQUENCE)?; // how it is signed
        let issuer = body.expect(der::SEQUENCE)?;
        let mut validity = Reader::new(body.expect(der::SEQUENCE)?);
        let valid_from = time(validity.value()?)?;
        let valid_until = time(validity.value()?)?;
        let subject = body.expect(der::SEQUENCE)?;
        let mut key = Reader::new(body.expect(der::SEQUENCE)?);
        key.expect(der::SEQUENCE)?; // what kind of key it is
        let [0, public_key @ ..] = key.expect(der::BIT_STRING)? else {
            return Err(Error::Certificate("its key is not a whole number of bytes"));
        };
        Ok(Identity {
            fingerprint: Sha256::digest(certificate).into(),
            name: common_name(subject),
            issued_by: common_name(issuer),
            issued_by_itself: issuer == subject,
            valid_from,
            valid_until,
            public_key: public_key.to_vec(),
        })
    }

    /// The fingerprint as people compare it: `21:1c:8d:…`.
    pub fn fingerprint_text(&self) -> String {
        self.fingerprint.iter().map(|byte| format!("{byte:02x}")).collect::<Vec<_>>().join(":")
    }
}

/// The common name in a name, where it has one: the last, which is the most particular.
fn common_name(name: &[u8]) -> Option<String> {
    const COMMON_NAME: &[u8] = &[0x55, 0x04, 0x03];
    let mut found = None;
    let mut parts = Reader::new(name);
    while let Ok(part) = parts.value() {
        let mut facts = Reader::new(part.holds);
        while let Ok(fact) = facts.value() {
            let mut pair = Reader::new(fact.holds);
            let (Ok(kind), Ok(value)) = (pair.value(), pair.value()) else { continue };
            if kind.kind == der::OBJECT_NAME && kind.holds == COMMON_NAME {
                found = Some(text(value));
            }
        }
    }
    found
}

/// A text, whichever of the encodings a certificate may use it is written in.
fn text(value: Value) -> String {
    if value.kind == der::BMP_STRING {
        let units: Vec<u16> = value.holds.as_chunks::<2>().0.iter().map(|pair| u16::from_be_bytes(*pair)).collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(value.holds).into_owned()
    }
}

/// A time, as `2026-11-14T12:54:24Z`.
fn time(value: Value) -> Result<String, Error> {
    let wrong = Error::Certificate("a time in it is not written as a time");
    let digits = std::str::from_utf8(value.holds).map_err(|_| wrong.clone())?;
    let (year, rest) = match value.kind {
        // a year of two digits is one of 1950 to 2049
        der::UTC_TIME if digits.len() == 13 => {
            let year: u32 = digits[..2].parse().map_err(|_| wrong.clone())?;
            (if year >= 50 { 1900 + year } else { 2000 + year }, &digits[2..])
        }
        der::GENERALIZED_TIME if digits.len() == 15 => (digits[..4].parse().map_err(|_| wrong.clone())?, &digits[4..]),
        _ => return Err(wrong),
    };
    if !rest.ends_with('Z') || !rest[..10].bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(wrong);
    }
    Ok(format!("{year:04}-{}-{}T{}:{}:{}Z", &rest[..2], &rest[2..4], &rest[4..6], &rest[6..8], &rest[8..10]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A certificate of the kind a Windows machine makes for itself.
    const WINDOWS: &[u8] = include_bytes!("../../away-try/tests/fixtures/windows.der");
    /// A certificate with another kind of key, and an organisation beside its name.
    const MACHINE: &[u8] = include_bytes!("../../away-try/tests/fixtures/machine.der");

    #[test]
    fn a_windows_machines_own_certificate_is_read() {
        let identity = Identity::read(WINDOWS).unwrap();
        assert_eq!(identity.name.as_deref(), Some("WIN-DESK-07.corp.example.com"));
        assert_eq!(identity.issued_by.as_deref(), Some("WIN-DESK-07.corp.example.com"));
        assert!(identity.issued_by_itself);
        assert_eq!(identity.valid_until, "2126-10-21T12:54:24Z");
        assert!(identity.valid_from.starts_with("2026-11-14T"));
        assert_eq!(
            identity.fingerprint_text(),
            "c1:25:41:a7:79:34:eb:ea:c0:69:64:c5:91:61:50:bc:bd:37:4c:eb:52:58:eb:1b:97:de:44:17:70:0c:69:df"
        );
        // an RSA key of 2048 bits, as the certificate writes it: a list of two numbers
        assert_eq!(identity.public_key.len(), 270);
        assert_eq!(&identity.public_key[..4], &[0x30, 0x82, 0x01, 0x0a]);
    }

    #[test]
    fn the_name_is_found_among_other_facts() {
        let identity = Identity::read(MACHINE).unwrap();
        assert_eq!(identity.name.as_deref(), Some("fin-app-02.example.com"));
        assert_eq!(
            identity.fingerprint_text(),
            "21:1c:8d:21:3d:84:d9:75:86:5d:58:a0:04:0e:0c:10:08:be:f0:af:5e:5c:cd:49:71:1e:46:1d:fc:97:a6:c3"
        );
        assert_eq!(identity.public_key.len(), 65);
    }

    #[test]
    fn a_certificate_that_is_cut_or_changed_is_refused_and_never_read_past_its_end() {
        for cut in 0..WINDOWS.len() {
            assert!(Identity::read(&WINDOWS[..cut]).is_err(), "cut at {cut}");
        }
        let mut followed = WINDOWS.to_vec();
        followed.push(0);
        assert_eq!(Identity::read(&followed), Err(Error::Certificate("something follows it")));
        // any one byte changed: read or refused, and never a panic
        for at in 0..WINDOWS.len() {
            let mut changed = WINDOWS.to_vec();
            changed[at] ^= 0xff;
            let _ = Identity::read(&changed);
        }
    }

    #[test]
    fn times_are_read_in_both_forms() {
        assert_eq!(time(Value { kind: der::UTC_TIME, holds: b"261114125424Z" }).unwrap(), "2026-11-14T12:54:24Z");
        assert_eq!(time(Value { kind: der::UTC_TIME, holds: b"990101000000Z" }).unwrap(), "1999-01-01T00:00:00Z");
        assert_eq!(time(Value { kind: der::GENERALIZED_TIME, holds: b"21261021125424Z" }).unwrap(), "2126-10-21T12:54:24Z");
        assert!(time(Value { kind: der::UTC_TIME, holds: b"2611141254Z" }).is_err());
        assert!(time(Value { kind: der::UTC_TIME, holds: b"26111412542xZ" }).is_err());
        assert!(time(Value { kind: der::INTEGER, holds: b"261114125424Z" }).is_err());
    }
}
