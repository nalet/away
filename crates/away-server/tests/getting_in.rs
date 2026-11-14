//! Getting in, from a browser's side: the server runs as it does in an installation,
//! a stand-in for the organisation's sign-in service runs beside it, and a browser of
//! the tests' own goes where it is sent, with the cookies it is given.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use away_part::address::Address;
use away_part::json::Json;
use away_part::{stop, Lines, Part, Settings, Stopper};
use away_secret::Secret;
use away_server::getting_in::{FROM_AWAYS_PAGE, NOT_SIGNED_IN, SIGNED_IN, SIGNING_IN};
use away_server::service;
use away_server::stand_in::SignInService;
use away_server::told::{SignIn, Told};
use away_server::Server;

/// The server, the sign-in service beside it, and what the server wrote.
struct World {
    /// What the server and the sign-in service run on, for as long as the world is there.
    _runtime: tokio::runtime::Runtime,
    service: SignInService,
    away: String,
    lines: Lines,
    _stopper: Stopper,
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").expect("a port").local_addr().expect("its address").port()
}

fn world() -> World {
    world_told(|_| {})
}

/// A world in which the server is told something else than usual.
fn world_told(change: impl FnOnce(&mut SignIn)) -> World {
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().expect("a runtime");
    let port = free_port();
    let away = format!("http://127.0.0.1:{port}");
    let back = format!("{away}/sign-in/back");
    let service = runtime.block_on(SignInService::start("away", "s3cr:t", &back));
    let mut sign_in = SignIn {
        public: Address::read(&away).expect("Away's address"),
        service: service::Settings {
            service: service.address(),
            client: "away".to_string(),
            secret: Secret::new("s3cr:t"),
            back,
            asks_for: "openid profile email".to_string(),
        },
        authorities: None,
        groups_entry: "groups".to_string(),
        administrators: vec!["away-admins".to_string()],
        record_readers: vec!["auditors".to_string()],
        looks_every: Duration::from_millis(100),
    };
    change(&mut sign_in);
    let told = Told { sign_in: Some(sign_in), contact: Some("it-help@example.com".to_string()) };
    let (away, lines, stopper) = serve(&runtime, port, told);
    World { _runtime: runtime, service, away, lines, _stopper: stopper }
}

fn serve(runtime: &tokio::runtime::Runtime, port: u16, told: Told) -> (String, Lines, Stopper) {
    let (stopper, stop) = stop::pair();
    let lines = Lines::kept_for("server");
    let part = Part::new("server", lines.clone(), stop);
    let settings = Settings { address: format!("127.0.0.1:{port}"), health_address: "127.0.0.1:0".to_string() };
    let server = runtime.block_on(Server::bind(&settings, told, part)).expect("the server binds");
    runtime.spawn(server.run());
    (format!("http://127.0.0.1:{port}"), lines, stopper)
}

/// What came back to the browser.
struct Page {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl Page {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(its, _)| its == name).map(|(_, value)| value.as_str())
    }

    fn goes_to(&self) -> &str {
        assert_eq!(self.status, 303, "{}", self.body);
        self.header("location").expect("where to go")
    }

    fn says(&self) -> Json {
        assert_eq!(self.status, 200, "{}", self.body);
        Json::read(&self.body).expect("JSON")
    }

    /// The cookies it sets, as they were written.
    fn sets(&self) -> Vec<&str> {
        self.headers.iter().filter(|(name, _)| name == "set-cookie").map(|(_, value)| value.as_str()).collect()
    }
}

/// A browser: it keeps the cookies each address gives it, and sends them back there.
#[derive(Default)]
struct Browser {
    cookies: HashMap<String, Vec<(String, String)>>,
}

