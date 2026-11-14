//! Getting in: the addresses a person's browser is sent through to sign in, the one that
//! says who is signed in, and the one that signs out.
//!
//! | Address | Does |
//! |---|---|
//! | `GET /sign-in?to=/a/page` | sends the browser to the sign-in service, and remembers the page asked for |
//! | `GET /sign-in/back` | where the sign-in service sends the browser back to: takes what it signed, holds the sign-in, and goes to the page |
//! | `GET /api/who` | who is signed in, or why signing in failed |
//! | `POST /sign-out` | ends the sign-in |
//! | `GET /api/favourites` | what the person starred, of what they reach |
//! | `PUT /api/favourites?machine=…&program=…` | stars a machine, or a program of it; `DELETE` takes the star away |
//!
//! The browser holds two cookies and nothing else: one that names its sign-in, and for
//! the minutes a sign-in is under way one that ties it to the browser that began it.
//! Neither can be read by a script, and neither is sent to anyone but Away.

use std::sync::Arc;

use away_part::address::read_form;
use away_part::client::{Client, Secures};
use away_part::http::{answer, plain, Answer};
use away_part::json::Json;
use away_part::{time, version, Part};
use hyper::body::Incoming;
use hyper::header::{HeaderValue, CACHE_CONTROL, COOKIE, LOCATION, ORIGIN, SET_COOKIE};
use hyper::{Method, Request, StatusCode};

