//! One session: a browser's connection on one side, a machine's desktop on the other,
//! and the gateway's picture of the desktop between them
//! ([ADR-0004](../../../spec/architecture/decisions/0004-the-session-and-its-picture.md)).
//!
//! The browser is told the steps of the connection, asked for a name and a password
//! where the machine signs people in by what they type, and then sent what the machine
//! draws: as rectangles it reads by itself, and only as fast as it confirms them. Keys
//! and the pointer go the other way.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use away_part::address::Address;
use away_part::client::Client;
use away_part::json::Json;
use away_part::lines::Lines;
use away_rdp::input::{Button, Input};
use away_rdp::ntlm::Account;
use away_rdp::picture::Picture;
use away_rdp::rail::Program;
use away_secret::Secret;
use away_wire::{Carried, Did, FromBrowser, ToBrowser};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

use crate::machine::{self, Area, Desktop, Failure, Opened, Step, Who, Why};
use crate::themselves::Domain;

/// How long a browser gets to say which session it is, once its connection is open.
const BEGINS_WITHIN: Duration = Duration::from_secs(10);
/// How long a person gets to type a name and a password.
const TYPED_WITHIN: Duration = Duration::from_secs(300);
/// How often a name and a password are asked for again before the session gives up.
const ASKS_AT_MOST: usize = 5;
/// How many batches of rectangles may be on their way to the browser at once: one being
/// drawn, and one behind it.
const ON_THEIR_WAY: u32 = 2;
/// The side of the squares the picture is divided into, to say what changed.
const TILE: usize = 64;
/// How long a program may be without a window of its own before its session is at its
/// end: a window that opens in the place of one that closed comes within this.
const CLOSED_FOR: Duration = Duration::from_secs(3);

/// The server, as the gateway asks it.
pub struct Server {
    client: Client,
    at: Address,
}

/// A session the gateway took over: what names it, whose it is, and where the machine is.
pub struct Taken {
    /// What names the session.
    pub session: String,
    /// The machine, by its name.
    pub machine: String,
    /// Where the machine is reached.
    pub address: String,
    /// On which port.
    pub port: u16,
    /// The collection of a farm that this is, where the address is a farm's broker.
    pub collection: Option<String>,
    /// The machines that serve the collection, by their addresses, as the files name
    /// them: each may name its port after a colon.
    pub served_by: Vec<String>,
    /// The program the session is for, where it is for one and not for the desktop.
    pub program: Option<Program>,
    /// Whether the person is signed in on the machine as themselves, with nothing typed.
    pub themselves: bool,
}

impl Server {
    /// The server at this address.
    pub fn new(client: Client, at: Address) -> Server {
        Server { client, at }
    }

    async fn ask(&self, path: &str, says: &[(&str, &str)]) -> Result<(u16, Json), String> {
        let at = Address { path: path.to_string(), ..self.at.clone() };
        let got = self.client.send_form(&at, says, None).await.map_err(|failed| format!("the server: {failed}"))?;
        let said = std::str::from_utf8(&got.body).ok().and_then(|text| Json::read(text).ok()).unwrap_or(Json::Null);
        Ok((got.status, said))
    }

