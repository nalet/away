//! Getting in: the addresses a person's browser is sent through to sign in, the one that
//! says who is signed in, and the one that signs out.
//!
//! | Address | Does |
//! |---|---|
//! | `GET /sign-in?to=/a/page` | sends the browser to the sign-in service, and remembers the page asked for |
//! | `GET /sign-in/back` | where the sign-in service sends the browser back to: takes what it signed, holds the sign-in, and goes to the page |
//! | `GET /api/who` | who is signed in, or why signing in failed |
//! | `POST /sign-out` | ends the sign-in |
//!
//! The browser holds two cookies and nothing else: one that names its sign-in, and for
//! the minutes a sign-in is under way one that ties it to the browser that began it.
//! Neither can be read by a script, and neither is sent to anyone but Away.

use std::sync::Arc;

use away_part::address::read_form;
use away_part::client::Client;
use away_part::http::{answer, plain, Answer};
use away_part::json::Json;
use away_part::{time, version, Part};
use hyper::body::Incoming;
use hyper::header::{HeaderValue, COOKIE, LOCATION, ORIGIN, SET_COOKIE};
use hyper::{Method, Request, StatusCode};

use crate::register::files::Register;
use crate::service::{Failed, Service};
use crate::sign_ins::{Failure, Person, SignIns, KEPT_FOR};
use crate::told::{SignIn, Told};

/// The cookie that names a browser's sign-in.
pub const SIGNED_IN: &str = "__Host-away";
/// The cookie that ties a sign-in under way to the browser that began it.
pub const SIGNING_IN: &str = "__Host-away-signing-in";
/// The header a request that changes something has to carry, which only a page of
/// Away's own can set.
pub const FROM_AWAYS_PAGE: &str = "x-away";
/// Where a browser goes that came back from the sign-in service without a sign-in: the
/// page that says how one signs in.
pub const NOT_SIGNED_IN: &str = "/?sign-in=none";

/// What the program is built from: every part with its version and its licence, as
/// `scripts/contents.py --parts` writes it from the lock files. The build fails when
/// the two differ.
const BUILT_FROM: &str = include_str!("../built-from.json");
/// The text of the licence Away is under.
const LICENCE_TEXT: &str = include_str!("../../../LICENSE");

struct Signing {
    told: SignIn,
    service: Service,
    sign_ins: SignIns,
}

/// Everything that getting in needs.
pub struct GettingIn {
    contact: Option<String>,
    signing: Option<Signing>,
    register: Arc<Register>,
    part: Part,
}

impl GettingIn {
    /// Getting in, as the installation is told. Gives the reason where the authorities
    /// it is told to trust can't be used.
    pub fn new(told: Told, part: Part) -> Result<GettingIn, String> {
        let signing = match told.sign_in {
            Some(sign_in) => {
                let client = Client::new(sign_in.authorities.as_deref())?;
                Some(Signing { service: Service::new(sign_in.service.clone(), client), sign_ins: SignIns::new(), told: sign_in })
            }
            None => None,
        };
        let register = Arc::new(Register::new(told.files, part.lines.clone(), time::now().as_secs()));
        Ok(GettingIn { contact: told.contact, signing, register, part })
    }

    /// Whether the installation is told about a sign-in service.
    pub fn is_set_up(&self) -> bool {
        self.signing.is_some()
    }

    /// Answers a request at one of the addresses above, or says that it is none of them.
    pub async fn answer(&self, request: &Request<Incoming>) -> Option<Answer> {
        let reading = matches!(*request.method(), Method::GET | Method::HEAD);
        Some(match (request.uri().path(), reading) {
            ("/api/who", true) => self.who(request),
            ("/api/built-from", true) => self.built_from(request),
            ("/api/machines", true) => self.machines(request),
            ("/api/register", true) => self.in_force(request),
            ("/licence.txt", true) => answer(StatusCode::OK, "text/plain; charset=utf-8", LICENCE_TEXT),
            ("/sign-in", true) => self.begin(request).await,
            ("/sign-in/back", true) => self.back(request).await,
            ("/sign-out", _) if request.method() == Method::POST => self.sign_out(request).await,
            ("/api/who" | "/api/built-from" | "/api/machines" | "/api/register" | "/licence.txt" | "/sign-in" | "/sign-in/back", _) => {
                plain(StatusCode::METHOD_NOT_ALLOWED, "This address only answers GET.\n")
            }
            ("/sign-out", _) => plain(StatusCode::METHOD_NOT_ALLOWED, "This address only answers POST.\n"),
            _ => return None,
        })
    }

