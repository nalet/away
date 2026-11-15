//! The server: the pages, the requests, people's sign-ins, the rules, the records.
//!
//! In this version it signs people in through the organisation's sign-in service and
//! holds their sign-ins, reads the machines and the rules from the operator's files and
//! tells each person the machines a rule gives them, serves the pages, and says whether
//! it is well. It reaches no machine.

pub mod farm;
pub mod getting_in;
pub mod pages;
pub mod records;
pub mod register;
pub mod service;
pub mod sessions;
pub mod sign_ins;
pub mod store;
pub mod told;

#[cfg(any(test, feature = "stand-in"))]
pub mod stand_in;

use std::sync::Arc;

use away_part::http::{answer, for_a_browser, plain, Answers, Listener};
use away_part::part::Bound;
use away_part::{Part, Settings};
use hyper::{Method, StatusCode};

use crate::farm::reader::Reader;
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

/// How often what changed is looked for, to be kept as records.
const RECORDS_ARE_WRITTEN_EVERY: std::time::Duration = std::time::Duration::from_millis(500);

/// How long reading one farm's feed may take.
const A_FARM_IS_READ_WITHIN: std::time::Duration = std::time::Duration::from_secs(120);

/// What a farm's state says where Away is given no account to read feeds with.
const NO_ACCOUNT: &str = "Away is given no account to read a farm's feed with: the installation guide says how to give it one.";

/// The server, bound and not yet answering.
pub struct Server {
    part: Part,
    public: Listener,
    health: Listener,
    getting_in: Arc<GettingIn>,
    /// Where the gateway asks the server, where the installation names such an address.
    internal: Option<Listener>,
    /// What reads the feeds of farms, and how often, where Away is given an account for it.
    farms: Option<(Arc<Reader>, std::time::Duration)>,
}

impl Server {
    /// Binds the server's addresses, and takes what the installation tells it.
    pub async fn bind(settings: &Settings, mut told: Told, part: Part) -> std::io::Result<Server> {
        let farms = match told.farms.take() {
            None => None,
            Some(farms) => match away_part::client::Client::new(farms.authorities.as_deref()) {
                Ok(client) => Some((Arc::new(Reader::new(client, &farms.account, farms.password)), farms.every)),
                Err(why) => {
                    let why = format!("The authorities to trust for the farms' feeds can't be used: {why}.");
                    part.lines.error("could not start", &[("why", (&why).into())]);
                    return Err(std::io::Error::other(why));
                }
            },
        };
        let internal = told.internal.take();
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
        let internal = match internal {
            None => None,
            Some(internal) => {
                let listener = part.listen("internal", &internal.address).await?;
                Some(match internal.certificate {
                    Some((certificate, key)) => listener.secured_with(&certificate, &key).map_err(|why| {
                        let why = format!("The certificate for what the gateway asks the server can't be used: {why}.");
                        part.lines.error("could not start", &[("why", (&why).into())]);
                        std::io::Error::other(why)
                    })?,
                    None => {
                        part.lines.warn("what the gateway asks the server is not secured: that is for the two on one machine", &[]);
                        listener
                    }
                })
            }
        };
        Ok(Server { part, public, health, getting_in, internal, farms })
    }

    /// The addresses it is bound to.
    pub fn bound(&self) -> Bound {
        Bound { address: Some(self.public.address()), health_address: self.health.address() }
    }