    /// Takes a session over with its ticket. Nothing where the ticket is none, is
    /// spent, or belongs to another browser's sign-in.
    pub async fn take(&self, ticket: &Secret, sign_in: &str) -> Result<Option<Taken>, String> {
        let (status, said) = self.ask("/internal/take", &[("ticket", ticket.expose()), ("sign_in", sign_in)]).await?;
        if status == 404 {
            return Ok(None);
        }
        let machine = said.get("machine");
        let text = |held: Option<&Json>, name: &str| held.and_then(|held| held.text_of(name)).map(str::to_string);
        let port = machine.and_then(|machine| machine.get("port")).and_then(Json::count).and_then(|port| u16::try_from(port).ok());
        let collection = text(machine, "collection");
        let served_by: Vec<String> = machine
            .and_then(|machine| machine.get("served_by"))
            .and_then(Json::list)
            .map(|served| served.iter().filter_map(Json::text).map(str::to_string).collect())
            .unwrap_or_default();
        match (status, text(Some(&said), "session"), text(machine, "name"), text(machine, "address"), port) {
            (200, Some(session), Some(name), Some(address), Some(port)) => {
                // what the machine is asked to start, where the session is for a program
                let program = machine
                    .and_then(|machine| machine.get("program"))
                    .and_then(|program| program.text_of("run"))
                    .map(|run| Program { run: run.to_string(), arguments: String::new() });
                let themselves = machine.and_then(|machine| machine.text_of("signs_in")) == Some("themselves");
                Ok(Some(Taken { session, machine: name, address, port, collection, served_by, program, themselves }))
            }
            _ => Err(format!("the server answered the ticket with the status {status}, and with nothing a session can begin with")),
        }
    }

    /// Says what became of a session. Gives the status the server answered with.
    pub async fn told(&self, session: &str, what: &str, more: &[(&str, &str)]) -> Result<u16, String> {
        let mut says = vec![("session", session), ("what", what)];
        says.extend_from_slice(more);
        self.ask("/internal/told", &says).await.map(|(status, _)| status)
    }
}

/// What a session needs of the gateway around it.
pub struct Around {
    /// The server, where the gateway is told of one.
    pub server: Option<Server>,
    /// The domain whose machines people are signed in on as themselves, where the
    /// installation names one.
    pub domain: Option<Domain>,
    /// Where the gateway writes what happens.
    pub lines: Lines,
}

/// Why a session ended, as the browser and the record are told it.
enum Ended {
    /// The person left.
    Left,
    /// The browser's connection ended.
    BrowserGone,
    /// The machine ended the connection, or it broke.
    Machine(String),
    /// The person closed the last window of the program.
    Closed,
}

/// Runs one session on a browser's connection, from its first message to its end.
pub async fn run<Connection>(mut browser: WebSocketStream<Connection>, sign_in: Option<String>, around: Arc<Around>)
where
    Connection: AsyncRead + AsyncWrite + Unpin,
{
    // which session this is: the ticket the server gave for it
    let begun = tokio::time::timeout(BEGINS_WITHIN, said(&mut browser)).await;
    let (ticket, width, height, layout) = match begun {
        Ok(Some(FromBrowser::Begins { ticket, width, height, layout })) => {
            (ticket, width, height, layout.unwrap_or(away_rdp::gcc::LAYOUT_US))
        }
        _ => return end(&mut browser, "no-session").await,
    };
    let (Some(server), Some(sign_in)) = (&around.server, sign_in) else { return end(&mut browser, "no-session").await };
    let taken = match server.take(&ticket, &sign_in).await {
        Ok(Some(taken)) => taken,
        Ok(None) => return end(&mut browser, "no-session").await,
        Err(why) => {
            around.lines.warn("a session could not be taken over", &[("why", why.as_str().into())]);
            return end(&mut browser, "no-session").await;
        }
    };
    around
        .lines
        .info("a session was taken over", &[("session", taken.session.as_str().into()), ("machine", taken.machine.as_str().into())]);

    let opened = open(&mut browser, server, around.domain.as_ref(), &taken, width, height, layout).await;
    let (why, details) = match opened {
        Ok(Some(Open { mut desktop, served_by })) => {
            let picture = desktop.picture();
            let (wide, high) = (picture.width.to_string(), picture.height.to_string());
            around.lines.info(
                "a desktop is open",
                &[
                    ("session", taken.session.as_str().into()),
                    ("machine", taken.machine.as_str().into()),
                    ("served_by", served_by.as_deref().unwrap_or(taken.address.as_str()).into()),
                ],
            );
            // the record that it is open, before anything of it is shown: with the machine
            // that serves it, where a farm's broker chose one
            let mut says = vec![("width", wide.as_str()), ("height", high.as_str())];
            says.extend(served_by.as_deref().map(|served_by| ("served_by", served_by)));
            let recorded = server.told(&taken.session, "opened", &says).await;
            let ended = match recorded {
                Ok(200) => show(&mut browser, &mut desktop).await,
                Ok(status) => Ended::Machine(format!("the server did not take the record that the desktop is open: status {status}")),
                Err(why) => Ended::Machine(why),
            };
            desktop.leave().await;
            match ended {
                Ended::Left => ("left", String::new()),
                Ended::BrowserGone => ("browser-gone", String::new()),
                Ended::Machine(details) => ("machine", details),
                Ended::Closed => ("closed", String::new()),
            }
        }
        // the person gave up before the desktop was open
        Ok(None) => ("left", String::new()),
        Err(failed) => {
            let (step, why) = (step_word(failed.step), why_word(failed.why));
            around.lines.warn(
                "a session did not open",
                &[
                    ("session", taken.session.as_str().into()),
                    ("step", step.into()),
                    ("why", why.into()),
                    ("details", failed.details.as_str().into()),
                ],
            );
            let _ = server.told(&taken.session, "failed", &[("step", step), ("why", why), ("details", &failed.details)]).await;
            let _ = tell(&mut browser, ToBrowser::Failed { step, why, details: &failed.details }).await;
            let _ = browser.close(None).await;
            return;
        }
    };
    around
        .lines
        .info("a session ended", &[("session", taken.session.as_str().into()), ("why", why.into()), ("details", details.as_str().into())]);
    if let Err(unsaid) = server.told(&taken.session, "ended", &[("why", why)]).await {
        around.lines.warn(
            "the server could not be told that a session ended",
            &[("session", taken.session.as_str().into()), ("why", unsaid.as_str().into())],
        );
    }
    end(&mut browser, why).await;
}

