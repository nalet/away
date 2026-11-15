//! The sign-in before the desktop, with a ticket in place of a proof of the password
//! (MS-CSSP, with Kerberos as the way of signing in).
//!
//! Its messages are those of the sign-in with a password ([`away_rdp::sign_in`]). What
//! they carry differs: the client shows a ticket for the machine, and the machine proves
//! itself by answering it. Then, as there, each side proves that it sees the same
//! certificate, and the client hands over what the machine signs the person in with.

use away_rdp::ntlm::Account;
use away_rdp::sign_in::{self, Message, Next};
use sha2::{Digest, Sha256};

use crate::kerberos::exchange::Ticket;
use crate::kerberos::gss::{self, Sealing, Shown};
use crate::Error;

const TO_MACHINE: &[u8] = b"CredSSP Client-To-Server Binding Hash\0";
const FROM_MACHINE: &[u8] = b"CredSSP Server-To-Client Binding Hash\0";

/// What a sign-in needs that has to be new each time, taken from the system's random
/// bytes by whoever holds the connection.
pub struct New {
    /// What showing the ticket needs.
    pub for_the_ticket: gss::Fresh,
    /// The number that binds the proofs to this connection.
    pub number: [u8; 32],
    /// Sixteen random bytes for each of the two messages the client seals.
    pub begin_with: [[u8; 16]; 2],
}

enum State {
    AwaitsTheMachinesProof(Box<Shown>),
    AwaitsTheSameCertificate,
    Done,
}

/// One sign-in before the desktop with a ticket, from its first message to its last.
pub struct SignIn {
    state: State,
    sealing: Option<Sealing>,
    public_key: Vec<u8>,
    number: [u8; 32],
    begin_with: [[u8; 16]; 2],
    hands_over: Vec<u8>,
}

impl SignIn {
    /// Begins a sign-in, and gives the first message to send.
    ///
    /// The ticket is one for the machine's desktop. `public_key` is the key in the
    /// certificate the machine showed when the connection was secured, and `now` the
    /// client's time. The account is what the machine signs the person in with on its
    /// desktop, once both have proven themselves.
    pub fn begin(ticket: &Ticket, public_key: &[u8], account: &Account, now: (u64, u32), new: New) -> (SignIn, Vec<u8>) {
        let (shown, token) = gss::show(ticket, now, new.for_the_ticket);
        let message = Message { version: sign_in::VERSION, carried: Some(token), ..Message::default() }.write();
        let sign_in = SignIn {
            state: State::AwaitsTheMachinesProof(Box::new(shown)),
            sealing: None,
            public_key: public_key.to_vec(),
            number: new.number,
            begin_with: new.begin_with,
            hands_over: sign_in::credentials(account),
        };
        (sign_in, message)
    }

    /// Takes one whole message from the machine, and says what to do next.
    pub fn receive(&mut self, message: &[u8]) -> Result<Next, Error> {
        let read = Message::read(message).map_err(Error::Machine)?;
        if let Some(code) = read.refused {
            return Err(Error::Machine(away_rdp::Error::SignInRefused(code)));
        }
        match std::mem::replace(&mut self.state, State::Done) {
            State::AwaitsTheMachinesProof(shown) => {
                if read.version < 5 {
                    return Err(Error::NotTrusted("the machine speaks a version of the sign-in older than this code speaks"));
                }
                let answer = read.carried.ok_or(Error::NotTrusted("the machine's answer carries no answer to the ticket"))?;
                let (mut sealing, last) = shown.receive(&answer)?;
                let proof = sealing.seal(&self.bound(TO_MACHINE), self.begin_with[0]);
                self.sealing = Some(sealing);
                self.state = State::AwaitsTheSameCertificate;
                Ok(Next::Send(
                    Message {
                        version: sign_in::VERSION,
                        carried: last,
                        proof: Some(proof),
                        number: Some(self.number.to_vec()),
                        ..Message::default()
                    }
                    .write(),
                ))
            }
            State::AwaitsTheSameCertificate => {
                let sealed =
                    read.proof.ok_or(Error::NotTrusted("the machine's answer carries no proof that it sees the same certificate"))?;
                let expected = self.bound(FROM_MACHINE);
                let sealing = self.sealing.as_mut().ok_or(Error::NotKerberos("the sign-in is out of order"))?;
                if sealing.open(&sealed)? != expected {
                    return Err(Error::NotTrusted(
                        "the machine's proof is for another certificate than the one this connection showed: someone may be in between",
                    ));
                }
                let handed_over = sealing.seal(&self.hands_over, self.begin_with[1]);
                Ok(Next::SendLast(Message { version: sign_in::VERSION, credentials: Some(handed_over), ..Message::default() }.write()))
            }
            State::Done => Err(Error::NotKerberos("the sign-in is complete, and the machine sent more of it")),
        }
    }

