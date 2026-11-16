//! Signing a person in on a machine of a Windows domain as themselves: which account
//! their sign-in names, whether Away signs anybody in as that, and the certificate that
//! does it, which the organisation's own authority issues on Away's request
//! ([ADR-0007](../../../spec/architecture/decisions/0007-short-lived-certificates.md),
//! [ADR-0008](../../../spec/architecture/decisions/0008-the-authority-for-windows-sign-ins.md)).
//!
//! The key the certificate is for is made by whoever asks, and stays there: what comes
//! here is the request for a certificate, which holds the public half alone.

use std::sync::Arc;

use away_part::client::Client;
use away_part::json::Json;
use away_winauth::certificate::{Certificate, FOR_ASKING_FOR_OTHERS, FOR_SIGNING_IN};
use away_winauth::der::Reader;
use away_winauth::kerberos::pkinit::Held;
use away_winauth::request::{self, Key};

use crate::authority::{Authority, Why};
use crate::told;

/// How long what signs a person in may last from the moment it is handed out, in
/// seconds: five minutes.
pub const LASTS_AT_MOST: u64 = 300;
/// How far the authority's clock and Away's may be apart, in seconds.
const CLOCKS_APART: u64 = 60;
/// An agent's certificate that is good for less than this long is not asked with any
/// more, in seconds.
const TOO_SHORT: u64 = 60;

/// Why nobody was signed in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused {
    /// Which of the reasons it is, as one word.
    pub why: &'static str,
    /// The reason, in words for whoever is asked for help.
    pub says: String,
}

fn refused(why: &'static str, says: impl Into<String>) -> Refused {
    Refused { why, says: says.into() }
}

/// What signs a person in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Made {
    /// The certificate, as it is written.
    pub certificate: Vec<u8>,
    /// The account it is for, as the authority wrote it into the certificate.
    pub account: String,
    /// The number the authority gave it, in hexadecimal.
    pub number: String,
    /// Until when it is good, in seconds since the beginning of 1970.
    pub until: u64,
}

/// The account a person's sign-in names in the domain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    /// Its name, as the domain knows it and as the authority is asked for it.
    pub name: String,
    /// The name the person signs in with in the directory, where their sign-in gives
    /// that one: what the authority issues has to name exactly it.
    pub signs_in_as: Option<String>,
}

/// The certificate Away asks for others with, and its key.
struct Agent {
    held: Held,
    /// From when on a new one is asked for.
    renewed_from: u64,
    until: u64,
}

/// A Windows domain's machines, as the issuer signs people in on them.
pub struct Windows {
    told: told::Windows,
    authority: Authority,
    agent: tokio::sync::Mutex<Option<Arc<Agent>>>,
}

impl Windows {
    /// The domain of these settings. Nothing is asked of its authority yet.
    pub fn new(told: told::Windows) -> Result<Windows, String> {
        let client = Client::new(told.authorities.as_deref())
            .map_err(|why| format!("The authorities that secure the organisation's authority can't be used: {why}."))?;
        let realm = told.domain.to_ascii_uppercase();
        let authority = Authority::new(client, &told.pages, &told.controllers, &realm, &told.account, told.password.clone())?;
        Ok(Windows { told, authority, agent: tokio::sync::Mutex::new(None) })
    }