/// A desktop that is open.
struct Open {
    desktop: Desktop,
    /// The machine that serves it, where a farm's broker sent the connection on to one.
    served_by: Option<String>,
}

/// The way to the desktop, step by step, each said to the browser as it begins. Nothing
/// where the person left on the way.
async fn open<Connection>(
    browser: &mut WebSocketStream<Connection>,
    server: &Server,
    domain: Option<&Domain>,
    taken: &Taken,
    width: u16,
    height: u16,
    layout: u32,
) -> Result<Option<Open>, Failure>
where
    Connection: AsyncRead + AsyncWrite + Unpin,
{
    let gone = |step: Step| Failure { step, why: Why::Broken, details: "the browser's connection ended".to_string() };
    // a farm's broker is told which collection is wanted, in the words a farm knows
    let wanted = taken.collection.as_ref().map(|collection| format!("tsv://MS Terminal Services Plugin.1.{collection}"));
    let wanted = wanted.as_deref().map(str::as_bytes);
    tell(browser, ToBrowser::Step("reaching")).await.map_err(|()| gone(Step::Reaching))?;
    let reached = machine::reach(&taken.address, taken.port, wanted).await?;

    // what the machine shows of itself, against what Away knows of it: kept the first time
    tell(browser, ToBrowser::Step("checking")).await.map_err(|()| gone(Step::Checking))?;
    let secured = reached.secure().await?;
    let shows = secured.identity().fingerprint_text();
    known(server, taken, &taken.address, taken.port, &shows).await?;
    tell(browser, ToBrowser::Step("signing-in")).await.map_err(|()| gone(Step::SigningIn))?;
    let (width, height) = (width.clamp(640, 4096) & !3, height.clamp(480, 2160));
    let mut again = false;
    let (mut desktop, served_by) = 'asked: {
        // the person is signed in as themselves: nothing is asked, and the connection
        // that showed who the machine is goes on
        if taken.themselves {
            let not = |details: &str| Failure { step: Step::SigningIn, why: Why::NotThemselves, details: details.to_string() };
            let domain = domain.ok_or_else(|| not("this installation names no domain to sign people in on the machines of"))?;
            let who = domain.themselves(&taken.session).await?;
            let theirs = domain.on(&who, &taken.address).await?;
            let program = taken.program.clone();
            let sent_on = match secured.sign_in(Who::Themselves(Box::new(theirs)), width, height, layout, None, program).await? {
                Opened::Desktop(desktop) => break 'asked (*desktop, None),
                Opened::SentOn(to) => to,
            };
            // a farm's broker says which machine serves the collection: the person is
            // signed in there with a ticket for that machine, and a card of its own
            let (host, port) = served_by(&sent_on, taken).await?;
            let secured = machine::reach(&host, port, sent_on.token.as_deref()).await?.secure().await?;
            known(server, taken, &host, port, &secured.identity().fingerprint_text()).await?;
            let theirs = domain.on(&who, &host).await?;
            let program = taken.program.clone();
            match secured.sign_in(Who::Themselves(Box::new(theirs)), width, height, layout, Some(sent_on.session), program).await? {
                Opened::Desktop(desktop) => break 'asked (*desktop, Some(format!("{host}:{port}"))),
                Opened::SentOn(_) => {
                    let details = format!("{host} sends the connection on again, which a machine of a farm doesn't");
                    return Err(Failure { step: Step::SigningIn, why: Why::Broken, details });
                }
            }
        }
        // the machine signs people in by what they type. The connection that showed who
        // it is is let go: the person may take their time to type, and a machine
        // doesn't wait that long
        drop(secured);
        for _ in 0..ASKS_AT_MOST {
            tell(browser, ToBrowser::Asks { again }).await.map_err(|()| gone(Step::SigningIn))?;
            let typed = tokio::time::timeout(TYPED_WITHIN, async {
                loop {
                    match said(browser).await {
                        Some(FromBrowser::Credentials { name, password }) => break Some((name, password)),
                        Some(FromBrowser::Leaves) | None => break None,
                        Some(_) => {}
                    }
                }
            })
            .await;
            let Ok(Some((name, password))) = typed else { return Ok(None) };
            let secured = machine::reach(&taken.address, taken.port, wanted).await?.secure().await?;
            if secured.identity().fingerprint_text() != shows {
                let details = "it showed another identity than a moment before".to_string();
                return Err(Failure { step: Step::Checking, why: Why::AnotherIdentity, details });
            }
            let program = taken.program.clone();
            let typed = Who::Account(Account::named(&name, password.clone()));
            let sent_on = match secured.sign_in(typed, width, height, layout, None, program).await {
                Ok(Opened::Desktop(desktop)) => break 'asked (*desktop, None),
                Ok(Opened::SentOn(to)) => to,
                Err(Failure { why: Why::SignInRefused, .. }) => {
                    again = true;
                    continue;
                }
                Err(failed) => return Err(failed),
            };
            // the machine is a farm's broker, and says which machine serves the collection:
            // Away goes there where the files name that machine, and nowhere otherwise
            let (host, port) = served_by(&sent_on, taken).await?;
            let secured = machine::reach(&host, port, sent_on.token.as_deref()).await?.secure().await?;
            known(server, taken, &host, port, &secured.identity().fingerprint_text()).await?;
            let program = taken.program.clone();
            let typed = Who::Account(Account::named(&name, password));
            match secured.sign_in(typed, width, height, layout, Some(sent_on.session), program).await {
                Ok(Opened::Desktop(desktop)) => break 'asked (*desktop, Some(format!("{host}:{port}"))),
                Ok(Opened::SentOn(_)) => {
                    let details = format!("{host} sends the connection on again, which a machine of a farm doesn't");
                    return Err(Failure { step: Step::SigningIn, why: Why::Broken, details });
                }
                Err(Failure { why: Why::SignInRefused, .. }) => again = true,
                Err(failed) => return Err(failed),
            }
        }
        let details = "the machine took none of the names and passwords".to_string();
        return Err(Failure { step: Step::SigningIn, why: Why::SignInRefused, details });
    };

    // then its first picture. A program is open once the machine says that it started:
    // its windows come when the program has them, and a first sign-in takes its time
    tell(browser, ToBrowser::Step("first-picture")).await.map_err(|()| gone(Step::FirstPicture))?;
    let within = match taken.program {
        Some(_) => machine::PROGRAM_STARTS_WITHIN,
        None => machine::FIRST_PICTURE_WITHIN,
    };
    let first = tokio::time::timeout(within, async {
        loop {
            let drawn = desktop.take().await?;
            if !drawn.is_empty() || desktop.started() {
                // what was drawn is for whoever shows the desktop: said again there
                break Ok::<(), Failure>(());
            }
            // a machine that signs the person in with a card asks the reader meanwhile,
            // and waits for its word as time passes
            if let Ok(arrived) = tokio::time::timeout(machine::TICKS_EVERY, desktop.arrives()).await {
                arrived?;
            }
            desktop.tick().await?;
        }
    })
    .await;
    match first {
        Ok(Ok(())) => Ok(Some(Open { desktop, served_by })),
        Ok(Err(failed)) => Err(Failure { step: Step::FirstPicture, ..failed }),
        Err(_) => {
            let details = format!("it showed nothing within {} seconds", within.as_secs());
            Err(Failure { step: Step::FirstPicture, why: Why::NoPicture, details })
        }
    }
}

