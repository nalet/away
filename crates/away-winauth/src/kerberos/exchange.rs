//! The exchange with a domain controller: with a certificate, a first ticket; with that
//! ticket, one for the machine. Each step gives the bytes to send and takes the bytes
//! that came back. Nothing here opens a connection.

use crate::Error;

use super::crypto::{Key, Kind};
use super::messages::{self, Asked, Extra, Name, NameKind, Proof, Reply, Sealed, WITH_A_CERTIFICATE_ANSWER, WITH_A_TICKET};
use super::pkinit::{self, Signs, Trusts};

/// How long a ticket is asked to be good for, in seconds: a session's sign-in takes
/// seconds, and a ticket that is stolen should be worth little.
const GOOD_FOR: u64 = 600;

/// A ticket, and what goes with it for whoever it was given to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ticket {
    /// The ticket, as it is shown: sealed for whoever it is for.
    pub bytes: Vec<u8>,
    /// The key that goes with it.
    pub key: Key,
    /// The realm of the account, as the domain writes it.
    pub realm: String,
    /// The account, as the domain writes it.
    pub account: Name,
    /// What the ticket is for.
    pub service: Name,
    /// The realm of what it is for.
    pub service_realm: String,
    /// Until when it is good, in seconds since the beginning of 1970.
    pub until: u64,
}

/// A message as it travels to a domain controller over a connection: its length in four
/// bytes, and the message.
pub fn framed(message: &[u8]) -> Vec<u8> {
    let mut framed = (message.len() as u32).to_be_bytes().to_vec();
    framed.extend_from_slice(message);
    framed
}

/// The message that the bytes of a connection begin with, once all of it has arrived.
/// A message longer than any a domain controller sends is refused.
pub fn unframed(arrived: &[u8]) -> Result<Option<&[u8]>, Error> {
    let Some(length) = arrived.get(..4) else { return Ok(None) };
    let length = u32::from_be_bytes([length[0], length[1], length[2], length[3]]) as usize;
    if length > 1024 * 1024 {
        return Err(Error::NotKerberos("it says its answer is longer than any answer is"));
    }
    Ok(arrived.get(4..4 + length))
}

fn refused(refused: messages::Refused) -> Error {
    Error::Refused { code: refused.code, text: refused.text }
}

/// The first request: a ticket for the domain's own ticket service, asked for with a
/// certificate.
pub struct First {
    nonce: u32,
    fresh: pkinit::Fresh,
}

impl First {
    /// Begins: the request to send. The account is named as the certificate names it,
    /// without the realm; the time is the client's, in seconds since the beginning of
    /// 1970 and millionths of a second; the number is one used once.
    pub fn begin(
        signs: &dyn Signs,
        account: &str,
        realm: &str,
        now: (u64, u32),
        nonce: u32,
        fresh: pkinit::Fresh,
    ) -> Result<(First, Vec<u8>), Error> {
        let asked = Asked {
            account: Some(Name::new(NameKind::Account, &[account])),
            realm: realm.to_string(),
            service: Name::new(NameKind::Service, &["krbtgt", realm]),
            until: now.0 + GOOD_FOR,
            nonce,
            kinds: vec![Kind::Aes256, Kind::Aes128],
        }
        .write();
        let extras = [pkinit::proof(signs, &asked, now, nonce, &fresh)?, messages::with_what_the_account_may()];
        Ok((First { nonce, fresh }, messages::request(true, &extras, &asked)))
    }

    /// Takes the domain controller's answer, and gives the first ticket. The domain
    /// controller has to prove itself with a certificate that is trusted.
    pub fn receive(self, reply: &[u8], trusts: &Trusts, now: u64) -> Result<Ticket, Error> {
        let answered = match messages::read_reply(reply)? {
            Reply::Answered(answered) => answered,
            Reply::Refused(why) => return Err(refused(why)),
        };
        let kind = Kind::of(answered.sealed.kind).ok_or(Error::NotKerberos("it sealed its answer in a way that was not asked for"))?;
        let Some(proof) = answered.extras.iter().find(|extra| extra.kind == WITH_A_CERTIFICATE_ANSWER) else {
            return Err(Error::NotTrusted("it answered without proving itself with a certificate"));
        };
        let key = pkinit::key(&proof.holds, kind, self.nonce, &self.fresh, trusts, now)?;
        given(answered, &key, 3, self.nonce)
    }
}

