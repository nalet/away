//! What each machine showed of itself the first time Away reached it: a machine that
//! shows something else later is not the machine Away knows.

use super::{Store, Unusable};

impl Store {
    /// Keeps what a machine shows of itself, where nothing is known of it yet, and says
    /// what is known of it after: this, or what it showed the first time.
    pub async fn identity(&self, machine: &str, shows: &str) -> Result<String, Unusable> {
        self.change(
            "insert into machine_identity (machine, fingerprint) values ($1, $2) on conflict (machine) do nothing",
            &[&machine, &shows],
        )
        .await?;
        let rows = self.rows("select fingerprint from machine_identity where machine = $1", &[&machine]).await?;
        Ok(rows.first().map_or_else(|| shows.to_string(), |row| row.get(0)))
    }
}
