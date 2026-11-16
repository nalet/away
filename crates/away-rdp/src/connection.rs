//! A connection, from what the client says of itself to the machine drawing: the steps
//! in their order, each answered with what to send next (MS-RDPBCGR 1.3.1.1).
//!
//! It begins once the connection is secured and, where the machine wanted it, the person
//! is signed in. It takes the frames the machine sends and gives bytes to send and what
//! happened. It holds no connection itself.

use crate::channel::{self, Pieces};
use crate::clipboard::{self, Clipboard};
use crate::devices;
use crate::dynamic::{self, Later};
use crate::gcc::{self, Client, Machine};
use crate::input::{self, Input};
use crate::licence::{self, Said, Taking};
use crate::mcs::{self, Arrived};
use crate::pdu::{self, FromMachine};
use crate::rail::{self, Command};
use crate::redirection::{self, Redirection};
use crate::secret::Secret;
use crate::smartcard::{self, Reader};
use crate::tpkt::{self, Frame};
use crate::update::{self, Rectangle, Updates};
use crate::window::{Change, Window};
use crate::Error;

/// The title Windows gives the language bar where it floats on its own.
const FLOATING_LANGUAGE_BAR: &str = "CiceroUIWndFrame-TF_FloatingLangBar_WndTitle";

/// What happened, for whoever holds the connection to show or to act on.
#[derive(Debug, PartialEq, Eq)]
pub enum Happened {
    /// The machine gave the channels: this client's own, and the one of the desktop.
    Channels {
        /// The client's own channel.
        user: u16,
        /// The channel everything of the desktop travels on.
        desktop: u16,
    },
    /// The machine takes the client without a licence.
    Licensed,
    /// The machine hands out licences, and asks the client to take one.
    AsksToTakeALicence {
        /// Who made what the licence is for.
        company: String,
        /// What the licence is for.
        product: String,
    },
    /// The machine handed over a licence, and the client took it.
    LicenceTaken(licence::Taken),
    /// The machine offers a desktop of this size.
    Desktop {
        /// How wide, in pixels.
        width: u16,
        /// How high.
        height: u16,
        /// How many bits a pixel.
        bits: u16,
    },
    /// Both sides have said what they can do, and the machine draws from now on.
    Open,
    /// The machine drew these rectangles.
    Drawn(Vec<Rectangle>),
    /// The machine is a farm's broker, and sends the client on to the machine that
    /// serves it. This connection is at its end.
    SentOn(Redirection),
    /// The machine is ready for programs, and was asked for the one this connection is for.
    AskedForTheProgram,
    /// The program started.
    Started,
    /// The program did not start.
    NotStarted {
        /// Why, as RDP numbers the reasons ([`rail::why`] says each in words).
        result: u16,
        /// Why, as the machine's own system numbers them.
        raw: u32,
    },
    /// What the machine says of the windows of programs: new ones, changed ones, gone
    /// ones, and which is in front.
    Windows(Vec<Change>),
    /// The machine's answer on the client's smart card reader: it took it, or did not.
    Reader {
        /// Whether the machine took the reader.
        taken: bool,
    },
    /// The machine asked the reader for something.
    AskedTheReader(smartcard::Asked),
    /// Text was copied on the machine, and is for the person's clipboard.
    Copied(String),
}

/// The client's smart card reader, and how far the machine and the client are with it.
struct Devices {
    reader: Reader,
    /// The pieces of a message about devices that is not whole yet.
    pieces: Pieces,
    /// Whether the machine said what it can do, and whether it took the client's number:
    /// the reader is named once both are so.
    can_do: bool,
    confirmed: bool,
    named: bool,
}

enum State {
    AwaitsMachine,
    AwaitsUser,
    Joins(Vec<u16>),
    AwaitsLicence,
    AwaitsWhatItCanDo,
    Finishes,
    Open,
}

/// One connection.
pub struct Connection {
    state: State,
    client: Client,
    domain: String,
    name: String,
    password: Option<Secret>,
    for_a_licence: Option<licence::New>,
    taking: Option<Taking>,
    machine: Machine,
    user: u16,
    share_id: u32,
    reason: u32,
    updates: Updates,
    /// The pieces of a message about programs that is not whole yet.
    pieces: Pieces,
    /// The size of the desktop the machine offers.
    size: (u16, u16),
    /// The further channels the client asked for, in the order it asked for them.
    asked_for: Vec<&'static str>,
    devices: Option<Devices>,
    /// The channels the machine opens later, and the pieces of a message about them
    /// that is not whole yet: where the client asked for the channel they travel on.
    later: Option<(Pieces, Later)>,
    /// The clipboard, and the pieces of a message about it that is not whole yet: where
    /// it passes at all.
    clipboard: Option<(Pieces, Clipboard)>,
    /// The windows the client hid, which it says nothing of to whoever holds it.
    hidden: Vec<u32>,
}