impl Browser {
    fn ask(&mut self, method: &str, address: &str, headers: &[(&str, &str)]) -> Page {
        let (origin, path) = match address.strip_prefix("http://").and_then(|rest| rest.split_once('/')) {
            Some((host, path)) => (host.to_string(), format!("/{path}")),
            None => (address.trim_start_matches("http://").to_string(), "/".to_string()),
        };
        let mut request = format!("{method} {path} HTTP/1.1\r\nhost: {origin}\r\nconnection: close\r\ncontent-length: 0\r\n");
        let kept = self.cookies.entry(origin.clone()).or_default();
        if !kept.is_empty() {
            let sent: Vec<String> = kept.iter().map(|(name, value)| format!("{name}={value}")).collect();
            request.push_str(&format!("cookie: {}\r\n", sent.join("; ")));
        }
        for (name, value) in headers {
            request.push_str(&format!("{name}: {value}\r\n"));
        }
        request.push_str("\r\n");
        let mut connection = TcpStream::connect(&origin).unwrap_or_else(|error| panic!("{origin} takes no connection: {error}"));
        connection.set_read_timeout(Some(Duration::from_secs(20))).expect("a time limit");
        connection.write_all(request.as_bytes()).expect("the request goes out");
        let mut answer = String::new();
        connection.read_to_string(&mut answer).expect("an answer comes back");
        let (head, body) = answer.split_once("\r\n\r\n").expect("a head and a body");
        let mut lines = head.split("\r\n");
        let status = lines.next().and_then(|first| first.split(' ').nth(1)).and_then(|status| status.parse().ok()).expect("a status");
        let headers: Vec<(String, String)> =
            lines.filter_map(|line| line.split_once(": ")).map(|(name, value)| (name.to_ascii_lowercase(), value.to_string())).collect();
        for (_, cookie) in headers.iter().filter(|(name, _)| name == "set-cookie") {
            let (pair, attributes) = cookie.split_once("; ").unwrap_or((cookie, ""));
            let (name, value) = pair.split_once('=').expect("a cookie's name and value");
            kept.retain(|(its, _)| its != name);
            if !attributes.contains("Max-Age=0") {
                kept.push((name.to_string(), value.to_string()));
            }
        }
        Page { status, headers, body: body.to_string() }
    }

    fn get(&mut self, address: &str) -> Page {
        self.ask("GET", address, &[])
    }

    fn cookie(&self, world: &World, name: &str) -> Option<String> {
        let origin = world.away.trim_start_matches("http://");
        self.cookies.get(origin)?.iter().find(|(its, _)| its == name).map(|(_, value)| value.clone())
    }

    /// Signs in the way a person does: asks for a page, is sent to the sign-in service,
    /// and comes back. Gives the last page: the one that sends the browser on.
    fn sign_in(&mut self, world: &World, to: &str) -> Page {
        let to_service = self.get(&format!("{}/sign-in?to={to}", world.away));
        let back = self.get(to_service.goes_to());
        self.get(back.goes_to())
    }

    fn who(&mut self, world: &World) -> Json {
        self.get(&format!("{}/api/who", world.away)).says()
    }
}

#[test]
fn away_says_what_it_is_built_from_to_those_who_are_signed_in_and_its_licence_to_anyone() {
    let world = world();
    let mut browser = Browser::default();
    // the licence's text is for anyone, as the licence itself asks
    let licence = browser.get(&format!("{}/licence.txt", world.away));
    assert_eq!((licence.status, licence.header("content-type")), (200, Some("text/plain; charset=utf-8")));
    assert!(licence.body.starts_with("Mozilla Public License Version 2.0"), "{}", &licence.body[..60.min(licence.body.len())]);
    assert_eq!(browser.who(&world).text_of("licence"), Some("MPL-2.0"));
    // the parts are for those who are signed in
    let refused = browser.get(&format!("{}/api/built-from", world.away));
    assert_eq!((refused.status, refused.body.as_str()), (401, "This is for people who are signed in.\n"));
    assert_eq!(browser.ask("POST", &format!("{}/api/built-from", world.away), &[]).status, 405);

    browser.sign_in(&world, "/");
    let parts = browser.get(&format!("{}/api/built-from", world.away)).says();
    let Json::List(parts) = parts else { panic!("a list of parts") };
    assert!(parts.len() > 20, "{} parts", parts.len());
    for part in &parts {
        assert!(part.text_of("name").is_some_and(|name| !name.is_empty()), "every part has a name");
        assert!(part.text_of("licence").is_some_and(|licence| !licence.is_empty()), "{:?} has a licence", part.text_of("name"));
        assert!(part.text_of("version").is_some());
        assert!(!part.text_of("name").unwrap().starts_with("away-"), "Away's own parts are not what it is built from");
    }
    // in the order of their names, whichever way a name is written
    let names: Vec<String> = parts.iter().map(|part| part.text_of("name").unwrap().to_lowercase()).collect();
    assert!(names.is_sorted(), "{names:?}");
    // the libraries that secure connections and that run them are among them, and the pages' faces
    for held in ["rustls", "tokio", "@fontsource/inter"] {
        assert!(names.iter().any(|name| name == held), "{held} is listed");
    }
}

