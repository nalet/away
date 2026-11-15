//! Reads a farm's feed with Away's own code and says what the farm publishes: for the
//! people who build Away, to try a real farm with.
//!
//! ```sh
//! cargo run --package away-server --example farm -- https://broker.corp.example/RDWeb/Feed/webfeed.aspx \
//!   --as 'CORP\svc-away' --password-file ~/.config/away/password [--authorities authority.pem]
//! ```
//!
//! The password is read from a file, so that it is on no command line. Nothing is
//! printed of the account but its name.

use away_part::address::Address;
use away_part::client::Client;
use away_rdp::ntlm::Fresh;
use away_secret::Secret;
use away_server::farm::reader::Reader;
use away_server::register::Farm;

const USAGE: &str = "farm <feed> --as <account> --password-file <file> [--authorities <file>]\n";

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let words: Vec<String> = std::env::args().skip(1).collect();
    let after = |name: &str| words.iter().position(|word| word == name).and_then(|at| words.get(at + 1)).cloned();
    let (Some(feed), Some(account), Some(password_file)) =
        (words.first().filter(|word| !word.starts_with('-')), after("--as"), after("--password-file"))
    else {
        eprint!("{USAGE}");
        return std::process::ExitCode::from(2);
    };
    let tried = async {
        Address::read(feed).map_err(|why| format!("The feed's address can't be used: {why}."))?;
        let password = std::fs::read_to_string(&password_file).map_err(|error| format!("The password's file can't be read: {error}."))?;
        let authorities = after("--authorities")
            .map(std::fs::read)
            .transpose()
            .map_err(|error| format!("The authorities' file can't be read: {error}."))?;
        let client = Client::new(authorities.as_deref())?;
        let reader = Reader::new(client, &account, Secret::new(password.trim_end_matches(['\r', '\n'])));
        let mut random = [0u8; 24];
        getrandom::fill(&mut random).map_err(|error| format!("The system gave no random bytes: {error}."))?;
        let fresh = Fresh { challenge: random[..8].try_into().expect("8 bytes"), key: random[8..].try_into().expect("16 bytes") };
        let clock = (away_part::time::now().as_secs() + 11_644_473_600) * 10_000_000;
        let farm = Farm {
            name: "the farm".to_string(),
            feed: feed.clone(),
            groups: Vec::new(),
            served_by: Vec::new(),
            themselves: false,
            clipboard: away_server::register::Clipboard::BOTH,
            after: 0,
            written_in: (String::new(), 0),
        };
        println!("Reading {feed} as {account}");
        reader.read(&farm, &[], clock, fresh).await
    }
    .await;
    match tried {
        Ok(items) => {
            let desktops = items.iter().filter(|item| item.resource.desktop).count();
            println!("   It publishes {desktops} desktops and {} programs.", items.len() - desktops);
            for item in &items {
                println!(
                    "   {} | {} | at {} | collection {} | asked for as {} | icon {}",
                    if item.resource.desktop { "desktop" } else { "program" },
                    item.resource.title,
                    item.reached.address.as_deref().unwrap_or("-"),
                    item.reached.collection.as_deref().unwrap_or("-"),
                    item.reached.program.as_deref().unwrap_or("-"),
                    item.icon.as_ref().map_or("none".to_string(), |icon| format!("{} bytes", icon.len())),
                );
            }
            std::process::ExitCode::SUCCESS
        }
        Err(why) => {
            println!("   {why}");
            std::process::ExitCode::FAILURE
        }
    }
}
