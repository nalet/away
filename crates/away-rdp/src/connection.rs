//! A connection, from what the client says of itself to the machine drawing: the steps
//! in their order, each answered with what to send next (MS-RDPBCGR 1.3.1.1).
//!
//! It begins once the connection is secured and, where the machine wanted it, the person
//! is signed in. It takes the frames the machine sends and gives bytes to send and what
//! happened. It holds no connection itself.

use crate::channel::{self, Pieces};
use crate::gcc::{self, Client, Machine};
use crate::input::{self, Input};
use crate::licence::{self, Said, Taking};
use crate::mcs::{self, Arrived};
use crate::pdu::{self, FromMachine};
use crate::rail::{self, Command};
use crate::redirection::{self, Redirection};
use crate::secret::Secret;
use crate::tpkt::{self, Frame};
use crate::update::{self, Rectangle, Updates};
use crate::window::Change;
use crate::Error;

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
}

impl Connection {
    /// Begins: what the client says of itself, to send first. The domain and the name are
    /// of the account the desktop is for. The password is given only where the machine
    /// signs the account in itself: where the account signed in before the desktop, the
    /// machine has it already. What a licence is taken with is used only where the
    /// machine hands out licences.
    pub fn begin(client: Client, domain: &str, name: &str, password: Option<Secret>, for_a_licence: licence::New) -> (Connection, Vec<u8>) {
        let first = tpkt::data_frame(&mcs::connect(&gcc::request(&client)));
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
                        let windows = self.updates.take_windows();
                        let mut happened = Vec::new();
                        // where the windows are comes before what is drawn in them
                        if !windows.is_empty() {
                            happened.push(Happened::Windows(windows));
                        }
                        if !drawn.is_empty() {
                            happened.push(Happened::Drawn(drawn));
                        }
                        Ok((Vec::new(), happened))
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
                if self.client.program.is_some() && self.machine.channels.is_empty() {
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
                Arrived::On(channel, arrived) if Some(channel) == self.programs_channel() => self.on_the_programs_channel(arrived),
                Arrived::On(..) => Ok((Vec::new(), Vec::new())),
            },
        }
    }

    /// The channel programs are started over, where this connection is for a program.
    fn programs_channel(&self) -> Option<u16> {
        self.client.program.as_ref().and_then(|_| self.machine.channels.first().copied())
    }

    /// A message about programs, in the frames it travels in.
    fn about_programs(&self, message: &[u8]) -> Vec<u8> {
        let Some(on) = self.programs_channel() else { return Vec::new() };
        channel::pieces(message).iter().flat_map(|piece| tpkt::data_frame(&mcs::send(self.user, on, piece))).collect()
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
                        let windows = update::read_windows(drawn)?;
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
    use crate::stand_in::desktop::{Desktop, WINDOW, WINDOW_AT, WINDOW_SIZE};
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
        let client =
            Client { width: 1280, height: 800, security: 2, name: "away".to_string(), layout: gcc::LAYOUT_US, sent_on_to: None, program };
        let for_a_licence = licence::New { random: [1; 32], secret: [2; 48], known_by: [3; 16] };
        let (mut connection, first) = Connection::begin(client, "CORP", "alex.urban", None, for_a_licence);
        let mut machine = machine;
        let happened = talk(&mut connection, &mut machine, first);
        (happened, connection, machine)
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
                Asked::Client(0x400),
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
