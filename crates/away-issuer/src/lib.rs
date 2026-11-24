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
//!
//! It also keeps the secrets an installation keeps for machines that know no directory,
//! such as the password of a desktop over VNC: one is handed to the gateway for a
//! session whose machine names it, after the same checks, and never to anybody else.

pub mod administrators;
pub mod authority;
pub mod keeping;
pub mod server;
pub mod sign_in;
pub mod told;
pub mod windows;

use std::path::PathBuf;
use std::sync::Arc;

use away_part::address::read_form;
use away_part::base64;
use away_part::client::Client;
use away_part::http::{answer, carried, plain, Answer, Answers, Listener};
use away_part::json::Json;
use away_part::part::Bound;
use away_part::{time, Lines, Part, Settings};
use away_secret::Secret;
use hyper::body::Incoming;
use hyper::{Method, Request, StatusCode};

use server::{NotVouched, Server};
use sign_in::Service;
pub use told::Told;
use windows::{Refused, Windows};

/// How long ago at most the sign-in service may have signed who a person is, for the
/// issuer to sign them in on it, in seconds.
const SIGNED_AT_MOST_AGO: u64 = 120;

/// What signs people in on the machines of a domain.
struct Domain {
    administrators: Option<(administrators::Policy, Windows)>,
    windows: Windows,
}

/// What everything the issuer hands out rests on: the server says whose a session is,
/// and the sign-in service's signature says who that person is.
struct Checks {
    service: Service,
    server: Server,
}

struct Inside {
    checks: Option<Checks>,
    domain: Option<Domain>,
    /// The folder of the secrets kept for machines.
    kept: Option<PathBuf>,
    /// The keys of the secrets Away keeps in its own store.
    keeping: Option<keeping::Keeping>,
    lines: Lines,
}

/// The longest name a kept secret has.
const LONGEST_KEPT_NAME: usize = 63;

