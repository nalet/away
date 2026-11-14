//! What Away keeps while it runs, in one PostgreSQL database that only the server
//! reaches ([ADR-0009](../../../spec/architecture/decisions/0009-one-database-and-files.md)).
//!
//! The server speaks to the database with code of its own: the messages in [`wire`],
//! the proof of the password in [`scram`], one connection in [`link`]. The hashes and
//! what secures the connection are libraries'.
//!
//! The server changes the database's shape itself, forward only: each step of
//! [`SHAPE`] is taken once, in its order, and the database remembers which were taken.

pub mod favourites;
pub mod link;
pub mod scram;
pub mod wire;

use away_part::Lines;
use tokio::sync::Mutex;

use link::Link;
pub use link::{Answer, Failed, Settings};

/// The database's shape, as steps: what each is for, and what it does. A step is never
/// changed once a version with it was published; what comes later is a step more.
pub const SHAPE: &[(&str, &str)] = &[(
    "what people starred",
    "create table favourite (
        person text not null,
        machine text not null,
        program text not null default '',
        since timestamptz not null default now(),
        primary key (person, machine, program)
    )",
)];

/// The number the servers of one database agree on, so that one of them at a time
/// changes its shape.
const ONE_AT_A_TIME: &str = "7262972018";

/// Why the store can't be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unusable {
    /// The database gave nothing.
    Database(Failed),
    /// The database is in the shape of a later version of Away than this one, which
    /// knows only this many steps.
    LaterShape {
        /// How many steps the database has taken.
        taken: usize,
        /// How many this version knows.
        known: usize,
    },
}

impl std::fmt::Display for Unusable {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unusable::Database(failed) => failed.fmt(out),
            Unusable::LaterShape { taken, known } => write!(
                out,
                "the database is in the shape of a later version of Away: it has taken {taken} steps, and this version knows {known}"
            ),
        }
    }
}

impl From<Failed> for Unusable {
    fn from(failed: Failed) -> Self {
        Unusable::Database(failed)
    }
}

/// The connection, and whether the database is known to be in shape.
#[derive(Default)]
struct Held {
    link: Option<Link>,
    in_shape: bool,
}

/// What Away keeps, as the server reaches it.
pub struct Store {
    settings: Settings,
    lines: Lines,
    held: Mutex<Held>,
}

impl Store {
    /// A store at this database. Nothing is opened until something is asked.
    pub fn new(settings: Settings, lines: Lines) -> Store {
        Store { settings, lines, held: Mutex::new(Held::default()) }
    }

    /// Reaches the database and brings it into shape, so that what keeps the store from
    /// being used shows when the server starts and not when somebody needs it.
    pub async fn begin(&self) -> Result<(), Unusable> {
        let mut held = self.held.lock().await;
        self.ready(&mut held).await.map(|_| ())
    }

    /// Asks one statement, with its values apart from it.
    ///
    /// A connection that the database closed while nothing was asked is opened anew,
    /// and the statement is asked once more.
    pub async fn ask(&self, statement: &str, values: &[Option<&str>]) -> Result<Answer, Unusable> {
        let mut held = self.held.lock().await;
        let kept = held.link.is_some();
        let link = self.ready(&mut held).await?;
        match link.ask(statement, values).await {
            Err(Failed::Gone(_)) if kept => {
                held.link = None;
                let link = self.ready(&mut held).await?;
                self.kept_or_dropped(link.ask(statement, values).await, &mut held)
            }
            answered => self.kept_or_dropped(answered, &mut held),
        }
    }

    /// A connection that failed in a way it doesn't come back from is not kept.
    fn kept_or_dropped(&self, answered: Result<Answer, Failed>, held: &mut Held) -> Result<Answer, Unusable> {
        if matches!(answered, Err(Failed::Gone(_) | Failed::NoAnswerInTime | Failed::NotADatabase(_))) {
            held.link = None;
        }
        answered.map_err(Unusable::Database)
    }

    /// The connection, opened if there is none, to a database that is in shape.
    async fn ready<'held>(&self, held: &'held mut Held) -> Result<&'held mut Link, Unusable> {
        if held.link.is_none() {
            let mut fresh = [0u8; 18];
            getrandom::fill(&mut fresh).map_err(|error| Failed::NotReached(format!("the system gave no random bytes: {error}")))?;
            held.link = Some(Link::open(&self.settings, &fresh).await?);
        }
        if !held.in_shape {
            let link = held.link.as_mut().expect("opened above");
            match into_shape(link, SHAPE).await {
                Ok(taken) => {
                    held.in_shape = true;
                    if taken > 0 {
                        self.lines.info("the database was brought into shape", &[("steps", (taken as u64).into())]);
                    }
                }
                Err(unusable) => {
                    // whatever was begun is undone as the connection ends
                    held.link = None;
                    return Err(unusable);
                }
            }
        }
        Ok(held.link.as_mut().expect("opened above"))
    }
}

/// Brings a database into the shape these steps describe: takes each step it has not
/// taken yet, in their order, each whole or not at all. Gives how many it took.
pub async fn into_shape(link: &mut Link, steps: &[(&str, &str)]) -> Result<usize, Unusable> {
    // one server at a time: another that starts beside this one waits here
    link.ask(&format!("select pg_advisory_lock({ONE_AT_A_TIME})"), &[]).await?;
    link.run(
        "create table if not exists away_shape (step integer primary key, what text not null, since timestamptz not null default now())",
    )
    .await?;
    let taken = link.ask("select count(*), coalesce(max(step), 0) from away_shape", &[]).await?;
    let taken: usize = taken
        .rows
        .first()
        .and_then(|row| row.get(1).cloned().flatten())
        .and_then(|step| step.parse().ok())
        .ok_or(Failed::NotADatabase("it does not say which steps of its shape it has taken"))?;
    if taken > steps.len() {
        return Err(Unusable::LaterShape { taken, known: steps.len() });
    }
    for (nth, (what, step)) in steps.iter().enumerate().skip(taken) {
        // the step and the note that it was taken, together or not at all
        link.run("begin").await?;
        link.run(step).await?;
        link.ask("insert into away_shape (step, what) values ($1::int, $2)", &[Some(&(nth + 1).to_string()), Some(what)]).await?;
        link.run("commit").await?;
    }
    link.ask(&format!("select pg_advisory_unlock({ONE_AT_A_TIME})"), &[]).await?;
    Ok(steps.len() - taken)
}
