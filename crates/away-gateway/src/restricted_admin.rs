//! Opt-in browser Restricted Admin for explicitly mapped OIDC subjects and targets.
//! The service account's KCD grants must be limited to those targets in AD as well.

use crate::machine::{Failure, Step, Why};
use away_part::{framed, json::Json, time};
use away_secret::Secret;
use away_winauth::kerberos::{
    delegation::Request,
    exchange::{Ticket, WithAPassword},
    messages::{Name, NameKind},
};
use std::collections::BTreeMap;
use std::time::Duration;

/// Installation policy and a dedicated, non-administrative AD service credential.
#[derive(Clone, Debug)]
pub struct RestrictedAdmin {
    realm: String,
    controllers: Vec<String>,
    account: String,
    service: Name,
    password: Secret,
    identities: BTreeMap<String, String>,
    targets: Vec<String>,
    fingerprints: BTreeMap<String, String>,
}

fn not(why: impl ToString) -> Failure {
    Failure { step: Step::SigningIn, why: Why::NotThemselves, details: why.to_string() }
}
fn random<const N: usize>() -> Result<[u8; N], Failure> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes).map_err(not)?;
    Ok(bytes)
}
fn now() -> (u64, u32) {
    let now = time::now();
    (now.as_secs(), now.subsec_micros())
}
fn nonce() -> Result<u32, Failure> {
    Ok(u32::from_be_bytes(random()?) & 0x7fff_ffff)
}

