//! The server: the pages, the requests, people's sign-ins, the rules, the records.
//!
//! In this version it signs people in through the organisation's sign-in service and
//! holds their sign-ins, answers on the address Away is reached at with one page that
//! says Away is installed, and says whether it is well. It reaches no machine.

pub mod getting_in;
pub mod service;
pub mod sign_ins;
pub mod token;
pub mod told;

#[cfg(any(test, feature = "stand-in"))]
pub mod stand_in;

use std::sync::Arc;

use away_part::http::{answer, for_a_browser, plain, Answers, Listener};
use away_part::part::Bound;
use away_part::{Part, Settings};
use hyper::{Method, StatusCode};

use crate::getting_in::GettingIn;
use crate::told::Told;

/// The page at `/`, until Away has pages of its own.
const INSTALLED: &str = "<!doctype html>\n\
<html lang=\"en\">\n\
<head>\n\
<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
<meta name=\"color-scheme\" content=\"dark light\">\n\
<title>away</title>\n\
</head>\n\
<body>\n\
<main>\n\
<h1>away</h1>\n\
<p>Away is installed and running. This version has no pages yet, so there is nothing to open here.</p>\n\
</main>\n\
</body>\n\
</html>\n";

/// Where the server listens when nothing says otherwise.
pub const ADDRESS: &str = "0.0.0.0:8080";

/// The server, bound and not yet answering.
pub struct Server {
    part: Part,
    public: Listener,
    health: Listener,
    getting_in: Arc<GettingIn>,
}

impl Server {
    /// Binds the server's addresses, and takes what the installation tells it.
    pub async fn bind(settings: &Settings, told: Told, part: Part) -> std::io::Result<Server> {
        let getting_in = match GettingIn::new(told, part.clone()) {
            Ok(getting_in) => Arc::new(getting_in),
            Err(why) => {
                part.lines.error("could not start", &[("why", (&why).into())]);
                return Err(std::io::Error::other(why));
            }
        };
        if !getting_in.is_set_up() {
            part.lines.warn("no sign-in service is set, so nobody can sign in", &[]);
        }
        let public = part.listen("public", &settings.address).await?;
        let health = part.listen("health", &settings.health_address).await?;
        Ok(Server { part, public, health, getting_in })
    }

    /// The addresses it is bound to.
    pub fn bound(&self) -> Bound {
        Bound { address: Some(self.public.address()), health_address: self.health.address() }
    }

    /// Answers until the part is told to stop.
    pub async fn run(self) {
        let renewing = tokio::spawn(self.getting_in.clone().keep_renewing());
        self.part.run(self.health, vec![(self.public, public(self.getting_in))]).await;
        renewing.abort();
    }
}

/// What answers on the address Away is reached at.
fn public(getting_in: Arc<GettingIn>) -> Answers {
    Arc::new(move |request| {
        let getting_in = getting_in.clone();
        Box::pin(async move {
            if let Some(answer) = getting_in.answer(&request).await {
                return for_a_browser(answer);
            }
            let reading = matches!(*request.method(), Method::GET | Method::HEAD);
            for_a_browser(match request.uri().path() {
                "/" if reading => answer(StatusCode::OK, "text/html; charset=utf-8", INSTALLED),
                "/" => plain(StatusCode::METHOD_NOT_ALLOWED, "This address only answers GET.\n"),
                _ => plain(StatusCode::NOT_FOUND, "Nothing is at this address.\n"),
            })
        })
    })
}