impl Connection {
    /// Begins: what the client says of itself, to send first. The domain and the name are
    /// of the account the desktop is for. The password is given only where the machine
    /// signs the account in itself: where the account signed in before the desktop, the
    /// machine has it already. What a licence is taken with is used only where the
    /// machine hands out licences. A smart card reader is given where the machine is to
    /// reach one over the connection, with the card the person is signed in with.
    pub fn begin(
        client: Client,
        domain: &str,
        name: &str,
        password: Option<Secret>,
        for_a_licence: licence::New,
        reader: Option<Reader>,
    ) -> (Connection, Vec<u8>) {
        let first = tpkt::data_frame(&mcs::connect(&gcc::request(&client, reader.is_some())));
        let asked_for = gcc::channels(&client, reader.is_some());
        let later = asked_for.contains(&dynamic::CHANNEL).then(|| (Pieces::default(), Later::default()));
        let passes = asked_for.contains(&clipboard::CHANNEL).then(|| (Pieces::default(), Clipboard::new(client.clipboard)));
        let connection = Connection {
            state: State::AwaitsMachine,
            client,
            domain: domain.to_string(),
            name: name.to_string(),
            password,
            for_a_licence: Some(for_a_licence),
            taking: None,
            machine: Machine::default(),
            user: 0,
            share_id: 0,
            reason: 0,
            updates: Updates::new(),
            pieces: Pieces::default(),
            size: (0, 0),
            asked_for,
            devices: reader.map(|reader| Devices { reader, pieces: Pieces::default(), can_do: false, confirmed: false, named: false }),
            later,
            clipboard: passes,
            hidden: Vec::new(),
        };
        (connection, first)
    }

    /// Takes one frame from the machine. Gives what to send, and what happened.
    pub fn receive(&mut self, frame: Frame) -> Result<(Vec<u8>, Vec<Happened>), Error> {
        let carried = match frame {
            Frame::Fast(_, carried) => {
                return match self.state {
                    State::Finishes | State::Open => {
                        let drawn = self.updates.short_frame(&carried)?;
                        let mut windows = self.updates.take_windows();
                        let mut happened = Vec::new();
                        let send = self.language_bar_put_away(&mut windows);
                        // where the windows are comes before what is drawn in them
                        if !windows.is_empty() {
                            happened.push(Happened::Windows(windows));
                        }
                        if !drawn.is_empty() {
                            happened.push(Happened::Drawn(drawn));
                        }
                        Ok((send, happened))
                    }
                    _ => Err(Error::NotRdp("the machine draws before the connection is open")),
                };
            }
            Frame::Slow(carried) => carried,
        };
        let message = tpkt::data(&carried)?;
        match &mut self.state {
            State::AwaitsMachine => {
                self.machine = gcc::read_answer(mcs::read_connect_answer(message)?)?;
                // a program is started over a channel of its own, which the machine has to give
                if self.client.program.is_some() && self.channel(rail::CHANNEL).is_none() {
                    return Err(Error::NoPrograms);
                }
                self.state = State::AwaitsUser;
                Ok((mcs::erect_and_attach().iter().flat_map(|message| tpkt::data_frame(message)).collect(), Vec::new()))
            }
            State::AwaitsUser => {
                self.user = mcs::read_attached(message)?;
                let mut joins = vec![self.machine.io_channel];
                joins.extend(self.machine.message_channel);
                joins.extend(&self.machine.channels);
                self.state = State::Joins(joins);
                Ok((tpkt::data_frame(&mcs::join(self.user, self.user)), Vec::new()))
            }
            State::Joins(left) => {
                mcs::read_joined(message)?;
                if left.is_empty() {
                    self.state = State::AwaitsLicence;
                    // the password is used here and kept no longer
                    let info = pdu::client_info(&self.domain, &self.name, self.password.take().as_ref(), self.client.program.is_some());
                    let send = tpkt::data_frame(&mcs::send(self.user, self.machine.io_channel, &info));
                    return Ok((send, vec![Happened::Channels { user: self.user, desktop: self.machine.io_channel }]));
                }
                let next = left.remove(0);
                Ok((tpkt::data_frame(&mcs::join(self.user, next)), Vec::new()))
            }
            _ => match mcs::read_arrived(message)? {
                Arrived::Ending => Err(Error::Ended(self.reason)),
                Arrived::On(channel, arrived) if channel == self.machine.io_channel => self.on_the_desktops_channel(arrived),
                Arrived::On(channel, arrived) if Some(channel) == self.channel(rail::CHANNEL) => self.on_the_programs_channel(arrived),
                Arrived::On(channel, arrived) if Some(channel) == self.channel(devices::CHANNEL) => self.on_the_devices_channel(arrived),
                Arrived::On(channel, arrived) if Some(channel) == self.channel(dynamic::CHANNEL) => self.on_the_later_channels(arrived),
                Arrived::On(channel, arrived) if Some(channel) == self.channel(clipboard::CHANNEL) => {
                    self.on_the_clipboards_channel(arrived)
                }
                Arrived::On(..) => Ok((Vec::new(), Vec::new())),
            },
        }
    }

    /// The number the machine gave a channel the client asked for by this name.
    fn channel(&self, name: &str) -> Option<u16> {
        let at = self.asked_for.iter().position(|asked| *asked == name)?;
        self.machine.channels.get(at).copied().filter(|number| *number != 0)
    }

    /// A message on a further channel, in the pieces and the frames it travels in.
    fn on(&self, name: &str, message: &[u8]) -> Vec<u8> {
        let Some(on) = self.channel(name) else { return Vec::new() };
        channel::pieces(message, channel::shows_its_heads(name))
            .iter()
            .flat_map(|piece| tpkt::data_frame(&mcs::send(self.user, on, piece)))
            .collect()
    }

    /// A message about programs, in the frames it travels in.
    fn about_programs(&self, message: &[u8]) -> Vec<u8> {
        self.on(rail::CHANNEL, message)
    }

