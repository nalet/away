//! Signing in on a Windows machine as oneself, with nothing typed (ADR-0007).
//!
//! A person who is signed in to Away has no password to show a machine. In its place
//! Away shows a certificate that lasts minutes: first to the domain, over Kerberos, and
//! then to the machine, as a smart card that exists in memory alone.
//!
//! Like the RDP code, this takes bytes and gives bytes and what happened, and opens no
//! connection itself. The ciphers and the hashes are libraries'; what the standards
//! build from them is written here and tried against the standards' own examples.

pub mod card;
pub mod certificate;
pub mod der;
pub mod kerberos;
pub mod sign_in;

mod error;

pub use error::Error;