    /// Who asks, where they are signed in and their sign-in has not run out.
    fn signed_in(&self, request: &Request<Incoming>) -> Option<(&Signing, Person)> {
        let signing = self.signing.as_ref()?;
        match signing.sign_ins.who(cookie(request, SIGNED_IN)?, time::now().as_secs())? {
            (person, false) => Some((signing, person)),
            _ => None,
        }
    }

    /// The register, for whatever looks at its files as time passes.
    pub fn register(&self) -> Arc<Register> {
        self.register.clone()
    }

    /// What Away is built from, for whoever is signed in.
    fn built_from(&self, request: &Request<Incoming>) -> Answer {
        match self.signed_in(request) {
            Some(_) => answer(StatusCode::OK, "application/json", BUILT_FROM),
            None => plain(StatusCode::UNAUTHORIZED, "This is for people who are signed in.\n"),
        }
    }

    /// The machines that whoever asks may reach: those a rule gives to one of their
    /// groups, and no others.
    fn machines(&self, request: &Request<Incoming>) -> Answer {
        let Some((_, person)) = self.signed_in(request) else {
            return plain(StatusCode::UNAUTHORIZED, "This is for people who are signed in.\n");
        };
        let (in_force, _) = self.register.in_force();
        let machines: Vec<Json> = in_force.reached_by(&person.groups).into_iter().map(|(machine, _)| machine.for_a_person()).collect();
        answer(StatusCode::OK, "application/json", Json::object([("machines", Json::List(machines))]).write())
    }

    /// What is in force, for those who administer Away: every machine and every rule
    /// with the file it comes from, since when, and the change that was refused, where
    /// one was.
    fn in_force(&self, request: &Request<Incoming>) -> Answer {
        let Some((signing, person)) = self.signed_in(request) else {
            return plain(StatusCode::UNAUTHORIZED, "This is for people who are signed in.\n");
        };
        if !person.groups.iter().any(|group| signing.told.administrators.contains(group)) {
            return plain(StatusCode::FORBIDDEN, "This is for those who administer Away.\n");
        }
        let (in_force, since) = self.register.in_force();
        let texts = |texts: &[String]| Json::List(texts.iter().map(|text| text.as_str().into()).collect());
        let written_in = |(file, line): &(String, usize)| [("file", Json::from(file.as_str())), ("line", Json::from(*line as u64))];
        let machines = in_force.machines.iter().map(|machine| {
            let Json::Object(mut says) = machine.for_a_person() else { unreachable!("a machine is an object") };
            says.extend(written_in(&machine.written_in).map(|(name, value)| (name.to_string(), value)));
            // who reaches it: each rule that gives it, and to whom
            let given: Vec<Json> = in_force
                .rules
                .iter()
                .filter(|rule| rule.reaches.iter().any(|reached| *reached == machine.name || machine.groups.contains(reached)))
                .map(|rule| Json::object([("rule", rule.name.as_str().into()), ("who", texts(&rule.who))]))
                .collect();
            says.push(("given_by".to_string(), Json::List(given)));
            says.push(("served_by".to_string(), texts(&machine.served_by)));
            Json::Object(says)
        });
        let rules = in_force.rules.iter().map(|rule| {
            let [file, line] = written_in(&rule.written_in);
            Json::object([("name", rule.name.as_str().into()), ("who", texts(&rule.who)), ("reaches", texts(&rule.reaches)), file, line])
        });
        let refused = self.register.refused().map_or(Json::Null, |refusal| {
            Json::object([
                ("at", refusal.at.into()),
                ("file", refusal.why.file.as_str().into()),
                ("line", (refusal.why.line as u64).into()),
                ("why", refusal.why.why.as_str().into()),
            ])
        });
        let says = Json::object([
            ("has_files", self.register.has_files().into()),
            ("since", since.into()),
            ("machines", Json::List(machines.collect())),
            ("rules", Json::List(rules.collect())),
            ("refused", refused),
        ]);
        answer(StatusCode::OK, "application/json", says.write())
    }

