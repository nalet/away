//! What a sign-in service signed: who somebody is. Reading one, and checking that the
//! service signed it, for Away, for this sign-in, and not too long ago (OpenID Connect
//! Core 3.1.3.7; RFC 7515, 7517 and 7519).
//!
//! Nothing a token says is used before its signature is checked against the keys the
//! service publishes. The signature itself is checked by the cryptography library.

use crate::base64;
use crate::json::Json;
use ring::signature::{self, RsaPublicKeyComponents, UnparsedPublicKey};

/// How far the service's clock and Away's may be apart, in seconds.
const CLOCKS_APART: u64 = 60;

/// One key a sign-in service signs with.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Key {
    name: Option<String>,
    /// The one way of signing it is for, where the service says so.
    only: Option<String>,
    kind: Kind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Kind {
    Rsa {
        modulus: Vec<u8>,
        exponent: Vec<u8>,
    },
    /// A point on a curve, as the library reads it, and the letters that name the curve.
    Curve(&'static str, Vec<u8>),
}

/// The keys a sign-in service publishes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Keys(Vec<Key>);

impl Keys {
    /// Reads the keys a service publishes. A key of a kind Away can't check with, or one
    /// that is not for signing, is left out.
    pub fn read(published: &Json) -> Keys {
        let part = |key: &Json, name: &str| key.text_of(name).and_then(base64::read_for_addresses);
        let keys = published.get("keys").and_then(Json::list).unwrap_or_default();
        Keys(
            keys.iter()
                .filter(|key| matches!(key.text_of("use"), None | Some("sig")))
                .filter_map(|key| {
                    let kind = match (key.text_of("kty")?, key.text_of("crv")) {
                        ("RSA", _) => Kind::Rsa { modulus: part(key, "n")?, exponent: part(key, "e")? },
                        ("EC", Some(curve @ ("P-256" | "P-384"))) => {
                            let (x, y) = (part(key, "x")?, part(key, "y")?);
                            let size = if curve == "P-256" { 32 } else { 48 };
                            if x.len() != size || y.len() != size {
                                return None;
                            }
                            Kind::Curve(if curve == "P-256" { "ES256" } else { "ES384" }, [&[4][..], &x, &y].concat())
                        }
                        _ => return None,
                    };
                    Some(Key { name: key.text_of("kid").map(str::to_string), only: key.text_of("alg").map(str::to_string), kind })
                })
                .collect(),
        )
    }

    /// Whether there is no key to check with.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// What a token has to say to be taken.
#[derive(Clone, Copy, Debug)]
pub struct Expected<'a> {
    /// The sign-in service, as it names itself.
    pub service: &'a str,
    /// The name the service knows Away by.
    pub client: &'a str,
    /// The number made for this sign-in, where the token has to carry it.
    pub number: Option<&'a str>,
    /// The time now, in seconds since 1970.
    pub now: u64,
}

/// Why a token is not taken.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Wrong {
    /// It is not built as a signed token is.
    NotAToken,
    /// It is signed in a way Away doesn't take: with no signature at all, or with a
    /// secret that both sides know.
    Signed(String),
    /// It is signed with a key that is not among those the service publishes. The
    /// service may have changed its keys since they were read.
    UnknownKey,
    /// Its signature is not the service's.
    NotSigned,
    /// It is signed by the service, and says something that is not expected.
    Says(&'static str),
}

impl std::fmt::Display for Wrong {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Wrong::NotAToken => out.write_str("what the sign-in service sent is not a signed token"),
            Wrong::Signed(way) => write!(out, "the token is signed in a way that isn't taken ({way})"),
            Wrong::UnknownKey => out.write_str("the token is signed with a key the sign-in service doesn't publish"),
            Wrong::NotSigned => out.write_str("the token's signature is not the sign-in service's"),
            Wrong::Says(what) => write!(out, "the token {what}"),
        }
    }
}

