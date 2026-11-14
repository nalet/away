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
//! | [`der`] | reads the encoding certificates are written in |
//!
//! The cryptography is never this crate's own: TLS is done by whoever holds the
//! connection, and a hash comes from a library.

pub mod der;
pub mod identity;
pub mod negotiation;
pub mod tpkt;

mod error;

pub use error::{Error, Refusal};