    /// The account a person's sign-in names in the domain, where it names one that
    /// Away signs people in as.
    ///
    /// The entry the installation named holds the account's name as the domain knows
    /// it, with or without the domain's short name before it, or the name the person
    /// signs in with in the directory, which has an `@` in it. What stands before that
    /// `@` need not be the account's name, and may be somebody else's: the authority is
    /// asked for the account of that name, and what it issues is taken only where it
    /// names exactly the name of the sign-in.
    pub fn account(&self, says: &Json) -> Result<Account, Refused> {
        let entry = &self.told.account_entry;
        let named = says
            .text_of(entry)
            .map(str::trim)
            .filter(|named| !named.is_empty())
            .ok_or_else(|| refused("no-account", format!("The person's sign-in names no account: it has no entry {entry}.")))?;
        let name = match named.split_once('\\') {
            Some((domain, name)) if domain.eq_ignore_ascii_case(&self.told.short_name) => name,
            Some((domain, _)) => {
                return Err(refused(
                    "no-account",
                    format!(
                        "The person's sign-in names an account of {domain}, and people are signed in on the machines of {}.",
                        self.told.short_name
                    ),
                ))
            }
            None => named,
        };
        // the name a person signs in with: an account's name, an @, and what such names
        // end in, which is written as a domain's name is
        let (name, signs_in_as) = match name.rsplit_once('@') {
            Some((before, after)) if !named.contains('\\') && ends_as_a_name_does(after) => (before, Some(named.to_string())),
            _ => (name, None),
        };
        let plain = !name.is_empty()
            && name.len() <= 64
            && !name.starts_with('.')
            && !name.ends_with('.')
            && name.chars().all(|letter| letter.is_alphanumeric() || matches!(letter, '.' | '-' | '_' | '$'));
        if !plain {
            return Err(refused(
                "no-account",
                format!(
                    "The entry {entry} of the person's sign-in holds {named}, which is no account's name as the domain {} knows one.",
                    self.told.short_name
                ),
            ));
        }
        if self.told.never_as.iter().any(|never| never.eq_ignore_ascii_case(name)) {
            return Err(refused("never", format!("Away signs nobody in as {name}.")));
        }
        // a group is named as it is, as the last part of a path, or as the first part of
        // what a directory calls it
        let groups = says.get(&self.told.groups_entry).and_then(Json::list).unwrap_or_default();
        for group in groups.iter().filter_map(Json::text) {
            let last = group.rsplit('/').next().unwrap_or(group);
            let first = group.strip_prefix("CN=").or(group.strip_prefix("cn=")).and_then(|rest| rest.split(',').next()).unwrap_or(group);
            let never = self.told.never_of.iter().find(|never| [group, last, first].iter().any(|named| never.eq_ignore_ascii_case(named)));
            if let Some(never) = never {
                return Err(refused("never", format!("Away signs nobody in on a machine as themselves who is in the group {never}.")));
            }
        }
        Ok(Account { name: name.to_string(), signs_in_as })
    }

    /// The certificate Away asks for others with: the one it has while that is good,
    /// and a new one from two thirds of its time on.
    async fn agent(&self, now: u64) -> Result<Arc<Agent>, Refused> {
        let mut kept = self.agent.lock().await;
        if let Some(agent) = kept.as_ref().filter(|agent| now < agent.renewed_from) {
            return Ok(agent.clone());
        }
        match self.new_agent(now).await {
            Ok(agent) => {
                let agent = Arc::new(agent);
                *kept = Some(agent.clone());
                Ok(agent)
            }
            // the one that is there serves for as long as it is good
            Err(why) => kept.as_ref().filter(|agent| agent.until > now + TOO_SHORT).cloned().ok_or(why),
        }
    }

    async fn new_agent(&self, now: u64) -> Result<Agent, Refused> {
        let could_not = |why: String| {
            refused("authority", format!("Away could not get the certificate it asks the organisation's authority for others with: {why}"))
        };
        let key = tokio::task::spawn_blocking(Key::new)
            .await
            .map_err(|error| could_not(error.to_string()))?
            .map_err(|error| could_not(error.to_string()))?;
        let asked = request::asked(&key, &self.told.account).map_err(|error| could_not(error.to_string()))?;
        let certificate = self.authority.ask(&asked, &self.told.agent_kind).await.map_err(|why| could_not(why.to_string()))?;
        let read = Certificate::read(&certificate).map_err(|error| could_not(error.to_string()))?;
        if !read.is_for(FOR_ASKING_FOR_OTHERS) || !read.is_good_at(now) {
            return Err(could_not(format!(
                "what the authority issues as {} is not good for asking for others, or is not good now.",
                self.told.agent_kind
            )));
        }
        let (renewed_from, until) = (now + read.until.saturating_sub(now) * 2 / 3, read.until);
        let held = Held::new(&certificate, &key.written().map_err(|error| could_not(error.to_string()))?)
            .map_err(|error| could_not(error.to_string()))?;
        Ok(Agent { held, renewed_from, until })
    }

