//! What an installation tells the issuer: whether people are signed in on the machines
//! of a Windows domain as themselves, and with what
//! ([ADR-0008](../../../spec/architecture/decisions/0008-the-authority-for-windows-sign-ins.md)).
//!
//! An installation that names no domain tells the issuer nothing of this, and the
//! issuer then signs nobody in. An installation that keeps secrets for machines names
//! the folder they are in, with or without a domain.

use away_part::address::Address;
use away_secret::Secret;

/// Which entry of a person's sign-in names their account, unless the installation says.
pub const ACCOUNT_ENTRY: &str = "preferred_username";
/// Which entry of a person's sign-in names their groups, unless the installation says.
pub const GROUPS_ENTRY: &str = "groups";
/// The accounts nobody is signed in as, unless the installation says.
pub const NEVER_AS: [&str; 3] = ["administrator", "krbtgt", "guest"];
/// The groups whose people are signed in as themselves on no machine, unless the
/// installation says: those of a directory's administrators.
pub const NEVER_OF: [&str; 4] = ["Domain Admins", "Enterprise Admins", "Schema Admins", "Administrators"];

/// Where Away's other parts ask the issuer, and what secures that.
#[derive(Clone, Debug)]
pub struct Internal {
    /// The address, such as `0.0.0.0:8083`.
    pub address: String,
    /// The certificate that secures it and the certificate's key, both as PEM. Without
    /// them it is not secured, which is for parts on one machine.
    pub certificate: Option<(Vec<u8>, Vec<u8>)>,
}

/// The sign-in service, as the issuer checks what it signed.
#[derive(Clone, Debug)]
pub struct SignInService {
    /// The service, as it names itself.
    pub service: String,
    /// The name the service knows Away by.
    pub client: String,
    /// The authorities that secure it, as PEM, beside the public ones.
    pub authorities: Option<Vec<u8>>,
}

/// The server, as the issuer asks it.
#[derive(Clone, Debug)]
pub struct Server {
    /// Where it answers Away's other parts.
    pub at: Address,
    /// The authority that secures that address, as PEM.
    pub authorities: Option<Vec<u8>>,
}

/// A Windows domain whose machines people are signed in on as themselves, and the
/// organisation's own authority that issues what signs them in.
#[derive(Clone, Debug)]
pub struct Windows {
    /// The domain, as it is named in full: `corp.example`.
    pub domain: String,
    /// The domain's short name, which accounts are written with before a backslash: `CORP`.
    pub short_name: String,
    /// The domain controllers that give tickets, each with its port where that is not
    /// Kerberos' own.
    pub controllers: Vec<String>,
    /// The pages the authority publishes for asking it: `https://authority.corp.example/certsrv/`.
    pub pages: String,
    /// The authorities that secure those pages, as PEM, beside the public ones.
    pub authorities: Option<Vec<u8>>,
    /// The account the organisation gave Away to ask its authority as, and its password.
    pub account: String,
    /// That account's password.
    pub password: Secret,
    /// The authority's name for the kind of certificate that allows asking for others.
    pub agent_kind: String,
    /// The authority's name for the kind of certificate that signs a person in.
    pub sign_in_kind: String,
    /// Which entry of a person's sign-in names their account in the domain.
    pub account_entry: String,
    /// Which entry of a person's sign-in names their groups.
    pub groups_entry: String,
    /// The accounts nobody is signed in as.
    pub never_as: Vec<String>,
    /// The groups whose people are signed in as themselves on no machine.
    pub never_of: Vec<String>,
}

/// What an installation tells the issuer.
#[derive(Clone, Debug, Default)]
pub struct Told {
    /// A separate, explicitly enabled administrative enrollment authority.
    pub administrators: Option<crate::administrators::Administrator>,
    /// Where the gateway asks the issuer. Without it nothing is asked of it.
    pub internal: Option<Internal>,
    /// The Windows domain, where the installation names one.
    pub windows: Option<Windows>,
    /// The sign-in service. A domain needs it.
    pub sign_in: Option<SignInService>,
    /// The server. A domain needs it.
    pub server: Option<Server>,
    /// The folder of the secrets Away keeps for machines, where it keeps any: each a
    /// file of its own, named as a machine's entry names it. It needs the sign-in
    /// service and the server.
    pub kept: Option<String>,
}