/// Compares what a machine shows of itself with what Away knows of the machine at that
/// address: the server keeps what it showed the first time.
async fn known(server: &Server, taken: &Taken, host: &str, port: u16, shows: &str) -> Result<(), Failure> {
    let at = format!("{host}:{port}");
    let failed = |why: Why, details: String| Failure { step: Step::Checking, why, details };
    match server.told(&taken.session, "identity", &[("at", &at), ("shows", shows)]).await {
        Ok(200) => Ok(()),
        Ok(409) => Err(failed(Why::AnotherIdentity, format!("{at} shows the identity {shows}, which is not the one Away knows"))),
        Ok(status) => Err(failed(Why::Broken, format!("the server could not say what it knows of the machine: status {status}"))),
        Err(why) => Err(failed(Why::Broken, why)),
    }
}

/// The machine a farm's broker sends the connection to, and its port. It is one the
/// files name: one of those that serve the collection, or the address the entry itself
/// is reached at, where the broker serves the collection too. Otherwise the connection
/// goes nowhere: a broker's word alone leads Away to no machine.
async fn served_by(sent_on: &away_rdp::redirection::Redirection, taken: &Taken) -> Result<(String, u16), Failure> {
    for served in taken.served_by.iter().map(String::as_str).chain([taken.address.as_str()]) {
        // an address, or an address and the port its desktop is opened on
        let (host, port) = match served.rsplit_once(':') {
            Some((host, port)) if !host.is_empty() && !host.ends_with(':') && port.parse::<u16>().is_ok() => {
                (host, port.parse().unwrap_or(taken.port))
            }
            _ => (served, taken.port),
        };
        // a number of the newer kind is written in brackets, which are not part of it
        let written = host.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')).unwrap_or(host);
        let is = |name: Option<&str>| name.is_some_and(|name| name.eq_ignore_ascii_case(written));
        let by_address = match &sent_on.address {
            Some(address) if address == written => true,
            // the broker names a number, and the files a name: the name's addresses say whether they are one machine
            Some(address) => {
                tokio::net::lookup_host((written, port)).await.is_ok_and(|mut found| found.any(|found| found.ip().to_string() == *address))
            }
            None => false,
        };
        if is(sent_on.name.as_deref()) || is(sent_on.short_name.as_deref()) || by_address {
            return Ok((host.to_string(), port));
        }
    }
    let to =
        sent_on.name.as_deref().or(sent_on.short_name.as_deref()).or(sent_on.address.as_deref()).unwrap_or("a machine it doesn't name");
    let details = match taken.served_by.is_empty() {
        true => format!("the farm's broker sends the connection to {to}, and the files name no machine that serves the collection"),
        false => format!("the farm's broker sends the connection to {to}, which the files don't name for the collection"),
    };
    Err(Failure { step: Step::SigningIn, why: Why::SentElsewhere, details })
}

