//! A smart card reader for the machine to reach over the connection (MS-RDPESC).
//!
//! The machine speaks to the reader as a program on Windows speaks to one: it opens a
//! context, asks which readers there are, watches them for a card, opens the card, and
//! gives it commands. Each of these is a request to the device ([`crate::devices`]),
//! written the way Windows writes what one of its programs calls in another (MS-RPCE
//! 2.2.6), and each gets an answer written the same way.
//!
//! The reader here has one card that is always in it. What the card is, is not this
//! module's: it gives the card's commands to a [`Card`] and takes the card's answers.

use std::collections::VecDeque;

use crate::Error;

/// A card in the reader: something that takes commands and answers them, as a card
/// does (ISO 7816-4).
pub trait Card: Send {
    /// What the card says of itself when it is given power.
    fn says_of_itself(&self) -> Vec<u8>;
    /// Takes one command, and gives the card's answer.
    fn answer(&mut self, command: &[u8]) -> Vec<u8>;
    /// The card loses its power and gets it again: it forgets what it was told.
    fn reset(&mut self);
}

/// What the machine asked of the reader, for whoever says what happens on a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asked {
    /// Whether the client's smart cards are there at all.
    WhetherThere,
    /// To begin or to end speaking to the reader.
    ToSpeak,
    /// Which readers there are.
    Readers,
    /// Whether a card is in the reader.
    ForACard,
    /// To open the card, to close it, or to have it for itself for a while.
    TheCard,
    /// The card, for an answer to a command.
    ACommand,
    /// Something of the reader or the card: its name, what the card says of itself.
    About,
    /// Something this reader does not do, by the request's code.
    Other(u32),
}

/// An answer to a request, to send back under the request's number.
#[derive(Debug, PartialEq, Eq)]
pub struct Answered {
    /// The request's number.
    pub request: u32,
    /// What the reader gives back; nothing where the request was not built as one, which
    /// is answered as a request that did not work.
    pub gives: Option<Vec<u8>>,
}

const WRONG: Error = Error::NotRdp("a request to the smart card reader is not built as one");

// the requests, by their codes (MS-RDPESC 3.1.4)
const ESTABLISH: u32 = 0x0009_0014;
const RELEASE: u32 = 0x0009_0018;
const IS_VALID: u32 = 0x0009_001c;
const READERS_A: u32 = 0x0009_0028;
const READERS_W: u32 = 0x0009_002c;
const CHANGE_A: u32 = 0x0009_00a0;
const CHANGE_W: u32 = 0x0009_00a4;
const CANCEL: u32 = 0x0009_00a8;
const CONNECT_A: u32 = 0x0009_00ac;
const CONNECT_W: u32 = 0x0009_00b0;
const RECONNECT: u32 = 0x0009_00b4;
const DISCONNECT: u32 = 0x0009_00b8;
const BEGIN: u32 = 0x0009_00bc;
const END: u32 = 0x0009_00c0;
const STATUS_A: u32 = 0x0009_00c8;
const STATUS_W: u32 = 0x0009_00cc;
const TRANSMIT: u32 = 0x0009_00d0;
const CONTROL: u32 = 0x0009_00d4;
const ATTRIBUTE: u32 = 0x0009_00d8;
const STARTED: u32 = 0x0009_00e0;
// what a machine asks only of a client that says it is a newer one (MS-RDPESC 1.7), and
// what is asked seldom: each is answered in the form its answer has
const GROUPS_A: u32 = 0x0009_0020;
const GROUPS_W: u32 = 0x0009_0024;
const LOCATE_A: u32 = 0x0009_0098;
const LOCATE_W: u32 = 0x0009_009c;
const STATE: u32 = 0x0009_00c4;
const LOCATE_BY_WHAT_IT_SAYS_A: u32 = 0x0009_00e8;
const LOCATE_BY_WHAT_IT_SAYS_W: u32 = 0x0009_00ec;
const KEPT_A: u32 = 0x0009_00f0;
const KEPT_W: u32 = 0x0009_00f4;
const KEEP_A: u32 = 0x0009_00f8;
const KEEP_W: u32 = 0x0009_00fc;
const COMMANDS_SO_FAR: u32 = 0x0009_0100;
const ICON: u32 = 0x0009_0104;
const KIND_OF_READER: u32 = 0x0009_0108;

// results, as Windows numbers them (MS-RDPESC 2.2.8)
const CANCELLED: u32 = 0x8010_0002;
const NO_SUCH_HANDLE: u32 = 0x8010_0003;
const TOO_SMALL: u32 = 0x8010_0008;
const NO_SUCH_READER: u32 = 0x8010_0009;
const TIMED_OUT: u32 = 0x8010_000a;
const OTHER_PROTOCOL: u32 = 0x8010_000f;
const NOT_HELD: u32 = 0x8010_0016;
const NOT_DONE_HERE: u32 = 0x8010_0022;
const NOT_KEPT: u32 = 0x8010_0070;

// the state of a reader (MS-RDPESC 2.2.7): what it is in the low half, a count in the high
const UNAWARE: u32 = 0x0000;
const IGNORE: u32 = 0x0001;
const CHANGED: u32 = 0x0002;
const UNKNOWN: u32 = 0x0004;
const PRESENT: u32 = 0x0020;
/// The name that stands for the readers as a whole: who watches it is told when a
/// reader comes or goes.
const NOTIFICATION: &str = "\\\\?PnP?\\Notification";

/// The way a card and a reader speak that sends whole blocks: the only one this card speaks.
const BLOCKS: u32 = 0x0000_0002;
/// A card that is ready for commands, in the way it speaks.
const READY: u32 = 6;
/// What a request for a length that has no limit carries as its length.
const ANY_LENGTH: u32 = 0xffff_ffff;
/// What a wait that never ends carries as its time.
const FOR_GOOD: u32 = 0xffff_ffff;

