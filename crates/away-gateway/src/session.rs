//! One session: a browser's connection on one side, a machine's desktop on the other,
//! and the gateway's picture of the desktop between them
//! ([ADR-0004](../../../spec/architecture/decisions/0004-the-session-and-its-picture.md)).
//!
//! The browser is told the steps of the connection, asked for a name and a password
//! where the machine signs people in by what they type, and then sent what the machine
//! draws: as rectangles it reads by itself, and only as fast as it confirms them. Keys
//! and the pointer go the other way.

use std::collections::{BTreeSet, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use away_part::address::Address;
use away_part::client::Client;
use away_part::json::Json;
use away_part::lines::Lines;
use away_part::Stop;
use away_rdp::ntlm::Account;
use away_rdp::rail::Program;
use away_secret::Secret;
use away_wire::{Carried, FromBrowser, ToBrowser};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

use crate::machine::{self, Area, CopiedFile, CopiedPicture, Desktop, Failure, Opened, Passes, Screen, Step, Wanted, Who, Why};
use crate::shown::{Pixels, Shown};
use crate::themselves::Domain;
use crate::vnc;

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
    /// Whether this session requires browser Restricted Admin.
    pub restricted_admin: bool,
    /// Immutable subject from the authenticated server session.
    pub subject: String,
    /// Which ways the clipboard passes on the machine.
    pub clipboard: Passes,
    /// Whether the desktop is reached over VNC, and not over RDP.
    pub over_vnc: bool,
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
                // the clipboard passes the ways the server says, and neither way where it says nothing
                let clipboard = match machine.and_then(|machine| machine.text_of("clipboard")) {
                    Some("both") => Passes::BOTH,
                    Some("to-the-machine") => Passes { to_the_machine: true, from_the_machine: false },
                    Some("from-the-machine") => Passes { to_the_machine: false, from_the_machine: true },
                    _ => Passes::NEITHER,
                };
                let restricted_admin = machine.and_then(|machine| machine.text_of("signs_in")) == Some("restricted-admin-sso");
                let subject = text(said.get("person"), "subject").unwrap_or_default();
                let over_vnc = machine.and_then(|machine| machine.text_of("over")) == Some("vnc");
                Ok(Some(Taken {
                    over_vnc,
                    restricted_admin,
                    subject,
                    session,
                    machine: name,
                    address,
                    port,
                    collection,
                    served_by,
                    program,
                    themselves,
                    clipboard,
                }))
            }
            _ => Err(format!("the server answered the ticket with the status {status}, and with nothing a session can begin with")),
        }
    }

    /// Asks for another program of the machine in a session that runs, by the name Away
    /// gives it: what the machine is asked to start, where the person may open it there
    /// still. Nothing where they may not.
    pub async fn start(&self, session: &str, program: &str) -> Result<Option<Program>, String> {
        let (status, said) = self.ask("/internal/start", &[("session", session), ("program", program)]).await?;
        match (status, said.text_of("run")) {
            (200, Some(run)) if !run.is_empty() => Ok(Some(Program { run: run.to_string(), arguments: String::new() })),
            (200, _) => Err("the server named no program to start".to_string()),
            (403 | 404, _) => Ok(None),
            (status, _) => Err(format!("the server answered with the status {status}")),
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
    /// The explicitly enabled browser Restricted Admin policy.
    pub restricted_admin: Option<crate::restricted_admin::RestrictedAdmin>,
    /// Where the gateway writes what happens.
    pub lines: Lines,
    /// Learns that the gateway is stopping: a desktop that is open then is told so.
    pub stop: Stop,
    /// How many sessions run: a gateway that stops gives them a moment to say so.
    pub running: Arc<AtomicUsize>,
}

/// One session that runs, counted for as long as it does.
struct Counted(Arc<AtomicUsize>);

impl Counted {
    fn one_more(running: &Arc<AtomicUsize>) -> Counted {
        running.fetch_add(1, Ordering::SeqCst);
        Counted(running.clone())
    }
}

impl Drop for Counted {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Why a session ended, as the browser and the record are told it.
enum Ended {
    /// The gateway stops, as when Away is updated: the desktop goes on at the machine,
    /// and the page opens it again by itself.
    Updating,
    /// The same desktop was opened in another window or on another device, and is there now.
    Elsewhere,
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
    let _counted = Counted::one_more(&around.running);
    // which session this is: the ticket the server gave for it
    let begun = tokio::time::timeout(BEGINS_WITHIN, said(&mut browser)).await;
    let (ticket, screen, layout) = match begun {
        Ok(Some(FromBrowser::Begins { ticket, width, height, layout, scale })) => {
            (ticket, Screen::for_a_window(width, height, scale.unwrap_or(100)), layout.unwrap_or(away_rdp::gcc::LAYOUT_US))
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

    // a desktop over VNC and one over RDP open in their own ways, and are shown in the same
    let shown = match taken.over_vnc {
        true => {
            let opened = open_over_vnc(&mut browser, &taken).await.map(|opened| opened.map(|desktop| Open { desktop, served_by: None }));
            shown(&mut browser, server, &around, &taken, screen, opened).await
        }
        false => {
            let opened = open(&mut browser, server, &around, &taken, screen, layout).await;
            shown(&mut browser, server, &around, &taken, screen, opened).await
        }
    };
    let Some((why, details)) = shown else { return };
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
struct Open<Of> {
    desktop: Of,
    /// The machine that serves it, where a farm's broker sent the connection on to one.
    served_by: Option<String>,
}

/// Shows a desktop that opened until the session ends, and says why it ended. Nothing
/// where it did not open: that was said, and the browser's connection is closed.
async fn shown<Connection, Of: Shown>(
    browser: &mut WebSocketStream<Connection>,
    server: &Server,
    around: &Around,
    taken: &Taken,
    screen: Screen,
    opened: Result<Option<Open<Of>>, Failure>,
) -> Option<(&'static str, String)>
where
    Connection: AsyncRead + AsyncWrite + Unpin,
{
    match opened {
        Ok(Some(Open { mut desktop, served_by })) => {
            let (wide, high) = (desktop.picture().width.to_string(), desktop.picture().height.to_string());
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
            // in a session for a program, the person may ask for others of the machine
            let starts = taken.program.is_some().then_some(Starts { server, session: &taken.session, lines: &around.lines });
            let ended = match recorded {
                Ok(200) => {
                    show(browser, &mut desktop, screen, taken.clipboard, starts, Some((server, &taken.session)), around.stop.clone()).await
                }
                Ok(status) => Ended::Machine(format!("the server did not take the record that the desktop is open: status {status}")),
                Err(why) => Ended::Machine(why),
            };
            desktop.leave().await;
            Some(match ended {
                Ended::Updating => ("updating", String::new()),
                Ended::Elsewhere => ("elsewhere", String::new()),
                Ended::Left => ("left", String::new()),
                Ended::BrowserGone => ("browser-gone", String::new()),
                Ended::Machine(details) => ("machine", details),
                Ended::Closed => ("closed", String::new()),
            })
        }
        // the person gave up before the desktop was open
        Ok(None) => Some(("left", String::new())),
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
            let _ = tell(browser, ToBrowser::Failed { step, why, details: &failed.details }).await;
            let _ = browser.close(None).await;
            None
        }
    }
}

/// The way to a desktop over VNC, step by step, each said to the browser as it begins.
/// Nothing where the person left on the way. Such a machine shows nothing of itself
/// that could be checked, and asks for the desktop's own password or for nothing.
async fn open_over_vnc<Connection>(browser: &mut WebSocketStream<Connection>, taken: &Taken) -> Result<Option<vnc::Desktop>, Failure>
where
    Connection: AsyncRead + AsyncWrite + Unpin,
{
    let gone = |step: Step| Failure { step, why: Why::Broken, details: "the browser's connection ended".to_string() };
    tell(browser, ToBrowser::Step("reaching")).await.map_err(|()| gone(Step::Reaching))?;
    let reached = vnc::reach(&taken.address, taken.port, taken.clipboard).await?;
    tell(browser, ToBrowser::Step("signing-in")).await.map_err(|()| gone(Step::SigningIn))?;
    let mut desktop = 'asked: {
        if !reached.asks() {
            break 'asked reached.sign_in(None).await?;
        }
        // the machine asks for its password. The connection that said so is let go: the
        // person may take their time to type, and a machine doesn't wait that long
        drop(reached);
        let mut again = false;
        for _ in 0..ASKS_AT_MOST {
            tell(browser, ToBrowser::Asks { again, instead: false, password_alone: true }).await.map_err(|()| gone(Step::SigningIn))?;
            let typed = tokio::time::timeout(TYPED_WITHIN, async {
                loop {
                    match said(browser).await {
                        Some(FromBrowser::Credentials { password, .. }) => break Some(password),
                        Some(FromBrowser::Leaves) | None => break None,
                        Some(_) => {}
                    }
                }
            })
            .await;
            let Ok(Some(password)) = typed else { return Ok(None) };
            match vnc::reach(&taken.address, taken.port, taken.clipboard).await?.sign_in(Some(&password)).await {
                Ok(desktop) => break 'asked desktop,
                Err(Failure { why: Why::SignInRefused, .. }) => again = true,
                Err(failed) => return Err(failed),
            }
        }
        let details = "the machine took none of the passwords".to_string();
        return Err(Failure { step: Step::SigningIn, why: Why::SignInRefused, details });
    };

    // then its first picture
    tell(browser, ToBrowser::Step("first-picture")).await.map_err(|()| gone(Step::FirstPicture))?;
    let within = machine::FIRST_PICTURE_WITHIN;
    let first = tokio::time::timeout(within, async {
        loop {
            if !desktop.take().await?.is_empty() {
                // what was drawn is for whoever shows the desktop: said again there
                break Ok::<(), Failure>(());
            }
            desktop.arrives().await?;
        }
    })
    .await;
    match first {
        Ok(Ok(())) => Ok(Some(desktop)),
        Ok(Err(failed)) => Err(Failure { step: Step::FirstPicture, ..failed }),
        Err(_) => {
            let details = format!("it showed nothing within {} seconds", within.as_secs());
            Err(Failure { step: Step::FirstPicture, why: Why::NoPicture, details })
        }
    }
}

fn not_owned(details: String) -> Failure {
    Failure { step: Step::SigningIn, why: Why::NotThemselves, details }
}

/// The way to the desktop, step by step, each said to the browser as it begins. Nothing
/// where the person left on the way.
async fn open<Connection>(
    browser: &mut WebSocketStream<Connection>,
    server: &Server,
    around: &Around,
    taken: &Taken,
    screen: Screen,
    layout: u32,
) -> Result<Option<Open<Desktop>>, Failure>
where
    Connection: AsyncRead + AsyncWrite + Unpin,
{
    let domain = around.domain.as_ref();
    let gone = |step: Step| Failure { step, why: Why::Broken, details: "the browser's connection ended".to_string() };
    // a farm's broker is told which collection is wanted, in the words a farm knows
    let wanted = taken.collection.as_ref().map(|collection| format!("tsv://MS Terminal Services Plugin.1.{collection}"));
    let wanted = wanted.as_deref().map(str::as_bytes);
    tell(browser, ToBrowser::Step("reaching")).await.map_err(|()| gone(Step::Reaching))?;
    let restricted = if taken.restricted_admin {
        let not = |details: &str| Failure { step: Step::SigningIn, why: Why::NotThemselves, details: details.into() };
        let policy = around.restricted_admin.as_ref().ok_or_else(|| not("browser Restricted Admin is not enabled in this installation"))?;
        if taken.collection.is_some() || taken.program.is_some() || !taken.served_by.is_empty() {
            return Err(not("browser Restricted Admin only opens a direct management desktop"));
        }
        policy.account_for(&taken.subject, &taken.address, taken.port)?;
        // Refresh the authenticated sign-in immediately before privileged admission.
        let (status, said) = server.ask("/internal/vouch", &[("session", &taken.session)]).await.map_err(not_owned)?;
        if status != 200 || said.text_of("subject") != Some(&taken.subject) {
            return Err(not("the signed-in identity could not be confirmed for Restricted Admin"));
        }
        Some(policy)
    } else {
        None
    };
    let reached = if restricted.is_some() {
        machine::reach_restricted_admin(&taken.address, taken.port).await?
    } else {
        machine::reach(&taken.address, taken.port, wanted).await?
    };

    // what the machine shows of itself, against what Away knows of it: kept the first time
    tell(browser, ToBrowser::Step("checking")).await.map_err(|()| gone(Step::Checking))?;
    let secured = reached.secure().await?.passing(taken.clipboard);
    let shows = secured.identity().fingerprint_text();
    if let Some(policy) = restricted {
        policy.check_identity(&taken.address, &shows)?;
    }
    known(server, taken, &taken.address, taken.port, &shows).await?;
    tell(browser, ToBrowser::Step("signing-in")).await.map_err(|()| gone(Step::SigningIn))?;
    let mut again = false;
    // A password is requested only for an explicitly typed catalog entry.
    let instead = false;
    let (mut desktop, served_by) = 'asked: {
        if let Some(policy) = restricted {
            let ticket = policy.ticket(&taken.subject, &taken.address, taken.port).await?;
            match secured.sign_in(Who::RestrictedAdmin(ticket), screen, layout, None, None).await? {
                Opened::Desktop(desktop) => break 'asked (*desktop, None),
                Opened::SentOn(_) => return Err(not_owned("Restricted Admin refuses redirected targets".into())),
            }
        }
        // A certificate refusal never downgrades a passwordless session to a password.
        if taken.themselves {
            let not = |details: &str| Failure { step: Step::SigningIn, why: Why::NotThemselves, details: details.to_string() };
            let domain = domain.ok_or_else(|| not("this installation names no domain to sign people in on the machines of"))?;
            if let Some(who) = domain.themselves(&taken.session).await? {
                let theirs = domain.on(&who, &taken.address).await?;
                let program = taken.program.clone();
                let sent_on = match secured.sign_in(Who::Themselves(Box::new(theirs)), screen, layout, None, program).await? {
                    Opened::Desktop(desktop) => break 'asked (*desktop, None),
                    Opened::SentOn(to) => to,
                };
                // a farm's broker says which machine serves the collection: the person is
                // signed in there with a ticket for that machine, and a card of its own
                let (host, port) = served_by(&sent_on, taken).await?;
                let secured = machine::reach(&host, port, sent_on.token.as_deref()).await?.secure().await?.passing(taken.clipboard);
                known(server, taken, &host, port, &secured.identity().fingerprint_text()).await?;
                let theirs = domain.on(&who, &host).await?;
                let program = taken.program.clone();
                match secured.sign_in(Who::Themselves(Box::new(theirs)), screen, layout, Some(sent_on.session), program).await? {
                    Opened::Desktop(desktop) => break 'asked (*desktop, Some(format!("{host}:{port}"))),
                    Opened::SentOn(_) => {
                        let details = format!("{host} sends the connection on again, which a machine of a farm doesn't");
                        return Err(Failure { step: Step::SigningIn, why: Why::Broken, details });
                    }
                }
            }
            return Err(not("certificate SSO is not authorized for this identity and destination"));
        }
        // the machine signs people in by what they type. The connection that showed who
        // it is is let go: the person may take their time to type, and a machine
        // doesn't wait that long
        drop(secured);
        for _ in 0..ASKS_AT_MOST {
            tell(browser, ToBrowser::Asks { again, instead, password_alone: false }).await.map_err(|()| gone(Step::SigningIn))?;
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
            let secured = machine::reach(&taken.address, taken.port, wanted).await?.secure().await?.passing(taken.clipboard);
            if secured.identity().fingerprint_text() != shows {
                let details = "it showed another identity than a moment before".to_string();
                return Err(Failure { step: Step::Checking, why: Why::AnotherIdentity, details });
            }
            let program = taken.program.clone();
            let typed = Who::Account(Account::named(&name, password.clone()));
            let sent_on = match secured.sign_in(typed, screen, layout, None, program).await {
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
            let secured = machine::reach(&host, port, sent_on.token.as_deref()).await?.secure().await?.passing(taken.clipboard);
            known(server, taken, &host, port, &secured.identity().fingerprint_text()).await?;
            let program = taken.program.clone();
            let typed = Who::Account(Account::named(&name, password));
            match secured.sign_in(typed, screen, layout, Some(sent_on.session), program).await {
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

/// Where a session for a program asks for others: the server says whether the person
/// may open them, and what the machine is asked to start.
struct Starts<'a> {
    server: &'a Server,
    session: &'a str,
    lines: &'a Lines,
}

/// Shows an open desktop to the browser, and passes keys and the pointer on, until one
/// of the two sides ends it.
async fn show<Connection, Of: Shown>(
    browser: &mut WebSocketStream<Connection>,
    desktop: &mut Of,
    mut screen: Screen,
    clipboard: Passes,
    starts: Option<Starts<'_>>,
    records: Option<(&Server, &str)>,
    mut stop: Stop,
) -> Ended
where
    Connection: AsyncRead + AsyncWrite + Unpin,
{
    let (mut wide, mut high) = (desktop.picture().width, desktop.picture().height);
    let written = desktop.takes_keys_as_written();
    if tell(browser, ToBrowser::Desktop { width: wide as u16, height: high as u16, written }).await.is_err() {
        return Ended::BrowserGone;
    }
    // which ways the clipboard passes, for the page to say and to act on
    let passes = ToBrowser::Passes {
        to_the_machine: clipboard.to_the_machine,
        from_the_machine: clipboard.from_the_machine,
        pictures: desktop.carries_pictures() && clipboard.at_all(),
    };
    if tell(browser, passes).await.is_err() {
        return Ended::BrowserGone;
    }
    // everything is new to a browser that has seen nothing yet
    let mut changed = Changed::default();
    changed.add(Area { left: 0, top: 0, width: wide, height: high });
    let (mut sent, mut seen) = (0u32, 0u32);
    // a program whose last window was closed is at its end, once no new one came in its
    // place and no program the person asked for is still on its way
    let mut had_windows = false;
    let mut closed_since: Option<tokio::time::Instant> = None;
    // the programs asked for in the session, by their names, until the machine says whether they started
    let mut asked_for: VecDeque<String> = VecDeque::new();
    // the pieces of a picture the person copied, until its last one is there: nothing
    // while none is on its way, or once it turned out larger than a picture may be
    let mut arriving: Option<Vec<u8>> = None;
    // the files that were last copied on the machine, the one the person is having, and
    // the one they asked for while a piece of another was still on its way
    let mut listed: Vec<CopiedFile> = Vec::new();
    let mut having: Option<Having> = None;
    let mut next: Option<u32> = None;
    let mut had = 0u32;
    // the files the person copied, for the machine to paste; what the machine wants of
    // them, in the order it asked; the piece that is being given, as it comes from the
    // browser; and the files of which the machine had the end, which are records
    let mut theirs: Vec<CopiedFile> = Vec::new();
    let mut wants: VecDeque<Wanted> = VecDeque::new();
    let mut giving: Option<(Wanted, Vec<u8>)> = None;
    let mut handed: BTreeSet<u32> = BTreeSet::new();
    loop {
        // where the windows of the programs are goes out before what is drawn in them, and
        // with them the icons that changed
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
        for (window, png) in desktop.icons() {
            if tell(browser, ToBrowser::Icon { window, png: &png }).await.is_err() {
                return Ended::BrowserGone;
            }
        }
        // the icons of the tray, and their pictures after them
        if let Some(icons) = desktop.tray() {
            if tell(browser, ToBrowser::Tray(&icons)).await.is_err() {
                return Ended::BrowserGone;
            }
        }
        for (window, icon, png) in desktop.tray_pictures() {
            if tell(browser, ToBrowser::TrayPicture { window, icon, png: &png }).await.is_err() {
                return Ended::BrowserGone;
            }
        }
        for started in desktop.started_later() {
            let program = asked_for.pop_front().unwrap_or_default();
            let details = started.as_ref().err().map_or("", String::as_str);
            // why a machine didn't start a program is for whoever looks after it, too: most often
            // the program is published at a place where it no longer is
            if let (Some(starts), false) = (&starts, started.is_ok()) {
                starts.lines.warn(
                    "a program did not start in a session",
                    &[("session", starts.session.into()), ("program", program.as_str().into()), ("details", details.into())],
                );
            }
            let started = started.is_ok();
            if tell(browser, ToBrowser::Started { program: &program, started, details }).await.is_err() {
                return Ended::BrowserGone;
            }
            if !started && had_windows && desktop.windows_listed() == 0 {
                closed_since = closed_since.or(Some(tokio::time::Instant::now()));
            }
        }
        let closing = closed_since.filter(|_| asked_for.is_empty());
        let closed = async {
            match closing {
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
                    return ended_by(failed);
                }
            }
            arrived = desktop.arrives() => {
                let drawn = match arrived {
                    Ok(()) => desktop.take().await,
                    Err(failed) => Err(failed),
                };
                let areas = match drawn {
                    Ok(areas) => areas,
                    Err(failed) => return ended_by(failed),
                };
                // the machine offers its desktop anew, at another size: the browser is
                // told the size, and everything of the new picture is new to it
                if desktop.resized() {
                    (wide, high) = (desktop.picture().width, desktop.picture().height);
                    if tell(browser, ToBrowser::Desktop { width: wide as u16, height: high as u16, written }).await.is_err() {
                        return Ended::BrowserGone;
                    }
                    changed = Changed::default();
                    changed.add(Area { left: 0, top: 0, width: wide, height: high });
                }
                areas.into_iter().for_each(|area| changed.add(area));
                // what was copied on the machine is for the person's clipboard
                if let Some(text) = desktop.copied() {
                    if tell(browser, ToBrowser::Copied(&text)).await.is_err() {
                        return Ended::BrowserGone;
                    }
                }
                // what the machine wants of the files the person copied, as it pastes
                // them: the browser is asked for it, a piece at a time
                wants.extend(desktop.wanted());
                if wants_the_next(browser, &mut wants, &mut giving).await.is_err() {
                    return Ended::BrowserGone;
                }
                // the files that were copied: which they are. What the person was having
                // of the ones before is given up
                if let Some(files) = desktop.copied_files() {
                    let told: Vec<away_wire::CopiedFile> =
                        files.iter().map(|file| away_wire::CopiedFile { name: file.name.clone(), size: file.size, folder: file.folder }).collect();
                    listed = files;
                    next = None;
                    if having.take().is_some() && tell(browser, ToBrowser::File { whole: false }).await.is_err() {
                        return Ended::BrowserGone;
                    }
                    if tell(browser, ToBrowser::CopiedFiles(&told)).await.is_err() {
                        return Ended::BrowserGone;
                    }
                }
                // a piece of the file the person is having goes on to them, and the next
                // one is asked for: the machine is asked no faster than the browser takes
                if let Some(piece) = desktop.piece() {
                    let mut ends = None;
                    match (having.as_mut(), piece) {
                        // given up meanwhile: the piece is dropped
                        (Some(file), _) if file.given_up => having = None,
                        (Some(file), Some(piece)) if !piece.is_empty() => {
                            file.came += piece.len() as u64;
                            if tell(browser, ToBrowser::PieceOfAFile(&piece)).await.is_err() {
                                return Ended::BrowserGone;
                            }
                            // all of it, by the size the machine named, or more to come
                            let whole = file.size > 0 && file.came >= file.size;
                            match whole {
                                true => ends = Some(true),
                                false => match desktop.ask_for_a_piece(file.at, file.came).await {
                                    Ok(true) => {}
                                    Ok(false) => ends = Some(false),
                                    Err(failed) => return ended_by(failed),
                                },
                            }
                        }
                        // a piece of nothing: the file ends there
                        (Some(_), Some(_)) => ends = Some(true),
                        (Some(_), None) => ends = Some(false),
                        (None, _) => {}
                    }
                    // the file is at its end, one way or the other: only then is it no longer had
                    if let (Some(whole), Some(file)) = (ends, having.take_if(|_| ends.is_some())) {
                        if tell(browser, ToBrowser::File { whole }).await.is_err() {
                            return Ended::BrowserGone;
                        }
                        // a file that passed is a record: its name and its size, and no more of it
                        if let (true, Some((server, session))) = (whole, records) {
                            had += 1;
                            let (number, size) = (had.to_string(), file.came.to_string());
                            let _ = server
                                .told(session, "file", &[("number", &number), ("name", &file.name), ("size", &size), ("direction", "from-the-machine")])
                                .await;
                        }
                    }
                    // the file they asked for meanwhile is begun
                    if let (None, Some(at)) = (&having, next.take()) {
                        match begin_having(desktop, &listed, at).await {
                            Ok(Some(file)) => having = Some(file),
                            Ok(None) => {
                                if tell(browser, ToBrowser::File { whole: false }).await.is_err() {
                                    return Ended::BrowserGone;
                                }
                            }
                            Err(failed) => return ended_by(failed),
                        }
                    }
                }
                // a picture goes as one a browser reads by itself, packed by the gateway:
                // the browser gets nothing the machine wrote
                if let Some(picture) = desktop.copied_picture() {
                    if let Some(png) = packed_whole(picture).await {
                        if tell(browser, ToBrowser::CopiedPicture { png: &png }).await.is_err() {
                            return Ended::BrowserGone;
                        }
                    }
                }
            }
            told = said(browser) => match told {
                Some(FromBrowser::Did(did)) => {
                    if let Err(failed) = desktop.did(&did).await {
                        return ended_by(failed);
                    }
                }
                Some(FromBrowser::Seen(number)) => seen = number,
                Some(FromBrowser::Window { id, does }) => {
                    if let Err(failed) = desktop.window(id, does).await {
                        return ended_by(failed);
                    }
                }
                Some(FromBrowser::Tray { window, icon, does }) => {
                    if let Err(failed) = desktop.use_tray(window, icon, does).await {
                        return ended_by(failed);
                    }
                }
                Some(FromBrowser::Start(program)) => {
                    // the server says whether the person may open it there, and what it is
                    let asked = match &starts {
                        Some(starts) => match starts.server.start(starts.session, &program).await {
                            Ok(asked) => asked,
                            Err(why) => {
                                starts.lines.warn(
                                    "a program could not be asked for in a session",
                                    &[("session", starts.session.into()), ("program", program.as_str().into()), ("why", why.as_str().into())],
                                );
                                None
                            }
                        },
                        None => None,
                    };
                    match asked {
                        Some(run) => {
                            if let Err(failed) = desktop.start(&run).await {
                                return ended_by(failed);
                            }
                            asked_for.push_back(program);
                        }
                        None => {
                            if tell(browser, ToBrowser::Started { program: &program, started: false, details: "" }).await.is_err() {
                                return Ended::BrowserGone;
                            }
                        }
                    }
                }
                Some(FromBrowser::Leaves) => return Ended::Left,
                // what the person copied on their own device can be pasted on the machine
                Some(FromBrowser::Copied(text)) => {
                    if let Err(failed) = desktop.offer(&text).await {
                        return ended_by(failed);
                    }
                }
                // a file that was copied on the machine, by its place among those listed:
                // what is in it is asked for, a piece at a time. One at a time: while a
                // piece of another is on its way, this one waits for it
                Some(FromBrowser::File(at)) => match having.as_mut() {
                    Some(file) => {
                        file.given_up = true;
                        next = Some(at);
                    }
                    None => match begin_having(desktop, &listed, at).await {
                        Ok(Some(file)) => having = Some(file),
                        Ok(None) => {
                            if tell(browser, ToBrowser::File { whole: false }).await.is_err() {
                                return Ended::BrowserGone;
                            }
                        }
                        Err(failed) => return ended_by(failed),
                    },
                },
                // they give up on it: the piece that is on its way is dropped when it comes
                Some(FromBrowser::NoFile) => {
                    next = None;
                    if let Some(file) = having.as_mut() {
                        file.given_up = true;
                    }
                }
                // the files they copied, for the machine to paste: it is told their names
                // and sizes. What it wanted of the ones before is not given any more
                Some(FromBrowser::Gives(files)) => {
                    for (wanted, _) in giving.take().into_iter().chain(wants.drain(..).map(|wanted| (wanted, Vec::new()))) {
                        if let Err(failed) = desktop.give_a_piece(wanted.number, None).await {
                            return ended_by(failed);
                        }
                    }
                    theirs = files.into_iter().map(|file| CopiedFile { name: file.name, size: file.size, folder: false }).collect();
                    handed.clear();
                    if let Err(failed) = desktop.offer_files(theirs.clone()).await {
                        return ended_by(failed);
                    }
                }
                // what the machine wanted of one comes in pieces: once its last one is
                // there, the machine is given it. More than it wanted is not given
                Some(FromBrowser::OfTheirFile { first, last, piece }) => {
                    if let Some((wanted, came)) = giving.as_mut() {
                        if first {
                            came.clear();
                        }
                        let fits = came.len() + piece.len() <= wanted.length as usize;
                        came.extend_from_slice(if fits { &piece } else { &[] });
                        if last || !fits {
                            let (wanted, came) = giving.take().expect("what is being given");
                            if let Err(failed) = desktop.give_a_piece(wanted.number, fits.then_some(came.as_slice())).await {
                                return ended_by(failed);
                            }
                            // a file of which the machine had the end passed: that is a record
                            let file = theirs.get(wanted.file as usize).filter(|file| fits && wanted.from + came.len() as u64 >= file.size);
                            if let (Some(file), true, Some((server, session))) = (file, handed.insert(wanted.file), records) {
                                had += 1;
                                let (number, size) = (had.to_string(), file.size.to_string());
                                let _ = server
                                    .told(session, "file", &[("number", &number), ("name", &file.name), ("size", &size), ("direction", "to-the-machine")])
                                    .await;
                            }
                            if wants_the_next(browser, &mut wants, &mut giving).await.is_err() {
                                return Ended::BrowserGone;
                            }
                        }
                    }
                }
                // their device has nothing to give of it: the machine is told so
                Some(FromBrowser::NothingOfTheirFile) => {
                    if let Some((wanted, _)) = giving.take() {
                        if let Err(failed) = desktop.give_a_piece(wanted.number, None).await {
                            return ended_by(failed);
                        }
                    }
                    if wants_the_next(browser, &mut wants, &mut giving).await.is_err() {
                        return Ended::BrowserGone;
                    }
                }
                // a picture they copied comes in pieces: once it is whole, and is one,
                // the machine is told that it can be pasted
                Some(FromBrowser::CopiedPicture { first, last, piece }) => {
                    if first {
                        arriving = Some(Vec::new());
                    }
                    arriving = arriving.filter(|held| held.len() + piece.len() <= LARGEST_FROM_A_BROWSER);
                    if let Some(held) = arriving.as_mut() {
                        held.extend_from_slice(&piece);
                    }
                    // only its last piece makes it whole: until then it goes on arriving
                    if let Some(png) = arriving.take_if(|_| last) {
                        if let Some(picture) = unpacked(png).await {
                            if let Err(failed) = desktop.offer_picture(picture).await {
                                return ended_by(failed);
                            }
                        }
                    }
                }
                // the window has another size, or is on another screen: a machine that
                // follows it is told, and offers its desktop anew
                Some(FromBrowser::Sized { width, height, scale }) => {
                    let now = Screen::for_a_window(width, height, scale.unwrap_or(100));
                    if now != screen {
                        match desktop.resize(now).await {
                            Ok(true) => screen = now,
                            Ok(false) => {}
                            Err(failed) => return ended_by(failed),
                        }
                    }
                }
                Some(_) => {}
                None => return Ended::BrowserGone,
            },
            () = closed => return Ended::Closed,
            // the gateway stops: the browser is told, so that it comes back by itself
            () = stop.wait() => return Ended::Updating,
        }
    }
}

/// How a session ended whose machine ended it, or whose connection to the machine broke.
fn ended_by(failed: Failure) -> Ended {
    match failed.why {
        Why::OpenedElsewhere => Ended::Elsewhere,
        _ => Ended::Machine(failed.details),
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
async fn packed(picture: Pixels<'_>, areas: Vec<Area>) -> Vec<(Area, Vec<u8>)> {
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

/// Asks the browser for the next piece the machine wants of a file the person copied,
/// where none is being given: one at a time, in the order the machine asked.
async fn wants_the_next<Connection>(
    browser: &mut WebSocketStream<Connection>,
    wants: &mut VecDeque<Wanted>,
    giving: &mut Option<(Wanted, Vec<u8>)>,
) -> Result<(), ()>
where
    Connection: AsyncRead + AsyncWrite + Unpin,
{
    if giving.is_none() {
        if let Some(wanted) = wants.pop_front() {
            tell(browser, ToBrowser::Wants { file: wanted.file, from: wanted.from, length: wanted.length }).await?;
            *giving = Some((wanted, Vec::new()));
        }
    }
    Ok(())
}

/// A file the person is having from the machine: its place among those listed, its name
/// and the size the machine named, how much of it came, and whether they gave up on it.
struct Having {
    at: u32,
    name: String,
    size: u64,
    came: u64,
    given_up: bool,
}

/// Begins having a file: asks the machine for its first piece. Nothing where it is not
/// among those listed, is a folder, or the machine can't be asked now.
async fn begin_having<Of: Shown>(desktop: &mut Of, listed: &[CopiedFile], at: u32) -> Result<Option<Having>, Failure> {
    let Some(file) = listed.get(at as usize).filter(|file| !file.folder) else { return Ok(None) };
    Ok(desktop.ask_for_a_piece(at, 0).await?.then(|| Having { at, name: file.name.clone(), size: file.size, came: 0, given_up: false }))
}

/// The largest picture a person's device hands over, as the PNG picture it comes as.
const LARGEST_FROM_A_BROWSER: usize = 16 * 1024 * 1024;

/// Packs a picture that was copied on the machine as one PNG picture. Nothing where
/// that fails.
async fn packed_whole(picture: CopiedPicture) -> Option<Vec<u8>> {
    let packing = tokio::task::spawn_blocking(move || {
        let mut png = Vec::with_capacity(picture.pixels.len() / 4 + 128);
        let mut encoder = png::Encoder::new(&mut png, picture.width as u32, picture.height as u32);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&picture.pixels).ok()?;
        writer.finish().ok()?;
        Some(png)
    });
    packing.await.ok().flatten()
}

/// Unpacks a PNG picture a person copied. Nothing for what is no such picture, or one
/// that is larger than a clipboard's picture may be. What is seen through in it lies
/// on white, as a machine's clipboard has no such pixels.
async fn unpacked(png: Vec<u8>) -> Option<CopiedPicture> {
    let unpacking = tokio::task::spawn_blocking(move || {
        let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
        decoder.set_limits(png::Limits { bytes: away_rdp::clipboard::LARGEST });
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder.read_info().ok()?;
        let mut held = vec![0; reader.output_buffer_size().filter(|size| *size <= away_rdp::clipboard::LARGEST)?];
        let frame = reader.next_frame(&mut held).ok()?;
        let (width, height) = (frame.width as usize, frame.height as usize);
        let on_white = |colour: u8, seen: u8| ((u32::from(colour) * u32::from(seen) + 255 * (255 - u32::from(seen)) + 127) / 255) as u8;
        let held = &held[..frame.buffer_size()];
        let pixels: Vec<u8> = match frame.color_type {
            png::ColorType::Rgb => held.to_vec(),
            png::ColorType::Rgba => {
                held.as_chunks::<4>().0.iter().flat_map(|pixel| [0, 1, 2].map(|nth| on_white(pixel[nth], pixel[3]))).collect()
            }
            png::ColorType::Grayscale => held.iter().flat_map(|grey| [*grey; 3]).collect(),
            png::ColorType::GrayscaleAlpha => held.as_chunks::<2>().0.iter().flat_map(|pixel| [on_white(pixel[0], pixel[1]); 3]).collect(),
            png::ColorType::Indexed => return None,
        };
        (pixels.len() == width * height * 3).then_some(CopiedPicture { width, height, pixels })
    });
    unpacking.await.ok().flatten()
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
        Why::OpenedElsewhere => "opened-elsewhere",
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

    /// A PNG picture of these pixels, written the way named.
    fn png_of(width: u32, height: u32, colours: png::ColorType, pixels: &[u8]) -> Vec<u8> {
        let mut png = Vec::new();
        let mut encoder = png::Encoder::new(&mut png, width, height);
        encoder.set_color(colours);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("a head");
        writer.write_image_data(pixels).expect("the pixels");
        writer.finish().expect("the end");
        png
    }

    #[tokio::test]
    async fn a_copied_picture_is_packed_for_the_browser_and_unpacked_for_the_machine() {
        // what the machine copied goes as a PNG picture, and comes back as it was
        let picture = CopiedPicture { width: 3, height: 2, pixels: (1..=18).map(|byte| byte * 10).collect() };
        let png = packed_whole(picture.clone()).await.expect("a PNG picture");
        assert_eq!(&png[..4], &[0x89, b'P', b'N', b'G']);
        assert_eq!(unpacked(png).await, Some(picture));

        // what a person copied may be seen through: it lies on white for the machine
        let seen_through = png_of(2, 1, png::ColorType::Rgba, &[200, 100, 50, 255, 200, 100, 50, 0]);
        assert_eq!(unpacked(seen_through).await, Some(CopiedPicture { width: 2, height: 1, pixels: vec![200, 100, 50, 255, 255, 255] }));
        // and may be grey, with or without what is seen through
        let grey = png_of(2, 1, png::ColorType::Grayscale, &[0, 128]);
        assert_eq!(unpacked(grey).await, Some(CopiedPicture { width: 2, height: 1, pixels: vec![0, 0, 0, 128, 128, 128] }));
        let grey_seen_through = png_of(1, 1, png::ColorType::GrayscaleAlpha, &[0, 128]);
        assert_eq!(unpacked(grey_seen_through).await.map(|picture| picture.pixels), Some(vec![127, 127, 127]));

        // what is no picture, or one that is cut, is none
        assert_eq!(unpacked(b"no picture".to_vec()).await, None);
        let whole = png_of(4, 4, png::ColorType::Rgb, &[7; 48]);
        assert_eq!(unpacked(whole[..whole.len() - 20].to_vec()).await, None);
        assert_eq!(unpacked(Vec::new()).await, None);
    }
}
