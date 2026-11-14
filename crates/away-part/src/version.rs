//! The version that runs.

/// The version of Away, as the workspace names it.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Where its source is.
pub const SOURCE: &str = env!("CARGO_PKG_REPOSITORY");

/// The licence Away is under, by its short name.
pub const LICENCE: &str = env!("CARGO_PKG_LICENSE");

/// The commit it was built from, when the build was told: `AWAY_COMMIT`. A build on
/// one's own machine that wasn't told says `unknown`.
pub const COMMIT: &str = match option_env!("AWAY_COMMIT") {
    Some(commit) => commit,
    None => "unknown",
};
