//! Kerberos, as far as signing in with a certificate needs it (RFC 4120, RFC 4556): how
//! its messages are sealed, the messages themselves, and the exchange with a domain
//! controller that ends with a ticket for a machine.

pub mod crypto;
pub mod exchange;
pub mod messages;
pub mod pkinit;
