//! The secrets Away keeps for machines, in its own store: each as an administrator's
//! browser sealed it to the issuer's key, so that neither the server nor the database
//! ever holds one as it is
//! ([ADR-0017](../../../../spec/architecture/decisions/0017-away-keeps-the-secrets-of-machines-sealed.md)).
//! Every server of an installation reads the same, whichever of them took it.

use super::{Store, Unusable};

/// A secret Away keeps, as the administration sees it: nothing of the secret itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kept {
    /// The name machines name it by.
    pub name: String,
    /// The name of the key it is sealed to.
    pub key: String,
    /// Since when it is kept as it is, in seconds since 1970.
    pub since: i64,
    /// Who kept it so, by the name they were known by then.
    pub by: String,
}

impl Store {
    /// The secrets Away keeps, by their names.
    pub async fn kept_secrets(&self) -> Result<Vec<Kept>, Unusable> {
        let rows = self.rows("select name, key, extract(epoch from since)::bigint, by_name from kept_secret order by name", &[]).await?;
        Ok(rows.iter().map(|row| Kept { name: row.get(0), key: row.get(1), since: row.get(2), by: row.get(3) }).collect())
    }

    /// A secret as it is sealed, where one of that name is kept.
    pub async fn sealed_secret(&self, name: &str) -> Result<Option<Vec<u8>>, Unusable> {
        let rows = self.rows("select sealed from kept_secret where name = $1", &[&name]).await?;
        Ok(rows.first().map(|row| row.get(0)))
    }

    /// Keeps a sealed secret under a name, in place of one that was kept so before.
    pub async fn keep_secret(&self, name: &str, sealed: &[u8], key: &str, by: (&str, &str)) -> Result<(), Unusable> {
        let statement = "insert into kept_secret (name, sealed, key, by_person, by_name) values ($1, $2, $3, $4, $5)
            on conflict (name) do update set sealed = excluded.sealed, key = excluded.key, since = now(),
            by_person = excluded.by_person, by_name = excluded.by_name";
        self.change(statement, &[&name, &sealed, &key, &by.0, &by.1]).await.map(|_| ())
    }

    /// Keeps the secret of that name no more. Whether one was kept.
    pub async fn forget_secret(&self, name: &str) -> Result<bool, Unusable> {
        Ok(self.change("delete from kept_secret where name = $1", &[&name]).await? > 0)
    }
}
