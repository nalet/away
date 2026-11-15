//! Asks the organisation's own authority for certificates (ADR-0008): over the pages it
//! publishes for that, signed in as the account the organisation gave Away for it.
//!
//! Nothing of Away's runs beside the authority. The authority itself decides what the
//! account may ask for, and in whose name: what comes back is a certificate, or the
//! authority's refusal in its own words.
//!
//! The account signs in with Kerberos, with a ticket of the domain's, and its proof is
//! bound to the connection it travels on: passed on by somebody in between, it is
//! worth nothing. The pages are asked over TLS alone.

pub mod pages;
#[cfg(feature = "stand-in")]
pub mod stand_in;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use away_part::address::Address;
use away_part::base64;
use away_part::client::{Client, Failed};
use away_secret::Secret;
use away_winauth::kerberos::exchange::{Later, Ticket, WithAPassword};
use away_winauth::kerberos::gss::{self, Bound, Fresh};
use away_winauth::kerberos::messages::{Name, NameKind};

use pages::Answer;

/// How long a domain controller gets to answer.
const WITHIN: Duration = Duration::from_secs(10);
/// A ticket that is good for less than this long is not shown any more.
const TOO_SHORT: u64 = 30;

/// Why no certificate came back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Why {
    /// The authority refused, and says this about it.
    Refused(String),
    /// The authority could not be asked, or what it answered is no answer.
    NotAsked(String),
}

impl std::fmt::Display for Why {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Why::Refused(said) => write!(out, "The authority refused. It says: {said}"),
            Why::NotAsked(why) => out.write_str(why),
        }
    }
}

/// The organisation's authority, as Away asks it.
pub struct Authority {
    client: Client,
    pages: Address,
    controllers: Vec<(String, u16)>,
    realm: String,
    account: String,
    password: Secret,
    ticket: tokio::sync::Mutex<Option<Ticket>>,
}

fn now() -> (u64, u32) {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    (now.as_secs(), now.subsec_micros())
}

fn random<const N: usize>() -> Result<[u8; N], Why> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).map_err(|error| Why::NotAsked(format!("The system gave no random bytes: {error}.")))?;
    Ok(bytes)
}

impl Authority {
    /// An authority whose pages are at this address, such as
    /// `https://authority.corp.example/certsrv/`, asked as this account of the realm.
    /// Each domain controller is named with its port where that is not Kerberos' own;
    /// they are asked in their order, the next where one is not reached.
    pub fn new(
        client: Client,
        pages: &str,
        controllers: &[String],
        realm: &str,
        account: &str,
        password: Secret,
    ) -> Result<Authority, String> {
        let mut pages = Address::read(pages).map_err(|why| format!("The address of the authority's pages can't be used: {why}."))?;
        if !pages.secured {
            return Err("The authority's pages are asked over TLS alone: their address begins with https://.".to_string());
        }
        if pages.path.contains('?') {
            return Err("The address of the authority's pages asks nothing after a question mark.".to_string());
        }
        if !pages.path.ends_with('/') {
            pages.path.push('/');
        }
        let controllers = controllers
            .iter()
            .map(|controller| match controller.rsplit_once(':') {
                Some((host, port)) => Ok((host.to_string(), port.parse::<u16>().map_err(|_| format!("{port} is no port."))?)),
                None => Ok((controller.to_string(), 88)),
            })
            .collect::<Result<Vec<_>, String>>()?;
        if controllers.is_empty() || controllers.iter().any(|(host, _)| host.is_empty()) || realm.is_empty() || account.is_empty() {
            return Err("The authority is asked as an account of a realm, with a domain controller that gives its tickets.".to_string());
        }
        Ok(Authority {
            client,
            pages,
            controllers,
            realm: realm.to_string(),
            account: account.to_string(),
            password,
            ticket: tokio::sync::Mutex::new(None),
        })
    }