/// Shows an open desktop to the browser, and passes keys and the pointer on, until one
/// of the two sides ends it.
async fn show<Connection>(browser: &mut WebSocketStream<Connection>, desktop: &mut Desktop) -> Ended
where
    Connection: AsyncRead + AsyncWrite + Unpin,
{
    let (wide, high) = (desktop.picture().width, desktop.picture().height);
    if tell(browser, ToBrowser::Desktop { width: wide as u16, height: high as u16 }).await.is_err() {
        return Ended::BrowserGone;
    }
    // everything is new to a browser that has seen nothing yet
    let mut changed = Changed::default();
    changed.add(Area { left: 0, top: 0, width: wide, height: high });
    let (mut sent, mut seen) = (0u32, 0u32);
    // a program whose last window was closed is at its end, once no new one came in its place
    let mut had_windows = false;
    let mut closed_since: Option<tokio::time::Instant> = None;
    loop {
        // where the windows of the programs are goes out before what is drawn in them
        if let Some((windows, in_front)) = desktop.windows() {
            if tell(browser, ToBrowser::Windows { windows: &windows, in_front }).await.is_err() {
                return Ended::BrowserGone;
            }
            match desktop.windows_listed() {
                0 if had_windows => closed_since = closed_since.or(Some(tokio::time::Instant::now())),
                0 => {}
                _ => (had_windows, closed_since) = (true, None),
            }
        }
        let closed = async {
            match closed_since {
                Some(since) => tokio::time::sleep_until(since + CLOSED_FOR).await,
                None => std::future::pending().await,
            }
        };
        // what changed goes out once the browser has drawn what went before: states in
        // between are left out, and the picture it gets is always the newest
        if !changed.is_empty() && sent.wrapping_sub(seen) < ON_THEIR_WAY {
            let rectangles = packed(desktop.picture(), changed.take(wide, high)).await;
            for (area, png) in &rectangles {
                let rectangle = ToBrowser::Rectangle {
                    left: area.left as u16,
                    top: area.top as u16,
                    width: area.width as u16,
                    height: area.height as u16,
                    png,
                };
                if tell(browser, rectangle).await.is_err() {
                    return Ended::BrowserGone;
                }
            }
            sent = sent.wrapping_add(1);
            if tell(browser, ToBrowser::Drawn(sent)).await.is_err() {
                return Ended::BrowserGone;
            }
        }
        tokio::select! {
            // a machine that reads a card over the connection is told how time passes
            _ = tokio::time::sleep(machine::TICKS_EVERY) => {
                if let Err(failed) = desktop.tick().await {
                    return Ended::Machine(failed.details);
                }
            }
            arrived = desktop.arrives() => {
                let drawn = match arrived {
                    Ok(()) => desktop.take().await,
                    Err(failed) => Err(failed),
                };
                match drawn {
                    Ok(areas) => areas.into_iter().for_each(|area| changed.add(area)),
                    Err(failed) => return Ended::Machine(failed.details),
                }
            }
            told = said(browser) => match told {
                Some(FromBrowser::Did(did)) => {
                    let events: Vec<Input> = did.iter().map(|did| input(did, wide, high)).collect();
                    if let Err(failed) = desktop.tell(&events).await {
                        return Ended::Machine(failed.details);
                    }
                }
                Some(FromBrowser::Seen(number)) => seen = number,
                Some(FromBrowser::Window { id, does }) => {
                    if let Err(failed) = desktop.window(id, does).await {
                        return Ended::Machine(failed.details);
                    }
                }
                Some(FromBrowser::Leaves) => return Ended::Left,
                Some(_) => {}
                None => return Ended::BrowserGone,
            },
            () = closed => return Ended::Closed,
        }
    }
}