/// Whether a name is one a kept secret may have: a file of the folder, and nothing
/// that leads out of it.
fn is_a_kept_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= LONGEST_KEPT_NAME
        && !name.starts_with('.')
        && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
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
        let checks = match (told.sign_in, told.server) {
            (Some(sign_in), Some(server)) => {
                let service = Client::new(sign_in.authorities.as_deref())
                    .map_err(|why| could_not(format!("The authorities that secure the sign-in service can't be used: {why}.")))?;
                let asks = Client::new(server.authorities.as_deref())
                    .map_err(|why| could_not(format!("The authority that secures the server's address can't be used: {why}.")))?;
                Some(Checks { service: Service::new(service, &sign_in.service, &sign_in.client), server: Server::new(asks, server.at) })
            }
            _ => None,
        };
        let domain = match (told.windows, &checks) {
            (Some(windows), Some(_)) => {
                let named = windows.domain.clone();
                let administrators = told
                    .administrators
                    .map(|admin| Windows::new(admin.windows).map(|windows| (admin.policy, windows)))
                    .transpose()
                    .map_err(could_not)?;
                let domain = Domain { administrators, windows: Windows::new(windows).map_err(could_not)? };
                part.lines.info("people are signed in as themselves on the machines of a domain", &[("domain", (&named).into())]);
                Some(domain)
            }
            _ => {
                part.lines.info("no domain is named, so nobody is signed in on a machine as themselves", &[]);
                None
            }
        };
        let kept = match (told.kept, &checks) {
            (Some(folder), Some(_)) => {
                part.lines.info("secrets kept for machines are handed out", &[("folder", folder.as_str().into())]);
                Some(PathBuf::from(folder))
            }
            (Some(_), None) => {
                return Err(could_not(
                    "Secrets kept for machines are handed out only with the sign-in service and the server told.".to_string(),
                ))
            }
            (None, _) => None,
        };
        let keeping = match (told.keeping, &checks) {
            (Some(keys), Some(_)) => {
                let keeping = keeping::Keeping::read(&keys).map_err(|why| could_not(format!("The keeping keys can't be used: {why}.")))?;
                let (name, _) = keeping.current();
                part.lines.info("secrets Away keeps are sealed to its key", &[("key", away_part::sealed::written(&name).as_str().into())]);
                Some(keeping)
            }
            (Some(_), None) => {
                return Err(could_not("Secrets Away keeps are opened only with the sign-in service and the server told.".to_string()))
            }
            (None, _) => None,
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
        let inside = Arc::new(Inside { checks, domain, kept, keeping, lines: part.lines.clone() });
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
            "/internal/kept" => {
                let Some(session) = said("session").filter(|session| !session.is_empty()) else {
                    return plain(StatusCode::BAD_REQUEST, "A kept secret is asked for with the session, as session=.\n");
                };
                self.kept(session).await
            }
            _ => plain(StatusCode::NOT_FOUND, "Nothing is at this address.\n"),
        }
    }

    /// The checks of a session that whatever the issuer hands out rests on: the session
    /// runs, the server says whose it is and hands over what the sign-in service signed
    /// about that person a moment ago, which the issuer checks for itself. Gives what the
    /// server said and what the service signed, or the refusal.
    async fn checked(&self, checks: &Checks, session: &str) -> Result<(server::Vouched, Json, u64), Refused> {
        let because = |why: &'static str, says: String| Refused { why, says };
        let vouched = match checks.server.vouch(session).await {
            Ok(vouched) => vouched,
            Err(NotVouched::NoSession) => return Err(because("no-session", "No session of that name runs.".to_string())),
            Err(NotVouched::SignedOut(said)) => return Err(because("signed-out", said)),
            Err(NotVouched::NotAsked(why)) => return Err(because("server", why)),
        };
        let now = time::now().as_secs();
        let about = checks.service.check(vouched.signed.expose(), now).await.map_err(|why| because("sign-in", why))?;
        if about.text_of("sub") != Some(vouched.subject.as_str()) {
            let why = "What the sign-in service signed is about another person than the one the session is of.";
            return Err(because("sign-in", why.to_string()));
        }
        if !about.get("iat").and_then(Json::count).is_some_and(|signed_at| signed_at + SIGNED_AT_MOST_AGO >= now) {
            let why = "The sign-in service signed who this person is too long ago, or doesn't say when.";
            return Err(because("sign-in", why.to_string()));
        }
        Ok((vouched, about, now))
    }

    /// The status a refusal is answered with.
    fn status_of(refused: &Refused) -> StatusCode {
        match refused.why {
            "no-session" | "not-kept" => StatusCode::NOT_FOUND,
            "server" | "authority" => StatusCode::BAD_GATEWAY,
            "request" => StatusCode::BAD_REQUEST,
            "not-recorded" => StatusCode::SERVICE_UNAVAILABLE,
            _ => StatusCode::FORBIDDEN,
        }
    }

    /// The secret kept for the machine of a session: checked as a sign-in is, recorded,
    /// and then handed to the gateway, for that session alone. Away's own store keeps it
    /// sealed, and the server hands it over with the session; where it holds none, the
    /// installation's folder of secrets is looked in.
    async fn kept(&self, session: &str) -> Answer {
        let Some(checks) = self.checks.as_ref().filter(|_| self.kept.is_some() || self.keeping.is_some()) else {
            return says(StatusCode::NOT_FOUND, "no-kept", "This installation keeps no secrets for machines.");
        };
        let refuse = |refused: Refused| {
            self.lines.warn(
                "handed out no kept secret",
                &[("session", session.into()), ("why", refused.why.into()), ("says", refused.says.as_str().into())],
            );
            says(Self::status_of(&refused), refused.why, &refused.says)
        };
        let because = |why: &'static str, says: String| Refused { why, says };
        let (vouched, _, _) = match self.checked(checks, session).await {
            Ok(checked) => checked,
            Err(refused) => return refuse(refused),
        };
        if vouched.signs_in != "kept" {
            return refuse(because("policy", "This session's machine is not one a kept secret signs in on.".to_string()));
        }
        let Some(name) = vouched.kept.as_deref().filter(|name| is_a_kept_name(name)) else {
            return refuse(because("policy", "The server names no kept secret for this session's machine.".to_string()));
        };
        let secret = match (&vouched.kept_sealed, &self.keeping, &self.kept) {
            // sealed in Away's own store: it opens with the issuer's key, under its name alone
            (Some(sealed), Some(keeping), _) => match keeping.open(sealed, name) {
                Ok(secret) => secret,
                Err(keeping::NotOpened::OtherKey) => {
                    return refuse(because("not-kept", format!("The secret named {name} is sealed to a key this issuer doesn't hold.")));
                }
                Err(_) => return refuse(because("not-kept", format!("The secret named {name} doesn't open: it is not as it was sealed."))),
            },
            (Some(_), None, _) => return refuse(because("not-kept", "This issuer holds no key that opens Away's own store.".to_string())),
            // the installation's folder: read anew each time, a small file, so that a
            // secret the installation changes is the one handed out next
            (None, _, Some(folder)) => {
                let held = match std::fs::read(folder.join(name)) {
                    Ok(held) => held,
                    Err(error) => return refuse(because("not-kept", format!("Away keeps no secret named {name}: {error}."))),
                };
                let Some(secret) = String::from_utf8(held)
                    .ok()
                    .map(|held| held.trim_end_matches(['\r', '\n']).to_string())
                    .filter(|held| !held.is_empty())
                else {
                    return refuse(because("not-kept", format!("The secret named {name} is empty, or is no text.")));
                };
                Secret::new(secret)
            }
            (None, _, None) => return refuse(because("not-kept", format!("Away keeps no secret named {name}."))),
        };
        // the record comes before the secret is handed out, and without it none is
        if let Err(why) = checks.server.handed(session, name).await {
            return refuse(because("not-recorded", why));
        }
        self.lines.info(
            "handed out a kept secret",
            &[
                ("who", vouched.subject.as_str().into()),
                ("machine", vouched.machine.as_str().into()),
                ("session", session.into()),
                ("kept", name.into()),
            ],
        );
        answer(StatusCode::OK, "application/json", Json::object([("password", secret.expose().into())]).write())
    }

    /// What signs the person of a session in on its machine: checked, made, recorded,
    /// and then handed out.
    async fn certificate(&self, session: &str, asked: &[u8]) -> Answer {
        let (Some(domain), Some(checks)) = (&self.domain, &self.checks) else {
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
        let (vouched, about, now) = match self.checked(checks, session).await {
            Ok(checked) => checked,
            Err(refused) => return refuse(Self::status_of(&refused), refused),
        };
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
        if let Err(why) = checks.server.made(session, &made.account, &made.number, made.until).await {
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