    /// Who is signed in, for the page.
    fn who(&self, request: &Request<Incoming>) -> Answer {
        let now = time::now().as_secs();
        let mut says = vec![
            ("version", version::VERSION.into()),
            ("commit", version::COMMIT.into()),
            ("source", version::SOURCE.into()),
            ("licence", version::LICENCE.into()),
            ("contact", self.contact.as_deref().map_or(Json::Null, Json::from)),
            ("set_up", self.signing.is_some().into()),
        ];
        let held = self.signing.as_ref().and_then(|signing| Some((signing, signing.sign_ins.who(cookie(request, SIGNED_IN)?, now)?)));
        match held {
            Some((signing, (person, ran_out))) => {
                let among = |groups: &[String]| person.groups.iter().any(|group| groups.contains(group));
                says.extend([
                    ("signed_in", true.into()),
                    ("ran_out", ran_out.into()),
                    ("name", person.name.as_str().into()),
                    ("address", person.address.as_deref().map_or(Json::Null, Json::from)),
                    ("initials", person.initials().into()),
                    ("administers", among(&signing.told.administrators).into()),
                    ("reads_records", among(&signing.told.record_readers).into()),
                ]);
            }
            None => {
                let failure = self
                    .signing
                    .as_ref()
                    .and_then(|signing| signing.sign_ins.take_failure(cookie(request, SIGNING_IN)?))
                    .map_or(Json::Null, |failure| Json::object([("why", failure.why.into()), ("details", failure.details.into())]));
                says.extend([("signed_in", false.into()), ("failed", failure)]);
            }
        }
        answer(
            StatusCode::OK,
            "application/json",
            Json::Object(says.into_iter().map(|(name, value)| (name.to_string(), value)).collect()).write(),
        )
    }

    /// Sends the browser to the sign-in service.
    async fn begin(&self, request: &Request<Incoming>) -> Answer {
        let Some(signing) = &self.signing else { return go_to("/") };
        let now = time::now().as_secs();
        let asked = read_form(request.uri().query().unwrap_or_default()).unwrap_or_default();
        let to = asked.iter().find(|(name, _)| name == "to").map(|(_, to)| to.as_str()).filter(|to| is_a_page(to)).unwrap_or("/");
        let beginning = signing.sign_ins.begin(to, now);
        let mut answer = match signing.service.sign_in_page(&beginning.state, &beginning.number, &beginning.challenge).await {
            Ok(page) => go_to(&page),
            Err(failed) => self.failed(signing, &beginning.browser, failed, now),
        };
        set_cookie(&mut answer, SIGNING_IN, &beginning.browser, 600);
        answer
    }

