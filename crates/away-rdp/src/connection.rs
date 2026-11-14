//! A connection, from what the client says of itself to the machine drawing: the steps
//! in their order, each answered with what to send next (MS-RDPBCGR 1.3.1.1).
//!
//! It begins once the connection is secured and, where the machine wanted it, the person
//! is signed in. It takes the frames the machine sends and gives bytes to send and what
//! happened. It holds no connection itself.

use crate::gcc::{self, Client, Machine};
use crate::input::{self, Input};
use crate::licence::{self, Said, Taking};
use crate::mcs::{self, Arrived};
use crate::pdu::{self, FromMachine};
use crate::secret::Secret;
use crate::tpkt::{self, Frame};
use crate::update::{self, Rectangle, Updates};
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
                        Ok((Vec::new(), if drawn.is_empty() { Vec::new() } else { vec![Happened::Drawn(drawn)] }))
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
                    let info = pdu::client_info(&self.domain, &self.name, self.password.take().as_ref());
                    let send = tpkt::data_frame(&mcs::send(self.user, self.machine.io_channel, &info));
                    return Ok((send, vec![Happened::Channels { user: self.user, desktop: self.machine.io_channel }]));
                }
                let next = left.remove(0);
                Ok((tpkt::data_frame(&mcs::join(self.user, next)), Vec::new()))
            }
            _ => match mcs::read_arrived(message)? {
                Arrived::Ending => Err(Error::Ended(self.reason)),
                Arrived::On(channel, arrived) if channel == self.machine.io_channel => self.on_the_desktops_channel(arrived),
                Arrived::On(..) => Ok((Vec::new(), Vec::new())),
            },
        }
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
                    let mut answers = vec![pdu::confirm(self.user, share_id, width, height, bits, self.client.layout)];
                    answers.extend(pdu::finish(self.user, share_id));
                    for answer in answers {
                        send.extend(tpkt::data_frame(&mcs::send(self.user, self.machine.io_channel, &answer)));
                    }
                    self.state = State::Finishes;
                    happened.push(Happened::Desktop { width, height, bits });
                }
                FromMachine::Again => self.state = State::AwaitsWhatItCanDo,
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
