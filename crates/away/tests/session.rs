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
/// The password of the desktop of the Mac that stands in.
const VNC_PASSWORD: &str = "a desktop";
/// The certificate of the key the tests seal what Away keeps to, which the server
/// reads, and the key, which the issuer reads.
const KEEPING_CERTIFICATE: &[u8] = include_bytes!("../../away-server/tests/fixtures/keeping.pem");
const KEEPING_KEY: &[u8] = include_bytes!("../../away-server/tests/fixtures/keeping.key.pem");

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

/// A port of this machine that nothing listens on. Not one the system hands out and
/// takes back: such a port is another test's a moment later, and whoever reaches for
/// it then finds something there.
const NOTHING_LISTENS: u16 = 1;

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
    /// A Mac, whose own screen sharing asks for the password of its desktop, and a
    /// desktop on Linux that asks for nothing: both speak VNC.
    mac: stand_in::vnc::Machine,
    open_desk: stand_in::vnc::Machine,
    database: Database,
    server_lines: Lines,
    /// What signing in as oneself takes, where the world has a domain.
    domain: Option<OfADomain>,
    /// What the issuer writes: it keeps the passwords of two desktops over VNC, one of
    /// them wrong, in a world with a domain or without one.
    issuer_lines: Lines,
    _issuer: Option<Stopper>,
    _stoppers: (Stopper, Stopper),
    _folder: Folder,
    _kept: Folder,
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
            // a machine of the domain: it takes a ticket, and knows an account by its password too
            Machine::start_in_a_domain_that_knows(
                password_of("the password of a machine that exists for these tests alone", b"CORP.EXAMPLETERMSRVlocalhost"),
                KNOWN,
                PASSWORD,
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
    // two machines that speak VNC: a Mac with the password of its desktop, and a desktop that asks for nothing
    let mac = stand_in::vnc::Machine::start_a_mac_at("127.0.0.1:0", VNC_PASSWORD, stand_in::vnc::desktop()).await;
    let open_desk = stand_in::vnc::Machine::start_open(stand_in::vnc::desktop()).await;
    let ((_, mac_port), (_, open_port)) = (mac.address(), open_desk.address());
    // a port of this machine that nothing listens on, for a machine that is not there
    let nothing = NOTHING_LISTENS;
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
         - name: hidden-09\n    kind: windows\n    address: hidden-09.corp.example\n    desktop: true\n  \
         - name: mac-01\n    kind: mac\n    address: {host}\n    desktop: {{ port: {mac_port} }}\n    clipboard: both\n  \
         - name: tux-02\n    kind: linux\n    address: {host}\n    desktop: {{ port: {open_port} }}\n    clipboard: none\n  \
         - name: mac-03\n    kind: mac\n    address: {host}\n    desktop: {{ port: {mac_port} }}\n    signs-in: {{ kept: mac-03.vnc }}\n  \
         - name: mac-04\n    kind: mac\n    address: {host}\n    desktop: {{ port: {mac_port} }}\n    signs-in: {{ kept: mac-04.vnc }}\n  \
         - name: mac-05\n    kind: mac\n    address: {host}\n    desktop: {{ port: {mac_port} }}\n    signs-in: {{ kept: mac-05 }}\n  \
         - name: mac-06\n    kind: mac\n    address: {host}\n    desktop: {{ port: {mac_port} }}\n    signs-in: {{ kept: mac-06 }}\n\
         rules:\n  - name: staff-desks\n    who: [staff]\n    \
         reaches: [desk-01, apps-01, gone-02, build-01, Desktops, Elsewhere, Unnamed, Alone, Anywhere, mac-01, tux-02, mac-03, mac-04, mac-05, mac-06]\n"
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
    // the passwords Away keeps for machines, in a folder of their own beside the files:
    // the right one of the Mac's desktop, and a wrong one
    let kept = std::env::temp_dir().join(format!("away-kept-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&kept);
    std::fs::create_dir_all(&kept).expect("a folder of kept secrets");
    std::fs::write(kept.join("mac-03.vnc"), format!("{VNC_PASSWORD}\n")).expect("a kept secret");
    std::fs::write(kept.join("mac-04.vnc"), "not the desktop's password").expect("a kept secret");
    let kept = Folder(kept);

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
        keeping: Some(KEEPING_CERTIFICATE.to_vec()),
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
    let (domain, in_a_domain, issuer) = match (kerberos, own) {
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
                known_as: Vec::new(),
                good_for: 300,
            })
            .await;
            let told = away_issuer::Told {
                administrators: None,
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
                kept: Some(kept.0.to_string_lossy().to_string()),
                keeping: Some(KEEPING_KEY.to_vec()),
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
            let issuer = (told.issuer.clone(), issuer_lines.clone(), None);
            (Some(OfADomain { machine: own, pages, issuer_lines, _stopper: stopper }), Some(told), issuer)
        }
        // without a domain the issuer keeps the secrets alone
        _ => {
            let told = away_issuer::Told {
                internal: Some(away_issuer::told::Internal { address: "127.0.0.1:0".to_string(), certificate: None }),
                sign_in: Some(away_issuer::told::SignInService {
                    service: service.address(),
                    client: "away".to_string(),
                    authorities: None,
                }),
                server: Some(away_issuer::told::Server {
                    at: Address::read(&format!("http://127.0.0.1:{}", internal.port())).unwrap(),
                    authorities: None,
                }),
                kept: Some(kept.0.to_string_lossy().to_string()),
                keeping: Some(KEEPING_KEY.to_vec()),
                ..away_issuer::Told::default()
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
            (None, None, (at, issuer_lines, Some(stopper)))
        }
    };
    let (issuer_at, issuer_lines, issuer_stopper) = issuer;

    let (gateway_stopper, stop) = stop::pair();
    let told = away_gateway::Told {
        server: Some(Address::read(&format!("http://127.0.0.1:{}", internal.port())).unwrap()),
        authorities: None,
        restricted_admin: None,
        domain: in_a_domain,
        issuer: Some((issuer_at, None)),
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
        mac,
        open_desk,
        database,
        server_lines,
        domain,
        issuer_lines,
        _issuer: issuer_stopper,
        _stoppers: (server_stopper, gateway_stopper),
        _folder: folder,
        _kept: kept,
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
    /// The icons of windows that arrived, each with its window, as PNG pictures.
    icons: Vec<(u32, Vec<u8>)>,
    /// What the tray was last said to hold, and the pictures of its icons that arrived.
    tray: Option<Json>,
    tray_pictures: Vec<(u32, u32, Vec<u8>)>,
    /// The pictures that were copied on the machine, as PNG pictures, and the pieces of
    /// the files that were asked for.
    copied_pictures: Vec<Vec<u8>>,
    pieces: Vec<Vec<u8>>,
}

impl Session {
    /// Opens a session's connection to the gateway, as the browser that is signed in.
    async fn begin(world: &World, sign_in: &str, ticket: &str) -> Session {
        let mut request = format!("ws://{}/session", world.gateway).into_client_request().expect("a request");
        request.headers_mut().insert("cookie", format!("__Host-away={sign_in}").parse().unwrap());
        request.headers_mut().insert("origin", format!("http://{}", world.gateway).parse().unwrap());
        let stream = TcpStream::connect(&world.gateway).await.expect("the gateway takes connections");
        let (mut connection, _) = tokio_tungstenite::client_async(request, stream).await.expect("the connection becomes a session's");
        // a window of 1280 by 800, on a screen with twice the dots each way, and a Swiss
        // German keyboard
        let begins = Json::object([
            ("ticket", ticket.into()),
            ("width", 1280u64.into()),
            ("height", 800u64.into()),
            ("scale", 200u64.into()),
            ("layout", 0x0807u64.into()),
        ]);
        connection.send(Message::text(begins.write())).await.expect("the first message goes out");
        Session {
            connection,
            picture: None,
            icons: Vec::new(),
            tray: None,
            tray_pictures: Vec::new(),
            copied_pictures: Vec::new(),
            pieces: Vec::new(),
        }
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
                    if let Some(tray) = says.get("tray") {
                        self.tray = Some(tray.clone());
                    }
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

    /// Draws a rectangle that arrived: where it goes, and its pixels as a PNG picture. An
    /// icon of a window is kept.
    fn draw(&mut self, bytes: &[u8]) {
        let number = |at: usize| u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
        if bytes[0] == 2 {
            self.icons.push((number(1), bytes[5..].to_vec()));
            return;
        }
        if bytes[0] == 3 {
            self.tray_pictures.push((number(1), number(5), bytes[9..].to_vec()));
            return;
        }
        // a picture that was copied on the machine, and a piece of a file that was asked for
        if bytes[0] == 4 {
            self.copied_pictures.push(bytes[1..].to_vec());
            return;
        }
        if bytes[0] == 5 {
            self.pieces.push(bytes[1..].to_vec());
            return;
        }
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

    /// Takes what the gateway says until a picture that was copied on the machine came.
    async fn until_a_picture(&mut self) {
        while self.copied_pictures.is_empty() {
            let message =
                tokio::time::timeout(Duration::from_secs(40), self.connection.next()).await.expect("the gateway says something in time");
            if let Some(Ok(Message::Binary(bytes))) = message {
                self.draw(&bytes);
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
    // the machine was asked for the dots of the person's screen, drawn twice as large
    assert_eq!((last.asked, last.scale), ((2560, 1600), 200));
    assert_eq!(last.account, ("CORP".to_string(), "alex.urban".to_string()));
    assert_eq!(last.input.len(), 5);

    // the window gets another size, on a screen that shows a dot for a dot: the machine
    // is told, offers its desktop anew at that size, and all of it is drawn anew
    let sized = Json::object([("width", 640u64.into()), ("height", 480u64.into()), ("scale", 100u64.into())]);
    session.say(Json::object([("sized", sized.clone())])).await;
    assert_eq!(session.until("desktop").await.write(), r#"{"desktop":{"width":640,"height":480}}"#);
    while session.picture != world.machine.picture() {
        drawn = session.until("drawn").await.get("drawn").and_then(Json::count).expect("a number");
        session.say(Json::object([("seen", drawn.into())])).await;
    }
    // a window that says its size once more, unchanged, is not told to the machine again
    session.say(Json::object([("sized", sized)])).await;
    session.did(&[4, 0x1f, 0, 1, 4, 0x1f, 0, 0]).await;
    while session.pixel(5, 5) != typed(0x1f) {
        drawn = session.until("drawn").await.get("drawn").and_then(Json::count).expect("a number");
        session.say(Json::object([("seen", drawn.into())])).await;
    }
    let seen = world.machine.seen();
    let last = seen.last().expect("a client came as far as the desktop");
    assert_eq!(last.screens, [(640, 480, 100)]);
    assert_eq!(last.took_later, [3]);

    // the clipboard: what the machine copies, on the key C, comes to the person; what
    // the person copied is pasted on the machine, which copies it back in capitals
    session.did(&[4, 0x2e, 0, 1, 4, 0x2e, 0, 0]).await;
    assert_eq!(session.until("copied").await.write(), r#"{"copied":"copied on the machine\nin two lines"}"#);
    session.say(Json::object([("copied", "from the person's device".into())])).await;
    session.did(&[4, 0x2e, 0, 1, 4, 0x2e, 0, 0]).await;
    assert_eq!(session.until("copied").await.write(), r#"{"copied":"FROM THE PERSON'S DEVICE"}"#);
    let seen = world.machine.seen();
    assert_eq!(seen.last().expect("the client").pasted, ["from the person's device"]);

    // text that is longer than a message of the browser's comes as the message that says
    // it, in pieces: the machine pastes all of it, and copies it back in capitals
    let long = "long text ".repeat(20_000);
    let message = Json::object([("copied", long.as_str().into())]).write();
    let pieces: Vec<&[u8]> = message.as_bytes().chunks(48 * 1024).collect();
    for (nth, piece) in pieces.iter().enumerate() {
        let which = u8::from(nth == 0) | (u8::from(nth + 1 == pieces.len()) << 1);
        session.did(&[&[0x12, which][..], piece].concat()).await;
    }
    session.did(&[4, 0x2e, 0, 1, 4, 0x2e, 0, 0]).await;
    assert!(session.until("copied").await.text_of("copied") == Some(long.to_uppercase().as_str()), "the long text came back whole");
    assert_eq!(world.machine.seen().last().expect("the client").pasted.len(), 2);

    // text keeps its formatting: the key H copies some on the machine, and the
    // formatting comes after the text; what the person copied with formatting is the
    // machine's to paste with it
    session.did(&[4, 0x23, 0, 1, 4, 0x23, 0, 0]).await;
    assert_eq!(session.until("formatted").await.write(), r#"{"formatted":"<b>copied</b> on the machine"}"#);
    session.say(Json::object([("copied", "slanted".into()), ("formatted", "<i>slanted</i>".into())])).await;
    let pasted = async {
        while world.machine.seen().last().is_none_or(|seen| seen.pasted_formatting.is_empty()) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), pasted).await.expect("the machine pastes the formatted text");
    let seen = world.machine.seen();
    let client = seen.last().expect("the client");
    assert_eq!(
        (client.pasted.last().map(String::as_str), client.pasted_formatting.as_slice()),
        (Some("slanted"), &["<i>slanted</i>".to_string()][..])
    );

    // a picture passes the same way: the key P copies one on the machine, which comes
    // as a PNG picture; the key F copies files, which are listed with nothing of what
    // is in them
    session.did(&[4, 0x19, 0, 1, 4, 0x19, 0, 0]).await;
    session.until_a_picture().await;
    session.did(&[4, 0x21, 0, 1, 4, 0x21, 0, 0]).await;
    assert_eq!(
        session.until("files").await.write(),
        r#"{"files":[{"name":"notes.txt","size":11},{"name":"folder","folder":true},{"name":"folder/data.bin","size":300000}]}"#
    );
    let came = session.copied_pictures.last().expect("the picture that was copied on the machine").clone();
    let mut reader = png::Decoder::new(std::io::Cursor::new(came)).read_info().expect("a PNG picture");
    let mut pixels = vec![0u8; reader.output_buffer_size().expect("a size")];
    let info = reader.next_frame(&mut pixels).expect("its pixels");
    assert_eq!((info.width, info.height, &pixels[..3]), (3, 2, &[10u8, 20, 30][..]));
    // a picture the person copied comes in pieces, and is handed to the machine once it is whole
    let mut theirs = Vec::new();
    let mut encoder = png::Encoder::new(&mut theirs, 2, 1);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("a head");
    writer.write_image_data(&[200, 100, 50, 1, 2, 3]).expect("the pixels");
    writer.finish().expect("the end");
    let (first, rest) = theirs.split_at(20);
    session.did(&[&[0x10, 1], first].concat()).await;
    session.did(&[&[0x10, 2], rest].concat()).await;

    // a file is had by its place in the list: it comes in pieces, in their order, and
    // then that all of it came
    session.say(Json::object([("file", 0u64.into())])).await;
    assert_eq!(session.until("file").await.write(), r#"{"file":"whole"}"#);
    assert_eq!(session.pieces.concat(), b"hello files");
    session.pieces.clear();
    session.say(Json::object([("file", 2u64.into())])).await;
    assert_eq!(session.until("file").await.write(), r#"{"file":"whole"}"#);
    assert_eq!(session.pieces.iter().map(Vec::len).collect::<Vec<_>>(), [262_144, 37_856]);
    assert_eq!(session.pieces.concat(), away_rdp::stand_in::desktop::copied_files()[2].1);
    // a folder holds nothing to have, and neither does what is not listed
    for nothing in [1u64, 9] {
        session.say(Json::object([("file", nothing.into())])).await;
        assert_eq!(session.until("file").await.write(), r#"{"file":"failed"}"#);
    }
    let seen = world.machine.seen();
    let pasted = &seen.last().expect("the client").pasted_pictures;
    assert_eq!(
        pasted.iter().map(|picture| (picture.width, picture.height, picture.pixels.clone())).collect::<Vec<_>>(),
        [(2, 1, vec![200, 100, 50, 1, 2, 3])]
    );

    // a file the person copied: the machine is told its name and its size, pastes at
    // once, and wants what is in it, which the person's device gives in pieces
    session.say(Json::read(r#"{"gives":[{"name":"report.txt","size":8}]}"#).expect("JSON")).await;
    assert_eq!(session.until("wants").await.write(), r#"{"wants":{"file":0,"from":0,"length":8}}"#);
    session.did(&[&[0x11, 1][..], b"a re"].concat()).await;
    session.did(&[&[0x11, 2][..], b"port"].concat()).await;
    let pasted = async {
        while world.machine.seen().last().is_none_or(|seen| seen.pasted_files.is_empty()) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), pasted).await.expect("the machine pastes the file");
    assert_eq!(world.machine.seen().last().expect("the client").pasted_files, [("report.txt".to_string(), b"a report".to_vec())]);

    // the person leaves: the session says so and ends, and the machine was told
    session.say(Json::object([("leaves", true.into())])).await;
    assert_eq!(session.until("ended").await.write(), r#"{"ended":"left"}"#);
    assert!(session.next().await.is_none());

    // every step of it is a record: asked for, open, ended, each with who and which machine
    let kept = records(&world.database).await;
    let of_sessions: Vec<_> = kept.iter().filter(|(kind, ..)| kind.starts_with("session-")).collect();
    assert_eq!(
        of_sessions.iter().map(|(kind, ..)| kind.as_str()).collect::<Vec<_>>(),
        ["session-asked", "session-opened", "session-file", "session-file", "session-file", "session-ended"]
    );
    for (_, person, machine, _) in &of_sessions {
        assert_eq!((person.as_deref(), machine.as_deref()), (Some("Alex Urban"), Some("desk-01")));
    }
    assert_eq!(of_sessions[0].3.text_of("way"), Some("desktop"));
    assert_eq!(
        (of_sessions[1].3.get("width").and_then(Json::count), of_sessions[1].3.get("height").and_then(Json::count)),
        (Some(200), Some(120))
    );
    // each file that passed, with its name, its size and which way it went, and nothing of what is in it
    let passed: Vec<_> = of_sessions[2..5]
        .iter()
        .map(|(.., details)| (details.text_of("name"), details.get("size").and_then(Json::count), details.text_of("direction")))
        .collect();
    assert_eq!(
        passed,
        [
            (Some("notes.txt"), Some(11), Some("from-the-machine")),
            (Some("folder/data.bin"), Some(300_000), Some("from-the-machine")),
            (Some("report.txt"), Some(8), Some("to-the-machine"))
        ]
    );
    assert_eq!(of_sessions[5].3.text_of("why"), Some("left"));
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
async fn management_methods_are_admitted_by_policy_and_rechecked_before_tickets_and_certificates() {
    let Some(world) = world("management-methods").await else { return };
    let file = world._folder.0.join("management.yaml");
    let policy = |mode: &str| {
        format!("machines:\n  - name: management\n    kind: windows\n    address: host.corp.example\n    desktop: true\n    signs-in: {mode}\nrules:\n  - name: management-access\n    who: [staff]\n    reaches: [management]\n")
    };
    std::fs::write(&file, policy("management-sso")).unwrap();
    let mut browser = Browser::default();
    browser.sign_in(&world);
    let sign_in = browser.signed_in_as(&world);
    tokio::time::timeout(Duration::from_secs(10), async {
        while !browser.ask("GET", &format!("{}/api/machines", world.away), false).2.contains("management") {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    for method in ["typed", "unknown"] {
        assert_eq!(browser.open(&world, &format!("management&signs_in={method}")).0, 403);
    }
    assert_eq!(browser.open(&world, "management&signs_in=themselves&signs_in=restricted-admin-sso").0, 400);
    assert_eq!(browser.open(&world, "desk-01&signs_in=themselves").0, 403);
    let client = away_part::client::Client::new(None).unwrap();
    let at = Address::read(&format!("http://127.0.0.1:{}/internal/take", world.internal.port())).unwrap();
    let mut certificate_session = String::new();
    for (query, expected) in [("management", "restricted-admin-sso"), ("management&signs_in=themselves", "themselves")] {
        let (status, body) = browser.open(&world, query);
        assert_eq!(status, 200, "{body}");
        let body = Json::read(&body).unwrap();
        let got = client.send_form(&at, &[("ticket", body.text_of("ticket").unwrap()), ("sign_in", &sign_in)], None).await.unwrap();
        assert_eq!(got.status, 200);
        let taken = Json::read(std::str::from_utf8(&got.body).unwrap()).unwrap();
        assert_eq!(taken.get("machine").unwrap().text_of("signs_in"), Some(expected));
        let session = body.text_of("session").unwrap();
        let (status, vouched) = vouched(&world, session).await;
        assert_eq!(status, 200, "{vouched}");
        assert_eq!(Json::read(&vouched).unwrap().text_of("signs_in"), Some(expected));
        if expected == "themselves" {
            certificate_session = session.to_string();
        }
    }
    let pending = browser.ticket(&world, "management&signs_in=themselves");
    std::fs::write(&file, policy("restricted-admin-sso")).unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while browser.open(&world, "management&signs_in=themselves").0 != 403 {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(client.send_form(&at, &[("ticket", &pending), ("sign_in", &sign_in)], None).await.unwrap().status, 404);
    assert_eq!(vouched(&world, &certificate_session).await.0, 403);
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
        (501, "Away opens desktops and programs so far, and this machine offers neither.\n".to_string())
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
    // which ways the clipboard passes is said first: both, where the files say nothing,
    // and with pictures, which a Windows machine carries
    assert_eq!(session.next().await.map(|says| says.write()).as_deref(), Some(r#"{"clipboard":{"to":true,"from":true,"pictures":true}}"#));
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
    // the machine was told that the connection is for a program, the layout of the
    // person's keyboard, and which program
    let seen = world.machine.seen();
    let last = seen.last().expect("a client came");
    assert!(last.for_a_program);
    assert!(last.about_programs.contains(&away_rdp::rail::stand_in::Asked::Keyboard(0x0807)));
    assert_eq!(
        last.about_programs.last(),
        Some(&away_rdp::rail::stand_in::Asked::Start(away_rdp::rail::Program { run: "||ledger".to_string(), arguments: String::new() }))
    );

    // the window's icon came with it, as a picture of 32 pixels square
    let (window, icon) = session.icons.first().cloned().expect("the window's icon");
    let picture = png::Decoder::new(std::io::Cursor::new(icon)).read_info().expect("a PNG picture").info().clone();
    assert_eq!((window, picture.width, picture.height, picture.color_type), (65537, 32, 32, png::ColorType::Rgba));

    // the program put an icon in the tray: its tip, and its picture after it
    assert_eq!(session.tray.as_ref().map(Json::write).as_deref(), Some(r#"[{"window":65537,"icon":1,"tip":"ledger"}]"#));
    assert_eq!(session.tray_pictures.iter().map(|(window, icon, _)| (*window, *icon)).collect::<Vec<_>>(), [(65537, 1)]);
    // its menu, asked for from the page, is asked of the program as Windows' taskbar asks it
    session
        .say(Json::object([("tray", Json::object([("window", 65537u64.into()), ("icon", 1u64.into()), ("does", "menu".into())]))]))
        .await;
    let asked_the_tray = async {
        loop {
            let told =
                world.machine.seen().last().map(|seen| {
                    seen.about_programs.iter().filter(|asked| matches!(asked, away_rdp::rail::stand_in::Asked::Tray(..))).count()
                });
            if told == Some(3) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), asked_the_tray).await.expect("the program is asked for the icon's menu");

    // another program of the machine, asked for in the session, opens in it: in front of the first
    session.say(Json::object([("start", "Ledger".into())])).await;
    let windows = session.until("windows").await;
    let ids: Vec<u64> =
        windows.get("windows").and_then(Json::list).unwrap().iter().filter_map(|window| window.get("id").and_then(Json::count)).collect();
    assert_eq!((ids, windows.get("in_front").and_then(Json::count)), (vec![65538, 65537], Some(65538)));
    assert_eq!(session.until("started").await.write(), r#"{"started":{"program":"Ledger","ok":true}}"#);
    // one the machine doesn't start, and one the person may not open there, are said not to
    // have started, and the session goes on
    session.say(Json::object([("start", "Payroll".into())])).await;
    assert_eq!(
        session.until("started").await.write(),
        r#"{"started":{"program":"Payroll","ok":false,"details":"the machine does not let this program be started: it is not among those it publishes (its own code 0x00000002)"}}"#
    );
    session.say(Json::object([("start", "Nothing".into())])).await;
    assert_eq!(session.until("started").await.write(), r#"{"started":{"program":"Nothing","ok":false}}"#);
    let last = world.machine.seen().last().expect("a client came").about_programs.clone();
    let asked: Vec<_> = last
        .iter()
        .filter_map(|asked| if let away_rdp::rail::stand_in::Asked::Start(program) = asked { Some(program.run.as_str()) } else { None })
        .collect();
    assert_eq!(asked, ["||ledger", "||ledger", "||Payroll"]);

    // the person closes the windows: they are gone, and with the last one the program's session ends
    session.say(Json::object([("window", 65538u64.into()), ("does", "close".into())])).await;
    session.say(Json::object([("window", 65537u64.into()), ("does", "close".into())])).await;
    loop {
        if session.until("windows").await.write() == r#"{"windows":[],"in_front":null}"# {
            break;
        }
    }
    assert_eq!(session.until("ended").await.write(), r#"{"ended":"closed"}"#);
    assert!(session.next().await.is_none());

    // the records say that it was a program, and which, and which others were asked for in it
    let kept = records(&world.database).await;
    let of_sessions: Vec<_> = kept.iter().filter(|(kind, ..)| kind.starts_with("session-")).collect();
    assert_eq!(
        of_sessions.iter().map(|(kind, ..)| kind.as_str()).collect::<Vec<_>>(),
        ["session-asked", "session-opened", "session-started", "session-started", "session-ended"]
    );
    assert_eq!((of_sessions[0].3.text_of("way"), of_sessions[0].3.text_of("program")), (Some("program"), Some("Ledger")));
    assert_eq!((of_sessions[2].3.text_of("program"), of_sessions[3].3.text_of("program")), (Some("Ledger"), Some("Payroll")));
    assert_eq!(of_sessions[4].3.text_of("why"), Some("closed"));

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
async fn a_desktop_over_vnc_whose_password_away_keeps_opens_with_nothing_typed_and_a_wrong_one_kept_is_asked_for() {
    let Some(world) = world("kept").await else { return };
    let mut browser = Browser::default();
    browser.sign_in(&world);
    let sign_in = browser.signed_in_as(&world);

    // the Mac whose password Away keeps: reached, signed in on, and its picture, with
    // nothing asked of the person
    let mut session = Session::begin(&world, &sign_in, &browser.ticket(&world, "mac-03")).await;
    let (steps, desktop) = session.steps().await;
    assert_eq!(steps, ["reaching", "signing-in", "first-picture"], "{}", desktop.write());
    assert_eq!(desktop.write(), r#"{"desktop":{"width":200,"height":120,"keys":"written"}}"#);
    session.say(Json::object([("leaves", true.into())])).await;
    assert_eq!(session.until("ended").await.write(), r#"{"ended":"left"}"#);

    // the issuer handed the password out for the session, and wrote who and which, and
    // nothing of the password itself; nor did anybody else
    let handed = world.issuer_lines.kept().into_iter().find(|line| line.contains("handed out a kept secret")).expect("a line");
    for fact in ["\"machine\":\"mac-03\"", "\"kept\":\"mac-03.vnc\""] {
        assert!(handed.contains(fact), "{fact} in {handed}");
    }
    assert!(world.issuer_lines.kept().iter().chain(world.server_lines.kept().iter()).all(|line| !line.contains(VNC_PASSWORD)));
    // the records say how the person was signed in, and which secret was handed out
    let kept = records(&world.database).await;
    let asked = kept.iter().find(|(kind, _, machine, _)| kind == "session-asked" && machine.as_deref() == Some("mac-03")).expect("asked");
    assert_eq!(asked.3.text_of("signs_in"), Some("kept"));
    let handed = kept.iter().find(|(kind, ..)| kind == "session-kept-secret").expect("the kept secret's record");
    assert_eq!((handed.2.as_deref(), handed.3.text_of("kept")), (Some("mac-03"), Some("mac-03.vnc")));

    // where the password Away keeps is not the desktop's, the machine refuses it, and the
    // person is asked as where nothing is kept: as after a wrong password
    let mut session = Session::begin(&world, &sign_in, &browser.ticket(&world, "mac-04")).await;
    let (steps, asks) = session.steps().await;
    assert_eq!(steps, ["reaching", "signing-in"], "{}", asks.write());
    assert_eq!(asks.write(), r#"{"asks":{"again":true,"only":"password"}}"#);
    session.say(credentials("", VNC_PASSWORD)).await;
    let (steps, _) = session.steps().await;
    assert_eq!(steps, ["first-picture"]);
    let handed_out = world.issuer_lines.kept().into_iter().filter(|line| line.contains("handed out a kept secret")).count();
    assert_eq!(handed_out, 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_password_away_keeps_in_its_own_store_is_sealed_there_and_only_the_issuer_opens_it_for_a_session() {
    let Some(world) = world("store").await else { return };
    // what an administrator's browser sealed to the issuer's key, as the store keeps it:
    // the desktop's password under mac-05, and under mac-06 one sealed under another name
    let public = away_server::keeping::read(KEEPING_CERTIFICATE).expect("the key's certificate").public;
    let kept = beside(&world.database).await;
    for (name, sealed) in [
        ("mac-05", away_issuer::keeping::seal(&public, "mac-05", VNC_PASSWORD)),
        ("mac-06", away_issuer::keeping::seal(&public, "another name", VNC_PASSWORD)),
    ] {
        let key = away_part::sealed::written(&away_part::sealed::key_name(&public));
        kept.execute(
            "insert into kept_secret (name, sealed, key, by_person, by_name) values ($1, $2, $3, 'a1', 'Ari Admin')",
            &[&name, &sealed, &key],
        )
        .await
        .expect("kept");
    }
    let mut browser = Browser::default();
    browser.sign_in(&world);
    let sign_in = browser.signed_in_as(&world);

    // the Mac whose password the store keeps opens with nothing asked
    let mut session = Session::begin(&world, &sign_in, &browser.ticket(&world, "mac-05")).await;
    let (steps, desktop) = session.steps().await;
    assert_eq!(steps, ["reaching", "signing-in", "first-picture"], "{}", desktop.write());
    session.say(Json::object([("leaves", true.into())])).await;
    assert_eq!(session.until("ended").await.write(), r#"{"ended":"left"}"#);
    let handed = world.issuer_lines.kept().into_iter().find(|line| line.contains("handed out a kept secret")).expect("a line");
    assert!(handed.contains("\"kept\":\"mac-05\""), "{handed}");
    // the password is in no line, and the sealed secret is what the database holds
    assert!(world.issuer_lines.kept().iter().chain(world.server_lines.kept().iter()).all(|line| !line.contains(VNC_PASSWORD)));
    let row = kept.query_one("select sealed from kept_secret where name = 'mac-05'", &[]).await.expect("the row");
    let sealed: Vec<u8> = row.get(0);
    assert!(sealed.starts_with(away_part::sealed::MARK) && !sealed.windows(VNC_PASSWORD.len()).any(|part| part == VNC_PASSWORD.as_bytes()));

    // a secret sealed under another name than the one it is kept under doesn't open: the
    // person is asked, as where nothing is kept
    let mut session = Session::begin(&world, &sign_in, &browser.ticket(&world, "mac-06")).await;
    let (steps, asks) = session.steps().await;
    assert_eq!(steps, ["reaching", "signing-in"], "{}", asks.write());
    assert_eq!(asks.write(), r#"{"asks":{"again":false,"only":"password"}}"#);
    assert!(world.issuer_lines.kept().iter().any(|line| line.contains("handed out no kept secret") && line.contains("doesn't open")));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_desktop_over_vnc_opens_with_its_password_and_a_mac_scrolls_as_far_as_the_person_turns() {
    let Some(world) = world("vnc").await else { return };
    let mut browser = Browser::default();
    browser.sign_in(&world);
    let sign_in = browser.signed_in_as(&world);

    // a Mac: reached, and then asked for the password of its desktop alone. There is
    // nothing such a machine shows of itself, so nothing is checked
    let mut session = Session::begin(&world, &sign_in, &browser.ticket(&world, "mac-01")).await;
    let (steps, asks) = session.steps().await;
    assert_eq!(steps, ["reaching", "signing-in"], "{}", asks.write());
    assert_eq!(asks.write(), r#"{"asks":{"again":false,"only":"password"}}"#);
    // a wrong password is asked for again, and the right one opens the desktop
    session.say(credentials("", "another")).await;
    assert_eq!(session.until("asks").await.write(), r#"{"asks":{"again":true,"only":"password"}}"#);
    session.say(credentials("", VNC_PASSWORD)).await;
    let (steps, desktop) = session.steps().await;
    assert_eq!(steps, ["first-picture"]);
    // the machine is told of keys by what they write
    assert_eq!(desktop.write(), r#"{"desktop":{"width":200,"height":120,"keys":"written"}}"#);
    // such a machine's clipboard carries text alone
    assert_eq!(session.next().await.map(|says| says.write()).as_deref(), Some(r#"{"clipboard":{"to":true,"from":true}}"#));
    // its picture arrives pixel for pixel
    let shows = stand_in::vnc::desktop();
    let is_the_desktop = |session: &Session, shows: &away_vnc::picture::Picture| {
        let picture = session.picture.as_ref().expect("a picture");
        picture.pixels.as_chunks::<4>().0.iter().zip(shows.pixels.as_chunks::<4>().0).all(|(has, shows)| has[..3] == shows[..3])
    };
    while !is_the_desktop(&session, &shows) {
        let drawn = session.until("drawn").await.get("drawn").and_then(Json::count).expect("a number");
        session.say(Json::object([("seen", drawn.into())])).await;
    }

    // a click, a key by what it writes, the command key, and a notch of the wheel towards the person
    let written = |sym: u32, down: bool| [&[7u8][..], &sym.to_be_bytes(), &[u8::from(down)]].concat();
    session.did(&[2, 0, 1, 0, 50, 0, 60, 2, 0, 0, 0, 50, 0, 60]).await;
    session.did(&[written(0xffeb, true), written(0x61, true), written(0x61, false), written(0xffeb, false)].concat()).await;
    session.did(&[3, 0xff, 0x88, 0, 0, 30, 0, 40]).await;
    // the machine's marks arrive: for the click where it was, for the key in the corner
    let clicked = stand_in::vnc::CLICKED;
    let typed = stand_in::vnc::typed(0x61);
    while session.pixel(55, 65) != [clicked[0], clicked[1], clicked[2]] || session.pixel(5, 5) != [typed[0], typed[1], typed[2]] {
        let drawn = session.until("drawn").await.get("drawn").and_then(Json::count).expect("a number");
        session.say(Json::object([("seen", drawn.into())])).await;
    }
    let seen = world.mac.seen().last().cloned().expect("a client came");
    assert_eq!((seen.answered.as_deref(), seen.chose, seen.shared), (Some("RFB 003.008\n"), Some(2), Some(true)));
    assert_eq!(seen.keys, [(0xffeb, true), (0x61, true), (0x61, false), (0xffeb, false)]);
    // the button at its place, down and up; then the wheel: a Mac scrolls by a pixel for a
    // step, so a notch, which stands for a hundred pixels, is a hundred steps down
    assert_eq!(seen.pointer[..2], [(1, 50, 60), (0, 50, 60)]);
    let steps = &seen.pointer[2..];
    assert_eq!(steps.len(), 200);
    assert!(steps.chunks(2).all(|step| step == [(0x10, 30, 40), (0, 30, 40)]));

    // text passes both ways: what the person copied, and what is copied on the machine
    session.say(Json::object([("copied", "from the person".into())])).await;
    session.did(&[written(0xffe3, true), written(0x63, true)].concat()).await;
    assert_eq!(session.until("copied").await.write(), r#"{"copied":"FROM THE PERSON"}"#);

    // the person leaves, and the records say what was opened, over what
    session.say(Json::object([("leaves", true.into())])).await;
    assert_eq!(session.until("ended").await.write(), r#"{"ended":"left"}"#);
    let kept = records(&world.database).await;
    let of_sessions: Vec<_> = kept.iter().filter(|(kind, ..)| kind.starts_with("session-")).collect();
    assert_eq!(
        of_sessions.iter().map(|(kind, ..)| kind.as_str()).collect::<Vec<_>>(),
        ["session-asked", "session-opened", "session-ended"]
    );
    assert_eq!((of_sessions[0].3.text_of("way"), of_sessions[0].3.text_of("over")), (Some("desktop"), Some("vnc")));

    // a desktop that asks for nothing opens with nothing asked, and its wheel is a step a notch
    let mut session = Session::begin(&world, &sign_in, &browser.ticket(&world, "tux-02")).await;
    let (steps, desktop) = session.steps().await;
    assert_eq!(steps, ["reaching", "signing-in", "first-picture"]);
    assert_eq!(desktop.write(), r#"{"desktop":{"width":200,"height":120,"keys":"written"}}"#);
    // the clipboard passes neither way where the files switch it off
    assert_eq!(session.next().await.map(|says| says.write()).as_deref(), Some(r#"{"clipboard":{"to":false,"from":false}}"#));
    session.did(&[3, 0xff, 0x88, 0, 0, 30, 0, 40]).await;
    session.did(&[2, 0, 1, 0, 9, 0, 9]).await;
    while session.pixel(12, 12) != [clicked[0], clicked[1], clicked[2]] {
        let drawn = session.until("drawn").await.get("drawn").and_then(Json::count).expect("a number");
        session.say(Json::object([("seen", drawn.into())])).await;
    }
    let seen = world.open_desk.seen().last().cloned().expect("a client came");
    assert_eq!((seen.chose, &seen.pointer[..]), (Some(1), &[(0x10, 30, 40), (0, 30, 40), (1, 9, 9)][..]));
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
        keeping: None,
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
async fn a_desktop_that_is_opened_on_another_device_is_there_and_the_one_it_was_on_says_so() {
    let Some(world) = world("taken-along").await else { return };
    let mut browser = Browser::default();
    browser.sign_in(&world);
    let sign_in = browser.signed_in_as(&world);
    let open = |ticket: String| {
        let (world, sign_in) = (&world, &sign_in);
        async move {
            let mut session = Session::begin(world, sign_in, &ticket).await;
            session.steps().await;
            session.say(credentials(KNOWN, PASSWORD)).await;
            let (steps, desktop) = session.steps().await;
            assert_eq!((steps, desktop.write().as_str()), (vec!["first-picture".to_string()], r#"{"desktop":{"width":200,"height":120}}"#));
            session
        }
    };
    // the desktop is open on one device, and is then opened on another
    let mut first = open(browser.ticket(&world, "desk-01")).await;
    let mut second = open(browser.ticket(&world, "desk-01")).await;
    // the first is told where the desktop went, and ends; the second has it
    assert_eq!(first.until("ended").await.write(), r#"{"ended":"elsewhere"}"#);
    while first.next().await.is_some() {}
    second.did(&[4, 0x1e, 0, 1, 4, 0x1e, 0, 0]).await;
    while second.pixel(5, 5) != typed(0x1e) {
        let drawn = second.until("drawn").await.get("drawn").and_then(Json::count).expect("a number");
        second.say(Json::object([("seen", drawn.into())])).await;
    }
    second.say(Json::object([("leaves", true.into())])).await;
    assert_eq!(second.until("ended").await.write(), r#"{"ended":"left"}"#);

    // the records say how each ended
    let kept = records(&world.database).await;
    let ended: Vec<Option<&str>> = kept.iter().filter(|(kind, ..)| kind == "session-ended").map(|record| record.3.text_of("why")).collect();
    assert_eq!(ended, [Some("elsewhere"), Some("left")]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_gateway_that_stops_tells_the_desktops_that_are_open_so_that_their_pages_open_them_again() {
    let Some(world) = world("stops").await else { return };
    let mut browser = Browser::default();
    browser.sign_in(&world);
    let sign_in = browser.signed_in_as(&world);
    let ticket = browser.ticket(&world, "desk-01");
    let mut session = Session::begin(&world, &sign_in, &ticket).await;
    session.steps().await;
    session.say(credentials(KNOWN, PASSWORD)).await;
    let (steps, desktop) = session.steps().await;
    assert_eq!((steps, desktop.write().as_str()), (vec!["first-picture".to_string()], r#"{"desktop":{"width":200,"height":120}}"#));

    // the gateway is told to stop, as the cluster tells it when Away is updated
    world._stoppers.1.stop();
    assert_eq!(session.until("ended").await.write(), r#"{"ended":"updating"}"#);
    while session.next().await.is_some() {}
    // the machine was left as by a person who leaves: the desktop goes on there
    let left = async {
        while !world.machine.seen().last().is_some_and(|seen| seen.left) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), left).await.expect("the machine is told that the client leaves");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_certificate_policy_refusal_never_falls_back_to_a_password() {
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
    let failed = failed.get("failed").expect("SSO fails closed, without asking for a password");
    assert_eq!(failed.text_of("why"), Some("not-as-yourself"));
    assert_eq!(domain.pages.seen(), Default::default());
    assert!(domain.machine.shown().is_empty());
    assert!(domain.issuer_lines.kept().iter().any(|line| line.contains("signed nobody in") && line.contains("\"why\":\"never\"")));
    while session.next().await.is_some() {}
    let kept = records(&world.database).await;
    assert!(!kept.iter().any(|(kind, ..)| kind == "session-opened"));
}