// what can be asked about a reader or a card (the numbers of the PC/SC specification)
const ABOUT_ITS_MAKER: u32 = 0x0001_0100;
const ABOUT_WHAT_THE_CARD_SAYS: u32 = 0x0009_0303;
const ABOUT_ITS_NAME_A: u32 = 0x7fff_0003;
const ABOUT_ITS_NAME_W: u32 = 0x7fff_0005;

/// Why a request gets no ordinary answer.
enum No {
    /// The reader refuses, with this result.
    Refused(u32),
    /// The request is not built as one.
    NotBuilt,
}

impl From<Error> for No {
    fn from(_: Error) -> No {
        No::NotBuilt
    }
}

/// What became of a request.
enum Became {
    /// It is answered.
    Answered(Vec<u8>),
    /// It waits: for a change of the reader, or for the card to be free.
    Waits,
}

/// Reads what a request carries.
struct Reads<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reads<'a> {
    /// What a request carries begins with two heads of eight bytes each: how it is
    /// written, and how long it is.
    fn new(carried: &'a [u8]) -> Result<Reads<'a>, Error> {
        match carried.get(..16) {
            Some(heads) if heads[..4] == [0x01, 0x10, 0x08, 0x00] => Ok(Reads { bytes: &carried[16..], at: 0 }),
            _ => Err(WRONG),
        }
    }

    fn number(&mut self) -> Result<u32, Error> {
        self.at = (self.at + 3) & !3;
        let four = self.take(4)?;
        Ok(u32::from_le_bytes([four[0], four[1], four[2], four[3]]))
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], Error> {
        let taken = self.bytes.get(self.at..self.at.checked_add(length).ok_or(WRONG)?).ok_or(WRONG)?;
        self.at += length;
        Ok(taken)
    }

    /// Bytes with their count before them.
    fn counted(&mut self) -> Result<&'a [u8], Error> {
        let count = self.number()? as usize;
        self.take(count)
    }

    /// Bytes behind a pointer, where the pointer points at any.
    fn behind(&mut self, pointer: u32) -> Result<&'a [u8], Error> {
        if pointer == 0 {
            Ok(&[])
        } else {
            self.counted()
        }
    }

    /// A text behind a pointer: how much room it has, where it begins, how long it is,
    /// and its characters with the zero that ends them.
    fn text(&mut self, wide: bool) -> Result<String, Error> {
        let (_, _, count) = (self.number()?, self.number()?, self.number()? as usize);
        if wide {
            let units: Vec<u16> =
                self.take(count.checked_mul(2).ok_or(WRONG)?)?.as_chunks::<2>().0.iter().map(|two| u16::from_le_bytes(*two)).collect();
            Ok(String::from_utf16_lossy(&units).trim_end_matches('\0').to_string())
        } else {
            Ok(String::from_utf8_lossy(self.take(count)?).trim_end_matches('\0').to_string())
        }
    }
}

/// Writes an answer.
struct Writes(Vec<u8>);

impl Writes {
    fn new(result: u32) -> Writes {
        Writes(result.to_le_bytes().to_vec())
    }

    fn number(mut self, number: u32) -> Writes {
        self.0.resize(self.0.len().next_multiple_of(4), 0);
        self.0.extend(number.to_le_bytes());
        self
    }

    fn bytes(mut self, bytes: &[u8]) -> Writes {
        self.0.extend_from_slice(bytes);
        self
    }

    /// Bytes with their count before them.
    fn counted(self, bytes: &[u8]) -> Writes {
        self.number(bytes.len() as u32).bytes(bytes)
    }

    /// The answer with its two heads, filled up to whole eights.
    fn done(mut self) -> Vec<u8> {
        self.0.resize(self.0.len().next_multiple_of(8), 0);
        let mut answer = vec![0x01, 0x10, 0x08, 0x00, 0xcc, 0xcc, 0xcc, 0xcc];
        answer.extend((self.0.len() as u32).to_le_bytes());
        answer.extend([0; 4]);
        answer.extend(self.0);
        answer
    }
}

/// An answer that is a result and nothing else, with this many further numbers that an
/// answer of its kind has, each zero.
fn only(result: u32, further: usize) -> Vec<u8> {
    Writes::new(result).bytes(&vec![0; 4 * further]).done()
}

/// What stands for a pointer that points at something: any number but zero.
const POINTS: u32 = 0x0002_0000;

/// A list of names as Windows writes one: each ended by a zero, and the list by one more.
fn names(name: &str, wide: bool) -> Vec<u8> {
    if wide {
        name.encode_utf16().chain([0, 0]).flat_map(u16::to_le_bytes).collect()
    } else {
        name.bytes().chain([0, 0]).collect()
    }
}

/// How something of a length the caller limits is given back (MS-RDPESC 2.2.2.4): the
/// length alone where only the length is wanted or the caller has no room at all, the
/// thing itself where it fits, and a refusal where it does not. The limit counts
/// characters where the thing is a text of wide ones.
fn limited(only_its_length: bool, limit: u32, each: usize, holds: &[u8]) -> Result<Option<&[u8]>, No> {
    if only_its_length || limit == 0 {
        Ok(None)
    } else if limit != ANY_LENGTH && (limit as usize) < holds.len() / each {
        Err(No::Refused(TOO_SMALL))
    } else {
        Ok(Some(holds))
    }
}

/// A request that watches the reader and waits for a change.
struct Watching {
    request: u32,
    context: u32,
    until: Option<u64>,
}

/// The reader, with its card.
pub struct Reader {
    name: String,
    card: Box<dyn Card>,
    contexts: Vec<u32>,
    /// The card as it was opened: each opening's number, and the context it belongs to.
    opened: Vec<(u32, u32)>,
    next: u32,
    /// Which opening has the card for itself, and how many times over.
    holds: Option<(u32, u32)>,
    /// Requests that wait for the card to be free, in their order.
    waiting: VecDeque<(u32, u32, Vec<u8>)>,
    watching: Vec<Watching>,
    /// How many commands the card was given.
    commands: u32,
    /// How much time went by, in milliseconds.
    clock: u64,
}

