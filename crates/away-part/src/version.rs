//! The version that runs.

/// The version of Away, as the workspace names it.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The commit it was built from, when the build was told: `AWAY_COMMIT`. A build on
/// one's own machine that wasn't told says `unknown`.
pub const COMMIT: &str = match option_env!("AWAY_COMMIT") {
    Some(commit) => commit,
    None => "unknown",
};
