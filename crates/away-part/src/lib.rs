//! What every part of Away has alike: the lines it writes, the measures it gives, the
//! addresses that say whether it is well, and the way it stops.
//!
//! The server, the gateway and the issuer are each built on this. None of it knows what
//! a part is for.

pub mod address;
pub mod base64;
pub mod client;
pub mod http;
pub mod json;
pub mod lines;
pub mod measures;
pub mod part;
pub mod stop;
pub mod time;
pub mod version;

pub use lines::{Lines, Value};
pub use measures::Measures;
pub use part::{Part, Settings};
pub use stop::{Stop, Stopper};
