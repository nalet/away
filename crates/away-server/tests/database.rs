//! The server's own way of speaking to a database, against a real PostgreSQL.
//!
//! `AWAY_TEST_DATABASE` names it, as `localhost:15432`: a database `away` with the
//! account `away`, whose password is the one below, and a certificate of the tests'
//! authority for `localhost` (`tests/machines/database/real.sh` starts one). Without the
//! variable these tests say so and pass: there is nothing to test against.

use std::time::Duration;

use away_part::client::Secures;
use away_secret::Secret;
use away_server::store::link::{Failed, Link, Settings};

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
        secured: None,
        within: Duration::from_secs(10),
    })
}

fn fresh() -> [u8; 18] {
    let mut fresh = [0u8; 18];
    getrandom::fill(&mut fresh).expect("random bytes");
    fresh
}

fn texts(row: &[Option<String>]) -> Vec<Option<&str>> {
    row.iter().map(Option::as_deref).collect()
}

#[tokio::test]
async fn the_server_signs_in_with_a_proof_of_its_password_and_asks_with_values_apart() {
    let Some(settings) = database() else { return };
    let mut link = Link::open(&settings, &fresh()).await.expect("signed in");

    // values travel apart from the statement: nothing in one is read as a statement
    let hostile = "'; drop table nothing; --";
    let answer =
        link.ask("select $1::text, $2::int + 1, $3::text, $4::text", &[Some(hostile), Some("41"), None, Some("Zürich ✓")]).await.unwrap();
    assert_eq!(answer.rows.len(), 1);
    assert_eq!(texts(&answer.rows[0]), [Some(hostile), Some("42"), None, Some("Zürich ✓")]);

    // several rows, and how many a statement changed
    link.run("create temporary table thing (name text primary key, size int)").await.unwrap();
    let added = link
        .ask("insert into thing values ($1, $2), ($3, $4), ($5, $6)", &[Some("a"), Some("1"), Some("b"), Some("2"), Some("c"), None])
        .await
        .unwrap();
    assert_eq!((added.changed, added.rows.len()), (3, 0));
    let read = link.ask("select name, size from thing order by name", &[]).await.unwrap();
    assert_eq!(
        read.rows.iter().map(|row| texts(row)).collect::<Vec<_>>(),
        [[Some("a"), Some("1")], [Some("b"), Some("2")], [Some("c"), None]]
    );
    assert_eq!(link.ask("delete from thing where size is not null", &[]).await.unwrap().changed, 2);

    // a statement the database refuses says why, by the database's own code, and the connection goes on
    let Err(Failed::Refused(refusal)) = link.ask("insert into thing values ($1, 1)", &[Some("c")]).await else { panic!("a second c") };
    assert_eq!((refusal.code.as_str(), refusal.severity.as_str()), ("23505", "ERROR"));
    assert!(refusal.message.contains("duplicate key"), "{}", refusal.message);
    let Err(Failed::Refused(refusal)) = link.ask("select nothing from nowhere", &[]).await else { panic!("no such table") };
    assert_eq!(refusal.code, "42P01");
    assert_eq!(link.ask("select count(*) from thing", &[]).await.unwrap().rows, [[Some("1".to_string())]]);

    // several statements at once: the first that fails is the reason, and what came before it in the same go is undone
    let Err(Failed::Refused(refusal)) =
        link.run("insert into thing values ('d', 4); insert into thing values ('c', 3); insert into thing values ('e', 5)").await
    else {
        panic!("a second c")
    };
    assert_eq!(refusal.code, "23505");
    assert_eq!(link.ask("select count(*) from thing", &[]).await.unwrap().rows, [[Some("1".to_string())]]);

    // a long answer comes whole
    let long = link.ask("select repeat('x', 300000), generate_series(1, 40)", &[]).await.unwrap();
    assert_eq!((long.rows.len(), long.rows[39][0].as_ref().map(String::len)), (40, Some(300_000)));
    link.leave().await;
}