    /// Whether Away has the certificate it asks for others with, asking for one where
    /// it has none: what the check of the setup looks at.
    pub async fn is_ready(&self, now: u64) -> Result<(), Refused> {
        self.agent(now).await.map(|_| ())
    }

    /// Makes what signs a person in: a certificate for the key of this request, for the
    /// account their sign-in names. `says` is what the sign-in service signed about the
    /// person, checked already.
    pub async fn sign_in(&self, says: &Json, asked: &[u8], now: u64) -> Result<Made, Refused> {
        let Account { name: account, signs_in_as } = self.account(says)?;
        let for_key = request::read(asked)
            .map_err(|error| refused("request", format!("What was asked with is no request for a certificate: {error}.")))?
            .key;
        let agent = self.agent(now).await?;
        let named = format!("{}\\{account}", self.told.short_name);
        let signed = request::in_the_name_of(asked, &named, &agent.held)
            .map_err(|error| refused("authority", format!("The request could not be signed in the name of {named}: {error}.")))?;
        let kind = &self.told.sign_in_kind;
        let certificate = match self.authority.ask(&signed, kind).await {
            Ok(certificate) => certificate,
            Err(Why::Refused(said)) => {
                return Err(refused(
                    "authority-refused",
                    format!("The organisation's authority refused to sign {named} in. It says: {said}"),
                ))
            }
            Err(Why::NotAsked(why)) => return Err(refused("authority", why)),
        };

        // what came back is looked at before it is handed on: it is for this key, for
        // this account, for signing in, and lasts minutes
        let unusable =
            |why: &str| refused("authority", format!("What the organisation's authority issued as {kind} for {named} is not taken: {why}"));
        let read = Certificate::read(&certificate).map_err(|error| unusable(&format!("{error}.")))?;
        if !read.key.is_some_and(|key| for_key.windows(key.len()).any(|part| part == key)) {
            return Err(unusable("it is for another key than the one that was asked with."));
        }
        if !read.is_for(FOR_SIGNING_IN) || !read.is_good_at(now) {
            return Err(unusable("it is not for signing in, or is not good now."));
        }
        let its = read.account.clone().unwrap_or_default();
        match &signs_in_as {
            // the name the person signs in with, whole: the account that was asked for
            // by what stands before the @ may be somebody else's
            Some(theirs) if its.to_lowercase() != theirs.to_lowercase() => {
                return Err(unusable(&format!("it names the account {its}, and the person's sign-in names {theirs}.")));
            }
            Some(_) => {}
            None => {
                let is_theirs = its
                    .split_once('@')
                    .is_some_and(|(name, domain)| name.eq_ignore_ascii_case(&account) && domain.eq_ignore_ascii_case(&self.told.domain));
                if !is_theirs {
                    return Err(unusable(&format!("it names the account {its}.")));
                }
            }
        }
        if read.until > now + LASTS_AT_MOST + CLOCKS_APART {
            return Err(refused(
                "lasts-too-long",
                format!(
                    "The organisation's authority issues {kind} for {} minutes from now, and what signs a person in lasts five at most.",
                    (read.until.saturating_sub(now) + 30) / 60
                ),
            ));
        }
        let number = Reader::new(read.number)
            .value()
            .map(|number| number.holds.iter().map(|byte| format!("{byte:02x}")).collect())
            .unwrap_or_default();
        Ok(Made { account: its, number, until: read.until, certificate })
    }
}

