//! Signing the person of a session in on a Windows machine as themselves, with nothing
//! typed ([ADR-0007](../../../spec/architecture/decisions/0007-short-lived-certificates.md)).
//!
//! The gateway makes a key for the session, which never leaves its memory, and asks
//! the issuer for a certificate for it. With the certificate it proves the person to
//! the domain, over Kerberos, and gets a ticket for the machine. The machine is shown
//! the ticket in the sign-in before the desktop, and handed a smart card that exists
//! in the gateway's memory alone: it reads the card over the session's connection and
//! signs the person in as it does with a card at a desk.

use std::time::Duration;

use away_part::address::Address;
use away_part::base64;
use away_part::client::Client;
use away_part::json::Json;
use away_part::time;
use away_rdp::smartcard::Reader;
use away_winauth::certificate::Certificate;
use away_winauth::kerberos::exchange::{First, Later, Ticket};
use away_winauth::kerberos::messages::{Name, NameKind};
use away_winauth::kerberos::pkinit::{Fresh, Held, Trusts};
use away_winauth::request::{self, Key};
use away_winauth::sign_in::OnACard;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::CertificateDer;

use crate::machine::{Failure, Step, Why};

/// How long a domain controller gets to answer.
const WITHIN: Duration = Duration::from_secs(10);
/// The name of the reader the card is in, and of the machine's own software that reads
/// a card of this kind.
const READER: &str = "Away 0";
/// The issuer's word for a person Away signs in as themselves on no machine: an account
/// or a group that the installation names for that, such as the directory's administrators.
const NEVER: &str = "never";
const PROVIDER: &str = "Microsoft Base Smart Card Crypto Provider";

fn not(details: impl ToString) -> Failure {
    Failure { step: Step::SigningIn, why: Why::NotThemselves, details: details.to_string() }
}

fn random<const N: usize>() -> Result<[u8; N], Failure> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).map_err(|error| not(format!("the system gave no random bytes: {error}")))?;
    Ok(bytes)
}

fn now() -> (u64, u32) {
    let now = time::now();
    (now.as_secs(), now.subsec_micros())
}

/// The issuer, as the gateway asks it.
pub struct Issuer {
    client: Client,
    at: Address,
}

impl Issuer {
    /// The issuer at this address.
    pub fn new(client: Client, at: Address) -> Issuer {
        Issuer { client, at }
    }

    /// Asks for what signs the person of a session in: a certificate for the key of
    /// this request. Nothing where the issuer says that Away signs this person in as
    /// themselves on no machine; any other refusal is given in the issuer's own words.
    async fn certificate(&self, session: &str, asked: &[u8]) -> Result<Option<Vec<u8>>, Failure> {
        let at = Address { path: "/internal/certificate".to_string(), ..self.at.clone() };
        let request = base64::ordinary(asked);
        let got = self
            .client
            .send_form(&at, &[("session", session), ("request", &request)], None)
            .await
            .map_err(|failed| not(format!("the issuer: {failed}")))?;
        let said = std::str::from_utf8(&got.body).ok().and_then(|text| Json::read(text).ok()).unwrap_or(Json::Null);
        if got.status != 200 {
            return match (said.text_of("why"), said.text_of("says")) {
                (Some(NEVER), Some(_)) => Ok(None),
                (Some(why), Some(says)) => Err(not(format!("{says} ({why})"))),
                _ => Err(not(format!("the issuer answered with the status {}", got.status))),
            };
        }
        said.text_of("certificate")
            .and_then(|letters| base64::read_for_addresses(&letters.replace('+', "-").replace('/', "_").replace('=', "")))
            .map(Some)
            .ok_or_else(|| not("the issuer answered with nothing that is a certificate"))
    }
}

/// The domain whose machines people are signed in on as themselves, as the gateway is
/// told of it.
pub struct Domain {
    realm: String,
    controllers: Vec<(String, u16)>,
    /// The authorities a domain controller's certificate has to lead to, each as it is written.
    authorities: Vec<Vec<u8>>,
    issuer: Issuer,
}

/// A person as the domain knows them, for one session: what signs them in, its key,
/// and their first ticket.
pub struct Themselves {
    certificate: Vec<u8>,
    key: Vec<u8>,
    name: String,
    first: Ticket,
    /// The domain controller that gave the ticket.
    controller: usize,
}

/// What signs a person in on one machine: the ticket for it, and the card.
pub struct OnAMachine {
    /// A ticket for the machine's desktop.
    pub ticket: Ticket,
    /// What the machine is told of the card.
    pub card: OnACard,
    /// The reader, with the card in it.
    pub reader: Reader,
}

impl Themselves {
    /// The account, as the domain writes it: its name and the realm.
    pub fn account(&self) -> String {
        format!("{}@{}", self.name, self.first.realm)
    }
}

impl Domain {
    /// A domain of this name, with these domain controllers, each with its port where
    /// that is not Kerberos' own, and the authorities their certificates lead to, as PEM.
    pub fn new(domain: &str, controllers: &[String], authorities: &[u8], issuer: Issuer) -> Result<Domain, String> {
        let controllers = controllers
            .iter()
            .map(|controller| match controller.rsplit_once(':') {
                Some((host, port)) => Ok((host.to_string(), port.parse::<u16>().map_err(|_| format!("{port} is no port"))?)),
                None => Ok((controller.to_string(), 88)),
            })
            .collect::<Result<Vec<_>, String>>()?;
        if controllers.is_empty() || controllers.iter().any(|(host, _)| host.is_empty()) {
            return Err("a domain has a domain controller that gives its tickets".to_string());
        }
        let authorities: Vec<Vec<u8>> = CertificateDer::pem_slice_iter(authorities)
            .map(|certificate| certificate.map(|certificate| certificate.as_ref().to_vec()))
            .collect::<Result<_, _>>()
            .map_err(|error| format!("a certificate of the domain's authorities can't be read: {error}"))?;
        if authorities.is_empty() || authorities.iter().any(|authority| Certificate::read(authority).is_err()) {
            return Err("the domain's authorities hold no certificate that can be read".to_string());
        }
        Ok(Domain { realm: domain.to_ascii_uppercase(), controllers, authorities, issuer })
    }

