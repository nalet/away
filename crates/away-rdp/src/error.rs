//! What can go wrong, in words that say what happened.

use std::fmt;

/// Why a machine refused the security Away asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// It takes only connections over TLS, and was asked for none.
    TlsRequired,
    /// It takes no connection over TLS.
    TlsNotAllowed,
    /// It has no certificate to show.
    NoCertificate,
    /// It found the request contradictory.
    InconsistentRequest,
    /// It takes only connections that sign in before the desktop.
    SignInFirstRequired,
    /// It takes only connections over TLS that sign in before the desktop.
    SignInFirstOverTlsRequired,
    /// It gave a reason this code doesn't know.
    Other(u32),
}

/// What went wrong.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// What arrived is not what an RDP machine sends at this point.
    NotRdp(&'static str),
    /// The machine refused the security it was asked for.
    Refused(Refusal),
    /// The machine offers only RDP's own old encryption, which Away doesn't use.
    OldSecurityOnly,
    /// The machine chose a security it was not asked for.
    NotAskedFor(u32),
    /// The certificate can't be read.
    Certificate(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotRdp(what) => write!(out, "What it sent is not RDP: {what}."),
            Error::Refused(Refusal::TlsRequired) => out.write_str("It takes only connections over TLS."),
            Error::Refused(Refusal::TlsNotAllowed) => out.write_str("It takes no connection over TLS."),
            Error::Refused(Refusal::NoCertificate) => out.write_str("It has no certificate to show."),
            Error::Refused(Refusal::InconsistentRequest) => out.write_str("It found the request contradictory."),
            Error::Refused(Refusal::SignInFirstRequired) => out.write_str("It takes only connections that sign in before the desktop."),
            Error::Refused(Refusal::SignInFirstOverTlsRequired) => {
                out.write_str("It takes only connections over TLS that sign in before the desktop.")
            }
            Error::Refused(Refusal::Other(code)) => write!(out, "It refused, with a reason this version doesn't know: {code}."),
            Error::OldSecurityOnly => out.write_str("It offers only RDP's own old encryption, which Away doesn't use."),
            Error::NotAskedFor(chosen) => write!(out, "It chose a security it was not asked for: {chosen}."),
            Error::Certificate(what) => write!(out, "Its certificate can't be read: {what}."),
        }
    }
}

impl std::error::Error for Error {}
