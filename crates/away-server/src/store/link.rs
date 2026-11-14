//! One connection to the database: opened, secured where it is to be, signed in with a
//! proof of the password, and then asked one statement at a time.

use std::time::Duration;

use away_part::client::{Secured, Secures};
use away_secret::Secret;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use super::scram;
use super::wire::{self, Asks, FromDatabase, Refusal};

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
    /// What secures the connection, where it is to be secured.
    pub secured: Option<Secures>,
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
    /// It did not take the account.
    NotSignedIn(String),
    /// It refused the statement.
    Refused(Refusal),
    /// What answered does not speak as a database does.
    NotADatabase(&'static str),
    /// It took longer than it may.
    NoAnswerInTime,
    /// The connection ended before the statement was answered.
    Gone(String),
}

impl std::fmt::Display for Failed {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Failed::NotReached(why) => write!(out, "the database was not reached: {why}"),
            Failed::NotSecured(why) => write!(out, "the connection to the database could not be secured: {why}"),
            Failed::NotSignedIn(why) => write!(out, "the database did not take the account: {why}"),
            Failed::Refused(refusal) => write!(out, "the database refused it: {} ({})", refusal.message, refusal.code),
            Failed::NotADatabase(why) => write!(out, "what answered is no database: {why}"),
            Failed::NoAnswerInTime => out.write_str("the database gave no answer in time"),
            Failed::Gone(why) => write!(out, "the connection to the database ended: {why}"),
        }
    }
}

/// What a statement gave.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Answer {
    /// The rows: each value as the text the database writes it as, or nothing.
    pub rows: Vec<Vec<Option<String>>>,
    /// How many rows the statement changed.
    pub changed: u64,
}

enum Stream {
    Plain(TcpStream),
    Secured(Box<Secured>),
}

impl Stream {
    async fn send(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        match self {
            Stream::Plain(stream) => stream.write_all(bytes).await,
            Stream::Secured(stream) => {
                stream.write_all(bytes).await?;
                stream.flush().await
            }
        }
    }

    async fn take(&mut self, into: &mut Vec<u8>) -> std::io::Result<usize> {
        match self {
            Stream::Plain(stream) => stream.read_buf(into).await,
            Stream::Secured(stream) => stream.read_buf(into).await,
        }
    }
}

/// One connection, signed in.
pub struct Link {
    stream: Stream,
    arrived: Vec<u8>,
    within: Duration,
}

impl Link {
    /// Opens a connection and signs in. The fresh bytes are random, and new each time.
    pub async fn open(settings: &Settings, fresh: &[u8; 18]) -> Result<Link, Failed> {
        match tokio::time::timeout(settings.within, Link::opened(settings, fresh)).await {
            Ok(opened) => opened,
            Err(_) => Err(Failed::NoAnswerInTime),
        }
    }

