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
use away_server::stand_in::farm::Farm;
use away_server::stand_in::SignInService;
use away_server::told::{Farms, SignIn, Told};
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

/// A world in which the server is told something else than usual about signing in.
fn world_told(change: impl FnOnce(&mut SignIn)) -> World {
    world_with(|told| change(told.sign_in.as_mut().expect("a sign-in service")))
}

/// A world in which the server is told something else than usual.
fn world_with(change: impl FnOnce(&mut Told)) -> World {
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().expect("a runtime");
    let port = free_port();
    let away = format!("http://127.0.0.1:{port}");
    let back = format!("{away}/sign-in/back");
    let service = runtime.block_on(SignInService::start("away", "s3cr:t", &back));
    let sign_in = SignIn {
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
    let files = Some(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/files").into());
    let mut told = Told { sign_in: Some(sign_in), contact: Some("it-help@example.com".to_string()), files, farms: None };
    change(&mut told);
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

/// The names of the machines an answer lists.
fn names(says: &Json, list: &str) -> Vec<String> {
    let Some(Json::List(listed)) = says.get(list) else { panic!("no {list} in {}", says.write()) };
    listed.iter().map(|one| one.text_of("name").expect("a name").to_string()).collect()
}

#[test]
fn every_person_is_told_the_machines_a_rule_gives_them_and_no_others() {
    let world = world();
    let machines = format!("{}/api/machines", world.away);
    // nobody who is not signed in is told of a machine
    let mut browser = Browser::default();
    assert_eq!(browser.get(&machines).status, 401);
    assert_eq!(browser.ask("POST", &machines, &[]).status, 405);

    // an administrator, who is in staff and in away-admins: what both rules give
    browser.sign_in(&world, "/");
    let told = browser.get(&machines).says();
    // in the order of the files: the engineers' file comes before the finance department's
    assert_eq!(names(&told, "machines"), ["build-01", "build-02", "dev-linux-03", "mac-design-01", "Desktops", "fin-app-02", "hr-app-01"]);
    let Some(Json::List(listed)) = told.get("machines") else { panic!() };
    let fin = listed.iter().find(|machine| machine.text_of("name") == Some("fin-app-02")).expect("fin-app-02");
    // a program with an icon names where the picture is: an address made from the picture itself
    let picture = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/files/icons/ledger.png")).expect("the tests' icon");
    let icon = {
        use sha2::{Digest, Sha256};
        let sum: String = Sha256::digest(&picture).iter().take(16).map(|byte| format!("{byte:02x}")).collect();
        format!("/api/icons/{sum}.png")
    };
    assert_eq!(
        fin.write(),
        format!(
            r#"{{"name":"fin-app-02","kind":"windows","address":"fin-app-02.corp.example","groups":["finance"],"desktop":{{"port":3389,"over":"rdp"}},"programs":[{{"name":"Ledger","icon":"{icon}"}},{{"name":"Payroll","icon":null}},{{"name":"Reports","icon":null}},{{"name":"Mail","icon":null}}],"shell":null,"collection":null}}"#
        )
    );
    // the picture itself: for those who are signed in, as the picture it is, to be kept
    let head = browser.ask("HEAD", &format!("{}{icon}", world.away), &[]);
    assert_eq!(head.status, 200);
    assert_eq!(head.header("content-type"), Some("image/png"));
    assert_eq!(head.header("content-length"), Some(picture.len().to_string().as_str()));
    assert_eq!(head.header("cache-control"), Some("private, max-age=31536000, immutable"));
    assert_eq!(browser.get(&format!("{}/api/icons/0000.png", world.away)).status, 404);
    assert_eq!(Browser::default().get(&format!("{}{icon}", world.away)).status, 401);
    assert!(
        !told.write().contains("yaml") && !told.write().contains("rdsh-01"),
        "a person is not told of files, nor of the machines behind a collection"
    );

    // someone who is in staff alone: what the one rule gives, and nothing of engineering
    world.service.signs_in(away_server::stand_in::person());
    let mut other = Browser::default();
    other.sign_in(&world, "/");
    assert_eq!(names(&other.get(&machines).says(), "machines"), ["Desktops", "fin-app-02", "hr-app-01"]);

    // someone the sign-in service says no group of: nothing
    world.service.signs_in(Json::object([("sub", "another".into()), ("name", "Sam Okoro".into())]));
    let mut third = Browser::default();
    third.sign_in(&world, "/");
    assert_eq!(names(&third.get(&machines).says(), "machines"), Vec::<String>::new());
}

#[test]
fn those_who_administer_see_what_is_in_force_and_where_it_is_written() {
    let world = world();
    let register = format!("{}/api/register", world.away);
    let mut browser = Browser::default();
    assert_eq!(browser.get(&register).status, 401);
    browser.sign_in(&world, "/");
    let says = browser.get(&register).says();
    assert_eq!(says.get("has_files"), Some(&Json::Flag(true)));
    assert_eq!(says.get("refused"), Some(&Json::Null));
    assert!(says.get("since").and_then(Json::count).is_some_and(|since| since > 1_700_000_000));
    assert_eq!(names(&says, "machines").len(), 7);
    assert_eq!(names(&says, "rules"), ["staff-finance", "admins-engineering"]);
    let Some(Json::List(machines)) = says.get("machines") else { panic!() };
    let farm = machines.iter().find(|machine| machine.text_of("name") == Some("Desktops")).expect("the farm");
    assert_eq!(farm.text_of("file"), Some("machines/engineering.yaml"));
    assert_eq!(farm.get("line").and_then(Json::count), Some(30));
    assert_eq!(farm.get("given_by").map(Json::write).as_deref(), Some(r#"[{"rule":"staff-finance","who":["staff"]}]"#));
    assert_eq!(farm.get("served_by").map(Json::write).as_deref(), Some(r#"["rdsh-01.corp.example","rdsh-02.corp.example"]"#));
    let Some(Json::List(rules)) = says.get("rules") else { panic!() };
    assert_eq!(
        rules[0].write(),
        r#"{"name":"staff-finance","who":["staff"],"reaches":["finance","Desktops"],"file":"rules/access.yaml","line":4}"#
    );

    // someone who administers nothing is told nothing of it
    world.service.signs_in(away_server::stand_in::person());
    let mut other = Browser::default();
    other.sign_in(&world, "/");
    let refused = other.get(&register);
    assert_eq!((refused.status, refused.body.as_str()), (403, "This is for those who administer Away.\n"));
}

/// A folder of files for one test, removed when the test ends.
struct Folder(std::path::PathBuf);

impl Folder {
    fn with(name: &str, files: &[(&str, &str)]) -> Folder {
        let path = std::env::temp_dir().join(format!("away-farms-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("a folder");
        for (file, text) in files {
            std::fs::write(path.join(file), text).expect("a file");
        }
        Folder(path)
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A PNG picture for the tests, told apart by its last byte.
fn picture(last: u8) -> Vec<u8> {
    [&b"\x89PNG\r\n\x1a\nthe rest of a picture "[..], &[last]].concat()
}

/// A farm's web access beside the server: it takes one account, and publishes two
/// programs and a desktop.
fn farm() -> (tokio::runtime::Runtime, Farm) {
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().expect("a runtime");
    let farm = runtime.block_on(Farm::start("CORP\\svc-away", "the password of the account that reads the feed"));
    farm.publishes("Ledger", false, "Finance", Some(&picture(1)));
    farm.publishes("Pay & Benefits", false, "Finance", None);
    farm.publishes("Desktop", true, "Desktops", Some(&picture(2)));
    (runtime, farm)
}

/// What the server is told to read the farms' feeds with.
fn reads_farms(password: &str) -> Option<Farms> {
    Some(Farms {
        account: "CORP\\svc-away".to_string(),
        password: Secret::new(password),
        authorities: None,
        every: Duration::from_millis(200),
    })
}

/// The files of a farm, and of a rule that gives it to everyone of staff.
fn farm_files(feed: &str) -> String {
    format!("farms:\n  - name: Work Resources\n    feed: {feed}\n    groups: [published]\nrules:\n  - name: staff-published\n    who: [staff]\n    reaches: [published]\n")
}

/// What the administration is told of the first farm.
fn first_farm(browser: &mut Browser, world: &World) -> Json {
    let says = browser.get(&format!("{}/api/register", world.away)).says();
    let Some(Json::List(farms)) = says.get("farms") else { panic!("no farms in {}", says.write()) };
    farms.first().expect("a farm").clone()
}

/// The programs of a machine, by their names, in what a person is told.
fn programs_of(told: &Json, machine: &str) -> Vec<String> {
    let Some(Json::List(machines)) = told.get("machines") else { panic!("no machines") };
    let Some(Json::List(programs)) = machines.iter().find(|one| one.text_of("name") == Some(machine)).and_then(|one| one.get("programs"))
    else {
        return Vec::new();
    };
    programs.iter().map(|program| program.text_of("name").expect("a name").to_string()).collect()
}

#[test]
fn a_farm_says_what_it_publishes_and_away_shows_it_as_it_changes() {
    let (_runtime, farm) = farm();
    let folder = Folder::with("changes", &[("farm.yaml", &farm_files(&farm.feed()))]);
    let world = world_with(|told| {
        told.files = Some(folder.0.clone());
        told.farms = reads_farms("the password of the account that reads the feed");
    });
    let machines = format!("{}/api/machines", world.away);
    let mut browser = Browser::default();
    browser.sign_in(&world, "/");

    // the farm's desktop is a machine, and its programs are those of one machine with the farm's name
    until("the farm's feed is read", || names(&browser.get(&machines).says(), "machines") == ["Desktop", "Work Resources"]);
    let told = browser.get(&machines).says();
    assert_eq!(programs_of(&told, "Work Resources"), ["Ledger", "Pay & Benefits"]);
    let Some(Json::List(listed)) = told.get("machines") else { panic!() };
    assert_eq!(
        listed[0].write(),
        r#"{"name":"Desktop","kind":"windows","address":"broker.corp.example","groups":["published"],"desktop":{"port":3389,"over":"rdp"},"programs":[],"shell":null,"collection":"Desktops"}"#
    );
    assert_eq!(listed[1].text_of("collection"), Some("Finance"));
    // a program's icon is the farm's own, served by Away
    let Some(Json::List(programs)) = listed[1].get("programs") else { panic!() };
    let icon = programs[0].text_of("icon").expect("Ledger has an icon").to_string();
    assert_eq!(programs[1].get("icon"), Some(&Json::Null));
    let head = browser.ask("HEAD", &format!("{}{icon}", world.away), &[]);
    assert_eq!((head.status, head.header("content-type"), head.header("content-length")), (200, Some("image/png"), Some("31")));

    // those who administer see the farm, and how its feed was read
    let state = first_farm(&mut browser, &world);
    assert_eq!(
        (state.text_of("name"), state.text_of("file"), state.get("line").and_then(Json::count)),
        (Some("Work Resources"), Some("farm.yaml"), Some(2))
    );
    assert_eq!((state.get("desktops").and_then(Json::count), state.get("programs").and_then(Json::count)), (Some(1), Some(2)));
    assert_eq!((state.get("failed"), state.get("left_out").map(Json::write).as_deref()), (Some(&Json::Null), Some("[]")));
    assert!(state.get("read_at").and_then(Json::count).is_some_and(|at| at > 1_700_000_000));

    // the farm publishes a program more: it is there without anybody touching a file
    farm.publishes("Reports", false, "Finance", Some(&picture(3)));
    until("the new program shows", || {
        programs_of(&browser.get(&machines).says(), "Work Resources") == ["Ledger", "Pay & Benefits", "Reports"]
    });
    // and one it publishes no longer is gone
    farm.withdraws("Pay & Benefits");
    until("the withdrawn program is gone", || programs_of(&browser.get(&machines).says(), "Work Resources") == ["Ledger", "Reports"]);
    // what did not change was not asked for again: three things were taken at first, and one more since
    let (feeds, pieces) = farm.gave();
    assert!(feeds >= 3, "{feeds} feeds");
    assert_eq!(pieces, 5 + 2, "two files and two icons for the programs, a file and an icon for the desktop less the icon it has none of; then the new program's two");

    // the farm doesn't answer: what it published stays, and the administration says why
    farm.answers(false);
    until("the failure shows", || first_farm(&mut browser, &world).get("failed") != Some(&Json::Null));
    assert_eq!(first_farm(&mut browser, &world).text_of("failed"), Some("The farm's feed answered with the status 503."));
    assert_eq!(programs_of(&browser.get(&machines).says(), "Work Resources"), ["Ledger", "Reports"]);
    farm.answers(true);
    until("the feed is read again", || first_farm(&mut browser, &world).get("failed") == Some(&Json::Null));

    // each outcome is a line of the server's
    let said = world.lines.kept().join("\n");
    assert!(said.contains(r#""event":"a farm's feed was read","farm":"Work Resources","desktops":1,"programs":2"#), "{said}");
    assert!(said.contains(r#""event":"a farm's feed could not be read, and what it published before stays","farm":"Work Resources","why":"The farm's feed answered with the status 503.""#), "{said}");
    assert!(!said.contains("the password of the account"), "the account's password is in no line");
}

#[test]
fn a_farm_that_does_not_take_the_account_or_is_given_none_shows_nothing_and_says_why() {
    let (_runtime, farm) = farm();
    let folder = Folder::with("refused", &[("farm.yaml", &farm_files(&farm.feed()))]);
    // a wrong password
    let world = world_with(|told| {
        told.files = Some(folder.0.clone());
        told.farms = reads_farms("another password");
    });
    let mut browser = Browser::default();
    browser.sign_in(&world, "/");
    until("the refusal shows", || first_farm(&mut browser, &world).get("failed") != Some(&Json::Null));
    assert_eq!(
        first_farm(&mut browser, &world).text_of("failed"),
        Some("The farm doesn't take the account Away reads its feed with: its name or its password is wrong, or it may not read the feed.")
    );
    assert_eq!(names(&browser.get(&format!("{}/api/machines", world.away)).says(), "machines"), Vec::<String>::new());

    // no account at all
    let world = world_with(|told| told.files = Some(folder.0.clone()));
    let mut browser = Browser::default();
    browser.sign_in(&world, "/");
    until("the lack of an account shows", || first_farm(&mut browser, &world).get("failed") != Some(&Json::Null));
    assert_eq!(
        first_farm(&mut browser, &world).text_of("failed"),
        Some("Away is given no account to read a farm's feed with: the installation guide says how to give it one.")
    );
}

#[test]
fn a_machine_of_the_files_keeps_its_name_against_what_a_farm_publishes() {
    let (_runtime, farm) = farm();
    let files = format!(
        "{}machines:\n  - name: Desktop\n    kind: linux\n    address: desk.corp.example\n    groups: [published]\n    shell: true\n",
        farm_files(&farm.feed())
    );
    let folder = Folder::with("names", &[("farm.yaml", &files)]);
    let world = world_with(|told| {
        told.files = Some(folder.0.clone());
        told.farms = reads_farms("the password of the account that reads the feed");
    });
    let mut browser = Browser::default();
    browser.sign_in(&world, "/");
    let machines = format!("{}/api/machines", world.away);
    // what the farm publishes comes where the farm is written: before the machine under it
    until("the farm's feed is read", || names(&browser.get(&machines).says(), "machines") == ["Work Resources", "Desktop"]);
    let told = browser.get(&machines).says();
    let Some(Json::List(listed)) = told.get("machines") else { panic!() };
    assert_eq!((listed[1].text_of("kind"), listed[1].text_of("address")), (Some("linux"), Some("desk.corp.example")));
    assert_eq!(first_farm(&mut browser, &world).get("left_out").map(Json::write).as_deref(), Some(r#"["Desktop"]"#));
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
