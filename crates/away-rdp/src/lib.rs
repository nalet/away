//! Away's own RDP code.
//!
//! It takes bytes and gives bytes and what happened. It opens no connection, reads no
//! clock and starts no thread: whoever uses it does that around it, so the same code is
//! tested with recorded bytes, fuzzed, and run wherever it is needed.
//!
//! What is here so far is the beginning of a connection:
//!
//! | Module | Does |
//! |---|---|
//! | [`tpkt`] | cuts the bytes of a connection into the frames RDP travels in |
//! | [`negotiation`] | asks a machine which security it takes, and reads its answer |
//! | [`identity`] | reads what a machine shows of itself: its certificate |
//! | [`sign_in`] | the sign-in before the desktop, as far as the machine saying who it is |
//! | [`ntlm`] | the messages of the way of signing in that needs no directory |
//! | [`der`] | reads and writes the encoding certificates and the sign-in are written in |
//! | [`secret`] | a value that can be used and can't be printed |
//! | [`licence`] | takes the licence that a machine for many people hands every client |
//! | [`planar`] | unpacks the rectangles a machine draws at 32 bits a pixel |
//! | [`interleaved`] | unpacks the rectangles a machine draws at 16 bits a pixel |
//! | [`picture`] | puts the rectangles together as the desktop's picture |
//!
//! The cryptography is never this crate's own: TLS is done by whoever holds the
//! connection, and a hash, a cipher and the arithmetic of large numbers come from
//! libraries.

pub mod connection;
pub mod der;
pub mod gcc;
pub mod identity;
pub mod input;
pub mod interleaved;
pub mod licence;
pub mod mcs;
pub mod negotiation;
pub mod ntlm;
pub mod pdu;
pub mod picture;
pub mod planar;
/// A value that can be used and can't be printed.
pub mod secret {
    pub use away_secret::Secret;
}
pub mod sign_in;
#[cfg(any(test, feature = "stand-in"))]
pub mod stand_in;
pub mod tpkt;
pub mod update;

mod error;

pub use error::{Error, Refusal};
