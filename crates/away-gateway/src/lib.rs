//! The gateway: the sessions, each with one connection to the browser and one to the
//! machine.
//!
//! A session's connection arrives at `/session`, as a request that is handed over to
//! become a WebSocket. Its first message is the ticket the server gave for it; the
//! gateway takes the session over at the server with that ticket, opens the machine,
//! and tells the server what became of it.
//!
//! | Variable | Says |
//! |---|---|
//! | `AWAY_SERVER` | where the gateway asks the server: `https://away-server:8082`. Without it no session opens |
//! | `AWAY_SERVER_AUTHORITIES_FILE` | the certificate of the authority that secures that address, as PEM |

pub mod machine;
pub mod restricted_admin;
pub mod session;
mod shown;
#[cfg(feature = "stand-in")]
pub mod stand_in;
pub mod themselves;
pub mod vnc;
pub mod windows;

use std::sync::Arc;

use away_part::address::Address;
use away_part::client::Client;
use away_part::http::{for_a_browser, plain, Answer, Answers, Listener};
use away_part::part::Bound;
use away_part::{Part, Settings};
use hyper::body::Incoming;
use hyper::header::{HeaderValue, CONNECTION, COOKIE, HOST, ORIGIN, UPGRADE};
use hyper::{Method, Request, StatusCode};
use hyper_util::rt::TokioIo;
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;
use tokio_tungstenite::tungstenite::protocol::{Role, WebSocketConfig};
use tokio_tungstenite::WebSocketStream;

use session::{Around, Server};

/// Where the gateway listens when nothing says otherwise.
pub const ADDRESS: &str = "0.0.0.0:8081";

/// The cookie that names a browser's sign-in, which the server gave it.
const SIGNED_IN: &str = "__Host-away";

/// The longest message a browser may send: a name and a password, what a person did
/// with the keys and the pointer in a moment, or a piece of a picture they copied.
const LONGEST_FROM_A_BROWSER: usize = 64 * 1024;

/// What an installation tells the gateway.
#[derive(Clone, Debug, Default)]
pub struct Told {
    /// Where the gateway asks the server. Without it no session opens.
    pub server: Option<Address>,
    /// The certificate of the authority that secures that address, as PEM.
    pub authorities: Option<Vec<u8>>,
    /// The domain whose machines people are signed in on as themselves, where the
    /// installation names one.
    pub domain: Option<InADomain>,
    /// Explicitly enabled, scoped browser Restricted Admin policy.
    pub restricted_admin: Option<restricted_admin::RestrictedAdmin>,
}

/// What an installation tells the gateway of a Windows domain.
#[derive(Clone, Debug)]
pub struct InADomain {
    /// The domain, as it is named in full: `corp.example`.
    pub name: String,
    /// The domain controllers that give tickets, each with its port where that is not
    /// Kerberos' own.
    pub controllers: Vec<String>,
    /// The authorities a domain controller's certificate has to lead to, as PEM.
    pub authorities: Vec<u8>,
    /// Where the gateway asks the issuer.
    pub issuer: Address,
    /// The authority that secures that address, as PEM.
    pub issuer_authorities: Option<Vec<u8>>,
}

impl Told {
    /// Reads what the environment says. A file is read with the function given.
    pub fn read(variable: impl Fn(&str) -> Option<String>, file: impl Fn(&str) -> std::io::Result<Vec<u8>>) -> Result<Told, String> {
        let server = match variable("AWAY_SERVER") {
            None => None,
            Some(written) => Some(Address::read(written.trim()).map_err(|why| format!("AWAY_SERVER is no address: {why}."))?),
        };
        let authorities = variable("AWAY_SERVER_AUTHORITIES_FILE")
            .map(|path| file(&path).map_err(|error| format!("AWAY_SERVER_AUTHORITIES_FILE names {path}, which can't be read: {error}.")))
            .transpose()?;
        let said = |name: &str| variable(name).map(|said| said.trim().to_string()).filter(|said| !said.is_empty());
        let read = |name: &str| -> Result<Option<Vec<u8>>, String> {
            said(name).map(|path| file(&path).map_err(|error| format!("{name} names {path}, which can't be read: {error}."))).transpose()
        };
        let domain = match said("AWAY_DOMAIN") {
            None => None,
            Some(name) => {
                let needed = |what: &str| {
                    said(what).ok_or_else(|| format!("{what} is not set, and signing people in on the machines of {name} needs it."))
                };
                let controllers = needed("AWAY_DOMAIN_CONTROLLERS")?
                    .split(',')
                    .map(str::trim)
                    .filter(|one| !one.is_empty())
                    .map(str::to_string)
                    .collect();
                needed("AWAY_DOMAIN_AUTHORITIES_FILE")?;
                let authorities = read("AWAY_DOMAIN_AUTHORITIES_FILE")?.unwrap_or_default();
                let issuer = Address::read(&needed("AWAY_ISSUER")?).map_err(|why| format!("AWAY_ISSUER is no address: {why}."))?;
                Some(InADomain { name, controllers, authorities, issuer, issuer_authorities: read("AWAY_ISSUER_AUTHORITIES_FILE")? })
            }
        };
        let restricted_admin = restricted_admin::RestrictedAdmin::read(&variable, &file)?;
        Ok(Told { server, authorities, domain, restricted_admin })
    }
}

/// The gateway, bound and not yet answering.
pub struct Gateway {
    part: Part,
    sessions: Listener,
    health: Listener,
    around: Arc<Around>,
}

