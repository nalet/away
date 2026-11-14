//! The records: what happened, kept so that it can be read later. They are only ever
//! added: the database itself refuses to change or to remove one.

use away_part::json::Json;

use super::{Store, Unusable};

/// One record, as it is added.
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    /// What names this record alone, made by whoever adds it: adding it twice, after a
    /// connection that broke in the middle, adds it once.
    pub key: String,
    /// What happened, as one word of a known kind.
    pub kind: String,
    /// Who did it: as the sign-in service names them, and by their name. Nobody where
    /// Away did it itself.
    pub person: Option<(String, String)>,
    /// The machine or the farm it is about, where it is about one.
    pub machine: Option<String>,
    /// The rest of it.
    pub details: Json,
}

impl Store {
    /// Adds a record.
    pub async fn record(&self, record: &Record) -> Result<(), Unusable> {
        let (person, name) = match &record.person {
            Some((person, name)) => (Some(person.as_str()), Some(name.as_str())),
            None => (None, None),
        };
        let details = record.details.write();
        self.change(
            "insert into record (key, kind, person, person_name, machine, details) values ($1, $2, $3, $4, $5, $6) on conflict (key) do nothing",
            &[&record.key, &record.kind, &person, &name, &record.machine, &details],
        )
        .await
        .map(|_| ())
    }

    /// The last record of these kinds about this machine or farm, or about none: its
    /// kind and its details, as they were written.
    pub async fn last_record(&self, kinds: &[&str], machine: Option<&str>) -> Result<Option<(String, String)>, Unusable> {
        let rows = self
            .rows(
                "select kind, details from record where kind = any($1) and machine is not distinct from $2 order by number desc limit 1",
                &[&kinds, &machine],
            )
            .await?;
        Ok(rows.first().map(|row| (row.get(0), row.get(1))))
    }
}