/// The first request where a password is what proves who asks: the domain controller is
/// asked once to learn how the account's key is made, and once more with the proof.
pub struct WithAPassword {
    account: String,
    realm: String,
    nonce: u32,
    key: Option<Key>,
}

impl WithAPassword {
    /// Begins: the request to send, which proves nothing yet. The domain controller
    /// answers it with how the account's key is made.
    pub fn begin(account: &str, realm: &str, now: (u64, u32), nonce: u32) -> (WithAPassword, Vec<u8>) {
        let asking = WithAPassword { account: account.to_string(), realm: realm.to_string(), nonce, key: None };
        let request = messages::request(true, &[messages::with_what_the_account_may()], &asking.asked(now));
        (asking, request)
    }

    fn asked(&self, now: (u64, u32)) -> Vec<u8> {
        Asked {
            account: Some(Name::new(NameKind::Account, &[&self.account])),
            realm: self.realm.clone(),
            service: Name::new(NameKind::Service, &["krbtgt", &self.realm]),
            until: now.0 + GOOD_FOR,
            nonce: self.nonce,
            kinds: vec![Kind::Aes256, Kind::Aes128],
        }
        .write()
    }

    /// Takes the domain controller's first answer, which says how the account's key is
    /// made, and gives the request that proves who asks: the time, sealed with that key.
    pub fn prove(&mut self, reply: &[u8], password: &str, now: (u64, u32), begins_with: [u8; 16]) -> Result<Vec<u8>, Error> {
        let how = match messages::read_reply(reply)? {
            // it asks for the proof, and says how the key is made
            Reply::Refused(asks) if asks.code == 25 => messages::read_how_keys_are_made(&asks.more.unwrap_or_default())?,
            Reply::Refused(why) => return Err(refused(why)),
            Reply::Answered(_) => return Err(Error::NotTrusted("it gave a ticket to somebody who proved nothing")),
        };
        // the first of its ways that is spoken here
        let (kind, how) = how
            .iter()
            .find_map(|how| Kind::of(how.kind).map(|kind| (kind, how)))
            .ok_or(Error::NotKerberos("it makes the account's key in no way that is spoken here"))?;
        let salt = how.salt.clone().unwrap_or_else(|| [self.realm.as_bytes(), self.account.as_bytes()].concat());
        let key = Key::from_password(kind, password, &salt, how.times.unwrap_or(4096));
        let extras = [messages::with_the_time(&key, now, begins_with), messages::with_what_the_account_may()];
        self.key = Some(key);
        Ok(messages::request(true, &extras, &self.asked(now)))
    }

    /// Takes the domain controller's answer to the proof, and gives the first ticket.
    pub fn receive(self, reply: &[u8]) -> Result<Ticket, Error> {
        let key = self.key.ok_or(Error::NotKerberos("an answer came before the proof was sent"))?;
        match messages::read_reply(reply)? {
            Reply::Answered(answered) => given(answered, &key, 3, self.nonce),
            Reply::Refused(why) => Err(refused(why)),
        }
    }
}

/// Opens what an answer seals for the client, and makes the ticket of it.
pub(crate) fn given(answered: messages::Answered, key: &Key, usage: u32, nonce: u32) -> Result<Ticket, Error> {
    let given = messages::read_given(&answered.sealed.open(key, usage)?)?;
    if given.nonce != nonce {
        return Err(Error::NotTrusted("it answered another request than the one that was made"));
    }
    Ok(Ticket {
        bytes: answered.ticket,
        key: given.key,
        realm: answered.realm,
        account: answered.account,
        service: given.service,
        service_realm: given.realm,
        until: given.until,
    })
}

/// A later request: a ticket for a service, asked for with the first ticket.
pub struct Later {
    nonce: u32,
    key: Key,
}

