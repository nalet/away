//! What Away keeps, against a real PostgreSQL: how the server reaches it, how it secures
//! the connection, the database's shape, and what people star.
//!
//! `AWAY_TEST_DATABASE` names it, as `localhost:15432`: a database `away` with the
//! account `away`, whose password is the one below, and a certificate of the tests'
//! authority for `localhost` (`tests/machines/database/real.sh` starts one). Without the
//! variable these tests say so and pass: there is nothing to test against.

use std::time::Duration;

use away_part::client::{encrypting_unchecked, securing};
use away_part::json::Json;
use away_part::Lines;
use away_secret::Secret;
use away_server::store::favourites::Favourite;
use away_server::store::records::Record;
use away_server::store::{Failed, Secured, Settings, Store, Unusable, SHAPE};

const PASSWORD: &str = "a password for this machine alone";

/// Where the database of the tests is, if there is one.
fn database() -> Option<Settings> {
    let Some(at) = std::env::var("AWAY_TEST_DATABASE").ok().filter(|at| !at.is_empty()) else {
        // the pipeline always has one: there, a test that finds none has not tested anything
        assert!(std::env::var_os("CI").is_none(), "the pipeline's tests need a database: AWAY_TEST_DATABASE is not set");
        eprintln!("no database to test against: AWAY_TEST_DATABASE is not set");
        return None;
    };
    let (host, port) = at.rsplit_once(':').expect("AWAY_TEST_DATABASE is host:port");
    Some(Settings {
        host: host.to_string(),
        port: port.parse().expect("a port"),
        database: "away".to_string(),
        account: "away".to_string(),
        password: Secret::new(PASSWORD),
        secured: Secured::Plain,
        within: Duration::from_secs(10),
    })
}

/// A connection of the tests' own, beside the store's: to make a database, and to look
/// at what the store did.
async fn beside(settings: &Settings) -> tokio_postgres::Client {
    let mut at = tokio_postgres::Config::new();
    at.host(&settings.host).port(settings.port).dbname(&settings.database).user(&settings.account).password(PASSWORD);
    let (client, connection) = at.connect(tokio_postgres::NoTls).await.expect("the tests' database");
    tokio::spawn(connection);
    client
}

/// A database of its own for one test, so that no test sees what another kept.
async fn a_database_of_its_own(settings: &Settings) -> Settings {
    let mut name = [0u8; 6];
    getrandom::fill(&mut name).expect("random bytes");
    let name = format!("test_{}", name.iter().map(|byte| format!("{byte:02x}")).collect::<String>());
    beside(settings).await.batch_execute(&format!("create database {name}")).await.expect("a database of its own");
    Settings { database: name, ..settings.clone() }
}

fn lines_of(lines: &Lines, event: &str) -> usize {
    lines.kept().iter().filter(|line| line.contains(event)).count()
}

fn store(settings: &Settings) -> Store {
    Store::new(settings.clone(), Lines::kept_for("server"))
}

fn machine(name: &str) -> Favourite {
    Favourite { machine: name.to_string(), program: None }
}

fn program(machine: &str, name: &str) -> Favourite {
    Favourite { machine: machine.to_string(), program: Some(name.to_string()) }
}

#[tokio::test]
async fn the_database_is_brought_into_shape_step_by_step_and_each_step_once() {
    let Some(settings) = database() else { return };
    let settings = a_database_of_its_own(&settings).await;
    let lines = Lines::kept_for("server");
    Store::new(settings.clone(), lines.clone()).begin().await.expect("in shape");
    assert_eq!(lines_of(&lines, "the database was brought into shape"), 1);
    let beside = beside(&settings).await;
    let steps = beside.query("select step, what from away_shape order by step", &[]).await.unwrap();
    assert_eq!(steps.len(), SHAPE.len());
    assert_eq!((steps[0].get::<_, i32>(0), steps[0].get::<_, String>(1)), (1, SHAPE[0].0.to_string()));

    // another server that starts on the same database finds it in shape, and changes nothing
    let again = Lines::kept_for("server");
    Store::new(settings.clone(), again.clone()).begin().await.expect("in shape");
    assert_eq!(lines_of(&again, "the database was brought into shape"), 0);

    // a later version knows a step more, and takes that one alone
    let mut later: Vec<(&str, &str)> = SHAPE.to_vec();
    later.push(("something a later version keeps", "create table later (name text primary key)"));
    let lines = Lines::kept_for("server");
    Store::with_shape(settings.clone(), lines.clone(), &later).begin().await.expect("a step more");
    assert!(lines.kept().iter().any(|line| line.contains("the database was brought into shape") && line.contains("\"steps\":1")));
    Store::with_shape(settings.clone(), lines.clone(), &later).begin().await.expect("in shape");
    assert_eq!(lines_of(&lines, "the database was brought into shape"), 1);

    // a step that fails is not taken in part, and not noted
    let mut broken = later.clone();
    broken.push(("a step with a mistake in it", "create table half (name text); create table later (name text)"));
    let Err(Unusable::Database(Failed::Refused { code, .. })) =
        Store::with_shape(settings.clone(), Lines::kept_for("server"), &broken).begin().await
    else {
        panic!("a table that is there")
    };
    assert_eq!(code, "42P07");
    let taken: i64 = beside.query_one("select count(*) from away_shape", &[]).await.unwrap().get(0);
    assert_eq!(taken as usize, later.len());
    let half: bool = beside.query_one("select to_regclass('half') is null", &[]).await.unwrap().get(0);
    assert!(half, "the half of a step is not there");

    // this version, on a database that a later one has shaped, says so and touches nothing
    let earlier = store(&settings);
    assert_eq!(earlier.begin().await, Err(Unusable::LaterShape { taken: later.len(), known: SHAPE.len() }));
    assert_eq!(
        earlier.begin().await.unwrap_err().to_string(),
        format!(
            "the database is in the shape of a later version of Away: it has taken {} steps, and this version knows {}",
            later.len(),
            SHAPE.len()
        )
    );
}