/// Waits until something is the case, for a few seconds at most.
fn until(what: &str, mut is: impl FnMut() -> bool) {
    let began = Instant::now();
    while !is() {
        assert!(began.elapsed() < Duration::from_secs(10), "it never happened: {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_person_signs_in_and_is_on_the_page_they_asked_for() {
    let world = world();
    let mut browser = Browser::default();

    // sent to the sign-in service, with everything it needs and nothing secret
    let to_service = browser.get(&format!("{}/sign-in?to=/machines/fin-app-02", world.away));
    let at_service = to_service.goes_to().to_string();
    assert!(at_service.starts_with(&format!("{}/authorize?response_type=code&client_id=away&redirect_uri=", world.service.address())));
    for needed in ["&scope=openid%20profile%20email", "&state=", "&nonce=", "&code_challenge=", "&code_challenge_method=S256"] {
        assert!(at_service.contains(needed), "{needed} in {at_service}");
    }
    assert!(!at_service.contains("s3cr"), "the client's secret never passes through the browser");
    let under_way = to_service.sets();
    assert_eq!(under_way.len(), 1);
    assert!(
        under_way[0].starts_with(&format!("{SIGNING_IN}="))
            && under_way[0].ends_with("; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=600")
    );

    // the sign-in service signs the person in and sends the browser back
    let back = browser.get(&at_service);
    let came_back = browser.get(back.goes_to());
    assert_eq!(came_back.goes_to(), "/machines/fin-app-02");
    let sets = came_back.sets();
    assert_eq!(sets.len(), 2, "{sets:?}");
    assert!(
        sets[0].starts_with(&format!("{SIGNED_IN}=")) && sets[0].ends_with("; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=2592000")
    );
    assert_eq!(sets[1], format!("{SIGNING_IN}=; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=0"));
    assert_eq!(came_back.header("cache-control"), Some("no-store"));

    // who is signed in, with nothing of the sign-in itself
    let who = browser.who(&world);
    assert_eq!(who.get("signed_in"), Some(&Json::Flag(true)));
    assert_eq!(who.text_of("name"), Some("Alex Urban"));
    assert_eq!(who.text_of("address"), Some("alex.urban@example.com"));
    assert_eq!(who.text_of("initials"), Some("AU"));
    assert_eq!(who.get("administers"), Some(&Json::Flag(true)));
    assert_eq!(who.get("reads_records"), Some(&Json::Flag(false)));
    assert_eq!(who.get("ran_out"), Some(&Json::Flag(false)));
    assert_eq!(who.text_of("contact"), Some("it-help@example.com"));
    assert_eq!(who.get("set_up"), Some(&Json::Flag(true)));
    let written = who.write();
    for held in world.service.renewals() {
        assert!(!written.contains(&held), "what renews the sign-in stays in the server");
    }
    assert!(!written.contains("eyJ"), "no token is in what the page is told");

    // a browser without the cookie is nobody
    let other = Browser::default().who(&world);
    assert_eq!(other.get("signed_in"), Some(&Json::Flag(false)));
    assert_eq!(other.get("failed"), Some(&Json::Null));
    assert_eq!(other.get("name"), None);

    // and the server wrote who signed in, by what the service knows them by
    assert!(world.lines.kept().iter().any(|line| line.contains("\"event\":\"signed in\"") && line.contains("\"who\":\"7c1f\"")));
    assert!(world.lines.kept().iter().all(|line| !line.contains("s3cr") && !line.contains("eyJ")), "and no secret");
}

#[test]
fn a_sign_in_is_ended_only_by_the_browser_that_began_it() {
    let world = world();
    let mut victim = Browser::default();
    let mut other = Browser::default();
    // somebody begins a sign-in, signs in themselves, and hands the last step to another browser
    let to_service = other.get(&format!("{}/sign-in", world.away));
    let back = other.get(to_service.goes_to());
    let last_step = back.goes_to().to_string();
    let came_back = victim.get(&last_step);
    assert_eq!(came_back.goes_to(), NOT_SIGNED_IN);
    assert!(came_back.sets().iter().all(|cookie| !cookie.starts_with(&format!("{SIGNED_IN}="))));
    assert_eq!(victim.who(&world).get("signed_in"), Some(&Json::Flag(false)));
    // and the one who began it can't use it either any more
    assert_eq!(other.get(&last_step).goes_to(), NOT_SIGNED_IN);
    assert_eq!(other.who(&world).get("signed_in"), Some(&Json::Flag(false)));
    // a way back that nobody began
    assert_eq!(victim.get(&format!("{}/sign-in/back?code=x&state=y", world.away)).goes_to(), NOT_SIGNED_IN);
    assert_eq!(victim.get(&format!("{}/sign-in/back", world.away)).goes_to(), NOT_SIGNED_IN);
}

#[test]
fn after_signing_in_the_browser_goes_to_a_page_of_aways_and_nowhere_else() {
    let world = world();
    for elsewhere in ["//elsewhere.example.com", "https%3A%2F%2Felsewhere.example.com%2F", "%2F%5Celsewhere.example.com", "machines"] {
        assert_eq!(Browser::default().sign_in(&world, elsewhere).goes_to(), "/", "{elsewhere}");
    }
    assert_eq!(Browser::default().sign_in(&world, "%2Fsessions%3Fopen%3D1").goes_to(), "/sessions?open=1");
}

#[test]
fn somebody_who_gives_up_on_the_sign_in_page_is_back_at_the_start() {
    let world = world();
    world.service.sends_back_with(Some("access_denied"));
    let mut browser = Browser::default();
    assert_eq!(browser.sign_in(&world, "/").goes_to(), NOT_SIGNED_IN);
    let who = browser.who(&world);
    assert_eq!(who.get("signed_in"), Some(&Json::Flag(false)));
    assert_eq!(who.get("failed"), Some(&Json::Null), "nothing failed: the sign-in page said what there was to say");
}

#[test]
fn a_sign_in_service_that_does_not_answer_is_said_to_not_answer() {
    let world = world();
    world.service.stop();
    until("the sign-in service is gone", || TcpStream::connect(world.service.address().trim_start_matches("http://")).is_err());
    let mut browser = Browser::default();
    let failed = browser.get(&format!("{}/sign-in", world.away));
    assert_eq!(failed.goes_to(), "/?sign-in=failed");
    let who = browser.who(&world);
    assert_eq!(who.get("signed_in"), Some(&Json::Flag(false)));
    let said = who.get("failed").expect("why it failed");
    assert_eq!(said.text_of("why"), Some("no-answer"));
    assert!(
        said.text_of("details").is_some_and(|details| details.starts_with("The sign-in service: it couldn't be reached: ")),
        "{said:?}"
    );
    // said once, and to the browser it happened to
    assert_eq!(browser.who(&world).get("failed"), Some(&Json::Null));
    assert!(world
        .lines
        .kept()
        .iter()
        .any(|line| line.contains("\"event\":\"signing in failed\"") && line.contains("\"why\":\"no-answer\"")));
}

#[test]
fn a_sign_in_service_whose_answer_cant_be_used_is_said_to_be_set_up_wrongly() {
    // it names itself something else than Away is told
    let world = self::world();
    world.service.names_itself("https://elsewhere.example.com");
    let mut browser = Browser::default();
    assert_eq!(browser.get(&format!("{}/sign-in", world.away)).goes_to(), "/?sign-in=failed");
    let who = browser.who(&world);
    let said = who.get("failed").expect("why it failed");
    assert_eq!(said.text_of("why"), Some("unusable"));
    assert_eq!(
        said.text_of("details"),
        Some(
            format!(
                "What the sign-in service says of itself can't be used: it names itself https://elsewhere.example.com, and Away is told {}.",
                world.service.address()
            )
            .as_str()
        )
    );

    // it knows Away by another secret
    let world = world_told(|sign_in| sign_in.service.secret = Secret::new("another"));
    let mut browser = Browser::default();
    assert_eq!(browser.sign_in(&world, "/").goes_to(), "/?sign-in=failed");
    let who = browser.who(&world);
    assert_eq!(who.get("failed").and_then(|failed| failed.text_of("why")), Some("unusable"));
    assert_eq!(
        who.get("failed").and_then(|failed| failed.text_of("details")),
        Some("The sign-in service refused Away itself (invalid_client: The client's name or its secret is wrong.).")
    );
    assert_eq!(who.get("signed_in"), Some(&Json::Flag(false)));

    // it sends the browser back with an error that is about the setup
    let world = self::world();
    world.service.sends_back_with(Some("invalid_scope"));
    let mut browser = Browser::default();
    assert_eq!(browser.sign_in(&world, "/").goes_to(), "/?sign-in=failed");
    assert_eq!(
        browser.who(&world).get("failed").and_then(|failed| failed.text_of("details")),
        Some("The sign-in service sent the browser back with an error (invalid_scope: said the stand-in).")
    );
}

#[test]
fn a_sign_in_service_that_changed_its_keys_is_still_believed() {
    let world = world();
    assert_eq!(Browser::default().sign_in(&world, "/").goes_to(), "/");
    world.service.changes_its_key();
    let mut browser = Browser::default();
    assert_eq!(browser.sign_in(&world, "/").goes_to(), "/");
    assert_eq!(browser.who(&world).get("signed_in"), Some(&Json::Flag(true)));
}

#[test]
fn a_sign_in_renews_itself_and_a_right_that_was_taken_away_is_gone_with_the_renewal() {
    let world = world();
    // what the service signs is good for a little over a minute, so it is renewed at once
    world.service.signs_for(61);
    let mut browser = Browser::default();
    browser.sign_in(&world, "/");
    assert_eq!(browser.who(&world).get("administers"), Some(&Json::Flag(true)));
    let name = browser.cookie(&world, SIGNED_IN);
    until("the sign-in is renewed", || world.service.renewed() >= 1);

    // the person is taken out of the administrators' group
    world.service.signs_in(Json::object([
        ("sub", "7c1f".into()),
        ("name", "Alex Urban".into()),
        ("groups", Json::List(vec!["staff".into(), "auditors".into()])),
    ]));
    let renewed = world.service.renewed();
    until("the sign-in is renewed again", || world.service.renewed() > renewed + 1);
    let who = browser.who(&world);
    assert_eq!(who.get("signed_in"), Some(&Json::Flag(true)));
    assert_eq!(who.get("administers"), Some(&Json::Flag(false)));
    assert_eq!(who.get("reads_records"), Some(&Json::Flag(true)));
    assert_eq!(who.get("ran_out"), Some(&Json::Flag(false)));
    assert_eq!(browser.cookie(&world, SIGNED_IN), name, "and nothing shows: the browser's cookie is the same");
}

#[test]
fn a_sign_in_that_the_service_renews_no_longer_has_run_out_and_signing_in_again_keeps_it() {
    let world = world();
    world.service.signs_for(61);
    let mut browser = Browser::default();
    browser.sign_in(&world, "/");
    let name = browser.cookie(&world, SIGNED_IN).expect("the sign-in's name");
    world.service.refuses_renewals(true);
    until("the sign-in has run out", || browser.who(&world).get("ran_out") == Some(&Json::Flag(true)));
    let who = browser.who(&world);
    assert_eq!(who.get("signed_in"), Some(&Json::Flag(true)), "it is still theirs, and wants signing in again");
    assert_eq!(who.text_of("name"), Some("Alex Urban"));
    assert!(world.lines.kept().iter().any(|line| line.contains("\"event\":\"a sign-in ran out\"")));

    // signing in again: the same sign-in, good again
    world.service.refuses_renewals(false);
    world.service.signs_for(300);
    assert_eq!(browser.sign_in(&world, "/sessions").goes_to(), "/sessions");
    assert_eq!(browser.cookie(&world, SIGNED_IN).as_deref(), Some(name.as_str()));
    assert_eq!(browser.who(&world).get("ran_out"), Some(&Json::Flag(false)));

    // somebody else signing in on this browser gets a sign-in of their own
    world.service.signs_in(Json::object([("sub", "another".into()), ("name", "Robin Meyer".into())]));
    assert_eq!(browser.sign_in(&world, "/").goes_to(), "/");
    assert_ne!(browser.cookie(&world, SIGNED_IN).as_deref(), Some(name.as_str()));
    let who = browser.who(&world);
    assert_eq!(who.text_of("name"), Some("Robin Meyer"));
    assert_eq!(who.text_of("initials"), Some("RM"));
    assert_eq!(who.get("address"), Some(&Json::Null));
    assert_eq!(who.get("administers"), Some(&Json::Flag(false)));
}

#[test]
fn signing_out_ends_the_sign_in_and_only_aways_own_page_can_ask_for_it() {
    let world = world();
    let mut browser = Browser::default();
    browser.sign_in(&world, "/");
    let sign_out = format!("{}/sign-out", world.away);

    // another site makes the browser ask: it can't add the header, and the browser says where it is from
    assert_eq!(browser.ask("POST", &sign_out, &[]).status, 403);
    assert_eq!(browser.ask("POST", &sign_out, &[(FROM_AWAYS_PAGE, "1"), ("origin", "https://elsewhere.example.com")]).status, 403);
    assert_eq!(browser.get(&sign_out).status, 405);
    assert_eq!(browser.who(&world).get("signed_in"), Some(&Json::Flag(true)));

    let held = world.service.renewals();
    let signed_out = browser.ask("POST", &sign_out, &[(FROM_AWAYS_PAGE, "1"), ("origin", &world.away)]);
    assert_eq!(signed_out.status, 200);
    assert_eq!(signed_out.sets(), [format!("{SIGNED_IN}=; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=0")]);
    assert_eq!(browser.who(&world).get("signed_in"), Some(&Json::Flag(false)));
    assert_eq!(world.service.forgotten(), held, "the sign-in service is told that the sign-in is over");
    assert!(world.lines.kept().iter().any(|line| line.contains("\"event\":\"signed out\"") && line.contains("\"who\":\"7c1f\"")));

    // the old cookie is worth nothing, whoever kept a copy
    let mut copy = Browser::default();
    copy.cookies.insert(world.away.trim_start_matches("http://").to_string(), vec![(SIGNED_IN.to_string(), held[0].clone())]);
    assert_eq!(copy.who(&world).get("signed_in"), Some(&Json::Flag(false)));
    // signing out twice is nothing
    assert_eq!(browser.ask("POST", &sign_out, &[(FROM_AWAYS_PAGE, "1")]).status, 200);
}

#[test]
fn an_installation_that_is_told_of_no_sign_in_service_says_so() {
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().expect("a runtime");
    let (away, lines, _stopper) = serve(&runtime, free_port(), Told::default());
    let mut browser = Browser::default();
    let who = browser.get(&format!("{away}/api/who")).says();
    assert_eq!(who.get("set_up"), Some(&Json::Flag(false)));
    assert_eq!(who.get("signed_in"), Some(&Json::Flag(false)));
    assert_eq!(who.get("contact"), Some(&Json::Null));
    assert_eq!(browser.get(&format!("{away}/sign-in")).goes_to(), "/");
    assert_eq!(browser.ask("POST", &format!("{away}/sign-out"), &[(FROM_AWAYS_PAGE, "1")]).status, 404);
    assert!(lines.kept().iter().any(|line| line.contains("no sign-in service is set")));
    // and its page is there
    let page = browser.get(&away);
    assert_eq!(page.status, 200);
    assert_eq!(page.header("content-type"), Some("text/html; charset=utf-8"));
    assert_eq!(page.header("cache-control"), Some("no-store"));
    assert_eq!(page.header("x-content-type-options"), Some("nosniff"));
    // what is asked of the server and is none of its addresses is no page
    assert_eq!(browser.get(&format!("{away}/api/none")).status, 404);
    drop(runtime);
}