    /// What to send to hide the language bar that Windows floats over a program's
    /// windows, where it covers what is under it: the page says which language the
    /// session types in, beside the windows. What is said of a window the client hid is
    /// left out of the changes, and one that was told of before it was known as the bar
    /// is gone from them.
    fn language_bar_put_away(&mut self, windows: &mut Vec<Change>) -> Vec<u8> {
        let mut send = Vec::new();
        let mut put_away = Vec::new();
        for change in windows.iter() {
            if let Change::Window(window) = change {
                let is_the_bar = window.title.as_deref() == Some(FLOATING_LANGUAGE_BAR);
                if is_the_bar && matches!(self.state, State::Open) && !self.hidden.contains(&window.id) {
                    send.extend(self.about_programs(&rail::cloak(window.id)));
                    self.hidden.push(window.id);
                    put_away.push(window.id);
                }
            }
        }
        if self.hidden.is_empty() {
            return send;
        }
        let hidden = &self.hidden;
        let mut closed = Vec::new();
        windows.retain_mut(|change| match change {
            Change::Window(Window { id, .. }) | Change::Icon { id, .. } | Change::KeptIcon { id, .. } | Change::InFront(id) => {
                !hidden.contains(id)
            }
            Change::Gone(id) => {
                closed.push(*id);
                !hidden.contains(id)
            }
            Change::Order(order) => {
                order.retain(|id| !hidden.contains(id));
                true
            }
        });
        windows.extend(put_away.into_iter().map(Change::Gone));
        self.hidden.retain(|id| !closed.contains(id));
        send
    }

    /// What the reader answers, as messages about devices. An answer that is longer
    /// than the machine has room for is not sent: the machine is told so, and asks again.
    fn answers(answered: Vec<smartcard::Answered>, room: Option<u32>) -> Vec<Vec<u8>> {
        let answer = |answered: smartcard::Answered| match answered.gives {
            Some(gives) if room.is_some_and(|room| gives.len() > room as usize) => devices::no_room(devices::READER, answered.request),
            Some(gives) => devices::answer(devices::READER, answered.request, &gives),
            None => devices::did_not_work(devices::READER, answered.request),
        };
        answered.into_iter().map(answer).collect()
    }

    /// What the machine says about the clipboard: the client answers it, and what was
    /// copied on the machine is for the person.
    fn on_the_clipboards_channel(&mut self, piece: &[u8]) -> Result<(Vec<u8>, Vec<Happened>), Error> {
        let Some((pieces, clipboard)) = self.clipboard.as_mut() else { return Ok((Vec::new(), Vec::new())) };
        let Some(message) = pieces.take(piece)? else { return Ok((Vec::new(), Vec::new())) };
        let (answers, copied) = clipboard.receive(&message)?;
        let send = answers.iter().flat_map(|answer| self.on(clipboard::CHANNEL, answer)).collect();
        Ok((send, copied.map(Happened::Copied).into_iter().collect()))
    }

    /// The person copied this text. Gives what to send to tell the machine so: nothing
    /// where the clipboard doesn't pass to the machine, and nothing before the machine
    /// is ready for it, which it is told then.
    pub fn copied(&mut self, text: &str) -> Vec<u8> {
        let told = match self.clipboard.as_mut() {
            Some((_, clipboard)) => clipboard.copied(text),
            None => Vec::new(),
        };
        told.iter().flat_map(|message| self.on(clipboard::CHANNEL, message)).collect()
    }

    /// What the machine says of the channels it opens later: the client answers it.
    fn on_the_later_channels(&mut self, piece: &[u8]) -> Result<(Vec<u8>, Vec<Happened>), Error> {
        let Some((pieces, later)) = self.later.as_mut() else { return Ok((Vec::new(), Vec::new())) };
        let Some(message) = pieces.take(piece)? else { return Ok((Vec::new(), Vec::new())) };
        let answers = later.receive(&message)?;
        Ok((answers.iter().flat_map(|answer| self.on(dynamic::CHANNEL, answer)).collect(), Vec::new()))
    }

    /// Whether the machine follows the client's screen as it changes.
    pub fn follows(&self) -> bool {
        self.later.as_ref().is_some_and(|(_, later)| later.follows())
    }

    /// What to send to tell the machine that the client's screen is now this: so wide
    /// and so high, in dots, with the machine drawing this much larger than usual, in
    /// percent. The machine then offers its desktop anew, at that size. Nothing where
    /// the machine doesn't follow the screen, where it takes no screen like this one,
    /// and before the connection is open.
    pub fn screen(&self, width: u16, height: u16, scale: u32) -> Vec<u8> {
        let said = match (&self.state, &self.later) {
            (State::Open, Some((_, later))) => later.screen(width, height, scale),
            _ => None,
        };
        said.map(|said| self.on(dynamic::CHANNEL, &said)).unwrap_or_default()
    }

