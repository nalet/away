//! A secret: a value that can be used and can't be printed.
//!
//! A password, a key or a token is held in one of these from the moment it is read. It
//! has no way to be written to a line, a record or a page: asking to print it gives
//! three dots.

use std::fmt;

/// A secret text, such as a password.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    /// Takes a text as a secret.
    pub fn new(text: impl Into<String>) -> Self {
        Secret(text.into())
    }

    /// The secret itself, for the one place that has to use it.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str("Secret(...)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_is_used_and_never_printed() {
        let secret = Secret::new("correct horse battery staple");
        assert_eq!(secret.expose(), "correct horse battery staple");
        assert_eq!(format!("{secret:?}"), "Secret(...)");
        assert_eq!(format!("{:?}", Some(secret.clone())), "Some(Secret(...))");
    }
}