#[tokio::test]
async fn a_wrong_password_an_unknown_account_and_a_database_that_is_not_there_say_so() {
    let Some(settings) = database() else { return };
    let wrong = Settings { password: Secret::new("another password"), ..settings.clone() };
    let Err(Failed::NotSignedIn(why)) = Link::open(&wrong, &fresh()).await else { panic!("a wrong password") };
    assert!(why.contains("password authentication failed"), "{why}");
    let nobody = Settings { account: "nobody".to_string(), ..settings.clone() };
    assert!(matches!(Link::open(&nobody, &fresh()).await, Err(Failed::NotSignedIn(_))));
    let nowhere = Settings { database: "nowhere".to_string(), ..settings.clone() };
    let Err(Failed::NotSignedIn(why)) = Link::open(&nowhere, &fresh()).await else { panic!("no such database") };
    assert!(why.contains("does not exist"), "{why}");
    // a port of this machine that nothing listens on
    let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let absent = Settings { host: "127.0.0.1".to_string(), port: free, ..settings };
    assert!(matches!(Link::open(&absent, &fresh()).await, Err(Failed::NotReached(_))));
}

#[tokio::test]
async fn a_connection_is_secured_where_it_is_to_be_and_the_database_proves_who_it_is() {
    let Some(settings) = database() else { return };
    let authority = include_bytes!("../../away-part/tests/fixtures/authority.pem");
    let secured = Settings { secured: Some(Secures::new(Some(authority), b"postgresql").unwrap()), ..settings.clone() };
    let mut link = Link::open(&secured, &fresh()).await.expect("secured and signed in");
    assert_eq!(link.ask("select ssl from pg_stat_ssl where pid = pg_backend_pid()", &[]).await.unwrap().rows, [[Some("t".to_string())]]);
    link.leave().await;

    // a database whose certificate is of no authority the server trusts is not spoken to
    let unknown = Settings { secured: Some(Secures::new(None, b"postgresql").unwrap()), ..settings.clone() };
    assert!(matches!(Link::open(&unknown, &fresh()).await, Err(Failed::NotSecured(_))));
    // nor one that is reached by another name than its certificate's
    let other_name = Settings { host: "127.0.0.1".to_string(), ..secured };
    assert!(matches!(Link::open(&other_name, &fresh()).await, Err(Failed::NotSecured(_))));
}

#[tokio::test]
async fn what_answers_and_is_no_database_or_secures_nothing_is_left_alone() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    // needs no database: three stand-ins on this machine, each with one answer
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
    let settings = |port: u16, secured: bool| Settings {
        host: "localhost".to_string(),
        port,
        database: "away".to_string(),
        account: "away".to_string(),
        password: Secret::new(PASSWORD),
        secured: secured.then(|| Secures::new(None, b"postgresql").unwrap()),
        within: Duration::from_secs(2),
    };
    let open = |port: u16, secured: bool| async move { Link::open(&settings(port, secured), &[7; 18]).await.err() };

    assert_eq!(open(answers(b"N").await, true).await, Some(Failed::NotSecured("the database secures no connections".to_string())));
    assert!(matches!(open(answers(b"H").await, true).await, Some(Failed::NotADatabase(_))));
    assert!(matches!(open(answers(b"HTTP/1.1 400 Bad Request\r\n\r\n").await, false).await, Some(Failed::NotADatabase(_))));
    // a database that wants the password itself is given none
    let Some(Failed::NotSignedIn(why)) = open(answers(b"R\0\0\0\x08\0\0\0\x03").await, false).await else { panic!("the password itself") };
    assert!(why.starts_with("it asks for the password itself"), "{why}");
    // one that says nothing at all is not waited for longer than it may take
    assert_eq!(open(answers(b"").await, false).await, Some(Failed::NoAnswerInTime));
}

mod the_store {
    use super::*;
    use away_part::Lines;
    use away_server::store::favourites::Favourite;
    use away_server::store::{into_shape, Store, Unusable, SHAPE};

    /// A database of its own for one test, so that no test sees what another kept.
    async fn a_database_of_its_own(settings: &Settings) -> Settings {
        let mut name = [0u8; 6];
        getrandom::fill(&mut name).expect("random bytes");
        let name = format!("test_{}", name.iter().map(|byte| format!("{byte:02x}")).collect::<String>());
        let mut link = Link::open(settings, &fresh()).await.expect("signed in");
        link.run(&format!("create database {name}")).await.expect("a database of its own");
        link.leave().await;
        Settings { database: name, ..settings.clone() }
    }

    fn lines_of(lines: &Lines, event: &str) -> usize {
        lines.kept().iter().filter(|line| line.contains(event)).count()
    }

