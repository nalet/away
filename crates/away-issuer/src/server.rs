//! The server, as the issuer asks it: whose a session is, and the record of a sign-in
//! the issuer made.
//!
//! The issuer signs a person in for a session that runs, and for nothing else. Whether
//! one runs, whose it is and on which machine, is the server's to say; who the person
//! is, the issuer then checks for itself with what the sign-in service signed.

use away_part::address::Address;
use away_part::client::Client;
use away_part::json::Json;
use away_secret::Secret;

/// The server.
pub struct Server {
    client: Client,
    at: Address,
}

/// What the server answers for: a session that runs.
#[derive(Clone, Debug)]
pub struct Vouched {
    /// The person the session is of, as the sign-in service knows them.
    pub subject: String,
    /// The machine, by its name.
    pub machine: String,
    /// What the sign-in service signed about the person a moment ago, as it signed it.
    pub signed: Secret,
}

/// Why the server answered for nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotVouched {
    /// No session of that name runs.
    NoSession,
    /// The person's sign-in has run out, or the sign-in service signs nothing anew for them.
    SignedOut(String),
    /// The server could not be asked, or what it answered is no answer.
    NotAsked(String),
}

impl Server {
    /// The server at this address.
    pub fn new(client: Client, at: Address) -> Server {
        Server { client, at }
    }

    async fn ask(&self, path: &str, says: &[(&str, &str)]) -> Result<(u16, String), String> {
        let at = Address { path: path.to_string(), ..self.at.clone() };
        let got = self.client.send_form(&at, says, None).await.map_err(|failed| format!("The server: {failed}."))?;
        Ok((got.status, String::from_utf8_lossy(&got.body).trim().to_string()))
    }

    /// Asks the server to answer for a session.
    pub async fn vouch(&self, session: &str) -> Result<Vouched, NotVouched> {
        let (status, said) = self.ask("/internal/vouch", &[("session", session)]).await.map_err(NotVouched::NotAsked)?;
        match status {
            200 => {
                let said = Json::read(&said).unwrap_or(Json::Null);
                match (said.text_of("subject"), said.text_of("machine"), said.text_of("token")) {
                    (Some(subject), Some(machine), Some(signed)) if !subject.is_empty() => {
                        Ok(Vouched { subject: subject.to_string(), machine: machine.to_string(), signed: Secret::new(signed) })
                    }
                    _ => Err(NotVouched::NotAsked("The server answered for the session with nothing a sign-in can rest on.".to_string())),
                }
            }
            404 => Err(NotVouched::NoSession),
            403 => Err(NotVouched::SignedOut(said)),
            status => Err(NotVouched::NotAsked(format!("The server answered with the status {status}: {said}"))),
        }
    }

    /// Has the server keep the record of a sign-in that was made for a session.
    pub async fn made(&self, session: &str, account: &str, number: &str, until: u64) -> Result<(), String> {
        let until = until.to_string();
        let says = [("session", session), ("what", "signed-in"), ("as", account), ("number", number), ("until", until.as_str())];
        match self.ask("/internal/told", &says).await? {
            (200, _) => Ok(()),
            (status, said) => Err(format!("The server kept no record of the sign-in: it answered with the status {status}: {said}")),
        }
    }
}