impl Reader {
    /// A reader of this name, with this card in it.
    pub fn new(name: &str, card: Box<dyn Card>) -> Reader {
        Reader {
            name: name.to_string(),
            card,
            contexts: Vec::new(),
            opened: Vec::new(),
            next: 1,
            holds: None,
            waiting: VecDeque::new(),
            watching: Vec::new(),
            commands: 0,
            clock: 0,
        }
    }

    /// The reader's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The machine begins again, as it does when it goes on to another session: every
    /// request that waits is dropped, and the card is as it was when it got its power.
    pub fn forget(&mut self) {
        self.contexts.clear();
        self.opened.clear();
        self.holds = None;
        self.waiting.clear();
        self.watching.clear();
        self.card.reset();
    }

    /// Takes one request. Gives the answers that are ready now, which may be to
    /// requests that came before and waited, and what was asked.
    pub fn asked(&mut self, request: u32, code: u32, carried: &[u8]) -> (Vec<Answered>, Asked) {
        let mut answered = Vec::new();
        self.take(request, code, carried, &mut answered);
        // the card may be free now, for requests that waited for it
        for (request, code, carried) in std::mem::take(&mut self.waiting) {
            self.take(request, code, &carried, &mut answered);
        }
        let asked = match code {
            STARTED => Asked::WhetherThere,
            ESTABLISH | RELEASE | IS_VALID | CANCEL => Asked::ToSpeak,
            READERS_A | READERS_W => Asked::Readers,
            CHANGE_A | CHANGE_W => Asked::ForACard,
            CONNECT_A | CONNECT_W | RECONNECT | DISCONNECT | BEGIN | END => Asked::TheCard,
            TRANSMIT => Asked::ACommand,
            STATUS_A | STATUS_W | ATTRIBUTE | KEPT_A | KEPT_W | KEEP_A | KEEP_W | COMMANDS_SO_FAR => Asked::About,
            other => Asked::Other(other),
        };
        (answered, asked)
    }

    /// Time went by. Gives the answers to requests that waited no longer than this.
    pub fn elapsed(&mut self, milliseconds: u64) -> Vec<Answered> {
        self.clock += milliseconds;
        let (over, still): (Vec<Watching>, Vec<Watching>) =
            std::mem::take(&mut self.watching).into_iter().partition(|watching| watching.until.is_some_and(|until| until <= self.clock));
        self.watching = still;
        over.iter().map(|watching| Answered { request: watching.request, gives: Some(only(TIMED_OUT, 2)) }).collect()
    }

    fn take(&mut self, request: u32, code: u32, carried: &[u8], answered: &mut Vec<Answered>) {
        // how many numbers an answer of this kind has after its result
        let further = match code {
            ESTABLISH | READERS_A | READERS_W | CHANGE_A | CHANGE_W | CONTROL | ATTRIBUTE => 2,
            GROUPS_A | GROUPS_W | LOCATE_A | LOCATE_W | LOCATE_BY_WHAT_IT_SAYS_A | LOCATE_BY_WHAT_IT_SAYS_W | KEPT_A | KEPT_W | ICON => 2,
            CONNECT_A | CONNECT_W => 5,
            RECONNECT | COMMANDS_SO_FAR | KIND_OF_READER => 1,
            STATUS_A | STATUS_W => 13,
            STATE => 4,
            TRANSMIT => 3,
            _ => 0,
        };
        let gives = match self.answer(request, code, carried, answered) {
            Ok(Became::Waits) => return,
            Ok(Became::Answered(gives)) => Some(gives),
            Err(No::Refused(result)) => Some(only(result, further)),
            Err(No::NotBuilt) => None,
        };
        answered.push(Answered { request, gives });
    }

    /// A context a request names: the two numbers of it among the request's own, and
    /// behind them its bytes.
    fn context(&self, reads: &mut Reads, pointer: u32) -> Result<u32, No> {
        let context = number_of(reads.behind(pointer)?);
        if self.contexts.contains(&context) {
            Ok(context)
        } else {
            Err(No::Refused(NO_SUCH_HANDLE))
        }
    }

    /// An opening of the card a request names, behind its context.
    fn opening(&self, reads: &mut Reads, pointers: (u32, u32)) -> Result<u32, No> {
        let context = self.context(reads, pointers.0)?;
        let opening = number_of(reads.behind(pointers.1)?);
        if self.opened.contains(&(opening, context)) {
            Ok(opening)
        } else {
            Err(No::Refused(NO_SUCH_HANDLE))
        }
    }

    /// Whether the card is another opening's for now.
    fn is_anothers(&self, opening: u32) -> bool {
        self.holds.is_some_and(|(holds, _)| holds != opening)
    }

    fn closes(&mut self, opening: u32) {
        self.opened.retain(|(opened, _)| *opened != opening);
        if self.holds.is_some_and(|(holds, _)| holds == opening) {
            self.holds = None;
        }
    }

    /// The state of a reader a watcher names, as the watcher believes it: what it is
    /// now, with the mark that it changed where the watcher believes something else.
    fn state(&self, name: &str, believed: u32) -> (u32, Vec<u8>) {
        if believed & IGNORE != 0 {
            return (IGNORE, Vec::new());
        }
        if name == NOTIFICATION {
            // the readers as a whole: how many there are, in the count
            let now = 1 << 16;
            return (if believed & 0xffff_0000 == now { now } else { now | CHANGED }, Vec::new());
        }
        if name != self.name {
            return (UNKNOWN | CHANGED | IGNORE, Vec::new());
        }
        // the card is in it, and went in once
        let now = PRESENT | 1 << 16;
        let same = believed != UNAWARE && believed & !CHANGED == now;
        (if same { now } else { now | CHANGED }, self.card.says_of_itself())
    }