/// The areas of the picture that changed and are not sent yet, as squares of a grid:
/// what changed twice is sent once.
#[derive(Default)]
struct Changed {
    tiles: BTreeSet<(usize, usize)>,
}

impl Changed {
    fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    fn add(&mut self, area: Area) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        for row in area.top / TILE..=(area.top + area.height - 1) / TILE {
            for column in area.left / TILE..=(area.left + area.width - 1) / TILE {
                self.tiles.insert((row, column));
            }
        }
    }

    /// Takes what changed, as few rectangles: squares side by side become one strip, and
    /// strips of the same width under each other one rectangle.
    fn take(&mut self, wide: usize, high: usize) -> Vec<Area> {
        let mut strips: Vec<(usize, usize, usize)> = Vec::new();
        for &(row, column) in &self.tiles {
            match strips.last_mut() {
                Some((of_row, _, until)) if *of_row == row && *until == column => *until += 1,
                _ => strips.push((row, column, column + 1)),
            }
        }
        self.tiles.clear();
        let mut areas: Vec<(usize, usize, usize, usize)> = Vec::new();
        for (row, from, until) in strips {
            match areas.iter_mut().find(|(_, bottom, left, right)| *bottom == row && *left == from && *right == until) {
                Some((_, bottom, _, _)) => *bottom += 1,
                None => areas.push((row, row + 1, from, until)),
            }
        }
        areas
            .into_iter()
            .map(|(top, bottom, left, right)| {
                let (left, top) = (left * TILE, top * TILE);
                Area { left, top, width: (right * TILE).min(wide) - left, height: (bottom * TILE).min(high) - top }
            })
            .filter(|area| area.left < wide && area.top < high)
            .collect()
    }
}

