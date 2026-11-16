//! The machine's side of a connection once the person is signed in, for tests: it gives
//! the channels, takes the client without a licence or hands it one, says what it can
//! do, and draws one picture, in each of the ways a machine sends rectangles that Away
//! reads. A client that asks for a program is shown no desktop: the program it asks for
//! opens in a window, of which the stand-in says where it is and draws what is in it.

use crate::channel::{self, Pieces};
use crate::input::{self, Button, Input};
use crate::licence::stand_in::HandsOut;
use crate::picture::Picture;
use crate::rail::stand_in::Asked;
use crate::redirection::{self, Redirection};
use crate::{clipboard, der, devices, dynamic, interleaved, mcs, ntlm, pdu, rail, smartcard, tpkt, window, Error};

const WRONG: Error = Error::NotRdp("the client's message is not built as one");

/// The channel the stand-in gives the client, the one of the desktop, and the number of
/// the sharing.
const USER: u16 = 1004;
const DESKTOP: u16 = 1003;
const SHARING: u32 = 0x0001_03ea;
/// The first of the further channels it gives. A client that asks for the channel of
/// programs asks for it first, so that this is that channel's number then.
const PROGRAMS: u16 = 1005;
/// The number it gives a client for what it says about devices.
const CLIENT: u32 = 0x0000_0d0c;
/// The text the stand-in copies when the key C goes down, where the clipboard passes
/// and it was handed none: what it was last handed it copies in capitals.
pub const COPIED: &str = "copied on the machine\nin two lines";
/// The key C, by its place.
const KEY_C: u8 = 0x2e;

/// The numbers it gives the channels it opens later: the one of the screen, and one
/// that is not Away's.
const SCREEN: u32 = 3;
const NOT_AWAYS: u32 = 4;

/// The window a program opens in on the stand-in: what names it, where it is on the
/// desktop, and how large it is.
pub const WINDOW: u32 = 0x0001_0001;
/// Where the window is.
pub const WINDOW_AT: (usize, usize) = (20, 10);
/// How wide and how high the window is.
pub const WINDOW_SIZE: (usize, usize) = (160, 100);
/// The window of the language bar, which a stand-in that floats one opens with the first
/// program, as Windows does.
pub const LANGUAGE_BAR: u32 = 0x0001_0100;

/// The icon the first program puts in the tray, among those of its window.
pub const TRAY_ICON: u32 = 1;

/// The icon of a program's window on the stand-in: 32 pixels square, blue, in a frame of
/// two pixels that covers nothing.
pub fn program_icon() -> window::Icon {
    let pixels = (0..32 * 32)
        .flat_map(|at| {
            let (x, y) = (at % 32, at / 32);
            match (2..30).contains(&x) && (2..30).contains(&y) {
                true => [0x25, 0x63, 0xeb, 0xff],
                false => [0, 0, 0, 0],
            }
        })
        .collect();
    window::Icon { width: 32, height: 32, pixels }
}

/// The most a piece of an update carries, so that its frame can say how long it is.
const MOST_IN_A_PIECE: usize = 30_000;

/// How wide and how high the rectangles are that the picture is cut into, at most.
const TILE: usize = 64;

/// What the stand-in saw of the client.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Seen {
    /// The size of the desktop the client asked for.
    pub asked: (u16, u16),
    /// How much larger than usual the client asked the machine to draw, in percent.
    pub scale: u32,
    /// The name the client gave itself.
    pub name: String,
    /// The security the client says was chosen.
    pub security: u32,
    /// The domain and the name of the account the desktop is for.
    pub account: (String, String),
    /// The password that travelled with the account's name, where one did.
    pub password: Option<String>,
    /// How many bits a pixel the client takes.
    pub bits: u16,
    /// Whether the client takes drawing from instructions, which Away never does.
    pub takes_instructions: bool,
    /// Whether the client said that it leaves.
    pub left: bool,
    /// What the client took a licence as, where the stand-in handed one out.
    pub licence: Option<crate::licence::stand_in::Seen>,
    /// The keys and the pointer, as the client told of them.
    pub input: Vec<Input>,
    /// The session the client says a farm's broker sent it on to, where it says one.
    pub sent_on_to: Option<u32>,
    /// Whether the client's information says that the connection is for a program.
    pub for_a_program: bool,
    /// What the client said about programs, in the order it said it.
    pub about_programs: Vec<Asked>,
    /// The further channels the client asked for, in the order it asked for them.
    pub channels: Vec<String>,
    /// The devices the client named: the kind, the number and the name of each.
    pub devices: Vec<(u32, u32, String)>,
    /// What the stand-in saw of the card in the client's reader, where it reads cards.
    pub card: Option<Card>,
    /// The texts the client handed over when the stand-in pasted: it pastes as soon as
    /// the client says that it holds text.
    pub pasted: Vec<String>,
    /// The channels it opened later that the client took, by their numbers.
    pub took_later: Vec<u32>,
    /// What the client said its screen is, each time it said so: how wide, how high,
    /// and how much larger than usual the machine is to draw.
    pub screens: Vec<(u16, u16, u32)>,
}

/// What a stand-in that reads the card in the client's reader saw of it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Card {
    /// The name of the reader the card is in.
    pub reader: String,
    /// What the card says of itself.
    pub says: Vec<u8>,
    /// The card's answers to the commands the stand-in gave it, in their order.
    pub answers: Vec<Vec<u8>>,
    /// Whether the stand-in went through all of it, to closing the card.
    pub closed: bool,
}

/// How far a stand-in that reads cards is with the client's reader.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Reading {
    WhetherThere,
    Begins,
    Readers,
    Watches,
    Opens,
    About,
    Holds,
    Commands(usize),
    LetsGo,
    Closes,
    Ends,
    Done,
}

/// The commands a stand-in gives the card, and where it is with them.
struct Reads {
    commands: Vec<Vec<u8>>,
    step: Reading,
    context: u32,
    opened: u32,
    request: u32,
}

/// How wide and how high the mark is that the stand-in draws where a button went down,
/// and in its corner for a key.
pub const MARK: usize = 12;

/// The colour of the mark for the left button: blue, green, red.
pub const CLICKED: [u8; 3] = [0x20, 0x40, 0xe0];

/// The colour of the mark for a key that went down: it says which key it was.
pub fn typed(code: u8) -> [u8; 3] {
    [code, 0xff - code, 0x55]
}

