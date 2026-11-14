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
    /// A message of the sign-in before the desktop is not what such a message is.
    SignIn(&'static str),
    /// The machine refused the sign-in before the desktop, with this code of its own.
    SignInRefused(u32),
    /// The machine took the sign-in and says this account may not come in over RDP, with
    /// this code of its own.
    MayNotComeIn(u32),
    /// The machine did something this version of Away's code doesn't speak yet.
    NotYet(&'static str),
    /// The machine ended the connection, with this reason of its own where it gave one.
    Ended(u32),
}

/// Why a machine refuses a sign-in, for the codes people meet.
fn refusal(code: u32) -> Option<&'static str> {
    Some(match code {
        0xc000_006d => "the name or the password is wrong",
        0xc000_006a => "the password is wrong",
        0xc000_0064 => "it knows no account of that name",
        0xc000_006e => "the account may not sign in here, or not now",
        0xc000_006f => "the account may not sign in at this hour",
        0xc000_0070 => "the account may not sign in from this device",
        0xc000_0071 => "the account's password has run out",
        0xc000_0072 => "the account is disabled",
        0xc000_0193 => "the account has run out",
        0xc000_0224 => "the account's password has to be changed first",
        0xc000_0234 => "the account is locked",
        0xc000_015b => "the account is not allowed this kind of sign-in on this machine",
        0xc000_0022 => "it refuses access",
        _ => return None,
    })
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
            Error::SignIn(what) => write!(out, "What it sent for the sign-in is not what the sign-in is: {what}."),
            Error::SignInRefused(code) => match refusal(*code) {
                Some(why) => write!(out, "It refused the sign-in: {why} (its own code {code:#010x})."),
                None => write!(out, "It refused the sign-in, with its own code {code:#010x}."),
            },
            Error::NotYet(what) => write!(out, "It {what}, which this version doesn't speak yet."),
            Error::Ended(0) => out.write_str("It ended the connection and gave no reason."),
            Error::Ended(code) => write!(out, "It ended the connection, with its own reason {code:#010x}."),
            Error::MayNotComeIn(5) => out.write_str("It took the sign-in, and says this account may not come in over RDP here."),
            Error::MayNotComeIn(code) => write!(out, "It took the sign-in, and then refused the account, with its own code {code:#010x}."),
        }
    }
}

impl std::error::Error for Error {}
