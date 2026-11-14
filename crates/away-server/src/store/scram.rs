//! Proving a password to the database without sending it (SCRAM-SHA-256, RFC 5802 and
//! RFC 7677): both sides prove that they know it, and what travels is of no use to
//! anyone who hears it.
//!
//! The hashes and the stretching of the password are a library's. The messages around
//! them are written here.

use away_part::base64;
use away_secret::Secret;
use ring::{digest, hmac, pbkdf2};

/// Why the proof did not go through.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failed(pub &'static str);

/// The first step under way: what the client said, to be proven later.
pub struct Begun {
    nonce: String,
    first: String,
}

/// The second step under way: what the database has to prove in its turn.
pub struct Proven {
    its_key: hmac::Key,
    all: String,
}

/// How often the database may have a password stretched, at most: more is an attempt to
/// keep the server busy.
const MOST_ROUNDS: u32 = 10_000_000;

/// The way of proving a password that this is.
pub const WAY: &str = "SCRAM-SHA-256";

/// Begins: what the client says first. The fresh bytes are random, and new each time.
pub fn begin(fresh: &[u8; 18]) -> (Begun, Vec<u8>) {
    let nonce = base64::ordinary(fresh);
    // the database knows the account from the connection's first message, so none is named here
    let first = format!("n=,r={nonce}");
    let says = format!("n,,{first}");
    (Begun { nonce, first }, says.into_bytes())
}

impl Begun {
    /// Takes the database's answer and gives the proof of the password.
    pub fn prove(self, answer: &[u8], password: &Secret) -> Result<(Proven, Vec<u8>), Failed> {
        let answer = std::str::from_utf8(answer).map_err(|_| Failed("the database's answer to the proof is no text"))?;
        let part = |name: &str| answer.split(',').find_map(|part| part.strip_prefix(name));
        let nonce = part("r=").ok_or(Failed("the database's answer to the proof names no number used once"))?;
        // the number used once has to be the client's own, carried on by the database
        if nonce.len() <= self.nonce.len() || !nonce.starts_with(&self.nonce) {
            return Err(Failed("the database answers a proof that is not this one"));
        }
        let salt = part("s=").and_then(read).ok_or(Failed("the database's answer to the proof names no salt"))?;
        let rounds = part("i=")
            .and_then(|rounds| rounds.parse::<u32>().ok())
            .and_then(std::num::NonZeroU32::new)
            .filter(|rounds| rounds.get() <= MOST_ROUNDS)
            .ok_or(Failed("the database's answer to the proof says no number of rounds that can be"))?;

        let mut stretched = [0u8; 32];
        pbkdf2::derive(pbkdf2::PBKDF2_HMAC_SHA256, rounds, &salt, password.expose().as_bytes(), &mut stretched);
        let stretched = hmac::Key::new(hmac::HMAC_SHA256, &stretched);
        let client_key = hmac::sign(&stretched, b"Client Key");
        let stored = digest::digest(&digest::SHA256, client_key.as_ref());
        let without_proof = format!("c=biws,r={nonce}");
        let all = format!("{},{answer},{without_proof}", self.first);
        let signed = hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, stored.as_ref()), all.as_bytes());
        let proof: Vec<u8> = client_key.as_ref().iter().zip(signed.as_ref()).map(|(key, signed)| key ^ signed).collect();

        let its_key = hmac::Key::new(hmac::HMAC_SHA256, hmac::sign(&stretched, b"Server Key").as_ref());
        let says = format!("{without_proof},p={}", base64::ordinary(&proof)).into_bytes();
        Ok((Proven { its_key, all }, says))
    }
}

impl Proven {
    /// Takes the database's last word: its own proof that it knows the password too.
    pub fn check(self, last: &[u8]) -> Result<(), Failed> {
        let last = std::str::from_utf8(last).map_err(|_| Failed("the database's last word on the proof is no text"))?;
        let its = last
            .split(',')
            .find_map(|part| part.strip_prefix("v="))
            .and_then(read)
            .ok_or(Failed("the database does not take the password"))?;
        // checked in a time that says nothing of where the two differ
        hmac::verify(&self.its_key, self.all.as_bytes(), &its)
            .map_err(|_| Failed("the database does not prove that it knows the password: it is not the database it is taken for"))
    }
}

/// The bytes of letters written the ordinary way, with their padding.
fn read(letters: &str) -> Option<Vec<u8>> {
    base64::read_ordinary(letters)
}

impl std::fmt::Debug for Proven {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str("a proof under way")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The example of RFC 7677, section 3, which names an account in its first message.
    #[test]
    fn the_example_of_the_standard() {
        let nonce = "rOprNGfwEbeRWgbNEkqO";
        let begun = Begun { nonce: nonce.to_string(), first: format!("n=user,r={nonce}") };
        let answer = b"r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096";
        let (proven, says) = begun.prove(answer, &Secret::new("pencil")).unwrap();
        assert_eq!(
            String::from_utf8(says).unwrap(),
            "c=biws,r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,p=dHzbZapWIk4jUhN+Ute9ytag9zjfMHgsqmmiz7AndVQ="
        );
        assert_eq!(proven.check(b"v=6rriTRBi23WpRR/wtup+mMhUZUn/dB5nLTJRsjl95G4="), Ok(()));
    }

    #[test]
    fn what_the_client_says_first_names_no_account_and_a_number_used_once() {
        let (begun, says) = begin(&[7; 18]);
        assert_eq!(String::from_utf8(says).unwrap(), "n,,n=,r=BwcHBwcHBwcHBwcHBwcHBwcH");
        assert_eq!(begun.first, "n=,r=BwcHBwcHBwcHBwcHBwcHBwcH");
    }

    #[test]
    fn an_answer_that_is_not_to_this_proof_or_asks_what_cannot_be_is_refused() {
        let answers: [(&[u8], &str); 6] = [
            (b"r=other,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096", "the database answers a proof that is not this one"),
            (b"r=BwcHBwcHBwcHBwcHBwcHBwcH,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096", "the database answers a proof that is not this one"),
            (b"s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096", "the database's answer to the proof names no number used once"),
            (b"r=BwcHBwcHBwcHBwcHBwcHBwcHx,s=?,i=4096", "the database's answer to the proof names no salt"),
            (
                b"r=BwcHBwcHBwcHBwcHBwcHBwcHx,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=0",
                "the database's answer to the proof says no number of rounds that can be",
            ),
            (
                b"r=BwcHBwcHBwcHBwcHBwcHBwcHx,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4000000000",
                "the database's answer to the proof says no number of rounds that can be",
            ),
        ];
        for (answer, why) in answers {
            let (begun, _) = begin(&[7; 18]);
            assert_eq!(begun.prove(answer, &Secret::new("pencil")).err(), Some(Failed(why)));
        }
    }

    #[test]
    fn a_database_that_does_not_know_the_password_is_not_trusted() {
        let prove = || {
            let (begun, _) = begin(&[7; 18]);
            begun.prove(b"r=BwcHBwcHBwcHBwcHBwcHBwcHx,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096", &Secret::new("pencil")).unwrap().0
        };
        assert_eq!(prove().check(b"e=invalid-proof"), Err(Failed("the database does not take the password")));
        assert!(prove().check(b"v=6rriTRBi23WpRR/wtup+mMhUZUn/dB5nLTJRsjl95G4=").is_err());
        assert!(prove().check(b"v=").is_err());
    }
}
