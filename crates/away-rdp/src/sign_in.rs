//! The sign-in before the desktop: once the connection is secured, the client signs the
//! person in before the machine shows anything (CredSSP, MS-CSSP).
//!
//! Its messages are records that carry the messages of a way of signing in, here NTLM.
//! It goes in three exchanges:
//!
//! 1. the client says what it can do, and the machine answers with a challenge and says
//!    who it is;
//! 2. the client proves that it knows the account's password, and proves that it sees
//!    the same certificate as the machine holds; the machine proves the same back, so
//!    nobody can sit in between;
//! 3. the client hands over the account's name and password, sealed, for the machine to
//!    sign the person in on its desktop.
//!
//! A machine that chose to say so then tells whether the account may come in at all.

use sha2::{Digest, Sha256};

use crate::der::{self, Reader};
use crate::ntlm::{self, Account, Challenge, Fresh, Session};
use crate::Error;

/// The version of the sign-in that Away speaks: the one that binds the proofs to the
/// connection with a number made for it.
pub const VERSION: u32 = 6;

const TO_MACHINE: &[u8] = b"CredSSP Client-To-Server Binding Hash\0";
const FROM_MACHINE: &[u8] = b"CredSSP Server-To-Client Binding Hash\0";

/// What a message of the sign-in holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Message {
    /// The version of the sign-in its sender speaks.
    pub version: u32,
    /// The message of the way of signing in that it carries, if it carries one.
    pub carried: Option<Vec<u8>>,
    /// The account's name and password, sealed.
    pub credentials: Option<Vec<u8>>,
    /// The proof that its sender sees the same certificate, sealed.
    pub proof: Option<Vec<u8>>,
    /// The machine's own code for why it refuses, if it refuses.
    pub refused: Option<u32>,
    /// The number the client made for this sign-in.
    pub number: Option<Vec<u8>>,
}

impl Message {
    /// The message, written out.
    pub fn write(&self) -> Vec<u8> {
        let mut fields = der::write(der::of_its_own(0), &der::write_number(self.version));
        if let Some(carried) = &self.carried {
            let token = der::write(der::of_its_own(0), &der::write(der::OCTET_STRING, carried));
            let tokens = der::write(der::SEQUENCE, &der::write(der::SEQUENCE, &token));
            fields.extend(der::write(der::of_its_own(1), &tokens));
        }
        for (number, value) in [(2, &self.credentials), (3, &self.proof)] {
            if let Some(value) = value {
                fields.extend(der::write(der::of_its_own(number), &der::write(der::OCTET_STRING, value)));
            }
        }
        if let Some(code) = self.refused {
            fields.extend(der::write(der::of_its_own(4), &der::write_number(code)));
        }
        if let Some(number) = &self.number {
            fields.extend(der::write(der::of_its_own(5), &der::write(der::OCTET_STRING, number)));
        }
        der::write(der::SEQUENCE, &fields)
    }

    /// Reads one whole message.
    pub fn read(message: &[u8]) -> Result<Message, Error> {
        const WRONG: Error = Error::SignIn("a message of the sign-in is not built as one");
        let as_sign_in = |_| WRONG;
        let mut outer = Reader::new(message);
        let mut fields = Reader::new(outer.expect(der::SEQUENCE).map_err(as_sign_in)?);
        let mut read = Message::default();
        while !fields.is_empty() {
            let field = fields.value().map_err(as_sign_in)?;
            let mut inside = Reader::new(field.holds);
            let mut bytes = || inside.expect(der::OCTET_STRING).map(<[u8]>::to_vec).map_err(as_sign_in);
            match field.kind & 0x1f {
                1 if field.kind == der::of_its_own(1) => {
                    let mut tokens = Reader::new(inside.expect(der::SEQUENCE).map_err(as_sign_in)?);
                    let mut token = Reader::new(tokens.expect(der::SEQUENCE).map_err(as_sign_in)?);
                    let mut carried = Reader::new(token.expect(der::of_its_own(0)).map_err(as_sign_in)?);
                    read.carried = Some(carried.expect(der::OCTET_STRING).map_err(as_sign_in)?.to_vec());
                }
                2 => read.credentials = Some(bytes()?),
                3 => read.proof = Some(bytes()?),
                5 => read.number = Some(bytes()?),
                0 => read.version = der::read_number(inside.expect(der::INTEGER).map_err(as_sign_in)?)?,
                4 => read.refused = Some(der::read_number(inside.expect(der::INTEGER).map_err(as_sign_in)?)?),
                _ => {}
            }
        }
        if read.version == 0 {
            return Err(WRONG);
        }
        Ok(read)
    }
}

