//! What Away keeps while it runs, in one PostgreSQL database that only the server
//! reaches ([ADR-0009](../../../spec/architecture/decisions/0009-one-database-and-files.md)).
//!
//! The server speaks to the database with the client that most programs in this language
//! use ([ADR-0015](../../../spec/architecture/decisions/0015-a-widely-used-database-client.md)).
//! What is Away's own is here: where the database is and how its connection is secured,
//! one connection that is opened anew when it is gone, and the database's shape.
//!
//! The server changes the database's shape itself, forward only: each step of
//! [`SHAPE`] is taken once, in its order, and the database remembers which were taken.

pub mod favourites;
pub mod identities;
pub mod records;

use std::time::Duration;

use away_part::Lines;
use away_secret::Secret;
use tokio::sync::Mutex;
use tokio_postgres::config::SslMode;
use tokio_postgres::types::ToSql;
use tokio_postgres::{Client, NoTls, Row};
use tokio_postgres_rustls::MakeRustlsConnect;

/// The database's shape, as steps: what each is for, and what it does. A step is never
/// changed once a version with it was published; what comes later is a step more.
pub const SHAPE: &[(&str, &str)] = &[
    (
        "what people starred",
        "create table favourite (
            person text not null,
            machine text not null,
            program text not null default '',
            since timestamptz not null default now(),
            primary key (person, machine, program)
        )",
    ),
    (
        "the records, which are only ever added",
        "create table record (
            number bigint generated always as identity primary key,
            key text not null unique,
            at timestamptz not null default now(),
            kind text not null,
            person text,
            person_name text,
            machine text,
            details text not null
        );
        create function record_stays() returns trigger language plpgsql as $record$
        begin
            raise exception 'a record is never changed and never removed' using errcode = 'restrict_violation';
        end $record$;
        create trigger record_is_not_changed before update or delete on record for each row execute function record_stays();
        create trigger record_is_not_emptied before truncate on record for each statement execute function record_stays();
        create index record_by_kind on record (kind, machine, number)",
    ),
    (
        "what each machine showed of itself the first time",
        "create table machine_identity (
            machine text primary key,
            fingerprint text not null,
            since timestamptz not null default now()
        )",
    ),
];

/// The number the servers of one database agree on, so that one of them at a time
/// changes its shape.
const ONE_AT_A_TIME: i64 = 7_262_972_018;

/// How the connection to the database is secured.
#[derive(Clone)]
pub enum Secured {
    /// Secured, and the database shows a certificate for its name that an authority
    /// the server trusts signed.
    Verified(rustls::ClientConfig),
    /// Encrypted, and the database's certificate is not checked.
    Encrypted(rustls::ClientConfig),
    /// Not at all.
    Plain,
}

/// Where the database is, and as whom the server signs in there.
#[derive(Clone)]
pub struct Settings {
    /// The database server's name on the network.
    pub host: String,
    /// Its port.
    pub port: u16,
    /// The database of that server which is Away's.
    pub database: String,
    /// The account the server signs in as.
    pub account: String,
    /// The account's password.
    pub password: Secret,
    /// How the connection is secured.
    pub secured: Secured,
    /// How long opening the connection, or one statement, may take.
    pub within: Duration,
}

/// Why the database gave nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failed {
    /// It was not reached.
    NotReached(String),
    /// The connection could not be secured.
    NotSecured(String),
    /// It did not take the account, or has no such database.
    NotSignedIn(String),
    /// It refused the statement.
    Refused {
        /// The database's code for the reason, five characters.
        code: String,
        /// What it says.
        message: String,
    },
    /// It took longer than it may.
    NoAnswerInTime,
    /// The connection ended, or what came over it could not be used.
    Gone(String),
}

impl std::fmt::Display for Failed {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Failed::NotReached(why) => write!(out, "the database was not reached: {why}"),
            Failed::NotSecured(why) => write!(out, "the connection to the database could not be secured: {why}"),
            Failed::NotSignedIn(why) => write!(out, "the database did not take the account: {why}"),
            Failed::Refused { code, message } => write!(out, "the database refused it: {message} ({code})"),
            Failed::NoAnswerInTime => out.write_str("the database gave no answer in time"),
            Failed::Gone(why) => write!(out, "the connection to the database ended: {why}"),
        }
    }
}