    /// What binds a proof to this connection: a hash over which way it goes, the number
    /// made for this sign-in, and the key in the certificate the connection showed.
    fn bound(&self, way: &[u8]) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(way);
        hash.update(self.number);
        hash.update(&self.public_key);
        hash.finalize().into()
    }
}

/// The machine's side of it, for the tests' stand-in for one: a machine of a domain,
/// which holds the key of its own service and the certificate it shows.
#[cfg(any(test, feature = "stand-in"))]
pub mod stand_in {
    use super::*;
    use crate::kerberos::crypto::Key;
    use crate::kerberos::gss::stand_in::{self as shown, Ends};

    /// Random bytes are for a machine that somebody could reach. This one is reached by
    /// tests alone, which are the better for getting the same bytes each time.
    const BEGINS_WITH: [u8; 16] = [0x5a; 16];
    const ITS_OWN_KEY: [u8; 32] = [0x6b; 32];

    /// A machine that takes the sign-in before the desktop with a ticket.
    pub struct Machine {
        of_the_service: Key,
        public_key: Vec<u8>,
        sealing: Option<shown::Machine>,
        proven: bool,
        /// Who the ticket is for, once it was shown: the account's name and its realm.
        pub account: Option<(String, String)>,
        /// What the client handed over, once it did: what the machine signs the person
        /// in with on its desktop.
        pub handed_over: Option<Vec<u8>>,
    }

    impl Machine {
        /// A machine with the key of its service, and the key in the certificate it shows.
        pub fn new(of_the_service: Key, public_key: &[u8]) -> Machine {
            Machine { of_the_service, public_key: public_key.to_vec(), sealing: None, proven: false, account: None, handed_over: None }
        }

