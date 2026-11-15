//! What can go wrong, in words that say what happened.

use std::fmt;

/// Why signing in as oneself did not go on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// What arrived is not what Kerberos sends at this point.
    NotKerberos(&'static str),
    /// Something that was sealed could not be opened: it was sealed with another key, or
    /// changed on the way.
    NotOpened(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotKerberos(what) => write!(out, "What it sent is not Kerberos: {what}."),
            Error::NotOpened(what) => write!(out, "What it sealed could not be opened: {what}."),
        }
    }
}

impl std::error::Error for Error {}
