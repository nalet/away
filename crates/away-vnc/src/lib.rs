//! Away's own VNC code: the protocol a Mac's own screen sharing and most desktops on
//! Linux speak (RFB, RFC 6143).
//!
//! It takes bytes and gives bytes and what happened. It opens no connection, reads no
//! clock and starts no thread: whoever uses it does that around it, so the same code is
//! tested with recorded bytes, and run wherever it is needed.
//!
//! | Module | Does |
//! |---|---|
//! | [`connection`] | a connection from the machine's first word to its open desktop: the version, the sign-in, the desktop's picture, keys and the pointer, text on the clipboard |
//! | [`password`] | answers the machine's question for its password |
//! | [`picture`] | the desktop's picture, into which every rectangle the machine draws is put |
//! | [`zrle`] | unpacks the rectangles a machine packs most densely |
//!
//! The cryptography is never this crate's own: the cipher of the password's answer and
//! the unpacking come from libraries. A connection over VNC is not secured by the
//! protocol itself: what travels on it, the picture and the keys, travels as it is.

pub mod connection;
pub mod password;
pub mod picture;
#[cfg(any(test, feature = "stand-in"))]
pub mod stand_in;
pub mod zrle;

use std::fmt;

/// What went wrong.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// What arrived is not what a machine that speaks VNC sends at this point.
    NotVnc(&'static str),
    /// The machine did something this version of Away's code doesn't speak yet.
    NotYet(&'static str),
    /// The machine did not take the password, and said this where it said why.
    PasswordRefused(String),
    /// The machine ended the conversation before its desktop, and said this where it said why.
    Refused(String),
}

impl fmt::Display for Error {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let said = |why: &String| if why.is_empty() { String::new() } else { format!(": {why}") };
        match self {
            Error::NotVnc(what) => write!(out, "this is not VNC: {what}"),
            Error::NotYet(what) => write!(out, "the machine {what}, which Away doesn't do yet"),
            Error::PasswordRefused(why) => write!(out, "the machine did not take the password{}", said(why)),
            Error::Refused(why) => write!(out, "the machine ended the connection before its desktop{}", said(why)),
        }
    }
}

impl std::error::Error for Error {}