/// What the client's error means for Away. `opening` says whether it came while the
/// connection was opened, where a refusal is about the account and not about a statement.
fn failed(error: &tokio_postgres::Error, opening: bool) -> Failed {
    use std::error::Error;
    let reason = error.source().map_or_else(|| error.to_string(), ToString::to_string);
    if let Some(refused) = error.as_db_error() {
        let (code, message) = (refused.code().code(), refused.message().to_string());
        return match opening {
            // the account, its password, or the database it asks for
            true if code.starts_with("28") || code.starts_with("3D") => Failed::NotSignedIn(message),
            // a database that is starting up, shutting down, or full
            true => Failed::NotReached(message),
            // the database ended the connection itself, as it does when it is stopped
            false if code.starts_with("57P") => Failed::Gone(message),
            false => Failed::Refused { code: code.to_string(), message },
        };
    }
    let said = error.to_string();
    if error.is_closed() {
        Failed::Gone(said)
    } else if said.starts_with("error performing TLS handshake") {
        Failed::NotSecured(reason)
    } else if said.starts_with("timeout waiting for server") {
        Failed::NoAnswerInTime
    } else if opening && said.starts_with("error connecting to server") {
        Failed::NotReached(reason)
    } else if opening {
        Failed::NotReached(said)
    } else {
        Failed::Gone(said)
    }
}

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
    client: Option<Client>,
    in_shape: bool,
}

/// What Away keeps, as the server reaches it.
pub struct Store {
    settings: Settings,
    shape: Vec<(String, String)>,
    lines: Lines,
    held: Mutex<Held>,
}

impl Store {
    /// A store at this database. Nothing is opened until something is asked.
    pub fn new(settings: Settings, lines: Lines) -> Store {
        Store::with_shape(settings, lines, SHAPE)
    }

    /// A store that brings its database into another shape than this version's: for
    /// the tests, which try what a later version does.
    pub fn with_shape(settings: Settings, lines: Lines, steps: &[(&str, &str)]) -> Store {
        let shape = steps.iter().map(|(what, step)| (what.to_string(), step.to_string())).collect();
        Store { settings, shape, lines, held: Mutex::new(Held::default()) }
    }

    /// Reaches the database and brings it into shape, so that what keeps the store from
    /// being used shows when the server starts and not when somebody needs it.
    pub async fn begin(&self) -> Result<(), Unusable> {
        let mut held = self.held.lock().await;
        self.ready(&mut held).await.map(|_| ())
    }

    /// Asks the database one statement on the store's connection, with its values apart
    /// from it, within the time a statement may take. Gives the rows, and how many rows
    /// it changed where it gives none. A connection that failed in a way it doesn't come
    /// back from is not kept.
    ///
    /// A connection that the database closed while nothing was asked shows as closed
    /// only when it is used: what was asked on it is then asked once more, on a new one.
    /// So what is asked this way is something that may be asked twice without harm.
    async fn asked(&self, statement: &str, values: &[&(dyn ToSql + Sync)], gives_rows: bool) -> Result<(Vec<Row>, u64), Unusable> {
        let mut held = self.held.lock().await;
        let mut kept = held.client.is_some();
        loop {
            let client = self.ready(&mut held).await?;
            let asking = async {
                match gives_rows {
                    true => client.query(statement, values).await.map(|rows| (rows, 0)),
                    false => client.execute(statement, values).await.map(|changed| (Vec::new(), changed)),
                }
            };
            let answered = match tokio::time::timeout(self.settings.within, asking).await {
                Ok(Ok(answer)) => return Ok(answer),
                Ok(Err(error)) => failed(&error, false),
                Err(_) => Failed::NoAnswerInTime,
            };
            if matches!(answered, Failed::Gone(_) | Failed::NoAnswerInTime) {
                held.client = None;
            }
            if !(kept && matches!(answered, Failed::Gone(_))) {
                return Err(Unusable::Database(answered));
            }
            // once more, on a connection that is new
            kept = false;
        }
    }

    /// The rows a statement gives.
    async fn rows(&self, statement: &str, values: &[&(dyn ToSql + Sync)]) -> Result<Vec<Row>, Unusable> {
        self.asked(statement, values, true).await.map(|(rows, _)| rows)
    }

