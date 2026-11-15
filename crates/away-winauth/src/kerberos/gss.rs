//! Showing a ticket to a machine, and sealing what the two then say to each other, the
//! way programs agree on a way to prove themselves (RFC 4178, RFC 4121, MS-KILE).
//!
//! The client offers Kerberos and shows its ticket in the same breath. The machine
//! answers with its own proof, and from then on each side can seal a message for the
//! other, and check one that the other sealed.

use crate::der::{self, Reader};
use crate::Error;

use super::crypto::{Key, Kind};
use super::exchange::Ticket;
use super::messages::{self, Proof, Sealed};

const WRONG: Error = Error::NotKerberos("the machine's answer to the ticket is not built as one");

/// The way two programs agree on how to prove themselves.
const AGREEING: &[u32] = &[1, 3, 6, 1, 5, 5, 2];
/// Kerberos, as Windows names it, and as the standard does.
const KERBEROS_OF_WINDOWS: &[u32] = &[1, 2, 840, 48018, 1, 2, 2];
const KERBEROS: &[u32] = &[1, 2, 840, 113554, 1, 2, 2];

/// The client proves itself and asks the machine to do so too, no message is taken
/// twice or out of its order, and messages are sealed and checked.
const WHAT_IS_ASKED: u32 = 0x02 | 0x04 | 0x08 | 0x10 | 0x20;

const SENT_BY_THE_MACHINE: u8 = 0x01;
const SEALED: u8 = 0x02;
const WITH_THE_MACHINES_KEY: u8 = 0x04;

/// What is new for one showing of a ticket.
pub struct Fresh {
    /// A key of the client's own for what follows.
    pub key: [u8; 32],
    /// Sixteen random bytes that begin what is sealed.
    pub begins_with: [u8; 16],
    /// The number the client counts its messages from.
    pub counts_from: u32,
}

/// A ticket that was shown, and waits for the machine's answer.
pub struct Shown {
    session: Key,
    ours: Key,
    time: (u64, u32),
    counts_from: u32,
    offered: Vec<u8>,
}

/// A token of Kerberos, in the frame every such token has: what it is of, its kind, and
/// the message.
fn framed(kind: [u8; 2], message: &[u8]) -> Vec<u8> {
    der::write(der::message(0), &[&der::name(KERBEROS)[..], &kind, message].concat())
}

/// Shows a ticket to the machine it is for: the token to send, which offers Kerberos and
/// carries the ticket with its proof. `now` is the client's time.
///
/// The proof is bound to nothing of the connection below, which the sign-in before a
/// desktop does itself.
pub fn show(ticket: &Ticket, now: (u64, u32), fresh: Fresh) -> (Shown, Vec<u8>) {
    show_bound(ticket, now, fresh, &Bound([0; 16]))
}

/// What a proof is bound to: the connection it travels on, so that it is worth nothing
/// on another one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bound([u8; 16]);

impl Bound {
    /// Bound to a connection that TLS secures, by the certificate the service showed on
    /// it (RFC 5929, `tls-server-end-point`): whoever passes the proof on to the service
    /// over a connection of their own shows another certificate.
    pub fn to_the_certificate(shown: &[u8]) -> Result<Bound, Error> {
        use md5::Md5;
        use sha2::{Digest, Sha256, Sha384, Sha512};
        // the hash is the one the certificate was signed with, and SHA-256 where that
        // one is weaker or is not named with the way of signing
        let how = crate::certificate::Certificate::read(shown)?.signed_how;
        let named = |numbers: &[u32]| how == &der::name(numbers)[2..];
        let hash = if named(&[1, 2, 840, 113549, 1, 1, 12]) || named(&[1, 2, 840, 10045, 4, 3, 3]) {
            Sha384::digest(shown).to_vec()
        } else if named(&[1, 2, 840, 113549, 1, 1, 13]) || named(&[1, 2, 840, 10045, 4, 3, 4]) {
            Sha512::digest(shown).to_vec()
        } else {
            Sha256::digest(shown).to_vec()
        };
        // what a proof is bound to is written as addresses of both ends, which a
        // connection of this kind has none of, and then what the application adds
        let of_the_connection = [&b"tls-server-end-point:"[..], &hash].concat();
        let mut written = vec![0; 16];
        written.extend((of_the_connection.len() as u32).to_le_bytes());
        written.extend(of_the_connection);
        Ok(Bound(Md5::digest(&written).into()))
    }
}

