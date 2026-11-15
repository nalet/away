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
use away_rdp::redirection::Redirection;
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
    /// Where the server answers Away's other parts.
    internal: std::net::SocketAddr,
    _service: SignInService,
    machine: Machine,
    /// A farm's broker, which sends whoever signs in on to the machine.
    broker: Machine,
    /// The broker of a farm of one machine, which sends whoever signs in on to itself.
    alone: Machine,
    /// A broker that sends whoever signs in on to a machine that is nowhere in the files.
    _lost: Machine,
    database: Database,
    server_lines: Lines,
    /// What signing in as oneself takes, where the world has a domain.
    domain: Option<OfADomain>,
    _stoppers: (Stopper, Stopper),
    _folder: Folder,
}

/// A domain's part of a world: a machine of the domain, which takes tickets and reads
/// the card it is handed; the organisation's authority; and Away's issuer.
struct OfADomain {
    machine: Machine,
    pages: away_issuer::authority::stand_in::Pages,
    issuer_lines: Lines,
    _stopper: Stopper,
}

const AUTHORITY: &[u8] = include_bytes!("../../away-winauth/tests/fixtures/authority.der");

/// The Kerberos server of the tests, where one is named.
fn kerberos() -> Option<String> {
    let at = std::env::var("AWAY_TEST_KERBEROS").ok().filter(|at| !at.is_empty());
    if at.is_none() {
        assert!(std::env::var_os("CI").is_none(), "the pipeline's tests need a Kerberos server: AWAY_TEST_KERBEROS is not set");
        eprintln!("no Kerberos server to test against: AWAY_TEST_KERBEROS is not set");
    }
    at
}

/// A folder of files for one test, removed when the test ends.
struct Folder(std::path::PathBuf);

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn world(name: &str) -> Option<World> {
    world_with(name, None).await
}