    fn on_the_devices_channel(&mut self, piece: &[u8]) -> Result<(Vec<u8>, Vec<Happened>), Error> {
        let Some(devices) = self.devices.as_mut() else { return Ok((Vec::new(), Vec::new())) };
        let Some(message) = devices.pieces.take(piece)? else { return Ok((Vec::new(), Vec::new())) };
        let (mut says, mut happened) = (Vec::new(), Vec::new());
        match devices::read(&message)? {
            // the machine begins, or begins again on another session: everything of
            // before is forgotten, and the client says who it is
            devices::Said::Begins { client, .. } => {
                devices.reader.forget();
                (devices.can_do, devices.confirmed, devices.named) = (false, false, false);
                says.extend([devices::begins(client), devices::name(&self.client.name)]);
            }
            devices::Said::CanDo => {
                devices.can_do = true;
                says.push(devices::can_do());
            }
            devices::Said::Confirms => devices.confirmed = true,
            devices::Said::Took { device: devices::READER, result } => happened.push(Happened::Reader { taken: result == 0 }),
            devices::Said::Asks(asked) if asked.device == devices::READER => match asked.what {
                devices::What::Control { code, room, carried } => {
                    let (answered, what) = devices.reader.asked(asked.request, code, &carried);
                    // the room is that of this request; what waited and is answered with it had its own
                    says.extend(Self::answers(answered, Some(room)));
                    happened.push(Happened::AskedTheReader(what));
                }
                devices::What::Other(_) => says.push(devices::did_not_work(asked.device, asked.request)),
            },
            // a device the client never named, the person being signed in, and what is not Away's
            devices::Said::Asks(_) | devices::Said::Took { .. } | devices::Said::SignedIn | devices::Said::Other(_) => {}
        }
        // the reader is named once the machine said what it can do and took the client's number
        if devices.can_do && devices.confirmed && !devices.named {
            devices.named = true;
            says.push(devices::reader());
        }
        Ok((says.iter().flat_map(|message| self.on(devices::CHANNEL, message)).collect(), happened))
    }

    /// Time went by. Gives what to send: the answers to what the machine asked the
    /// reader and waited for no longer than this.
    pub fn elapsed(&mut self, milliseconds: u64) -> Vec<u8> {
        let Some(devices) = self.devices.as_mut() else { return Vec::new() };
        let says = Self::answers(devices.reader.elapsed(milliseconds), None);
        says.iter().flat_map(|message| self.on(devices::CHANNEL, message)).collect()
    }

    fn on_the_programs_channel(&mut self, piece: &[u8]) -> Result<(Vec<u8>, Vec<Happened>), Error> {
        let Some(message) = self.pieces.take(piece)? else { return Ok((Vec::new(), Vec::new())) };
        Ok(match rail::read(&message)? {
            // the machine is ready: the client is too, says what it is and how much room
            // the windows have, and asks for the program
            rail::Said::Ready => {
                let (width, height) = match self.size {
                    (0, _) | (_, 0) => (self.client.width, self.client.height),
                    size => size,
                };
                let mut says = vec![rail::ready(), rail::client(), rail::work_area(width, height)];
                if let Some(program) = &self.client.program {
                    says.push(rail::start(program)?);
                }
                (says.iter().flat_map(|message| self.about_programs(message)).collect(), vec![Happened::AskedForTheProgram])
            }
            rail::Said::Started => (Vec::new(), vec![Happened::Started]),
            rail::Said::NotStarted { result, raw } => (Vec::new(), vec![Happened::NotStarted { result, raw }]),
            rail::Said::Other(_) => (Vec::new(), Vec::new()),
        })
    }

    fn on_the_desktops_channel(&mut self, arrived: &[u8]) -> Result<(Vec<u8>, Vec<Happened>), Error> {
        if matches!(self.state, State::AwaitsLicence) {
            const OUT_OF_ORDER: Error = Error::NotRdp("the machine's words on a licence come in no order they have");
            let to_the_machine = |answer: &[u8]| tpkt::data_frame(&mcs::send(self.user, self.machine.io_channel, answer));
            return match licence::read(arrived)? {
                Said::Valid => {
                    self.state = State::AwaitsWhatItCanDo;
                    Ok((Vec::new(), vec![Happened::Licensed]))
                }
                Said::Asks(asked) => {
                    // what a licence is taken with is used here, once, and kept no longer
                    let new = self.for_a_licence.take().ok_or(OUT_OF_ORDER)?;
                    let (taking, answer) = Taking::begin(&asked, new, &self.name, &self.client.name)?;
                    let send = to_the_machine(&answer);
                    self.taking = Some(taking);
                    Ok((send, vec![Happened::AsksToTakeALicence { company: asked.company, product: asked.product }]))
                }
                Said::Challenges(challenge, check) => {
                    let answer = self.taking.as_ref().ok_or(OUT_OF_ORDER)?.answer(&challenge, &check)?;
                    Ok((to_the_machine(&answer), Vec::new()))
                }
                Said::Hands(sealed, check) => {
                    let taken = self.taking.take().ok_or(OUT_OF_ORDER)?.open(&sealed, &check)?;
                    self.state = State::AwaitsWhatItCanDo;
                    Ok((Vec::new(), vec![Happened::LicenceTaken(taken)]))
                }
                Said::Refuses(why) => Err(Error::NoLicence(why)),
            };
        }
        let mut send = Vec::new();
        let mut happened = Vec::new();
        for message in pdu::read_from_machine(arrived)? {
            match message {
                FromMachine::CanDo { share_id, width, height, bits } => {
                    // a Windows Server offers 16 bits a pixel unless it is told otherwise, a workstation 32
                    if !matches!(bits, 16 | 32) {
                        return Err(Error::NotYet("offers its desktop at 8, 15 or 24 bits a pixel"));
                    }
                    self.share_id = share_id;
                    self.size = (width, height);
                    let for_a_program = self.client.program.is_some();
                    let mut answers = vec![pdu::confirm(self.user, share_id, width, height, bits, self.client.layout, for_a_program)];
                    answers.extend(pdu::finish(self.user, share_id));
                    for answer in answers {
                        send.extend(tpkt::data_frame(&mcs::send(self.user, self.machine.io_channel, &answer)));
                    }
                    self.state = State::Finishes;
                    happened.push(Happened::Desktop { width, height, bits });
                }
                FromMachine::Again => self.state = State::AwaitsWhatItCanDo,
                FromMachine::SentOn(packet) => happened.push(Happened::SentOn(redirection::read(packet)?)),
                // the machine names the fonts: the last step, after which it draws
                FromMachine::Data(0x28, _) => {
                    if matches!(self.state, State::Finishes) {
                        self.state = State::Open;
                        happened.push(Happened::Open);
                    }
                }
                FromMachine::Data(0x02, drawn) => {
                    // an update of the long kind: only rectangles are read
                    if drawn.starts_with(&[1, 0]) {
                        happened.push(Happened::Drawn(update::read_rectangles(drawn)?));
                    } else if drawn.starts_with(&[0, 0]) {
                        // instructions, of which Away takes the words on windows alone
                        let mut windows = update::read_windows(drawn)?;
                        send.extend(self.language_bar_put_away(&mut windows));
                        if !windows.is_empty() {
                            happened.push(Happened::Windows(windows));
                        }
                    }
                }
                // why the machine is about to end the connection
                FromMachine::Data(0x2f, reason) => {
                    if let Some(reason) = reason.get(..4) {
                        self.reason = u32::from_le_bytes([reason[0], reason[1], reason[2], reason[3]]);
                    }
                }
                FromMachine::Data(..) | FromMachine::Other => {}
            }
        }
        Ok((send, happened))
    }