/// Shows a ticket as [`show`] does, with its proof bound to the connection it travels
/// on: what a service asks for that signs people in over HTTP.
pub fn show_bound(ticket: &Ticket, now: (u64, u32), fresh: Fresh, bound: &Bound) -> (Shown, Vec<u8>) {
    let ours = Key::new(ticket.key.kind(), &fresh.key[..ticket.key.kind().length()]).expect("a key as long as its kind takes");
    // the check that goes with the proof says what it is bound to and what is asked of
    // the connection
    let mut asked = 16u32.to_le_bytes().to_vec();
    asked.extend(bound.0);
    asked.extend(WHAT_IS_ASKED.to_le_bytes());
    let proof = Proof {
        realm: ticket.realm.clone(),
        account: ticket.account.clone(),
        check: Some((0x8003, asked)),
        time: now,
        key: Some(ours.clone()),
        counts_from: Some(fresh.counts_from),
    };
    let shown = messages::show(&ticket.bytes, &Sealed::with(&ticket.key, 11, fresh.begins_with, &proof.write()), true);
    let offered = der::sequence(&[der::name(KERBEROS_OF_WINDOWS), der::name(KERBEROS)]);
    let first = der::sequence(&[der::under(0, &offered), der::under(2, &der::bytes(&framed([1, 0], &shown)))]);
    let token = der::write(der::message(0), &[der::name(AGREEING), der::under(0, &first)].concat());
    (Shown { session: ticket.key.clone(), ours, time: now, counts_from: fresh.counts_from, offered }, token)
}

impl Shown {
    /// Takes the machine's answer. The machine has proven itself where it repeats the
    /// time of the client's proof, sealed with the ticket's key, which only the machine
    /// the ticket is for can have. Gives what both sides seal with from now on, and a
    /// last token to send where the machine waits for one.
    pub fn receive(self, token: &[u8]) -> Result<(Sealing, Option<Vec<u8>>), Error> {
        let mut answer = Reader::new(token).expect(der::numbered(1))?.inside().expect(der::SEQUENCE)?.inside();
        // whether it agreed: it did, or it waits for more, or it refuses
        let state = answer.optional(der::numbered(0))?.map(|state| state.holds.last().copied().unwrap_or(0));
        if state == Some(2) {
            return Err(Error::NotTrusted("the machine refused the ticket"));
        }
        answer.optional(der::numbered(1))?;
        let carried = answer.under(2)?.ok_or(Error::NotTrusted("the machine answered the ticket without proving itself"))?;
        let check_on_what_was_offered = answer.under(3)?.map(|check| check.holds);

        // its token: Kerberos, an answer to a ticket, and the answer
        let inside = Reader::new(carried.holds).expect(der::message(0))?.holds;
        let rest = inside.strip_prefix(&der::name(KERBEROS)[..]).ok_or(WRONG)?;
        let rest = rest.strip_prefix(&[2, 0]).ok_or(WRONG)?;
        let returned = messages::read_returned(&messages::read_shown(rest)?.open(&self.session, 12)?)?;
        if returned.time != self.time {
            return Err(Error::NotTrusted("the machine answered another ticket's proof than the one that was shown"));
        }
        // the machine's key for what follows, where it made one; else the client's own
        let (key, of_the_machine) = match returned.key {
            Some(key) => (key, true),
            None => (self.ours, false),
        };
        let mut sealing =
            Sealing { key, of_the_machine, ours: u64::from(self.counts_from), theirs: u64::from(returned.counts_from.unwrap_or(0)) };
        // a check on what the client offered, where the machine sends one, has to hold:
        // nobody changed the offer on the way
        if let Some(check) = check_on_what_was_offered {
            sealing.check_theirs(&self.offered, check)?;
        }
        // a machine that waits for more waits for the client's own check on the offer
        let last = (state == Some(1)).then(|| {
            let ours = sealing.check(&self.offered);
            der::write(der::numbered(1), &der::sequence(&[der::under(0, &[0x0a, 1, 0]), der::under(3, &der::bytes(&ours))]))
        });
        Ok((sealing, last))
    }
}

/// What both sides seal with once the ticket was shown and answered.
#[derive(Debug)]
pub struct Sealing {
    key: Key,
    of_the_machine: bool,
    ours: u64,
    theirs: u64,
}