#[tokio::test]
async fn servers_that_start_at_once_on_a_new_database_shape_it_once() {
    let Some(settings) = database() else { return };
    let settings = a_database_of_its_own(&settings).await;
    let lines = Lines::kept_for("server");
    let stores: Vec<Store> = (0..4).map(|_| Store::new(settings.clone(), lines.clone())).collect();
    let (a, b, c, d) = tokio::join!(stores[0].begin(), stores[1].begin(), stores[2].begin(), stores[3].begin());
    assert_eq!((a, b, c, d), (Ok(()), Ok(()), Ok(()), Ok(())));
    assert_eq!(lines_of(&lines, "the database was brought into shape"), 1);
}

#[tokio::test]
async fn what_a_person_starred_is_kept_with_the_person_in_the_order_they_starred_it() {
    let Some(settings) = database() else { return };
    let settings = a_database_of_its_own(&settings).await;
    let store = store(&settings);

    assert_eq!(store.favourites("alex").await, Ok(Vec::new()));
    store.star("alex", &machine("fin-app-02"), true).await.unwrap();
    store.star("alex", &program("fin-app-02", "Ledger"), true).await.unwrap();
    store.star("alex", &machine("build-01"), true).await.unwrap();
    // starring what is starred changes nothing, its place neither
    store.star("alex", &machine("fin-app-02"), true).await.unwrap();
    store.star("sam", &machine("build-01"), true).await.unwrap();
    assert_eq!(store.favourites("alex").await.unwrap(), [machine("fin-app-02"), program("fin-app-02", "Ledger"), machine("build-01")]);
    assert_eq!(store.favourites("sam").await.unwrap(), [machine("build-01")]);

    // a machine and its program are two things: taking the star from one leaves the other
    store.star("alex", &machine("fin-app-02"), false).await.unwrap();
    store.star("alex", &machine("never starred"), false).await.unwrap();
    assert_eq!(store.favourites("alex").await.unwrap(), [program("fin-app-02", "Ledger"), machine("build-01")]);
    assert_eq!(store.favourites("sam").await.unwrap(), [machine("build-01")]);

    // a name travels apart from the statement: nothing in one is read as a statement
    let hostile = "'; drop table favourite; --";
    store.star("'; delete from favourite; --", &program(hostile, "Zürich ✓ \"x\""), true).await.unwrap();
    assert_eq!(store.favourites("'; delete from favourite; --").await.unwrap(), [program(hostile, "Zürich ✓ \"x\"")]);
    assert_eq!(store.favourites("alex").await.unwrap().len(), 2);

    // another server on the same database sees the same
    let other = self::store(&settings);
    assert_eq!(other.favourites("alex").await.unwrap(), [program("fin-app-02", "Ledger"), machine("build-01")]);
}

