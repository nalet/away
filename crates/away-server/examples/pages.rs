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

use std::time::Duration;

use away_part::address::Address;
use away_part::{stop, Lines, Part, Settings};
use away_secret::Secret;
use away_server::service;
use away_server::stand_in::farm::Farm;
use away_server::stand_in::{person, SignInService};
use away_server::told::{Database, DatabaseConnection, Farms, SignIn, Told};
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
        internal: None,
    };

    let lines = Lines::to_output("server");
    let (stopper, stop) = stop::pair();
    tokio::spawn(stop::on_signal(stopper, lines.clone()));
    let settings = Settings { address: format!("127.0.0.1:{port}"), health_address: "127.0.0.1:0".to_string() };
    let server = Server::bind(&settings, told, Part::new("server", lines, stop)).await?;
    server.run().await;
    drop(farm);
    Ok(())
}
