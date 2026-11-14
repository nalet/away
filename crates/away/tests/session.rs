//! A whole session, as a browser has it: the server, the gateway, a stand-in for a
//! machine and a real database, each as it runs in an installation. A browser of the
//! tests' own signs in, asks for a machine's desktop, types the account's password when
//! it is asked for it, and takes the picture off the session's connection.
//!
//! No machine is opened without a record, and records are kept in a database, so these
//! tests need one: `AWAY_TEST_DATABASE` names it (`tests/machines/database/real.sh`
//! starts one). Without it they say so and pass.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::time::Duration;

use away_gateway::stand_in::{self, Machine};
use away_gateway::Gateway;
use away_part::address::Address;
use away_part::json::Json;
use away_part::{stop, Lines, Part, Settings, Stopper};
use away_rdp::picture::Picture;
use away_rdp::stand_in::desktop::{typed, CLICKED};
use away_secret::Secret;
use away_server::service;
use away_server::stand_in::SignInService;
use away_server::told::{Database, DatabaseConnection, Internal, SignIn, Told};
use away_server::Server;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

const KNOWN: &str = "CORP\\alex.urban";
const PASSWORD: &str = "a password that exists for these tests alone";
const DATABASE_PASSWORD: &str = "a password for this machine alone";

/// The tests' database, with a database of its own in it for one test.
async fn a_database_of_its_own() -> Option<Database> {
    let Some(at) = std::env::var("AWAY_TEST_DATABASE").ok().filter(|at| !at.is_empty()) else {
        // the pipeline always has one: there, a test that finds none has not tested anything
        assert!(std::env::var_os("CI").is_none(), "the pipeline's tests need a database: AWAY_TEST_DATABASE is not set");
        eprintln!("no database to test against: AWAY_TEST_DATABASE is not set");
        return None;
    };
    let (host, port) = at.rsplit_once(':').expect("AWAY_TEST_DATABASE is host:port");
    let port: u16 = port.parse().expect("a port");
    let mut random = [0u8; 6];
    getrandom::fill(&mut random).expect("random bytes");
    let name = format!("test_{}", random.iter().map(|byte| format!("{byte:02x}")).collect::<String>());
    let database = Database {
        host: host.to_string(),
        port,
        name: "away".to_string(),
        account: "away".to_string(),
        password: Secret::new(DATABASE_PASSWORD),
        connection: DatabaseConnection::Plain,
        authorities: None,
    };
    beside(&database).await.batch_execute(&format!("create database {name}")).await.expect("a database of its own");
    Some(Database { name, ..database })
}

/// A connection of the tests' own to a database, beside the server's.
async fn beside(database: &Database) -> tokio_postgres::Client {
    let mut at = tokio_postgres::Config::new();
    at.host(&database.host).port(database.port).dbname(&database.name).user(&database.account).password(DATABASE_PASSWORD);
    let (client, connection) = at.connect(tokio_postgres::NoTls).await.expect("the tests' database");
    tokio::spawn(connection);
    client
}

/// The records of a database, in their order: each its kind, the person, the machine, and its details.
async fn records(database: &Database) -> Vec<(String, Option<String>, Option<String>, Json)> {
    let rows = beside(database)
        .await
        .query("select kind, person_name, machine, details from record order by number", &[])
        .await
        .unwrap_or_default();
    rows.iter().map(|row| (row.get(0), row.get(1), row.get(2), Json::read(&row.get::<_, String>(3)).expect("details as JSON"))).collect()
}

/// Everything of Away that a session takes, running on this machine.
struct World {
    away: String,
    gateway: String,
    _service: SignInService,
    machine: Machine,
    database: Database,
    server_lines: Lines,
    _stoppers: (Stopper, Stopper),
    _folder: Folder,
}