/// Checks a token, and gives what it says.
pub fn check(token: &str, keys: &Keys, expected: &Expected) -> Result<Json, Wrong> {
    let mut parts = token.split('.');
    let (Some(head), Some(body), Some(signed), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
        return Err(Wrong::NotAToken);
    };
    let read = |letters: &str| -> Option<Json> { Json::read(std::str::from_utf8(&base64::read_for_addresses(letters)?).ok()?).ok() };
    let says_of_itself = read(head).ok_or(Wrong::NotAToken)?;
    let way = says_of_itself.text_of("alg").ok_or(Wrong::NotAToken)?;
    let signature = base64::read_for_addresses(signed).ok_or(Wrong::NotAToken)?;
    // a token names what it is not allowed to leave unread: Away reads none of those
    if says_of_itself.get("crit").is_some() {
        return Err(Wrong::Signed("it asks for something Away doesn't know".to_string()));
    }
    let message = &token.as_bytes()[..head.len() + 1 + body.len()];

    let named = says_of_itself.text_of("kid");
    let fitting: Vec<&Key> = keys
        .0
        .iter()
        .filter(|key| named.is_none() || key.name.as_deref() == named)
        .filter(|key| key.only.as_deref().is_none_or(|only| only == way))
        .filter(|key| match (&key.kind, way) {
            (Kind::Rsa { .. }, "RS256" | "RS384" | "RS512" | "PS256" | "PS384" | "PS512") => true,
            (Kind::Curve(its, _), way) => *its == way,
            _ => false,
        })
        .collect();
    if !matches!(way, "RS256" | "RS384" | "RS512" | "PS256" | "PS384" | "PS512" | "ES256" | "ES384") {
        return Err(Wrong::Signed(way.to_string()));
    }
    if fitting.is_empty() {
        return Err(Wrong::UnknownKey);
    }
    let signed_by = |key: &Key| match &key.kind {
        Kind::Rsa { modulus, exponent } => {
            let how = match way {
                "RS256" => &signature::RSA_PKCS1_2048_8192_SHA256,
                "RS384" => &signature::RSA_PKCS1_2048_8192_SHA384,
                "RS512" => &signature::RSA_PKCS1_2048_8192_SHA512,
                "PS256" => &signature::RSA_PSS_2048_8192_SHA256,
                "PS384" => &signature::RSA_PSS_2048_8192_SHA384,
                _ => &signature::RSA_PSS_2048_8192_SHA512,
            };
            RsaPublicKeyComponents { n: modulus, e: exponent }.verify(how, message, &signature).is_ok()
        }
        Kind::Curve("ES256", point) => {
            UnparsedPublicKey::new(&signature::ECDSA_P256_SHA256_FIXED, point).verify(message, &signature).is_ok()
        }
        Kind::Curve(_, point) => UnparsedPublicKey::new(&signature::ECDSA_P384_SHA384_FIXED, point).verify(message, &signature).is_ok(),
    };
    if !fitting.into_iter().any(signed_by) {
        return Err(Wrong::NotSigned);
    }

    // signed by the service: now what it says
    let says = read(body).filter(|says| matches!(says, Json::Object(_))).ok_or(Wrong::NotAToken)?;
    if says.text_of("iss") != Some(expected.service) {
        return Err(Wrong::Says("is from another sign-in service"));
    }
    let for_away = match says.get("aud") {
        Some(Json::Text(one)) => one == expected.client,
        Some(Json::List(several)) => {
            let among = several.iter().any(|one| one.text() == Some(expected.client));
            // made for several: it has to say that Away is the one it was handed to
            among && (several.len() == 1 || says.text_of("azp") == Some(expected.client))
        }
        _ => false,
    };
    if !for_away || says.text_of("azp").is_some_and(|handed_to| handed_to != expected.client) {
        return Err(Wrong::Says("was made for somebody else"));
    }
    let time = |name: &str| says.get(name).and_then(Json::count);
    match time("exp") {
        Some(until) if until + CLOCKS_APART > expected.now => {}
        Some(_) => return Err(Wrong::Says("has run out")),
        None => return Err(Wrong::Says("doesn't say how long it is good for")),
    }
    if [time("iat"), time("nbf")].into_iter().flatten().any(|from| from > expected.now + CLOCKS_APART) {
        return Err(Wrong::Says("is from the future: the clocks of Away and of the sign-in service are apart"));
    }
    if expected.number.is_some_and(|number| says.text_of("nonce") != Some(number)) {
        return Err(Wrong::Says("is not the answer to this sign-in"));
    }
    if says.text_of("sub").is_none_or(str::is_empty) {
        return Err(Wrong::Says("doesn't say who it is about"));
    }
    Ok(says)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stand_in::Signer;

    const SERVICE: &str = "https://sign-in.example.com/realms/staff";
    const NOW: u64 = 1_800_000_000;

    fn says() -> Json {
        Json::object([
            ("iss", SERVICE.into()),
            ("aud", "away".into()),
            ("sub", "7c1f".into()),
            ("exp", (NOW + 300).into()),
            ("iat", NOW.into()),
            ("nonce", "n-1".into()),
            ("name", "Alex Urban".into()),
        ])
    }

    fn expected() -> Expected<'static> {
        Expected { service: SERVICE, client: "away", number: Some("n-1"), now: NOW }
    }

    /// The same claims with one changed, or taken away where the new value is nothing.
    fn with(name: &str, value: Json) -> Json {
        let Json::Object(mut held) = says() else { unreachable!() };
        held.retain(|(its, _)| its != name);
        if value != Json::Null {
            held.push((name.to_string(), value));
        }
        Json::Object(held)
    }

    #[test]
    fn a_token_the_service_signed_for_away_is_taken_in_each_way_of_signing() {
        for signer in [Signer::curve(), Signer::rsa("RS256"), Signer::rsa("PS256"), Signer::rsa("RS512")] {
            let keys = Keys::read(&signer.published());
            let token = signer.sign(&says());
            assert_eq!(check(&token, &keys, &expected()).unwrap().text_of("name"), Some("Alex Urban"), "{}", signer.way());
        }
    }

    #[test]
    fn a_token_that_was_changed_or_signed_by_another_is_not_the_services() {
        let signer = Signer::curve();
        let keys = Keys::read(&signer.published());
        let token = signer.sign(&says());
        // what it says, changed after it was signed
        let (head, rest) = token.split_once('.').unwrap();
        let (_, signed) = rest.split_once('.').unwrap();
        let changed = format!("{head}.{}.{signed}", base64::for_addresses(with("sub", "root".into()).write().as_bytes()));
        assert_eq!(check(&changed, &keys, &expected()), Err(Wrong::NotSigned));
        // signed by somebody with the same key's name
        let other = Signer::curve();
        assert_eq!(check(&other.sign(&says()), &keys, &expected()), Err(Wrong::NotSigned));
        // signed with a key of another name: the service may have changed its keys
        let renamed = Signer::curve().named("2027");
        assert_eq!(check(&renamed.sign(&says()), &keys, &expected()), Err(Wrong::UnknownKey));
        assert_eq!(check(&token, &Keys::default(), &expected()), Err(Wrong::UnknownKey));
    }

    #[test]
    fn a_token_without_a_signature_or_signed_with_a_shared_secret_is_refused() {
        let signer = Signer::curve();
        let keys = Keys::read(&signer.published());
        let body = base64::for_addresses(says().write().as_bytes());
        for way in ["none", "HS256", "ES512", "EdDSA", ""] {
            let head = base64::for_addresses(Json::object([("alg", way.into())]).write().as_bytes());
            assert_eq!(check(&format!("{head}.{body}."), &keys, &expected()), Err(Wrong::Signed(way.to_string())), "{way}");
        }
        // a key of the service's, and a token that says it is signed another way than the key is for
        let rsa = Signer::rsa("RS256");
        let as_another = rsa.sign(&says()).replacen(&rsa.head("RS256"), &rsa.head("ES256"), 1);
        assert_eq!(check(&as_another, &Keys::read(&rsa.published()), &expected()), Err(Wrong::UnknownKey));
        // something Away would have to understand and doesn't
        let critical = Json::object([("alg", "ES256".into()), ("crit", Json::List(vec!["exp".into()]))]);
        let token = format!("{}.{body}.{}", base64::for_addresses(critical.write().as_bytes()), base64::for_addresses(&[0; 64]));
        assert!(matches!(check(&token, &keys, &expected()), Err(Wrong::Signed(_))));
    }

    #[test]
    fn what_is_no_token_is_refused() {
        let keys = Keys::read(&Signer::curve().published());
        for wrong in ["", "a", "a.b", "a.b.c.d", "!!.!!.!!", "e30.e30.", "bm8.bm8.bm8"] {
            assert_eq!(check(wrong, &keys, &expected()), Err(Wrong::NotAToken), "{wrong}");
        }
    }

    #[test]
    fn a_signed_token_still_has_to_say_what_is_expected() {
        let signer = Signer::curve();
        let keys = Keys::read(&signer.published());
        let refused = |says: Json| match check(&signer.sign(&says), &keys, &expected()) {
            Err(Wrong::Says(what)) => what,
            other => panic!("{other:?}"),
        };
        assert_eq!(refused(with("iss", "https://elsewhere.example.com".into())), "is from another sign-in service");
        assert_eq!(refused(with("aud", "another".into())), "was made for somebody else");
        assert_eq!(refused(with("aud", Json::Null)), "was made for somebody else");
        assert_eq!(refused(with("aud", Json::List(vec!["away".into(), "another".into()]))), "was made for somebody else");
        assert_eq!(refused(with("azp", "another".into())), "was made for somebody else");
        assert_eq!(refused(with("exp", (NOW - 61).into())), "has run out");
        assert_eq!(refused(with("exp", Json::Null)), "doesn't say how long it is good for");
        assert_eq!(refused(with("exp", "tomorrow".into())), "doesn't say how long it is good for");
        assert_eq!(refused(with("iat", (NOW + 61).into())), "is from the future: the clocks of Away and of the sign-in service are apart");
        assert_eq!(refused(with("nbf", (NOW + 600).into())), "is from the future: the clocks of Away and of the sign-in service are apart");
        assert_eq!(refused(with("nonce", "n-2".into())), "is not the answer to this sign-in");
        assert_eq!(refused(with("nonce", Json::Null)), "is not the answer to this sign-in");
        assert_eq!(refused(with("sub", Json::Null)), "doesn't say who it is about");
        assert_eq!(refused(with("sub", "".into())), "doesn't say who it is about");
    }

    #[test]
    fn what_is_expected_allows_for_clocks_that_are_a_little_apart_and_for_several_audiences() {
        let signer = Signer::curve();
        let keys = Keys::read(&signer.published());
        let taken = |says: Json, expected: &Expected| check(&signer.sign(&says), &keys, expected).is_ok();
        assert!(taken(with("exp", (NOW - 30).into()), &expected()));
        assert!(taken(with("iat", (NOW + 30).into()), &expected()));
        assert!(taken(with("aud", Json::List(vec!["away".into()])), &expected()));
        let several = with("aud", Json::List(vec!["away".into(), "another".into()]));
        let Json::Object(mut held) = several else { unreachable!() };
        held.push(("azp".to_string(), "away".into()));
        assert!(taken(Json::Object(held), &expected()));
        // a renewal's token is not asked for the number of the sign-in
        assert!(taken(with("nonce", Json::Null), &Expected { number: None, ..expected() }));
    }

    #[test]
    fn keys_that_are_of_no_use_are_left_out() {
        let published = Json::read(
            r#"{"keys":[
                {"kty":"oct","k":"c2VjcmV0"},
                {"kty":"EC","crv":"P-521","x":"AA","y":"AA"},
                {"kty":"EC","crv":"P-256","x":"AA","y":"AA"},
                {"kty":"RSA","n":"not letters!","e":"AQAB"},
                {"kty":"RSA","use":"enc","n":"AQAB","e":"AQAB"},
                {"kty":"RSA","kid":"a","n":"AQAB","e":"AQAB"}
            ]}"#,
        )
        .unwrap();
        let keys = Keys::read(&published);
        assert_eq!(keys.0.len(), 1);
        assert_eq!(keys.0[0].name.as_deref(), Some("a"));
        assert!(Keys::read(&Json::Null).is_empty());
        assert!(Keys::read(&Json::object([("keys", "none".into())])).is_empty());
    }
}
