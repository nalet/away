//! What a person starred: machines, and programs of machines. They are kept with the
//! person, so they are the same on every device.

use super::{Store, Unusable};

/// One thing a person starred.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Favourite {
    /// The machine, by its name.
    pub machine: String,
    /// The program of that machine, where it is a program that was starred.
    pub program: Option<String>,
}

impl Store {
    /// What a person starred, the first starred first.
    pub async fn favourites(&self, person: &str) -> Result<Vec<Favourite>, Unusable> {
        let rows =
            self.rows("select machine, program from favourite where person = $1 order by since, machine, program", &[&person]).await?;
        Ok(rows
            .iter()
            .map(|row| {
                let program: String = row.get(1);
                Favourite { machine: row.get(0), program: Some(program).filter(|program| !program.is_empty()) }
            })
            .collect())
    }

    /// Stars something for a person, or takes the star away again. Starring what is
    /// starred, or taking a star from what has none, changes nothing.
    pub async fn star(&self, person: &str, favourite: &Favourite, starred: bool) -> Result<(), Unusable> {
        let program = favourite.program.as_deref().unwrap_or("");
        let statement = if starred {
            "insert into favourite (person, machine, program) values ($1, $2, $3) on conflict do nothing"
        } else {
            "delete from favourite where person = $1 and machine = $2 and program = $3"
        };
        self.change(statement, &[&person, &favourite.machine, &program]).await.map(|_| ())
    }
}