/// Whether this is written as what the names people sign in with end in: a domain's
/// name, of letters, digits and hyphens between dots.
fn ends_as_a_name_does(end: &str) -> bool {
    end.len() <= 255
        && end.split('.').all(|part| {
            !part.is_empty()
                && !part.starts_with('-')
                && !part.ends_with('-')
                && part.chars().all(|letter| letter.is_alphanumeric() || letter == '-')
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use away_secret::Secret;

    fn windows() -> Windows {
        Windows::new(told::Windows {
            domain: "corp.example".to_string(),
            short_name: "CORP".to_string(),
            controllers: vec!["dc-01.corp.example".to_string()],
            pages: "https://authority.corp.example/certsrv/".to_string(),
            authorities: None,
            account: "svc-away".to_string(),
            password: Secret::new("a password"),
            agent_kind: "CorpAgent".to_string(),
            sign_in_kind: "CorpSignIn".to_string(),
            account_entry: told::ACCOUNT_ENTRY.to_string(),
            groups_entry: told::GROUPS_ENTRY.to_string(),
            never_as: told::NEVER_AS.iter().map(|never| never.to_string()).collect(),
            never_of: told::NEVER_OF.iter().map(|never| never.to_string()).collect(),
        })
        .expect("a domain")
    }

    fn says(account: Json, groups: &[&str]) -> Json {
        Json::object([
            ("sub", "7c1f".into()),
            ("preferred_username", account),
            ("groups", Json::List(groups.iter().map(|group| (*group).into()).collect())),
        ])
    }

    #[test]
    fn administrative_policy_binds_subject_account_and_every_destination() {
        let mut windows = windows();
        let about = says("alex.urban".into(), &["Domain Admins"]);
        assert_eq!(windows.account(&about).unwrap_err().why, "never");
        windows.told.never_of.clear();
        let policy = crate::administrators::Policy::read(
            r#"{"7c1f":"alex.urban@corp.example"}"#,
            "host.corp.example,broker.corp.example",
            "corp.example",
        )
        .unwrap();
        let approved = vec!["host.corp.example:3389".into(), "broker.corp.example".into()];
        assert!(policy.authorize(&about, &approved, &windows).is_ok());
        for targets in [
            vec![],
            vec!["other.corp.example".into()],
            vec!["host.corp.example:3390".into()],
            vec!["host.corp.example".into(), "other.corp.example".into()],
        ] {
            assert!(policy.authorize(&about, &targets, &windows).is_err());
        }
        for name in ["sam.other", "alex.urban@other.example", "Administrator", "krbtgt"] {
            assert!(policy.authorize(&says(name.into(), &[]), &approved, &windows).is_err());
        }
        let mut other = about.clone();
        if let Json::Object(fields) = &mut other {
            fields.iter_mut().find(|(key, _)| key == "sub").unwrap().1 = "another-subject".into();
        }
        assert!(policy.authorize(&other, &approved, &windows).is_err());
    }

    #[test]
    fn the_account_is_what_the_entry_the_installation_named_holds() {
        let windows = windows();
        for (named, account) in [
            ("alex.urban", "alex.urban"),
            (" alex.urban ", "alex.urban"),
            ("CORP\\alex.urban", "alex.urban"),
            ("corp\\Alex.Urban", "Alex.Urban"),
            ("desk-01$", "desk-01$"),
            ("zoë_k", "zoë_k"),
        ] {
            let named_so = windows.account(&says(named.into(), &["staff"]));
            assert_eq!(named_so, Ok(Account { name: account.to_string(), signs_in_as: None }), "{named}");
        }
        // the name a person signs in with: the account is asked for by what stands
        // before the @, and the name is kept whole for what the authority issues
        for (named, account) in [
            ("alex.urban@corp.example", "alex.urban"),
            ("Alex.Urban@CORP.EXAMPLE", "Alex.Urban"),
            ("alex.urban@firm.example", "alex.urban"),
            (" zoë_k@corp.example ", "zoë_k"),
        ] {
            let named_so = windows.account(&says(named.into(), &["staff"]));
            assert_eq!(named_so, Ok(Account { name: account.to_string(), signs_in_as: Some(named.trim().to_string()) }), "{named}");
        }
        // a name that is none, or of another domain
        for (named, says_so) in [
            ("", "The person's sign-in names no account: it has no entry preferred_username."),
            ("OTHER\\alex.urban", "The person's sign-in names an account of OTHER, and people are signed in on the machines of CORP."),
            ("@corp.example", "The entry preferred_username of the person's sign-in holds @corp.example, which is no account's name as the domain CORP knows one."),
            ("alex.urban@", "The entry preferred_username of the person's sign-in holds alex.urban@, which is no account's name as the domain CORP knows one."),
            (
                "alex@urban@corp.example",
                "The entry preferred_username of the person's sign-in holds alex@urban@corp.example, which is no account's name as the domain CORP knows one.",
            ),
            (
                "alex.urban@corp..example",
                "The entry preferred_username of the person's sign-in holds alex.urban@corp..example, which is no account's name as the domain CORP knows one.",
            ),
            (
                "CORP\\alex.urban@corp.example",
                "The entry preferred_username of the person's sign-in holds CORP\\alex.urban@corp.example, which is no account's name as the domain CORP knows one.",
            ),
            ("alex urban", "The entry preferred_username of the person's sign-in holds alex urban, which is no account's name as the domain CORP knows one."),
            ("CORP\\", "The entry preferred_username of the person's sign-in holds CORP\\, which is no account's name as the domain CORP knows one."),
            (".alex", "The entry preferred_username of the person's sign-in holds .alex, which is no account's name as the domain CORP knows one."),
        ] {
            assert_eq!(windows.account(&says(named.into(), &[])), Err(refused("no-account", says_so)), "{named}");
        }
        assert_eq!(windows.account(&says(Json::Null, &[])).unwrap_err().why, "no-account");
        assert_eq!(windows.account(&says(7u64.into(), &[])).unwrap_err().why, "no-account");
    }

    #[test]
    fn nobody_is_signed_in_as_an_account_or_out_of_a_group_that_is_never_signed_in() {
        let windows = windows();
        for never in ["Administrator", "CORP\\administrator", "krbtgt", "GUEST", "Administrator@corp.example", "krbtgt@CORP.EXAMPLE"] {
            let refused = windows.account(&says(never.into(), &["staff"])).unwrap_err();
            assert_eq!((refused.why, refused.says.starts_with("Away signs nobody in as ")), ("never", true), "{never}");
        }
        // a group is named as it is, as the end of a path, or as a directory calls it
        for group in [
            "Domain Admins",
            "/population/Domain Admins",
            "domain admins",
            "CN=Enterprise Admins,CN=Users,DC=corp,DC=example",
            "Administrators",
        ] {
            let refused = windows.account(&says("alex.urban".into(), &["staff", group])).unwrap_err();
            assert_eq!(refused.why, "never", "{group}");
            assert!(refused.says.starts_with("Away signs nobody in on a machine as themselves who is in the group "), "{}", refused.says);
        }
        // a group that only sounds like one is none of them
        for group in ["Domain Admins of the canteen", "/Domain Admins/visitors", "Not Administrators"] {
            assert_eq!(
                windows.account(&says("alex.urban".into(), &[group])).map(|account| account.name).as_deref(),
                Ok("alex.urban"),
                "{group}"
            );
        }
    }

    #[tokio::test]
    async fn a_request_that_is_none_and_a_person_who_is_never_signed_in_ask_the_authority_nothing() {
        // the authority of these settings is nowhere: whoever reaches for it fails otherwise
        let windows = windows();
        let never = windows.sign_in(&says("administrator".into(), &[]), b"a request", 1_800_000_000).await.unwrap_err();
        assert_eq!(never.why, "never");
        let none = windows.sign_in(&says("alex.urban".into(), &[]), b"a request", 1_800_000_000).await.unwrap_err();
        assert_eq!(none.why, "request");
    }
}