impl Sealing {
    fn flags(&self, sealed: bool) -> u8 {
        (if sealed { SEALED } else { 0 }) | (if self.of_the_machine { WITH_THE_MACHINES_KEY } else { 0 })
    }

    /// The way of sealing that is used.
    pub fn kind(&self) -> Kind {
        self.key.kind()
    }

    /// Seals a message for the machine. `begins_with` is sixteen random bytes.
    ///
    /// The message and the head again are sealed together, with nothing to fill it up
    /// and nothing turned (RFC 4121, 4.2.4): the cipher needs no filling, and a machine
    /// reads what is not turned whichever way it turns what it sends itself.
    pub fn seal(&mut self, message: &[u8], begins_with: [u8; 16]) -> Vec<u8> {
        let mut head = vec![0x05, 0x04, self.flags(true), 0xff, 0, 0, 0, 0];
        head.extend(self.ours.to_be_bytes());
        self.ours += 1;
        let sealed = self.key.seal(24, begins_with, &[message, &head].concat());
        head.extend(sealed);
        head
    }

    /// Opens a message the machine sealed.
    pub fn open(&mut self, token: &[u8]) -> Result<Vec<u8>, Error> {
        const NOT_SEALED: Error = Error::NotOpened("it is not a message the machine sealed");
        let (head, sealed) = token.split_at_checked(16).ok_or(NOT_SEALED)?;
        if head[..2] != [0x05, 0x04] || head[2] & SENT_BY_THE_MACHINE == 0 || head[2] & SEALED == 0 || head[3] != 0xff || sealed.is_empty()
        {
            return Err(NOT_SEALED);
        }
        let filled_with = usize::from(u16::from_be_bytes([head[4], head[5]]));
        let turned_by = usize::from(u16::from_be_bytes([head[6], head[7]]));
        let number = u64::from_be_bytes(head[8..16].try_into().expect("eight bytes"));
        // what is sealed was turned so that its end comes first: by what the head says, or,
        // as Windows turns what it fills up, by that and by what fills it up (MS-KILE
        // 3.4.5.4.1). Only the right way opens it
        let turned = |by: usize| {
            let mut turned = sealed.to_vec();
            turned.rotate_left(by % sealed.len());
            self.key.open(22, &turned)
        };
        let plain = match turned(turned_by) {
            Err(_) if filled_with != 0 => turned(turned_by + filled_with)?,
            opened => opened?,
        };
        // it ends with the head again, as it was before the turning was written into it
        let mut expected = head.to_vec();
        expected[6..8].copy_from_slice(&[0, 0]);
        let message = plain.len().checked_sub(16 + filled_with).ok_or(NOT_SEALED)?;
        if plain[message + filled_with..] != expected[..] {
            return Err(Error::NotOpened("its head was changed on the way"));
        }
        if number != self.theirs {
            return Err(Error::NotOpened("it is not the machine's next message"));
        }
        self.theirs += 1;
        Ok(plain[..message].to_vec())
    }

    /// A check on a message that is not sealed, for the machine.
    pub fn check(&mut self, message: &[u8]) -> Vec<u8> {
        let mut head = vec![0x04, 0x04, self.flags(false), 0xff, 0xff, 0xff, 0xff, 0xff];
        head.extend(self.ours.to_be_bytes());
        self.ours += 1;
        let check = self.key.check(25, &[message, &head].concat());
        head.extend(check);
        head
    }

    /// Whether the machine's check on a message holds.
    fn check_theirs(&mut self, message: &[u8], check: &[u8]) -> Result<(), Error> {
        const NO_CHECK: Error = Error::NotTrusted("the machine's check on what was offered does not hold");
        let (head, check) = check.split_at_checked(16).ok_or(NO_CHECK)?;
        if head[..2] != [0x04, 0x04] || head[2] & SENT_BY_THE_MACHINE == 0 || check != self.key.check(23, &[message, head].concat()) {
            return Err(NO_CHECK);
        }
        self.theirs = self.theirs.max(u64::from_be_bytes(head[8..16].try_into().expect("eight bytes")) + 1);
        Ok(())
    }
}

/// The machine's side of it, for the tests' stand-in for one. It knows the key that goes
/// with the ticket, as a machine does once it has opened the ticket with its own key.
#[cfg(any(test, feature = "stand-in"))]
pub mod stand_in {
    use super::*;