impl Gateway {
    /// Binds the gateway's addresses, and takes what the installation tells it.
    pub async fn bind(settings: &Settings, told: Told, part: Part) -> std::io::Result<Gateway> {
        let server = match told.server {
            None => {
                part.lines.warn("the gateway is told of no server, so no session opens", &[]);
                None
            }
            Some(at) => match Client::new(told.authorities.as_deref()) {
                Ok(client) => Some(Server::new(client, at)),
                Err(why) => {
                    let why = format!("The authority that secures the server's address can't be used: {why}.");
                    part.lines.error("could not start", &[("why", (&why).into())]);
                    return Err(std::io::Error::other(why));
                }
            },
        };
        let domain = match told.domain {
            None => None,
            Some(told) => {
                let could_not = |why: String| {
                    let why = format!("The domain {} can't be used to sign people in on its machines: {why}.", told.name);
                    part.lines.error("could not start", &[("why", (&why).into())]);
                    std::io::Error::other(why)
                };
                let asks = Client::new(told.issuer_authorities.as_deref()).map_err(could_not)?;
                let issuer = themselves::Issuer::new(asks, told.issuer.clone());
                let domain = themselves::Domain::new(&told.name, &told.controllers, &told.authorities, issuer).map_err(could_not)?;
                part.lines.info("people are signed in as themselves on the machines of a domain", &[("domain", (&told.name).into())]);
                Some(domain)
            }
        };
        let sessions = part.listen("sessions", &settings.address).await?;
        let health = part.listen("health", &settings.health_address).await?;
        let around = Arc::new(Around {
            server,
            domain,
            restricted_admin: told.restricted_admin,
            lines: part.lines.clone(),
            stop: part.stop.clone(),
            running: Arc::default(),
        });
        Ok(Gateway { part, sessions, health, around })
    }

    /// The addresses it is bound to.
    pub fn bound(&self) -> Bound {
        Bound { address: Some(self.sessions.address()), health_address: self.health.address() }
    }

    /// Answers until the part is told to stop. The sessions that run then are told that
    /// the gateway stops, and are given a moment to say so to their browsers, which
    /// open them again by themselves.
    pub async fn run(self) {
        let running = self.around.running.clone();
        self.part.run(self.health, vec![(self.sessions, sessions(self.around))]).await;
        let until = tokio::time::Instant::now() + SESSIONS_END_WITHIN;
        while running.load(std::sync::atomic::Ordering::SeqCst) > 0 && tokio::time::Instant::now() < until {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }
}

/// How long the sessions that run get to say that the gateway stops.
const SESSIONS_END_WITHIN: std::time::Duration = std::time::Duration::from_secs(3);

/// What answers where a session's connection arrives.
fn sessions(around: Arc<Around>) -> Answers {
    Arc::new(move |request| {
        let around = around.clone();
        Box::pin(async move { for_a_browser(begin(request, around)) })
    })
}

/// Takes a request that wants to become a session's connection, and hands it over.
fn begin(mut request: Request<Incoming>, around: Arc<Around>) -> Answer {
    if request.uri().path() != "/session" {
        return plain(StatusCode::NOT_FOUND, "No session is at this address.\n");
    }
    let header = |name| request.headers().get(name).and_then(|value: &HeaderValue| value.to_str().ok());
    let wants_a_socket = request.method() == Method::GET
        && header(UPGRADE).is_some_and(|upgrade| upgrade.eq_ignore_ascii_case("websocket"))
        && header(CONNECTION).is_some_and(|connection| connection.split(',').any(|part| part.trim().eq_ignore_ascii_case("upgrade")))
        && request.headers().get("sec-websocket-version").is_some_and(|version| version.as_bytes() == b"13");
    let Some(key) = request.headers().get("sec-websocket-key").filter(|_| wants_a_socket) else {
        return plain(StatusCode::BAD_REQUEST, "A session's connection is a WebSocket.\n");
    };
    // a page of another site may not open a session's connection: a browser says where a
    // request is from, and that has to be the address this one came to
    let from = header(ORIGIN).and_then(|origin| origin.split_once("://")).map(|(_, host)| host);
    if from.is_some_and(|from| Some(from) != header(HOST)) {
        return plain(StatusCode::FORBIDDEN, "Only Away's own pages open a session.\n");
    }
    let accept = derive_accept_key(key.as_bytes());
    let sign_in = request
        .headers()
        .get_all(COOKIE)
        .iter()
        .filter_map(|sent| sent.to_str().ok())
        .flat_map(|sent| sent.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, value)| *name == SIGNED_IN && !value.is_empty())
        .map(|(_, value)| value.to_string());

    let handed_over = hyper::upgrade::on(&mut request);
    tokio::spawn(async move {
        let Ok(connection) = handed_over.await else { return };
        let limits = WebSocketConfig::default().max_message_size(Some(LONGEST_FROM_A_BROWSER)).max_frame_size(Some(LONGEST_FROM_A_BROWSER));
        let browser = WebSocketStream::from_raw_socket(TokioIo::new(connection), Role::Server, Some(limits)).await;
        session::run(browser, sign_in, around).await;
    });
    let mut answer = plain(StatusCode::SWITCHING_PROTOCOLS, "");
    answer.headers_mut().remove(hyper::header::CONTENT_TYPE);
    answer.headers_mut().insert(UPGRADE, HeaderValue::from_static("websocket"));
    answer.headers_mut().insert(CONNECTION, HeaderValue::from_static("Upgrade"));
    if let Ok(accept) = HeaderValue::from_str(&accept) {
        answer.headers_mut().insert("sec-websocket-accept", accept);
    }
    answer
}
