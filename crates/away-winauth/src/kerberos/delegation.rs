//! Kerberos constrained delegation (MS-SFU): protocol transition for an explicitly
//! authorized identity, followed by a ticket for one configured service. The KDC
//! enforces the service account's delegation grants. No user password is involved.

use super::crypto::{Key, Kind};
use super::exchange::{given, Ticket};
use super::messages::{self, Asked, Extra, Name, NameKind, Proof, Reply, Sealed};
use crate::{der, Error};
use hmac::{Hmac, KeyInit, Mac};
use md5::{Digest, Md5};

/// One request, bound to the expected client, realm, service, nonce and lifetime.
pub struct Request {
    key: Key,
    nonce: u32,
    account: Name,
    realm: String,
    service: Name,
    now: u64,
    until: u64,
}

fn same_name(a: &Name, b: &Name) -> bool {
    a.parts.len() == b.parts.len() && a.parts.iter().zip(&b.parts).all(|(a, b)| a.eq_ignore_ascii_case(b))
}

impl Request {
    /// Requests a forwardable ticket to the service itself for a configured domain
    /// account. Both names are canonical names provisioned by the administrator.
    pub fn for_user(first: &Ticket, service: Name, account: &str, now: (u64, u32), nonce: u32, fresh: [u8; 16]) -> (Self, Vec<u8>) {
        let account = Name::new(NameKind::Account, &[account]);
        // MS-SFU 2.2.1 / RFC 4757 section 7: keyed checksum, even with an AES TGT.
        let mut signed = (account.kind as u32).to_le_bytes().to_vec();
        for part in &account.parts {
            signed.extend_from_slice(part.as_bytes());
        }
        signed.extend_from_slice(first.realm.as_bytes());
        signed.extend_from_slice(b"Kerberos");
        let mut signing = Hmac::<Md5>::new_from_slice(first.key.expose()).expect("HMAC accepts any key length");
        signing.update(b"signaturekey\0");
        let signing = signing.finalize().into_bytes();
        let digest = Md5::digest([17u32.to_le_bytes().as_slice(), &signed].concat());
        let mut check = Hmac::<Md5>::new_from_slice(&signing).expect("HMAC accepts any key length");
        check.update(&digest);
        let extra = Extra {
            kind: 129,
            holds: der::sequence(&[
                der::under(0, &account.write()),
                der::under(1, &der::text(&first.realm)),
                der::under(
                    2,
                    &der::sequence(&[der::under(0, &der::number(-138)), der::under(1, &der::bytes(&check.finalize().into_bytes()))]),
                ),
                der::under(3, &der::text("Kerberos")),
            ]),
        };
        Self::begin(first, service, account, now, nonce, fresh, Some(extra), None)
    }

    /// Exchanges the service's evidence ticket for a ticket to the exact target.
    /// The service TGT and evidence stay here; only the final ticket goes to RDP.
    pub fn for_target(first: &Ticket, evidence: &Ticket, service: Name, now: (u64, u32), nonce: u32, fresh: [u8; 16]) -> (Self, Vec<u8>) {
        Self::begin(first, service, evidence.account.clone(), now, nonce, fresh, None, Some(&evidence.bytes))
    }

    #[allow(clippy::too_many_arguments)]
    fn begin(
        first: &Ticket,
        service: Name,
        account: Name,
        now: (u64, u32),
        nonce: u32,
        fresh: [u8; 16],
        extra: Option<Extra>,
        evidence: Option<&[u8]>,
    ) -> (Self, Vec<u8>) {
        let until = first.until.min(now.0 + 600);
        let asked = Asked {
            account: None,
            realm: first.realm.clone(),
            service: service.clone(),
            until,
            nonce,
            kinds: vec![Kind::Aes256, Kind::Aes128],
        }
        .with_options(if evidence.is_some() { [0x40, 0x03, 0x00, 0x00] } else { [0x40, 0x01, 0, 0] }, evidence);
        let proof = Proof {
            realm: first.realm.clone(),
            account: first.account.clone(),
            check: Some((if first.key.kind() == Kind::Aes256 { 16 } else { 15 }, first.key.check(6, &asked).to_vec())),
            time: now,
            key: None,
            counts_from: None,
        };
        let shown = messages::show(&first.bytes, &Sealed::with(&first.key, 7, fresh, &proof.write()), false);
        let mut extras = vec![Extra { kind: messages::WITH_A_TICKET, holds: shown }];
        extras.extend(extra);
        (
            Self { key: first.key.clone(), nonce, account, realm: first.realm.clone(), service, now: now.0, until },
            messages::request(false, &extras, &asked),
        )
    }