    #[tokio::test]
    async fn the_database_is_brought_into_shape_step_by_step_and_each_step_once() {
        let Some(settings) = database() else { return };
        let settings = a_database_of_its_own(&settings).await;
        let lines = Lines::kept_for("server");
        let store = Store::new(settings.clone(), lines.clone());
        store.begin().await.expect("in shape");
        assert_eq!(lines_of(&lines, "the database was brought into shape"), 1);
        let steps = store.ask("select step, what from away_shape order by step", &[]).await.unwrap();
        assert_eq!(steps.rows.len(), SHAPE.len());
        assert_eq!(steps.rows[0], [Some("1".to_string()), Some(SHAPE[0].0.to_string())]);

        // another server that starts on the same database finds it in shape, and changes nothing
        let again = Lines::kept_for("server");
        Store::new(settings.clone(), again.clone()).begin().await.expect("in shape");
        assert_eq!(lines_of(&again, "the database was brought into shape"), 0);

        // a later version knows a step more, and takes that one alone
        let mut later: Vec<(&str, &str)> = SHAPE.to_vec();
        later.push(("something a later version keeps", "create table later (name text primary key)"));
        let mut link = Link::open(&settings, &fresh()).await.unwrap();
        assert_eq!(into_shape(&mut link, &later).await, Ok(1));
        assert_eq!(into_shape(&mut link, &later).await, Ok(0));
        link.leave().await;

        // a step that fails is not taken in part, and not noted
        let mut broken = later.clone();
        broken.push(("a step with a mistake in it", "create table half (name text); create table later (name text)"));
        let mut link = Link::open(&settings, &fresh()).await.unwrap();
        let Err(Unusable::Database(Failed::Refused(refusal))) = into_shape(&mut link, &broken).await else {
            panic!("a table that is there")
        };
        assert_eq!(refusal.code, "42P07");
        // the connection is given up, as the store gives it up: what was begun on it is undone
        link.leave().await;
        let mut link = Link::open(&settings, &fresh()).await.unwrap();
        assert_eq!(link.ask("select count(*) from away_shape", &[]).await.unwrap().rows, [[Some(later.len().to_string())]]);
        assert_eq!(link.ask("select to_regclass('half') is null", &[]).await.unwrap().rows, [[Some("t".to_string())]]);
        link.leave().await;

        // this version, on a database that a later one has shaped, says so and touches nothing
        let lines = Lines::kept_for("server");
        let earlier = Store::new(settings, lines);
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
        let store = Store::new(settings.clone(), Lines::kept_for("server"));
        let machine = |name: &str| Favourite { machine: name.to_string(), program: None };
        let program = |machine: &str, name: &str| Favourite { machine: machine.to_string(), program: Some(name.to_string()) };

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

        // another server on the same database sees the same
        let other = Store::new(settings, Lines::kept_for("server"));
        assert_eq!(other.favourites("alex").await.unwrap(), [program("fin-app-02", "Ledger"), machine("build-01")]);
    }

    #[tokio::test]
    async fn a_connection_the_database_closed_is_opened_anew_and_a_database_that_is_gone_says_so() {
        let Some(settings) = database() else { return };
        let settings = a_database_of_its_own(&settings).await;
        let store = Store::new(settings.clone(), Lines::kept_for("server"));
        let own = store.ask("select pg_backend_pid()", &[]).await.unwrap().rows[0][0].clone().unwrap();
        // the database ends the store's connection while nothing is asked
        let mut link = Link::open(&settings, &fresh()).await.unwrap();
        assert_eq!(link.ask("select pg_terminate_backend($1::int)", &[Some(&own)]).await.unwrap().rows, [[Some("t".to_string())]]);
        let anew = store.ask("select pg_backend_pid()", &[]).await.unwrap().rows[0][0].clone().unwrap();
        assert_ne!(anew, own);

        // a store at a database that isn't there says so each time, and holds nothing half open
        let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let absent = Store::new(Settings { host: "127.0.0.1".to_string(), port: free, ..settings }, Lines::kept_for("server"));
        for _ in 0..2 {
            assert!(matches!(absent.ask("select 1", &[]).await, Err(Unusable::Database(Failed::NotReached(_)))));
        }
    }
}