fn dns(name: &str) -> bool {
    name.contains('.')
        && name.len() <= 253
        && name.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

impl RestrictedAdmin {
    /// Reads a disabled-by-default policy. Partial or malformed enabled policy stops
    /// startup, and secrets are read only from their mounted file.
    pub fn read(
        variable: &impl Fn(&str) -> Option<String>,
        file: &impl Fn(&str) -> std::io::Result<Vec<u8>>,
    ) -> Result<Option<Self>, String> {
        match variable("AWAY_RESTRICTED_ADMIN_ENABLED").as_deref() {
            None | Some("false") => return Ok(None),
            Some("true") => {}
            _ => return Err("AWAY_RESTRICTED_ADMIN_ENABLED must be true or false".into()),
        }
        let needed = |name: &str| {
            variable(name).filter(|s| !s.trim().is_empty()).ok_or_else(|| format!("{name} is required for browser Restricted Admin"))
        };
        let realm = needed("AWAY_RESTRICTED_ADMIN_REALM")?.to_ascii_uppercase();
        let controllers: Vec<String> = needed("AWAY_RESTRICTED_ADMIN_CONTROLLERS")?.split(',').map(|s| s.trim().into()).collect();
        let targets: Vec<String> = needed("AWAY_RESTRICTED_ADMIN_TARGETS")?.split(',').map(|s| s.trim().to_ascii_lowercase()).collect();
        let account = needed("AWAY_RESTRICTED_ADMIN_ACCOUNT")?;
        let service = needed("AWAY_RESTRICTED_ADMIN_SERVICE")?;
        let Some((class, host)) = service.split_once('/') else {
            return Err("the Restricted Admin service must be a registered SPN".into());
        };
        if !dns(&realm)
            || controllers.iter().chain(&targets).any(|s| !dns(s))
            || !dns(host)
            || class != "HTTP"
            || !account.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        {
            return Err("invalid Restricted Admin realm, controller, target or service account".into());
        }
        let service = Name::new(NameKind::Service, &[class, host]);
        let Json::Object(entries) =
            Json::read(&needed("AWAY_RESTRICTED_ADMIN_IDENTITIES")?).map_err(|_| "Restricted Admin identities must be a JSON object")?
        else {
            return Err("Restricted Admin identities must map immutable OIDC subjects to domain UPNs".into());
        };
        let mut identities = BTreeMap::new();
        for (subject, upn) in entries {
            let Some(upn) = upn.text() else { return Err("a Restricted Admin identity must be a UPN".into()) };
            let Some((user, domain)) = upn.split_once('@') else { return Err("a Restricted Admin identity must be a UPN".into()) };
            if subject.trim().is_empty()
                || user.is_empty()
                || !domain.eq_ignore_ascii_case(&realm)
                || !user.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
                || identities.insert(subject, user.to_string()).is_some()
            {
                return Err("invalid or duplicated Restricted Admin identity".into());
            }
        }
        if identities.is_empty() {
            return Err("Restricted Admin needs at least one explicit identity".into());
        }
        let Json::Object(pins) = Json::read(&needed("AWAY_RESTRICTED_ADMIN_FINGERPRINTS")?)
            .map_err(|_| "Restricted Admin fingerprints must be a JSON object")?
        else {
            return Err("Restricted Admin fingerprints must map targets to SHA-256 certificate fingerprints".into());
        };
        let mut fingerprints = BTreeMap::new();
        for (host, pin) in pins {
            let Some(pin) = pin.text() else { return Err("a Restricted Admin fingerprint must be text".into()) };
            if pin.len() != 95
                || !pin.split(':').all(|byte| byte.len() == 2 && byte.bytes().all(|b| b.is_ascii_hexdigit()))
                || fingerprints.insert(host.to_ascii_lowercase(), pin.to_ascii_lowercase()).is_some()
            {
                return Err("invalid or duplicated Restricted Admin fingerprint".into());
            }
        }
        if targets.iter().any(|host| !fingerprints.contains_key(host)) || fingerprints.len() != targets.len() {
            return Err("every Restricted Admin target requires exactly one certificate fingerprint".into());
        }
        let bytes = file(&needed("AWAY_RESTRICTED_ADMIN_PASSWORD_FILE")?)
            .map_err(|_| "the Restricted Admin service password file cannot be read")?;
        let password = std::str::from_utf8(&bytes).map_err(|_| "the Restricted Admin service password is not UTF-8")?;
        if password.trim().is_empty() {
            return Err("the Restricted Admin service password is empty".into());
        }
        Ok(Some(Self { realm, controllers, account, service, password: Secret::new(password), identities, targets, fingerprints }))
    }

    /// Authorizes before contacting a KDC. No wildcard accounts, ports, redirects,
    /// or user-controlled names are accepted.
    pub fn account_for(&self, subject: &str, host: &str, port: u16) -> Result<&str, Failure> {
        if port != 3389 || !self.targets.iter().any(|target| target.eq_ignore_ascii_case(host)) {
            return Err(not("this target is not enabled for browser Restricted Admin"));
        }
        self.identities
            .get(subject)
            .map(String::as_str)
            .ok_or_else(|| not("this signed-in identity is not enabled for browser Restricted Admin"))
    }

    /// Checks the management host against an independently provisioned certificate pin.
    pub fn check_identity(&self, host: &str, fingerprint: &str) -> Result<(), Failure> {
        if self.fingerprints.get(&host.to_ascii_lowercase()).is_some_and(|pin| pin.eq_ignore_ascii_case(fingerprint)) {
            Ok(())
        } else {
            Err(Failure {
                step: Step::Checking,
                why: Why::AnotherIdentity,
                details: "the management host certificate does not match its configured Restricted Admin fingerprint".into(),
            })
        }
    }

    /// Obtains only a ten-minute TERMSRV ticket for this authorized session. All
    /// intermediate tickets stay in memory and are dropped before RDP begins.
    pub async fn ticket(&self, subject: &str, host: &str, port: u16) -> Result<Ticket, Failure> {
        let account = self.account_for(subject, host, port)?;
        let mut last = not("no Restricted Admin domain controller answered");
        for controller in &self.controllers {
            match self.at(controller, account, host).await {
                Ok(ticket) => return Ok(ticket),
                Err(failure) => last = failure,
            }
        }
        Err(last)
    }

    async fn at(&self, controller: &str, account: &str, host: &str) -> Result<Ticket, Failure> {
        let ask = |request: Vec<u8>| async move { framed::ask(controller, 88, &request, Duration::from_secs(10)).await.map_err(not) };
        let (mut first, request) = WithAPassword::begin(&self.account, &self.realm, now(), nonce()?);
        let reply = ask(request).await?;
        let proof = first.prove(&reply, self.password.expose(), now(), random()?).map_err(not)?;
        let first = first.receive(&ask(proof).await?).map_err(not)?;
        if !first.realm.eq_ignore_ascii_case(&self.realm)
            || first.account.parts != [self.account.clone()]
            || first.service.parts != ["krbtgt", &self.realm]
            || !first.service_realm.eq_ignore_ascii_case(&self.realm)
        {
            return Err(not("the service TGT does not match the configured account and realm"));
        }
        let (request, bytes) = Request::for_user(&first, self.service.clone(), account, now(), nonce()?, random()?);
        let evidence = request.receive(&ask(bytes).await?).map_err(not)?;
        let target = Name::new(NameKind::Service, &["TERMSRV", host]);
        let (request, bytes) = Request::for_target(&first, &evidence, target, now(), nonce()?, random()?);
        request.receive(&ask(bytes).await?).map_err(not)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn settings() -> BTreeMap<String, String> {
        [
            ("ENABLED", "true"),
            ("REALM", "corp.example"),
            ("CONTROLLERS", "dc.corp.example"),
            ("TARGETS", "desk.corp.example"),
            ("ACCOUNT", "svc-away"),
            ("SERVICE", "HTTP/away.corp.example"),
            ("PASSWORD_FILE", "/secret"),
            ("FINGERPRINTS", r#"{"desk.corp.example":"11:11:11:11:11:11:11:11:11:11:11:11:11:11:11:11:11:11:11:11:11:11:11:11:11:11:11:11:11:11:11:11"}"#),
            ("IDENTITIES", r#"{"immutable-subject":"alex.urban@corp.example"}"#),
        ]
        .into_iter()
        .map(|(k, v)| (format!("AWAY_RESTRICTED_ADMIN_{k}"), v.into()))
        .collect()
    }
    #[test]
    fn authorization_is_exact_and_independent_of_display_name_or_email() {
        let vars = settings();
        let policy = RestrictedAdmin::read(&|k| vars.get(k).cloned(), &|_| Ok(b"service test secret".to_vec())).unwrap().unwrap();
        assert_eq!(policy.account_for("immutable-subject", "desk.corp.example", 3389).unwrap(), "alex.urban");
        for (subject, host, port) in [
            ("alex.urban@corp.example", "desk.corp.example", 3389),
            ("other-subject", "desk.corp.example", 3389),
            ("immutable-subject", "other.corp.example", 3389),
            ("immutable-subject", "desk.corp.example", 3390),
        ] {
            assert!(policy.account_for(subject, host, port).is_err());
        }
        assert!(!format!("{policy:?}").contains("service test secret"));
        assert!(policy.check_identity("desk.corp.example", &std::iter::repeat_n("11", 32).collect::<Vec<_>>().join(":")).is_ok());
        assert!(policy.check_identity("desk.corp.example", "other certificate").is_err());
    }
    #[test]
    fn disabled_never_reads_secrets_and_enabled_policy_must_be_complete() {
        assert!(RestrictedAdmin::read(&|_| None, &|_| panic!("disabled policy reads nothing")).unwrap().is_none());
        let vars = settings();
        for missing in vars.keys().filter(|key| !key.ends_with("ENABLED")) {
            let mut vars = vars.clone();
            vars.remove(missing);
            assert!(RestrictedAdmin::read(&|k| vars.get(k).cloned(), &|_| Ok(b"test".to_vec())).is_err(), "{missing}");
        }
        for invalid in [
            "{}",
            r#"{"subject":"alex@another.example"}"#,
            r#"{"subject":"administrator"}"#,
            r#"{"subject":"alex@corp.example","subject":"bob@corp.example"}"#,
        ] {
            let mut vars = vars.clone();
            vars.insert("AWAY_RESTRICTED_ADMIN_IDENTITIES".into(), invalid.into());
            assert!(RestrictedAdmin::read(&|k| vars.get(k).cloned(), &|_| Ok(b"test".to_vec())).is_err());
        }
    }
}
