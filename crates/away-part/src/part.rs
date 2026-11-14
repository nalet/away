//! A part of Away as the cluster sees it: it says whether it is alive and whether it is
//! ready, gives its measures, writes its lines, and stops when it is told to.
//!
//! Every part answers three addresses on a listener of its own, which the way in never
//! leads to:
//!
//! | Address | Answers |
//! |---|---|
//! | `/livez` | 200 for as long as the part runs |
//! | `/readyz` | 200 once the part answers what it is there for, 503 before that and while it stops |
//! | `/metrics` | the part's measures |

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use hyper::{Method, StatusCode};

use crate::http::{answer, at_once, plain, Answers, Listener};
use crate::lines::Lines;
use crate::measures::Measures;
use crate::stop::Stop;
use crate::{time, version};

/// What a part is told when it starts.
#[derive(Clone, Debug)]
pub struct Settings {
    /// Where it answers what it is there for, such as `0.0.0.0:8080`. The issuer has none.
    pub address: String,
    /// Where it says whether it is well and gives its measures.
    pub health_address: String,
}

impl Settings {
    /// The settings a part has when nothing says otherwise.
    pub fn with_address(address: &str) -> Self {
        Settings { address: address.to_string(), health_address: "0.0.0.0:9090".to_string() }
    }

    /// The same, with what the environment says in place of what it names: `AWAY_ADDRESS`
    /// and `AWAY_HEALTH_ADDRESS`.
    pub fn from_environment(self, variable: impl Fn(&str) -> Option<String>) -> Self {
        Settings {
            address: variable("AWAY_ADDRESS").unwrap_or(self.address),
            health_address: variable("AWAY_HEALTH_ADDRESS").unwrap_or(self.health_address),
        }
    }
}

/// What a part needs of this library: its name, its lines, its measures, when to stop,
/// and whether it is ready.
#[derive(Clone)]
pub struct Part {
    /// Its name: `server`, `gateway` or `issuer`.
    pub name: &'static str,
    /// Where its lines go.
    pub lines: Lines,
    /// Its measures.
    pub measures: Measures,
    /// Learns when the part has to stop.
    pub stop: Stop,
    ready: Arc<AtomicBool>,
}

impl Part {
    /// A part that is not ready yet.
    pub fn new(name: &'static str, lines: Lines, stop: Stop) -> Self {
        let measures = Measures::of(name);
        measures
            .gauge(
                "away_build_info",
                "The version that runs: always 1, with the version and the commit as labels.",
                &[("version", version::VERSION), ("commit", version::COMMIT)],
            )
            .set(1);
        measures.gauge("away_start_time_seconds", "When the part started, in seconds since 1970.", &[]).set(time::now().as_secs());
        measures.gauge("away_ready", "Whether the part answers what it is there for: 1 or 0.", &[]).set(0);
        Part { name, lines, measures, stop, ready: Arc::new(AtomicBool::new(false)) }
    }

    /// Says whether the part answers what it is there for.
    pub fn set_ready(&self, ready: bool) {
        self.ready.store(ready, Ordering::Relaxed);
        self.measures.gauge("away_ready", "Whether the part answers what it is there for: 1 or 0.", &[]).set(u64::from(ready));
    }

    /// Whether the part answers what it is there for.
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Relaxed)
    }

    /// Binds a listener, and writes a line that says where, or why it couldn't.
    pub async fn listen(&self, name: &'static str, address: &str) -> std::io::Result<Listener> {
        match Listener::bind(name, address).await {
            Ok(listener) => {
                self.lines.info("listening", &[("listener", name.into()), ("on", (&listener.address().to_string()).into())]);
                Ok(listener)
            }
            Err(error) => {
                self.lines
                    .error("could not listen", &[("listener", name.into()), ("on", address.into()), ("why", (&error.to_string()).into())]);
                Err(error)
            }
        }
    }

    /// Answers a listener's requests until the part stops.
    pub async fn answer(&self, listener: Listener, answers: Answers) {
        listener.answer_with(answers, self.stop.clone(), self.lines.clone(), self.measures.clone()).await;
    }

    /// What answers on the listener that says whether the part is well.
    pub fn health(&self) -> Answers {
        let part = self.clone();
        at_once(move |request| {
            if request.method() != Method::GET {
                return plain(StatusCode::METHOD_NOT_ALLOWED, "This address only answers GET.\n");
            }
            match request.uri().path() {
                "/livez" => plain(StatusCode::OK, "alive\n"),
                "/readyz" if part.is_ready() && !part.stop.stopping() => plain(StatusCode::OK, "ready\n"),
                "/readyz" => plain(StatusCode::SERVICE_UNAVAILABLE, "not ready\n"),
                "/metrics" => answer(StatusCode::OK, "text/plain; version=0.0.4; charset=utf-8", part.measures.text()),
                _ => plain(StatusCode::NOT_FOUND, "Nothing is at this address.\n"),
            }
        })
    }

    /// Runs the part's listeners until it is told to stop: it is ready while they answer,
    /// and not ready from the moment it is told.
    pub async fn run(&self, health: Listener, others: Vec<(Listener, Answers)>) {
        self.set_ready(true);
        let mut stop = self.stop.clone();
        let stopping = {
            let part = self.clone();
            async move {
                stop.wait().await;
                part.set_ready(false);
            }
        };
        let answering = async {
            let mut all = tokio::task::JoinSet::new();
            let part = self.clone();
            let answers = self.health();
            all.spawn(async move { part.answer(health, answers).await });
            for (listener, answers) in others {
                let part = self.clone();
                all.spawn(async move { part.answer(listener, answers).await });
            }
            while all.join_next().await.is_some() {}
        };
        tokio::join!(stopping, answering);
        self.lines.info("stopped", &[]);
    }
}

/// The addresses a part is bound to, for whoever started it to find it by.
#[derive(Clone, Copy, Debug)]
pub struct Bound {
    /// Where it answers what it is there for, if it has such an address.
    pub address: Option<SocketAddr>,
    /// Where it says whether it is well.
    pub health_address: SocketAddr,
}