    /// Hands a request to the authority, for a certificate of this kind as the authority
    /// names its kinds, and gives the certificate.
    pub async fn ask(&self, request: &[u8], kind: &str) -> Result<Vec<u8>, Why> {
        let reach = |failed: Failed| Why::NotAsked(format!("The authority's pages: {failed}."));
        let ticket = self.ticket().await?;
        let mut talk = self.client.talk(&self.pages).await.map_err(reach)?;
        let shown = talk.shown().ok_or_else(|| Why::NotAsked("The authority's pages showed no certificate of their own.".to_string()))?;
        let bound = Bound::to_the_certificate(shown)
            .map_err(|error| Why::NotAsked(format!("The certificate the authority's pages showed can't be read: {error}.")))?;
        // every request shows the ticket with a proof of its own
        let signed_in = || -> Result<Secret, Why> {
            let fresh = Fresh { key: random()?, begins_with: random()?, counts_from: u32::from_be_bytes(random()?) & 0x3fff_ffff };
            let (_, token) = gss::show_bound(&ticket, now(), fresh, &bound);
            Ok(Secret::new(format!("Negotiate {}", base64::ordinary(&token))))
        };

        let form = pages::form(request, kind);
        let pairs: Vec<(&str, &str)> = form.iter().map(|(name, value)| (*name, value.as_str())).collect();
        let at = format!("{}{}", self.pages.path, pages::ASKED_AT);
        let answered = talk.send_form(&at, &[("authorization", &signed_in()?)], &pairs).await.map_err(reach)?;
        let number = match answered.status {
            200 => match pages::read(&String::from_utf8_lossy(&answered.body)) {
                Answer::Issued(number) => number,
                Answer::Refused(said) => return Err(Why::Refused(said)),
                Answer::Other(shows) => {
                    return Err(Why::NotAsked(format!(
                        "The authority's pages gave neither a certificate nor a refusal. They show: {shows}"
                    )))
                }
            },
            401 => {
                self.ticket.lock().await.take();
                return Err(Why::NotAsked(
                    "The authority's pages don't take the sign-in of the account Away asks as: they take no Kerberos, or not this account."
                        .to_string(),
                ));
            }
            403 => return Err(Why::NotAsked("The authority's pages are not open to the account Away asks as.".to_string())),
            status => return Err(Why::NotAsked(format!("The authority's pages answered with the status {status}."))),
        };

        let at = format!("{}{}", self.pages.path, pages::issued_at(number));
        let file = talk.get(&at, &[("authorization", &signed_in()?)]).await.map_err(reach)?;
        if file.status != 200 {
            return Err(Why::NotAsked(format!(
                "The authority issued the certificate under its number {number}, and its pages answered with the status {} when it was fetched.",
                file.status
            )));
        }
        pages::certificate(&file.body).ok_or_else(|| {
            Why::NotAsked(format!(
                "The authority issued the certificate under its number {number}, and what its pages gave for it is no certificate."
            ))
        })
    }

    /// The ticket the account shows the pages: the one it has while that is good, and
    /// else a new one.
    async fn ticket(&self) -> Result<Ticket, Why> {
        let mut kept = self.ticket.lock().await;
        if let Some(ticket) = kept.as_ref().filter(|ticket| ticket.until > now().0 + TOO_SHORT) {
            return Ok(ticket.clone());
        }
        let said = |error: away_winauth::Error| {
            Why::NotAsked(format!("The domain controller, asked for a ticket for the account Away asks the authority as: {error}."))
        };
        let nonce = u32::from_be_bytes(random()?) & 0x7fff_ffff;
        let (mut asking, request) = WithAPassword::begin(&self.account, &self.realm, now(), nonce);
        let reply = self.ask_the_controller(&request).await?;
        let proof = asking.prove(&reply, self.password.expose(), now(), random()?).map_err(said)?;
        let first = asking.receive(&self.ask_the_controller(&proof).await?).map_err(said)?;
        let service = Name::new(NameKind::Service, &["HTTP", &self.pages.host]);
        let (later, request) = Later::begin(&first, service, now(), nonce ^ 0x5555_5555, random()?);
        let ticket = later.receive(&self.ask_the_controller(&request).await?).map_err(said)?;
        *kept = Some(ticket.clone());
        Ok(ticket)
    }

    /// Sends a domain controller one request, and gives what came back: the first of
    /// them that answers.
    async fn ask_the_controller(&self, request: &[u8]) -> Result<Vec<u8>, Why> {
        let mut last = Why::NotAsked("No domain controller is named.".to_string());
        for (host, port) in &self.controllers {
            match self.ask_one_controller(host, *port, request).await {
                Ok(answered) => return Ok(answered),
                Err(why) => last = why,
            }
        }
        Err(last)
    }

    async fn ask_one_controller(&self, host: &str, port: u16, request: &[u8]) -> Result<Vec<u8>, Why> {
        away_part::framed::ask(host, port, request, WITHIN).await.map_err(|why| Why::NotAsked(format!("The domain controller {why}.")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn authority(pages: &str, controller: &str) -> Result<Authority, String> {
        Authority::new(Client::new(None).unwrap(), pages, &[controller.to_string()], "CORP.EXAMPLE", "svc-away", Secret::new("a password"))
    }

    #[test]
    fn the_pages_are_asked_over_tls_alone_and_named_with_their_folder() {
        let asked = authority("https://authority.corp.example/certsrv", "dc-01.corp.example").expect("an authority");
        assert_eq!((asked.pages.path.as_str(), asked.controllers.clone()), ("/certsrv/", vec![("dc-01.corp.example".to_string(), 88)]));
        assert_eq!(authority("https://authority.corp.example/certsrv/", "dc-01.corp.example:1088").unwrap().controllers[0].1, 1088);
        assert!(authority("http://127.0.0.1/certsrv/", "dc-01.corp.example").err().unwrap().contains("over TLS alone"));
        assert!(authority("https://authority.corp.example/certsrv/?a=b", "dc-01.corp.example").is_err());
        assert!(authority("https://authority.corp.example/certsrv/", "dc-01.corp.example:port").is_err());
        assert!(authority("https://authority.corp.example/certsrv/", "").is_err());
    }
}
