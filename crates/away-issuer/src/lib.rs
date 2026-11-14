//! The issuer: makes the sign-ins on machines.
//!
//! In this version it signs nothing. It runs apart from the server and the gateway, as it
//! always will, and says whether it is well. Nothing reaches it from outside the cluster.

use away_part::http::Listener;
use away_part::part::Bound;
use away_part::{Part, Settings};

/// The issuer, bound and not yet answering.
pub struct Issuer {
    part: Part,
    health: Listener,
}

impl Issuer {
    /// Binds the issuer's address.
    pub async fn bind(settings: &Settings, part: Part) -> std::io::Result<Issuer> {
        let health = part.listen("health", &settings.health_address).await?;
        Ok(Issuer { part, health })
    }

    /// The address it is bound to.
    pub fn bound(&self) -> Bound {
        Bound { address: None, health_address: self.health.address() }
    }

    /// Answers until the part is told to stop.
    pub async fn run(self) {
        self.part.run(self.health, Vec::new()).await;
    }
}