    /// Accepts only the requested identity and target, in this realm and lifetime.
    pub fn receive(self, reply: &[u8]) -> Result<Ticket, Error> {
        let answered = match messages::read_reply(reply)? {
            Reply::Answered(answered) => answered,
            Reply::Refused(why) => return Err(Error::Refused { code: why.code, text: why.text }),
        };
        let ticket = given(answered, &self.key, 8, self.nonce)?;
        if !same_name(&ticket.account, &self.account)
            || !same_name(&ticket.service, &self.service)
            || !ticket.realm.eq_ignore_ascii_case(&self.realm)
            || !ticket.service_realm.eq_ignore_ascii_case(&self.realm)
            || ticket.until <= self.now
            || ticket.until > self.until
        {
            return Err(Error::NotTrusted("the delegated ticket does not match the authorized identity, target or lifetime"));
        }
        Ok(ticket)
    }
}

#[cfg(test)]
mod tests {
    use super::super::messages::tests::{answer, bytes, given as body, ticket};
    use super::*;
    use crate::der::Reader;
    fn first() -> Ticket {
        Ticket {
            bytes: ticket(),
            key: Key::new(Kind::Aes256, &[5; 32]).unwrap(),
            realm: "CORP.EXAMPLE".into(),
            account: Name::new(NameKind::Account, &["svc-away"]),
            service: Name::new(NameKind::Service, &["krbtgt", "CORP.EXAMPLE"]),
            service_realm: "CORP.EXAMPLE".into(),
            until: 1_794_736_800,
        }
    }
    fn service() -> Name {
        Name::new(NameKind::Service, &["HTTP", "away.corp.example"])
    }
    #[test]
    fn protocol_transition_binds_the_identity_and_proxy_binds_the_evidence() {
        let first = first();
        let (request, wire) = Request::for_user(&first, service(), "alex.urban", (1_794_736_400, 0), 77, [2; 16]);
        let mut msg = Reader::new(&wire).value().unwrap().inside().expect(der::SEQUENCE).unwrap().inside();
        msg.skip_to(3).unwrap();
        let extras = msg.needed(3).unwrap();
        let checksum = bytes("ce1c99b46d5b64537ef4222a77c8b08f");
        assert!(extras.whole.windows(checksum.len()).any(|w| w == checksum));
        let mut asked = msg.needed(4).unwrap().inside();
        assert_eq!(asked.needed(0).unwrap().holds, [0, 0x40, 1, 0, 0]);
        let key = Key::new(Kind::Aes256, &[8; 32]).unwrap();
        let sealed = Sealed::with(&first.key, 8, [3; 16], &body(false, &key, 77, &service()));
        let evidence = request.receive(&answer(false, &[], &ticket(), &sealed)).unwrap();
        let target = Name::new(NameKind::Service, &["TERMSRV", "desk.corp.example"]);
        let (_, wire) = Request::for_target(&first, &evidence, target, (1_794_736_400, 0), 78, [4; 16]);
        let mut msg = Reader::new(&wire).value().unwrap().inside().expect(der::SEQUENCE).unwrap().inside();
        msg.skip_to(4).unwrap();
        let mut asked = msg.needed(4).unwrap().inside();
        assert_eq!(asked.needed(0).unwrap().holds, [0, 0x40, 3, 0, 0]);
        asked.skip_to(11).unwrap();
        assert_eq!(asked.needed(11).unwrap().holds, evidence.bytes);
    }
    #[test]
    fn wrong_client_target_nonce_and_expired_tickets_are_rejected() {
        let first = first();
        for (account, target, nonce, now) in [
            ("another.user", service(), 77, 1_794_736_400),
            ("alex.urban", Name::new(NameKind::Service, &["ldap", "dc.corp.example"]), 77, 1_794_736_400),
            ("alex.urban", service(), 78, 1_794_736_400),
            ("alex.urban", service(), 77, 1_794_736_800),
        ] {
            let (request, _) = Request::for_user(&first, service(), account, (now, 0), 77, [2; 16]);
            let key = Key::new(Kind::Aes256, &[8; 32]).unwrap();
            let sealed = Sealed::with(&first.key, 8, [3; 16], &body(false, &key, nonce, &target));
            assert!(request.receive(&answer(false, &[], &ticket(), &sealed)).is_err());
        }
    }
}