/// Packs areas of the picture as PNG pictures, which every browser reads by itself and
/// which lose nothing. The packing is work, and is done beside what waits for the
/// machine and the browser.
async fn packed(picture: &Picture, areas: Vec<Area>) -> Vec<(Area, Vec<u8>)> {
    // the pixels of each area, as red, green and blue
    let cut: Vec<(Area, Vec<u8>)> = areas
        .into_iter()
        .map(|area| {
            let mut pixels = Vec::with_capacity(area.width * area.height * 3);
            for row in area.top..area.top + area.height {
                let from = (row * picture.width + area.left) * 4;
                for pixel in picture.pixels[from..from + area.width * 4].as_chunks::<4>().0 {
                    pixels.extend([pixel[2], pixel[1], pixel[0]]);
                }
            }
            (area, pixels)
        })
        .collect();
    let packing = tokio::task::spawn_blocking(move || {
        cut.into_iter()
            .filter_map(|(area, pixels)| {
                let mut png = Vec::with_capacity(pixels.len() / 4 + 128);
                let mut encoder = png::Encoder::new(&mut png, area.width as u32, area.height as u32);
                encoder.set_color(png::ColorType::Rgb);
                encoder.set_depth(png::BitDepth::Eight);
                encoder.set_compression(png::Compression::Fast);
                let mut writer = encoder.write_header().ok()?;
                writer.write_image_data(&pixels).ok()?;
                writer.finish().ok()?;
                Some((area, png))
            })
            .collect::<Vec<_>>()
    });
    packing.await.unwrap_or_default()
}

/// What a person did, as the machine is told of it: a place beyond the desktop is its edge.
fn input(did: &Did, wide: usize, high: usize) -> Input {
    let within = |x: u16, y: u16| (x.min(wide.saturating_sub(1) as u16), y.min(high.saturating_sub(1) as u16));
    match *did {
        Did::Moved { x, y } => {
            let (x, y) = within(x, y);
            Input::Moved { x, y }
        }
        Did::Button { button, down, x, y } => {
            let (x, y) = within(x, y);
            let button = match button {
                away_wire::Button::Main => Button::Left,
                away_wire::Button::Middle => Button::Middle,
                away_wire::Button::Second => Button::Right,
                away_wire::Button::Back => Button::Back,
                away_wire::Button::Forward => Button::Forward,
            };
            Input::Button { button, down, x, y }
        }
        Did::Wheel { by, sideways, x, y } => {
            let (x, y) = within(x, y);
            Input::Wheel { by, sideways, x, y }
        }
        Did::Key { code, extended, down } => Input::Key { code, extended, down },
        Did::Character { unit, down } => Input::Character { unit, down },
        Did::Locks { scroll, num, caps } => Input::Locks { scroll, num, caps },
    }
}

