//! The gateway's connection to a machine, against a stand-in for one: the steps to the
//! desktop, the picture, and keys and the pointer.

use std::time::Duration;

use away_gateway::machine::{reach, Area, Desktop, Step, Why};
use away_gateway::stand_in::{self, Machine};
use away_rdp::input::{Button, Input};
use away_rdp::ntlm::Account;
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
    let reached = reach(&host, port).await.expect("it is reached");
    let secured = reached.secure().await.expect("the connection is secured");
    // what the machine shows of itself is what it is known by
    assert_eq!(secured.identity().fingerprint_text(), machine.fingerprint());
    assert!(!secured.signs_in_itself());
    let mut desktop = secured.sign_in(Account::named(KNOWN, Secret::new(PASSWORD)), 1280, 800).await.expect("the account is signed in");

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
    assert_eq!((seen.asked, seen.name.as_str()), ((1280, 800), "away"));
    assert_eq!(seen.account, ("CORP".to_string(), "alex.urban".to_string()));
    // the password was proven before the desktop, and does not travel with the account's name
    assert_eq!(seen.password, None);
    assert_eq!(seen.input.len(), 5);
    assert_eq!(seen.input[1], Input::Button { button: Button::Left, down: true, x: 50, y: 60 });
}

#[tokio::test]
async fn a_machine_that_does_not_take_the_password_or_is_not_there_says_at_which_step() {
    let machine = Machine::start(KNOWN, PASSWORD, stand_in::desktop()).await;
    let (host, port) = machine.address();
    let secured = reach(&host, port).await.unwrap().secure().await.unwrap();
    let Err(refused) = secured.sign_in(Account::named(KNOWN, Secret::new("another password")), 1280, 800).await else {
        panic!("a wrong password")
    };
    assert_eq!((refused.step, refused.why), (Step::SigningIn, Why::SignInRefused), "{}", refused.details);
    assert!(machine.seen().is_empty(), "nobody came as far as the desktop");

    // a port of this machine that nothing listens on
    let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let Err(nothing) = reach("127.0.0.1", free).await else { panic!("nothing listens there") };
    assert_eq!((nothing.step, nothing.why), (Step::Reaching, Why::Refused), "{}", nothing.details);
    // a name that leads to no address
    let Err(nowhere) = reach("no-such-machine.invalid", 3389).await else { panic!("no such name") };
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
    let Err(not_one) = reach("127.0.0.1", port).await else { panic!("no machine") };
    assert_eq!(not_one.step, Step::Reaching, "{}", not_one.details);
}
