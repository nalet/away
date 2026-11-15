//! A certificate, as far as a sign-in reads one (RFC 5280): who issued it and under which
//! number, whom it names, when it is good, its key, and what it may be used for. And the
//! check that one certificate was signed by the holder of another's key.

use crate::der::{self, Reader, Value};
use crate::Error;

const WRONG: Error = Error::NotACertificate("it is not built as a certificate is");

/// Signing in to a domain with a smart card: what a certificate has to be good for.
pub const FOR_SIGNING_IN: &[u32] = &[1, 3, 6, 1, 4, 1, 311, 20, 2, 2];
/// Being the domain controller that answers a sign-in with a certificate.
pub const FOR_A_DOMAIN_CONTROLLER: &[u32] = &[1, 3, 6, 1, 5, 2, 3, 5];

/// The name of an account in a certificate, as Windows writes it there.
const ACCOUNT_NAME: &[u32] = &[1, 3, 6, 1, 4, 1, 311, 20, 2, 3];
const OTHER_NAMES: &[u32] = &[2, 5, 29, 17];
const USES: &[u32] = &[2, 5, 29, 37];
const RSA: &[u32] = &[1, 2, 840, 113549, 1, 1, 1];

/// What a certificate says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Certificate<'a> {
    /// The certificate as it was written.
    pub whole: &'a [u8],
    /// The part of it that its issuer signed.
    pub signed: &'a [u8],
    /// The number its issuer gave it, as it is written.
    pub number: &'a [u8],
    /// Who issued it, as it is written.
    pub issuer: &'a [u8],
    /// Whom it is for, as it is written.
    pub subject: &'a [u8],
    /// From when it is good, in seconds since the beginning of 1970.
    pub from: u64,
    /// Until when.
    pub until: u64,
    /// Its key, where it is an RSA key: the key as RSA writes it.
    pub key: Option<&'a [u8]>,
    /// What it may be used for, each as the name of the use is written.
    pub uses: Vec<&'a [u8]>,
    /// The names of machines it is for.
    pub machines: Vec<&'a str>,
    /// The account it is for, as the organisation writes it, where it names one.
    pub account: Option<String>,
    /// How its issuer signed it: the name of the way, as it is written.
    pub signed_how: &'a [u8],
    /// The signature.
    pub signature: &'a [u8],
}

fn name_of(numbers: &[u32]) -> Vec<u8> {
    der::name(numbers)[2..].to_vec()
}

/// A time of a certificate: with a year of two digits, or of four.
fn time(value: Value) -> Result<u64, Error> {
    match value.kind {
        // the years 50 to 99 are of the last century
        0x17 => {
            let century: &[u8] = if value.holds.first().is_some_and(|digit| *digit >= b'5') { b"19" } else { b"20" };
            der::read_time(&[century, value.holds].concat()).map_err(|_| WRONG)
        }
        der::GENERALIZED_TIME => der::read_time(value.holds).map_err(|_| WRONG),
        _ => Err(WRONG),
    }
}