fn step_word(step: Step) -> &'static str {
    match step {
        Step::Reaching => "reaching",
        Step::Checking => "checking",
        Step::SigningIn => "signing-in",
        Step::FirstPicture => "first-picture",
    }
}

fn why_word(why: Why) -> &'static str {
    match why {
        Why::NoAddress => "no-address",
        Why::NoAnswer => "no-answer",
        Why::Refused => "refused",
        Why::AnotherIdentity => "another-identity",
        Why::SignInRefused => "sign-in-refused",
        Why::NotThemselves => "not-as-yourself",
        Why::NoPicture => "no-picture",
        Why::SentElsewhere => "sent-elsewhere",
        Why::ProgramNotStarted => "program-not-started",
        Why::Broken => "broken",
    }
}

/// The next thing the browser says. Nothing where its connection ended, or where what
/// it sent is no message of a session.
async fn said<Connection>(browser: &mut WebSocketStream<Connection>) -> Option<FromBrowser>
where
    Connection: AsyncRead + AsyncWrite + Unpin,
{
    loop {
        let carried = match browser.next().await? {
            Ok(Message::Text(text)) => Carried::Text(text.as_str().to_string()),
            Ok(Message::Binary(bytes)) => Carried::Bytes(bytes.to_vec()),
            // the connection answers a ping by itself
            Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_)) => continue,
            Ok(Message::Close(_)) | Err(_) => return None,
        };
        return FromBrowser::read(&carried).ok();
    }
}

/// Tells the browser something.
async fn tell<Connection>(browser: &mut WebSocketStream<Connection>, says: ToBrowser<'_>) -> Result<(), ()>
where
    Connection: AsyncRead + AsyncWrite + Unpin,
{
    let message = match says.write() {
        Carried::Text(text) => Message::text(text),
        Carried::Bytes(bytes) => Message::binary(bytes),
    };
    browser.send(message).await.map_err(|_| ())
}

/// Says why the session ended, and closes the browser's connection.
async fn end<Connection>(browser: &mut WebSocketStream<Connection>, why: &str)
where
    Connection: AsyncRead + AsyncWrite + Unpin,
{
    let _ = tell(browser, ToBrowser::Ended(why)).await;
    let _ = browser.close(None).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_changed_is_sent_as_few_rectangles_and_what_changed_twice_once() {
        let mut changed = Changed::default();
        assert!(changed.is_empty());
        // a small mark, twice: one square of the grid
        changed.add(Area { left: 70, top: 70, width: 12, height: 12 });
        changed.add(Area { left: 75, top: 72, width: 3, height: 3 });
        assert_eq!(changed.take(200, 120), [Area { left: 64, top: 64, width: 64, height: 56 }]);
        assert!(changed.is_empty());
        // the whole picture is one rectangle, cut at the picture's edges
        changed.add(Area { left: 0, top: 0, width: 200, height: 120 });
        assert_eq!(changed.take(200, 120), [Area { left: 0, top: 0, width: 200, height: 120 }]);
        // a row of squares is one strip, and squares apart are apart
        changed.add(Area { left: 0, top: 0, width: 130, height: 10 });
        changed.add(Area { left: 190, top: 100, width: 5, height: 5 });
        assert_eq!(
            changed.take(200, 120),
            [Area { left: 0, top: 0, width: 192, height: 64 }, Area { left: 128, top: 64, width: 72, height: 56 }]
        );
        // an area of nothing changes nothing
        changed.add(Area { left: 5, top: 5, width: 0, height: 9 });
        assert!(changed.is_empty());
    }
}
