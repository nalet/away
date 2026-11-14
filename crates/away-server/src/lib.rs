//! The server: the pages, the requests, people's sign-ins, the rules, the records.
//!
//! In this version it answers on the address Away is reached at, with one page that says
//! Away is installed, and says whether it is well. It reaches nothing yet.

use std::sync::Arc;

use away_part::http::{answer, for_a_browser, plain, Answers, Listener};
use away_part::part::Bound;
use away_part::{Part, Settings};
use hyper::{Method, StatusCode};

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
}

impl Server {
    /// Binds the server's addresses.
    pub async fn bind(settings: &Settings, part: Part) -> std::io::Result<Server> {
        let public = part.listen("public", &settings.address).await?;
        let health = part.listen("health", &settings.health_address).await?;
        Ok(Server { part, public, health })
    }

    /// The addresses it is bound to.
    pub fn bound(&self) -> Bound {
        Bound { address: Some(self.public.address()), health_address: self.health.address() }
    }

    /// Answers until the part is told to stop.
    pub async fn run(self) {
        self.part.run(self.health, vec![(self.public, public())]).await;
    }
}

/// What answers on the address Away is reached at.
fn public() -> Answers {
    Arc::new(|request| {
        let reading = matches!(*request.method(), Method::GET | Method::HEAD);
        for_a_browser(match request.uri().path() {
            "/" if reading => answer(StatusCode::OK, "text/html; charset=utf-8", INSTALLED),
            "/" => plain(StatusCode::METHOD_NOT_ALLOWED, "This address only answers GET.\n"),
            _ => plain(StatusCode::NOT_FOUND, "Nothing is at this address.\n"),
        })
    })
}