enum State {
    AwaitsClient,
    AwaitsUser,
    AwaitsAccount,
    HandsOutALicence,
    Open,
}

/// What a stand-in does about licences once the client has answered its asking.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Licences {
    /// It challenges the client, and hands a licence over.
    HandedOver,
    /// It takes the client as it is.
    NoneHandedOver,
}

/// A stand-in for a machine's desktop.
pub struct Desktop {
    state: State,
    picture: Picture,
    bits: u16,
    seen: Seen,
    hands_out: Option<(HandsOut, Licences)>,
    /// Where it sends its clients on to, where it stands in for a farm's broker.
    sends_on: Option<Redirection>,
    /// The programs it publishes, by what they are asked for as.
    publishes: Vec<String>,
    /// Whether the client asked for the channel programs are started over.
    programs: bool,
    /// The pieces of a message about programs that is not whole yet.
    pieces: Pieces,
    /// The pieces of a message about devices that is not whole yet.
    device_pieces: Pieces,
    /// What it does with the card in the client's reader, where it reads cards.
    reads: Option<Reads>,
    /// The pieces of a message about the channels it opens later that is not whole yet.
    later_pieces: Pieces,
    /// Whether it began to speak of those channels.
    speaks_of_later: bool,
    /// The pieces of a message about the clipboard that is not whole yet.
    clipboard_pieces: Pieces,
    /// Whether it said that it is ready for the clipboard.
    speaks_of_the_clipboard: bool,
    /// The windows of the programs that are open, the front one first.
    windows: Vec<u32>,
    /// How many windows of programs it opened.
    opened: u32,
    /// Whether it floats a language bar over the programs' windows.
    floats_a_language_bar: bool,
    /// Whether it said that it is ready for programs.
    ready_for_programs: bool,
}

/// The picture a stand-in draws on a desktop whose size the client asked for: every
/// pixel says where it is.
pub fn at_a_size(width: usize, height: usize) -> Picture {
    let mut picture = Picture::new(width, height);
    for (at, pixel) in picture.pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (x, y) = (at % width, at / width);
        pixel[..3].copy_from_slice(&[(x * 7) as u8, (y * 5) as u8, (x + y) as u8]);
    }
    picture
}

impl Desktop {
    /// A stand-in that offers a desktop of the picture's size, and draws the picture at
    /// 32 bits a pixel, as a workstation does.
    pub fn new(picture: Picture) -> Desktop {
        Desktop {
            state: State::AwaitsClient,
            picture,
            bits: 32,
            seen: Seen::default(),
            hands_out: None,
            sends_on: None,
            publishes: Vec::new(),
            programs: false,
            pieces: Pieces::default(),
            device_pieces: Pieces::default(),
            reads: None,
            later_pieces: Pieces::default(),
            speaks_of_later: false,
            clipboard_pieces: Pieces::default(),
            speaks_of_the_clipboard: false,
            windows: Vec::new(),
            opened: 0,
            floats_a_language_bar: false,
            ready_for_programs: false,
        }
    }

    /// The same, as a machine that floats its language bar over the programs' windows,
    /// in a window of its own.
    pub fn that_floats_its_language_bar(self) -> Desktop {
        Desktop { floats_a_language_bar: true, ..self }
    }

    /// The same, as a machine that reads the card in the client's reader the way a
    /// Windows machine does when it signs a person in with one: it finds the reader,
    /// opens the card, has it for itself, gives it these commands, and closes it.
    pub fn that_reads_a_card(self, commands: &[Vec<u8>]) -> Desktop {
        let reads = Reads { commands: commands.to_vec(), step: Reading::WhetherThere, context: 0, opened: 0, request: 100 };
        Desktop { reads: Some(reads), ..self }
    }

    /// The number of a further channel the client asked for by this name.
    fn channel(&self, name: &str) -> Option<u16> {
        self.seen.channels.iter().position(|asked| asked == name).map(|at| PROGRAMS + at as u16)
    }

    /// The same, drawing at 16 bits a pixel, as a Windows Server does unless it is set
    /// otherwise.
    pub fn at_16_bits(self) -> Desktop {
        Desktop { bits: 16, ..self }
    }

    /// The same, as a machine for many people that counts their devices: before it
    /// shows anything it asks the client to take a licence, and hands one over.
    pub fn that_hands_out_licences(self) -> Desktop {
        Desktop { hands_out: Some((HandsOut::new([0x4c; 32]), Licences::HandedOver)), ..self }
    }

    /// The same, as a machine for many people that counts the people: it asks the
    /// client to take a licence, and after the client's answer takes it as it is.
    pub fn that_asks_for_a_licence_and_hands_none_over(self) -> Desktop {
        Desktop { hands_out: Some((HandsOut::new([0x4c; 32]), Licences::NoneHandedOver)), ..self }
    }

    /// The same, as a farm's broker: once the account is signed in it shows nothing, and
    /// sends the client on to the machine that serves it. A client that says it was sent
    /// on already is shown the desktop, as a broker does that serves a collection itself.
    pub fn that_sends_on(self, to: Redirection) -> Desktop {
        Desktop { sends_on: Some(to), ..self }
    }

    /// The same, as a machine that publishes programs: each by what a client asks for it
    /// as. A client that asks for one of them is shown its window, and one that asks for
    /// another is told that the machine doesn't let it be started.
    pub fn that_publishes(self, programs: &[&str]) -> Desktop {
        Desktop { publishes: programs.iter().map(|program| program.to_string()).collect(), ..self }
    }

    /// What it saw of the client so far.
    pub fn seen(&self) -> &Seen {
        &self.seen
    }

