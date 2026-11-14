//! The gateway: the sessions, each with one connection to the browser and one to the
//! machine.
//!
//! In this version it holds no session. It listens where a session's connection will
//! arrive, answers that nothing is there, and says whether it is well.

use away_part::http::{at_once, for_a_browser, plain, Answers, Listener};
use away_part::part::Bound;
use away_part::{Part, Settings};
use hyper::StatusCode;

/// Where the gateway listens when nothing says otherwise.
pub const ADDRESS: &str = "0.0.0.0:8081";

/// The gateway, bound and not yet answering.
pub struct Gateway {
    part: Part,
    sessions: Listener,
    health: Listener,
}

impl Gateway {
    /// Binds the gateway's addresses.
    pub async fn bind(settings: &Settings, part: Part) -> std::io::Result<Gateway> {
        let sessions = part.listen("sessions", &settings.address).await?;
        let health = part.listen("health", &settings.health_address).await?;
        Ok(Gateway { part, sessions, health })
    }

    /// The addresses it is bound to.
    pub fn bound(&self) -> Bound {
        Bound { address: Some(self.sessions.address()), health_address: self.health.address() }
    }

    /// Answers until the part is told to stop.
    pub async fn run(self) {
        self.part.run(self.health, vec![(self.sessions, sessions())]).await;
    }
}

/// What answers where a session's connection arrives.
fn sessions() -> Answers {
    at_once(|_| for_a_browser(plain(StatusCode::NOT_FOUND, "No session is at this address.\n")))
}
