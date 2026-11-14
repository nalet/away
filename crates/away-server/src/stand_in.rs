//! What stands in for a sign-in service in the tests: something that signs tokens as a
//! real one does, with keys that exist for the tests alone.
//!
//! It is built only for tests: nothing of it is in a release.

use away_part::base64;
use away_part::json::Json;
use ring::rand::SystemRandom;
use ring::signature::{self, EcdsaKeyPair, KeyPair, RsaKeyPair, RsaPublicKeyComponents};

enum Key {
    Curve(EcdsaKeyPair),
    Rsa(RsaKeyPair),
}

/// Signs tokens, and publishes the key to check them with.
pub struct Signer {
    way: &'static str,
    name: String,
    key: Key,
}

impl Signer {
    /// A signer with a new key on a curve.
    pub fn curve() -> Signer {
        let random = SystemRandom::new();
        let made = EcdsaKeyPair::generate_pkcs8(&signature::ECDSA_P256_SHA256_FIXED_SIGNING, &random).expect("a new key");
        let key = EcdsaKeyPair::from_pkcs8(&signature::ECDSA_P256_SHA256_FIXED_SIGNING, made.as_ref(), &random).expect("the key just made");
        Signer { way: "ES256", name: "2026".to_string(), key: Key::Curve(key) }
    }

    /// A signer with the tests' RSA key, that signs this way: `RS256`, `PS256`, `RS512`.
    pub fn rsa(way: &'static str) -> Signer {
        let key = RsaKeyPair::from_pkcs8(include_bytes!("../tests/fixtures/service.key.der")).expect("the tests' key");
        Signer { way, name: "2026".to_string(), key: Key::Rsa(key) }
    }

    /// The same signer, with another name for its key.
    pub fn named(self, name: &str) -> Signer {
        Signer { name: name.to_string(), ..self }
    }

    /// The way it signs.
    pub fn way(&self) -> &'static str {
        self.way
    }

    /// The first part of a token that says it is signed this way with this signer's key.
    pub fn head(&self, way: &str) -> String {
        base64::for_addresses(
            Json::object([("alg", way.into()), ("kid", self.name.as_str().into()), ("typ", "JWT".into())]).write().as_bytes(),
        )
    }

    /// The keys as a sign-in service publishes them.
    pub fn published(&self) -> Json {
        let letters = |bytes: &[u8]| Json::Text(base64::for_addresses(bytes));
        let key = match &self.key {
            Key::Curve(key) => {
                let point = key.public_key().as_ref();
                Json::object([
                    ("kty", "EC".into()),
                    ("crv", "P-256".into()),
                    ("use", "sig".into()),
                    ("kid", self.name.as_str().into()),
                    ("x", letters(&point[1..33])),
                    ("y", letters(&point[33..])),
                ])
            }
            Key::Rsa(key) => {
                let parts: RsaPublicKeyComponents<Vec<u8>> = RsaPublicKeyComponents::from(key.public());
                Json::object([
                    ("kty", "RSA".into()),
                    ("use", "sig".into()),
                    ("kid", self.name.as_str().into()),
                    ("n", letters(&parts.n)),
                    ("e", letters(&parts.e)),
                ])
            }
        };
        Json::object([("keys", Json::List(vec![key]))])
    }

    /// A token that says this, signed.
    pub fn sign(&self, says: &Json) -> String {
        let random = SystemRandom::new();
        let message = format!("{}.{}", self.head(self.way), base64::for_addresses(says.write().as_bytes()));
        let signed = match &self.key {
            Key::Curve(key) => key.sign(&random, message.as_bytes()).expect("a signature").as_ref().to_vec(),
            Key::Rsa(key) => {
                let how: &dyn signature::RsaEncoding = match self.way {
                    "RS256" => &signature::RSA_PKCS1_SHA256,
                    "RS384" => &signature::RSA_PKCS1_SHA384,
                    "RS512" => &signature::RSA_PKCS1_SHA512,
                    "PS256" => &signature::RSA_PSS_SHA256,
                    "PS384" => &signature::RSA_PSS_SHA384,
                    _ => &signature::RSA_PSS_SHA512,
                };
                let mut signed = vec![0; key.public().modulus_len()];
                key.sign(how, &random, message.as_bytes(), &mut signed).expect("a signature");
                signed
            }
        };
        format!("{message}.{}", base64::for_addresses(&signed))
    }
}