    fn answer(&mut self, request: u32, code: u32, carried: &[u8], answered: &mut Vec<Answered>) -> Result<Became, No> {
        // the one request that carries four bytes that mean nothing, and no heads
        if code == STARTED {
            return Ok(Became::Answered(only(0, 0)));
        }
        let mut reads = Reads::new(carried)?;
        let done = |writes: Writes| Ok(Became::Answered(writes.done()));
        match code {
            ESTABLISH => {
                reads.number()?;
                let context = self.next;
                self.next += 1;
                self.contexts.push(context);
                done(Writes::new(0).number(4).number(POINTS).counted(&context.to_le_bytes()))
            }
            RELEASE | IS_VALID | CANCEL => {
                let (_, pointer) = (reads.number()?, reads.number()?);
                let context = self.context(&mut reads, pointer)?;
                if code != IS_VALID {
                    // whoever waits in this context waits no longer
                    let (ended, still): (Vec<Watching>, Vec<Watching>) =
                        std::mem::take(&mut self.watching).into_iter().partition(|watching| watching.context == context);
                    self.watching = still;
                    answered.extend(ended.iter().map(|watching| Answered { request: watching.request, gives: Some(only(CANCELLED, 2)) }));
                }
                if code == RELEASE {
                    self.contexts.retain(|known| *known != context);
                    for (opening, _) in self.opened.clone().into_iter().filter(|(_, of)| *of == context) {
                        self.closes(opening);
                    }
                }
                done(Writes::new(0))
            }
            READERS_A | READERS_W => {
                let (_, pointer, _, groups) = (reads.number()?, reads.number()?, reads.number()?, reads.number()?);
                let (only_its_length, limit) = (reads.number()? != 0, reads.number()?);
                self.context(&mut reads, pointer)?;
                reads.behind(groups)?;
                let wide = code == READERS_W;
                let list = names(&self.name, wide);
                match limited(only_its_length, limit, if wide { 2 } else { 1 }, &list)? {
                    Some(list) => done(Writes::new(0).number(list.len() as u32).number(POINTS).counted(list)),
                    None => done(Writes::new(0).number(list.len() as u32).number(0)),
                }
            }
            CHANGE_A | CHANGE_W => {
                let (_, pointer, wait, count, _) = (reads.number()?, reads.number()?, reads.number()?, reads.number()?, reads.number()?);
                let context = self.context(&mut reads, pointer)?;
                if count > 11 || reads.number()? != count {
                    return Err(No::NotBuilt);
                }
                // each reader that is watched: whether it is named, and what the watcher believes of it
                let mut watched = Vec::new();
                for _ in 0..count {
                    let (named, believed) = (reads.number()? != 0, reads.number()?);
                    reads.take(44)?;
                    watched.push((named, believed));
                }
                let mut states = Vec::new();
                for (named, believed) in watched {
                    let name = if named { reads.text(code == CHANGE_W)? } else { String::new() };
                    states.push((believed, self.state(&name, believed)));
                }
                if !states.iter().any(|(_, (now, _))| now & CHANGED != 0) {
                    // nothing is other than the watcher believes: the card never leaves, so
                    // the watcher waits until its time is over or it is told to stop
                    if wait == 0 {
                        return Err(No::Refused(TIMED_OUT));
                    }
                    let until = (wait != FOR_GOOD).then(|| self.clock + u64::from(wait));
                    self.watching.push(Watching { request, context, until });
                    return Ok(Became::Waits);
                }
                let mut writes = Writes::new(0).number(count).number(POINTS).number(count);
                for (believed, (now, says)) in states {
                    let mut of_itself = [0u8; 36];
                    of_itself[..says.len().min(36)].copy_from_slice(&says[..says.len().min(36)]);
                    writes = writes.number(believed).number(now).number(says.len().min(36) as u32).bytes(&of_itself);
                }
                done(writes)
            }
            CONNECT_A | CONNECT_W => {
                let (named, _, pointer, _, ways) = (reads.number()?, reads.number()?, reads.number()?, reads.number()?, reads.number()?);
                let name = if named != 0 { reads.text(code == CONNECT_W)? } else { String::new() };
                let context = self.context(&mut reads, pointer)?;
                if name != self.name {
                    return Err(No::Refused(NO_SUCH_READER));
                }
                if ways & BLOCKS == 0 {
                    return Err(No::Refused(OTHER_PROTOCOL));
                }
                let opening = self.next;
                self.next += 1;
                self.opened.push((opening, context));
                done(
                    Writes::new(0)
                        .number(4)
                        .number(POINTS)
                        .number(4)
                        .number(POINTS + 4)
                        .number(BLOCKS)
                        .counted(&context.to_le_bytes())
                        .counted(&opening.to_le_bytes()),
                )
            }
            RECONNECT => {
                let pointers = (reads.number()?, reads.number()?, reads.number()?, reads.number()?);
                let (_, ways, how) = (reads.number()?, reads.number()?, reads.number()?);
                self.opening(&mut reads, (pointers.1, pointers.3))?;
                if ways & BLOCKS == 0 {
                    return Err(No::Refused(OTHER_PROTOCOL));
                }
                if how != 0 {
                    self.card.reset();
                }
                done(Writes::new(0).number(BLOCKS))
            }
            DISCONNECT | BEGIN | END => {
                let pointers = (reads.number()?, reads.number()?, reads.number()?, reads.number()?);
                let how = reads.number()?;
                let opening = self.opening(&mut reads, (pointers.1, pointers.3))?;
                match code {
                    DISCONNECT => {
                        self.closes(opening);
                        // left as it is, or its power taken and given again
                        if how != 0 {
                            self.card.reset();
                        }
                    }
                    BEGIN => {
                        if self.is_anothers(opening) {
                            self.waiting.push_back((request, code, carried.to_vec()));
                            return Ok(Became::Waits);
                        }
                        self.holds = Some((opening, self.holds.map_or(1, |(_, times)| times + 1)));
                    }
                    _ => match self.holds {
                        Some((holds, times)) if holds == opening => {
                            self.holds = (times > 1).then_some((holds, times - 1));
                            if how != 0 {
                                self.card.reset();
                            }
                        }
                        _ => return Err(No::Refused(NOT_HELD)),
                    },
                }
                done(Writes::new(0))
            }
            STATUS_A | STATUS_W => {
                let pointers = (reads.number()?, reads.number()?, reads.number()?, reads.number()?);
                let (only_its_length, limit, _) = (reads.number()? != 0, reads.number()?, reads.number()?);
                self.opening(&mut reads, (pointers.1, pointers.3))?;
                let wide = code == STATUS_W;
                let list = names(&self.name, wide);
                let given = limited(only_its_length, limit, if wide { 2 } else { 1 }, &list)?;
                let says = self.card.says_of_itself();
                let mut of_itself = [0u8; 32];
                of_itself[..says.len().min(32)].copy_from_slice(&says[..says.len().min(32)]);
                let writes = Writes::new(0)
                    .number(list.len() as u32)
                    .number(if given.is_some() { POINTS } else { 0 })
                    .number(READY)
                    .number(BLOCKS)
                    .bytes(&of_itself)
                    .number(says.len().min(32) as u32);
                done(match given {
                    Some(list) => writes.counted(list),
                    None => writes,
                })
            }
            TRANSMIT => {
                let pointers = (reads.number()?, reads.number()?, reads.number()?, reads.number()?);
                let (_, _, extra) = (reads.number()?, reads.number()?, reads.number()?);
                let (length, command, back) = (reads.number()?, reads.number()?, reads.number()?);
                let (only_its_length, limit) = (reads.number()? != 0, reads.number()?);
                let opening = self.opening(&mut reads, (pointers.1, pointers.3))?;
                if self.is_anothers(opening) {
                    self.waiting.push_back((request, code, carried.to_vec()));
                    return Ok(Became::Waits);
                }
                reads.behind(extra)?;
                let command = reads.behind(command)?;
                if command.len() != length as usize {
                    return Err(No::NotBuilt);
                }
                let answer = self.card.answer(command);
                self.commands += 1;
                if !only_its_length && (limit as usize) < answer.len() {
                    return Err(No::Refused(TOO_SMALL));
                }
                // what the way of speaking adds to an answer: nothing, and it is given back as nothing
                let _ = back;
                let writes = Writes::new(0).number(0).number(answer.len() as u32);
                done(if only_its_length { writes.number(0) } else { writes.number(POINTS).counted(&answer) })
            }
            ATTRIBUTE => {
                let pointers = (reads.number()?, reads.number()?, reads.number()?, reads.number()?);
                let (which, only_its_length, limit) = (reads.number()?, reads.number()? != 0, reads.number()?);
                self.opening(&mut reads, (pointers.1, pointers.3))?;
                let holds = match which {
                    ABOUT_WHAT_THE_CARD_SAYS => self.card.says_of_itself(),
                    ABOUT_ITS_MAKER => b"Away\0".to_vec(),
                    ABOUT_ITS_NAME_A => self.name.bytes().chain([0]).collect(),
                    ABOUT_ITS_NAME_W => self.name.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect(),
                    _ => return Err(No::Refused(NOT_DONE_HERE)),
                };
                match limited(only_its_length, limit, 1, &holds)? {
                    Some(holds) => done(Writes::new(0).number(holds.len() as u32).number(POINTS).counted(holds)),
                    None => done(Writes::new(0).number(holds.len() as u32).number(0)),
                }
            }
            // the reader keeps nothing for the machine between two sessions: what the machine
            // gives it to keep is taken and let go, and what it asks for is not there
            KEEP_A | KEEP_W => done(Writes::new(0)),
            KEPT_A | KEPT_W => Err(No::Refused(NOT_KEPT)),
            COMMANDS_SO_FAR => done(Writes::new(0).number(self.commands)),
            // what a reader with buttons or a screen of its own is asked, and whatever else
            _ => Err(No::Refused(NOT_DONE_HERE)),
        }
    }
}

