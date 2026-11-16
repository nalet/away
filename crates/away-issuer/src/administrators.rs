//! A separate, opt-in enrollment authority for explicitly named administrative
//! identities and destinations. Ordinary enrollment restrictions stay in force.

use std::collections::{BTreeMap, BTreeSet};

use away_part::json::Json;

use crate::{
    told,
    windows::{Refused, Windows},
};

/// A certificate policy keyed by the immutable subject of the verified OIDC token.
#[derive(Clone, Debug)]
pub struct Policy {
    identities: BTreeMap<String, String>,
    targets: BTreeSet<String>,
}

impl Policy {
    /// Reads an explicit subject-to-UPN map and exact DNS targets. No wildcards,
    /// alternate realms, empty lists, or duplicate subjects are accepted.
    pub fn read(identities: &str, targets: &str, domain: &str) -> Result<Self, String> {
        let bad = || "Administrative certificate SSO needs unique subjects mapped to same-domain UPNs and exact DNS targets.".to_string();
        let Json::Object(pairs) = Json::read(identities).map_err(|_| bad())? else { return Err(bad()) };
        let mut mapped = BTreeMap::new();
        for (subject, upn) in pairs {
            let upn = upn.text().ok_or_else(bad)?.to_ascii_lowercase();
            let Some((account, realm)) = upn.split_once('@') else { return Err(bad()) };
            if subject.trim() != subject
                || subject.is_empty()
                || !realm.eq_ignore_ascii_case(domain)
                || account.is_empty()
                || account.starts_with('.')
                || account.ends_with('.')
                || !account.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
                || mapped.insert(subject, upn).is_some()
            {
                return Err(bad());
            }
        }
        let mut hosts = BTreeSet::new();
        for host in targets.split(',').map(str::trim) {
            let host = host.to_ascii_lowercase();
            if !host.ends_with(&format!(".{}", domain.to_ascii_lowercase()))
                || host.split('.').any(|part| {
                    part.is_empty()
                        || part.starts_with('-')
                        || part.ends_with('-')
                        || !part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                })
                || !hosts.insert(host)
            {
                return Err(bad());
            }
        }
        if mapped.is_empty() || hosts.is_empty() {
            return Err(bad());
        }
        Ok(Self { identities: mapped, targets: hosts })
    }

    /// Whether this identity belongs to the separate policy. Once selected, a
    /// refusal cannot fall back to the ordinary enrollment authority.
    pub fn contains(&self, subject: &str) -> bool {
        self.identities.contains_key(subject)
    }

    /// Checks the authenticated account and every destination of the server-vouched
    /// session before any request reaches AD CS. The browser supplies none of these.
    pub fn authorize(&self, says: &Json, targets: &[String], windows: &Windows) -> Result<(), Refused> {
        let denied = || Refused {
            why: "admin-policy",
            says: "Administrative certificate SSO is not authorized for this identity and destination.".into(),
        };
        let upn = says.text_of("sub").and_then(|sub| self.identities.get(sub)).ok_or_else(denied)?;
        let account = windows.account(says)?;
        let name = upn.split_once('@').ok_or_else(denied)?.0;
        if !account.name.eq_ignore_ascii_case(name)
            || account.signs_in_as.as_ref().is_some_and(|signed| !signed.eq_ignore_ascii_case(upn))
            || targets.is_empty()
            || targets.iter().any(|target| {
                let (host, port) = target.split_once(':').unwrap_or((target, "3389"));
                port != "3389" || !self.targets.contains(&host.to_ascii_lowercase())
            })
        {
            return Err(denied());
        }
        Ok(())
    }
}

/// Independent credentials and templates, whose CA enrollment restrictions name
/// only the approved administrative recipients.
#[derive(Clone, Debug)]
pub struct Administrator {
    /// The immutable identities and exact destinations.
    pub policy: Policy,
    /// The independent authority account and templates.
    pub windows: told::Windows,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn administrative_enrollment_requires_explicit_same_domain_identities_and_targets() {
        let good = r#"{"immutable-subject":"alex@corp.example"}"#;
        let policy = Policy::read(good, "host.corp.example", "corp.example").unwrap();
        assert!(policy.contains("immutable-subject"));
        assert!(!policy.contains("alex@corp.example"));
        for identities in
            ["{}", r#"{"a":"alex@other.example"}"#, r#"{"a":"alex@corp.example","a":"sam@corp.example"}"#, r#"{"a":"administrator"}"#]
        {
            assert!(Policy::read(identities, "host.corp.example", "corp.example").is_err());
        }
        for targets in ["", "*.corp.example", "host.corp.example:3390", "host.other.example", "host.corp.example,host.corp.example"] {
            assert!(Policy::read(good, targets, "corp.example").is_err());
        }
    }
}