/// What a sign-in needs that has to be new each time, taken from the system's random
/// bytes by whoever holds the connection.
#[derive(Clone)]
pub struct New {
    /// What the proof of the password needs.
    pub for_the_proof: Fresh,
    /// The number that binds the proofs to this connection.
    pub number: [u8; 32],
}

/// What to do next.
#[derive(Debug, PartialEq, Eq)]
pub enum Next {
    /// Send this, and hand over the machine's answer when it has arrived.
    Send(Vec<u8>),
    /// Send this. The sign-in is then complete on the client's side.
    SendLast(Vec<u8>),
}

enum State {
    AwaitsChallenge,
    AwaitsMachinesProof,
    Done,
}

/// One sign-in before the desktop, from its first message to its last.
pub struct SignIn {
    state: State,
    account: Account,
    serves: String,
    public_key: Vec<u8>,
    new: New,
    clock: u64,
    first: Vec<u8>,
    session: Option<Session>,
    machine: Option<Challenge>,
}

impl SignIn {
    /// Begins a sign-in, and gives the first message to send.
    ///
    /// `machine` is the name the machine was reached by, and `public_key` the key in the
    /// certificate it showed when the connection was secured. `clock` is the time now, in
    /// ten-millionths of a second since 1601, used where the machine doesn't send its own.
    pub fn begin(account: Account, machine: &str, public_key: &[u8], new: New, clock: u64) -> (SignIn, Vec<u8>) {
        let first = ntlm::negotiate();
        let message = Message { version: VERSION, carried: Some(first.clone()), ..Message::default() }.write();
        let sign_in = SignIn {
            state: State::AwaitsChallenge,
            account,
            serves: format!("TERMSRV/{machine}"),
            public_key: public_key.to_vec(),
            new,
            clock,
            first,
            session: None,
            machine: None,
        };
        (sign_in, message)
    }

    /// Who the machine said it is, once its challenge has arrived.
    pub fn machine(&self) -> Option<&Challenge> {
        self.machine.as_ref()
    }

    /// Takes one whole message from the machine, and says what to do next.
    pub fn receive(&mut self, message: &[u8]) -> Result<Next, Error> {
        let read = Message::read(message)?;
        if let Some(code) = read.refused {
            return Err(Error::SignInRefused(code));
        }
        match self.state {
            State::AwaitsChallenge => {
                if read.version < 5 {
                    return Err(Error::SignIn("the machine speaks a version of the sign-in older than this code speaks"));
                }
                let second = read.carried.ok_or(Error::SignIn("the machine's answer carries no challenge"))?;
                let challenge = ntlm::read_challenge(&second)?;
                let (third, mut session) =
                    ntlm::authenticate(&self.first, &second, &challenge, &self.account, &self.serves, &self.new.for_the_proof, self.clock);
                let proof = session.seal(&self.bound(TO_MACHINE));
                self.session = Some(session);
                self.machine = Some(challenge);
                self.state = State::AwaitsMachinesProof;
                Ok(Next::Send(
                    Message {
                        version: VERSION,
                        carried: Some(third),
                        proof: Some(proof),
                        number: Some(self.new.number.to_vec()),
                        ..Message::default()
                    }
                    .write(),
                ))
            }
            State::AwaitsMachinesProof => {
                let sealed = read.proof.ok_or(Error::SignIn("the machine's answer carries no proof that it sees the same certificate"))?;
                let session = self.session.as_mut().ok_or(Error::SignIn("the sign-in is out of order"))?;
                if session.open(&sealed)? != self.bound(FROM_MACHINE) {
                    return Err(Error::SignIn(
                        "the machine's proof is for another certificate than the one this connection showed: someone may be in between",
                    ));
                }
                let session = self.session.as_mut().ok_or(Error::SignIn("the sign-in is out of order"))?;
                let credentials = session.seal(&credentials(&self.account));
                self.state = State::Done;
                Ok(Next::SendLast(Message { version: VERSION, credentials: Some(credentials), ..Message::default() }.write()))
            }
            State::Done => Err(Error::SignIn("the sign-in is complete, and the machine sent more of it")),
        }
    }