    /// Answers what one frame of the client's carries.
    pub fn answer(&mut self, carried: &[u8]) -> Result<Vec<u8>, Error> {
        let message = tpkt::data(carried)?;
        match (&self.state, message) {
            (State::AwaitsClient, [0x7f, 0x65, ..]) => {
                let at = message.windows(4).position(|part| part == b"Duca").ok_or(WRONG)? + 4;
                let blocks = match message.get(at..).ok_or(WRONG)? {
                    [first, _, rest @ ..] if first & 0x80 != 0 => rest,
                    [_, rest @ ..] => rest,
                    _ => return Err(WRONG),
                };
                // the client's own block comes first: as long as it says, and long enough
                // for what every client says
                let length = usize::from(number16(blocks, 2)?);
                let core = blocks.get(..length).filter(|core| core.len() >= 216 && core[..2] == [0x01, 0xc0]).ok_or(WRONG)?;
                self.seen.asked = (number16(core, 8)?, number16(core, 10)?);
                // a client that says nothing of a scale is drawn for at the usual size
                self.seen.scale = match core.get(226..230) {
                    Some(scale) => u32::from_le_bytes([scale[0], scale[1], scale[2], scale[3]]),
                    None => 100,
                };
                self.seen.name = ntlm::utf16(&core[24..56]).trim_end_matches('\0').to_string();
                self.seen.security = u32::from(number16(core, 212)?);
                // what the client says of a farm: whether it was sent on, and to which session
                let mut rest = blocks;
                while let (Ok(kind), Ok(length)) = (number16(rest, 0), number16(rest, 2)) {
                    let block = rest.get(..usize::from(length)).filter(|block| block.len() >= 4).ok_or(WRONG)?;
                    if kind == 0xc004 && block.len() >= 12 && block[4] & 0x02 != 0 {
                        self.seen.sent_on_to = Some(u32::from_le_bytes([block[8], block[9], block[10], block[11]]));
                    }
                    // the further channels it asks for, by their names
                    if kind == 0xc003 {
                        let asked = block.get(8..).ok_or(WRONG)?.as_chunks::<12>().0;
                        let name = |asked: &[u8; 12]| asked[..8].iter().take_while(|byte| **byte != 0).map(|byte| *byte as char).collect();
                        self.seen.channels = asked.iter().map(name).collect();
                        self.programs = self.channel(rail::CHANNEL).is_some();
                    }
                    rest = &rest[block.len()..];
                }
                self.state = State::AwaitsUser;
                Ok(tpkt::data_frame(&connect_answer(self.seen.channels.len())))
            }
            (State::AwaitsUser, [0x04, ..]) => Ok(Vec::new()),
            (State::AwaitsUser, [0x28]) => {
                self.state = State::AwaitsAccount;
                let [high, low] = (USER - 1001).to_be_bytes();
                Ok(tpkt::data_frame(&[0x2e, 0, high, low]))
            }
            (State::AwaitsAccount, [0x38, user_high, user_low, high, low]) => {
                Ok(tpkt::data_frame(&[0x3e, 0, *user_high, *user_low, *high, *low, *high, *low]))
            }
            (State::AwaitsAccount | State::HandsOutALicence | State::Open, [0x64, _, _, high, low, _, rest @ ..]) => {
                let arrived = match rest {
                    [first, second, arrived @ ..] if first & 0x80 != 0 => {
                        arrived.get(..(usize::from(first & 0x7f) << 8) | usize::from(*second)).ok_or(WRONG)?
                    }
                    [first, arrived @ ..] => arrived.get(..usize::from(*first)).ok_or(WRONG)?,
                    _ => return Err(WRONG),
                };
                match (u16::from_be_bytes([*high, *low]), &self.state) {
                    (DESKTOP, _) => {}
                    (PROGRAMS, State::Open) if self.programs => return self.about_programs(arrived),
                    (on, State::Open) if Some(on) == self.channel(devices::CHANNEL) => return self.about_devices(arrived),
                    (on, State::Open) if Some(on) == self.channel(dynamic::CHANNEL) => return self.about_later_channels(arrived),
                    (on, State::Open) if Some(on) == self.channel(clipboard::CHANNEL) => return self.about_the_clipboard(arrived),
                    _ => return Ok(Vec::new()),
                }
                match self.state {
                    State::AwaitsAccount => self.account(arrived),
                    State::HandsOutALicence => self.licence(arrived),
                    _ => self.open(arrived),
                }
            }
            (_, [0x21, ..]) => {
                self.seen.left = true;
                Ok(Vec::new())
            }
            _ => Err(WRONG),
        }
    }

    /// Who the desktop is for. The answer: no licence is needed, and what the machine can
    /// do; or, from a machine that hands out licences, that the client is to take one.
    fn account(&mut self, arrived: &[u8]) -> Result<Vec<u8>, Error> {
        if number16(arrived, 0)? != 0x0040 {
            return Err(WRONG);
        }
        let (domain, name, password) = (usize::from(number16(arrived, 12)?), usize::from(number16(arrived, 14)?), number16(arrived, 16)?);
        let texts = arrived.get(22..).ok_or(WRONG)?;
        self.seen.account =
            (ntlm::utf16(texts.get(..domain).ok_or(WRONG)?), ntlm::utf16(texts.get(domain + 2..domain + 2 + name).ok_or(WRONG)?));
        let travelled = texts.get(domain + name + 4..domain + name + 4 + usize::from(password)).ok_or(WRONG)?;
        self.seen.password = (password != 0).then(|| ntlm::utf16(travelled));
        self.seen.for_a_program = number16(arrived, 8)? & 0x8000 != 0;
        if let Some((machine, _)) = &self.hands_out {
            self.state = State::HandsOutALicence;
            return Ok(to_the_client(&machine.asks()));
        }
        self.state = State::Open;

        let mut licence = vec![0x80, 0, 0, 0, 0xff, 0x03, 0x10, 0x00];
        licence.extend(7u32.to_le_bytes());
        licence.extend(2u32.to_le_bytes());
        licence.extend([4, 0, 0, 0]);
        let mut send = to_the_client(&licence);
        // a broker shows nothing: it says where the connection belongs. A client that
        // comes as it was told to is where it belongs
        if let Some(to) = self.sends_on.as_ref().filter(|_| self.seen.sent_on_to.is_none()) {
            let word = [&[0, 0][..], &redirection::write(to), &[0]].concat();
            send.extend(to_the_client(&pdu::share(10, mcs::MACHINE, &word)));
            return Ok(send);
        }
        send.extend(self.can_do());
        Ok(send)
    }

    /// The client's answers on the way to a licence: the first is challenged, and after
    /// the second it is handed the licence and told what the machine can do. A machine
    /// that hands none over goes on after the first.
    fn licence(&mut self, arrived: &[u8]) -> Result<Vec<u8>, Error> {
        let (machine, licences) = self.hands_out.as_mut().ok_or(WRONG)?;
        let last = match (arrived.get(4), *licences) {
            (Some(0x13), Licences::HandedOver) => return Ok(to_the_client(&machine.challenges(arrived)?)),
            (Some(0x13), Licences::NoneHandedOver) => machine.goes_on(arrived)?,
            (Some(0x15), Licences::HandedOver) => machine.hands(arrived)?,
            _ => return Err(WRONG),
        };
        self.seen.licence = Some(machine.seen.clone());
        self.state = State::Open;
        let mut send = to_the_client(&last);
        send.extend(self.can_do());
        Ok(send)
    }

