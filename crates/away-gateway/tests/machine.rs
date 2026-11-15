//! The gateway's connection to a machine, against a stand-in for one: the steps to the
//! desktop, the picture, and keys and the pointer. And against a stand-in for a farm's
//! broker, which sends the connection on to the machine that serves it.

use std::time::Duration;

use away_gateway::machine::{reach, Area, Desktop, Opened, Screen, Step, Who, Why};
use away_gateway::stand_in::{self, Machine};
use away_rdp::input::{Button, Input};
use away_rdp::ntlm::Account;
use away_rdp::redirection::Redirection;
use away_rdp::stand_in::desktop::{typed, CLICKED, MARK};
use away_secret::Secret;

const KNOWN: &str = "CORP\\alex.urban";
const PASSWORD: &str = "a password that exists for these tests alone";

/// Takes what the machine sends until something is the case, for a few seconds at most.
async fn until(desktop: &mut Desktop, what: &str, mut is: impl FnMut(&Desktop) -> bool) -> Vec<Area> {
    // what arrived with the machine's last word on the way to the desktop is there already
    let mut areas = desktop.take().await.expect("what the machine sent is read");
    let taking = async {
        while !is(desktop) {
            desktop.arrives().await.expect("the machine stays");
            areas.extend(desktop.take().await.expect("what the machine sends is read"));
        }
    };
    tokio::time::timeout(Duration::from_secs(10), taking).await.unwrap_or_else(|_| panic!("it never happened: {what}"));
    areas
}