    /// What to send to tell the machine of keys and of the pointer. Nothing before the
    /// connection is open: a machine takes none until it draws.
    pub fn input(&self, events: &[Input]) -> Vec<u8> {
        match self.state {
            State::Open => input::write(events),
            _ => Vec::new(),
        }
    }

    /// What to send to tell the machine that a window is the one the person works in
    /// now, or no longer is. Nothing where the connection is for no program.
    pub fn activate(&self, window: u32, active: bool) -> Vec<u8> {
        match self.state {
            State::Open => self.about_programs(&rail::activate(window, active)),
            _ => Vec::new(),
        }
    }

    /// What to send to ask the machine for another program, in the session that runs
    /// one already. Nothing where the connection is for no program, or not open.
    pub fn start(&self, program: &rail::Program) -> Result<Vec<u8>, Error> {
        match self.state {
            State::Open if self.client.program.is_some() => Ok(self.about_programs(&rail::start(program)?)),
            _ => Ok(Vec::new()),
        }
    }

    /// What to send to tell the machine what the person does with a window as a whole:
    /// make it small, make it large, bring it back, close it.
    pub fn command(&self, window: u32, command: Command) -> Vec<u8> {
        match self.state {
            State::Open => self.about_programs(&rail::command(window, command)),
            _ => Vec::new(),
        }
    }

    /// What to send to leave. The machine keeps what runs on the desktop, for the account
    /// to come back to.
    pub fn leave(&self) -> Vec<u8> {
        tpkt::data_frame(&mcs::leave())
    }