    /// What the machine can do, as it tells the client.
    fn can_do(&self) -> Vec<u8> {
        let (width, height) = (self.picture.width as u16, self.picture.height as u16);
        let mut sets = pdu::capability(1, &pdu::words(&[1, 3, 0x0200, 0, 0, 0x041d, 0, 0, 0, 0]));
        sets.extend(pdu::capability(2, &pdu::words(&[self.bits, 1, 1, 1, width, height, 0, 1, 1, 0, 1, 0])));
        let mut can = Vec::new();
        can.extend(SHARING.to_le_bytes());
        can.extend(pdu::words(&[4, (sets.len() + 4) as u16]));
        can.extend(b"RDP\0");
        can.extend(pdu::words(&[2, 0]));
        can.extend(sets);
        can.extend(0u32.to_le_bytes());
        to_the_client(&pdu::share(1, mcs::MACHINE, &can))
    }

    /// The messages of the open connection: each of the client's last steps is answered,
    /// and after the last the picture is drawn.
    fn open(&mut self, mut arrived: &[u8]) -> Result<Vec<u8>, Error> {
        let mut send = Vec::new();
        let data = |kind: u8, holds: &[u8]| to_the_client(&pdu::share_data(mcs::MACHINE, SHARING, kind, holds));
        while !arrived.is_empty() {
            let message = arrived.get(..usize::from(number16(arrived, 0)?)).filter(|message| message.len() >= 6).ok_or(WRONG)?;
            arrived = &arrived[message.len()..];
            let holds = &message[6..];
            match number16(message, 2)? & 0x0f {
                // what the client can do
                3 => {
                    let named = usize::from(number16(holds, 6)?);
                    let count = number16(holds, 10 + named)?;
                    let mut sets = holds.get(14 + named..).ok_or(WRONG)?;
                    for _ in 0..count {
                        let set = sets.get(..usize::from(number16(sets, 2)?)).filter(|set| set.len() >= 4).ok_or(WRONG)?;
                        match number16(set, 0)? {
                            2 => self.seen.bits = number16(set, 4)?,
                            // the 32 instructions it could take, after its description and what it keeps of the desktop
                            3 => self.seen.takes_instructions = set.get(36..68).ok_or(WRONG)?.iter().any(|takes| *takes != 0),
                            _ => {}
                        }
                        sets = &sets[set.len()..];
                    }
                }
                7 => match (holds.get(8).ok_or(WRONG)?, holds.get(12..).ok_or(WRONG)?) {
                    (0x1f, _) => send.extend(data(0x1f, &pdu::words(&[1, mcs::MACHINE]))),
                    (0x14, [4, 0, ..]) => send.extend(data(0x14, &pdu::words(&[4, 0, 0, 0]))),
                    (0x14, [1, 0, ..]) => send.extend(data(0x14, &pdu::words(&[2, USER, 0x03ea, 0]))),
                    (0x27, _) => {
                        send.extend(data(0x28, &pdu::words(&[0, 0, 3, 4])));
                        // a client that came for a program is shown no desktop: the machine
                        // says that it is ready for programs
                        // once: a desktop of programs offered anew goes on with the programs it has
                        match (self.programs, self.ready_for_programs) {
                            (true, false) => {
                                self.ready_for_programs = true;
                                send.extend(on_the_programs_channel(&rail::stand_in::ready()));
                            }
                            (true, true) => {}
                            (false, _) => send.extend(self.draw()),
                        }
                        // to a client that asked for the channel of the ones that are
                        // opened later, it says once which version of them it speaks
                        if let (Some(on), false) = (self.channel(dynamic::CHANNEL), self.speaks_of_later) {
                            self.speaks_of_later = true;
                            send.extend(on_a_channel(on, &dynamic::stand_in::can_do(3), false));
                        }
                        // to a client that asked for the channel of the clipboard, it says
                        // once what it can do and that it is ready
                        if let (Some(on), false) = (self.channel(clipboard::CHANNEL), self.speaks_of_the_clipboard) {
                            self.speaks_of_the_clipboard = true;
                            for message in clipboard::stand_in::ready(true) {
                                send.extend(on_a_channel(on, &message, true));
                            }
                        }
                        // and to a client that asked for the channel of devices and the one
                        // of sound, it begins to speak about devices
                        if let (Some(on), Some(_)) = (self.channel(devices::CHANNEL), self.channel(devices::SOUND)) {
                            send.extend(on_a_channel(on, &devices::stand_in::begins(CLIENT), false));
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        Ok(send)
    }

    /// What the client says about the clipboard. A client that says it holds text is
    /// confirmed, and asked for the text at once, as when somebody pastes on the machine;
    /// what it hands over is kept. A client that asks for the machine's text is given
    /// what the stand-in copies: the text it was last handed, in capitals, or its own.
    fn about_the_clipboard(&mut self, piece: &[u8]) -> Result<Vec<u8>, Error> {
        use clipboard::stand_in::Said;
        let Some(message) = self.clipboard_pieces.take(piece)? else { return Ok(Vec::new()) };
        let Some(on) = self.channel(clipboard::CHANNEL) else { return Ok(Vec::new()) };
        let says = |message: &[u8]| on_a_channel(on, message, true);
        Ok(match clipboard::stand_in::read(&message) {
            Said::Holds { text: true } => [says(&clipboard::stand_in::took()), says(&clipboard::stand_in::asks())].concat(),
            Said::Holds { text: false } => says(&clipboard::stand_in::took()),
            Said::Gives(Some(text)) => {
                self.seen.pasted.push(text);
                Vec::new()
            }
            // what it copies: the text it was last handed, in capitals, or its own
            Said::Asks => {
                let copies = self.seen.pasted.last().map(|handed| handed.to_uppercase());
                says(&clipboard::stand_in::gives(copies.as_deref().unwrap_or(COPIED)))
            }
            _ => Vec::new(),
        })
    }

    /// What the client says of the channels the machine opens later. Once it said which
    /// version it speaks, the stand-in opens two: the one of the screen, and one that is
    /// not Away's. On the one of the screen it says what it takes, and a client that
    /// says how its screen is now is offered the desktop anew, at that size.
    fn about_later_channels(&mut self, piece: &[u8]) -> Result<Vec<u8>, Error> {
        let Some(message) = self.later_pieces.take(piece)? else { return Ok(Vec::new()) };
        let Some(on) = self.channel(dynamic::CHANNEL) else { return Ok(Vec::new()) };
        let says = |message: &[u8]| on_a_channel(on, message, false);
        let mut send = Vec::new();
        if message.first().is_some_and(|head| head >> 4 == 5) {
            send.extend(says(&dynamic::stand_in::opens(NOT_AWAYS, "Microsoft::Windows::RDS::Geometry::v08.01")));
            send.extend(says(&dynamic::stand_in::opens(SCREEN, dynamic::SCREEN)));
        } else if let Some((id, taken)) = dynamic::stand_in::read_answer(&message) {
            if taken {
                self.seen.took_later.push(id);
            }
            if (id, taken) == (SCREEN, true) {
                send.extend(says(&dynamic::stand_in::takes(SCREEN, 8192, 8192)));
            }
        } else if let Some((SCREEN, width, height, scale)) = dynamic::stand_in::read_screen(&message) {
            self.seen.screens.push((width, height, scale));
            // what it could do is void, and it says it again for a desktop of that size:
            // the client answers as it did when the connection opened, and is drawn for anew
            self.picture = at_a_size(usize::from(width), usize::from(height));
            send.extend(to_the_client(&pdu::share(6, mcs::MACHINE, &[0; 8])));
            send.extend(self.can_do());
        }
        Ok(send)
    }

    /// What the client says about programs: each program it asks for opens in a window of
    /// its own, with its icon, where the stand-in publishes it, and a window the person
    /// closes is gone.
    fn about_programs(&mut self, piece: &[u8]) -> Result<Vec<u8>, Error> {
        let Some(message) = self.pieces.take(piece)? else { return Ok(Vec::new()) };
        let asked = rail::stand_in::read(&message)?;
        self.seen.about_programs.push(asked.clone());
        let words = |orders: &[Vec<u8>]| short_frame(0x00, &[&(orders.len() as u16).to_le_bytes()[..], &orders.concat()].concat());
        Ok(match asked {
            Asked::Start(program) if self.publishes.contains(&program.run) => {
                let mut send = on_the_programs_channel(&rail::stand_in::result(&program, 0));
                // where the window is, its icon, and that it is in front, and then what is in
                // it: a window opened later lies over the ones before
                let title = program.run.trim_start_matches('|');
                let (at, size) = ((WINDOW_AT.0 as i32, WINDOW_AT.1 as i32), (WINDOW_SIZE.0 as u16, WINDOW_SIZE.1 as u16));
                let id = WINDOW + self.opened;
                self.windows.insert(0, id);
                let mut orders = vec![
                    window::stand_in::window(id, true, title, at, size),
                    window::stand_in::icon(id, true, Some((0, self.opened as u16)), &program_icon()),
                    window::stand_in::in_front(id, &self.windows),
                ];
                // the first program puts an icon in the tray, as many do
                if self.opened == 0 {
                    orders.push(window::stand_in::tray(id, TRAY_ICON, title, 4, &program_icon()));
                }
                if self.floats_a_language_bar && self.opened == 0 {
                    orders.push(window::stand_in::window(
                        LANGUAGE_BAR,
                        true,
                        "CiceroUIWndFrame-TF_FloatingLangBar_WndTitle",
                        (120, 0),
                        (80, 8),
                    ));
                }
                self.opened += 1;
                send.extend(words(&orders));
                for top in (WINDOW_AT.1..WINDOW_AT.1 + WINDOW_SIZE.1).step_by(TILE) {
                    for left in (WINDOW_AT.0..WINDOW_AT.0 + WINDOW_SIZE.0).step_by(TILE) {
                        let (wide, high) = (TILE.min(WINDOW_AT.0 + WINDOW_SIZE.0 - left), TILE.min(WINDOW_AT.1 + WINDOW_SIZE.1 - top));
                        let mut update = vec![1, 0, 1, 0];
                        update.extend(self.rectangle(left, top, wide, high, left / TILE + top / TILE));
                        send.extend(short_frame(0x01, &update));
                    }
                }
                send
            }
            // a program it doesn't publish is not among those it lets be started
            Asked::Start(program) => on_the_programs_channel(&rail::stand_in::result(&program, 3)),
            // a window the person works in comes to the front, as on a Windows machine
            Asked::Activate(id, true) if self.windows.contains(&id) => {
                self.windows.retain(|open| *open != id);
                self.windows.insert(0, id);
                words(&[window::stand_in::in_front(id, &self.windows)])
            }
            Asked::Command(id, rail::Command::Close) if self.windows.contains(&id) => {
                self.windows.retain(|open| *open != id);
                words(&[window::stand_in::gone(id)])
            }
            _ => Vec::new(),
        })
    }

    /// What the client says about devices: the machine says what it can do and takes
    /// the client's number, takes the reader the client names, and, where it reads
    /// cards, asks the reader one thing after the other.
    fn about_devices(&mut self, piece: &[u8]) -> Result<Vec<u8>, Error> {
        let Some(on) = self.channel(devices::CHANNEL) else { return Ok(Vec::new()) };
        let Some(message) = self.device_pieces.take(piece)? else { return Ok(Vec::new()) };
        let mut says = Vec::new();
        match devices::stand_in::read(&message)? {
            devices::stand_in::Client::Begins { client, .. } if client != CLIENT => return Err(WRONG),
            devices::stand_in::Client::Begins { .. } | devices::stand_in::Client::CanDo { .. } => {}
            devices::stand_in::Client::Name(_) => says.extend([devices::stand_in::can_do(), devices::stand_in::confirms(CLIENT)]),
            devices::stand_in::Client::Devices(named) => {
                says.extend(named.iter().map(|(_, device, _)| devices::stand_in::took(*device)));
                self.seen.devices.extend(named);
                // a machine that reads cards begins with the reader at once
                if self.reads.is_some() && self.seen.devices.iter().any(|(_, device, _)| *device == devices::READER) {
                    self.seen.card = Some(Card::default());
                    says.push(devices::stand_in::asks(
                        devices::READER,
                        100,
                        smartcard::stand_in::started().0,
                        &smartcard::stand_in::started().1,
                    ));
                }
            }
            devices::stand_in::Client::Answers { request, result, gives, .. } => {
                if let Some((code, carried)) = self.reads_on(request, result, &gives)? {
                    let request = self.reads.as_ref().map_or(0, |reads| reads.request);
                    says.push(devices::stand_in::asks(devices::READER, request, code, &carried));
                }
            }
        }
        Ok(says.iter().flat_map(|message| on_a_channel(on, message, false)).collect())
    }

    /// Takes the reader's answer to what was asked last, and gives what to ask next.
    fn reads_on(&mut self, request: u32, result: u32, gives: &[u8]) -> Result<Option<smartcard::stand_in::Request>, Error> {
        use smartcard::stand_in as asks;
        let (Some(reads), Some(card)) = (self.reads.as_mut(), self.seen.card.as_mut()) else { return Ok(None) };
        // an answer to another request than the last, one that did not work, or one the reader refused
        let answer = asks::read(gives)?;
        if request != reads.request || result != 0 || answer.result != 0 {
            reads.step = Reading::Done;
            return Ok(None);
        }
        let (context, opened) = (reads.context, reads.opened);
        let command = |at: usize, reads: &Reads| match reads.commands.get(at) {
            Some(command) => (Reading::Commands(at), asks::transmit(context, opened, command)),
            None => (Reading::LetsGo, asks::end(context, opened)),
        };
        let (step, next) = match reads.step {
            Reading::WhetherThere => (Reading::Begins, asks::establish()),
            Reading::Begins => {
                reads.context = answer.context();
                (Reading::Readers, asks::readers(reads.context, true, None))
            }
            Reading::Readers => {
                let units: Vec<u16> = answer.carried().as_chunks::<2>().0.iter().map(|two| u16::from_le_bytes(*two)).collect();
                card.reader = String::from_utf16_lossy(&units).split('\0').next().unwrap_or_default().to_string();
                (Reading::Watches, asks::watch(context, None, &[(&card.reader, 0)]))
            }
            Reading::Watches => (Reading::Opens, asks::connect(context, &card.reader)),
            Reading::Opens => {
                reads.opened = answer.opened();
                (Reading::About, asks::status(context, reads.opened))
            }
            Reading::About => {
                card.says = answer.about_the_card().1;
                (Reading::Holds, asks::begin(context, opened))
            }
            Reading::Holds => command(0, reads),
            Reading::Commands(at) => {
                card.answers.push(answer.carried());
                command(at + 1, reads)
            }
            Reading::LetsGo => (Reading::Closes, asks::disconnect(context, opened, false)),
            Reading::Closes => (Reading::Ends, asks::release(context)),
            Reading::Ends | Reading::Done => {
                card.closed = true;
                reads.step = Reading::Done;
                return Ok(None);
            }
        };
        reads.step = step;
        reads.request += 1;
        Ok(Some(next))
    }

    /// What a machine sends a client whose desktop was opened over another connection:
    /// its reason for ending this one, and that it ends it.
    pub fn taken_elsewhere(&self) -> Vec<u8> {
        let mut send = to_the_client(&pdu::share_data(mcs::MACHINE, SHARING, 0x2f, &5u32.to_le_bytes()));
        // the reason a machine gives the connection's end itself: that it ends it
        send.extend(tpkt::data_frame(&[0x20, 0x80]));
        send
    }

    /// Takes what a frame of the short kind tells of keys and of the pointer, and draws
    /// as a desktop would: a mark where the left button went down, and a mark in the
    /// corner for a key, whose colour says which key it was. The answer is what it drew.
    pub fn input(&mut self, head: u8, carried: &[u8]) -> Result<Vec<u8>, Error> {
        if !matches!(self.state, State::Open) {
            return Err(WRONG);
        }
        let mut send = Vec::new();
        for event in input::read(head, carried)? {
            self.seen.input.push(event);
            // the key C copies: the machine's clipboard holds text, which the client is told
            if let (Input::Key { code: KEY_C, down: true, .. }, Some(on)) = (event, self.channel(clipboard::CHANNEL)) {
                send.extend(on_a_channel(on, &clipboard::stand_in::holds_text(true), true));
            }
            let (left, top, colour) = match event {
                Input::Button { button: Button::Left, down: true, x, y } => (usize::from(x), usize::from(y), CLICKED),
                Input::Key { code, down: true, .. } => (0, 0, typed(code)),
                _ => continue,
            };
            let (left, top) = (left.min(self.picture.width - 1), top.min(self.picture.height - 1));
            let (wide, high) = (MARK.min(self.picture.width - left), MARK.min(self.picture.height - top));
            for row in top..top + high {
                for column in left..left + wide {
                    self.picture.pixels[(row * self.picture.width + column) * 4..][..3].copy_from_slice(&colour);
                }
            }
            // one rectangle, as it is, in a frame of the short kind
            let rectangle = self.rectangle(left, top, wide, high, if self.bits == 16 { 1 } else { 2 });
            let mut update = vec![1, 0, 1, 0];
            update.extend(rectangle);
            send.extend(short_frame(0x01, &update));
        }
        Ok(send)
    }

    /// The picture as the stand-in has it now, with what it drew for keys and the pointer.
    pub fn picture(&self) -> &Picture {
        &self.picture
    }

    /// The picture, cut into rectangles and sent in each way a machine sends them.
    fn draw(&self) -> Vec<u8> {
        let (width, height) = (self.picture.width, self.picture.height);
        let mut rectangles = Vec::new();
        for top in (0..height).step_by(TILE) {
            for left in (0..width).step_by(TILE) {
                rectangles.push(self.rectangle(left, top, TILE.min(width - left), TILE.min(height - top), rectangles.len()));
            }
        }
        let update = |rectangles: &[Vec<u8>]| {
            let mut update = vec![1, 0];
            update.extend((rectangles.len() as u16).to_le_bytes());
            update.extend(rectangles.concat());
            update
        };
        let mut send = Vec::new();
        // the last rectangle in a frame of the ordinary kind
        let last = rectangles.pop();
        // the first half, each in a short frame of its own
        let half = rectangles.len() / 2;
        for rectangle in &rectangles[..half] {
            send.extend(short_frame(0x01, &update(std::slice::from_ref(rectangle))));
        }
        // the rest as one update that travels in three pieces: the first, one in between,
        // the last. Of a large picture it is several such updates, each of as many
        // rectangles as three pieces carry
        let mut rest = &rectangles[half..];
        while !rest.is_empty() {
            let (mut count, mut bytes) = (0, 0);
            while count < rest.len() && (count == 0 || bytes + rest[count].len() <= 3 * MOST_IN_A_PIECE) {
                bytes += rest[count].len();
                count += 1;
            }
            let together = update(&rest[..count]);
            rest = &rest[count..];
            let pieces: Vec<&[u8]> = together.chunks(together.len() / 3 + 1).collect();
            for (nth, piece) in pieces.iter().enumerate() {
                let which = match nth {
                    _ if pieces.len() == 1 => 0x0,
                    0 => 0x2,
                    _ if nth + 1 == pieces.len() => 0x1,
                    _ => 0x3,
                };
                send.extend(short_frame(0x01 | (which << 4), piece));
            }
        }
        if let Some(last) = last {
            send.extend(to_the_client(&pdu::share_data(mcs::MACHINE, SHARING, 0x02, &update(&[last]))));
        }
        send
    }

    /// One rectangle of the picture, in the way its number chooses.
    fn rectangle(&self, left: usize, top: usize, wide: usize, high: usize, nth: usize) -> Vec<u8> {
        // every fourth carries more columns than it has room for, as a machine does to
        // keep its rows at whole words; at 16 bits one that travels as it is carries one
        // more, so that its rows have to be filled up
        let carried = match (nth % 4, self.bits) {
            (3, _) => wide + 4,
            (1, 16) => wide + 1,
            _ => wide,
        };
        let mut pixels = Vec::with_capacity(carried * high * 4);
        for row in top..top + high {
            let from = (row * self.picture.width + left) * 4;
            pixels.extend_from_slice(&self.picture.pixels[from..from + wide * 4]);
            // what is beyond the rectangle's room is never to be seen
            pixels.extend((0..(carried - wide) * 4).map(|at| (at * 37 + row) as u8));
        }
        let (flags, carries): (u16, Vec<u8>) = match nth % 4 {
            _ if self.bits == 16 => {
                // the pixels cut to 16 bits, as they travel: from the bottom row up
                let cut: Vec<u16> = pixels
                    .chunks_exact(carried * 4)
                    .rev()
                    .flatten()
                    .copied()
                    .collect::<Vec<u8>>()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|pixel| interleaved::to_16_bits(pixel))
                    .collect();
                match nth % 4 {
                    1 => {
                        let filled = |row: &[u16]| {
                            row.iter()
                                .flat_map(|pixel| pixel.to_le_bytes())
                                .chain([0xaa; 4])
                                .take((carried * 2).next_multiple_of(4))
                                .collect::<Vec<u8>>()
                        };
                        (0, cut.chunks_exact(carried).flat_map(filled).collect())
                    }
                    // with a head of its own before what it carries
                    2 => (0x0001, [&[0; 8][..], &interleaved::pack(&cut, carried)].concat()),
                    _ => (0x0401, interleaved::pack(&cut, carried)),
                }
            }
            0 => (0x0401, pack(&pixels, carried, high, true)),
            1 => (0x0401, pack(&pixels, carried, high, false)),
            2 => (0, pixels.chunks_exact(carried * 4).rev().flatten().copied().collect()),
            // with a head of its own before what it carries
            _ => (0x0001, [&[0; 8][..], &pack(&pixels, carried, high, true)].concat()),
        };
        let bits = usize::from(self.bits);
        let numbers = [left, top, left + wide - 1, top + high - 1, carried, high, bits, usize::from(flags), carries.len()]
            .map(|number| number as u16);
        let mut rectangle = pdu::words(&numbers);
        rectangle.extend(carries);
        rectangle
    }
}

fn number16(bytes: &[u8], at: usize) -> Result<u16, Error> {
    bytes.get(at..at + 2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).ok_or(WRONG)
}

/// A length as the conference's messages write it: one byte, or two with the top bit set.
fn length(of: usize) -> Vec<u8> {
    if of < 0x80 {
        vec![of as u8]
    } else {
        (0x8000 | of as u16).to_be_bytes().to_vec()
    }
}

/// The machine's answer to what the client says of itself: the desktop's channel, and no
/// encryption of RDP's own.
fn connect_answer(further: usize) -> Vec<u8> {
    let block = |kind: u16, holds: &[u8]| {
        let mut block = pdu::words(&[kind, (holds.len() + 4) as u16]);
        block.extend_from_slice(holds);
        block
    };
    let mut blocks = block(0x0c01, &[0x04, 0x00, 0x08, 0x00, 0x0b, 0, 0, 0, 0x01, 0, 0, 0]);
    blocks.extend(block(0x0c02, &[0; 8]));
    // the channels: the desktop's, and one for each further channel the client asked
    // for, filled up to a whole four bytes
    let mut channels = vec![DESKTOP, further as u16];
    channels.extend((0..further as u16).map(|at| PROGRAMS + at));
    channels.resize(channels.len().next_multiple_of(2), 0);
    blocks.extend(block(0x0c03, &pdu::words(&channels)));
    let mut says = vec![0x00, 0x05, 0x00, 0x14, 0x7c, 0x00, 0x01, 0x2a, 0x14, 0x76, 0x0a, 0x01, 0x01, 0x00, 0x01, 0xc0, 0x00];
    says.extend(b"McDn");
    says.extend(length(blocks.len()));
    says.extend(blocks);
    let mut fields = vec![0x0a, 0x01, 0x00, 0x02, 0x01, 0x00];
    fields.extend(mcs::limits(34, 3, 0, 65_528));
    fields.extend(der::write(der::OCTET_STRING, &says));
    let mut answer = vec![0x7f, 0x66];
    answer.extend(&der::write(0, &fields)[1..]);
    answer
}

/// A message for the client, on the desktop's channel, in a frame of the ordinary kind.
fn to_the_client(carried: &[u8]) -> Vec<u8> {
    let [high, low] = DESKTOP.to_be_bytes();
    let mut message = vec![0x68, 0x00, 0x01, high, low, 0x70];
    message.extend(length(carried.len()));
    message.extend_from_slice(carried);
    tpkt::data_frame(&message)
}

/// A message about programs for the client, in the pieces and the frames it travels in.
fn on_the_programs_channel(message: &[u8]) -> Vec<u8> {
    on_a_channel(PROGRAMS, message, true)
}

/// A message for the client on a further channel, in the pieces and the frames it
/// travels in.
fn on_a_channel(on: u16, message: &[u8], shows_its_heads: bool) -> Vec<u8> {
    let [high, low] = on.to_be_bytes();
    let mut send = Vec::new();
    for piece in channel::pieces(message, shows_its_heads) {
        let mut carried = vec![0x68, 0x00, 0x01, high, low, 0x70];
        carried.extend(length(piece.len()));
        carried.extend(piece);
        send.extend(tpkt::data_frame(&carried));
    }
    send
}

/// A frame of the short kind around one update, or one piece of one.
fn short_frame(head: u8, update: &[u8]) -> Vec<u8> {
    let mut carried = vec![head];
    carried.extend((update.len() as u16).to_le_bytes());
    carried.extend_from_slice(update);
    let whole = carried.len() + if carried.len() + 2 < 0x80 { 2 } else { 3 };
    let mut frame = vec![0x00];
    frame.extend(length(whole));
    frame.extend(carried);
    frame
}

/// Packs pixels the way a machine does when nothing may be left out: each colour as a
/// plane of its own, as it is or as runs. The pixels come from the top row down, four
/// bytes each: blue, green, red, and a fourth that is left out.
pub fn pack(pixels: &[u8], width: usize, height: usize, in_runs: bool) -> Vec<u8> {
    let mut packed = vec![if in_runs { 0x30 } else { 0x20 }];
    for colour in [2, 1, 0] {
        // a plane's rows travel from the bottom up
        let plane: Vec<u8> =
            (0..height).rev().flat_map(|row| (0..width).map(move |column| pixels[(row * width + column) * 4 + colour])).collect();
        if in_runs {
            for row in 0..height {
                let line = &plane[row * width..(row + 1) * width];
                // the first row as it is, every later one as what changed against the row before
                let written: Vec<u8> = match row {
                    0 => line.to_vec(),
                    _ => line
                        .iter()
                        .zip(&plane[(row - 1) * width..])
                        .map(|(now, before)| match now.wrapping_sub(*before) as i8 {
                            change @ 0.. => (change as u8) << 1,
                            change => (((-i16::from(change) - 1) as u8) << 1) | 1,
                        })
                        .collect(),
                };
                packed.extend(runs(&written));
            }
        } else {
            packed.extend(plane);
        }
    }
    if !in_runs {
        packed.push(0);
    }
    packed
}

/// One row as pieces: values written out, each piece followed by a run of its last value.
fn runs(row: &[u8]) -> Vec<u8> {
    let mut pieces = Vec::new();
    let (mut at, mut last) = (0, 0u8);
    while at < row.len() {
        // values are written out until at least three in a row repeat the one before them
        let mut written = at;
        let run = loop {
            let before = if written == at { last } else { row[written - 1] };
            let run = row[written..].iter().take_while(|value| **value == before).count();
            if run >= 3 || written == row.len() {
                break run;
            }
            written += 1;
        };
        let value = if written == at { last } else { row[written - 1] };
        let mut values = &row[at..written];
        while values.len() > 15 {
            pieces.push(0xf0);
            pieces.extend_from_slice(&values[..15]);
            values = &values[15..];
        }
        let mut left = run;
        if left < 16 {
            // a short run shares its first byte with the values before it
            if !values.is_empty() || left > 0 {
                pieces.push(((values.len() as u8) << 4) | left as u8);
                pieces.extend_from_slice(values);
            }
            left = 0;
        } else if !values.is_empty() {
            pieces.push((values.len() as u8) << 4);
            pieces.extend_from_slice(values);
        }
        // a long run has bytes of its own: 32 to 47 at a time, or 16 to 31
        while left >= 16 {
            let now = left.min(47);
            pieces.push(if now >= 32 { (((now - 32) as u8) << 4) | 2 } else { (((now - 16) as u8) << 4) | 1 });
            left -= now;
        }
        match left {
            0 => {}
            // too short to be a run: written out
            1 | 2 => {
                pieces.push((left as u8) << 4);
                pieces.extend(std::iter::repeat_n(value, left));
            }
            _ => pieces.push(left as u8),
        }
        last = value;
        at = written + run;
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planar;

    /// Pixels that hold flat parts, slopes and noise, so that every kind of piece occurs.
    fn pixels(width: usize, height: usize, seed: u32) -> Vec<u8> {
        let mut state = seed;
        let mut pixels = Vec::with_capacity(width * height * 4);
        for row in 0..height {
            for column in 0..width {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let pixel = match (column * 4 / width.max(1) + seed as usize) % 4 {
                    0 => [0xf3, 0xf2, 0xf1],
                    1 => [(row * 3) as u8, column as u8, (row + column) as u8],
                    2 => [(state >> 8) as u8, (state >> 16) as u8, (state >> 24) as u8],
                    _ => [if row % 2 == 0 { 0 } else { 255 }; 3],
                };
                pixels.extend(pixel);
                pixels.push(0xff);
            }
        }
        pixels
    }

    #[test]
    fn what_is_packed_unpacks_to_the_same_pixels() {
        for (width, height) in [(1, 1), (1, 9), (2, 2), (3, 5), (16, 4), (17, 3), (47, 2), (48, 6), (64, 64), (100, 7), (131, 33)] {
            for seed in 0..4 {
                let pixels = pixels(width, height, seed);
                for in_runs in [true, false] {
                    let packed = pack(&pixels, width, height, in_runs);
                    assert_eq!(planar::unpack(&packed, width, height).unwrap(), pixels, "{width} by {height}, runs: {in_runs}");
                }
            }
        }
        // one colour all over: a few bytes a row
        let flat = [7, 8, 9, 0xff].repeat(64 * 64);
        assert!(pack(&flat, 64, 64, true).len() < 64 * 3 * 3);
        assert_eq!(planar::unpack(&pack(&flat, 64, 64, true), 64, 64).unwrap(), flat);
    }

    #[test]
    fn runs_are_written_as_a_machine_writes_them() {
        assert_eq!(runs(&[7, 9, 9, 9, 9, 9]), [0x24, 7, 9]);
        assert_eq!(runs(&[0; 6]), [0x06]);
        assert_eq!(runs(&[5; 20]), [0x10, 5, 0x31]);
        assert_eq!(runs(&[5; 40]), [0x10, 5, 0x72]);
        // two alike are no run
        assert_eq!(runs(&[1, 2, 2, 3]), [0x40, 1, 2, 2, 3]);
        assert_eq!(runs(&[5; 18]), [0x10, 5, 0x11]);
        // 48 left after one value: 47 as a long run, and the last, too short for a run, written out
        assert_eq!(runs(&[5; 49]), [0x10, 5, 0xf2, 0x10, 5]);
    }
}
