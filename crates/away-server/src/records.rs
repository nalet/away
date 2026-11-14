//! Keeping records: each is added to the database, and put out as a line on the
//! server's output at the moment it is written, so that the organisation's log system
//! holds a copy Away can't touch
//! ([ADR-0009](../../../spec/architecture/decisions/0009-one-database-and-files.md)).
//!
//! Without a database nothing is kept: what happens is on the server's output alone.

use std::sync::Arc;

use away_part::lines::Lines;

use crate::register::files::{Change, FARM_CHANGE, FILE_CHANGES};
use crate::store::records::Record;
use crate::store::{Store, Unusable};

/// What adds records.
#[derive(Clone)]
pub struct Records {
    store: Option<Arc<Store>>,
    lines: Lines,
}

impl Records {
    /// Records that are kept in this store, where there is one.
    pub fn new(store: Option<Arc<Store>>, lines: Lines) -> Records {
        Records { store, lines }
    }

    /// Whether records are kept at all.
    pub fn are_kept(&self) -> bool {
        self.store.is_some()
    }

    /// A key for a new record: random, and so no other record's.
    pub fn key() -> Result<String, String> {
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).map_err(|error| format!("the system gave no random bytes: {error}"))?;
        Ok(random.iter().map(|byte| format!("{byte:02x}")).collect())
    }

    /// Adds a record, and puts it out as a line once it is written. Where nothing is
    /// kept, it says so: whoever asks decides whether what they do may go on without.
    pub async fn add(&self, record: &Record) -> Result<(), Unusable> {
        let Some(store) = &self.store else {
            return Err(Unusable::Database(crate::store::Failed::NotReached("Away is given no database".to_string())));
        };
        store.record(record).await?;
        let details = record.details.write();
        let (person, name) = record.person.as_ref().map_or(("", ""), |(person, name)| (person.as_str(), name.as_str()));
        self.lines.info(
            "record",
            &[
                ("kind", record.kind.as_str().into()),
                ("person", person.into()),
                ("name", name.into()),
                ("machine", record.machine.as_deref().unwrap_or("").into()),
                ("details", details.as_str().into()),
            ],
        );
        Ok(())
    }

    /// Keeps a change of what is in force as a record, where it is one: a state that is
    /// the last one recorded, met again after the server started anew, is no change.
    /// Says whether a record was added.
    pub async fn changed(&self, change: &Change, key: &str) -> Result<bool, Unusable> {
        let Some(store) = &self.store else { return Ok(false) };
        let kinds: &[&str] = if change.kind == FARM_CHANGE { &[FARM_CHANGE] } else { &FILE_CHANGES };
        let details = change.details.write();
        if let Some((kind, last)) = store.last_record(kinds, change.farm.as_deref()).await? {
            if kind == change.kind && last == details {
                return Ok(false);
            }
        }
        let record = Record {
            key: key.to_string(),
            kind: change.kind.to_string(),
            person: None,
            machine: change.farm.clone(),
            details: change.details.clone(),
        };
        self.add(&record).await.map(|()| true)
    }
}