/// The number a context or an opening goes by, from its bytes.
fn number_of(bytes: &[u8]) -> u32 {
    match bytes {
        [a, b, c, d] => u32::from_le_bytes([*a, *b, *c, *d]),
        _ => 0,
    }
}

/// The machine's side of it, for the tests' stand-in for one: the requests as a
/// Windows machine writes them, and the answers read.
#[cfg(any(test, feature = "stand-in"))]
pub mod stand_in {
    use super::*;

    /// A card for the tests: it says a fixed thing of itself, and answers every
    /// command with the command's own bytes and that all is well.
    pub struct Echo;

    impl Card for Echo {
        fn says_of_itself(&self) -> Vec<u8> {
            vec![0x3b, 0x80, 0x80, 0x01, 0x01]
        }

        fn answer(&mut self, command: &[u8]) -> Vec<u8> {
            [command, &[0x90, 0x00]].concat()
        }

        fn reset(&mut self) {}
    }

    /// A request's code, and what it carries.
    pub type Request = (u32, Vec<u8>);

    fn written(fixed: Writes) -> Vec<u8> {
        // a request has no result before its fields: the four bytes of one are taken off
        let whole = fixed.done();
        let mut carried = whole[..8].to_vec();
        let inside = &whole[20..];
        let length = inside.len().next_multiple_of(8);
        carried.extend((length as u32).to_le_bytes());
        carried.extend([0; 4]);
        carried.extend_from_slice(inside);
        carried.resize(16 + length, 0);
        carried
    }