fn list(said: Option<String>, otherwise: &[&str]) -> Vec<String> {
    match said {
        Some(said) => said.split(',').map(str::trim).filter(|one| !one.is_empty()).map(str::to_string).collect(),
        None => otherwise.iter().map(|one| one.to_string()).collect(),
    }
}

impl Told {
    /// Reads what the environment says. A file is read with the function given.
    pub fn read(variable: impl Fn(&str) -> Option<String>, file: impl Fn(&str) -> std::io::Result<Vec<u8>>) -> Result<Told, String> {
        let said = |name: &str| variable(name).map(|said| said.trim().to_string()).filter(|said| !said.is_empty());
        let read = |name: &str| -> Result<Option<Vec<u8>>, String> {
            said(name).map(|path| file(&path).map_err(|error| format!("{name} names {path}, which can't be read: {error}."))).transpose()
        };
        let internal = match said("AWAY_INTERNAL_ADDRESS") {
            None => None,
            Some(address) => {
                let certificate = match (read("AWAY_INTERNAL_CERTIFICATE_FILE")?, read("AWAY_INTERNAL_KEY_FILE")?) {
                    (Some(certificate), Some(key)) => Some((certificate, key)),
                    (None, None) => None,
                    _ => {
                        return Err(
                            "AWAY_INTERNAL_CERTIFICATE_FILE and AWAY_INTERNAL_KEY_FILE go together: one of them is not set.".to_string()
                        )
                    }
                };
                Some(Internal { address, certificate })
            }
        };
        let kept = said("AWAY_KEPT_SECRETS");
        // the issuer checks a person's sign-in itself, and asks the server whose a session
        // is: for whatever it hands out, a sign-in on a domain's machine or a kept secret
        let checks = |what: &str| -> Result<(SignInService, Server), String> {
            let needed = |name: &str| said(name).ok_or_else(|| format!("{name} is not set, and {what} needs it."));
            let service = needed("AWAY_SIGN_IN_SERVICE")?;
            Address::read(&service).map_err(|why| format!("AWAY_SIGN_IN_SERVICE is no address: {why}."))?;
            let sign_in =
                SignInService { service, client: needed("AWAY_SIGN_IN_CLIENT")?, authorities: read("AWAY_SIGN_IN_AUTHORITIES_FILE")? };
            let at = Address::read(&needed("AWAY_SERVER")?).map_err(|why| format!("AWAY_SERVER is no address: {why}."))?;
            let server = Server { at, authorities: read("AWAY_SERVER_AUTHORITIES_FILE")? };
            if internal.is_none() {
                return Err(format!("AWAY_INTERNAL_ADDRESS is not set, and {what} needs it: the gateway asks there."));
            }
            Ok((sign_in, server))
        };
        let Some(domain) = said("AWAY_DOMAIN") else {
            if said("AWAY_ADMIN_CERTIFICATE_ENABLED").as_deref().is_some_and(|value| value != "false") {
                return Err("Administrative certificate SSO requires AWAY_DOMAIN.".into());
            }
            // no domain: whatever else is said of one is said by mistake
            for name in ["AWAY_AUTHORITY", "AWAY_AUTHORITY_ACCOUNT", "AWAY_DOMAIN_CONTROLLERS", "AWAY_DOMAIN_SHORT_NAME"] {
                if said(name).is_some() {
                    return Err(format!(
                        "{name} is set and AWAY_DOMAIN is not: people are signed in as themselves on the machines of a domain."
                    ));
                }
            }
            let Some(kept) = kept else { return Ok(Told { internal, ..Told::default() }) };
            let (sign_in, server) = checks("handing out the secrets Away keeps for machines")?;
            return Ok(Told {
                internal: internal.clone(),
                sign_in: Some(sign_in),
                server: Some(server),
                kept: Some(kept),
                ..Told::default()
            });
        };

        let needed = |name: &str| {
            said(name).ok_or_else(|| format!("{name} is not set, and signing people in on the machines of {domain} needs it."))
        };
        if !domain.contains('.') || domain.chars().any(|letter| !(letter.is_ascii_alphanumeric() || matches!(letter, '.' | '-'))) {
            return Err(format!("AWAY_DOMAIN is a domain's name in full, such as corp.example, and not {domain}."));
        }
        let short_name = needed("AWAY_DOMAIN_SHORT_NAME")?;
        if short_name.chars().any(|letter| !(letter.is_ascii_alphanumeric() || letter == '-')) || short_name.len() > 15 {
            return Err(format!("AWAY_DOMAIN_SHORT_NAME is the domain's short name, such as CORP, and not {short_name}."));
        }
        let controllers = list(Some(needed("AWAY_DOMAIN_CONTROLLERS")?), &[]);
        let pages = needed("AWAY_AUTHORITY")?;
        match Address::read(&pages) {
            Ok(address) if address.secured => {}
            Ok(_) => return Err("AWAY_AUTHORITY begins with https://: an authority's pages are asked over TLS alone.".to_string()),
            Err(why) => return Err(format!("AWAY_AUTHORITY is no address: {why}.")),
        }
        let account = needed("AWAY_AUTHORITY_ACCOUNT")?;
        if account.contains(['\\', '@', '/']) {
            return Err(format!("AWAY_AUTHORITY_ACCOUNT is the account's name alone, without its domain, and not {account}."));
        }
        needed("AWAY_AUTHORITY_PASSWORD_FILE")?;
        let password = String::from_utf8(read("AWAY_AUTHORITY_PASSWORD_FILE")?.unwrap_or_default())
            .map_err(|_| "AWAY_AUTHORITY_PASSWORD_FILE names a file that holds no text.".to_string())?;
        let password = password.trim_end_matches(['\r', '\n']);
        if password.is_empty() {
            return Err("AWAY_AUTHORITY_PASSWORD_FILE names a file that is empty.".to_string());
        }
        let windows = Windows {
            short_name,
            controllers,
            pages,
            authorities: read("AWAY_AUTHORITY_AUTHORITIES_FILE")?,
            account,
            password: Secret::new(password),
            agent_kind: needed("AWAY_AUTHORITY_AGENT_KIND")?,
            sign_in_kind: needed("AWAY_AUTHORITY_SIGN_IN_KIND")?,
            account_entry: said("AWAY_ACCOUNT_ENTRY").unwrap_or_else(|| ACCOUNT_ENTRY.to_string()),
            groups_entry: said("AWAY_SIGN_IN_GROUPS").unwrap_or_else(|| GROUPS_ENTRY.to_string()),
            never_as: list(variable("AWAY_NEVER_AS"), &NEVER_AS),
            never_of: list(variable("AWAY_NEVER_OF"), &NEVER_OF),
            domain: domain.clone(),
        };

        let administrators = match said("AWAY_ADMIN_CERTIFICATE_ENABLED").as_deref() {
            None | Some("false") => None,
            Some("true") => {
                let policy = crate::administrators::Policy::read(
                    &needed("AWAY_ADMIN_CERTIFICATE_IDENTITIES")?,
                    &needed("AWAY_ADMIN_CERTIFICATE_TARGETS")?,
                    &domain,
                )?;
                let mut admin = windows.clone();
                admin.account = needed("AWAY_ADMIN_CERTIFICATE_ACCOUNT")?;
                if admin.account.eq_ignore_ascii_case(&windows.account) || admin.account.contains(['\\', '@', '/']) {
                    return Err("Administrative enrollment needs its own authority account, named without a domain.".into());
                }
                admin.agent_kind = needed("AWAY_ADMIN_CERTIFICATE_AGENT_KIND")?;
                admin.sign_in_kind = needed("AWAY_ADMIN_CERTIFICATE_SIGN_IN_KIND")?;
                if admin.agent_kind.eq_ignore_ascii_case(&windows.agent_kind)
                    || admin.sign_in_kind.eq_ignore_ascii_case(&windows.sign_in_kind)
                {
                    return Err("Administrative enrollment needs its own agent and sign-in templates.".into());
                }
                let password = read("AWAY_ADMIN_CERTIFICATE_PASSWORD_FILE")?
                    .ok_or_else(|| "AWAY_ADMIN_CERTIFICATE_PASSWORD_FILE is required.".to_string())?;
                let password = String::from_utf8(password).map_err(|_| "The administrative authority password is not text.".to_string())?;
                let password = password.trim_end_matches(['\r', '\n']);
                if password.is_empty() {
                    return Err("The administrative authority password is empty.".into());
                }
                admin.password = Secret::new(password);
                // Only Policy::authorize may admit this branch; never_as still applies.
                admin.never_of.clear();
                Some(crate::administrators::Administrator { policy, windows: admin })
            }
            Some(_) => return Err("AWAY_ADMIN_CERTIFICATE_ENABLED must be true or false.".into()),
        };

        let (sign_in, server) = checks(&format!("signing people in on the machines of {domain}"))?;
        Ok(Told { administrators, internal, windows: Some(windows), sign_in: Some(sign_in), server: Some(server), kept })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads settings, with some of them said otherwise.
    fn read_with(whole: &[(&str, &str)], more: &[(&str, &str)]) -> Result<Told, String> {
        let variable = |name: &str| more.iter().chain(whole.iter()).find(|(its, _)| *its == name).map(|(_, value)| value.to_string());
        let file = |path: &str| match path {
            "/secrets/password" => Ok(b"a password\n".to_vec()),
            "/secrets/empty" => Ok(Vec::new()),
            "/secrets/authorities" => Ok(b"-----BEGIN CERTIFICATE-----".to_vec()),
            _ => Err(std::io::Error::from(std::io::ErrorKind::NotFound)),
        };
        Told::read(variable, file)
    }

    fn told(pairs: &[(&str, &str)]) -> Result<Told, String> {
        read_with(pairs, &[])
    }

    const WHOLE: [(&str, &str); 12] = [
        ("AWAY_DOMAIN", "corp.example"),
        ("AWAY_DOMAIN_SHORT_NAME", "CORP"),
        ("AWAY_DOMAIN_CONTROLLERS", "dc-01.corp.example, dc-02.corp.example:1088"),
        ("AWAY_AUTHORITY", "https://authority.corp.example/certsrv/"),
        ("AWAY_AUTHORITY_ACCOUNT", "svc-away"),
        ("AWAY_AUTHORITY_PASSWORD_FILE", "/secrets/password"),
        ("AWAY_AUTHORITY_AGENT_KIND", "CorpAgent"),
        ("AWAY_AUTHORITY_SIGN_IN_KIND", "CorpSignIn"),
        ("AWAY_SIGN_IN_SERVICE", "https://sign-in.example.com/realms/staff"),
        ("AWAY_SIGN_IN_CLIENT", "away"),
        ("AWAY_SERVER", "https://away-server:8082"),
        ("AWAY_INTERNAL_ADDRESS", "0.0.0.0:8083"),
    ];

    #[test]
    fn an_installation_that_names_no_domain_tells_nothing_of_one() {
        let nothing = told(&[]).expect("nothing is said");
        assert!(nothing.windows.is_none() && nothing.internal.is_none() && nothing.sign_in.is_none() && nothing.server.is_none());
        // what is said of a domain that is not named is said by mistake
        let why = told(&[("AWAY_AUTHORITY", "https://authority.corp.example/certsrv/")]).unwrap_err();
        assert_eq!(why, "AWAY_AUTHORITY is set and AWAY_DOMAIN is not: people are signed in as themselves on the machines of a domain.");
    }

    #[test]
    fn a_domain_is_told_whole_and_what_is_not_said_is_as_it_usually_is() {
        let told = told(&WHOLE).expect("a domain");
        let windows = told.windows.expect("the domain");
        assert_eq!((windows.domain.as_str(), windows.short_name.as_str()), ("corp.example", "CORP"));
        assert_eq!(windows.controllers, ["dc-01.corp.example", "dc-02.corp.example:1088"]);
        assert_eq!((windows.account.as_str(), windows.password.expose()), ("svc-away", "a password"));
        assert_eq!((windows.agent_kind.as_str(), windows.sign_in_kind.as_str()), ("CorpAgent", "CorpSignIn"));
        assert_eq!((windows.account_entry.as_str(), windows.groups_entry.as_str()), ("preferred_username", "groups"));
        assert_eq!(windows.never_as, ["administrator", "krbtgt", "guest"]);
        assert_eq!(windows.never_of, ["Domain Admins", "Enterprise Admins", "Schema Admins", "Administrators"]);
        assert_eq!(told.sign_in.expect("the sign-in service").client, "away");
        assert_eq!(told.server.expect("the server").at.host, "away-server");

        // each of them can be said otherwise, and a list that is said empty is empty
        let more = [
            ("AWAY_ACCOUNT_ENTRY", "account"),
            ("AWAY_SIGN_IN_GROUPS", "roles"),
            ("AWAY_NEVER_AS", "root.admin, Administrator"),
            ("AWAY_NEVER_OF", ""),
            ("AWAY_AUTHORITY_AUTHORITIES_FILE", "/secrets/authorities"),
        ];
        let windows = read_with(&WHOLE, &more).expect("what an installation tells").windows.expect("the domain");
        assert_eq!((windows.account_entry.as_str(), windows.groups_entry.as_str()), ("account", "roles"));
        assert_eq!((windows.never_as, windows.never_of.len()), (vec!["root.admin".to_string(), "Administrator".to_string()], 0));
        assert!(windows.authorities.is_some());
    }

    #[test]
    fn administrative_enrollment_is_opt_in_and_requires_independent_credentials_and_templates() {
        let settings = [
            ("AWAY_ADMIN_CERTIFICATE_ENABLED", "true"),
            ("AWAY_ADMIN_CERTIFICATE_IDENTITIES", r#"{"subject":"alex@corp.example"}"#),
            ("AWAY_ADMIN_CERTIFICATE_TARGETS", "host.corp.example"),
            ("AWAY_ADMIN_CERTIFICATE_ACCOUNT", "svc-admin"),
            ("AWAY_ADMIN_CERTIFICATE_PASSWORD_FILE", "/secrets/password"),
            ("AWAY_ADMIN_CERTIFICATE_AGENT_KIND", "AdminAgent"),
            ("AWAY_ADMIN_CERTIFICATE_SIGN_IN_KIND", "AdminLogon"),
        ];
        let told = read_with(&WHOLE, &settings).unwrap();
        assert!(!told.windows.as_ref().unwrap().never_of.is_empty());
        let admin = told.administrators.unwrap();
        assert!(admin.windows.never_of.is_empty());
        assert_eq!(admin.windows.never_as, told.windows.unwrap().never_as);
        for (missing, _) in settings.iter().skip(1) {
            let incomplete: Vec<_> = settings.iter().copied().filter(|(key, _)| key != missing).collect();
            assert!(read_with(&WHOLE, &incomplete).is_err(), "{missing}");
        }
        for (key, ordinary) in [
            ("AWAY_ADMIN_CERTIFICATE_ACCOUNT", "svc-away"),
            ("AWAY_ADMIN_CERTIFICATE_AGENT_KIND", "CorpAgent"),
            ("AWAY_ADMIN_CERTIFICATE_SIGN_IN_KIND", "CorpSignIn"),
        ] {
            let mut changed = settings.to_vec();
            changed.iter_mut().find(|(name, _)| name == &key).unwrap().1 = ordinary;
            assert!(read_with(&WHOLE, &changed).is_err());
        }
    }

    #[test]
    fn secrets_kept_for_machines_are_handed_out_with_or_without_a_domain() {
        let rests_on = [
            ("AWAY_SIGN_IN_SERVICE", "https://sign-in.example.com/realms/staff"),
            ("AWAY_SIGN_IN_CLIENT", "away"),
            ("AWAY_SERVER", "https://away-server:8082"),
            ("AWAY_INTERNAL_ADDRESS", "0.0.0.0:8083"),
        ];
        let kept: Vec<(&str, &str)> = std::iter::once(("AWAY_KEPT_SECRETS", "/run/away/kept")).chain(rests_on).collect();
        let without_a_domain = told(&kept).expect("kept secrets");
        assert_eq!(without_a_domain.kept.as_deref(), Some("/run/away/kept"));
        assert!(without_a_domain.windows.is_none() && without_a_domain.sign_in.is_some() && without_a_domain.server.is_some());
        // the issuer hands a kept secret out only for a session the server says runs, of
        // a person the sign-in service signed: what it asks them at is needed
        for (name, _) in rests_on {
            let without: Vec<(&str, &str)> = kept.iter().filter(|(its, _)| *its != name).copied().collect();
            let why = told(&without).unwrap_err();
            assert!(why.starts_with(&format!("{name} is not set, and handing out the secrets Away keeps for machines needs it")), "{why}");
        }
        // and beside a domain, as well
        let with_a_domain = read_with(&WHOLE, &[("AWAY_KEPT_SECRETS", "/run/away/kept")]).expect("a domain and kept secrets");
        assert!(with_a_domain.windows.is_some() && with_a_domain.kept.is_some());
        assert!(told(&WHOLE).expect("a domain").kept.is_none());
    }

    #[test]
    fn what_a_domain_needs_and_is_not_said_is_named() {
        for (name, _) in WHOLE.iter().skip(1) {
            let without: Vec<(&str, &str)> = WHOLE.iter().filter(|(its, _)| its != name).copied().collect();
            let why = told(&without).unwrap_err();
            assert!(
                why.starts_with(&format!("{name} is not set, and signing people in on the machines of corp.example needs it")),
                "{why}"
            );
        }
        for (name, value, says) in [
            ("AWAY_DOMAIN", "CORP", "AWAY_DOMAIN is a domain's name in full, such as corp.example, and not CORP."),
            (
                "AWAY_DOMAIN_SHORT_NAME",
                "corp.example",
                "AWAY_DOMAIN_SHORT_NAME is the domain's short name, such as CORP, and not corp.example.",
            ),
            (
                "AWAY_AUTHORITY",
                "http://127.0.0.1/certsrv/",
                "AWAY_AUTHORITY begins with https://: an authority's pages are asked over TLS alone.",
            ),
            (
                "AWAY_AUTHORITY_ACCOUNT",
                "CORP\\svc-away",
                "AWAY_AUTHORITY_ACCOUNT is the account's name alone, without its domain, and not CORP\\svc-away.",
            ),
            ("AWAY_AUTHORITY_PASSWORD_FILE", "/secrets/empty", "AWAY_AUTHORITY_PASSWORD_FILE names a file that is empty."),
            (
                "AWAY_AUTHORITY_PASSWORD_FILE",
                "/secrets/none",
                "AWAY_AUTHORITY_PASSWORD_FILE names /secrets/none, which can't be read: entity not found.",
            ),
        ] {
            assert_eq!(read_with(&WHOLE, &[(name, value)]).err().as_deref(), Some(says));
        }
    }
}