    async fn opened(settings: &Settings, fresh: &[u8; 18]) -> Result<Link, Failed> {
        let not_reached = |error: std::io::Error| Failed::NotReached(error.to_string());
        let mut connection = TcpStream::connect((settings.host.as_str(), settings.port)).await.map_err(not_reached)?;
        let stream = match &settings.secured {
            None => Stream::Plain(connection),
            Some(secures) => {
                // the database is asked to secure the connection before anything else is said
                connection.write_all(&wire::SECURE_IT).await.map_err(not_reached)?;
                let mut answer = [0u8; 1];
                connection.read_exact(&mut answer).await.map_err(not_reached)?;
                match answer[0] {
                    b'S' => {}
                    b'N' => return Err(Failed::NotSecured("the database secures no connections".to_string())),
                    _ => return Err(Failed::NotADatabase("it answers the wish for a secured connection with neither yes nor no")),
                }
                Stream::Secured(Box::new(secures.secure(&settings.host, connection).await.map_err(Failed::NotSecured)?))
            }
        };
        let mut link = Link { stream, arrived: Vec::new(), within: settings.within };
        link.send(wire::begin(&settings.account, &settings.database)).await?;

        let mut begun = None;
        let mut proven = None;
        let mut checked = false;
        loop {
            match link.next().await? {
                FromDatabase::SignIn(Asks::Proof(ways)) => {
                    if !ways.iter().any(|way| way == scram::WAY) {
                        return Err(Failed::NotSignedIn(format!("it offers no proof of a password that Away gives: {}", ways.join(", "))));
                    }
                    let (step, says) = scram::begin(fresh);
                    begun = Some(step);
                    link.send(wire::proof_begins(scram::WAY, &says)).await?;
                }
                FromDatabase::SignIn(Asks::Goes(answer)) => {
                    let step = begun.take().ok_or(Failed::NotADatabase("it answers a proof that was not begun"))?;
                    let (step, says) =
                        step.prove(&answer, &settings.password).map_err(|failed| Failed::NotSignedIn(failed.0.to_string()))?;
                    proven = Some(step);
                    link.send(Ok(wire::proof_goes_on(&says))).await?;
                }
                FromDatabase::SignIn(Asks::Ends(last)) => {
                    let step = proven.take().ok_or(Failed::NotADatabase("it ends a proof that was not given"))?;
                    step.check(&last).map_err(|failed| Failed::NotSignedIn(failed.0.to_string()))?;
                    checked = true;
                }
                FromDatabase::SignIn(Asks::Nothing) => {
                    // a proof that was begun has to have been ended by the database's own
                    if (begun.is_some() || proven.is_some()) && !checked {
                        return Err(Failed::NotADatabase("it takes the account before the proof is at its end"));
                    }
                }
                FromDatabase::SignIn(Asks::Other(_)) => {
                    return Err(Failed::NotSignedIn(
                        "it asks for the password itself or for its old hash, which Away gives to no database: let it take SCRAM-SHA-256"
                            .to_string(),
                    ));
                }
                FromDatabase::Refused(refusal) => return Err(Failed::NotSignedIn(refusal.message)),
                FromDatabase::Ready(_) => return Ok(link),
                FromDatabase::Row(_) | FromDatabase::Done(_) => return Err(Failed::NotADatabase("it answers a statement nobody made")),
                FromDatabase::Passed => {}
            }
        }
    }

    /// Asks one statement, with its values apart from it.
    pub async fn ask(&mut self, statement: &str, values: &[Option<&str>]) -> Result<Answer, Failed> {
        let asked = wire::ask(statement, values);
        self.answered(asked).await
    }

    /// Runs several statements at once, without values: for the database's shape.
    pub async fn run(&mut self, statements: &str) -> Result<Answer, Failed> {
        let asked = wire::run(statements);
        self.answered(asked).await
    }

    /// Tells the database that the server leaves.
    pub async fn leave(mut self) {
        let _ = self.stream.send(&wire::leave()).await;
    }

    async fn answered(&mut self, asked: Result<Vec<u8>, wire::NotThis>) -> Result<Answer, Failed> {
        let within = self.within;
        let exchange = async {
            self.send(asked).await?;
            let mut answer = Answer::default();
            let mut refused = None;
            loop {
                match self.next().await? {
                    FromDatabase::Row(row) => answer
                        .rows
                        .push(row.into_iter().map(|value| value.map(|bytes| String::from_utf8_lossy(&bytes).into_owned())).collect()),
                    FromDatabase::Done(did) => answer.changed += wire::changed(&did),
                    // the first refusal is the reason: what follows it was not run
                    FromDatabase::Refused(refusal) => refused = refused.or(Some(refusal)),
                    FromDatabase::Ready(_) => return refused.map_or(Ok(answer), |refusal| Err(Failed::Refused(refusal))),
                    FromDatabase::SignIn(_) => return Err(Failed::NotADatabase("it signs in again in the middle of a statement")),
                    FromDatabase::Passed => {}
                }
            }
        };
        match tokio::time::timeout(within, exchange).await {
            Ok(answered) => answered,
            Err(_) => Err(Failed::NoAnswerInTime),
        }
    }

    async fn send(&mut self, message: Result<Vec<u8>, wire::NotThis>) -> Result<(), Failed> {
        let message = message.map_err(|wire::NotThis(why)| Failed::NotADatabase(why))?;
        self.stream.send(&message).await.map_err(|error| Failed::Gone(error.to_string()))
    }

    /// The next message of the database.
    async fn next(&mut self) -> Result<FromDatabase, Failed> {
        loop {
            let not_this = |wire::NotThis(why)| Failed::NotADatabase(why);
            if let Some((kind, carried, took)) = wire::frame(&self.arrived).map_err(not_this)? {
                let message = wire::read(kind, carried).map_err(not_this)?;
                self.arrived.drain(..took);
                return Ok(message);
            }
            match self.stream.take(&mut self.arrived).await {
                Ok(0) => return Err(Failed::Gone("the database closed it".to_string())),
                Ok(_) => {}
                Err(error) => return Err(Failed::Gone(error.to_string())),
            }
        }
    }
}