impl<'a> Certificate<'a> {
    /// Reads a certificate.
    pub fn read(bytes: &'a [u8]) -> Result<Certificate<'a>, Error> {
        let mut outside = Reader::new(bytes);
        let whole = outside.expect(der::SEQUENCE).map_err(|_| WRONG)?;
        let mut parts = whole.inside();
        let signed = parts.expect(der::SEQUENCE).map_err(|_| WRONG)?;
        let signed_how = parts.expect(der::SEQUENCE).map_err(|_| WRONG)?.inside().expect(der::OBJECT_NAME).map_err(|_| WRONG)?.holds;
        let signature = parts.expect(der::BIT_STRING).map_err(|_| WRONG)?.holds.get(1..).ok_or(WRONG)?;

        let mut fields = signed.inside();
        // its version, where it says one
        fields.optional(der::numbered(0)).map_err(|_| WRONG)?;
        let number = fields.expect(der::INTEGER).map_err(|_| WRONG)?.whole;
        fields.expect(der::SEQUENCE).map_err(|_| WRONG)?;
        let issuer = fields.expect(der::SEQUENCE).map_err(|_| WRONG)?.whole;
        let mut good = fields.expect(der::SEQUENCE).map_err(|_| WRONG)?.inside();
        let (from, until) = (time(good.value().map_err(|_| WRONG)?)?, time(good.value().map_err(|_| WRONG)?)?);
        let subject = fields.expect(der::SEQUENCE).map_err(|_| WRONG)?.whole;
        let mut its_key = fields.expect(der::SEQUENCE).map_err(|_| WRONG)?.inside();
        let kind = its_key.expect(der::SEQUENCE).map_err(|_| WRONG)?.inside().expect(der::OBJECT_NAME).map_err(|_| WRONG)?.holds;
        let key = its_key.expect(der::BIT_STRING).map_err(|_| WRONG)?.holds.get(1..).ok_or(WRONG)?;
        let key = (kind == name_of(RSA)).then_some(key);

        let (mut uses, mut machines, mut account) = (Vec::new(), Vec::new(), None);
        // what follows the key and is no list of further things is passed over
        while let Some(kind) = fields.next_kind() {
            if kind != der::numbered(3) {
                fields.value().map_err(|_| WRONG)?;
                continue;
            }
            let mut further = fields.value().map_err(|_| WRONG)?.inside().expect(der::SEQUENCE).map_err(|_| WRONG)?.inside();
            while !further.is_empty() {
                let mut one = further.expect(der::SEQUENCE).map_err(|_| WRONG)?.inside();
                let which = one.expect(der::OBJECT_NAME).map_err(|_| WRONG)?.holds;
                // whether it has to be understood, where it says so
                one.optional(der::BOOLEAN).map_err(|_| WRONG)?;
                let holds = one.expect(der::OCTET_STRING).map_err(|_| WRONG)?.holds;
                if which == name_of(USES) {
                    let mut listed = Reader::new(holds).expect(der::SEQUENCE).map_err(|_| WRONG)?.inside();
                    while !listed.is_empty() {
                        uses.push(listed.expect(der::OBJECT_NAME).map_err(|_| WRONG)?.holds);
                    }
                } else if which == name_of(OTHER_NAMES) {
                    let mut listed = Reader::new(holds).expect(der::SEQUENCE).map_err(|_| WRONG)?.inside();
                    while !listed.is_empty() {
                        let named = listed.value().map_err(|_| WRONG)?;
                        match named.kind {
                            // the name of a machine
                            0x82 => machines.push(named.text().map_err(|_| WRONG)?),
                            // a name of another kind: the account's, as Windows writes it
                            0xa0 => {
                                let mut other = named.inside();
                                if other.expect(der::OBJECT_NAME).map_err(|_| WRONG)?.holds == name_of(ACCOUNT_NAME) {
                                    let written = other.expect(der::numbered(0)).map_err(|_| WRONG)?.inside().value().map_err(|_| WRONG)?;
                                    account = Some(written.text().map_err(|_| WRONG)?.to_string());
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        Ok(Certificate {
            whole: whole.whole,
            signed: signed.whole,
            number,
            issuer,
            subject,
            from,
            until,
            key,
            uses,
            machines,
            account,
            signed_how,
            signature,
        })
    }

    /// Whether it may be used for this.
    pub fn is_for(&self, a_use: &[u32]) -> bool {
        self.uses.contains(&&name_of(a_use)[..])
    }

    /// Whether it is good at this time, in seconds since the beginning of 1970.
    pub fn is_good_at(&self, now: u64) -> bool {
        self.from <= now && now <= self.until
    }

    /// Whether it was signed with the key of this certificate, which is its issuer's.
    pub fn is_signed_by(&self, issuer: &Certificate) -> bool {
        self.issuer == issuer.subject && issuer.key.is_some_and(|key| signed_with(key, self.signed_how, self.signed, self.signature))
    }
}

/// Whether a signature on a message was made with the key that goes with this one, in
/// the way that is named: RSA with one of the hashes that are in use.
pub fn signed_with(key: &[u8], how: &[u8], message: &[u8], signature: &[u8]) -> bool {
    use ring::signature::{
        UnparsedPublicKey, RSA_PKCS1_2048_8192_SHA1_FOR_LEGACY_USE_ONLY, RSA_PKCS1_2048_8192_SHA256, RSA_PKCS1_2048_8192_SHA384,
        RSA_PKCS1_2048_8192_SHA512,
    };
    let rsa_with = |hash: u32| name_of(&[1, 2, 840, 113549, 1, 1, hash]);
    let way = match how {
        how if how == rsa_with(11) => &RSA_PKCS1_2048_8192_SHA256,
        how if how == rsa_with(12) => &RSA_PKCS1_2048_8192_SHA384,
        how if how == rsa_with(13) => &RSA_PKCS1_2048_8192_SHA512,
        how if how == rsa_with(5) => &RSA_PKCS1_2048_8192_SHA1_FOR_LEGACY_USE_ONLY,
        _ => return false,
    };
    UnparsedPublicKey::new(way, key).verify(message, signature).is_ok()
}

/// Whether a certificate leads to one of the authorities that are trusted, through the
/// certificates that came with it: each signed by the next, and each good now.
pub fn leads_to(certificate: &Certificate, came_with: &[Certificate], trusted: &[Certificate], now: u64) -> bool {
    let mut at = certificate;
    // a chain is short, and none goes round
    for _ in 0..8 {
        if !at.is_good_at(now) {
            return false;
        }
        if trusted.iter().any(|authority| authority.is_good_at(now) && (at.whole == authority.whole || at.is_signed_by(authority))) {
            return true;
        }
        match came_with.iter().find(|next| next.whole != at.whole && at.is_signed_by(next)) {
            Some(next) => at = next,
            None => return false,
        }
    }
    false
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// An authority, a domain controller's certificate and an account's, made for these
    /// tests alone with OpenSSL: see `tests/fixtures/README.md`.
    pub(crate) const AUTHORITY: &[u8] = include_bytes!("../tests/fixtures/authority.der");
    pub(crate) const CONTROLLER: &[u8] = include_bytes!("../tests/fixtures/controller.der");
    pub(crate) const ACCOUNT: &[u8] = include_bytes!("../tests/fixtures/account.der");
    pub(crate) const ACCOUNT_KEY: &[u8] = include_bytes!("../tests/fixtures/account.key.der");
    pub(crate) const STRANGER: &[u8] = include_bytes!("../tests/fixtures/stranger.der");
    /// A moment at which the tests' certificates are good.
    pub(crate) const NOW: u64 = 1_794_787_200;

    #[test]
    fn a_certificate_says_whom_it_names_what_it_is_for_and_who_signed_it() {
        let authority = Certificate::read(AUTHORITY).unwrap();
        let controller = Certificate::read(CONTROLLER).unwrap();
        let account = Certificate::read(ACCOUNT).unwrap();
        assert_eq!(account.account.as_deref(), Some("alex.urban@CORP.EXAMPLE"));
        assert!(account.is_for(FOR_SIGNING_IN) && !account.is_for(FOR_A_DOMAIN_CONTROLLER));
        assert_eq!(
            (controller.machines.clone(), controller.is_for(FOR_A_DOMAIN_CONTROLLER)),
            (vec!["localhost", "dc-01.corp.example"], true)
        );
        assert_eq!(account.issuer, authority.subject);
        assert!(account.key.is_some() && account.is_good_at(NOW) && !account.is_good_at(1_000_000_000));
        assert_eq!(account.whole, ACCOUNT);

        // each is signed by the authority, the authority by itself, and nothing by a stranger
        assert!(account.is_signed_by(&authority) && controller.is_signed_by(&authority) && authority.is_signed_by(&authority));
        let stranger = Certificate::read(STRANGER).unwrap();
        assert!(!account.is_signed_by(&stranger) && !stranger.is_signed_by(&authority) && !account.is_signed_by(&controller));
        // a changed byte in what was signed, and the signature holds no longer
        let mut changed = ACCOUNT.to_vec();
        let at = changed.windows(10).position(|part| part == b"alex.urban").unwrap();
        changed[at] = b'b';
        assert!(!Certificate::read(&changed).unwrap().is_signed_by(&authority));

        assert!(leads_to(&controller, &[], std::slice::from_ref(&authority), NOW));
        assert!(leads_to(&authority, &[], std::slice::from_ref(&authority), NOW));
        assert!(!leads_to(&stranger, std::slice::from_ref(&stranger), std::slice::from_ref(&authority), NOW));
        assert!(!leads_to(&controller, &[], &[], NOW));
        // an authority or a certificate that is good no longer leads nowhere
        assert!(!leads_to(&controller, &[], std::slice::from_ref(&authority), authority.until + 1));
    }

    #[test]
    fn what_is_no_certificate_is_refused() {
        for cut in 0..ACCOUNT.len() {
            assert!(Certificate::read(&ACCOUNT[..cut]).is_err(), "cut at {cut}");
        }
        assert_eq!(Certificate::read(b"nothing of the kind"), Err(WRONG));
    }
}
