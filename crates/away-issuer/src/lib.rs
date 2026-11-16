//! The issuer: makes the sign-ins on machines.
//!
//! It runs apart from the server and the gateway, as it always will, and nothing
//! reaches it from outside the cluster. Where an installation names a Windows domain,
//! the gateway asks it for what signs a person in on a machine of that domain as
//! themselves: a certificate that lasts minutes, for a key the gateway made and keeps
//! ([ADR-0007](../../../spec/architecture/decisions/0007-short-lived-certificates.md)).
//!
//! Before it signs anybody in, the issuer checks for itself that the session runs, whose
//! it is, and that the sign-in service signed who that person is a moment ago. An
//! installation that names no domain has an issuer that signs nobody in.

pub mod administrators;
pub mod authority;
pub mod server;
pub mod sign_in;
pub mod told;
pub mod windows;

use std::sync::Arc;

use away_part::address::read_form;
use away_part::base64;
use away_part::client::Client;
use away_part::http::{answer, carried, plain, Answer, Answers, Listener};
use away_part::json::Json;
use away_part::part::Bound;
use away_part::{time, Lines, Part, Settings};
use hyper::body::Incoming;
use hyper::{Method, Request, StatusCode};

use server::{NotVouched, Server};
use sign_in::Service;
pub use told::Told;
use windows::{Refused, Windows};

/// How long ago at most the sign-in service may have signed who a person is, for the
/// issuer to sign them in on it, in seconds.
const SIGNED_AT_MOST_AGO: u64 = 120;

/// What signs people in on the machines of a domain, and what that rests on.
struct Domain {
    administrators: Option<(administrators::Policy, Windows)>,
    windows: Windows,
    service: Service,
    server: Server,
}

struct Inside {
    domain: Option<Domain>,
    lines: Lines,
}

/// The issuer, bound and not yet answering.
pub struct Issuer {
    part: Part,
    health: Listener,
    internal: Option<Listener>,
    inside: Arc<Inside>,
}

fn says(status: StatusCode, why: &str, says: &str) -> Answer {
    answer(status, "application/json", Json::object([("why", why.into()), ("says", says.into())]).write())
}

impl Issuer {
    /// Binds the issuer's addresses, and takes what the installation tells it.
    pub async fn bind(settings: &Settings, told: Told, part: Part) -> std::io::Result<Issuer> {
        let could_not = |why: String| {
            part.lines.error("could not start", &[("why", (&why).into())]);
            std::io::Error::other(why)
        };
        let domain = match (told.windows, told.sign_in, told.server) {
            (Some(windows), Some(sign_in), Some(server)) => {
                let named = windows.domain.clone();
                let service = Client::new(sign_in.authorities.as_deref())
                    .map_err(|why| could_not(format!("The authorities that secure the sign-in service can't be used: {why}.")))?;
                let asks = Client::new(server.authorities.as_deref())
                    .map_err(|why| could_not(format!("The authority that secures the server's address can't be used: {why}.")))?;
                let administrators = told
                    .administrators
                    .map(|admin| Windows::new(admin.windows).map(|windows| (admin.policy, windows)))
                    .transpose()
                    .map_err(could_not)?;
                let domain = Domain {
                    administrators,
                    windows: Windows::new(windows).map_err(could_not)?,
                    service: Service::new(service, &sign_in.service, &sign_in.client),
                    server: Server::new(asks, server.at),
                };
                part.lines.info("people are signed in as themselves on the machines of a domain", &[("domain", (&named).into())]);
                Some(domain)
            }
            _ => {
                part.lines.info("no domain is named, so nobody is signed in on a machine as themselves", &[]);
                None
            }
        };
        let health = part.listen("health", &settings.health_address).await?;
        let internal = match told.internal {
            None => None,
            Some(internal) => {
                let listener = part.listen("internal", &internal.address).await?;
                Some(match internal.certificate {
                    Some((certificate, key)) => listener
                        .secured_with(&certificate, &key)
                        .map_err(|why| could_not(format!("The certificate for what the gateway asks the issuer can't be used: {why}.")))?,
                    None => {
                        part.lines.warn("what the gateway asks the issuer is not secured: that is for the two on one machine", &[]);
                        listener
                    }
                })
            }
        };
        let inside = Arc::new(Inside { domain, lines: part.lines.clone() });
        Ok(Issuer { part, health, internal, inside })
    }

    /// The addresses it is bound to.
    pub fn bound(&self) -> Bound {
        Bound { address: self.internal.as_ref().map(Listener::address), health_address: self.health.address() }
    }

    /// Answers until the part is told to stop.
    pub async fn run(self) {
        let mut others = Vec::new();
        if let Some(internal) = self.internal {
            let inside = self.inside.clone();
            let answers: Answers = Arc::new(move |request| {
                let inside = inside.clone();
                Box::pin(async move { inside.answer(request).await })
            });
            others.push((internal, answers));
        }
        self.part.run(self.health, others).await;
    }
}

