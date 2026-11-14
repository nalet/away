//! What an installation tells the server: the address people reach Away at, the sign-in
//! service, who administers, and whom people turn to. The chart sets each from its
//! values, as a variable of the environment; a secret is named by the file it is in.
//!
//! | Variable | Says |
//! |---|---|
//! | `AWAY_PUBLIC_ADDRESS` | the address people reach Away at: `https://away.example.com` |
//! | `AWAY_SIGN_IN_SERVICE` | the sign-in service, as it names itself |
//! | `AWAY_SIGN_IN_CLIENT` | the name the sign-in service knows Away by |
//! | `AWAY_SIGN_IN_SECRET_FILE` | the file that holds the secret Away proves itself with |
//! | `AWAY_SIGN_IN_AUTHORITIES_FILE` | certificates of authorities to trust besides the public ones, as PEM |
//! | `AWAY_SIGN_IN_ASKS_FOR` | what the service is asked to say of a person (`openid profile email`) |
//! | `AWAY_SIGN_IN_GROUPS` | the entry of a sign-in that names the person's groups (`groups`) |
//! | `AWAY_ADMINISTRATORS` | the groups whose people administer Away, with commas between |
//! | `AWAY_RECORD_READERS` | the groups whose people read its records |
//! | `AWAY_CONTACT` | whom people turn to when something fails |

use std::time::Duration;

use away_part::address::Address;
use away_secret::Secret;

use crate::service;

/// What the server is told about signing in.
#[derive(Clone, Debug)]
pub struct SignIn {
    /// The address people reach Away at.
    pub public: Address,
    /// The sign-in service, and Away as its client.
    pub service: service::Settings,
    /// Certificates of authorities to trust besides the public ones, as PEM.
    pub authorities: Option<Vec<u8>>,
    /// The entry of a sign-in that names the person's groups.
    pub groups_entry: String,
    /// The groups whose people administer Away.
    pub administrators: Vec<String>,
    /// The groups whose people read its records.
    pub record_readers: Vec<String>,
    /// How often the server looks for sign-ins to renew.
    pub looks_every: Duration,
}

/// Everything the server is told.
#[derive(Clone, Debug, Default)]
pub struct Told {
    /// About signing in. Without it nobody can sign in, and the page says so.
    pub sign_in: Option<SignIn>,
    /// Whom people turn to when something fails.
    pub contact: Option<String>,
}