    /// Takes the browser back from the sign-in service.
    async fn back(&self, request: &Request<Incoming>) -> Answer {
        let Some(signing) = &self.signing else { return go_to("/") };
        let now = time::now().as_secs();
        let came_with = read_form(request.uri().query().unwrap_or_default()).unwrap_or_default();
        let came_with = |name: &str| came_with.iter().find(|(its, _)| its == name).map(|(_, value)| value.as_str());
        let browser = cookie(request, SIGNING_IN);
        // a sign-in that this browser didn't begin, or began too long ago: to the way in,
        // which doesn't send the browser on by itself, so that it never goes in circles
        let Some(begun) = came_with("state").and_then(|state| signing.sign_ins.back(state, browser, now)) else {
            return go_to(NOT_SIGNED_IN);
        };
        let mut answer = match (came_with("error"), came_with("code")) {
            // the person gave up on the sign-in page, or was refused there, and it said so itself
            (Some("access_denied" | "login_required" | "interaction_required" | "consent_required" | "account_selection_required"), _) => {
                go_to(NOT_SIGNED_IN)
            }
            (Some(error), _) => {
                let more = came_with("error_description").map(|more| format!(": {more}")).unwrap_or_default();
                let failed = Failed::Unusable(format!("The sign-in service sent the browser back with an error ({error}{more})."));
                self.failed(signing, &begun.browser, failed, now)
            }
            (None, None) => {
                let failed = Failed::Unusable("The sign-in service sent the browser back with nothing to exchange.".to_string());
                self.failed(signing, &begun.browser, failed, now)
            }
            (None, Some(code)) => match signing.service.exchange(code, &begun.proof, &begun.number, now).await {
                Err(failed) => self.failed(signing, &begun.browser, failed, now),
                Ok(signed) => match signed.says.as_ref().and_then(|says| Person::read(says, &signing.told.groups_entry)) {
                    None => {
                        let failed = Failed::Unusable("What the sign-in service signed doesn't say who signed in.".to_string());
                        self.failed(signing, &begun.browser, failed, now)
                    }
                    Some(person) => {
                        // a sign-in of this browser that ran out is good again for the same
                        // person; anybody else gets a new one, and the old one ends
                        let had = cookie(request, SIGNED_IN);
                        let name = match had {
                            Some(had) if signing.sign_ins.again(had, person.clone(), &signed, now) => had.to_string(),
                            _ => {
                                if let Some((_, Some(renewal))) = had.and_then(|had| signing.sign_ins.end(had)) {
                                    signing.service.forget(&renewal).await;
                                }
                                signing.sign_ins.hold(person.clone(), &signed, now)
                            }
                        };
                        self.part
                            .lines
                            .info("signed in", &[("who", (&person.subject).into()), ("groups", (person.groups.len() as u64).into())]);
                        self.count(signing);
                        let mut answer = go_to(&begun.to);
                        set_cookie(&mut answer, SIGNED_IN, &name, KEPT_FOR);
                        answer
                    }
                },
            },
        };
        // the sign-in is no longer under way, unless its failure is still to be said
        if answer.headers().get(LOCATION).is_none_or(|to| to != "/?sign-in=failed") {
            set_cookie(&mut answer, SIGNING_IN, "", 0);
        }
        answer
    }

    /// Ends the sign-in of the browser that asks.
    async fn sign_out(&self, request: &Request<Incoming>) -> Answer {
        let Some(signing) = &self.signing else { return plain(StatusCode::NOT_FOUND, "Nobody is signed in here.\n") };
        // only a page of Away's own can add this header, and a browser says where a request is from
        let from_elsewhere =
            request.headers().get(ORIGIN).is_some_and(|origin| origin.as_bytes() != signing.told.public.origin().as_bytes());
        if request.headers().get(FROM_AWAYS_PAGE).is_none() || from_elsewhere {
            return plain(StatusCode::FORBIDDEN, "Only Away's own pages sign out.\n");
        }
        if let Some((person, renewal)) = cookie(request, SIGNED_IN).and_then(|name| signing.sign_ins.end(name)) {
            if let Some(renewal) = renewal {
                signing.service.forget(&renewal).await;
            }
            self.part.lines.info("signed out", &[("who", (&person.subject).into())]);
            self.count(signing);
        }
        let mut answer = answer(StatusCode::OK, "application/json", "{}");
        set_cookie(&mut answer, SIGNED_IN, "", 0);
        answer
    }

    /// Keeps why a sign-in failed for the page to say, writes a line, and sends the
    /// browser to that page.
    fn failed(&self, signing: &Signing, browser: &str, failed: Failed, now: u64) -> Answer {
        let why = match failed {
            Failed::NoAnswer(_) => "no-answer",
            Failed::Unusable(_) | Failed::Refused(_) => "unusable",
        };
        self.part.lines.warn("signing in failed", &[("why", why.into()), ("details", failed.details().into())]);
        self.part.measures.counter("away_sign_in_failures_total", "Sign-ins that failed, by what failed.", &[("why", why)]).add_one();
        signing.sign_ins.failed(browser, Failure { why, details: failed.details().to_string() }, now);
        go_to("/?sign-in=failed")
    }

