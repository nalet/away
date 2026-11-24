//! The server beside a stand-in for a sign-in service, both on this machine: for working
//! on the pages, and for the pages' tests.
//!
//! ```sh
//! cargo run --package away-server --example pages --features stand-in
//! ```
//!
//! Away is then at <http://127.0.0.1:8080>, and whoever opens it is signed in as Alex
//! Urban, an administrator, without being asked anything. `PORT` chooses another port,
//! `GOOD_FOR` how many seconds a sign-in is good for before it is renewed, and
//! `ROLE=person` signs in somebody who administers nothing. `SIGN_IN_PORT` puts the
//! stand-in at a port that is known, so that a test can tell it how to behave, and
//! `SIGN_IN=absent` tells the server of a sign-in service that isn't there. `FILES`
//! names a folder of machines and rules; without it the server takes the tests' own.
//! `FARM_PORT` starts a stand-in for a farm's web access at that port, which publishes
//! a desktop and two programs, and writes the farm into the folder of files.
//! `DATABASE` names a PostgreSQL for what Away keeps, as `localhost:15432`: the one
//! `tests/machines/database/real.sh` starts. Without it Away keeps nothing. The server
//! makes a database of its own there each time it starts, so that it starts with
//! nothing kept; `DATABASE_NAME` names one to keep using in its place.
//! `MACHINE_PORT` starts a stand-in for a Windows machine at that port, and the gateway
//! beside the server, so that a desktop opens: `desk-01`, for the account
//! `CORP\alex.urban` with the password `a password that exists for these tests alone`.
//! The same stand-in is `apps-01`, which has two programs: `Books`, which the stand-in
//! publishes and opens in a window, and `Wages`, which it doesn't let be started.
//! Beside it stand two stand-ins for a farm's broker. One sends whoever signs in on to
//! that machine, and `Farm Desk` is its collection, whose files name the machine. The
//! other sends them on to a machine the files don't name: `Lost Desk`. A session's
//! connection then goes to the gateway and everything else to the server, as a way in
//! passes them on. No machine opens without a database.

use std::time::Duration;

