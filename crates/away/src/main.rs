//! Away, the program. Its first word chooses the part it runs as:
//!
//! ```text
//! away server     the pages, the requests, people's sign-ins, the rules, the records
//! away gateway    the sessions
//! away issuer     the sign-ins on machines
//! ```
//!
//! A part runs until the cluster ends it, and writes what happens as lines on its output.

use std::process::ExitCode;

use away_gateway::Gateway;
use away_issuer::Issuer;
use away_part::{stop, version, Lines, Part, Settings};
use away_server::told::Told;
use away_server::Server;

const USAGE: &str = "\
away <part>

  server     the pages, the requests, people's sign-ins, the rules, the records
  gateway    the sessions
  issuer     the sign-ins on machines

  version    the version that this is
  help       this

A part reads where to listen from its environment:
  AWAY_ADDRESS           where it answers what it is there for
                         (server 0.0.0.0:8080, gateway 0.0.0.0:8081; the issuer has none)
  AWAY_HEALTH_ADDRESS    where it says whether it is well and gives its measures
                         (0.0.0.0:9090)

The server also reads what an installation tells it, which the chart sets from its
values: the address people reach Away at, the sign-in service, who administers, and
whom people turn to. The gateway reads where the server is. docs/install.md has each.
";

fn main() -> ExitCode {
    let words: Vec<String> = std::env::args().skip(1).collect();
    let word = match words.as_slice() {
        [word] => word.as_str(),
        [] => {
            eprint!("{USAGE}");
            return ExitCode::from(2);
        }
        _ => {
            eprintln!("away takes one word, and got {}.\n\n{USAGE}", words.len());
            return ExitCode::from(2);
        }
    };
    let part = match word {
        "server" => "server",
        "gateway" => "gateway",
        "issuer" => "issuer",
        "version" | "--version" | "-V" => {
            println!("away {} ({})", version::VERSION, version::COMMIT);
            return ExitCode::SUCCESS;
        }
        "help" | "--help" | "-h" => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        other => {
            eprintln!("away knows no part named {other}.\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let lines = Lines::to_output(part);
    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            lines.error("could not start", &[("why", (&error.to_string()).into())]);
            return ExitCode::FAILURE;
        }
    };
    runtime.block_on(run(part, lines))
}

/// Runs one part until it is told to stop.
async fn run(name: &'static str, lines: Lines) -> ExitCode {
    lines.info("starting", &[("version", version::VERSION.into()), ("commit", version::COMMIT.into())]);
    let (stopper, stop) = stop::pair();
    tokio::spawn(stop::on_signal(stopper, lines.clone()));
    let part = Part::new(name, lines, stop);
    let from_environment = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
    // a part that couldn't listen has said why in a line of its own
    let ran = match name {
        "server" => {
            let settings = Settings::with_address(away_server::ADDRESS).from_environment(from_environment);
            let told = match Told::read(from_environment, |path| std::fs::read(path)) {
                Ok(told) => told,
                Err(why) => {
                    part.lines.error("could not start", &[("why", (&why).into())]);
                    return ExitCode::FAILURE;
                }
            };
            match Server::bind(&settings, told, part).await {
                Ok(server) => {
                    server.run().await;
                    true
                }
                Err(_) => false,
            }
        }
        "gateway" => {
            let settings = Settings::with_address(away_gateway::ADDRESS).from_environment(from_environment);
            let told = match away_gateway::Told::read(from_environment, |path| std::fs::read(path)) {
                Ok(told) => told,
                Err(why) => {
                    part.lines.error("could not start", &[("why", (&why).into())]);
                    return ExitCode::FAILURE;
                }
            };
            match Gateway::bind(&settings, told, part).await {
                Ok(gateway) => {
                    gateway.run().await;
                    true
                }
                Err(_) => false,
            }
        }
        _ => {
            let settings = Settings::with_address("").from_environment(from_environment);
            let told = match away_issuer::Told::read(from_environment, |path| std::fs::read(path)) {
                Ok(told) => told,
                Err(why) => {
                    part.lines.error("could not start", &[("why", (&why).into())]);
                    return ExitCode::FAILURE;
                }
            };
            match Issuer::bind(&settings, told, part).await {
                Ok(issuer) => {
                    issuer.run().await;
                    true
                }
                Err(_) => false,
            }
        }
    };
    if ran {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
