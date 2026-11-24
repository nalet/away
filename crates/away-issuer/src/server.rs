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
    /// The admission method fixed by the server.
    pub signs_in: String,
    /// Every destination allowed for this resource, including collection hosts.
    pub targets: Vec<String>,
    /// The person the session is of, as the sign-in service knows them.
    pub subject: String,
    /// The machine, by its name.
    pub machine: String,
    /// What the sign-in service signed about the person a moment ago, as it signed it.
    pub signed: Secret,
    /// The secret kept for the machine, by its name, where the machine's entry names one.
    pub kept: Option<String>,
    /// That secret as Away's own store keeps it, sealed to the issuer, where it keeps it.
    pub kept_sealed: Option<Vec<u8>>,
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
                    (Some(subject), Some(machine), Some(signed)) if !subject.is_empty() => Ok(Vouched {
                        signs_in: said.text_of("signs_in").unwrap_or_default().to_string(),
                        targets: said
                            .get("targets")
                            .and_then(Json::list)
                            .unwrap_or_default()
                            .iter()
                            .filter_map(Json::text)
                            .map(str::to_string)
                            .collect(),
                        subject: subject.to_string(),
                        machine: machine.to_string(),
                        signed: Secret::new(signed),
                        kept: said.text_of("kept").map(str::to_string),
                        kept_sealed: said.text_of("kept_sealed").and_then(|letters| {
                            away_part::base64::read_for_addresses(&letters.replace('+', "-").replace('/', "_").replace('=', ""))
                        }),
                    }),
                    _ => Err(NotVouched::NotAsked("The server answered for the session with nothing a sign-in can rest on.".to_string())),
                }
            }
            404 => Err(NotVouched::NoSession),
            403 => Err(NotVouched::SignedOut(said)),
            status => Err(NotVouched::NotAsked(format!("The server answered with the status {status}: {said}"))),
        }
    }

    /// Has the server keep the record of a kept secret that was handed out for a session:
    /// which one, by its name, and nothing of the secret itself.
    pub async fn handed(&self, session: &str, kept: &str) -> Result<(), String> {
        match self.ask("/internal/told", &[("session", session), ("what", "kept-secret"), ("kept", kept)]).await? {
            (200, _) => Ok(()),
            (status, said) => Err(format!("The server kept no record of the kept secret: it answered with the status {status}: {said}")),
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