use away_part::address::Address;
use away_part::{stop, Lines, Part, Settings};
use away_secret::Secret;
use away_server::service;
use away_server::stand_in::farm::Farm;
use away_server::stand_in::{person, SignInService};
use away_server::told::{Database, DatabaseConnection, Farms, Internal, SignIn, Told};
use away_server::Server;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let said = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
    let port = said("PORT").and_then(|port| port.parse::<u16>().ok()).unwrap_or(8080);
    let away = format!("http://127.0.0.1:{port}");
    let back = format!("{away}/sign-in/back");

    let at = format!("127.0.0.1:{}", said("SIGN_IN_PORT").and_then(|port| port.parse::<u16>().ok()).unwrap_or(0));
    let sign_in_service = SignInService::start_at(&at, "away", "a secret for this machine alone", &back).await;
    if let Some(seconds) = said("GOOD_FOR").and_then(|seconds| seconds.parse().ok()) {
        sign_in_service.signs_for(seconds);
    }
    if said("ROLE").as_deref() == Some("person") {
        sign_in_service.signs_in(person());
    }
    let mut service_at = sign_in_service.address();
    if said("SIGN_IN").as_deref() == Some("absent") {
        // a port of this machine that nothing listens on
        let free = std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?.port();
        service_at = format!("http://127.0.0.1:{free}");
    }

    // a farm beside the server, for the tests that want one: its feed is named in a file
    // of the folder, and a rule gives what it publishes to the people of the farm
    let files = said("FILES").unwrap_or_else(|| concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/files").to_string());
    let mut farms = None;
    let mut farm = None;
    if let Some(port) = said("FARM_PORT").and_then(|port| port.parse::<u16>().ok()) {
        let password = "the password of the account that reads the feed";
        let stand_in = Farm::start_at(&format!("127.0.0.1:{port}"), "CORP\\svc-away", password).await;
        let icon = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/files/icons/ledger.png"))?;
        stand_in.publishes("Farm Desktop", true, "Desktops", None);
        stand_in.publishes("Calculator", false, "Tools", Some(&icon));
        stand_in.publishes("Notepad", false, "Tools", None);
        std::fs::write(
            std::path::Path::new(&files).join("farm.yaml"),
            format!(
                "farms:\n  - name: Work Resources\n    feed: {}\n    groups: [published]\nrules:\n  - name: farm-users-published\n    who: [farm-users]\n    reaches: [published]\n",
                stand_in.feed()
            ),
        )?;
        farms = Some(Farms {
            account: "CORP\\svc-away".to_string(),
            password: Secret::new(password),
            authorities: None,
            every: Duration::from_millis(300),
        });
        farm = Some(stand_in);
    }

    // the database, where one is named: one of its own for this start, unless one is named
    let mut database = None;
    if let Some(at) = said("DATABASE") {
        let (host, port) = at.rsplit_once(':').expect("DATABASE is host:port");
        let password = "a password for this machine alone";
        let mut name = said("DATABASE_NAME");
        if name.is_none() {
            let mut random = [0u8; 6];
            getrandom::fill(&mut random).map_err(std::io::Error::other)?;
            let made = format!("pages_{}", random.iter().map(|byte| format!("{byte:02x}")).collect::<String>());
            let mut at = tokio_postgres::Config::new();
            at.host(host).port(port.parse().expect("DATABASE names a port")).dbname("away").user("away").password(password);
            let (client, connection) = at.connect(tokio_postgres::NoTls).await.map_err(std::io::Error::other)?;
            let carried = tokio::spawn(connection);
            client.batch_execute(&format!("create database {made}")).await.map_err(std::io::Error::other)?;
            drop(client);
            let _ = carried.await;
            name = Some(made);
        }
        database = Some(Database {
            host: host.to_string(),
            port: port.parse().expect("DATABASE names a port"),
            name: name.expect("named above"),
            account: "away".to_string(),
            password: Secret::new(password),
            connection: DatabaseConnection::Plain,
            authorities: None,
        });
    }

    // a machine beside the server, for the tests that open a desktop: it is in a file of
    // the folder, with one that is not there, and a rule gives both to everyone of staff.
    // And two brokers of farms: one sends whoever signs in on to that machine, which the
    // files name for its collection, and one to a machine the files don't name
    let mut machine = None;
    if let Some(at) = said("MACHINE_PORT").and_then(|port| port.parse::<u16>().ok()) {
        let (account, password) = ("CORP\\alex.urban", "a password that exists for these tests alone");
        let stand_in =
            away_gateway::stand_in::Machine::start_at(&format!("127.0.0.1:{at}"), account, password, away_gateway::stand_in::desktop())
                .await;
        let sends_on = away_gateway::stand_in::Redirection {
            session: 7,
            address: Some("127.0.0.1".to_string()),
            name: Some("localhost".to_string()),
            ..Default::default()
        };
        let broker = away_gateway::stand_in::Machine::start_broker(account, password, sends_on).await;
        let (_, broker_at) = broker.address();
        let astray = away_gateway::stand_in::Redirection {
            session: 8,
            address: Some("10.20.0.7".to_string()),
            name: Some("rdsh-07.corp.example".to_string()),
            ..Default::default()
        };
        let lost = away_gateway::stand_in::Machine::start_broker(account, password, astray).await;
        let (_, lost_at) = lost.address();
        // two machines that speak VNC: a Mac, whose screen sharing asks for the password of
        // its desktop, and a desktop on Linux that asks for nothing
        let shows = away_gateway::stand_in::vnc::desktop;
        let mac = away_gateway::stand_in::vnc::Machine::start_a_mac_at("127.0.0.1:0", "a desktop", shows()).await;
        let tux = away_gateway::stand_in::vnc::Machine::start_open(shows()).await;
        let ((_, mac_at), (_, tux_at)) = (mac.address(), tux.address());
        let nothing = std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?.port();
        std::fs::write(
            std::path::Path::new(&files).join("desks.yaml"),
            format!(
                "machines:\n  - name: desk-01\n    kind: windows\n    address: localhost\n    groups: [desks]\n    desktop: {{ port: {at} }}\n  \
                 - name: gone-02\n    kind: windows\n    address: 127.0.0.1\n    groups: [desks]\n    desktop: {{ port: {nothing} }}\n  \
                 - name: apps-01\n    kind: windows\n    address: localhost\n    groups: [desks]\n    desktop: {{ port: {at} }}\n    \
                 programs:\n      - name: Books\n        run: \"||ledger\"\n      - Wages\n  \
                 - name: Farm Desk\n    kind: windows\n    address: localhost\n    groups: [desks]\n    desktop: {{ port: {broker_at} }}\n    \
                 collection: Desks\n    served-by: [localhost:{at}]\n  \
                 - name: Lost Desk\n    kind: windows\n    address: localhost\n    groups: [desks]\n    desktop: {{ port: {lost_at} }}\n    \
                 collection: Lost\n    served-by: [127.0.0.9]\n  \
                 - name: mac-01\n    kind: mac\n    address: localhost\n    groups: [desks]\n    desktop: {{ port: {mac_at} }}\n  \
                 - name: tux-02\n    kind: linux\n    address: localhost\n    groups: [desks]\n    desktop: {{ port: {tux_at} }}\n\
                 rules:\n  - name: staff-desks\n    who: [staff]\n    reaches: [desks]\n"
            ),
        )?;
        machine = Some((stand_in, broker, lost, mac, tux));
    }

    let told = Told {
        sign_in: Some(SignIn {
            public: Address::read(&away).expect("Away's address on this machine"),
            service: service::Settings {
                service: service_at,
                client: "away".to_string(),
                secret: Secret::new("a secret for this machine alone"),
                back,
                asks_for: "openid profile email".to_string(),
            },
            authorities: None,
            groups_entry: "groups".to_string(),
            administrators: vec!["away-admins".to_string()],
            record_readers: vec!["auditors".to_string()],
            looks_every: Duration::from_secs(5),
        }),
        contact: Some("it-help@example.com".to_string()),
        files: Some(files.into()),
        farms,
        database,
        internal: machine.as_ref().map(|_| Internal { address: "127.0.0.1:0".to_string(), certificate: None }),
    };

    let lines = Lines::to_output("server");
    let (stopper, stop) = stop::pair();
    tokio::spawn(stop::on_signal(stopper, lines.clone()));
    // with a machine to open, the server listens beside the gateway, and Away's address
    // passes a session's connection on to the gateway and everything else to the server
    let at = if machine.is_some() { "127.0.0.1:0".to_string() } else { format!("127.0.0.1:{port}") };
    let settings = Settings { address: at, health_address: "127.0.0.1:0".to_string() };
    let server = Server::bind(&settings, told, Part::new("server", lines.clone(), stop.clone())).await?;
    if machine.is_some() {
        let pages = server.bound().address.expect("the server's address");
        let internal = server.internal_address().expect("the address the gateway asks the server at");
        let told = away_gateway::Told {
            server: Some(Address::read(&format!("http://127.0.0.1:{}", internal.port())).expect("the server's address")),
            authorities: None,
            restricted_admin: None,
            domain: None,
            issuer: None,
        };
        let settings = Settings { address: "127.0.0.1:0".to_string(), health_address: "127.0.0.1:0".to_string() };
        let gateway = away_gateway::Gateway::bind(&settings, told, Part::new("gateway", Lines::to_output("gateway"), stop)).await?;
        let sessions = gateway.bound().address.expect("the gateway's address");
        tokio::spawn(gateway.run());
        let way_in = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}")).await?;
        tokio::spawn(async move {
            while let Ok((connection, _)) = way_in.accept().await {
                tokio::spawn(pass_on(connection, pages, sessions));
            }
        });
    }
    server.run().await;
    drop((farm, machine));
    Ok(())
}

/// Passes a connection on as a way in does: one that begins as a session's goes to the
/// gateway, and every other to the server.
async fn pass_on(mut from: tokio::net::TcpStream, pages: std::net::SocketAddr, sessions: std::net::SocketAddr) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    // the request's first line says where it is for
    let mut begins = Vec::new();
    while !begins.windows(2).any(|pair| pair == b"\r\n") && begins.len() < 8192 {
        let mut bytes = [0u8; 1024];
        match from.read(&mut bytes).await {
            Ok(count) if count > 0 => begins.extend_from_slice(&bytes[..count]),
            _ => return,
        }
    }
    let to = if begins.starts_with(b"GET /session ") { sessions } else { pages };
    let Ok(mut to) = tokio::net::TcpStream::connect(to).await else { return };
    if to.write_all(&begins).await.is_ok() {
        let _ = tokio::io::copy_bidirectional(&mut from, &mut to).await;
    }
}
