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
    /// What was given as a certificate can't be read as one.
    NotACertificate(&'static str),
    /// The other side did not prove that it is who it has to be.
    NotTrusted(&'static str),
    /// The machine refused, or what it sent in the sign-in before the desktop is not
    /// what that sign-in is.
    Machine(away_rdp::Error),
    /// The domain controller refused, with this reason as Kerberos numbers them, and in
    /// its own words where it gave some.
    Refused {
        /// The reason, as its number.
        code: i64,
        /// The domain controller's words.
        text: Option<String>,
    },
}

/// What a domain controller's reason for refusing means, where it is one that a sign-in
/// with a certificate meets.
fn refusal(code: i64) -> Option<&'static str> {
    Some(match code {
        6 => "it knows no account of that name",
        7 => "it knows no such service",
        12 => "its rules don't allow this sign-in",
        14 => "it seals in none of the ways the client speaks",
        18 => "the account is disabled, locked or run out",
        23 => "the account's password has run out",
        24 => "the proof of who asks did not hold",
        37 => "its clock and the client's are too far apart",
        62 => "it does not trust the certificate: its authority is not one the domain takes sign-ins from",
        63 => "the client did not trust the domain controller's certificate",
        64 => "the signature made with the certificate's key did not hold",
        65 => "the numbers of the exchange of keys are too small for it",
        66 => "the certificate is for another account than the one that was named",
        70 => "it could not check the certificate",
        71 => "the certificate is not valid: it has run out, or is not good yet",
        72 => "the certificate was withdrawn",
        73 => "it could not find out whether the certificate was withdrawn",
        75 => "the name in the certificate is not the account's",
        77 => "the certificate is not one for signing in",
        78 => "it does not take the way the certificate was signed",
        79 => "it asks for a check on what was asked that the client did not send",
        80 => "it does not take the hash that what was signed was hashed with",
        81 => "it does not do the exchange of keys the client asked for",
        _ => return None,
    })
}

impl fmt::Display for Error {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotKerberos(what) => write!(out, "What it sent is not Kerberos: {what}."),
            Error::NotOpened(what) => write!(out, "What it sealed could not be opened: {what}."),
            Error::NotACertificate(what) => write!(out, "The certificate can't be read: {what}."),
            Error::NotTrusted(what) => write!(out, "It did not prove who it is: {what}."),
            Error::Machine(why) => write!(out, "{why}"),
            Error::Refused { code, text } => {
                match refusal(*code) {
                    Some(why) => write!(out, "The domain controller refused: {why} (its own code {code})")?,
                    None => write!(out, "The domain controller refused, with its own code {code}")?,
                }
                match text.as_deref().filter(|text| !text.is_empty()) {
                    Some(text) => write!(out, ". It says: {text}."),
                    None => out.write_str("."),
                }
            }
        }
    }
}

impl std::error::Error for Error {}
