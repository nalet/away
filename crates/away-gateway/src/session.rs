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
use away_secret::Secret;
use away_wire::{Carried, Did, FromBrowser, ToBrowser};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

use crate::machine::{self, Area, Desktop, Failure, Step, Why};

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
        match (status, text(Some(&said), "session"), text(machine, "name"), text(machine, "address"), port) {
            (200, Some(session), Some(machine), Some(address), Some(port)) => Ok(Some(Taken { session, machine, address, port })),
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

    let opened = open(&mut browser, server, &taken, width, height, layout).await;
    let (why, details) = match opened {
        Ok(Some(mut desktop)) => {
            let picture = desktop.picture();
            let (wide, high) = (picture.width.to_string(), picture.height.to_string());
            around
                .lines
                .info("a desktop is open", &[("session", taken.session.as_str().into()), ("machine", taken.machine.as_str().into())]);
            // the record that it is open, before anything of it is shown
            let recorded = server.told(&taken.session, "opened", &[("width", &wide), ("height", &high)]).await;
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

/// The way to the desktop, step by step, each said to the browser as it begins. Nothing
/// where the person left on the way.
async fn open<Connection>(
    browser: &mut WebSocketStream<Connection>,
    server: &Server,
    taken: &Taken,
    width: u16,
    height: u16,
    layout: u32,
) -> Result<Option<Desktop>, Failure>
where
    Connection: AsyncRead + AsyncWrite + Unpin,
{
    let gone = |step: Step| Failure { step, why: Why::Broken, details: "the browser's connection ended".to_string() };
    tell(browser, ToBrowser::Step("reaching")).await.map_err(|()| gone(Step::Reaching))?;
    let reached = machine::reach(&taken.address, taken.port).await?;

    // what the machine shows of itself, against what Away knows of it: kept the first time
    tell(browser, ToBrowser::Step("checking")).await.map_err(|()| gone(Step::Checking))?;
    let secured = reached.secure().await?;
    let shows = secured.identity().fingerprint_text();
    let at = format!("{}:{}", taken.address, taken.port);
    match server.told(&taken.session, "identity", &[("at", &at), ("shows", &shows)]).await {
        Ok(200) => {}
        Ok(409) => {
            return Err(Failure {
                step: Step::Checking,
                why: Why::AnotherIdentity,
                details: format!("it shows the identity {shows}, which is not the one Away knows"),
            })
        }
        Ok(status) => {
            return Err(Failure {
                step: Step::Checking,
                why: Why::Broken,
                details: format!("the server could not say what it knows of the machine: status {status}"),
            })
        }
        Err(why) => return Err(Failure { step: Step::Checking, why: Why::Broken, details: why }),
    }
    // the connection that showed it is let go: the person may take their time to type,
    // and a machine doesn't wait that long
    drop(secured);

    // the machine signs people in by what they type
    tell(browser, ToBrowser::Step("signing-in")).await.map_err(|()| gone(Step::SigningIn))?;
    let (width, height) = (width.clamp(640, 4096) & !3, height.clamp(480, 2160));
    let mut again = false;
    let mut desktop = 'asked: {
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
            let secured = machine::reach(&taken.address, taken.port).await?.secure().await?;
            if secured.identity().fingerprint_text() != shows {
                return Err(Failure {
                    step: Step::Checking,
                    why: Why::AnotherIdentity,
                    details: "it showed another identity than a moment before".to_string(),
                });
            }
            match secured.sign_in(Account::named(&name, password), width, height, layout).await {
                Ok(desktop) => break 'asked desktop,
                Err(Failure { why: Why::SignInRefused, .. }) => again = true,
                Err(failed) => return Err(failed),
            }
        }
        return Err(Failure {
            step: Step::SigningIn,
            why: Why::SignInRefused,
            details: "the machine took none of the names and passwords".to_string(),
        });
    };

    // then its first picture
    tell(browser, ToBrowser::Step("first-picture")).await.map_err(|()| gone(Step::FirstPicture))?;
    let first = tokio::time::timeout(machine::FIRST_PICTURE_WITHIN, async {
        loop {
            let drawn = desktop.take().await?;
            if !drawn.is_empty() {
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
            Err(Failure { step: Step::FirstPicture, why: Why::NoPicture, details: "it showed nothing within 30 seconds".to_string() })
        }
    }
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
    loop {
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
                Some(FromBrowser::Leaves) => return Ended::Left,
                Some(_) => {}
                None => return Ended::BrowserGone,
            },
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
        Why::NoPicture => "no-picture",
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