    /// What the client said of itself.
    pub fn client(&self) -> &Client {
        &self.client
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::picture::Picture;
    use crate::rail::stand_in::Asked;
    use crate::rail::Program;
    use crate::stand_in::desktop::{program_icon, Desktop, LANGUAGE_BAR, WINDOW, WINDOW_AT, WINDOW_SIZE};
    use crate::tpkt::Frames;
    use crate::window::{Part, Shown, Window};

    /// A picture whose every pixel says where it is.
    fn picture() -> Picture {
        let mut picture = Picture::new(200, 120);
        for row in 0..120 {
            for column in 0..200 {
                picture.pixels[(row * 200 + column) * 4..][..3].copy_from_slice(&[column as u8, row as u8, 0x7f]);
            }
        }
        picture
    }

    /// Runs a connection against a stand-in for a machine until neither side has
    /// anything to say, and gives what happened, the connection and the machine.
    fn run(program: Option<&str>, machine: Desktop) -> (Result<Vec<Happened>, Error>, Connection, Desktop) {
        let program = program.map(|run| Program { run: run.to_string(), arguments: String::new() });
        let client = Client {
            width: 1280,
            height: 800,
            scale: 100,
            security: 2,
            name: "away".to_string(),
            layout: gcc::LAYOUT_US,
            sent_on_to: None,
            program,
            clipboard: clipboard::Passes::NEITHER,
        };
        let for_a_licence = licence::New { random: [1; 32], secret: [2; 48], known_by: [3; 16] };
        let (mut connection, first) = Connection::begin(client, "CORP", "alex.urban", None, for_a_licence, None);
        let mut machine = machine;
        let happened = talk(&mut connection, &mut machine, first);
        (happened, connection, machine)
    }

    #[test]
    fn the_machine_reaches_the_clients_reader_and_gives_the_card_in_it_commands() {
        use crate::smartcard::stand_in::Echo;
        use crate::smartcard::Asked;
        let client = Client {
            width: 1280,
            height: 800,
            scale: 100,
            security: 2,
            name: "away".to_string(),
            layout: gcc::LAYOUT_US,
            sent_on_to: None,
            program: None,
            clipboard: clipboard::Passes::NEITHER,
        };
        let for_a_licence = licence::New { random: [1; 32], secret: [2; 48], known_by: [3; 16] };
        let reader = Reader::new("Away 0", Box::new(Echo));
        let (mut connection, first) = Connection::begin(client, "CORP", "alex.urban", None, for_a_licence, Some(reader));
        // a long command travels in more than one piece, both ways
        let long: Vec<u8> = (0..1800).map(|at| at as u8).collect();
        let mut machine = Desktop::new(picture()).that_reads_a_card(&[vec![0x00, 0xa4, 0x04, 0x00], long.clone()]);
        let happened = talk(&mut connection, &mut machine, first).expect("the connection opens");

        // the client asked for the channel of devices and for the one of sound, named one reader, and the machine took it
        let seen = machine.seen();
        assert_eq!(seen.channels, ["drdynvc", "rdpdr", "rdpsnd"]);
        assert_eq!(seen.devices, [(0x20, devices::READER, "SCARD".to_string())]);
        assert!(happened.contains(&Happened::Reader { taken: true }));
        // the machine found the reader by its name, read what the card says of itself, and got an answer to each command
        let card = seen.card.as_ref().expect("the machine read the card");
        assert_eq!((card.reader.as_str(), &card.says[..]), ("Away 0", &[0x3b, 0x80, 0x80, 0x01, 0x01][..]));
        assert_eq!(card.answers.len(), 2);
        assert_eq!(card.answers[0], [0x00, 0xa4, 0x04, 0x00, 0x90, 0x00]);
        assert!(card.answers[1] == [&long[..], &[0x90, 0x00]].concat(), "the long command came back whole");
        assert!(card.closed, "it went through all of it");
        // and what it asked is told, in its order
        let asked: Vec<Asked> = happened
            .iter()
            .filter_map(|happened| match happened {
                Happened::AskedTheReader(asked) => Some(*asked),
                _ => None,
            })
            .collect();
        use Asked::{ACommand, About, ForACard, Readers, TheCard, ToSpeak, WhetherThere};
        assert_eq!(
            asked,
            [WhetherThere, ToSpeak, Readers, ForACard, TheCard, About, TheCard, ACommand, ACommand, TheCard, TheCard, ToSpeak]
        );
        // the desktop is drawn as it is without a reader
        assert!(happened.iter().any(|happened| matches!(happened, Happened::Drawn(_))));
    }

    #[test]
    fn without_a_reader_the_client_asks_for_no_channel_of_devices_and_names_no_device() {
        // without a reader, the client asks for the channel of the ones that are opened
        // later and for no other, and the machine says nothing about devices
        let (happened, _, machine) = run(None, Desktop::new(picture()));
        assert!(machine.seen().channels == ["drdynvc"] && machine.seen().devices.is_empty());
        assert!(!happened.expect("the connection opens").iter().any(|happened| matches!(happened, Happened::Reader { .. })));
    }

    #[test]
    fn a_machine_whose_desktop_was_opened_elsewhere_says_so_as_it_ends_the_connection() {
        let (happened, mut connection, machine) = run(None, Desktop::new(picture()));
        happened.expect("the connection opens");
        let mut frames = Frames::new();
        frames.push(&machine.taken_elsewhere());
        let mut ended = None;
        while let Some(frame) = frames.next_of_either_kind().expect("the machine's frames") {
            if let Err(error) = connection.receive(frame) {
                ended = Some(error);
            }
        }
        // the reason is the machine's own: another connection took the session
        assert_eq!(ended, Some(Error::Ended(5)));
    }

    #[test]
    fn text_passes_between_the_persons_clipboard_and_the_machines() {
        use crate::stand_in::desktop::COPIED;
        let client = Client {
            width: 1280,
            height: 800,
            scale: 100,
            security: 2,
            name: "away".to_string(),
            layout: gcc::LAYOUT_US,
            sent_on_to: None,
            program: None,
            clipboard: clipboard::Passes::BOTH,
        };
        let for_a_licence = licence::New { random: [1; 32], secret: [2; 48], known_by: [3; 16] };
        let (mut connection, first) = Connection::begin(client, "CORP", "alex.urban", None, for_a_licence, None);
        let mut machine = Desktop::new(picture());
        // the key C copies on the machine: the client asks for the text at once, and it is for the person
        let key_c = [Input::Key { code: 0x2e, extended: false, down: true }];
        let happened = talk(&mut connection, &mut machine, first).expect("the connection opens");
        assert_eq!(machine.seen().channels, ["drdynvc", "cliprdr"]);
        assert!(machine.seen().pasted.is_empty() && !happened.iter().any(|happened| matches!(happened, Happened::Copied(_))));
        let pressed = connection.input(&key_c);
        let happened = talk(&mut connection, &mut machine, pressed).expect("the machine copies");
        assert!(happened.contains(&Happened::Copied(COPIED.to_string())));

        // the person copies: the machine is told, pastes, and is handed the text
        let told = connection.copied("from the person's device\nin two lines");
        assert!(!told.is_empty());
        let happened = talk(&mut connection, &mut machine, told).expect("the machine pastes");
        assert_eq!(machine.seen().pasted.last().map(String::as_str), Some("from the person's device\nin two lines"));
        assert!(!happened.iter().any(|happened| matches!(happened, Happened::Copied(_))));

        // the stand-in copies what it was handed, in capitals: that comes back to the person
        let pressed = connection.input(&key_c);
        let happened = talk(&mut connection, &mut machine, pressed).expect("the machine copies");
        assert!(happened.contains(&Happened::Copied("FROM THE PERSON'S DEVICE\nIN TWO LINES".to_string())));

        // what the person copied before the machine was ready is told once it is
        let client = Client { clipboard: clipboard::Passes::BOTH, ..connection.client().clone() };
        let for_a_licence = licence::New { random: [1; 32], secret: [2; 48], known_by: [3; 16] };
        let (mut early, first) = Connection::begin(client, "CORP", "alex.urban", None, for_a_licence, None);
        assert!(early.copied("copied before the desktop opened").is_empty());
        let mut machine = Desktop::new(picture());
        talk(&mut early, &mut machine, first).expect("the connection opens");
        assert_eq!(machine.seen().pasted, ["copied before the desktop opened"]);

        // where the clipboard passes neither way, no channel is asked for and nothing is said
        let (_, mut without, machine) = run(None, Desktop::new(picture()));
        assert_eq!(machine.seen().channels, ["drdynvc"]);
        assert!(without.copied("nothing passes").is_empty());
    }

    #[test]
    fn the_machine_offers_its_desktop_anew_at_the_size_the_clients_screen_has_now() {
        let (happened, mut connection, mut machine) = run(None, Desktop::new(picture()));
        let happened = happened.expect("the connection opens");
        assert_eq!(happened.iter().filter(|happened| matches!(happened, Happened::Open)).count(), 1);
        // the machine opened the channel of the screen and one that is not Away's: the
        // client took the first, and the machine said what it takes
        assert_eq!(machine.seen().took_later, [3]);
        assert!(connection.follows());

        // a screen no machine takes is not said; one it takes is, and the machine offers
        // its desktop at that size, as it did when the connection opened
        assert!(connection.screen(100, 100, 100).is_empty());
        let said = connection.screen(640, 480, 150);
        assert!(!said.is_empty());
        let happened = talk(&mut connection, &mut machine, said).expect("the desktop is offered anew");
        assert_eq!(machine.seen().screens, [(640, 480, 150)]);
        assert!(happened.contains(&Happened::Desktop { width: 640, height: 480, bits: 32 }));
        assert_eq!(happened.iter().filter(|happened| matches!(happened, Happened::Open)).count(), 1);
        // and draws it anew: every pixel of the new picture arrives
        let mut drawn = Picture::new(640, 480);
        for happened in &happened {
            if let Happened::Drawn(rectangles) = happened {
                rectangles.iter().for_each(|rectangle| drawn.draw(rectangle).expect("a rectangle of the picture"));
            }
        }
        assert!(drawn == *machine.picture(), "the picture is the machine's new one");

        // a connection for a program asks for no such channel, and says nothing of a screen
        let (_, program, _) = run(Some("||ledger"), Desktop::new(picture()).that_publishes(&["||ledger"]));
        assert!(!program.follows() && program.screen(640, 480, 100).is_empty());
    }

    /// Passes what one side says to the other, and back, until both are silent.
    fn talk(connection: &mut Connection, machine: &mut Desktop, mut to_machine: Vec<u8>) -> Result<Vec<Happened>, Error> {
        let mut happened = Vec::new();
        let (mut at_machine, mut at_client) = (Frames::new(), Frames::new());
        while !to_machine.is_empty() {
            at_machine.push(&to_machine);
            to_machine.clear();
            while let Some(frame) = at_machine.next_of_either_kind().expect("the client's frames") {
                at_client.push(&match frame {
                    Frame::Slow(carried) => machine.answer(&carried).expect("the machine takes what the client says"),
                    Frame::Fast(head, carried) => machine.input(head, &carried).expect("the machine takes keys and the pointer"),
                });
            }
            while let Some(frame) = at_client.next_of_either_kind().expect("the machine's frames") {
                let (send, more) = connection.receive(frame)?;
                to_machine.extend(send);
                happened.extend(more);
            }
        }
        Ok(happened)
    }

    #[test]
    fn a_program_is_asked_for_starts_and_its_window_is_told_of_before_what_is_drawn_in_it() {
        let (happened, connection, machine) = run(Some("||ledger"), Desktop::new(picture()).that_publishes(&["||ledger"]));
        let happened = happened.expect("the connection opens");
        // the machine gave a channel for programs beside the desktop's, and no desktop is shown
        let kinds: Vec<&str> = happened
            .iter()
            .map(|happened| match happened {
                Happened::Channels { .. } => "channels",
                Happened::Licensed => "licensed",
                Happened::Desktop { .. } => "desktop",
                Happened::Open => "open",
                Happened::AskedForTheProgram => "asked",
                Happened::Started => "started",
                Happened::Windows(_) => "windows",
                Happened::Drawn(_) => "drawn",
                _ => "other",
            })
            .collect();
        assert_eq!(&kinds[..7], ["channels", "licensed", "desktop", "open", "asked", "started", "windows"]);
        assert!(kinds[7..].iter().all(|kind| *kind == "drawn") && kinds.len() > 7);
        let windows = happened.iter().find_map(|happened| if let Happened::Windows(windows) = happened { Some(windows) } else { None });
        assert_eq!(
            windows.unwrap(),
            &[
                Change::Window(Window {
                    id: WINDOW,
                    is_new: true,
                    owner: None,
                    style: Some((0x14cf_0000, 0x100)),
                    shown: Some(Shown::AsItIs),
                    title: Some("ledger".to_string()),
                    at: Some((20, 10)),
                    size: Some((160, 100)),
                    seen_at: Some((20, 10)),
                    seen: Some(vec![Part { left: 0, top: 0, right: 160, bottom: 100 }]),
                }),
                Change::Icon { id: WINDOW, large: true, icon: program_icon(), kept: Some((0, 0)) },
                Change::InFront(WINDOW),
                Change::Order(vec![WINDOW]),
            ]
        );
        // what is drawn is the window and nothing beside it
        let drawn: Vec<&Rectangle> = happened
            .iter()
            .filter_map(|happened| if let Happened::Drawn(drawn) = happened { Some(drawn) } else { None })
            .flatten()
            .collect();
        let (left, top) = (WINDOW_AT.0 as u16, WINDOW_AT.1 as u16);
        let (right, bottom) = (left + WINDOW_SIZE.0 as u16 - 1, top + WINDOW_SIZE.1 as u16 - 1);
        assert!(drawn.iter().all(|drawn| drawn.left >= left && drawn.top >= top && drawn.right <= right && drawn.bottom <= bottom));
        assert_eq!(
            drawn.iter().map(|drawn| u32::from(drawn.right - drawn.left + 1) * u32::from(drawn.bottom - drawn.top + 1)).sum::<u32>(),
            160 * 100
        );

        // the machine was told that the connection is for a program, and then, in this
        // order: that the client is ready, what it is, how much room there is, the program
        let seen = machine.seen();
        assert!(seen.for_a_program);
        assert_eq!(
            seen.about_programs,
            [
                Asked::Ready,
                Asked::Client(0x200),
                Asked::WorkArea(200, 120),
                Asked::Start(Program { run: "||ledger".to_string(), arguments: String::new() }),
            ]
        );

        // the person works in the window, and then closes it: the machine says it is gone
        let mut machine = machine;
        let mut connection = connection;
        let says = [connection.activate(WINDOW, true), connection.command(WINDOW, Command::Close)].concat();
        let after = talk(&mut connection, &mut machine, says).unwrap();
        assert_eq!(after, [Happened::Windows(vec![Change::Gone(WINDOW)])]);
        assert_eq!(machine.seen().about_programs[4..], [Asked::Activate(WINDOW, true), Asked::Command(WINDOW, Command::Close)]);
    }

    #[test]
    fn another_program_starts_in_the_same_session_and_the_floating_language_bar_is_hidden() {
        let machine = Desktop::new(picture()).that_publishes(&["||ledger"]).that_floats_its_language_bar();
        let (happened, mut connection, mut machine) = run(Some("||ledger"), machine);
        let happened = happened.expect("the connection opens");
        // the language bar that opened with the first program was hidden, and only it; of
        // its window nothing is told
        assert_eq!(machine.seen().about_programs[4..], [Asked::Cloak(LANGUAGE_BAR)]);
        let told: Vec<&Change> = happened
            .iter()
            .filter_map(|happened| if let Happened::Windows(changes) = happened { Some(changes) } else { None })
            .flatten()
            .collect();
        assert!(told.iter().any(|change| matches!(change, Change::Window(window) if window.id == WINDOW)));
        assert!(!told.iter().any(|change| matches!(change, Change::Window(window) if window.id == LANGUAGE_BAR)));

        // another program, asked for on the same connection, opens a window of its own in front
        let books = Program { run: "||ledger".to_string(), arguments: String::new() };
        let said = connection.start(&books).expect("a program's name");
        let after = talk(&mut connection, &mut machine, said).expect("the program starts");
        assert_eq!(after[0], Happened::Started);
        let windows = after.iter().find_map(|happened| if let Happened::Windows(windows) = happened { Some(windows) } else { None });
        assert!(windows.unwrap().contains(&Change::Order(vec![WINDOW + 1, WINDOW])));
        // one the machine doesn't publish is not started, and the session goes on
        let payroll = Program { run: "||payroll".to_string(), arguments: String::new() };
        let said = connection.start(&payroll).expect("a program's name");
        assert_eq!(talk(&mut connection, &mut machine, said), Ok(vec![Happened::NotStarted { result: 3, raw: 2 }]));
        assert!(!connection.activate(WINDOW, true).is_empty());

        // a connection for the desktop starts no program
        let (_, desktop, _) = run(None, Desktop::new(picture()).that_publishes(&["||ledger"]));
        assert_eq!(desktop.start(&books), Ok(Vec::new()));
    }

    #[test]
    fn a_program_the_machine_does_not_publish_is_not_started_and_a_desktop_asks_for_no_program() {
        let (happened, _, machine) = run(Some("||payroll"), Desktop::new(picture()).that_publishes(&["||ledger"]));
        let happened = happened.unwrap();
        assert_eq!(happened.last(), Some(&Happened::NotStarted { result: 3, raw: 2 }));
        assert!(!happened.iter().any(|happened| matches!(happened, Happened::Windows(_) | Happened::Drawn(_))));
        assert_eq!(machine.seen().about_programs.len(), 4);

        // a connection for the desktop asks for no channel, says nothing of programs, and is shown the desktop
        let (happened, connection, machine) = run(None, Desktop::new(picture()).that_publishes(&["||ledger"]));
        let happened = happened.unwrap();
        assert!(happened.iter().any(|happened| matches!(happened, Happened::Drawn(_))));
        assert!(!happened.iter().any(|happened| matches!(happened, Happened::AskedForTheProgram | Happened::Windows(_))));
        assert!(!machine.seen().for_a_program && machine.seen().about_programs.is_empty());
        assert!(connection.activate(WINDOW, true).is_empty() && connection.command(WINDOW, Command::Close).is_empty());
    }
}