    fn text(writes: Writes, text: &str, wide: bool) -> Writes {
        let count = text.encode_utf16().count() as u32 + 1;
        let writes = writes.number(count).number(0).number(count);
        if wide {
            writes.bytes(&text.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect::<Vec<u8>>())
        } else {
            writes.bytes(&text.bytes().chain([0]).collect::<Vec<u8>>())
        }
    }

    /// Whether the client's smart cards are there.
    pub fn started() -> Request {
        (STARTED, vec![0x5a; 4])
    }

    /// To begin speaking to the readers.
    pub fn establish() -> Request {
        (ESTABLISH, written(Writes::new(0).number(2)))
    }

    fn in_context(code: u32, context: u32) -> Request {
        (code, written(Writes::new(0).number(4).number(POINTS).counted(&context.to_le_bytes())))
    }

    /// To end speaking in a context.
    pub fn release(context: u32) -> Request {
        in_context(RELEASE, context)
    }

    /// Whether a context is one.
    pub fn is_valid(context: u32) -> Request {
        in_context(IS_VALID, context)
    }

    /// To stop whatever waits in a context.
    pub fn cancel(context: u32) -> Request {
        in_context(CANCEL, context)
    }

    /// Which readers there are, as a list of any length or of this many characters at most.
    pub fn readers(context: u32, wide: bool, limit: Option<u32>) -> Request {
        let writes = Writes::new(0).number(4).number(POINTS).number(0).number(0).number(0).number(limit.unwrap_or(ANY_LENGTH));
        (if wide { READERS_W } else { READERS_A }, written(writes.counted(&context.to_le_bytes())))
    }

    /// To be told when one of these readers is other than believed, within this many
    /// milliseconds, or whenever that is.
    pub fn watch(context: u32, wait: Option<u32>, believed: &[(&str, u32)]) -> Request {
        let mut writes = Writes::new(0)
            .number(4)
            .number(POINTS)
            .number(wait.unwrap_or(FOR_GOOD))
            .number(believed.len() as u32)
            .number(POINTS + 4)
            .counted(&context.to_le_bytes())
            .number(believed.len() as u32);
        for (at, (_, state)) in believed.iter().enumerate() {
            writes = writes.number(POINTS + 8 + 4 * at as u32).number(*state).number(0).number(0).bytes(&[0; 36]);
        }
        for (name, _) in believed {
            writes = text(writes, name, true);
        }
        (CHANGE_W, written(writes))
    }

    /// To open the card in a reader.
    pub fn connect(context: u32, reader: &str) -> Request {
        let writes = Writes::new(0).number(POINTS).number(4).number(POINTS + 4).number(2).number(3);
        (CONNECT_W, written(text(writes, reader, true).counted(&context.to_le_bytes())))
    }

    fn opening() -> Writes {
        Writes::new(0).number(4).number(POINTS).number(4).number(POINTS + 4)
    }

    fn behind(writes: Writes, context: u32, opening: u32) -> Writes {
        writes.counted(&context.to_le_bytes()).counted(&opening.to_le_bytes())
    }

    fn with_the_card(code: u32, context: u32, opened: u32, how: u32) -> Request {
        (code, written(behind(opening().number(how), context, opened)))
    }

    /// To close the card, leaving it as it is or taking its power.
    pub fn disconnect(context: u32, opened: u32, takes_its_power: bool) -> Request {
        with_the_card(DISCONNECT, context, opened, if takes_its_power { 2 } else { 0 })
    }

    /// To have the card for itself.
    pub fn begin(context: u32, opened: u32) -> Request {
        with_the_card(BEGIN, context, opened, 0)
    }

    /// To let others have the card again.
    pub fn end(context: u32, opened: u32) -> Request {
        with_the_card(END, context, opened, 0)
    }

    /// The reader's name and what its card says of itself.
    pub fn status(context: u32, opened: u32) -> Request {
        (STATUS_W, written(behind(opening().number(0).number(ANY_LENGTH).number(0), context, opened)))
    }

    /// A command for the card.
    pub fn transmit(context: u32, opened: u32, command: &[u8]) -> Request {
        let writes =
            opening().number(BLOCKS).number(0).number(0).number(command.len() as u32).number(POINTS + 8).number(0).number(0).number(2048);
        (TRANSMIT, written(behind(writes, context, opened).counted(command)))
    }

    /// Something about the reader or the card, by its number.
    pub fn attribute(context: u32, opened: u32, which: u32) -> Request {
        (ATTRIBUTE, written(behind(opening().number(which).number(0).number(ANY_LENGTH), context, opened)))
    }

    /// What the card says of itself, among what can be asked about.
    pub const WHAT_THE_CARD_SAYS: u32 = ABOUT_WHAT_THE_CARD_SAYS;

    /// An answer, read: its result, and the numbers and bytes after it.
    pub struct Answer {
        /// The result: zero where all is well.
        pub result: u32,
        inside: Vec<u8>,
    }

    /// Reads an answer.
    pub fn read(gives: &[u8]) -> Result<Answer, Error> {
        let mut reads = Reads::new(gives)?;
        let result = reads.number()?;
        Ok(Answer { result, inside: reads.bytes[4..].to_vec() })
    }

    impl Answer {
        fn number(&self, at: usize) -> u32 {
            self.inside.get(4 * at..4 * at + 4).map_or(0, |four| u32::from_le_bytes([four[0], four[1], four[2], four[3]]))
        }

        /// The context an answer to beginning gives.
        pub fn context(&self) -> u32 {
            self.number(3)
        }

        /// The opening an answer to opening the card gives.
        pub fn opened(&self) -> u32 {
            self.number(8)
        }

        /// The bytes an answer carries behind its one pointer: a list of names, a
        /// card's answer, or something about the reader.
        pub fn carried(&self) -> Vec<u8> {
            // the length and the pointer are the last two numbers before what is carried
            let at = if self.number(1) == POINTS { 0 } else { 1 };
            let length = self.number(at) as usize;
            self.inside.get(4 * (at + 3)..4 * (at + 3) + length).unwrap_or_default().to_vec()
        }