    /// What a machine answers a ticket with, and what it seals with afterwards.
    pub struct Machine {
        key: Key,
        of_the_machine: bool,
        ours: u64,
        theirs: u64,
    }

    /// What a client's first token says: the account it is for.
    pub struct Shown {
        /// The account's name.
        pub account: String,
        /// The realm.
        pub realm: String,
        /// What the client asks of the connection.
        pub asked: u32,
        /// What the client's proof is bound to.
        pub bound: Bound,
    }

    /// What a client's first token carries: the ticket and its proof, as their fields.
    fn carried(token: &[u8]) -> Result<Reader<'_>, Error> {
        let inside = Reader::new(token).expect(der::message(0))?.holds;
        let first = inside.strip_prefix(&der::name(AGREEING)[..]).ok_or(WRONG)?;
        let mut first = Reader::new(first).expect(der::numbered(0))?.inside().expect(der::SEQUENCE)?.inside();
        first.skip_to(2)?;
        let rest = Reader::new(first.needed(2)?.holds).expect(der::message(0))?.holds;
        let rest = rest.strip_prefix(&der::name(KERBEROS)[..]).and_then(|rest| rest.strip_prefix(&[1, 0])).ok_or(WRONG)?;
        Ok(Reader::new(rest).expect(der::message(14))?.inside().expect(der::SEQUENCE)?.inside())
    }

    /// The key that goes with the ticket a client's first token carries, as a machine
    /// finds it: it opens the ticket with the key of its own service, which the domain
    /// controller sealed the ticket with.
    pub fn key_in_the_ticket(token: &[u8], of_the_service: &Key) -> Result<Key, Error> {
        let mut shown = carried(token)?;
        shown.skip_to(3)?;
        let mut ticket = shown.needed(3)?.inside().expect(der::SEQUENCE)?.inside();
        ticket.skip_to(3)?;
        let mut sealed = ticket.needed(3)?.inside();
        sealed.skip_to(2)?;
        let opened = of_the_service.open(2, sealed.needed(2)?.holds)?;
        let mut inside = Reader::new(&opened).expect(der::message(3))?.inside().expect(der::SEQUENCE)?.inside();
        inside.skip_to(1)?;
        let mut key = inside.needed(1)?.inside();
        let kind = Kind::of(key.needed(0)?.number()?).ok_or(WRONG)?;
        Key::new(kind, key.needed(1)?.holds).ok_or(WRONG)
    }

    /// A ticket for a service as a domain controller makes one, with no more in it than
    /// a machine reads to find the key that goes with it.
    pub fn ticket(realm: &str, service: &messages::Name, of_the_service: &Key, goes_with_it: &Key, begins_with: [u8; 16]) -> Vec<u8> {
        let key =
            der::sequence(&[der::under(0, &der::number(goes_with_it.kind() as i64)), der::under(1, &der::bytes(goes_with_it.expose()))]);
        let inside = der::write(der::message(3), &der::sequence(&[der::under(0, &der::bits(&[0x40, 0x81, 0, 0])), der::under(1, &key)]));
        let sealed = der::sequence(&[
            der::under(0, &der::number(of_the_service.kind() as i64)),
            der::under(2, &der::bytes(&of_the_service.seal(2, begins_with, &inside))),
        ]);
        der::write(
            der::message(1),
            &der::sequence(&[
                der::under(0, &der::number(5)),
                der::under(1, &der::text(realm)),
                der::under(2, &service.write()),
                der::under(3, &sealed),
            ]),
        )
    }

    /// How a machine ends its answer to a ticket.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum Ends {
        /// It is content, and says nothing of what the client offered.
        Content,
        /// It is content, and adds its check on what the client offered.
        ContentWithItsCheck,
        /// It adds its check on what the client offered, and waits for the client's own.
        WaitsForTheClientsCheck,
    }

    /// Reads a client's first token with the key that goes with its ticket, and answers
    /// it: with a key of the machine's own, or without, and ending as it is told to.
    pub fn answer(
        token: &[u8],
        session: &Key,
        own: Option<[u8; 32]>,
        ends: Ends,
        begins_with: [u8; 16],
    ) -> Result<(Machine, Shown, Vec<u8>), Error> {
        let inside = Reader::new(token).expect(der::message(0))?.holds;
        let first = inside.strip_prefix(&der::name(AGREEING)[..]).ok_or(WRONG)?;
        let offered = Reader::new(first).expect(der::numbered(0))?.inside().expect(der::SEQUENCE)?.inside().needed(0)?.whole.to_vec();
        let mut shown = carried(token)?;
        shown.skip_to(4)?;
        let mut sealed = shown.needed(4)?.inside();
        sealed.skip_to(2)?;
        let proof = session.open(11, sealed.needed(2)?.holds)?;
        let mut proof = Reader::new(&proof).expect(der::message(2))?.inside().expect(der::SEQUENCE)?.inside();
        proof.skip_to(1)?;
        let realm = proof.needed(1)?.text()?.to_string();
        let mut name = proof.needed(2)?.inside();
        name.skip_to(1)?;
        let account = name.needed(1)?.inside().expect(der::GENERAL_STRING)?.text()?.to_string();
        let mut check = proof.needed(3)?.inside();
        check.skip_to(1)?;
        let check = check.needed(1)?.holds;
        let asked = u32::from_le_bytes(check.get(20..24).and_then(|asked| asked.try_into().ok()).ok_or(WRONG)?);
        let bound = Bound(check.get(4..20).and_then(|bound| bound.try_into().ok()).ok_or(WRONG)?);
        let millionths = proof.needed(4)?.number()?;
        let seconds = proof.needed(5)?;
        let theirs_key = proof.needed(6)?;
        let theirs_from = proof.needed(7)?.number()? as u64;
        let mut theirs_key = theirs_key.inside();
        theirs_key.skip_to(1)?;
        let theirs_key = Key::new(session.kind(), theirs_key.needed(1)?.holds).ok_or(WRONG)?;

        // the answer: the time of the proof, repeated, and what the machine adds
        let ours_from = 0x4000_0000u32;
        let own = own.map(|own| Key::new(session.kind(), &own[..session.kind().length()]).expect("a key"));
        let mut fields = vec![der::under(0, seconds.whole), der::under(1, &der::number(millionths))];
        if let Some(own) = &own {
            fields.push(der::under(
                2,
                &der::sequence(&[der::under(0, &der::number(own.kind() as i64)), der::under(1, &der::bytes(own.expose()))]),
            ));
        }
        fields.push(der::under(3, &der::number(i64::from(ours_from))));
        let returned = der::write(der::message(27), &der::sequence(&fields));
        let sealed = session.seal(12, begins_with, &returned);
        let sealed = der::sequence(&[der::under(0, &der::number(session.kind() as i64)), der::under(2, &der::bytes(&sealed))]);
        let answered = der::write(
            der::message(15),
            &der::sequence(&[der::under(0, &der::number(5)), der::under(1, &der::number(15)), der::under(2, &sealed)]),
        );
        let mut machine =
            Machine { of_the_machine: own.is_some(), key: own.unwrap_or(theirs_key), ours: u64::from(ours_from), theirs: theirs_from };
        let state = if ends == Ends::WaitsForTheClientsCheck { 1 } else { 0 };
        let mut fields = vec![
            der::under(0, &[0x0a, 1, state]),
            der::under(1, &der::name(KERBEROS_OF_WINDOWS)),
            der::under(2, &der::bytes(&framed([2, 0], &answered))),
        ];
        if ends != Ends::Content {
            fields.push(der::under(3, &der::bytes(&machine.check(&offered))));
        }
        Ok((machine, Shown { account, realm, asked, bound }, der::write(der::numbered(1), &der::sequence(&fields))))
    }

    impl Machine {
        fn flags(&self, sealed: bool) -> u8 {
            SENT_BY_THE_MACHINE | (if sealed { SEALED } else { 0 }) | (if self.of_the_machine { WITH_THE_MACHINES_KEY } else { 0 })
        }

        fn check(&mut self, message: &[u8]) -> Vec<u8> {
            let mut head = vec![0x04, 0x04, self.flags(false), 0xff, 0xff, 0xff, 0xff, 0xff];
            head.extend(self.ours.to_be_bytes());
            self.ours += 1;
            let check = self.key.check(23, &[message, &head].concat());
            head.extend(check);
            head
        }

        /// Seals a message for the client, filled up and turned as the machine chooses:
        /// turned by what its head says, with what fills it up as Windows does.
        pub fn seal(&mut self, message: &[u8], begins_with: [u8; 16], filled_with: usize, turned_by: usize) -> Vec<u8> {
            self.seal_turned(message, begins_with, filled_with, turned_by, true)
        }

        /// The same, turned by what its head says alone, as the standard has it.
        pub fn seal_as_the_standard_does(
            &mut self,
            message: &[u8],
            begins_with: [u8; 16],
            filled_with: usize,
            turned_by: usize,
        ) -> Vec<u8> {
            self.seal_turned(message, begins_with, filled_with, turned_by, false)
        }

        fn seal_turned(
            &mut self,
            message: &[u8],
            begins_with: [u8; 16],
            filled_with: usize,
            turned_by: usize,
            with_what_fills: bool,
        ) -> Vec<u8> {
            let mut head = vec![0x05, 0x04, self.flags(true), 0xff];
            head.extend((filled_with as u16).to_be_bytes());
            head.extend(0u16.to_be_bytes());
            head.extend(self.ours.to_be_bytes());
            self.ours += 1;
            let mut plain = message.to_vec();
            plain.extend(vec![0xff; filled_with]);
            plain.extend_from_slice(&head);
            let mut sealed = self.key.seal(22, begins_with, &plain);
            let by = (turned_by + if with_what_fills { filled_with } else { 0 }) % sealed.len();
            sealed.rotate_right(by);
            head[6..8].copy_from_slice(&(turned_by as u16).to_be_bytes());
            head.extend(sealed);
            head
        }

        /// Opens a message the client sealed.
        pub fn open(&mut self, token: &[u8]) -> Result<Vec<u8>, Error> {
            let (head, sealed) = token.split_at_checked(16).ok_or(WRONG)?;
            let filled_with = usize::from(u16::from_be_bytes([head[4], head[5]]));
            let turned_by = usize::from(u16::from_be_bytes([head[6], head[7]]));
            let mut sealed = sealed.to_vec();
            let by = (turned_by + filled_with) % sealed.len();
            sealed.rotate_left(by);
            let plain = self.key.open(24, &sealed)?;
            if u64::from_be_bytes(head[8..16].try_into().expect("eight bytes")) != self.theirs {
                return Err(Error::NotOpened("it is not the client's next message"));
            }
            self.theirs += 1;
            Ok(plain[..plain.len() - 16 - filled_with].to_vec())
        }

        /// Whether the client's check on what it offered holds.
        pub fn check_holds(&mut self, token: &[u8], offered_by: &[u8]) -> bool {
            let Ok(last) = Reader::new(token).expect(der::numbered(1)).and_then(|last| last.inside().expect(der::SEQUENCE)) else {
                return false;
            };
            let mut last = last.inside();
            if last.skip_to(3).is_err() {
                return false;
            }
            let Ok(Some(check)) = last.under(3) else { return false };
            let Some((head, check)) = check.holds.split_at_checked(16) else { return false };
            // what the client offered, as it stands in its first token
            let Ok(inside) = Reader::new(offered_by).expect(der::message(0)) else { return false };
            let Some(first) = inside.holds.strip_prefix(&der::name(AGREEING)[..]) else { return false };
            let Ok(first) = Reader::new(first).expect(der::numbered(0)).and_then(|first| first.inside().expect(der::SEQUENCE)) else {
                return false;
            };
            let Ok(offered) = first.inside().needed(0) else { return false };
            self.theirs += 1;
            check == self.key.check(25, &[offered.whole, head].concat())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kerberos::messages::tests::{bytes, ticket};
    use crate::kerberos::messages::{Name, NameKind};

    fn a_ticket() -> Ticket {
        Ticket {
            bytes: ticket(),
            key: Key::new(Kind::Aes256, &[5; 32]).unwrap(),
            realm: "CORP.EXAMPLE".to_string(),
            account: Name::new(NameKind::Account, &["alex.urban"]),
            service: Name::new(NameKind::Service, &["TERMSRV", "desk-01.corp.example"]),
            service_realm: "CORP.EXAMPLE".to_string(),
            until: 1_794_736_800,
        }
    }

    fn fresh() -> Fresh {
        Fresh { key: [6; 32], begins_with: [3; 16], counts_from: 1000 }
    }

    #[test]
    fn a_proof_is_bound_to_the_certificate_the_service_showed_on_its_connection() {
        use crate::certificate::tests::{AUTHORITY, CONTROLLER};
        // as RFC 5929 and RFC 4121 say it, worked out apart from this code
        let bound = Bound::to_the_certificate(CONTROLLER).expect("a certificate");
        assert_eq!(bound, Bound(bytes("f0cb80deb6a09895dffbbfa7189527be").try_into().unwrap()));
        assert_ne!(bound, Bound::to_the_certificate(AUTHORITY).unwrap());
        assert!(Bound::to_the_certificate(&CONTROLLER[..100]).is_err());

        // the machine reads in the proof what it is bound to, and nothing where it is bound to nothing
        let (_, token) = show_bound(&a_ticket(), (1_794_700_800, 123_456), fresh(), &bound);
        let (_, said, _) = stand_in::answer(&token, &a_ticket().key, None, stand_in::Ends::Content, [4; 16]).unwrap();
        assert_eq!(said.bound, bound);
        let (_, token) = show(&a_ticket(), (1_794_700_800, 123_456), fresh());
        let (_, said, _) = stand_in::answer(&token, &a_ticket().key, None, stand_in::Ends::Content, [4; 16]).unwrap();
        assert_eq!(said.bound, Bound([0; 16]));
    }

    #[test]
    fn a_ticket_is_shown_the_machine_proves_itself_and_both_seal_for_each_other() {
        use stand_in::Ends::{Content, ContentWithItsCheck, WaitsForTheClientsCheck};
        for (own, ends) in [
            (Some([7; 32]), Content),
            (None, Content),
            (Some([7; 32]), WaitsForTheClientsCheck),
            (None, WaitsForTheClientsCheck),
            (None, ContentWithItsCheck),
        ] {
            let (shown, token) = show(&a_ticket(), (1_794_700_800, 123_456), fresh());
            // the token offers Kerberos, in the frame of agreeing on a way
            assert_eq!(&token[..2], &[0x60, 0x82]);
            assert_eq!(&token[4..12], &[0x06, 0x06, 0x2b, 0x06, 0x01, 0x05, 0x05, 0x02]);
            let (mut machine, said, answer) = stand_in::answer(&token, &a_ticket().key, own, ends, [4; 16]).unwrap();
            assert_eq!((said.account.as_str(), said.realm.as_str(), said.asked), ("alex.urban", "CORP.EXAMPLE", 0x3e));
            let (mut sealing, last) = shown.receive(&answer).unwrap();
            // a machine that waits for the client's check on what was offered gets it
            assert_eq!(last.is_some(), ends == WaitsForTheClientsCheck, "{ends:?}");
            if let Some(last) = last {
                assert!(machine.check_holds(&last, &token), "the client's check holds");
            }

            // the client seals, and the machine opens: nothing fills it up, and nothing is turned
            let sealed = sealing.seal(b"what the client says", [8; 16]);
            assert_eq!(&sealed[..2], &[0x05, 0x04]);
            assert_eq!(sealed[2], if own.is_some() { 0x06 } else { 0x02 });
            assert_eq!(&sealed[3..8], &[0xff, 0, 0, 0, 0]);
            assert_eq!(sealed.len(), 16 + 16 + 20 + 16 + 12);
            assert_eq!(machine.open(&sealed).as_deref(), Ok(&b"what the client says"[..]));
            // the machine seals, in each way a machine may, and the client opens
            for (filled_with, turned_by) in [(16, 28), (0, 0), (5, 28), (0, 28)] {
                let sealed = machine.seal(b"what the machine says", [9; 16], filled_with, turned_by);
                assert_eq!(
                    sealing.open(&sealed).as_deref(),
                    Ok(&b"what the machine says"[..]),
                    "filled with {filled_with}, turned by {turned_by}"
                );
                let sealed = machine.seal_as_the_standard_does(b"what the machine says", [9; 16], filled_with, turned_by);
                assert_eq!(
                    sealing.open(&sealed).as_deref(),
                    Ok(&b"what the machine says"[..]),
                    "the standard's way, filled with {filled_with}"
                );
            }
            // a message that came before, one that was changed, and one of the client's own
            let again = machine.seal(b"once", [9; 16], 16, 28);
            assert!(sealing.open(&again).is_ok());
            assert_eq!(sealing.open(&again), Err(Error::NotOpened("it is not the machine's next message")));
            let mut changed = machine.seal(b"changed", [9; 16], 16, 28);
            let at = changed.len() - 1;
            changed[at] ^= 1;
            assert_eq!(sealing.open(&changed), Err(Error::NotOpened("its check does not hold")));
            let own_message = sealing.seal(b"mine", [8; 16]);
            assert_eq!(sealing.open(&own_message), Err(Error::NotOpened("it is not a message the machine sealed")));
            assert!(sealing.open(&[0x05, 0x04]).is_err());
        }
    }

    #[test]
    fn messages_are_sealed_as_another_implementation_of_the_standards_seals_them() {
        // made with another implementation, with the machine's key of the test above:
        // a client's message, numbered 1000, and a machine's, numbered from where the
        // tests' machine counts
        let (shown, token) = show(&a_ticket(), (1_794_700_800, 123_456), fresh());
        let (mut machine, _, answer) = stand_in::answer(&token, &a_ticket().key, Some([7; 32]), stand_in::Ends::Content, [4; 16]).unwrap();
        let (mut sealing, _) = shown.receive(&answer).unwrap();
        let from_a_client = bytes(
            "050406ff000c001c00000000000003e8756bea04c125cb50c3cef3c20db0ded71abab5234e018fbc75500eda32c0704275b6650b72317fad6d92dd2e1721d17607f2e71991959b6413866865e7f52defcf00737ea4aac2e262a20c06",
        );
        assert_eq!(machine.open(&from_a_client).as_deref(), Ok(&b"what the client says"[..]));
        let from_a_machine = bytes(
            "050407ff000b001c0000000040000000a31250abffacedc4b51eae871d7161243a493375e4e2171a86603f302a7cb313729d2e7216dd38ec9f8a5e607f4a7d74ff5db46f1773dc390b5c05a538c23239d6044803e1600a0c19ce6c2c",
        );
        assert_eq!(sealing.open(&from_a_machine).as_deref(), Ok(&b"what the machine says"[..]));
        // and what this code seals has the same head as theirs, but for what fills it up and how it is turned
        let ours = sealing.seal(b"what the client says", [8; 16]);
        assert_eq!((&ours[..4], &ours[8..16]), (&from_a_client[..4], &from_a_client[8..16]));
    }

    #[test]
    fn a_machine_that_does_not_have_the_tickets_key_proves_nothing() {
        let (shown, token) = show(&a_ticket(), (1_794_700_800, 123_456), fresh());
        // somebody else answers: with another key, for another proof, or with a refusal
        let other = Key::new(Kind::Aes256, &[1; 32]).unwrap();
        assert!(stand_in::answer(&token, &other, None, stand_in::Ends::Content, [4; 16]).is_err(), "it can't even read the proof");
        let (_, _, answer) = stand_in::answer(&token, &a_ticket().key, None, stand_in::Ends::Content, [4; 16]).unwrap();
        let (later, _) = show(&a_ticket(), (1_794_700_801, 0), fresh());
        assert_eq!(
            later.receive(&answer).unwrap_err(),
            Error::NotTrusted("the machine answered another ticket's proof than the one that was shown")
        );
        let refusal = der::write(der::numbered(1), &der::sequence(&[der::under(0, &[0x0a, 1, 2])]));
        let (again, _) = show(&a_ticket(), (1_794_700_800, 123_456), fresh());
        assert_eq!(again.receive(&refusal).unwrap_err(), Error::NotTrusted("the machine refused the ticket"));
        // a check on what was offered that does not hold
        let (_, _, mut with_check) = stand_in::answer(&token, &a_ticket().key, None, stand_in::Ends::ContentWithItsCheck, [4; 16]).unwrap();
        let at = with_check.len() - 1;
        with_check[at] ^= 1;
        assert_eq!(shown.receive(&with_check).unwrap_err(), Error::NotTrusted("the machine's check on what was offered does not hold"));
        // cut anywhere, the answer is refused
        let (_, _, answer) =
            stand_in::answer(&token, &a_ticket().key, Some([7; 32]), stand_in::Ends::WaitsForTheClientsCheck, [4; 16]).unwrap();
        for cut in 0..answer.len() {
            let (shown, _) = show(&a_ticket(), (1_794_700_800, 123_456), fresh());
            assert!(shown.receive(&answer[..cut]).is_err(), "cut at {cut}");
        }
    }
}