impl Inside {
    /// Answers the gateway, on the address nothing outside the cluster reaches.
    async fn answer(&self, request: Request<Incoming>) -> Answer {
        if request.method() != Method::POST {
            return plain(StatusCode::METHOD_NOT_ALLOWED, "This address only answers POST.\n");
        }
        let path = request.uri().path().to_string();
        let said = match carried(request, 16 * 1024).await.and_then(|sent| read_form(std::str::from_utf8(&sent).ok()?)) {
            Some(said) => said,
            None => return plain(StatusCode::BAD_REQUEST, "What is said here is said as a form.\n"),
        };
        let said = |name: &str| said.iter().find(|(its, _)| its == name).map(|(_, value)| value.as_str());
        match path.as_str() {
            "/internal/certificate" => {
                let asked = said("request")
                    .and_then(|letters| base64::read_for_addresses(&letters.replace('+', "-").replace('/', "_").replace('=', "")));
                let (Some(session), Some(asked)) = (said("session").filter(|session| !session.is_empty()), asked) else {
                    return plain(
                        StatusCode::BAD_REQUEST,
                        "A certificate is asked for with the session, as session=, and the request for it, as request=.\n",
                    );
                };
                self.certificate(session, &asked).await
            }
            _ => plain(StatusCode::NOT_FOUND, "Nothing is at this address.\n"),
        }
    }

    /// What signs the person of a session in on its machine: checked, made, recorded,
    /// and then handed out.
    async fn certificate(&self, session: &str, asked: &[u8]) -> Answer {
        let Some(domain) = &self.domain else {
            return says(
                StatusCode::NOT_FOUND,
                "no-domain",
                "This installation signs nobody in on a machine as themselves: it names no domain.",
            );
        };
        let refuse = |status: StatusCode, refused: Refused| {
            self.lines.warn(
                "signed nobody in",
                &[("session", session.into()), ("why", refused.why.into()), ("says", refused.says.as_str().into())],
            );
            says(status, refused.why, &refused.says)
        };
        let because = |why: &'static str, says: String| Refused { why, says };
        // the session runs, the server says whose it is, and hands over what the sign-in
        // service signed about that person a moment ago
        let vouched = match domain.server.vouch(session).await {
            Ok(vouched) => vouched,
            Err(NotVouched::NoSession) => {
                return refuse(StatusCode::NOT_FOUND, because("no-session", "No session of that name runs.".to_string()));
            }
            Err(NotVouched::SignedOut(said)) => return refuse(StatusCode::FORBIDDEN, because("signed-out", said)),
            Err(NotVouched::NotAsked(why)) => return refuse(StatusCode::BAD_GATEWAY, because("server", why)),
        };
        // which the issuer checks for itself: the service signed it, about this person, just now
        let now = time::now().as_secs();
        let about = match domain.service.check(vouched.signed.expose(), now).await {
            Ok(about) => about,
            Err(why) => return refuse(StatusCode::FORBIDDEN, because("sign-in", why)),
        };
        if about.text_of("sub") != Some(vouched.subject.as_str()) {
            let why = "What the sign-in service signed is about another person than the one the session is of.";
            return refuse(StatusCode::FORBIDDEN, because("sign-in", why.to_string()));
        }
        if !about.get("iat").and_then(Json::count).is_some_and(|signed_at| signed_at + SIGNED_AT_MOST_AGO >= now) {
            let why = "The sign-in service signed who this person is too long ago, or doesn't say when.";
            return refuse(StatusCode::FORBIDDEN, because("sign-in", why.to_string()));
        }
        if vouched.signs_in != "themselves" {
            return refuse(StatusCode::FORBIDDEN, because("policy", "This session is not authorized to request a certificate.".into()));
        }
        let windows = match &domain.administrators {
            Some((policy, windows)) if policy.contains(&vouched.subject) => {
                if let Err(why) = policy.authorize(&about, &vouched.targets, windows) {
                    return refuse(StatusCode::FORBIDDEN, why);
                }
                windows
            }
            _ => &domain.windows,
        };
        let made = match windows.sign_in(&about, asked, now).await {
            Ok(made) => made,
            Err(refused) => {
                let status = match refused.why {
                    "request" => StatusCode::BAD_REQUEST,
                    "authority" => StatusCode::BAD_GATEWAY,
                    _ => StatusCode::FORBIDDEN,
                };
                return refuse(status, refused);
            }
        };
        // the record comes before the certificate is handed out, and without it none is
        if let Err(why) = domain.server.made(session, &made.account, &made.number, made.until).await {
            return refuse(StatusCode::SERVICE_UNAVAILABLE, because("not-recorded", why));
        }
        self.lines.info(
            "signed a person in",
            &[
                ("who", vouched.subject.as_str().into()),
                ("as", made.account.as_str().into()),
                ("machine", vouched.machine.as_str().into()),
                ("session", session.into()),
                ("number", made.number.as_str().into()),
                ("until", made.until.into()),
            ],
        );
        let says = Json::object([
            ("certificate", base64::ordinary(&made.certificate).into()),
            ("account", made.account.as_str().into()),
            ("until", made.until.into()),
        ]);
        answer(StatusCode::OK, "application/json", says.write())
    }
}