#[tokio::test]
async fn a_desktop_opens_shows_the_machines_picture_and_takes_keys_and_the_pointer() {
    let machine = Machine::start(KNOWN, PASSWORD, stand_in::desktop()).await;
    let (host, port) = machine.address();
    let reached = reach(&host, port, None).await.expect("it is reached");
    let secured = reached.secure().await.expect("the connection is secured");
    // what the machine shows of itself is what it is known by
    assert_eq!(secured.identity().fingerprint_text(), machine.fingerprint());
    assert!(!secured.signs_in_itself());
    let Ok(Opened::Desktop(mut desktop)) =
        secured.sign_in(Who::Account(Account::named(KNOWN, Secret::new(PASSWORD))), Screen::of(1280, 800), 0x0807, None, None).await
    else {
        panic!("the account is signed in, and the machine shows its desktop")
    };

    // the picture is the machine's, pixel for pixel, and every area of it was drawn
    let shows = stand_in::desktop();
    let areas = until(&mut desktop, "the first picture", |desktop| desktop.picture() == &shows).await;
    assert_eq!((desktop.picture().width, desktop.picture().height), (200, 120));
    let mut covered = vec![false; 200 * 120];
    for area in &areas {
        for row in area.top..area.top + area.height {
            covered[row * 200 + area.left..row * 200 + area.left + area.width].fill(true);
        }
    }
    assert!(covered.iter().all(|drawn| *drawn), "every pixel was drawn");

    // a click, and a key: the machine draws a mark for each, and the picture has them
    desktop
        .tell(&[
            Input::Moved { x: 50, y: 60 },
            Input::Button { button: Button::Left, down: true, x: 50, y: 60 },
            Input::Button { button: Button::Left, down: false, x: 50, y: 60 },
            Input::Key { code: 0x1e, extended: false, down: true },
            Input::Key { code: 0x1e, extended: false, down: false },
        ])
        .await
        .expect("the keys and the pointer are told");
    let pixel =
        |desktop: &Desktop, x: usize, y: usize| -> [u8; 3] { desktop.picture().pixels[(y * 200 + x) * 4..][..3].try_into().unwrap() };
    let marks = until(&mut desktop, "the marks", |desktop| pixel(desktop, 55, 65) == CLICKED && pixel(desktop, 5, 5) == typed(0x1e)).await;
    assert_eq!(
        marks,
        [Area { left: 50, top: 60, width: MARK, height: MARK }, Area { left: 0, top: 0, width: MARK, height: MARK }],
        "only what changed is drawn anew"
    );
    assert_eq!(Some(desktop.picture()), machine.picture().as_ref());

    desktop.leave().await;
    let seen = loop {
        let seen = machine.seen();
        if seen.last().is_some_and(|seen| seen.left) {
            break seen;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    let seen = &seen[0];
    assert_eq!((seen.asked, seen.scale, seen.name.as_str()), ((1280, 800), 100, "away"));
    assert_eq!(seen.account, ("CORP".to_string(), "alex.urban".to_string()));
    // the password was proven before the desktop, and does not travel with the account's name
    assert_eq!(seen.password, None);
    assert_eq!(seen.input.len(), 5);
    assert_eq!(seen.input[1], Input::Button { button: Button::Left, down: true, x: 50, y: 60 });
    // a machine that is reached at its own address is told nothing of a farm
    assert_eq!((seen.sent_on_to, machine.said_first()), (None, vec![None]));
}

#[tokio::test]
async fn a_farms_broker_says_where_the_connection_belongs_and_the_machine_it_names_shows_the_desktop() {
    let machine = Machine::start(KNOWN, PASSWORD, stand_in::desktop()).await;
    let (host, port) = machine.address();
    let sends_on = Redirection {
        session: 7,
        address: Some("127.0.0.1".to_string()),
        name: Some(host.clone()),
        token: Some(b"Cookie: msts=285216778.15629.0000\r\n".to_vec()),
        ..Redirection::default()
    };
    let broker = Machine::start_broker(KNOWN, PASSWORD, sends_on.clone()).await;
    let account = || Account::named(KNOWN, Secret::new(PASSWORD));

    // the broker is told which collection is wanted, signs the account in as a machine
    // does, and then shows nothing: it says where the connection belongs
    let (at, on) = broker.address();
    let wanted = b"tsv://MS Terminal Services Plugin.1.Desktops";
    let secured = reach(&at, on, Some(wanted)).await.expect("the broker is reached").secure().await.expect("the connection is secured");
    let Ok(Opened::SentOn(to)) = secured.sign_in(Who::Account(account()), Screen::of(1280, 800), 0x0409, None, None).await else {
        panic!("the broker sends the connection on")
    };
    assert_eq!(to, sends_on);
    assert_eq!(broker.said_first(), [Some(wanted.to_vec())]);
    // a wrong password ends at the broker, as it does at a machine
    let secured = reach(&at, on, Some(wanted)).await.unwrap().secure().await.unwrap();
    let Err(refused) = secured
        .sign_in(Who::Account(Account::named(KNOWN, Secret::new("another password"))), Screen::of(1280, 800), 0x0409, None, None)
        .await
    else {
        panic!("a wrong password")
    };
    assert_eq!((refused.step, refused.why), (Step::SigningIn, Why::SignInRefused), "{}", refused.details);

    // the machine it names is told what the broker gave for it, and which session is asked for
    let secured = reach(&host, port, to.token.as_deref()).await.expect("the machine is reached").secure().await.expect("it is secured");
    let Ok(Opened::Desktop(mut desktop)) =
        secured.sign_in(Who::Account(account()), Screen::of(1280, 800), 0x0409, Some(to.session), None).await
    else {
        panic!("the machine shows its desktop")
    };
    let shows = stand_in::desktop();
    until(&mut desktop, "the first picture", |desktop| desktop.picture() == &shows).await;
    desktop.leave().await;
    assert_eq!(machine.said_first(), [Some(b"Cookie: msts=285216778.15629.0000".to_vec())]);
    assert_eq!(machine.seen()[0].sent_on_to, Some(7));

    // what is said first is one line, and not a long one
    for wrong in [&b"two\r\nlines"[..], &[b'a'; 201]] {
        let Err(refused) = reach(&host, port, Some(wrong)).await else { panic!("it is not said") };
        assert_eq!((refused.step, refused.why), (Step::Reaching, Why::Broken), "{}", refused.details);
    }
}

#[tokio::test]
async fn a_machine_that_does_not_take_the_password_or_is_not_there_says_at_which_step() {
    let machine = Machine::start(KNOWN, PASSWORD, stand_in::desktop()).await;
    let (host, port) = machine.address();
    let secured = reach(&host, port, None).await.unwrap().secure().await.unwrap();
    let Err(refused) = secured
        .sign_in(Who::Account(Account::named(KNOWN, Secret::new("another password"))), Screen::of(1280, 800), 0x0409, None, None)
        .await
    else {
        panic!("a wrong password")
    };
    assert_eq!((refused.step, refused.why), (Step::SigningIn, Why::SignInRefused), "{}", refused.details);
    assert!(machine.seen().is_empty(), "nobody came as far as the desktop");

    // a port of this machine that nothing listens on
    let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let Err(nothing) = reach("127.0.0.1", free, None).await else { panic!("nothing listens there") };
    assert_eq!((nothing.step, nothing.why), (Step::Reaching, Why::Refused), "{}", nothing.details);
    // a number in brackets is the number
    assert!(reach("[127.0.0.1]", port, None).await.is_ok());
    // a name that leads to no address
    let Err(nowhere) = reach("no-such-machine.invalid", 3389, None).await else { panic!("no such name") };
    assert_eq!((nowhere.step, nowhere.why), (Step::Reaching, Why::NoAddress), "{}", nowhere.details);
    // something that answers and is no machine
    let other = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = other.local_addr().unwrap().port();
    tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        while let Ok((mut connection, _)) = other.accept().await {
            let _ = connection.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
        }
    });
    let Err(not_one) = reach("127.0.0.1", port, None).await else { panic!("no machine") };
    assert_eq!(not_one.step, Step::Reaching, "{}", not_one.details);
}