/// A folder of files for one test, removed when the test ends.
struct Folder(std::path::PathBuf);

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn world(name: &str) -> Option<World> {
    let database = a_database_of_its_own().await?;
    let machine = Machine::start(KNOWN, PASSWORD, stand_in::desktop()).await;
    let (host, port) = machine.address();
    // a port of this machine that nothing listens on, for a machine that is not there
    let nothing = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let folder = Folder(std::env::temp_dir().join(format!("away-session-{}-{name}", std::process::id())));
    let _ = std::fs::remove_dir_all(&folder.0);
    std::fs::create_dir_all(&folder.0).expect("a folder");
    let files = format!(
        "machines:\n  - name: desk-01\n    kind: windows\n    address: {host}\n    desktop: {{ port: {port} }}\n  \
         - name: gone-02\n    kind: windows\n    address: 127.0.0.1\n    desktop: {{ port: {nothing} }}\n  \
         - name: build-01\n    kind: linux\n    address: build-01.corp.example\n    shell: true\n  \
         - name: Desktops\n    kind: windows\n    address: broker.corp.example\n    desktop: true\n    collection: Desktops\n    served-by: [rdsh-01.corp.example]\n  \
         - name: hidden-09\n    kind: windows\n    address: hidden-09.corp.example\n    desktop: true\n\
         rules:\n  - name: staff-desks\n    who: [staff]\n    reaches: [desk-01, gone-02, build-01, Desktops]\n"
    );
    std::fs::write(folder.0.join("machines.yaml"), files).expect("the files");

    let free = || std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let away = format!("http://127.0.0.1:{}", free());
    let back = format!("{away}/sign-in/back");
    let service = SignInService::start("away", "s3cr:t", &back).await;
    let told = Told {
        sign_in: Some(SignIn {
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
        }),
        contact: None,
        files: Some(folder.0.clone()),
        farms: None,
        database: Some(database.clone()),
        internal: Some(Internal { address: "127.0.0.1:0".to_string(), certificate: None }),
    };
    let (server_stopper, stop) = stop::pair();
    let server_lines = Lines::kept_for("server");
    let settings = Settings { address: away.trim_start_matches("http://").to_string(), health_address: "127.0.0.1:0".to_string() };
    let server = Server::bind(&settings, told, Part::new("server", server_lines.clone(), stop)).await.expect("the server binds");
    let internal = server.internal_address().expect("an address for the gateway");
    tokio::spawn(server.run());

    let (gateway_stopper, stop) = stop::pair();
    let told =
        away_gateway::Told { server: Some(Address::read(&format!("http://127.0.0.1:{}", internal.port())).unwrap()), authorities: None };
    let settings = Settings { address: "127.0.0.1:0".to_string(), health_address: "127.0.0.1:0".to_string() };
    let gateway = Gateway::bind(&settings, told, Part::new("gateway", Lines::kept_for("gateway"), stop)).await.expect("the gateway binds");
    let at = format!("127.0.0.1:{}", gateway.bound().address.expect("the sessions' address").port());
    tokio::spawn(gateway.run());

    // the server reaches its database and shapes it before anybody asks for a session
    let reached = async {
        while !server_lines.kept().iter().any(|line| line.contains("the database is reached, and in shape")) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), reached).await.expect("the database is reached");
    Some(World {
        away,
        gateway: at,
        _service: service,
        machine,
        database,
        server_lines,
        _stoppers: (server_stopper, gateway_stopper),
        _folder: folder,
    })
}

/// A browser of the tests' own: it keeps the cookies it is given, and asks as a page of Away's does.
#[derive(Default)]
struct Browser {
    cookies: HashMap<String, Vec<(String, String)>>,
}