#[tokio::test]
async fn records_are_added_and_the_database_itself_keeps_them_from_being_changed() {
    let Some(settings) = database() else { return };
    let settings = a_database_of_its_own(&settings).await;
    let store = store(&settings);
    let record = |key: &str, kind: &str, machine: Option<&str>| Record {
        key: key.to_string(),
        kind: kind.to_string(),
        person: Some(("7c1f".to_string(), "Alex Urban".to_string())),
        machine: machine.map(str::to_string),
        details: Json::object([("way", "desktop".into()), ("from", "'; drop table record; --".into())]),
    };
    store.record(&record("one", "session-asked", Some("fin-app-02"))).await.unwrap();
    store.record(&Record { person: None, ..record("two", "files-applied", None) }).await.unwrap();
    // a record that is added twice, as after a connection that broke in the middle, is one record
    store.record(&record("one", "session-asked", Some("fin-app-02"))).await.unwrap();

    let beside = beside(&settings).await;
    let kept = beside
        .query("select number, key, kind, person, person_name, machine, details, at <= now() from record order by number", &[])
        .await
        .unwrap();
    assert_eq!(kept.len(), 2);
    assert_eq!(
        (kept[0].get::<_, i64>(0), kept[0].get::<_, String>(1), kept[0].get::<_, String>(2), kept[0].get::<_, Option<String>>(3)),
        (1, "one".to_string(), "session-asked".to_string(), Some("7c1f".to_string()))
    );
    assert_eq!(
        (kept[0].get::<_, Option<String>>(4), kept[0].get::<_, Option<String>>(5)),
        (Some("Alex Urban".to_string()), Some("fin-app-02".to_string()))
    );
    assert_eq!(kept[0].get::<_, String>(6), r#"{"way":"desktop","from":"'; drop table record; --"}"#);
    assert!(kept[0].get::<_, bool>(7));
    assert_eq!((kept[1].get::<_, Option<String>>(3), kept[1].get::<_, Option<String>>(5)), (None, None));

    // the last of a kind, about a machine or about none
    assert_eq!(
        store.last_record(&["session-asked"], Some("fin-app-02")).await.unwrap().map(|(kind, _)| kind).as_deref(),
        Some("session-asked")
    );
    assert_eq!(store.last_record(&["session-asked"], None).await.unwrap(), None);
    assert_eq!(
        store.last_record(&["files-applied", "files-refused"], None).await.unwrap().map(|(kind, _)| kind).as_deref(),
        Some("files-applied")
    );

    // nobody changes a record, removes one or empties the records: not even the account that adds them
    for forbidden in ["update record set kind = 'other'", "delete from record where key = 'one'", "truncate record"] {
        let refused = beside.batch_execute(forbidden).await.expect_err(forbidden);
        let refused = refused.as_db_error().expect("the database's own refusal");
        assert_eq!((refused.code().code(), refused.message()), ("23001", "a record is never changed and never removed"), "{forbidden}");
    }
    assert_eq!(beside.query_one("select count(*) from record", &[]).await.unwrap().get::<_, i64>(0), 2);
}

#[tokio::test]
async fn a_connection_the_database_closed_is_opened_anew() {
    let Some(settings) = database() else { return };
    let settings = a_database_of_its_own(&settings).await;
    let store = store(&settings);
    store.star("alex", &machine("build-01"), true).await.unwrap();
    // the database ends the store's connection while nothing is asked
    let beside = beside(&settings).await;
    let ended = beside
        .execute(
            "select pg_terminate_backend(pid) from pg_stat_activity where application_name = 'away' and datname = current_database()",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(ended, 1, "the store holds one connection");
    assert_eq!(store.favourites("alex").await.unwrap(), [machine("build-01")]);
    store.star("alex", &machine("build-02"), true).await.unwrap();
    assert_eq!(store.favourites("alex").await.unwrap().len(), 2);
}

#[tokio::test]
async fn a_wrong_password_an_unknown_account_and_a_database_that_is_not_there_say_so() {
    let Some(settings) = database() else { return };
    let wrong = Settings { password: Secret::new("another password"), ..settings.clone() };
    let Err(Unusable::Database(Failed::NotSignedIn(why))) = store(&wrong).begin().await else { panic!("a wrong password") };
    assert!(why.contains("password authentication failed"), "{why}");
    let nobody = Settings { account: "nobody".to_string(), ..settings.clone() };
    assert!(matches!(store(&nobody).begin().await, Err(Unusable::Database(Failed::NotSignedIn(_)))));
    let nowhere = Settings { database: "nowhere".to_string(), ..settings.clone() };
    let Err(Unusable::Database(Failed::NotSignedIn(why))) = store(&nowhere).begin().await else { panic!("no such database") };
    assert!(why.contains("does not exist"), "{why}");

    // a port of this machine that nothing listens on: it says so each time, and holds nothing half open.
    // Not a port the system hands out, which would be another test's a moment later
    let absent = store(&Settings { host: "127.0.0.1".to_string(), port: 1, ..settings });
    for _ in 0..2 {
        let Err(Unusable::Database(Failed::NotReached(why))) = absent.favourites("alex").await else { panic!("nothing listens there") };
        assert!(why.to_lowercase().contains("refused"), "{why}");
    }
}

#[tokio::test]
async fn a_connection_is_secured_where_it_is_to_be_and_the_database_proves_who_it_is() {
    let Some(settings) = database() else { return };
    // how many of the connections to a database of the tests are encrypted, and Away's
    let encrypted = "select count(*) from pg_stat_ssl join pg_stat_activity using (pid) \
                     where ssl and application_name = 'away' and datname = current_database()";
    let authority = include_bytes!("../../away-part/tests/fixtures/authority.pem");
    let verified = Secured::Verified(securing(Some(authority), b"postgresql").unwrap());
    let secured = Settings { secured: verified, ..a_database_of_its_own(&settings).await };
    let held = store(&secured);
    held.begin().await.expect("secured and signed in");
    assert_eq!(beside(&secured).await.query_one(encrypted, &[]).await.unwrap().get::<_, i64>(0), 1);

    // a database whose certificate is of no authority the server trusts is not spoken to
    let unknown = Settings { secured: Secured::Verified(securing(None, b"postgresql").unwrap()), ..settings.clone() };
    let Err(Unusable::Database(Failed::NotSecured(why))) = store(&unknown).begin().await else { panic!("an unknown authority") };
    assert!(why.contains("UnknownIssuer"), "{why}");
    // nor one that is reached by another name than its certificate's
    let other_name = Settings { host: "127.0.0.1".to_string(), ..secured };
    assert!(matches!(store(&other_name).begin().await, Err(Unusable::Database(Failed::NotSecured(_)))));

    // where the installation says that the certificate is not checked, the connection is encrypted all the same
    let unchecked = Settings {
        host: "127.0.0.1".to_string(),
        secured: Secured::Encrypted(encrypting_unchecked(b"postgresql").unwrap()),
        ..a_database_of_its_own(&settings).await
    };
    let held = store(&unchecked);
    held.begin().await.expect("encrypted, and the certificate not checked");
    assert_eq!(beside(&unchecked).await.query_one(encrypted, &[]).await.unwrap().get::<_, i64>(0), 1);
}

#[tokio::test]
async fn a_database_that_keeps_passwords_the_old_way_is_signed_in_at_over_an_encrypted_connection() {
    let Some(settings) = database() else { return };
    let settings = a_database_of_its_own(&settings).await;
    // an account whose password the database keeps as its old hash, as older installations do
    let account = settings.database.replace("test_", "old_");
    let beside = beside(&settings).await;
    beside
        .batch_execute(&format!(
            "set password_encryption = 'md5'; create role {account} login password '{PASSWORD}'; \
             grant all on database {} to {account}; grant all on schema public to {account}",
            settings.database
        ))
        .await
        .unwrap();
    let kept: String = beside.query_one("select left(rolpassword, 3) from pg_authid where rolname = $1", &[&account]).await.unwrap().get(0);
    assert_eq!(kept, "md5");
    let old = Settings { account, secured: Secured::Encrypted(encrypting_unchecked(b"postgresql").unwrap()), ..settings.clone() };
    let store = store(&old);
    store.star("alex", &machine("build-01"), true).await.expect("signed in the old way");
    assert_eq!(store.favourites("alex").await.unwrap(), [machine("build-01")]);
}

#[tokio::test]
async fn what_answers_and_is_no_database_or_secures_nothing_is_left_alone() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    // needs no database: stand-ins on this machine, each with one answer
    async fn answers(with: &'static [u8]) -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut connection, _)) = listener.accept().await {
                let mut asked = [0u8; 512];
                let _ = connection.read(&mut asked).await;
                let _ = connection.write_all(with).await;
                let _ = connection.read(&mut asked).await;
            }
        });
        port
    }
    let at = |port: u16, secured: Secured| Settings {
        host: "localhost".to_string(),
        port,
        database: "away".to_string(),
        account: "away".to_string(),
        password: Secret::new(PASSWORD),
        secured,
        within: Duration::from_secs(2),
    };
    let verified = || Secured::Verified(securing(None, b"postgresql").unwrap());

    // a database that secures no connections is not spoken to where the connection is to be secured
    assert!(matches!(store(&at(answers(b"N").await, verified())).begin().await, Err(Unusable::Database(Failed::NotSecured(_)))));
    // what answers with something else than a database does is not one
    assert!(store(&at(answers(b"HTTP/1.1 400 Bad Request\r\n\r\n").await, Secured::Plain)).begin().await.is_err());
    // one that says nothing at all is not waited for longer than it may take
    assert_eq!(store(&at(answers(b"").await, Secured::Plain)).begin().await, Err(Unusable::Database(Failed::NoAnswerInTime)));
}