        /// Answers one whole message of the client's: with a message to send, or with
        /// nothing once the account was handed over.
        pub fn answer(&mut self, message: &[u8]) -> Result<Vec<u8>, Error> {
            let read = Message::read(message).map_err(Error::Machine)?;
            let bound = |way: &[u8], number: &[u8]| -> Vec<u8> {
                let mut hash = Sha256::new();
                hash.update(way);
                hash.update(number);
                hash.update(&self.public_key);
                hash.finalize().to_vec()
            };
            let Some(sealing) = self.sealing.as_mut() else {
                // the ticket: only who holds the service's key finds the key that goes with it
                let token = read.carried.ok_or(Error::NotTrusted("the client showed no ticket"))?;
                let goes_with_it = shown::key_in_the_ticket(&token, &self.of_the_service)?;
                let (sealing, said, answer) = shown::answer(&token, &goes_with_it, Some(ITS_OWN_KEY), Ends::Content, BEGINS_WITH)?;
                self.sealing = Some(sealing);
                self.account = Some((said.account, said.realm));
                return Ok(Message { version: sign_in::VERSION, carried: Some(answer), ..Message::default() }.write());
            };
            if !self.proven {
                // the client's proof that it sees this machine's certificate, answered with the machine's own
                let (proof, number) = read.proof.zip(read.number).ok_or(Error::NotTrusted("the client proved nothing"))?;
                if sealing.open(&proof)? != bound(TO_MACHINE, &number) {
                    return Err(Error::NotTrusted("the client's proof is for another certificate than this machine's"));
                }
                self.proven = true;
                let theirs = sealing.seal(&bound(FROM_MACHINE, &number), BEGINS_WITH, 16, 28);
                return Ok(Message { version: sign_in::VERSION, proof: Some(theirs), ..Message::default() }.write());
            }
            let sealed = read.credentials.ok_or(Error::NotTrusted("the client handed nothing over"))?;
            self.handed_over = Some(sealing.open(&sealed)?);
            Ok(Vec::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kerberos::crypto::{Key, Kind};
    use crate::kerberos::gss::stand_in;
    use crate::kerberos::messages::tests::ticket;
    use crate::kerberos::messages::{Name, NameKind};
    use away_rdp::secret::Secret;

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

    fn new() -> New {
        New {
            for_the_ticket: gss::Fresh { key: [6; 32], begins_with: [3; 16], counts_from: 1000 },
            number: [0x4e; 32],
            begin_with: [[8; 16], [9; 16]],
        }
    }

    fn bound(way: &[u8], key: &[u8]) -> Vec<u8> {
        let mut hash = Sha256::new();
        hash.update(way);
        hash.update([0x4e; 32]);
        hash.update(key);
        hash.finalize().to_vec()
    }

    #[test]
    fn a_ticket_signs_in_before_the_desktop_and_the_account_is_handed_over_sealed() {
        let account = Account::named("CORP\\alex.urban", Secret::new("a password that exists for these tests alone"));
        let key = b"the key in the machine's certificate";
        let (mut signing_in, first) = SignIn::begin(&a_ticket(), key, &account, (1_794_700_800, 123_456), new());

        // the machine answers the ticket
        let first = Message::read(&first).unwrap();
        assert_eq!((first.version, first.proof.is_some(), first.number.is_some()), (6, false, false));
        let (mut machine, said, answer) =
            stand_in::answer(&first.carried.unwrap(), &a_ticket().key, Some([7; 32]), stand_in::Ends::Content, [4; 16]).unwrap();
        assert_eq!(said.account, "alex.urban");
        let Next::Send(second) = signing_in.receive(&Message { version: 6, carried: Some(answer), ..Message::default() }.write()).unwrap()
        else {
            panic!("the client goes on")
        };
        // the client proves that it sees the machine's certificate, bound to its number
        let second = Message::read(&second).unwrap();
        assert_eq!(second.number.as_deref(), Some(&[0x4e; 32][..]));
        assert_eq!(machine.open(&second.proof.unwrap()), Ok(bound(TO_MACHINE, key)));
        // the machine proves the same back, and the client hands the account over
        let theirs = machine.seal(&bound(FROM_MACHINE, key), [1; 16], 16, 28);
        let Next::SendLast(last) = signing_in.receive(&Message { version: 6, proof: Some(theirs), ..Message::default() }.write()).unwrap()
        else {
            panic!("the client's last message")
        };
        let handed_over = machine.open(&Message::read(&last).unwrap().credentials.unwrap()).unwrap();
        assert_eq!(handed_over, sign_in::credentials(&account));
        // the password travels sealed: it is in no message as it is
        let password: Vec<u8> = "a password".encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert!(handed_over.windows(password.len()).any(|part| part == password));
        assert!(!last.windows(password.len()).any(|part| part == password));
        assert_eq!(signing_in.receive(&last), Err(Error::NotKerberos("the sign-in is complete, and the machine sent more of it")));
    }

    #[test]
    fn a_machine_finds_the_tickets_key_with_its_services_key_and_takes_the_whole_sign_in() {
        // a ticket as a domain controller makes one: its key sealed with the service's
        let of_the_service = Key::new(Kind::Aes256, &[0x11; 32]).unwrap();
        let service = Name::new(NameKind::Service, &["TERMSRV", "desk-01.corp.example"]);
        let ticket = Ticket { bytes: stand_in::ticket("CORP.EXAMPLE", &service, &of_the_service, &a_ticket().key, [2; 16]), ..a_ticket() };
        let account = Account::named("CORP\\alex.urban", Secret::new("a password that exists for these tests alone"));
        let key = b"the key in the machine's certificate";

        let mut machine = super::stand_in::Machine::new(of_the_service, key);
        let (mut signing_in, mut message) = SignIn::begin(&ticket, key, &account, (1_794_700_800, 123_456), new());
        let last = loop {
            match signing_in.receive(&machine.answer(&message).expect("the machine goes on")).expect("the client goes on") {
                Next::Send(next) => message = next,
                Next::SendLast(last) => break last,
            }
        };
        assert_eq!(machine.answer(&last), Ok(Vec::new()));
        assert_eq!(machine.account, Some(("alex.urban".to_string(), "CORP.EXAMPLE".to_string())));
        assert_eq!(machine.handed_over, Some(sign_in::credentials(&account)));

        // a machine with another key than the service's can't open the ticket, and one that
        // shows another certificate is not given the proof it asks for
        let (_, first) = SignIn::begin(&ticket, key, &account, (1_794_700_800, 123_456), new());
        let mut another = super::stand_in::Machine::new(Key::new(Kind::Aes256, &[0x12; 32]).unwrap(), key);
        assert_eq!(another.answer(&first), Err(Error::NotOpened("its check does not hold")));
        let mut in_between = super::stand_in::Machine::new(Key::new(Kind::Aes256, &[0x11; 32]).unwrap(), b"another key");
        let (mut signing_in, first) = SignIn::begin(&ticket, key, &account, (1_794_700_800, 123_456), new());
        let Next::Send(proof) = signing_in.receive(&in_between.answer(&first).unwrap()).unwrap() else { panic!("the client goes on") };
        assert_eq!(in_between.answer(&proof), Err(Error::NotTrusted("the client's proof is for another certificate than this machine's")));
    }

    #[test]
    fn a_machine_in_between_or_one_that_refuses_ends_the_sign_in() {
        let account = Account::named("CORP\\alex.urban", Secret::new("a password"));
        let begin = || {
            let (mut signing_in, first) = SignIn::begin(&a_ticket(), b"the key", &account, (1_794_700_800, 123_456), new());
            let carried = Message::read(&first).unwrap().carried.unwrap();
            let (machine, _, answer) =
                stand_in::answer(&carried, &a_ticket().key, Some([7; 32]), stand_in::Ends::Content, [4; 16]).unwrap();
            signing_in.receive(&Message { version: 6, carried: Some(answer), ..Message::default() }.write()).unwrap();
            (signing_in, machine)
        };
        // a proof for another certificate: somebody shows the client one, and the machine sees another
        let (mut signing_in, mut machine) = begin();
        let theirs = machine.seal(&bound(FROM_MACHINE, b"another key"), [1; 16], 16, 28);
        assert_eq!(
            signing_in.receive(&Message { version: 6, proof: Some(theirs), ..Message::default() }.write()),
            Err(Error::NotTrusted(
                "the machine's proof is for another certificate than the one this connection showed: someone may be in between"
            ))
        );
        // no proof at all, and a refusal with the machine's own code
        let (mut signing_in, _) = begin();
        assert_eq!(
            signing_in.receive(&Message { version: 6, ..Message::default() }.write()),
            Err(Error::NotTrusted("the machine's answer carries no proof that it sees the same certificate"))
        );
        let (mut signing_in, _) = begin();
        assert_eq!(
            signing_in.receive(&Message { version: 6, refused: Some(0xc000_006d), ..Message::default() }.write()),
            Err(Error::Machine(away_rdp::Error::SignInRefused(0xc000_006d)))
        );
        // an answer that is no answer to the ticket
        let (mut signing_in, _) = SignIn::begin(&a_ticket(), b"the key", &account, (1_794_700_800, 123_456), new());
        assert!(signing_in.receive(&Message { version: 6, carried: Some(vec![1, 2, 3]), ..Message::default() }.write()).is_err());
        let (mut signing_in, _) = SignIn::begin(&a_ticket(), b"the key", &account, (1_794_700_800, 123_456), new());
        assert_eq!(
            signing_in.receive(&Message { version: 6, ..Message::default() }.write()),
            Err(Error::NotTrusted("the machine's answer carries no answer to the ticket"))
        );
    }
}
