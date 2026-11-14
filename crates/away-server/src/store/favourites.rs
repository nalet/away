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
        let answer =
            self.ask("select machine, program from favourite where person = $1 order by since, machine, program", &[Some(person)]).await?;
        Ok(answer
            .rows
            .into_iter()
            .filter_map(|row| {
                let mut values = row.into_iter();
                let machine = values.next()??;
                let program = values.next()?.filter(|program| !program.is_empty());
                Some(Favourite { machine, program })
            })
            .collect())
    }

    /// Stars something for a person, or takes the star away again. Starring what is
    /// starred, or taking a star from what has none, changes nothing.
    pub async fn star(&self, person: &str, favourite: &Favourite, starred: bool) -> Result<(), Unusable> {
        let values = [Some(person), Some(favourite.machine.as_str()), Some(favourite.program.as_deref().unwrap_or(""))];
        let statement = if starred {
            "insert into favourite (person, machine, program) values ($1, $2, $3) on conflict do nothing"
        } else {
            "delete from favourite where person = $1 and machine = $2 and program = $3"
        };
        self.ask(statement, &values).await.map(|_| ())
    }
}
