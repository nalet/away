//! The server: the pages, the requests, people's sign-ins, the rules, the records.
//!
//! In this version it signs people in through the organisation's sign-in service and
//! holds their sign-ins, reads the machines and the rules from the operator's files and
//! tells each person the machines a rule gives them, serves the pages, and says whether
//! it is well. It reaches no machine.

pub mod getting_in;
pub mod pages;
pub mod register;
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

/// The page at `/` of a program that was built without its pages.
const NO_PAGES: &str = "<!doctype html>\n\
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
<p>Away is installed and running. This build holds no pages: they are built in web/, before the program is.</p>\n\
</main>\n\
</body>\n\
</html>\n";

/// Where the server listens when nothing says otherwise.
pub const ADDRESS: &str = "0.0.0.0:8080";

/// How often the server looks whether the files of machines and rules changed.
const LOOKS_AT_THE_FILES_EVERY: std::time::Duration = std::time::Duration::from_secs(5);

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
        // the files are looked at as time passes, and applied where they changed
        let register = self.getting_in.register();
        let looking = tokio::spawn(async move {
            loop {
                tokio::time::sleep(LOOKS_AT_THE_FILES_EVERY).await;
                let register = register.clone();
                let _ = tokio::task::spawn_blocking(move || register.look(away_part::time::now().as_secs())).await;
            }
        });
        self.part.run(self.health, vec![(self.public, public(self.getting_in))]).await;
        renewing.abort();
        looking.abort();
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
            let address = request.uri().path();
            // an address that is none of the server's own is one of the pages' files, or the
            // page. What a program asks is never answered with a page: a check of whether the
            // part is well that reached this address by mistake would take one for a yes
            let for_programs = address.starts_with("/api/") || matches!(address, "/livez" | "/readyz" | "/metrics");
            let page = if for_programs { None } else { pages::at(address) };
            for_a_browser(match (page, address) {
                _ if !reading => plain(StatusCode::METHOD_NOT_ALLOWED, "This address only answers GET.\n"),
                (Some(page), _) => page,
                (None, "/") if !pages::are_built() => answer(StatusCode::OK, "text/html; charset=utf-8", NO_PAGES),
                (None, _) => plain(StatusCode::NOT_FOUND, "Nothing is at this address.\n"),
            })
        })
    })
}