impl Later {
    /// Begins: the request to send. `begins_with` is sixteen random bytes.
    pub fn begin(first: &Ticket, service: Name, now: (u64, u32), nonce: u32, begins_with: [u8; 16]) -> (Later, Vec<u8>) {
        let asked = Asked {
            account: None,
            realm: first.service_realm.clone(),
            service,
            until: first.until.min(now.0 + GOOD_FOR),
            nonce,
            kinds: vec![Kind::Aes256, Kind::Aes128],
        }
        .write();
        // what is asked is bound to the ticket by a check with the ticket's key
        let check_kind = match first.key.kind() {
            Kind::Aes256 => 16,
            Kind::Aes128 => 15,
        };
        let proof = Proof {
            realm: first.realm.clone(),
            account: first.account.clone(),
            check: Some((check_kind, first.key.check(6, &asked).to_vec())),
            time: now,
            key: None,
            counts_from: None,
        };
        let shown = messages::show(&first.bytes, &Sealed::with(&first.key, 7, begins_with, &proof.write()), false);
        let request = messages::request(false, &[Extra { kind: WITH_A_TICKET, holds: shown }], &asked);
        (Later { nonce, key: first.key.clone() }, request)
    }

    /// Takes the domain controller's answer, and gives the ticket for the service.
    pub fn receive(self, reply: &[u8]) -> Result<Ticket, Error> {
        match messages::read_reply(reply)? {
            Reply::Answered(answered) => given(answered, &self.key, 8, self.nonce),
            Reply::Refused(why) => Err(refused(why)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::certificate::tests::{ACCOUNT, ACCOUNT_KEY, AUTHORITY, CONTROLLER, NOW};
    use crate::der::{self, Reader};
    use crate::kerberos::messages::tests::{answer, given as sealed_for_the_client, ticket};
    use crate::kerberos::pkinit::{stand_in, Fresh, Held};

    const CONTROLLER_KEY: &[u8] = include_bytes!("../../tests/fixtures/controller.key.der");

    fn fresh() -> Fresh {
        Fresh { secret: [0x42; 32], once: [0x17; 32] }
    }

    /// What goes with a request, and the part that says what is asked, as it was written.
    fn parts(request: &[u8]) -> (Vec<Extra>, Vec<u8>) {
        let mut inside = Reader::new(request).value().unwrap().inside().expect(der::SEQUENCE).unwrap().inside();
        inside.skip_to(3).unwrap();
        let mut extras = Vec::new();
        let mut listed = inside.needed(3).unwrap().inside();
        while !listed.is_empty() {
            let mut extra = listed.expect(der::SEQUENCE).unwrap().inside();
            extras.push(Extra { kind: extra.needed(1).unwrap().number().unwrap(), holds: extra.needed(2).unwrap().holds.to_vec() });
        }
        (extras, inside.needed(4).unwrap().whole.to_vec())
    }

    #[test]
    fn a_certificate_gives_a_first_ticket_and_the_first_ticket_one_for_a_machine() {
        let account = Held::new(ACCOUNT, ACCOUNT_KEY).unwrap();
        let controller = Held::new(CONTROLLER, CONTROLLER_KEY).unwrap();
        let trusts = Trusts { authorities: &[AUTHORITY], controller: "dc-01.corp.example" };

        // the first request: a domain controller reads the proof, and answers with its own
        let (first, request) = First::begin(&account, "alex.urban", "CORP.EXAMPLE", (NOW, 5), 0x0102_0304, fresh()).unwrap();
        assert_eq!(request[0], 0x6a);
        let (extras, asked) = parts(&request);
        assert_eq!(extras.iter().map(|extra| extra.kind).collect::<Vec<_>>(), [16, 128]);
        let proven = stand_in::read_proof(&extras[0]).unwrap();
        assert_eq!((proven.nonce, proven.time), (0x0102_0304, (NOW, 5)));
        // the proof is bound to what is asked
        assert_eq!(proven.second_check, <sha2::Sha256 as sha2::Digest>::digest(&asked).as_slice());
        let (theirs, reply_key) = stand_in::answer(&proven, &controller, &[0x24; 32], Some(&[0x99; 32]), Kind::Aes256, false);
        let session = Key::new(Kind::Aes256, &[5; 32]).unwrap();
        let ticket_service = Name::new(NameKind::Service, &["krbtgt", "CORP.EXAMPLE"]);
        let sealed = Sealed::with(&reply_key, 3, [1; 16], &sealed_for_the_client(true, &session, 0x0102_0304, &ticket_service));
        let reply = answer(true, &[Extra { kind: WITH_A_CERTIFICATE_ANSWER, holds: theirs.clone() }], &ticket(), &sealed);
        let first_ticket = first.receive(&reply, &trusts, NOW).unwrap();
        assert_eq!(
            (first_ticket.key.clone(), first_ticket.service.clone(), first_ticket.until),
            (session.clone(), ticket_service.clone(), 1_794_736_800)
        );
        assert_eq!((first_ticket.realm.as_str(), first_ticket.bytes.clone()), ("CORP.EXAMPLE", ticket()));

        // the later request shows the first ticket, with a proof sealed with its key and a
        // check over what is asked
        let machine = Name::new(NameKind::Service, &["TERMSRV", "desk-01.corp.example"]);
        let (later, request) = Later::begin(&first_ticket, machine.clone(), (NOW, 6), 77, [2; 16]);
        assert_eq!(request[0], 0x6c);
        let (extras, asked) = parts(&request);
        assert_eq!(extras.len(), 1);
        let mut shown = Reader::new(&extras[0].holds).expect(der::message(14)).unwrap().inside().expect(der::SEQUENCE).unwrap().inside();
        shown.skip_to(3).unwrap();
        assert_eq!(shown.needed(3).unwrap().whole, ticket());
        let mut sealed_proof = shown.needed(4).unwrap().inside();
        sealed_proof.skip_to(2).unwrap();
        let proof = session.open(7, sealed_proof.needed(2).unwrap().holds).unwrap();
        let mut proof = Reader::new(&proof).expect(der::message(2)).unwrap().inside().expect(der::SEQUENCE).unwrap().inside();
        proof.skip_to(3).unwrap();
        let mut check = proof.needed(3).unwrap().inside();
        assert_eq!((check.needed(0).unwrap().number(), check.needed(1).unwrap().holds), (Ok(16), &session.check(6, &asked)[..]));

        let for_the_machine = Key::new(Kind::Aes256, &[8; 32]).unwrap();
        let sealed = Sealed::with(&session, 8, [3; 16], &sealed_for_the_client(false, &for_the_machine, 77, &machine));
        let machines_ticket = later.receive(&answer(false, &[], &ticket(), &sealed)).unwrap();
        assert_eq!((machines_ticket.key, machines_ticket.service), (for_the_machine, machine.clone()));

        // an answer to another request, one that proves nothing, and a refusal
        let (first, _) = First::begin(&account, "alex.urban", "CORP.EXAMPLE", (NOW, 5), 0x0102_0304, fresh()).unwrap();
        let other = Sealed::with(&reply_key, 3, [1; 16], &sealed_for_the_client(true, &session, 9, &ticket_service));
        let reply = answer(true, &[Extra { kind: WITH_A_CERTIFICATE_ANSWER, holds: theirs }], &ticket(), &other);
        assert_eq!(first.receive(&reply, &trusts, NOW), Err(Error::NotTrusted("it answered another request than the one that was made")));
        let (first, _) = First::begin(&account, "alex.urban", "CORP.EXAMPLE", (NOW, 5), 0x0102_0304, fresh()).unwrap();
        assert_eq!(
            first.receive(&answer(true, &[], &ticket(), &sealed), &trusts, NOW),
            Err(Error::NotTrusted("it answered without proving itself with a certificate"))
        );
        let refusal = der::write(
            der::message(30),
            &der::sequence(&[
                der::under(0, &der::number(5)),
                der::under(1, &der::number(30)),
                der::under(4, &der::time(NOW)),
                der::under(5, &der::number(0)),
                der::under(6, &der::number(6)),
                der::under(9, &der::text("CORP.EXAMPLE")),
                der::under(10, &machine.write()),
            ]),
        );
        let (later, _) = Later::begin(&first_ticket, machine, (NOW, 6), 77, [2; 16]);
        assert_eq!(later.receive(&refusal), Err(Error::Refused { code: 6, text: None }));
    }

    #[test]
    fn a_message_travels_with_its_length_before_it() {
        assert_eq!(framed(b"abc"), [0, 0, 0, 3, b'a', b'b', b'c']);
        assert_eq!(unframed(&[0, 0, 0, 3, b'a', b'b', b'c', 9]), Ok(Some(&b"abc"[..])));
        assert_eq!(unframed(&[0, 0, 0, 3, b'a', b'b']), Ok(None));
        assert_eq!(unframed(&[0, 0]), Ok(None));
        assert!(unframed(&[0x7f, 0, 0, 0]).is_err());
    }
}
