//! The sign-in service, as the issuer sees it: the keys it publishes, and whether it
//! signed what somebody is said to be (OpenID Connect Discovery 1.0 and Core 3.1.3.7).
//!
//! The issuer signs nobody in on the word of Away's other parts alone. What the sign-in
//! service signed about a person is handed to it as it was signed, and it checks the
//! signature itself, against keys it reads from the service itself
//! ([ADR-0007](../../../spec/architecture/decisions/0007-short-lived-certificates.md)).

use std::sync::Mutex;

use away_part::address::Address;
use away_part::client::Client;
use away_part::json::Json;
use away_part::token::{self, Expected, Keys, Wrong};

/// How often at most the keys a service publishes are read anew because a token is
/// signed with one that is not among them: once in this many seconds.
const KEYS_ANEW_AT_MOST_EVERY: u64 = 10;

struct Known {
    keys_at: Option<Address>,
    keys: Keys,
    read_anew_at: Option<u64>,
}

/// The sign-in service.
pub struct Service {
    client: Client,
    service: String,
    away: String,
    known: Mutex<Known>,
}

impl Service {
    /// The service that names itself this, and knows Away by this name. Nothing is asked
    /// of it yet.
    pub fn new(client: Client, service: &str, away: &str) -> Service {
        Service {
            client,
            service: service.trim_end_matches('/').to_string(),
            away: away.to_string(),
            known: Mutex::new(Known { keys_at: None, keys: Keys::default(), read_anew_at: None }),
        }
    }

    async fn get(&self, at: &Address) -> Result<Json, String> {
        let got = self.client.get(at).await.map_err(|failed| format!("The sign-in service: {failed}."))?;
        if got.status != 200 {
            return Err(format!("The sign-in service answered with the status {}.", got.status));
        }
        std::str::from_utf8(&got.body)
            .ok()
            .and_then(|text| Json::read(text).ok())
            .ok_or_else(|| "The sign-in service's answer can't be read.".to_string())
    }

    /// Where the service publishes its keys, as it says itself.
    async fn keys_at(&self) -> Result<Address, String> {
        if let Some(at) = self.known.lock().ok().and_then(|known| known.keys_at.clone()) {
            return Ok(at);
        }
        let unusable = |why: String| format!("What the sign-in service says of itself can't be used: {why}.");
        let asked = Address::read(&format!("{}/.well-known/openid-configuration", self.service))
            .map_err(|why| format!("The sign-in service's address can't be used: {why}."))?;
        let says = self.get(&asked).await?;
        if says.text_of("issuer").map(|issuer| issuer.trim_end_matches('/')) != Some(self.service.as_str()) {
            return Err(unusable(format!(
                "it names itself {}, and Away is told {}",
                says.text_of("issuer").unwrap_or("nothing"),
                self.service
            )));
        }
        let written = says.text_of("jwks_uri").ok_or_else(|| unusable("it has no jwks_uri".to_string()))?;
        let at = Address::read(written).map_err(|why| unusable(format!("its jwks_uri is no address Away can use: {why}")))?;
        // a service that is reached over a secured connection sends Away nowhere that is not
        if asked.secured && !at.secured {
            return Err(unusable("its jwks_uri is not secured".to_string()));
        }
        if let Ok(mut known) = self.known.lock() {
            known.keys_at = Some(at.clone());
        }
        Ok(at)
    }

    /// The keys the service publishes: as they were read, or read anew where a token
    /// was signed with one that is not among them.
    async fn keys(&self, anew: bool, now: u64) -> Result<Keys, String> {
        if let Ok(known) = self.known.lock() {
            let recently = known.read_anew_at.is_some_and(|at| now.saturating_sub(at) < KEYS_ANEW_AT_MOST_EVERY);
            if !known.keys.is_empty() && (!anew || recently) {
                return Ok(known.keys.clone());
            }
        }
        let keys = Keys::read(&self.get(&self.keys_at().await?).await?);
        if keys.is_empty() {
            return Err("The sign-in service publishes no key that Away can check a signature with.".to_string());
        }
        if let Ok(mut known) = self.known.lock() {
            known.keys = keys.clone();
            if anew {
                known.read_anew_at = Some(now);
            }
        }
        Ok(keys)
    }

    /// Checks what the service is said to have signed about a person: that it signed
    /// it, for Away, and that it has not run out. Gives what it says.
    pub async fn check(&self, signed: &str, now: u64) -> Result<Json, String> {
        let expected = Expected { service: &self.service, client: &self.away, number: None, now };
        let wrong = |wrong: Wrong| format!("What the sign-in service is said to have signed can't be taken: {wrong}.");
        match token::check(signed, &self.keys(false, now).await?, &expected) {
            // the service may have changed its keys: read them once more
            Err(Wrong::UnknownKey) => token::check(signed, &self.keys(true, now).await?, &expected).map_err(wrong),
            other => other.map_err(wrong),
        }
    }
}