/// The same, with a domain whose Kerberos server is at this address: one machine more,
/// `own-01`, on which people are signed in as themselves.
async fn world_with(name: &str, kerberos: Option<&str>) -> Option<World> {
    use away_winauth::kerberos::crypto::{Key, Kind};
    let database = a_database_of_its_own().await?;
    let password_of = |what: &str, salt: &[u8]| Key::from_password(Kind::Aes256, what, salt, 4096);
    let own = match kerberos {
        Some(_) => Some(
            Machine::start_in_a_domain(
                password_of("the password of a machine that exists for these tests alone", b"CORP.EXAMPLETERMSRVlocalhost"),
                stand_in::desktop(),
            )
            .await,
        ),
        None => None,
    };
    let machine = Machine::start(KNOWN, PASSWORD, stand_in::desktop()).await;
    let (host, port) = machine.address();
    // a farm's broker beside it: it sends whoever signs in on to the machine, by its name and its number
    let sends_on = Redirection {
        session: 7,
        address: Some("127.0.0.1".to_string()),
        name: Some(host.clone()),
        token: Some(b"Cookie: msts=285216778.15629.0000\r\n".to_vec()),
        ..Redirection::default()
    };
    let broker = Machine::start_broker(KNOWN, PASSWORD, sends_on).await;
    let (broker_host, broker_port) = broker.address();
    // one that sends whoever signs in on to a machine that is nowhere in the files
    let astray = Redirection {
        session: 8,
        address: Some("10.20.0.7".to_string()),
        name: Some("rdsh-07.corp.example".to_string()),
        ..Redirection::default()
    };
    let lost = Machine::start_broker(KNOWN, PASSWORD, astray).await;
    let (_, lost_port) = lost.address();
    // and a farm of one machine, whose broker serves its collection itself: it sends
    // whoever signs in on to itself
    let alone = Machine::start_broker(
        KNOWN,
        PASSWORD,
        Redirection { session: 9, address: Some("127.0.0.1".to_string()), ..Redirection::default() },
    )
    .await;
    let (_, alone_port) = alone.address();
    // a port of this machine that nothing listens on, for a machine that is not there
    let nothing = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let folder = Folder(std::env::temp_dir().join(format!("away-session-{}-{name}", std::process::id())));
    let _ = std::fs::remove_dir_all(&folder.0);
    std::fs::create_dir_all(&folder.0).expect("a folder");
    let files = format!(
        "machines:\n  - name: desk-01\n    kind: windows\n    address: {host}\n    desktop: {{ port: {port} }}\n  \
         - name: apps-01\n    kind: windows\n    address: {host}\n    desktop: {{ port: {port} }}\n    programs:\n      \
         - name: Ledger\n        run: \"||ledger\"\n      - Payroll\n  \
         - name: gone-02\n    kind: windows\n    address: 127.0.0.1\n    desktop: {{ port: {nothing} }}\n  \
         - name: build-01\n    kind: linux\n    address: build-01.corp.example\n    shell: true\n  \
         - name: Desktops\n    kind: windows\n    address: {broker_host}\n    desktop: {{ port: {broker_port} }}\n    collection: Desktops\n    \
         served-by: [127.0.0.9, {host}:{port}]\n  \
         - name: Elsewhere\n    kind: windows\n    address: {broker_host}\n    desktop: {{ port: {lost_port} }}\n    collection: Elsewhere\n    \
         served-by: [127.0.0.9, \"127.0.0.8:{port}\"]\n  \
         - name: Unnamed\n    kind: windows\n    address: {broker_host}\n    desktop: {{ port: {lost_port} }}\n    collection: Unnamed\n  \
         - name: Alone\n    kind: windows\n    address: {broker_host}\n    desktop: {{ port: {alone_port} }}\n    collection: Alone\n  \
         - name: Anywhere\n    kind: windows\n    address: {broker_host}\n    desktop: {{ port: {port} }}\n    collection: Anywhere\n  \
         - name: hidden-09\n    kind: windows\n    address: hidden-09.corp.example\n    desktop: true\n\
         rules:\n  - name: staff-desks\n    who: [staff]\n    reaches: [desk-01, apps-01, gone-02, build-01, Desktops, Elsewhere, Unnamed, Alone, Anywhere]\n"
    );
    // the machine of the domain, where the world has one, in a file of its own: people
    // are signed in on it as themselves
    if let Some(own) = &own {
        let (host, port) = own.address();
        let of_the_domain = format!(
            "machines:\n  - name: own-01\n    kind: windows\n    address: {host}\n    desktop: {{ port: {port} }}\n    signs-in: themselves\n\
             rules:\n  - name: own\n    who: [staff]\n    reaches: [own-01]\n"
        );
        std::fs::write(folder.0.join("own.yaml"), of_the_domain).expect("the domain's file");
    }
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

    // the issuer, and what it rests on, where the world has a domain: the organisation's
    // authority, the sign-in service, the server, and the domain's Kerberos server
    let authority = {
        let letters = away_part::base64::ordinary(AUTHORITY);
        let lines: Vec<&str> = letters.as_bytes().chunks(64).map(|line| std::str::from_utf8(line).unwrap()).collect();
        format!("-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n", lines.join("\n"))
    };
    let (domain, in_a_domain) = match (kerberos, own) {
        (Some(kerberos), Some(own)) => {
            use away_issuer::authority::stand_in::{Pages, With};
            let fixture =
                |name: &str| std::fs::read(format!("{}/../away-winauth/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap();
            let pages = Pages::open(With {
                shows: (fixture("controller.der"), fixture("controller.key.der")),
                of_the_service: password_of("the password of pages that exist for these tests alone", b"CORP.EXAMPLEHTTPlocalhost"),
                issues_with: (fixture("issuing.der"), fixture("issuing.key.der")),
                asked_as: "pat.typed".to_string(),
                domain: "CORP".to_string(),
                realm: "CORP.EXAMPLE".to_string(),
                for_signing_in: "CorpSignIn".to_string(),
                for_an_agent: "CorpAgent".to_string(),
                never_for: Vec::new(),
                good_for: 300,
            })
            .await;
            let told = away_issuer::Told {
                internal: Some(away_issuer::told::Internal { address: "127.0.0.1:0".to_string(), certificate: None }),
                windows: Some(away_issuer::told::Windows {
                    domain: "corp.example".to_string(),
                    short_name: "CORP".to_string(),
                    controllers: vec![kerberos.to_string()],
                    pages: pages.address.clone(),
                    authorities: Some(authority.clone().into_bytes()),
                    account: "pat.typed".to_string(),
                    password: Secret::new(PASSWORD),
                    agent_kind: "CorpAgent".to_string(),
                    sign_in_kind: "CorpSignIn".to_string(),
                    account_entry: away_issuer::told::ACCOUNT_ENTRY.to_string(),
                    groups_entry: away_issuer::told::GROUPS_ENTRY.to_string(),
                    never_as: away_issuer::told::NEVER_AS.iter().map(|never| never.to_string()).collect(),
                    never_of: away_issuer::told::NEVER_OF.iter().map(|never| never.to_string()).collect(),
                }),
                sign_in: Some(away_issuer::told::SignInService {
                    service: service.address(),
                    client: "away".to_string(),
                    authorities: None,
                }),
                server: Some(away_issuer::told::Server {
                    at: Address::read(&format!("http://127.0.0.1:{}", internal.port())).unwrap(),
                    authorities: None,
                }),
            };
            let (stopper, stop) = stop::pair();
            let issuer_lines = Lines::kept_for("issuer");
            let settings = Settings { address: String::new(), health_address: "127.0.0.1:0".to_string() };
            let issuer = away_issuer::Issuer::bind(&settings, told, Part::new("issuer", issuer_lines.clone(), stop))
                .await
                .expect("the issuer binds");
            let at =
                Address::read(&format!("http://127.0.0.1:{}", issuer.bound().address.expect("where the gateway asks").port())).unwrap();
            tokio::spawn(issuer.run());
            let told = away_gateway::InADomain {
                name: "corp.example".to_string(),
                controllers: vec![kerberos.to_string()],
                authorities: authority.into_bytes(),
                issuer: at,
                issuer_authorities: None,
            };
            (Some(OfADomain { machine: own, pages, issuer_lines, _stopper: stopper }), Some(told))
        }
        _ => (None, None),
    };

    let (gateway_stopper, stop) = stop::pair();
    let told = away_gateway::Told {
        server: Some(Address::read(&format!("http://127.0.0.1:{}", internal.port())).unwrap()),
        authorities: None,
        domain: in_a_domain,
    };
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
        internal,
        _service: service,
        machine,
        broker,
        alone,
        _lost: lost,
        database,
        server_lines,
        domain,
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

    /// Asks to open a program of a machine, and gives the ticket.
    fn ticket_for_a_program(&mut self, world: &World, machine: &str, program: &str) -> String {
        self.ticket(world, &format!("{machine}&program={program}"))
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

/// What the server answers when it is asked to answer for a session.
async fn vouched(world: &World, session: &str) -> (u16, String) {
    let at = Address::read(&format!("http://127.0.0.1:{}/internal/vouch", world.internal.port())).expect("the server's own address");
    let got =
        away_part::client::Client::new(None).expect("a client").send_form(&at, &[("session", session)], None).await.expect("an answer");
    (got.status, String::from_utf8_lossy(&got.body).to_string())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_server_answers_for_a_session_that_runs_with_what_the_sign_in_service_signs_at_that_moment() {
    let Some(world) = world("answers-for").await else { return };
    let mut browser = Browser::default();
    browser.sign_in(&world);
    let sign_in = browser.signed_in_as(&world);
    let ticket = browser.ticket(&world, "desk-01");
    // the session's name is the key of the record that it was asked for
    let named: String = beside(&world.database)
        .await
        .query_one("select key from record where kind = 'session-asked'", &[])
        .await
        .expect("the record")
        .get(0);
    // a session nobody took over yet doesn't run
    assert_eq!(vouched(&world, &named).await.0, 404);
    let mut session = Session::begin(&world, &sign_in, &ticket).await;
    session.steps().await;

    // the server renews the person's sign-in for it, and hands over what the service signed just now
    let renewed = world._service.renewed();
    let (status, said) = vouched(&world, &named).await;
    assert_eq!(status, 200, "{said}");
    let said = Json::read(&said).expect("JSON");
    assert_eq!(
        (said.text_of("session"), said.text_of("subject"), said.text_of("machine")),
        (Some(named.as_str()), Some("7c1f"), Some("desk-01"))
    );
    assert_eq!(world._service.renewed(), renewed + 1);
    let token = said.text_of("token").expect("what the service signed");
    let says = token.split('.').nth(1).and_then(away_part::base64::read_for_addresses).expect("a token of three parts");
    let says = Json::read(std::str::from_utf8(&says).expect("text")).expect("JSON");
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    assert_eq!(says.text_of("sub"), Some("7c1f"));
    assert!(says.get("iat").and_then(Json::count).is_some_and(|signed_at| signed_at + 5 >= now), "{}", says.write());

    // no session of another name, and nothing once the sign-in service takes the person's sign-in no more
    assert_eq!(vouched(&world, "another").await.0, 404);
    world._service.refuses_renewals(true);
    assert_eq!(vouched(&world, &named).await, (403, "The person's sign-in has run out.\n".to_string()));
    world._service.refuses_renewals(false);
    assert_eq!(vouched(&world, &named).await.0, 403, "a sign-in that ran out stays run out");
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
    // what Away doesn't open yet says so: a shell
    assert_eq!(
        browser.open(&world, "build-01"),
        (501, "Away opens Windows desktops and programs so far, and this machine offers neither.\n".to_string())
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
async fn the_desktop_of_a_farm_opens_on_the_machine_its_broker_names_where_the_files_name_that_machine() {
    let Some(world) = world("farm").await else { return };
    let mut browser = Browser::default();
    browser.sign_in(&world);
    let sign_in = browser.signed_in_as(&world);
    let (host, port) = world.machine.address();

    // the same steps as for one machine: the broker is reached, checked, and asks who it is for
    let mut session = Session::begin(&world, &sign_in, &browser.ticket(&world, "Desktops")).await;
    let (steps, asks) = session.steps().await;
    assert_eq!(steps, ["reaching", "checking", "signing-in"]);
    assert_eq!(asks.write(), r#"{"asks":{"again":false}}"#);
    // a password the broker doesn't take: asked again, and no machine was gone to
    session.say(credentials(KNOWN, "another password")).await;
    assert_eq!(session.next().await.map(|says| says.write()).as_deref(), Some(r#"{"asks":{"again":true}}"#));
    assert!(world.machine.said_first().is_empty(), "nobody went on to the machine");
    // the right one: the broker says where the connection belongs, and the desktop is the machine's
    session.say(credentials(KNOWN, PASSWORD)).await;
    let (steps, desktop) = session.steps().await;
    assert_eq!(steps, ["first-picture"]);
    assert_eq!(desktop.write(), r#"{"desktop":{"width":200,"height":120}}"#);
    let shows = stand_in::desktop();
    while session.picture.as_ref() != Some(&shows) {
        let drawn = session.until("drawn").await.get("drawn").and_then(Json::count).expect("a number");
        session.say(Json::object([("seen", drawn.into())])).await;
    }

    // the broker was told the collection each time, and the machine what the broker gave
    // for it and the session the broker named
    let wanted = b"tsv://MS Terminal Services Plugin.1.Desktops".to_vec();
    let told = world.broker.said_first();
    assert!(!told.is_empty() && told.iter().all(|said| said.as_ref() == Some(&wanted)), "{told:?}");
    assert_eq!(world.machine.said_first(), [Some(b"Cookie: msts=285216778.15629.0000".to_vec())]);
    let seen = world.machine.seen();
    assert_eq!(seen.len(), 1);
    assert_eq!((seen[0].sent_on_to, seen[0].account.clone()), (Some(7), ("CORP".to_string(), "alex.urban".to_string())));

    session.say(Json::object([("leaves", true.into())])).await;
    assert_eq!(session.until("ended").await.write(), r#"{"ended":"left"}"#);

    // the record says which machine served the session, and Away knows both by what they showed
    let kept = records(&world.database).await;
    let of_sessions: Vec<_> = kept.iter().filter(|(kind, ..)| kind.starts_with("session-")).collect();
    assert_eq!(
        of_sessions.iter().map(|(kind, ..)| kind.as_str()).collect::<Vec<_>>(),
        ["session-asked", "session-opened", "session-ended"]
    );
    assert!(of_sessions.iter().all(|(_, _, machine, _)| machine.as_deref() == Some("Desktops")));
    assert_eq!(of_sessions[1].3.text_of("served_by"), Some(format!("{host}:{port}").as_str()));
    let known = beside(&world.database).await.query("select machine from machine_identity order by machine", &[]).await.unwrap();
    let mut known: Vec<String> = known.iter().map(|row| row.get(0)).collect();
    let (broker_host, broker_port) = world.broker.address();
    let mut both = vec![format!("{broker_host}:{broker_port}"), format!("{host}:{port}")];
    known.sort();
    both.sort();
    assert_eq!(known, both);

    // a collection whose machines the files name, and the broker names another: Away goes nowhere
    let mut session = Session::begin(&world, &sign_in, &browser.ticket(&world, "Elsewhere")).await;
    assert_eq!(session.steps().await.0, ["reaching", "checking", "signing-in"]);
    session.say(credentials(KNOWN, PASSWORD)).await;
    let failed = session.until("failed").await.get("failed").expect("a step that failed").clone();
    assert_eq!((failed.text_of("step"), failed.text_of("why")), (Some("signing-in"), Some("sent-elsewhere")));
    assert!(session.next().await.is_none());
    assert_eq!(world.machine.seen().len(), 1, "nobody else came to the machine");
    let last = records(&world.database).await.pop().expect("a record");
    assert_eq!(
        (last.0.as_str(), last.2.as_deref(), last.3.text_of("step"), last.3.text_of("why")),
        ("session-failed", Some("Elsewhere"), Some("signing-in"), Some("sent-elsewhere"))
    );
    assert_eq!(
        last.3.text_of("details"),
        Some("the farm's broker sends the connection to rdsh-07.corp.example, which the files don't name for the collection")
    );

    // a collection whose machines the files don't name at all: a broker's word alone leads nowhere
    let mut session = Session::begin(&world, &sign_in, &browser.ticket(&world, "Unnamed")).await;
    assert_eq!(session.steps().await.0, ["reaching", "checking", "signing-in"]);
    session.say(credentials(KNOWN, PASSWORD)).await;
    let failed = session.until("failed").await.get("failed").expect("a step that failed").clone();
    assert_eq!((failed.text_of("step"), failed.text_of("why")), (Some("signing-in"), Some("sent-elsewhere")));
    assert!(failed.text_of("details").is_some_and(|details| details.ends_with("the files name no machine that serves the collection")));
    assert_eq!(world.machine.seen().len(), 1, "nobody else came to the machine");

    // a broker that serves its collection itself sends the connection on to the address the
    // files name for the entry: Away comes again as it was told to, and the desktop opens
    let mut session = Session::begin(&world, &sign_in, &browser.ticket(&world, "Alone")).await;
    assert_eq!(session.steps().await.0, ["reaching", "checking", "signing-in"]);
    session.say(credentials(KNOWN, PASSWORD)).await;
    let (steps, desktop) = session.steps().await;
    assert_eq!((steps, desktop.get("desktop").is_some()), (vec!["first-picture".to_string()], true));
    let came: Vec<Option<u32>> = world.alone.seen().iter().map(|seen| seen.sent_on_to).collect();
    assert_eq!(came, [None, Some(9)]);
    session.say(Json::object([("leaves", true.into())])).await;
    session.until("ended").await;

    // a collection that is written at a machine which is no broker: its desktop opens as any machine's
    let mut session = Session::begin(&world, &sign_in, &browser.ticket(&world, "Anywhere")).await;
    assert_eq!(session.steps().await.0, ["reaching", "checking", "signing-in"]);
    session.say(credentials(KNOWN, PASSWORD)).await;
    let (steps, desktop) = session.steps().await;
    assert_eq!((steps, desktop.get("desktop").is_some()), (vec!["first-picture".to_string()], true));
    let seen = world.machine.seen();
    assert_eq!((seen.len(), seen[1].sent_on_to), (2, None));
    assert_eq!(world.machine.said_first().last(), Some(&Some(b"tsv://MS Terminal Services Plugin.1.Anywhere".to_vec())));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_program_opens_in_a_window_of_its_own_and_closing_it_ends_the_session() {
    let Some(world) = world("program").await else { return };
    let mut browser = Browser::default();
    browser.sign_in(&world);
    let sign_in = browser.signed_in_as(&world);
    // a program the machine doesn't have, for whoever asks, is not there
    assert_eq!(browser.open(&world, "apps-01&program=Nothing"), (404, "Nothing of that name is there for you.\n".to_string()));

    // the same steps as for a desktop, and then the program's window in the place of one
    let mut session = Session::begin(&world, &sign_in, &browser.ticket_for_a_program(&world, "apps-01", "Ledger")).await;
    assert_eq!(session.steps().await.0, ["reaching", "checking", "signing-in"]);
    session.say(credentials(KNOWN, PASSWORD)).await;
    let (steps, desktop) = session.steps().await;
    assert_eq!(steps, ["first-picture"]);
    assert_eq!(desktop.write(), r#"{"desktop":{"width":200,"height":120}}"#);
    // where the window is comes before what is drawn in it
    let windows = session.next().await.expect("the windows");
    assert_eq!(
        windows.write(),
        r#"{"windows":[{"id":65537,"title":"ledger","shown":"as-it-is","listed":true,"seen":[[20,10,180,110]]}],"in_front":65537}"#
    );
    // what is drawn is the window, where it is on the machine's desktop, and nothing beside it
    let shows = stand_in::desktop();
    let inside = |x: usize, y: usize| (20..180).contains(&x) && (10..110).contains(&y);
    let is_the_window = |session: &Session| {
        let picture = session.picture.as_ref().expect("a picture");
        (0..120).all(|y| {
            (0..200).all(|x| {
                let at = (y * 200 + x) * 4;
                let expected = if inside(x, y) { &shows.pixels[at..at + 3] } else { &[0, 0, 0][..] };
                &picture.pixels[at..at + 3] == expected
            })
        })
    };
    while !is_the_window(&session) {
        let drawn = session.until("drawn").await.get("drawn").and_then(Json::count).expect("a number");
        session.say(Json::object([("seen", drawn.into())])).await;
    }
    // the machine was told that the connection is for a program, and which
    let seen = world.machine.seen();
    let last = seen.last().expect("a client came");
    assert!(last.for_a_program);
    assert_eq!(
        last.about_programs.last(),
        Some(&away_rdp::rail::stand_in::Asked::Start(away_rdp::rail::Program { run: "||ledger".to_string(), arguments: String::new() }))
    );

    // the person closes the window: it is gone, and with its last window the program's session ends
    session.say(Json::object([("window", 65537u64.into()), ("does", "close".into())])).await;
    assert_eq!(session.until("windows").await.write(), r#"{"windows":[],"in_front":null}"#);
    assert_eq!(session.until("ended").await.write(), r#"{"ended":"closed"}"#);
    assert!(session.next().await.is_none());

    // the records say that it was a program, and which
    let kept = records(&world.database).await;
    let of_sessions: Vec<_> = kept.iter().filter(|(kind, ..)| kind.starts_with("session-")).collect();
    assert_eq!(
        of_sessions.iter().map(|(kind, ..)| kind.as_str()).collect::<Vec<_>>(),
        ["session-asked", "session-opened", "session-ended"]
    );
    assert_eq!((of_sessions[0].3.text_of("way"), of_sessions[0].3.text_of("program")), (Some("program"), Some("Ledger")));
    assert_eq!(of_sessions[2].3.text_of("why"), Some("closed"));

    // a program the machine doesn't publish is not started, and the session says so
    let mut session = Session::begin(&world, &sign_in, &browser.ticket_for_a_program(&world, "apps-01", "Payroll")).await;
    assert_eq!(session.steps().await.0, ["reaching", "checking", "signing-in"]);
    session.say(credentials(KNOWN, PASSWORD)).await;
    let (steps, failed) = session.steps().await;
    assert_eq!(steps, ["first-picture"]);
    let failed = failed.get("failed").expect("a step that failed").clone();
    assert_eq!((failed.text_of("step"), failed.text_of("why")), (Some("first-picture"), Some("program-not-started")));
    assert!(failed.text_of("details").is_some_and(|details| details.contains("not among those it publishes")));

    // and a desktop of the same machine opens as before
    let mut session = Session::begin(&world, &sign_in, &browser.ticket(&world, "apps-01")).await;
    assert_eq!(session.steps().await.0, ["reaching", "checking", "signing-in"]);
    session.say(credentials(KNOWN, PASSWORD)).await;
    let (_, desktop) = session.steps().await;
    assert!(desktop.get("desktop").is_some());
    assert!(!world.machine.seen().last().expect("a client came").for_a_program);
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

/// What the sign-in service says of a person of the domain: the account's name as the
/// domain knows it, and their groups.
fn of_the_domain(groups: &[&str]) -> Json {
    Json::object([
        ("sub", "7c1f".into()),
        ("name", "Alex Urban".into()),
        ("preferred_username", "alex.urban".into()),
        ("groups", Json::List(groups.iter().map(|group| (*group).into()).collect())),
    ])
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_person_opens_a_desktop_of_their_domain_as_themselves_and_nothing_is_asked() {
    let Some(kerberos) = kerberos() else { return };
    let Some(world) = world_with("as-themselves", Some(&kerberos)).await else { return };
    let domain = world.domain.as_ref().expect("a world with a domain");
    world._service.signs_in(of_the_domain(&["staff"]));
    let mut browser = Browser::default();
    browser.sign_in(&world);
    let sign_in = browser.signed_in_as(&world);
    let ticket = browser.ticket(&world, "own-01");

    // the steps, as a person sees them: no name and no password is asked for
    let mut session = Session::begin(&world, &sign_in, &ticket).await;
    let (steps, desktop) = session.steps().await;
    assert_eq!(steps, ["reaching", "checking", "signing-in", "first-picture"]);
    assert_eq!(desktop.write(), r#"{"desktop":{"width":200,"height":120}}"#);
    let shows = stand_in::desktop();
    while session.picture.as_ref() != Some(&shows) {
        let drawn = session.until("drawn").await.get("drawn").and_then(Json::count).expect("a number");
        session.say(Json::object([("seen", drawn.into())])).await;
    }

    // the machine read in the ticket who the person is, and on the card what signs them in
    assert_eq!(domain.machine.shown(), [("alex.urban".to_string(), "CORP.EXAMPLE".to_string())]);
    let read_the_card = async {
        while !domain.machine.seen().last().and_then(|seen| seen.card.as_ref()).is_some_and(|card| card.closed) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), read_the_card).await.expect("the machine reads the card to its end");
    let card = domain.machine.seen().last().and_then(|seen| seen.card.clone()).expect("the machine read the card");
    let read = away_winauth::card::stand_in::read(&card.answers).expect("the answers of a card of this kind");
    let on_it = away_winauth::certificate::Certificate::read(&read.certificate).expect("the certificate on the card");
    assert_eq!(on_it.account.as_deref(), Some("alex.urban@CORP.EXAMPLE"));
    assert!(away_winauth::card::stand_in::is_signed_by(&read.certificate, stand_in::TO_SIGN, &read.signed));

    // the organisation's authority issued the issuer's own certificate, and one for the person
    let kinds: Vec<String> = domain.pages.seen().issued.iter().map(|issued| issued.kind.clone()).collect();
    assert_eq!(kinds, ["CorpAgent", "CorpSignIn"]);
    session.say(Json::object([("leaves", true.into())])).await;
    while session.next().await.is_some() {}

    // the sign-in that was made is a record, between the one that the session was asked
    // for and the one that it is open, and a line of the issuer's
    let kept = records(&world.database).await;
    let of_sessions: Vec<_> = kept.iter().filter(|(kind, ..)| kind.starts_with("session-")).collect();
    assert_eq!(
        of_sessions.iter().map(|(kind, ..)| kind.as_str()).collect::<Vec<_>>(),
        ["session-asked", "session-signed-in", "session-opened", "session-ended"]
    );
    assert_eq!(of_sessions[0].3.text_of("signs_in"), Some("themselves"));
    let made = &of_sessions[1];
    assert_eq!((made.1.as_deref(), made.2.as_deref()), (Some("Alex Urban"), Some("own-01")));
    assert_eq!(made.3.text_of("as"), Some("alex.urban@CORP.EXAMPLE"));
    assert!(made.3.text_of("number").is_some_and(|number| !number.is_empty()) && made.3.get("until").and_then(Json::count).is_some());
    assert!(domain.issuer_lines.kept().iter().any(|line| line.contains("signed a person in") && line.contains("\"machine\":\"own-01\"")));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_person_away_signs_in_as_themselves_on_no_machine_is_told_so_and_it_is_a_record() {
    let Some(kerberos) = kerberos() else { return };
    let Some(world) = world_with("never-themselves", Some(&kerberos)).await else { return };
    let domain = world.domain.as_ref().expect("a world with a domain");
    // one of the directory's administrators
    world._service.signs_in(of_the_domain(&["staff", "Domain Admins"]));
    let mut browser = Browser::default();
    browser.sign_in(&world);
    let sign_in = browser.signed_in_as(&world);
    let ticket = browser.ticket(&world, "own-01");

    let mut session = Session::begin(&world, &sign_in, &ticket).await;
    let (steps, failed) = session.steps().await;
    assert_eq!(steps, ["reaching", "checking", "signing-in"]);
    let failed = failed.get("failed").expect("the step failed");
    assert_eq!((failed.text_of("step"), failed.text_of("why")), (Some("signing-in"), Some("not-as-yourself")));
    assert_eq!(
        failed.text_of("details"),
        Some("Away signs nobody in on a machine as themselves who is in the group Domain Admins. (never)")
    );
    while session.next().await.is_some() {}

    // nothing was asked of the authority, the machine saw no ticket, and the records say what happened
    assert_eq!(domain.pages.seen(), Default::default());
    assert!(domain.machine.shown().is_empty());
    let kept = records(&world.database).await;
    let kinds: Vec<&str> = kept.iter().map(|(kind, ..)| kind.as_str()).filter(|kind| kind.starts_with("session-")).collect();
    assert_eq!(kinds, ["session-asked", "session-failed"]);
    let last = kept.last().expect("a record");
    assert_eq!((last.3.text_of("step"), last.3.text_of("why")), (Some("signing-in"), Some("not-as-yourself")));
    assert!(domain.issuer_lines.kept().iter().any(|line| line.contains("signed nobody in") && line.contains("\"why\":\"never\"")));
}