impl Told {
    /// Reads what the environment says. A file is read with the function given.
    pub fn read(variable: impl Fn(&str) -> Option<String>, file: impl Fn(&str) -> std::io::Result<Vec<u8>>) -> Result<Told, String> {
        let contact = variable("AWAY_CONTACT");
        let needed = ["AWAY_PUBLIC_ADDRESS", "AWAY_SIGN_IN_SERVICE", "AWAY_SIGN_IN_CLIENT", "AWAY_SIGN_IN_SECRET_FILE"];
        let said: Vec<Option<String>> = needed.iter().map(|name| variable(name)).collect();
        if said.iter().all(Option::is_none) {
            return Ok(Told { sign_in: None, contact });
        }
        if let Some(missing) = needed.iter().zip(&said).find(|(_, said)| said.is_none()).map(|(name, _)| name) {
            return Err(format!("{missing} is not set, and signing in needs it."));
        }
        let [public, service, client, secret_file] = [0, 1, 2, 3].map(|nth| said[nth].clone().unwrap_or_default());

        let public = Address::read(&public).map_err(|why| format!("AWAY_PUBLIC_ADDRESS is no address: {why}."))?;
        if public.path != "/" {
            return Err("AWAY_PUBLIC_ADDRESS ends with its host: Away answers on an address of its own, not below a path.".to_string());
        }
        Address::read(&service).map_err(|why| format!("AWAY_SIGN_IN_SERVICE is no address: {why}."))?;
        let read = |name: &str, path: &str| file(path).map_err(|error| format!("{name} names {path}, which can't be read: {error}."));
        let secret = String::from_utf8(read("AWAY_SIGN_IN_SECRET_FILE", &secret_file)?)
            .map_err(|_| "AWAY_SIGN_IN_SECRET_FILE names a file that holds no text.".to_string())?;
        let secret = secret.trim_end_matches(['\r', '\n']);
        if secret.is_empty() {
            return Err("AWAY_SIGN_IN_SECRET_FILE names a file that is empty.".to_string());
        }
        let authorities = variable("AWAY_SIGN_IN_AUTHORITIES_FILE").map(|path| read("AWAY_SIGN_IN_AUTHORITIES_FILE", &path)).transpose()?;
        let groups = |name: &str| -> Vec<String> {
            variable(name).unwrap_or_default().split(',').map(str::trim).filter(|group| !group.is_empty()).map(str::to_string).collect()
        };
        Ok(Told {
            sign_in: Some(SignIn {
                service: service::Settings {
                    service,
                    client,
                    secret: Secret::new(secret),
                    back: format!("{}/sign-in/back", public.origin()),
                    asks_for: variable("AWAY_SIGN_IN_ASKS_FOR").unwrap_or_else(|| "openid profile email".to_string()),
                },
                public,
                authorities,
                groups_entry: variable("AWAY_SIGN_IN_GROUPS").unwrap_or_else(|| "groups".to_string()),
                administrators: groups("AWAY_ADMINISTRATORS"),
                record_readers: groups("AWAY_RECORD_READERS"),
                looks_every: Duration::from_secs(15),
            }),
            contact,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn told(pairs: &[(&str, &str)]) -> Result<Told, String> {
        let pairs: Vec<(String, String)> = pairs.iter().map(|(name, value)| (name.to_string(), value.to_string())).collect();
        Told::read(
            |name| pairs.iter().find(|(its, _)| its == name).map(|(_, value)| value.clone()),
            |path| match path {
                "/secrets/sign-in" => Ok(b"s3cret\n".to_vec()),
                "/secrets/empty" => Ok(b"\n".to_vec()),
                "/secrets/authorities" => Ok(b"-----BEGIN CERTIFICATE-----".to_vec()),
                _ => Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no such file")),
            },
        )
    }

    const WHOLE: [(&str, &str); 4] = [
        ("AWAY_PUBLIC_ADDRESS", "https://away.example.com"),
        ("AWAY_SIGN_IN_SERVICE", "https://sign-in.example.com/realms/staff"),
        ("AWAY_SIGN_IN_CLIENT", "away"),
        ("AWAY_SIGN_IN_SECRET_FILE", "/secrets/sign-in"),
    ];

    #[test]
    fn told_nothing_nobody_signs_in_and_the_server_still_runs() {
        let told = told(&[("AWAY_CONTACT", "it-help@example.com")]).unwrap();
        assert!(told.sign_in.is_none());
        assert_eq!(told.contact.as_deref(), Some("it-help@example.com"));
    }

    #[test]
    fn told_everything_it_knows_the_service_and_where_people_come_back_to() {
        let mut pairs = WHOLE.to_vec();
        pairs.extend([
            ("AWAY_ADMINISTRATORS", "away-admins, platform"),
            ("AWAY_RECORD_READERS", ""),
            ("AWAY_SIGN_IN_GROUPS", "roles"),
            ("AWAY_SIGN_IN_AUTHORITIES_FILE", "/secrets/authorities"),
        ]);
        let sign_in = told(&pairs).unwrap().sign_in.unwrap();
        assert_eq!(sign_in.service.service, "https://sign-in.example.com/realms/staff");
        assert_eq!(sign_in.service.client, "away");
        assert_eq!(sign_in.service.secret.expose(), "s3cret", "without the line's end");
        assert_eq!(sign_in.service.back, "https://away.example.com/sign-in/back");
        assert_eq!(sign_in.service.asks_for, "openid profile email");
        assert_eq!(sign_in.groups_entry, "roles");
        assert_eq!(sign_in.administrators, ["away-admins", "platform"]);
        assert!(sign_in.record_readers.is_empty());
        assert!(sign_in.authorities.is_some());
        assert!(!format!("{sign_in:?}").contains("s3cret"), "a secret is never printed");
    }

    #[test]
    fn told_half_of_it_or_something_wrong_it_says_what() {
        let without = |name: &str| WHOLE.iter().filter(|(its, _)| *its != name).copied().collect::<Vec<_>>();
        let with = |name: &'static str, value: &'static str| {
            let mut pairs = without(name);
            pairs.push((name, value));
            pairs
        };
        let why = |pairs: Vec<(&str, &str)>| told(&pairs).unwrap_err();
        assert_eq!(why(without("AWAY_SIGN_IN_CLIENT")), "AWAY_SIGN_IN_CLIENT is not set, and signing in needs it.");
        assert_eq!(why(without("AWAY_PUBLIC_ADDRESS")), "AWAY_PUBLIC_ADDRESS is not set, and signing in needs it.");
        assert_eq!(
            why(with("AWAY_PUBLIC_ADDRESS", "http://away.example.com")),
            "AWAY_PUBLIC_ADDRESS is no address: an address that is not on this machine begins with https://."
        );
        assert!(why(with("AWAY_PUBLIC_ADDRESS", "https://example.com/away")).starts_with("AWAY_PUBLIC_ADDRESS ends with its host"));
        assert!(why(with("AWAY_SIGN_IN_SERVICE", "sign-in.example.com")).starts_with("AWAY_SIGN_IN_SERVICE is no address: "));
        assert_eq!(
            why(with("AWAY_SIGN_IN_SECRET_FILE", "/secrets/none")),
            "AWAY_SIGN_IN_SECRET_FILE names /secrets/none, which can't be read: no such file."
        );
        assert_eq!(why(with("AWAY_SIGN_IN_SECRET_FILE", "/secrets/empty")), "AWAY_SIGN_IN_SECRET_FILE names a file that is empty.");
        let mut pairs = WHOLE.to_vec();
        pairs.push(("AWAY_SIGN_IN_AUTHORITIES_FILE", "/secrets/none"));
        assert!(why(pairs).starts_with("AWAY_SIGN_IN_AUTHORITIES_FILE names /secrets/none, which can't be read"));
    }
}
