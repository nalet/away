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
//! `ROLE=person` signs in somebody who administers nothing.

use std::time::Duration;

use away_part::address::Address;
use away_part::json::Json;
use away_part::{stop, Lines, Part, Settings};
use away_secret::Secret;
use away_server::service;
use away_server::stand_in::SignInService;
use away_server::told::{SignIn, Told};
use away_server::Server;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let said = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
    let port = said("PORT").and_then(|port| port.parse::<u16>().ok()).unwrap_or(8080);
    let away = format!("http://127.0.0.1:{port}");
    let back = format!("{away}/sign-in/back");

    let sign_in_service = SignInService::start("away", "a secret for this machine alone", &back).await;
    if let Some(seconds) = said("GOOD_FOR").and_then(|seconds| seconds.parse().ok()) {
        sign_in_service.signs_for(seconds);
    }
    if said("ROLE").as_deref() == Some("person") {
        sign_in_service.signs_in(Json::object([
            ("sub", "9d2e".into()),
            ("name", "Robin Meyer".into()),
            ("email", "robin.meyer@example.com".into()),
            ("groups", Json::List(vec!["staff".into()])),
        ]));
    }

    let told = Told {
        sign_in: Some(SignIn {
            public: Address::read(&away).expect("Away's address on this machine"),
            service: service::Settings {
                service: sign_in_service.address(),
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
    };

    let lines = Lines::to_output("server");
    let (stopper, stop) = stop::pair();
    tokio::spawn(stop::on_signal(stopper, lines.clone()));
    let settings = Settings { address: format!("127.0.0.1:{port}"), health_address: "127.0.0.1:0".to_string() };
    let server = Server::bind(&settings, told, Part::new("server", lines, stop)).await?;
    server.run().await;
    Ok(())
}