    /// The address the gateway asks it at, where it has one.
    pub fn internal_address(&self) -> Option<std::net::SocketAddr> {
        self.internal.as_ref().map(Listener::address)
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
        // the database is reached as the server starts, and tried again until it is
        let store = self.getting_in.store();
        let lines = self.part.lines.clone();
        let reaching = tokio::spawn(async move {
            let Some(store) = store else { return };
            let mut said: Option<String> = None;
            loop {
                match store.begin().await {
                    Ok(()) => {
                        lines.info("the database is reached, and in shape", &[]);
                        return;
                    }
                    Err(unusable) => {
                        let why = unusable.to_string();
                        if said.as_deref() != Some(why.as_str()) {
                            lines.warn("the database can't be used, so nothing Away keeps is at hand", &[("why", why.as_str().into())]);
                            said = Some(why);
                        }
                    }
                }
                tokio::time::sleep(LOOKS_AT_THE_FILES_EVERY).await;
            }
        });
        // what changes in what is in force is kept as a record, in the order it happened;
        // while the database gives nothing the changes wait, and nothing of them is lost
        let register = self.getting_in.register();
        let records = records::Records::new(self.getting_in.store(), self.part.lines.clone());
        let lines = self.part.lines.clone();
        let recording = tokio::spawn(async move {
            let mut waiting: std::collections::VecDeque<(String, register::files::Change)> = std::collections::VecDeque::new();
            let mut said = false;
            loop {
                for change in register.take_changes() {
                    if !records.are_kept() {
                        continue;
                    }
                    match records::Records::key() {
                        Ok(key) => waiting.push_back((key, change)),
                        Err(why) => lines.error("a change could not be kept as a record", &[("why", why.as_str().into())]),
                    }
                }
                while let Some((key, change)) = waiting.front() {
                    match records.changed(change, key).await {
                        Ok(_) => {
                            waiting.pop_front();
                            said = false;
                        }
                        Err(unusable) => {
                            if !said {
                                let why = unusable.to_string();
                                lines.warn("a change waits to be kept as a record", &[("why", why.as_str().into())]);
                                said = true;
                            }
                            break;
                        }
                    }
                }
                tokio::time::sleep(RECORDS_ARE_WRITTEN_EVERY).await;
            }
        });
        // the farms' feeds are read as time passes, each when it is due
        let register = self.getting_in.register();
        let farms = self.farms.clone();
        let reading = tokio::spawn(async move {
            let every = farms.as_ref().map_or(std::time::Duration::from_secs(300), |(_, every)| *every);
            loop {
                let now = away_part::time::now().as_secs();
                for (farm, kept) in register.farms_to_read(now, every.as_secs().max(1)) {
                    let outcome = match &farms {
                        None => Err(NO_ACCOUNT.to_string()),
                        Some((reader, _)) => {
                            let mut random = [0u8; 24];
                            match getrandom::fill(&mut random) {
                                Err(_) => Err("The system gave no random bytes for the sign-in at the farm.".to_string()),
                                Ok(()) => {
                                    let fresh = away_rdp::ntlm::Fresh {
                                        challenge: random[..8].try_into().expect("8 bytes"),
                                        key: random[8..].try_into().expect("16 bytes"),
                                    };
                                    // the time now as Windows counts it: in ten-millionths of a second since 1601
                                    let clock = (now + 11_644_473_600) * 10_000_000;
                                    match tokio::time::timeout(A_FARM_IS_READ_WITHIN, reader.read(&farm, &kept, clock, fresh)).await {
                                        Ok(outcome) => outcome,
                                        Err(_) => Err("The farm's feed took longer to read than a feed may.".to_string()),
                                    }
                                }
                            }
                        }
                    };
                    register.farm_was_read(&farm, outcome, away_part::time::now().as_secs());
                }
                tokio::time::sleep(every.min(std::time::Duration::from_secs(5))).await;
            }
        });
        let mut others = Vec::new();
        if let Some(internal) = self.internal {
            let getting_in = self.getting_in.clone();
            let answers: Answers = Arc::new(move |request| {
                let getting_in = getting_in.clone();
                Box::pin(async move { getting_in.answer_internal(request).await })
            });
            others.push((internal, answers));
        }
        others.push((self.public, public(self.getting_in)));
        self.part.run(self.health, others).await;
        renewing.abort();
        looking.abort();
        reading.abort();
        reaching.abort();
        recording.abort();
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