impl Browser {
    /// Asks for something, and gives the status, where the answer sends the browser, and what it says.
    fn ask(&mut self, method: &str, address: &str, from_away: bool) -> (u16, Option<String>, String) {
        let (origin, path) = address.strip_prefix("http://").and_then(|rest| rest.split_once('/')).expect("an address with a path");
        let mut request = format!("{method} /{path} HTTP/1.1\r\nhost: {origin}\r\nconnection: close\r\ncontent-length: 0\r\n");
        let kept = self.cookies.entry(origin.to_string()).or_default();
        if !kept.is_empty() {
            let sent: Vec<String> = kept.iter().map(|(name, value)| format!("{name}={value}")).collect();
            request.push_str(&format!("cookie: {}\r\n", sent.join("; ")));
        }
        if from_away {
            request.push_str(&format!("x-away: 1\r\norigin: http://{origin}\r\n"));
        }
        request.push_str("\r\n");
        let mut connection = std::net::TcpStream::connect(origin).expect("a connection");
        connection.set_read_timeout(Some(Duration::from_secs(20))).expect("a time limit");
        connection.write_all(request.as_bytes()).expect("the request goes out");
        let mut answer = String::new();
        connection.read_to_string(&mut answer).expect("an answer comes back");
        let (head, body) = answer.split_once("\r\n\r\n").expect("a head and a body");
        let mut lines = head.split("\r\n");
        let status = lines.next().and_then(|first| first.split(' ').nth(1)).and_then(|status| status.parse().ok()).expect("a status");
        let mut goes_to = None;
        for (name, value) in lines.filter_map(|line| line.split_once(": ")) {
            match name.to_ascii_lowercase().as_str() {
                "location" => goes_to = Some(value.to_string()),
                "set-cookie" => {
                    let (pair, attributes) = value.split_once("; ").unwrap_or((value, ""));
                    let (name, value) = pair.split_once('=').expect("a cookie's name and value");
                    kept.retain(|(its, _)| its != name);
                    if !attributes.contains("Max-Age=0") {
                        kept.push((name.to_string(), value.to_string()));
                    }
                }
                _ => {}
            }
        }
        (status, goes_to, body.to_string())
    }

    /// Signs in the way a person does: is sent to the sign-in service, and comes back.
    fn sign_in(&mut self, world: &World) {
        let (_, to_service, _) = self.ask("GET", &format!("{}/sign-in?to=/", world.away), false);
        let (_, back, _) = self.ask("GET", &to_service.expect("to the sign-in service"), false);
        let (status, _, _) = self.ask("GET", &back.expect("back to Away"), false);
        assert_eq!(status, 303);
    }

    /// The cookie that names this browser's sign-in at Away.
    fn signed_in_as(&self, world: &World) -> String {
        let kept = &self.cookies[world.away.trim_start_matches("http://")];
        kept.iter().find(|(name, _)| name == "__Host-away").map(|(_, value)| value.clone()).expect("a sign-in")
    }

    /// Asks to open a machine's desktop: the status, and what the server says.
    fn open(&mut self, world: &World, machine: &str) -> (u16, String) {
        let (status, _, body) = self.ask("POST", &format!("{}/api/sessions?machine={machine}", world.away), true);
        (status, body)
    }

    /// The same, where it is granted: the ticket.
    fn ticket(&mut self, world: &World, machine: &str) -> String {
        let (status, body) = self.open(world, machine);
        assert_eq!(status, 200, "{body}");
        Json::read(&body).expect("JSON").text_of("ticket").expect("a ticket").to_string()
    }
}

/// A session's connection, as the page holds it.
struct Session {
    connection: WebSocketStream<TcpStream>,
    /// The desktop's picture, as the rectangles that arrived make it.
    picture: Option<Picture>,
}

impl Session {
    /// Opens a session's connection to the gateway, as the browser that is signed in.
    async fn begin(world: &World, sign_in: &str, ticket: &str) -> Session {
        let mut request = format!("ws://{}/session", world.gateway).into_client_request().expect("a request");
        request.headers_mut().insert("cookie", format!("__Host-away={sign_in}").parse().unwrap());
        request.headers_mut().insert("origin", format!("http://{}", world.gateway).parse().unwrap());
        let stream = TcpStream::connect(&world.gateway).await.expect("the gateway takes connections");
        let (mut connection, _) = tokio_tungstenite::client_async(request, stream).await.expect("the connection becomes a session's");
        let begins = Json::object([("ticket", ticket.into()), ("width", 1280u64.into()), ("height", 800u64.into())]);
        connection.send(Message::text(begins.write())).await.expect("the first message goes out");
        Session { connection, picture: None }
    }