        /// The states of the readers that were watched: for each, what it is now.
        pub fn states(&self) -> Vec<u32> {
            (0..self.number(0) as usize).map(|at| self.number(3 + 12 * at + 1)).collect()
        }

        /// The names and what the card says of itself, from an answer about the card.
        pub fn about_the_card(&self) -> (Vec<u8>, Vec<u8>) {
            let (length, says) = (self.number(0) as usize, self.number(12) as usize);
            (self.inside.get(56..56 + length).unwrap_or_default().to_vec(), self.inside.get(16..16 + says).unwrap_or_default().to_vec())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::stand_in::{self, Echo};
    use super::*;

    const NAME: &str = "Away 0";

    fn reader() -> Reader {
        Reader::new(NAME, Box::new(Echo))
    }

    /// One request, answered at once.
    fn asks(reader: &mut Reader, request: stand_in::Request) -> stand_in::Answer {
        let (answered, _) = reader.asked(7, request.0, &request.1);
        assert_eq!(answered.len(), 1, "one answer");
        assert_eq!(answered[0].request, 7);
        stand_in::read(answered[0].gives.as_deref().expect("an answer that is built")).expect("an answer")
    }

    fn wide(text: &str) -> Vec<u8> {
        text.encode_utf16().flat_map(u16::to_le_bytes).collect()
    }

    #[test]
    fn an_answer_is_written_as_the_specification_writes_one() {
        // the answer to beginning: the two heads, the result, and a context of four bytes behind a pointer
        let mut reader = reader();
        let (answered, asked) = reader.asked(1, stand_in::establish().0, &stand_in::establish().1);
        assert_eq!(asked, Asked::ToSpeak);
        assert_eq!(
            answered[0].gives.as_deref(),
            Some(
                &[
                    0x01, 0x10, 0x08, 0x00, 0xcc, 0xcc, 0xcc, 0xcc, 0x18, 0, 0, 0, 0, 0, 0, 0, // the heads: 24 bytes follow
                    0, 0, 0, 0, 4, 0, 0, 0, 0x00, 0x00, 0x02, 0x00, // all is well; four bytes, behind a pointer
                    4, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, // their count, the bytes, and what fills it up to eights
                ][..]
            )
        );
        // the request itself, as a Windows machine writes it: the heads, and the kind of context
        assert_eq!(
            stand_in::establish().1,
            [0x01, 0x10, 0x08, 0x00, 0xcc, 0xcc, 0xcc, 0xcc, 8, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0]
        );
        // whether smart cards are there is asked with four bytes that mean nothing
        let (answered, asked) = reader.asked(2, stand_in::started().0, &stand_in::started().1);
        assert_eq!(asked, Asked::WhetherThere);
        assert_eq!(answered[0].gives.as_deref().map(<[u8]>::len), Some(24));
    }

    #[test]
    fn the_machine_finds_the_reader_and_the_card_in_it() {
        let mut reader = reader();
        let context = asks(&mut reader, stand_in::establish()).context();
        assert_eq!(asks(&mut reader, stand_in::is_valid(context)).result, 0);
        assert_eq!(asks(&mut reader, stand_in::is_valid(context + 1)).result, NO_SUCH_HANDLE);

        // the readers, in wide characters and in narrow ones, and where the caller has too little room
        assert_eq!(asks(&mut reader, stand_in::readers(context, true, None)).carried(), wide("Away 0\0\0"));
        assert_eq!(asks(&mut reader, stand_in::readers(context, false, None)).carried(), b"Away 0\0\0");
        assert_eq!(asks(&mut reader, stand_in::readers(context, true, Some(8))).carried(), wide("Away 0\0\0"));
        assert_eq!(asks(&mut reader, stand_in::readers(context, true, Some(7))).result, TOO_SMALL);
        let length_alone = asks(&mut reader, stand_in::readers(context, true, Some(0)));
        assert_eq!((length_alone.result, length_alone.carried()), (0, Vec::new()));

        // who knows nothing of the readers is told at once: one reader there is, and a card is in it
        let first = asks(&mut reader, stand_in::watch(context, None, &[(NOTIFICATION, 0), (NAME, 0)]));
        assert_eq!(first.states(), [0x0001_0002, 0x0001_0022]);
        // a reader that is not there is said to be unknown
        assert_eq!(asks(&mut reader, stand_in::watch(context, None, &[("Another 0", 0)])).states(), [0x0007]);

        // the card is opened, says what it is, and answers a command
        let opened = asks(&mut reader, stand_in::connect(context, NAME)).opened();
        assert_eq!(asks(&mut reader, stand_in::connect(context, "Another 0")).result, NO_SUCH_READER);
        let (names, says) = asks(&mut reader, stand_in::status(context, opened)).about_the_card();
        assert_eq!((names, says), (wide("Away 0\0\0"), vec![0x3b, 0x80, 0x80, 0x01, 0x01]));
        assert_eq!(
            asks(&mut reader, stand_in::attribute(context, opened, stand_in::WHAT_THE_CARD_SAYS)).carried(),
            [0x3b, 0x80, 0x80, 0x01, 0x01]
        );
        assert_eq!(asks(&mut reader, stand_in::attribute(context, opened, 0x0001_0102)).result, NOT_DONE_HERE);
        assert_eq!(asks(&mut reader, stand_in::begin(context, opened)).result, 0);
        assert_eq!(
            asks(&mut reader, stand_in::transmit(context, opened, &[0x00, 0xa4, 0x04, 0x00])).carried(),
            [0x00, 0xa4, 0x04, 0x00, 0x90, 0x00]
        );
        assert_eq!(asks(&mut reader, stand_in::end(context, opened)).result, 0);
        assert_eq!(asks(&mut reader, stand_in::end(context, opened)).result, NOT_HELD);
        assert_eq!(asks(&mut reader, stand_in::disconnect(context, opened, false)).result, 0);
        // the card is closed: nothing more is asked of it under that opening
        assert_eq!(asks(&mut reader, stand_in::transmit(context, opened, &[0])).result, NO_SUCH_HANDLE);
        assert_eq!(asks(&mut reader, stand_in::release(context)).result, 0);
        assert_eq!(asks(&mut reader, stand_in::readers(context, true, None)).result, NO_SUCH_HANDLE);
    }

    #[test]
    fn who_watches_a_reader_that_does_not_change_waits_until_told_to_stop_or_until_the_time_is_over() {
        let mut reader = reader();
        let context = asks(&mut reader, stand_in::establish()).context();
        let believed = [(NOTIFICATION, 0x0001_0000), (NAME, 0x0001_0020)];
        // nothing is other than believed: no answer, for good
        let watch = stand_in::watch(context, None, &believed);
        assert_eq!(reader.asked(11, watch.0, &watch.1).0, []);
        assert_eq!(reader.elapsed(3_600_000), []);
        // another watches for a second
        let watch = stand_in::watch(context, Some(1000), &believed);
        assert_eq!(reader.asked(12, watch.0, &watch.1).0, []);
        assert_eq!(reader.elapsed(999), []);
        let over = reader.elapsed(1);
        assert_eq!(over.len(), 1);
        assert_eq!((over[0].request, stand_in::read(over[0].gives.as_deref().unwrap()).unwrap().result), (12, TIMED_OUT));
        // who has no time at all is told so at once
        assert_eq!(asks(&mut reader, stand_in::watch(context, Some(0), &believed)).result, TIMED_OUT);
        // the first is told to stop: it is answered before the request that stopped it
        let cancel = stand_in::cancel(context);
        let (answered, _) = reader.asked(13, cancel.0, &cancel.1);
        assert_eq!(answered.iter().map(|answered| answered.request).collect::<Vec<_>>(), [11, 13]);
        assert_eq!(stand_in::read(answered[0].gives.as_deref().unwrap()).unwrap().result, CANCELLED);
        assert_eq!(stand_in::read(answered[1].gives.as_deref().unwrap()).unwrap().result, 0);
    }

    #[test]
    fn the_card_is_one_openings_at_a_time_and_the_others_wait_for_it() {
        let mut reader = reader();
        let context = asks(&mut reader, stand_in::establish()).context();
        let first = asks(&mut reader, stand_in::connect(context, NAME)).opened();
        let second = asks(&mut reader, stand_in::connect(context, NAME)).opened();
        assert_ne!(first, second);
        assert_eq!(asks(&mut reader, stand_in::begin(context, first)).result, 0);
        // the second wants the card for itself, and gives it a command: both wait
        let begin = stand_in::begin(context, second);
        assert_eq!(reader.asked(21, begin.0, &begin.1).0, []);
        let command = stand_in::transmit(context, second, &[0x02]);
        assert_eq!(reader.asked(22, command.0, &command.1).0, []);
        // the first goes on meanwhile, and lets the card go: what waited is answered, in its order
        assert_eq!(asks(&mut reader, stand_in::transmit(context, first, &[0x01])).carried(), [0x01, 0x90, 0x00]);
        let end = stand_in::end(context, first);
        let (answered, _) = reader.asked(23, end.0, &end.1);
        assert_eq!(answered.iter().map(|answered| answered.request).collect::<Vec<_>>(), [23, 21, 22]);
        assert_eq!(stand_in::read(answered[2].gives.as_deref().unwrap()).unwrap().carried(), [0x02, 0x90, 0x00]);
        // now the first waits, until the second closes the card without letting it go first
        let command = stand_in::transmit(context, first, &[0x03]);
        assert_eq!(reader.asked(24, command.0, &command.1).0, []);
        let close = stand_in::disconnect(context, second, true);
        let (answered, _) = reader.asked(25, close.0, &close.1);
        assert_eq!(answered.iter().map(|answered| answered.request).collect::<Vec<_>>(), [25, 24]);
    }

    #[test]
    fn a_request_that_is_not_built_as_one_is_said_not_to_have_worked_and_one_not_done_here_is_refused() {
        let mut reader = reader();
        let context = asks(&mut reader, stand_in::establish()).context();
        // cut anywhere, a request gets an answer of one kind or the other, and nothing breaks
        let opened = asks(&mut reader, stand_in::connect(context, NAME)).opened();
        for request in [
            stand_in::watch(context, None, &[(NAME, 0)]),
            stand_in::transmit(context, opened, &[1, 2, 3]),
            stand_in::connect(context, NAME),
        ] {
            for cut in 0..request.1.len() {
                let (answered, _) = reader.asked(31, request.0, &request.1[..cut]);
                assert_eq!(answered.len(), 1, "cut at {cut}");
            }
        }
        let (answered, _) = reader.asked(32, TRANSMIT, &[0; 8]);
        assert_eq!(answered, [Answered { request: 32, gives: None }]);
        // a request this reader does not do, and one for what it never kept
        let (answered, asked) = reader.asked(33, ICON, &stand_in::is_valid(context).1);
        assert_eq!(asked, Asked::Other(ICON));
        assert_eq!(answered[0].gives, Some(only(NOT_DONE_HERE, 2)));
        let (answered, asked) = reader.asked(34, KEPT_W, &stand_in::is_valid(context).1);
        assert_eq!((asked, answered[0].gives.clone()), (Asked::About, Some(only(NOT_KEPT, 2))));
        let (answered, _) = reader.asked(35, KEEP_W, &stand_in::is_valid(context).1);
        assert_eq!(answered[0].gives, Some(only(0, 0)));
        // and when the machine begins again, nothing of before is known
        reader.forget();
        assert_eq!(asks(&mut reader, stand_in::is_valid(context)).result, NO_SUCH_HANDLE);
    }
}