    fn count(&self, signing: &Signing) {
        self.part.measures.gauge("away_sign_ins", "The sign-ins the server holds.", &[]).set(signing.sign_ins.count() as u64);
    }

    /// Renews the sign-ins that are about to run out.
    pub async fn renew(&self) {
        let Some(signing) = &self.signing else { return };
        let now = time::now().as_secs();
        for renewal in signing.sign_ins.forget_old(now) {
            signing.service.forget(&renewal).await;
        }
        self.count(signing);
        for (name, renewal) in signing.sign_ins.due(now) {
            match signing.service.renew(&renewal, now).await {
                Ok(signed) => signing.sign_ins.renewed(&name, &signed, &signing.told.groups_entry, now),
                Err(Failed::Refused(why)) => {
                    signing.sign_ins.refused(&name);
                    self.part.lines.info("a sign-in ran out", &[("details", (&why).into())]);
                }
                Err(Failed::NoAnswer(why) | Failed::Unusable(why)) => {
                    signing.sign_ins.unanswered(&name, now);
                    self.part.lines.warn("a sign-in could not be renewed", &[("details", (&why).into())]);
                }
            }
        }
    }

    /// Renews sign-ins as they come due, until the part stops.
    pub async fn keep_renewing(self: Arc<Self>) {
        let Some(every) = self.signing.as_ref().map(|signing| signing.told.looks_every) else { return };
        let mut stop = self.part.stop.clone();
        loop {
            tokio::select! {
                _ = tokio::time::sleep(every) => self.renew().await,
                _ = stop.wait() => return,
            }
        }
    }
}

/// Whether a text names a page of Away's own, and nothing anywhere else.
fn is_a_page(to: &str) -> bool {
    to.starts_with('/')
        && !to.starts_with("//")
        && to.len() <= 2048
        && !to.chars().any(|character| character == '\\' || character.is_control())
}

/// What a cookie of this name holds, in what the browser sent.
fn cookie<'a>(request: &'a Request<Incoming>, name: &str) -> Option<&'a str> {
    request
        .headers()
        .get_all(COOKIE)
        .iter()
        .filter_map(|sent| sent.to_str().ok())
        .flat_map(|sent| sent.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(its, value)| *its == name && !value.is_empty())
        .map(|(_, value)| value)
}

/// Gives the browser a cookie that no script can read, that travels over TLS alone, and
/// that is sent to Away alone. For no seconds at all, the browser forgets it.
fn set_cookie(answer: &mut Answer, name: &str, value: &str, for_seconds: u64) {
    let cookie = format!("{name}={value}; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age={for_seconds}");
    if let Ok(cookie) = HeaderValue::from_str(&cookie) {
        answer.headers_mut().append(SET_COOKIE, cookie);
    }
}

/// Sends the browser on to another address.
fn go_to(address: &str) -> Answer {
    let mut answer = plain(StatusCode::SEE_OTHER, "This way.\n");
    let to = HeaderValue::from_str(address).unwrap_or(HeaderValue::from_static("/"));
    answer.headers_mut().insert(LOCATION, to);
    answer
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_page_of_aways_own_is_gone_to_after_signing_in() {
        for page in ["/", "/machines/fin-app-02", "/sessions?open=1", "/a/b#c"] {
            assert!(is_a_page(page), "{page}");
        }
        for elsewhere in [
            "",
            "machines",
            "//elsewhere.example.com",
            "/\\elsewhere.example.com",
            "https://elsewhere.example.com/",
            "/a\r\nx: y",
            "javascript:alert(1)",
        ] {
            assert!(!is_a_page(elsewhere), "{elsewhere:?}");
        }
        assert!(!is_a_page(&format!("/{}", "a".repeat(2048))));
    }
}
