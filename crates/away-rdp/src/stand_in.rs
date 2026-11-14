//! The machine's side of the sign-in before the desktop, for tests and for whatever
//! stands in for a machine. It knows one account and its password, and answers as a
//! Windows machine of a domain does. What follows the sign-in is the [`desktop`]'s.
//!
//! It is built only for tests: nothing of it is in a release.

pub mod desktop;

use sha2::{Digest, Sha256};

use crate::ntlm::{self, Account, Session};
use crate::sign_in::Message;
use crate::{der, Error};

/// A stand-in for a machine.
pub struct StandIn {
    account: Account,
    public_key: Vec<u8>,
    first: Vec<u8>,
    second: Vec<u8>,
    session: Option<Session>,
    signed_in: Option<(String, String, String)>,
    /// Whether it checks that the client's proof is for the certificate it holds, as a
    /// machine does. A test turns it off to play a machine that doesn't.
    pub checks_the_clients_proof: bool,
}

impl StandIn {
    /// A stand-in that knows this account, and holds a certificate with this key.
    pub fn new(account: Account, public_key: &[u8]) -> StandIn {
        StandIn {
            account,
            public_key: public_key.to_vec(),
            first: Vec::new(),
            second: Vec::new(),
            session: None,
            signed_in: None,
            checks_the_clients_proof: true,
        }
    }

    /// The domain, the name and the password the client handed over, once it has.
    pub fn signed_in(&self) -> Option<(String, String, String)> {
        self.signed_in.clone()
    }

    /// Answers one whole message of the client's. The answer to the last is empty.
    pub fn answer(&mut self, message: &[u8]) -> Result<Vec<u8>, Error> {
        let read = Message::read(message)?;
        let answer = |message: Message| Ok(Message { version: 6, ..message }.write());
        if let Some(sealed) = read.credentials {
            let session = self.session.as_mut().ok_or(Error::SignIn("the name and the password came before the proof"))?;
            self.signed_in = Some(credentials(&session.open(&sealed)?)?);
            return Ok(Vec::new());
        }
        let carried = read.carried.ok_or(Error::SignIn("the message carries nothing"))?;
        if self.second.is_empty() {
            self.first = carried;
            self.second = ntlm::challenge_for_tests("FIN-APP-02", "CORP", "corp.example.com", 26_100);
            return answer(Message { carried: Some(self.second.clone()), ..Message::default() });
        }
        let Some(key) = self.check(&carried)? else {
            // "the logon attempt failed"
            return answer(Message { refused: Some(0xc000_006d), ..Message::default() });
        };
        let mut session = Session::as_the_machine(&key);
        let number = read.number.ok_or(Error::SignIn("the client made no number for this sign-in"))?;
        let bound = |way: &[u8]| -> [u8; 32] {
            let mut hash = Sha256::new();
            hash.update(way);
            hash.update(&number);
            hash.update(&self.public_key);
            hash.finalize().into()
        };
        let proof = session.open(&read.proof.ok_or(Error::SignIn("the client sent no proof that it sees the same certificate"))?)?;
        if self.checks_the_clients_proof && proof != bound(b"CredSSP Client-To-Server Binding Hash\0") {
            return Err(Error::SignIn("the client's proof is for another certificate than the one this machine holds"));
        }
        let own = session.seal(&bound(b"CredSSP Server-To-Client Binding Hash\0"));
        self.session = Some(session);
        answer(Message { proof: Some(own), ..Message::default() })
    }

    /// Checks the client's proof that it knows the account's password, and the check over
    /// all three messages. Gives the key for what follows, or nothing when the proof is
    /// not one for this account.
    fn check(&self, third: &[u8]) -> Result<Option<[u8; 16]>, Error> {
        ntlm::check_for_tests(&self.first, &self.second, third, &self.account)
    }
}

/// The domain, the name and the password in what the client handed over.
fn credentials(record: &[u8]) -> Result<(String, String, String), Error> {
    const WRONG: Error = Error::SignIn("the name and the password are not built as they are handed over");
    let wrong = |_| WRONG;
    let mut outer = der::Reader::new(record);
    let mut fields = der::Reader::new(outer.expect(der::SEQUENCE).map_err(wrong)?);
    let mut kind = der::Reader::new(fields.expect(der::of_its_own(0)).map_err(wrong)?);
    if der::read_number(kind.expect(der::INTEGER).map_err(wrong)?)? != 1 {
        return Err(WRONG);
    }
    let mut holds = der::Reader::new(fields.expect(der::of_its_own(1)).map_err(wrong)?);
    let mut inner = der::Reader::new(holds.expect(der::OCTET_STRING).map_err(wrong)?);
    let mut texts = der::Reader::new(inner.expect(der::SEQUENCE).map_err(wrong)?);
    let mut text = |number: u8| -> Result<String, Error> {
        let mut field = der::Reader::new(texts.expect(der::of_its_own(number)).map_err(wrong)?);
        Ok(ntlm::utf16(field.expect(der::OCTET_STRING).map_err(wrong)?))
    };
    Ok((text(0)?, text(1)?, text(2)?))
}