    async fn say(&mut self, says: Json) {
        self.connection.send(Message::text(says.write())).await.expect("a message goes out");
    }

    async fn did(&mut self, bytes: &[u8]) {
        self.connection.send(Message::binary(bytes.to_vec())).await.expect("keys and pointer go out");
    }

    /// The next thing the gateway says as text. Rectangles on the way are drawn into the
    /// picture. Nothing once the connection has ended.
    async fn next(&mut self) -> Option<Json> {
        loop {
            let message =
                tokio::time::timeout(Duration::from_secs(40), self.connection.next()).await.expect("the gateway says something in time")?;
            match message.ok()? {
                Message::Text(text) => {
                    let says = Json::read(text.as_str()).expect("JSON");
                    if let Some(desktop) = says.get("desktop") {
                        let size = |name: &str| desktop.get(name).and_then(Json::count).expect("a size") as usize;
                        self.picture = Some(Picture::new(size("width"), size("height")));
                    }
                    return Some(says);
                }
                Message::Binary(bytes) => self.draw(&bytes),
                Message::Close(_) => return None,
                _ => {}
            }
        }
    }

    /// Draws a rectangle that arrived: where it goes, and its pixels as a PNG picture.
    fn draw(&mut self, bytes: &[u8]) {
        assert_eq!((bytes[0], bytes[9]), (1, 1), "a rectangle, as a PNG picture");
        let number = |at: usize| usize::from(u16::from_be_bytes([bytes[at], bytes[at + 1]]));
        let (left, top, width, height) = (number(1), number(3), number(5), number(7));
        let mut reader = png::Decoder::new(std::io::Cursor::new(&bytes[10..])).read_info().expect("a PNG picture");
        let mut pixels = vec![0u8; reader.output_buffer_size().expect("a size")];
        let info = reader.next_frame(&mut pixels).expect("its pixels");
        assert_eq!((info.width as usize, info.height as usize, info.color_type), (width, height, png::ColorType::Rgb));
        let picture = self.picture.as_mut().expect("a desktop before its rectangles");
        assert!(left + width <= picture.width && top + height <= picture.height, "a rectangle lies inside the desktop");
        for row in 0..height {
            for column in 0..width {
                let from = (row * width + column) * 3;
                let to = ((top + row) * picture.width + left + column) * 4;
                // blue, green, red, as the gateway's own picture has them
                picture.pixels[to..to + 3].copy_from_slice(&[pixels[from + 2], pixels[from + 1], pixels[from]]);
            }
        }
    }

    /// Takes what the gateway says until it says this, and gives it.
    async fn until(&mut self, name: &str) -> Json {
        loop {
            let says = self.next().await.unwrap_or_else(|| panic!("the session ended before it said {name}"));
            if says.get(name).is_some() {
                return says;
            }
        }
    }

    /// The steps the gateway says, up to the first thing that is no step.
    async fn steps(&mut self) -> (Vec<String>, Json) {
        let mut steps = Vec::new();
        loop {
            let says = self.next().await.expect("the session goes on");
            match says.text_of("step") {
                Some(step) => steps.push(step.to_string()),
                None => return (steps, says),
            }
        }
    }

    fn pixel(&self, x: usize, y: usize) -> [u8; 3] {
        let picture = self.picture.as_ref().expect("a picture");
        picture.pixels[(y * picture.width + x) * 4..][..3].try_into().unwrap()
    }
}