use crate::register::files::Register;
use crate::service::{Failed, Service};
use crate::sign_ins::{Failure, Person, SignIns, KEPT_FOR};
use crate::store::favourites::Favourite;
use crate::store::{self, Store};
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
    /// What Away keeps, where it is given a database.
    store: Option<Arc<Store>>,
    /// Why the database last gave nothing, so that it is written once and not with every request.
    said_of_the_store: std::sync::Mutex<Option<String>>,
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
        let store = match told.database {
            None => None,
            Some(database) => {
                let secured = match database.secured {
                    true => Some(
                        Secures::new(database.authorities.as_deref(), b"postgresql")
                            .map_err(|why| format!("The authorities to trust for the database can't be used: {why}."))?,
                    ),
                    false => None,
                };
                let settings = store::Settings {
                    host: database.host,
                    port: database.port,
                    database: database.name,
                    account: database.account,
                    password: database.password,
                    secured,
                    within: std::time::Duration::from_secs(10),
                };
                Some(Arc::new(Store::new(settings, part.lines.clone())))
            }
        };
        Ok(GettingIn { contact: told.contact, signing, register, store, said_of_the_store: std::sync::Mutex::new(None), part })
    }

    /// What Away keeps, where it is given a database.
    pub fn store(&self) -> Option<Arc<Store>> {
        self.store.clone()
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
            (path, true) if path.starts_with("/api/icons/") => self.icon(request, path),
            ("/api/built-from", true) => self.built_from(request),
            ("/api/machines", true) => self.machines(request),
            ("/api/register", true) => self.in_force(request),
            ("/api/favourites", true) => self.favourites(request).await,
            ("/api/favourites", _) if matches!(*request.method(), Method::PUT | Method::DELETE) => self.star(request).await,
            ("/api/favourites", _) => plain(StatusCode::METHOD_NOT_ALLOWED, "This address answers GET, PUT and DELETE.\n"),
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

    /// The picture a program has as its icon, for whoever is signed in. A picture is
    /// asked for by a name that is made from the picture itself, so a browser may keep it.
    fn icon(&self, request: &Request<Incoming>, path: &str) -> Answer {
        if self.signed_in(request).is_none() {
            return plain(StatusCode::UNAUTHORIZED, "This is for people who are signed in.\n");
        }
        let name = path.trim_start_matches("/api/icons/").strip_suffix(".png").unwrap_or_default();
        let (in_force, _) = self.register.in_force();
        match in_force.pictures.get(name) {
            Some(picture) => {
                let mut answer = answer(StatusCode::OK, "image/png", picture.as_ref().clone());
                answer.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("private, max-age=31536000, immutable"));
                answer
            }
            None => plain(StatusCode::NOT_FOUND, "No picture is at this address.\n"),
        }
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
                .filter(|rule| rule.reaches.iter().any(|reached| machine.is_reached_as(reached)))
                .map(|rule| Json::object([("rule", rule.name.as_str().into()), ("who", texts(&rule.who))]))
                .collect();
            says.push(("given_by".to_string(), Json::List(given)));
            says.push(("served_by".to_string(), texts(&machine.served_by)));
            says.push(("farm".to_string(), machine.farm.as_deref().map_or(Json::Null, Json::from)));
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
        // the farms, each with how its feed was last read
        let farms = self.register.farms().into_iter().map(|state| {
            let [file, line] = written_in(&state.farm.written_in);
            Json::object([
                ("name", state.farm.name.as_str().into()),
                ("feed", state.farm.feed.as_str().into()),
                ("groups", texts(&state.farm.groups)),
                ("read_at", state.read_at.map_or(Json::Null, Json::from)),
                ("failed", state.failed.as_deref().map_or(Json::Null, Json::from)),
                ("desktops", (state.desktops as u64).into()),
                ("programs", (state.programs as u64).into()),
                ("left_out", texts(&state.left_out)),
                file,
                line,
            ])
        });
        let says = Json::object([
            ("has_files", self.register.has_files().into()),
            ("since", since.into()),
            ("machines", Json::List(machines.collect())),
            ("rules", Json::List(rules.collect())),
            ("farms", Json::List(farms.collect())),
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

    /// What whoever asks starred, of what they reach now: the first starred first.
    async fn favourites(&self, request: &Request<Incoming>) -> Answer {
        let Some((_, person)) = self.signed_in(request) else {
            return plain(StatusCode::UNAUTHORIZED, "This is for people who are signed in.\n");
        };
        let says = |kept: bool, starred: Vec<Json>| {
            answer(StatusCode::OK, "application/json", Json::object([("kept", kept.into()), ("favourites", Json::List(starred))]).write())
        };
        let Some(store) = &self.store else { return says(false, Vec::new()) };
        let starred = match store.favourites(&person.subject).await {
            Ok(starred) => starred,
            Err(unusable) => return self.kept_nothing(&unusable),
        };
        self.store_answers();
        let (in_force, _) = self.register.in_force();
        let reached = in_force.reached_by(&person.groups);
        let starred = starred
            .into_iter()
            .filter(|favourite| reached.iter().any(|(machine, _)| is_there(machine, favourite)))
            .map(|favourite| {
                Json::object([
                    ("machine", favourite.machine.as_str().into()),
                    ("program", favourite.program.as_deref().map_or(Json::Null, Json::from)),
                ])
            })
            .collect();
        says(true, starred)
    }

    /// Stars a machine or a program for whoever asks, or takes the star away: only
    /// something they reach.
    async fn star(&self, request: &Request<Incoming>) -> Answer {
        let Some((signing, person)) = self.signed_in(request) else {
            return plain(StatusCode::UNAUTHORIZED, "This is for people who are signed in.\n");
        };
        if !from_aways_page(request, signing) {
            return plain(StatusCode::FORBIDDEN, "Only Away's own pages change what is starred.\n");
        }
        let named = read_form(request.uri().query().unwrap_or_default()).unwrap_or_default();
        let named =
            |name: &str| named.iter().find(|(its, _)| its == name).map(|(_, value)| value.clone()).filter(|value| !value.is_empty());
        let Some(machine) = named("machine") else {
            return plain(
                StatusCode::BAD_REQUEST,
                "A star is for a machine, named as machine=, or for a program of it, named as program= too.\n",
            );
        };
        let favourite = Favourite { machine, program: named("program") };
        let (in_force, _) = self.register.in_force();
        if !in_force.reached_by(&person.groups).iter().any(|(machine, _)| is_there(machine, &favourite)) {
            return plain(StatusCode::NOT_FOUND, "Nothing of that name is there for you.\n");
        }
        let Some(store) = &self.store else {
            return plain(StatusCode::SERVICE_UNAVAILABLE, "Away keeps nothing here: it is given no database.\n");
        };
        match store.star(&person.subject, &favourite, *request.method() == Method::PUT).await {
            Ok(()) => {
                self.store_answers();
                answer(StatusCode::OK, "application/json", "{}")
            }
            Err(unusable) => self.kept_nothing(&unusable),
        }
    }

    /// The answer where the database gave nothing. Why is written once, and again only
    /// when it is another reason or the database answered in between.
    fn kept_nothing(&self, unusable: &store::Unusable) -> Answer {
        let why = unusable.to_string();
        let mut said = self.said_of_the_store.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if said.as_deref() != Some(why.as_str()) {
            self.part.lines.warn("the database gave nothing", &[("why", why.as_str().into())]);
            *said = Some(why);
        }
        plain(StatusCode::SERVICE_UNAVAILABLE, "What Away keeps can't be reached right now.\n")
    }

    fn store_answers(&self) {
        let mut said = self.said_of_the_store.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if said.take().is_some() {
            self.part.lines.info("the database answers again", &[]);
        }
    }

    /// Ends the sign-in of the browser that asks.
    async fn sign_out(&self, request: &Request<Incoming>) -> Answer {
        let Some(signing) = &self.signing else { return plain(StatusCode::NOT_FOUND, "Nobody is signed in here.\n") };
        if !from_aways_page(request, signing) {
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
/// Whether a request that changes something comes from a page of Away's own: only such
/// a page can add the header, and a browser says where a request is from.
fn from_aways_page(request: &Request<Incoming>, signing: &Signing) -> bool {
    let from_elsewhere = request.headers().get(ORIGIN).is_some_and(|origin| origin.as_bytes() != signing.told.public.origin().as_bytes());
    request.headers().get(FROM_AWAYS_PAGE).is_some() && !from_elsewhere
}

/// Whether a machine is what was starred, or offers the program that was.
fn is_there(machine: &crate::register::Machine, favourite: &Favourite) -> bool {
    machine.name == favourite.machine
        && favourite.program.as_ref().is_none_or(|program| machine.programs.iter().any(|offered| offered.name == *program))
}

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