    /// What binds a proof to this connection: a hash over which way it goes, the number
    /// made for this sign-in, and the key in the certificate the connection showed.
    fn bound(&self, way: &[u8]) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(way);
        hash.update(self.new.number);
        hash.update(&self.public_key);
        hash.finalize().into()
    }
}

/// The account's name and password, as the machine takes them to sign the person in.
pub fn credentials(account: &Account) -> Vec<u8> {
    let mut fields = Vec::new();
    for (number, text) in [(0, account.domain.as_str()), (1, account.name.as_str()), (2, account.password.expose())] {
        fields.extend(der::write(der::of_its_own(number), &der::write(der::OCTET_STRING, &ntlm::wide(text))));
    }
    let password = der::write(der::SEQUENCE, &fields);
    // the kind of credential, a name and a password, and the credential itself
    let mut record = der::write(der::of_its_own(0), &der::write_number(1));
    record.extend(der::write(der::of_its_own(1), &der::write(der::OCTET_STRING, &password)));
    der::write(der::SEQUENCE, &record)
}

/// Reads the machine's verdict on whether the account may come in, which a machine sends
/// after the sign-in when it chose to say so.
pub fn read_verdict(verdict: [u8; 4]) -> Result<(), Error> {
    match u32::from_le_bytes(verdict) {
        0 => Ok(()),
        code => Err(Error::MayNotComeIn(code)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret::Secret;
    use crate::stand_in::StandIn;

    const KEY: &[u8] = b"the key in the certificate the connection showed";

    fn new() -> New {
        New { for_the_proof: Fresh { challenge: [0xaa; 8], key: [0x55; 16] }, number: [0x11; 32] }
    }

    fn account(password: &str) -> Account {
        Account::named("CORP\\alex.urban", Secret::new(password))
    }

    #[test]
    fn the_first_message_carries_ntlms_first_and_says_its_version() {
        let (_, message) = SignIn::begin(account("a password"), "fin-app-02.corp.example.com", KEY, new(), 0);
        assert_eq!(der::whole(&message), Ok(Some(message.len())));
        // a record of two fields: the version 6, and a list of one message
        assert_eq!(&message[..7], &[0x30, 0x37, 0xa0, 0x03, 0x02, 0x01, 0x06]);
        assert_eq!(&message[7..17], &[0xa1, 0x30, 0x30, 0x2e, 0x30, 0x2c, 0xa0, 0x2a, 0x04, 0x28]);
        assert_eq!(&message[17..], &ntlm::negotiate()[..]);
        assert_eq!(Message::read(&message).unwrap(), Message { version: 6, carried: Some(ntlm::negotiate()), ..Message::default() });
    }

    #[test]
    fn a_person_is_signed_in_and_the_machine_gets_the_name_and_the_password() {
        let mut machine = StandIn::new(account("a password"), KEY);
        let (mut sign_in, first) = SignIn::begin(account("a password"), "fin-app-02.corp.example.com", KEY, new(), 0);
        let Next::Send(second) = sign_in.receive(&machine.answer(&first).unwrap()).unwrap() else { panic!("the proof is not the last") };
        assert_eq!(sign_in.machine().unwrap().computer.as_deref(), Some("FIN-APP-02"));
        let Next::SendLast(third) = sign_in.receive(&machine.answer(&second).unwrap()).unwrap() else {
            panic!("the credentials are the last")
        };
        assert!(machine.answer(&third).unwrap().is_empty(), "the machine has nothing more to say");
        assert_eq!(machine.signed_in(), Some(("CORP".to_string(), "alex.urban".to_string(), "a password".to_string())));
        // the password is in no message as it is
        let password = ntlm::wide("a password");
        for message in [&first, &second, &third] {
            assert!(!message.windows(password.len()).any(|part| part == password));
        }
        assert!(sign_in.receive(&third).is_err(), "nothing follows the last message");
    }

    #[test]
    fn a_wrong_password_is_refused_in_words() {
        let mut machine = StandIn::new(account("a password"), KEY);
        let (mut sign_in, first) = SignIn::begin(account("another password"), "fin-app-02.corp.example.com", KEY, new(), 0);
        let Next::Send(second) = sign_in.receive(&machine.answer(&first).unwrap()).unwrap() else { panic!() };
        let refused = sign_in.receive(&machine.answer(&second).unwrap()).unwrap_err();
        assert_eq!(refused, Error::SignInRefused(0xc000_006d));
        assert_eq!(refused.to_string(), "It refused the sign-in: the name or the password is wrong (its own code 0xc000006d).");
        assert_eq!(machine.signed_in(), None);
    }

    #[test]
    fn a_machine_that_shows_another_certificate_than_it_holds_is_caught() {
        // someone in between: the connection showed one key, the machine holds another
        let mut machine = StandIn::new(account("a password"), b"the key the machine really holds");
        let (mut sign_in, first) = SignIn::begin(account("a password"), "fin-app-02.corp.example.com", KEY, new(), 0);
        let Next::Send(second) = sign_in.receive(&machine.answer(&first).unwrap()).unwrap() else { panic!() };
        // the machine refuses the client's proof, because it is for another key
        assert!(machine.answer(&second).is_err());
        // and a machine that answered all the same is refused by the client
        let mut lenient = StandIn::new(account("a password"), b"the key the machine really holds");
        lenient.checks_the_clients_proof = false;
        let (mut sign_in, first) = SignIn::begin(account("a password"), "fin-app-02.corp.example.com", KEY, new(), 0);
        let Next::Send(second) = sign_in.receive(&lenient.answer(&first).unwrap()).unwrap() else { panic!() };
        let caught = sign_in.receive(&lenient.answer(&second).unwrap()).unwrap_err();
        assert!(caught.to_string().contains("someone may be in between"), "{caught}");
        assert_eq!(lenient.signed_in(), None, "the name and the password were never sent");
    }

    #[test]
    fn the_machines_verdict_is_read() {
        assert_eq!(read_verdict([0, 0, 0, 0]), Ok(()));
        assert_eq!(read_verdict([5, 0, 0, 0]), Err(Error::MayNotComeIn(5)));
        assert_eq!(Error::MayNotComeIn(5).to_string(), "It took the sign-in, and says this account may not come in over RDP here.");
    }

    #[test]
    fn a_message_that_is_cut_or_changed_is_refused_and_never_read_past_its_end() {
        let mut machine = StandIn::new(account("a password"), KEY);
        let (_, first) = SignIn::begin(account("a password"), "fin-app-02.corp.example.com", KEY, new(), 0);
        let whole = machine.answer(&first).unwrap();
        for cut in 0..whole.len() {
            let (mut sign_in, _) = SignIn::begin(account("a password"), "fin-app-02.corp.example.com", KEY, new(), 0);
            assert!(sign_in.receive(&whole[..cut]).is_err(), "cut at {cut}");
        }
        for at in 0..whole.len() {
            let mut changed = whole.clone();
            changed[at] ^= 0xff;
            let (mut sign_in, _) = SignIn::begin(account("a password"), "fin-app-02.corp.example.com", KEY, new(), 0);
            let _ = sign_in.receive(&changed);
        }
        assert!(Message::read(&der::write(der::SEQUENCE, &[])).is_err(), "a record that names no version");
        let old = Message {
            version: 2,
            carried: Some(ntlm::challenge_for_tests("FIN-APP-02", "CORP", "corp.example.com", 9_600)),
            ..Message::default()
        };
        let (mut sign_in, _) = SignIn::begin(account("a password"), "fin-app-02.corp.example.com", KEY, new(), 0);
        assert!(sign_in.receive(&old.write()).unwrap_err().to_string().contains("older than this code speaks"));
    }
}