    /// Runs a statement that gives no rows, and says how many it changed.
    async fn change(&self, statement: &str, values: &[&(dyn ToSql + Sync)]) -> Result<u64, Unusable> {
        self.asked(statement, values, false).await.map(|(_, changed)| changed)
    }

    /// The connection, opened if there is none, to a database that is in shape.
    async fn ready<'held>(&self, held: &'held mut Held) -> Result<&'held Client, Unusable> {
        if held.client.as_ref().is_none_or(Client::is_closed) {
            held.client = Some(self.open().await?);
        }
        if !held.in_shape {
            let client = held.client.as_ref().expect("opened above");
            let shaping = tokio::time::timeout(self.settings.within, into_shape(client, &self.shape));
            match shaping.await.unwrap_or(Err(Unusable::Database(Failed::NoAnswerInTime))) {
                Ok(taken) => {
                    held.in_shape = true;
                    if taken > 0 {
                        self.lines.info("the database was brought into shape", &[("steps", (taken as u64).into())]);
                    }
                }
                Err(unusable) => {
                    // whatever was begun is undone as the connection ends
                    held.client = None;
                    return Err(unusable);
                }
            }
        }
        Ok(held.client.as_ref().expect("opened above"))
    }

    /// Opens a connection and signs in.
    async fn open(&self) -> Result<Client, Failed> {
        let settings = &self.settings;
        let mut config = tokio_postgres::Config::new();
        config
            .host(&settings.host)
            .port(settings.port)
            .dbname(&settings.database)
            .user(&settings.account)
            .password(settings.password.expose())
            .application_name("away")
            .connect_timeout(settings.within);
        let opening = async {
            match &settings.secured {
                Secured::Plain => {
                    let (client, connection) = config.ssl_mode(SslMode::Disable).connect(NoTls).await?;
                    // the connection is carried until the client is dropped, or the database ends it
                    tokio::spawn(async move {
                        let _ = connection.await;
                    });
                    Ok(client)
                }
                Secured::Verified(secures) | Secured::Encrypted(secures) => {
                    let secures = MakeRustlsConnect::new(secures.clone());
                    let (client, connection) = config.ssl_mode(SslMode::Require).connect(secures).await?;
                    tokio::spawn(async move {
                        let _ = connection.await;
                    });
                    Ok(client)
                }
            }
        };
        match tokio::time::timeout(settings.within, opening).await {
            Ok(Ok(client)) => Ok(client),
            Ok(Err(error)) => Err(failed(&error, true)),
            Err(_) => Err(Failed::NoAnswerInTime),
        }
    }
}

/// Brings a database into the shape these steps describe: takes each step it has not
/// taken yet, in their order, each whole or not at all. Gives how many it took.
async fn into_shape(client: &Client, steps: &[(String, String)]) -> Result<usize, Unusable> {
    let asked = |error: tokio_postgres::Error| Unusable::Database(failed(&error, false));
    // one server at a time: another that starts beside this one waits here
    client.query("select pg_advisory_lock($1)", &[&ONE_AT_A_TIME]).await.map_err(asked)?;
    client
        .batch_execute(
            "create table if not exists away_shape (step integer primary key, what text not null, since timestamptz not null default now())",
        )
        .await
        .map_err(asked)?;
    let taken: i32 = client.query_one("select coalesce(max(step), 0) from away_shape", &[]).await.map_err(asked)?.get(0);
    let taken = usize::try_from(taken).unwrap_or(0);
    if taken > steps.len() {
        return Err(Unusable::LaterShape { taken, known: steps.len() });
    }
    for (nth, (what, step)) in steps.iter().enumerate().skip(taken) {
        // the step and the note that it was taken, together or not at all
        client.batch_execute("begin").await.map_err(asked)?;
        client.batch_execute(step).await.map_err(asked)?;
        let number = i32::try_from(nth + 1).unwrap_or(i32::MAX);
        client.execute("insert into away_shape (step, what) values ($1, $2)", &[&number, what]).await.map_err(asked)?;
        client.batch_execute("commit").await.map_err(asked)?;
    }
    client.query("select pg_advisory_unlock($1)", &[&ONE_AT_A_TIME]).await.map_err(asked)?;
    Ok(steps.len() - taken)
}