fn credentials(name: &str, password: &str) -> Json {
    Json::object([("name", name.into()), ("password", password.into())])
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_person_opens_a_desktop_types_the_password_sees_the_machines_picture_and_works_on_it() {
    let Some(world) = world("opens").await else { return };
    let mut browser = Browser::default();
    browser.sign_in(&world);
    let sign_in = browser.signed_in_as(&world);
    let ticket = browser.ticket(&world, "desk-01");

    // the steps, as a person sees them, and then the machine asks who it is for
    let mut session = Session::begin(&world, &sign_in, &ticket).await;
    let (steps, asks) = session.steps().await;
    assert_eq!(steps, ["reaching", "checking", "signing-in"]);
    assert_eq!(asks.write(), r#"{"asks":{"again":false}}"#);
    // a password it doesn't take: asked again, and nothing else happened
    session.say(credentials(KNOWN, "another password")).await;
    assert_eq!(session.next().await.map(|says| says.write()).as_deref(), Some(r#"{"asks":{"again":true}}"#));
    session.say(credentials(KNOWN, PASSWORD)).await;
    let (steps, desktop) = session.steps().await;
    assert_eq!(steps, ["first-picture"]);
    assert_eq!(desktop.write(), r#"{"desktop":{"width":200,"height":120}}"#);

    // the picture becomes the machine's, pixel for pixel: what the machine has drawn so
    // far comes first, and the rest as the machine draws it and the browser confirms
    let shows = stand_in::desktop();
    let mut drawn = 0;
    while session.picture.as_ref() != Some(&shows) {
        let number = session.until("drawn").await.get("drawn").and_then(Json::count).expect("a number");
        assert_eq!(number, drawn + 1, "the batches are numbered one after the other");
        drawn = number;
        session.say(Json::object([("seen", drawn.into())])).await;
    }

    // a click and a key: the machine draws a mark for each, and only what changed arrives
    session.did(&[1, 0, 50, 0, 60, 2, 0, 1, 0, 50, 0, 60, 2, 0, 0, 0, 50, 0, 60]).await;
    session.did(&[4, 0x1e, 0, 1, 4, 0x1e, 0, 0]).await;
    let before = drawn;
    while session.pixel(55, 65) != CLICKED || session.pixel(5, 5) != typed(0x1e) {
        drawn = session.until("drawn").await.get("drawn").and_then(Json::count).expect("a number");
        session.say(Json::object([("seen", drawn.into())])).await;
    }
    assert!(drawn > before);
    assert!(session.picture == world.machine.picture(), "the picture is the machine's, with its marks");
    let seen = world.machine.seen();
    let last = seen.last().expect("a client came as far as the desktop");
    assert_eq!((last.asked, last.account.clone()), ((1280, 800), ("CORP".to_string(), "alex.urban".to_string())));
    assert_eq!(last.input.len(), 5);

    // the person leaves: the session says so and ends, and the machine was told
    session.say(Json::object([("leaves", true.into())])).await;
    assert_eq!(session.until("ended").await.write(), r#"{"ended":"left"}"#);
    assert!(session.next().await.is_none());

    // every step of it is a record: asked for, open, ended, each with who and which machine
    let kept = records(&world.database).await;
    let of_sessions: Vec<_> = kept.iter().filter(|(kind, ..)| kind.starts_with("session-")).collect();
    assert_eq!(
        of_sessions.iter().map(|(kind, ..)| kind.as_str()).collect::<Vec<_>>(),
        ["session-asked", "session-opened", "session-ended"]
    );
    for (_, person, machine, _) in &of_sessions {
        assert_eq!((person.as_deref(), machine.as_deref()), (Some("Alex Urban"), Some("desk-01")));
    }
    assert_eq!(of_sessions[0].3.text_of("way"), Some("desktop"));
    assert_eq!(
        (of_sessions[1].3.get("width").and_then(Json::count), of_sessions[1].3.get("height").and_then(Json::count)),
        (Some(200), Some(120))
    );
    assert_eq!(of_sessions[2].3.text_of("why"), Some("left"));
    // and each is a line on the server's output
    let lines = world.server_lines.kept();
    for kind in ["session-asked", "session-opened", "session-ended"] {
        assert_eq!(
            lines.iter().filter(|line| line.contains("\"event\":\"record\"") && line.contains(&format!("\"kind\":\"{kind}\""))).count(),
            1,
            "{kind}"
        );
    }
    // the password is in no line and in no record
    assert!(!lines.iter().any(|line| line.contains(PASSWORD)));
    assert!(!kept.iter().any(|(.., details)| details.write().contains(PASSWORD)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn only_what_a_rule_gives_is_opened_and_a_ticket_is_good_once_for_the_browser_that_asked() {
    let Some(world) = world("tickets").await else { return };
    let mut browser = Browser::default();
    // nobody who is not signed in, and no page but Away's own
    assert_eq!(browser.open(&world, "desk-01").0, 401);
    browser.sign_in(&world);
    let (status, _, _) = browser.ask("POST", &format!("{}/api/sessions?machine=desk-01", world.away), false);
    assert_eq!(status, 403);
    // a machine no rule gives is not there, and neither is one that doesn't exist
    for machine in ["hidden-09", "nowhere"] {
        assert_eq!(browser.open(&world, machine), (404, "Nothing of that name is there for you.\n".to_string()));
    }
    // what Away doesn't open yet says so: a shell, and the desktop of a farm
    assert_eq!(browser.open(&world, "build-01").0, 501);
    assert_eq!(
        browser.open(&world, "Desktops"),
        (501, "Away doesn't open the desktop of a farm yet: it can't follow a farm's broker so far.\n".to_string())
    );
    assert_eq!(browser.ask("GET", &format!("{}/api/sessions?machine=desk-01", world.away), true).0, 405);

    let sign_in = browser.signed_in_as(&world);
    // a connection that names no ticket, or one that is none, is no session
    let mut session = Session::begin(&world, &sign_in, "no ticket").await;
    assert_eq!(session.next().await.map(|says| says.write()).as_deref(), Some(r#"{"ended":"no-session"}"#));
    // a ticket taken by another browser's sign-in is spent, and gives that browser nothing
    let ticket = browser.ticket(&world, "desk-01");
    let mut other = Browser::default();
    other.sign_in(&world);
    let mut session = Session::begin(&world, &other.signed_in_as(&world), &ticket).await;
    assert_eq!(session.next().await.map(|says| says.write()).as_deref(), Some(r#"{"ended":"no-session"}"#));
    let mut session = Session::begin(&world, &sign_in, &ticket).await;
    assert_eq!(session.next().await.map(|says| says.write()).as_deref(), Some(r#"{"ended":"no-session"}"#));
    // a ticket is good once
    let ticket = browser.ticket(&world, "desk-01");
    let mut session = Session::begin(&world, &sign_in, &ticket).await;
    assert_eq!(session.steps().await.0, ["reaching", "checking", "signing-in"]);
    let mut again = Session::begin(&world, &sign_in, &ticket).await;
    assert_eq!(again.next().await.map(|says| says.write()).as_deref(), Some(r#"{"ended":"no-session"}"#));
    // a person who leaves before typing anything ends it
    session.say(Json::object([("leaves", true.into())])).await;
    assert_eq!(session.until("ended").await.write(), r#"{"ended":"left"}"#);
    assert!(world.machine.seen().is_empty(), "nobody came as far as the desktop");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_machine_that_is_not_there_or_is_another_than_the_one_away_knows_fails_at_its_step_and_is_a_record() {
    let Some(world) = world("fails").await else { return };
    let mut browser = Browser::default();
    browser.sign_in(&world);
    let sign_in = browser.signed_in_as(&world);

    // a machine that is not there
    let mut session = Session::begin(&world, &sign_in, &browser.ticket(&world, "gone-02")).await;
    let (steps, failed) = session.steps().await;
    assert_eq!(steps, ["reaching"]);
    let failed = failed.get("failed").expect("a step that failed").clone();
    assert_eq!((failed.text_of("step"), failed.text_of("why")), (Some("reaching"), Some("refused")));
    assert!(session.next().await.is_none());

    // a machine that Away has seen before, and that shows another identity now
    let mut session = Session::begin(&world, &sign_in, &browser.ticket(&world, "desk-01")).await;
    assert_eq!(session.steps().await.0, ["reaching", "checking", "signing-in"]);
    session.say(Json::object([("leaves", true.into())])).await;
    session.until("ended").await;
    let changed = beside(&world.database).await.execute("update machine_identity set fingerprint = 'another identity'", &[]).await.unwrap();
    assert_eq!(changed, 1, "what the machine showed the first time is kept");
    let mut session = Session::begin(&world, &sign_in, &browser.ticket(&world, "desk-01")).await;
    let (steps, failed) = session.steps().await;
    assert_eq!(steps, ["reaching", "checking"]);
    let failed = failed.get("failed").expect("a step that failed").clone();
    assert_eq!((failed.text_of("step"), failed.text_of("why")), (Some("checking"), Some("another-identity")));

    let kept = records(&world.database).await;
    let kinds: Vec<&str> = kept.iter().map(|(kind, ..)| kind.as_str()).filter(|kind| kind.starts_with("session-")).collect();
    assert_eq!(kinds, ["session-asked", "session-failed", "session-asked", "session-ended", "session-asked", "session-failed"]);
    let last = kept.last().expect("a record");
    assert_eq!(
        (last.3.text_of("step"), last.3.text_of("why"), last.2.as_deref()),
        (Some("checking"), Some("another-identity"), Some("desk-01"))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn without_a_database_no_machine_is_opened() {
    // needs no database: that is the point
    let free = || std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let away = format!("http://127.0.0.1:{}", free());
    let back = format!("{away}/sign-in/back");
    let service = SignInService::start("away", "s3cr:t", &back).await;
    let folder = Folder(std::env::temp_dir().join(format!("away-session-{}-no-database", std::process::id())));
    let _ = std::fs::remove_dir_all(&folder.0);
    std::fs::create_dir_all(&folder.0).expect("a folder");
    let files = "machines:\n  - name: desk-01\n    kind: windows\n    address: desk-01.corp.example\n    desktop: true\nrules:\n  - name: staff-desks\n    who: [staff]\n    reaches: [desk-01]\n";
    std::fs::write(folder.0.join("machines.yaml"), files).expect("the files");
    let told = Told {
        sign_in: Some(SignIn {
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
            administrators: Vec::new(),
            record_readers: Vec::new(),
            looks_every: Duration::from_millis(100),
        }),
        contact: None,
        files: Some(folder.0.clone()),
        farms: None,
        database: None,
        internal: None,
    };
    let (_stopper, stop) = stop::pair();
    let settings = Settings { address: away.trim_start_matches("http://").to_string(), health_address: "127.0.0.1:0".to_string() };
    let server = Server::bind(&settings, told, Part::new("server", Lines::kept_for("server"), stop)).await.expect("the server binds");
    tokio::spawn(server.run());

    let mut browser = Browser::default();
    let (_, to_service, _) = browser.ask("GET", &format!("{away}/sign-in?to=/"), false);
    let (_, back, _) = browser.ask("GET", &to_service.expect("to the sign-in service"), false);
    browser.ask("GET", &back.expect("back to Away"), false);
    let (status, _, body) = browser.ask("POST", &format!("{away}/api/sessions?machine=desk-01"), true);
    assert_eq!((status, body.as_str()), (503, "Away opens no machine here: it is given no database to keep its records in.\n"));
    drop(service);
}