    /// Makes the person of a session who they are in the domain: a key that stays
    /// here, a certificate for it from the issuer, and with it their first ticket.
    /// Nothing for a person Away signs in as themselves on no machine: they sign in as
    /// on a machine where people type.
    pub async fn themselves(&self, session: &str) -> Result<Option<Themselves>, Failure> {
        let key = tokio::task::spawn_blocking(Key::new).await.map_err(not)?.map_err(not)?;
        let asked = request::asked(&key, "a session's key").map_err(not)?;
        let Some(certificate) = self.issuer.certificate(session, &asked).await? else { return Ok(None) };
        let key = key.written().map_err(not)?;
        // the account is the one the certificate names, as the domain writes it
        let named = Certificate::read(&certificate).map_err(not)?.account.ok_or_else(|| not("the certificate names no account"))?;
        let name = named.split('@').next().unwrap_or(&named).to_string();
        let held = Held::new(&certificate, &key).map_err(not)?;
        let authorities: Vec<&[u8]> = self.authorities.iter().map(Vec::as_slice).collect();
        let mut last = not("no domain controller is named");
        for (nth, (host, port)) in self.controllers.iter().enumerate() {
            let nonce = u32::from_be_bytes(random()?) & 0x7fff_ffff;
            let (first, request) =
                First::begin(&held, &name, &self.realm, now(), nonce, Fresh { secret: random()?, once: random()? }).map_err(not)?;
            match away_part::framed::ask(host, *port, &request, WITHIN).await {
                Ok(reply) => {
                    let first = first
                        .receive(&reply, &Trusts { authorities: &authorities, controller: host }, now().0)
                        .map_err(|error| not(format!("the domain controller {host}, asked for a first ticket for {named}: {error}")))?;
                    return Ok(Some(Themselves { certificate, key, name, first, controller: nth }));
                }
                Err(why) => last = not(format!("the domain controller {why}")),
            }
        }
        Err(last)
    }

    /// What signs them in on one machine, by the name the domain knows it by: a ticket
    /// for its desktop, and a card for this one connection, with a PIN nobody is told.
    pub async fn on(&self, who: &Themselves, machine: &str) -> Result<OnAMachine, Failure> {
        let (host, port) = &self.controllers[who.controller];
        let service = Name::new(NameKind::Service, &["TERMSRV", machine]);
        let nonce = u32::from_be_bytes(random()?) & 0x7fff_ffff;
        let (later, request) = Later::begin(&who.first, service, now(), nonce, random()?);
        let reply =
            away_part::framed::ask(host, *port, &request, WITHIN).await.map_err(|why| not(format!("the domain controller {why}")))?;
        let ticket = later
            .receive(&reply)
            .map_err(|error| not(format!("the domain controller {host}, asked for a ticket for the desktop of {machine}: {error}")))?;

        let hex = |bytes: &[u8]| bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
        let pin = hex(&random::<16>()?);
        let container = format!("away-{}", hex(&random::<12>()?));
        let (card, _) =
            away_winauth::card::Card::new(&who.certificate, &who.key, away_rdp::secret::Secret::new(&pin), &container, random()?)
                .map_err(not)?;
        let card_told = OnACard {
            pin: away_rdp::secret::Secret::new(&pin),
            name: who.name.clone(),
            domain: who.first.realm.clone(),
            card: String::new(),
            reader: READER.to_string(),
            container,
            provider: PROVIDER.to_string(),
        };
        Ok(OnAMachine { ticket, card: card_told, reader: Reader::new(READER, Box::new(card)) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AUTHORITY: &[u8] = include_bytes!("../../away-part/tests/fixtures/authority.pem");

    fn issuer() -> Issuer {
        Issuer::new(Client::new(None).unwrap(), Address::read("https://away-issuer:8083").unwrap())
    }

    #[test]
    fn a_domain_is_named_with_its_controllers_and_the_authorities_they_lead_to() {
        let domain =
            Domain::new("corp.example", &["dc-01.corp.example".to_string(), "dc-02.corp.example:1088".to_string()], AUTHORITY, issuer())
                .expect("a domain");
        assert_eq!(domain.realm, "CORP.EXAMPLE");
        assert_eq!(domain.controllers, [("dc-01.corp.example".to_string(), 88), ("dc-02.corp.example".to_string(), 1088)]);
        assert_eq!(domain.authorities.len(), 1);
        for (controllers, authorities, why) in [
            (vec![], AUTHORITY, "a domain has a domain controller that gives its tickets"),
            (vec!["dc-01:port".to_string()], AUTHORITY, "port is no port"),
            (vec!["dc-01".to_string()], &b"nothing of the kind"[..], "the domain's authorities hold no certificate that can be read"),
        ] {
            assert_eq!(Domain::new("corp.example", &controllers, authorities, issuer()).err().as_deref(), Some(why));
        }
    }
}
