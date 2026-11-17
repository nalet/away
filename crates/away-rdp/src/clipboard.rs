//! The clipboard, shared between the person's device and the machine (MS-RDPECLIP), as
//! far as text, pictures and the machine's files go. It travels on a further channel of
//! its own.
//!
//! Each side says when its clipboard holds something new, by the kinds it holds it in,
//! and the other side asks for it in the kind it wants. Away takes text, files where
//! there is no text, and a picture where there is neither: what the machine copies is
//! asked for at once, and what the person copied is handed over when the machine asks
//! for it. Of files that were copied on the machine their names and sizes are asked for
//! at once, and what is in one is asked for a piece at a time, when the person wants
//! it. Each direction passes only where the installation lets it.

use std::collections::VecDeque;

use crate::Error;

/// The further channel the clipboard travels on.
pub const CHANNEL: &str = "cliprdr";

/// The longest text that passes, in the bytes it travels in: two for most characters.
pub const LONGEST: usize = 512 * 1024;
/// The largest picture that passes, in the bytes it travels in: four for a pixel, which
/// is a screen of 3840 by 2160 pixels and some more.
pub const LARGEST: usize = 48 * 1024 * 1024;
/// How many pixels a picture is wide or high at most.
const MOST_ACROSS: usize = 16_384;

const READY: u16 = 0x0001;
const HOLDS: u16 = 0x0002;
const TOOK: u16 = 0x0003;
const ASKS: u16 = 0x0004;
const GIVES: u16 = 0x0005;
const CAN_DO: u16 = 0x0007;
const ASKS_FOR_A_FILE: u16 = 0x0008;
const GIVES_A_FILE: u16 = 0x0009;
const KEEPS: u16 = 0x000a;
const LETS_GO: u16 = 0x000b;

const WORKED: u16 = 0x0001;
const FAILED: u16 = 0x0002;

/// Text, as Windows numbers the kinds a clipboard holds: in units of sixteen bits.
const TEXT: u32 = 13;
/// A picture, as Windows numbers the kinds: a head that says how it is written, then
/// its pixels.
const PICTURE: u32 = 8;
/// How a picture's pixels are written: as they are, or by three masks that say where
/// in a pixel each colour is.
const AS_THEY_ARE: u32 = 0;
const BY_MASKS: u32 = 3;
/// What a side says it can do: that it writes the kinds' names at their full length.
const LONG_NAMES: u32 = 0x0000_0002;
/// That it hands files over as what is in them, a piece at a time, and names none by
/// where it lies on its own disk.
const FILES_IN_PIECES: u32 = 0x0000_0004 | 0x0000_0008;
/// That it keeps what was copied for as long as the other side says, also once
/// something else was copied.
const KEEPS_WHAT_WAS_COPIED: u32 = 0x0000_0010;
/// That files of more than four thousand million bytes pass.
const LARGE_FILES: u32 = 0x0000_0020;
/// The name of the kind that lists the files that were copied.
const LIST_OF_FILES: &str = "FileGroupDescriptorW";
/// The name of the kind that is what is in those files, and the numbers the client
/// gives the two kinds where it holds files itself.
const WHAT_IS_IN_FILES: &str = "FileContents";
const THEIR_LIST: u32 = 0xc0f1;
const WHAT_IS_IN_THEIRS: u32 = 0xc0f2;
/// The name of the kind that is text with its formatting, written as a page of the web
/// is, and the number the client gives it where it holds such text itself.
const FORMATTING: &str = "HTML Format";
const THEIR_FORMATTING: u32 = 0xc0f3;
/// How long the entry of one file in that list is, where its name begins in it, and
/// the mark of a folder among what a file is.
const ONE_FILE: usize = 592;
const NAME_AT: usize = 72;
const A_FOLDER: u32 = 0x0000_0010;
/// The most files of one copy that are listed.
pub const MOST_FILES: usize = 4096;
/// The longest piece of a file that is asked for at once.
pub const LONGEST_PIECE: u32 = 256 * 1024;

/// Which ways the clipboard passes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Passes {
    /// What the person copied can be pasted on the machine.
    pub to_the_machine: bool,
    /// What is copied on the machine reaches the person's device.
    pub from_the_machine: bool,
}

impl Passes {
    /// Both ways.
    pub const BOTH: Passes = Passes { to_the_machine: true, from_the_machine: true };
    /// Neither way: the channel is not asked for.
    pub const NEITHER: Passes = Passes { to_the_machine: false, from_the_machine: false };

    /// Whether it passes at all.
    pub fn at_all(self) -> bool {
        self.to_the_machine || self.from_the_machine
    }
}

/// A picture as a clipboard holds one: its pixels from the top row down, each as red,
/// green and blue.
#[derive(Clone, PartialEq, Eq)]
pub struct Picture {
    /// How many pixels wide.
    pub width: usize,
    /// How many high.
    pub height: usize,
    /// Three bytes a pixel.
    pub pixels: Vec<u8>,
}

impl std::fmt::Debug for Picture {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "a picture of {} by {} pixels", self.width, self.height)
    }
}

/// A file that was copied on the machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct File {
    /// Its name, after the folders it lies in among what was copied, each followed by a
    /// slash. Nothing in it leads out of what was copied.
    pub name: String,
    /// How many bytes it holds, as the machine says.
    pub size: u64,
    /// Whether it is a folder, which holds nothing itself.
    pub folder: bool,
}

/// What a clipboard carries from one side to the other.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Carried {
    /// Text.
    Text(String),
    /// A picture.
    Picture(Picture),
    /// The files that were copied on the machine: which they are, and nothing of what
    /// is in them.
    Files(Vec<File>),
    /// A piece of a file that was asked for. An empty one: the file ends before there.
    Piece(Vec<u8>),
    /// The machine did not give the piece that was asked for.
    NoPiece,
    /// The formatting of the text that was copied on the machine, written as a page of
    /// the web is: it comes after the text itself, where the machine holds both.
    Formatting(String),
    /// The machine pastes a file the person copied, and wants a piece of it: the file by
    /// its place among those the person copied, the piece by where it begins and how
    /// long it is. The answer is given under the number.
    Wants {
        /// The number the answer is given under.
        number: u32,
        /// The file.
        file: u32,
        /// Where the piece begins.
        from: u64,
        /// How long it is.
        length: u32,
    },
}

impl Carried {
    /// The text, where it is text.
    pub fn text(&self) -> Option<&str> {
        match self {
            Carried::Text(text) => Some(text),
            _ => None,
        }
    }
}

/// The clipboard of one connection, as far as the machine and the client are with it.
#[derive(Debug)]
pub struct Clipboard {
    passes: Passes,
    /// Whether the machine writes the kinds' names at their full length.
    long_names: bool,
    /// Whether the machine said that it is ready.
    ready: bool,
    /// What the person copied, for the machine to ask for.
    theirs: Option<Carried>,
    /// The kinds the client asked the machine for and waits for, in the order it asked:
    /// the machine answers in that order. One that is no longer wanted, because
    /// something newer was copied on either side since, waits there as nothing, so that
    /// its answer is taken for no other.
    asked: VecDeque<Option<u32>>,
    /// Whether the files that are copied on the machine pass to the person.
    files: bool,
    /// Whether the machine keeps what was copied for as long as it is told to.
    machine_keeps: bool,
    /// The number the machine's list of files has in what it holds now.
    list: Option<u32>,
    /// The number under which the machine keeps the files that were listed last.
    kept: Option<u32>,
    /// How many times the machine was told to keep files, and how many pieces were asked for.
    keeps: u32,
    pieces: u32,
    /// The piece of a file the client asked for and waits for, by its number.
    asked_for_a_piece: Option<u32>,
    /// The number the machine gives the formatting of the text it holds now.
    formatting: Option<u32>,
    /// The formatting of the text the person copied, where it has some.
    their_formatting: Option<String>,
}

/// A message of the clipboard's channel: its kind, how it went, and what it holds.
fn message(kind: u16, how: u16, holds: &[u8]) -> Vec<u8> {
    let mut message = Vec::with_capacity(holds.len() + 8);
    message.extend(kind.to_le_bytes());
    message.extend(how.to_le_bytes());
    message.extend((holds.len() as u32).to_le_bytes());
    message.extend_from_slice(holds);
    message
}

/// Text as the machine holds it: in units of sixteen bits, lines ended as Windows ends
/// them, and a zero after the last.
fn for_the_machine(text: &str) -> Vec<u8> {
    let mut units = Vec::with_capacity(text.len() * 2 + 2);
    let mut before = '\0';
    for character in text.chars() {
        if character == '\n' && before != '\r' {
            units.extend(0x000du16.to_le_bytes());
        }
        let mut pair = [0u16; 2];
        for unit in character.encode_utf16(&mut pair) {
            units.extend(unit.to_le_bytes());
        }
        before = character;
    }
    units.extend([0, 0]);
    units
}

/// Text as the person's device holds it: up to the first zero, with lines ended by
/// one character.
fn for_the_person(held: &[u8]) -> String {
    let units: Vec<u16> = held.as_chunks::<2>().0.iter().map(|pair| u16::from_le_bytes(*pair)).take_while(|unit| *unit != 0).collect();
    String::from_utf16_lossy(&units).replace("\r\n", "\n")
}

/// A picture as the machine holds one: a head of forty bytes, then the pixels from the
/// bottom row up, each as blue, green, red and a byte that says it is not seen through.
fn picture_for_the_machine(picture: &Picture) -> Vec<u8> {
    let (width, height) = (picture.width, picture.height);
    let mut held = Vec::with_capacity(40 + width * height * 4);
    held.extend(40u32.to_le_bytes());
    held.extend((width as u32).to_le_bytes());
    held.extend((height as u32).to_le_bytes());
    held.extend(1u16.to_le_bytes());
    held.extend(32u16.to_le_bytes());
    held.extend(AS_THEY_ARE.to_le_bytes());
    held.extend(((width * height * 4) as u32).to_le_bytes());
    held.extend([0; 16]);
    for row in (0..height).rev() {
        for pixel in picture.pixels[row * width * 3..][..width * 3].as_chunks::<3>().0 {
            held.extend([pixel[2], pixel[1], pixel[0], 0xff]);
        }
    }
    held
}

/// A picture the machine holds, as the person's device takes one. Nothing for one that
/// is written in a way Away doesn't read, that is cut, or that is larger than a picture
/// is: with 24 or 32 bits a pixel it is read, with its rows from the bottom up or from
/// the top down.
fn picture_for_the_person(held: &[u8]) -> Option<Picture> {
    let four = |at: usize| held.get(at..at + 4).map(|four| u32::from_le_bytes([four[0], four[1], four[2], four[3]]));
    let head = four(0)? as usize;
    let (across, down) = (four(4)? as i32, four(8)? as i32);
    let bits = usize::from(u16::from_le_bytes([*held.get(14)?, *held.get(15)?]));
    let (written, colours) = (four(16)?, four(32)? as usize);
    let (width, height) = (usize::try_from(across).ok()?, down.unsigned_abs() as usize);
    if head < 40 || width == 0 || height == 0 || width > MOST_ACROSS || height > MOST_ACROSS || colours > 256 {
        return None;
    }
    // where in a pixel each colour is: its byte, counted from the first
    let places: [usize; 3] = match (bits, written) {
        (24 | 32, AS_THEY_ARE) => [2, 1, 0],
        (32, BY_MASKS) => {
            let mut places = [0; 3];
            for (nth, place) in places.iter_mut().enumerate() {
                let mask = four(40 + nth * 4)?;
                *place = [0x0000_00ffu32, 0x0000_ff00, 0x00ff_0000, 0xff00_0000].iter().position(|byte| *byte == mask)?;
            }
            places
        }
        _ => return None,
    };
    // the pixels follow the head, the three masks where the head is the short one, and
    // the colours of a table, which such a picture seldom has
    let begins = head + if written == BY_MASKS && head == 40 { 12 } else { 0 } + colours * 4;
    let (each, row) = (bits / 8, (width * bits).div_ceil(32) * 4);
    let rows = held.get(begins..begins.checked_add(row.checked_mul(height)?)?)?;
    let mut pixels = Vec::with_capacity(width * height * 3);
    for nth in 0..height {
        let from = if down > 0 { height - 1 - nth } else { nth } * row;
        for pixel in rows[from..from + width * each].chunks_exact(each) {
            pixels.extend(places.map(|place| pixel[place]));
        }
    }
    Some(Picture { width, height, pixels })
}

/// What is written before and after formatted text, for a machine to know it as a page.
const BEFORE_IT: &str = "<html>\r\n<body>\r\n<!--StartFragment-->";
const AFTER_IT: &str = "<!--EndFragment-->\r\n</body>\r\n</html>";

/// Formatted text as a machine holds it: a head that says where the page and the text
/// in it begin and end, counted in bytes from the head's first, and then the page. Each
/// number is ten digits long, so that the head's length doesn't depend on them.
fn formatting_for_the_machine(formatted: &str) -> Vec<u8> {
    let head = |page: usize, ends: usize, text: usize, text_ends: usize| {
        format!("Version:0.9\r\nStartHTML:{page:010}\r\nEndHTML:{ends:010}\r\nStartFragment:{text:010}\r\nEndFragment:{text_ends:010}\r\n")
    };
    let page = head(0, 0, 0, 0).len();
    let text = page + BEFORE_IT.len();
    let text_ends = text + formatted.len();
    let mut held = head(page, text_ends + AFTER_IT.len(), text, text_ends).into_bytes();
    held.extend([BEFORE_IT, formatted, AFTER_IT].concat().as_bytes());
    held.push(0);
    held
}

/// Formatted text a machine holds, as the person's device takes it: the part of the
/// page that the head says is the text. Nothing where the head doesn't say so.
fn formatting_for_the_person(held: &[u8]) -> Option<String> {
    let head = &held[..held.len().min(1024)];
    let number = |name: &[u8]| {
        let at = head.windows(name.len()).position(|here| here == name)? + name.len();
        let digits: String = head[at..].iter().take_while(|digit| digit.is_ascii_digit()).map(|digit| char::from(*digit)).collect();
        digits.parse::<usize>().ok()
    };
    let (from, to) = (number(b"StartFragment:")?, number(b"EndFragment:")?);
    Some(String::from_utf8_lossy(held.get(from..to)?).into_owned())
}

/// The files the person copied, as a machine takes their list: each by its own name,
/// with nothing in it that a machine reads as a folder or a drive, and its size.
fn files_for_the_machine(files: &[File]) -> Vec<u8> {
    let mut held = (files.len() as u32).to_le_bytes().to_vec();
    for file in files {
        let mut entry = [0u8; ONE_FILE];
        // it says what the file is and how large, and that the machine shows how far it is
        entry[..4].copy_from_slice(&0x0000_4044u32.to_le_bytes());
        entry[36..40].copy_from_slice(&if file.folder { A_FOLDER } else { 0x80 }.to_le_bytes());
        entry[64..68].copy_from_slice(&((file.size >> 32) as u32).to_le_bytes());
        entry[68..72].copy_from_slice(&(file.size as u32).to_le_bytes());
        let name = file.name.chars().map(|one| if one < ' ' || "\\/:*?\"<>|".contains(one) { '_' } else { one });
        for (at, unit) in name.collect::<String>().encode_utf16().take(259).enumerate() {
            entry[NAME_AT + at * 2..][..2].copy_from_slice(&unit.to_le_bytes());
        }
        held.extend(entry);
    }
    held
}

/// The files a machine lists, as the person's device takes them. An entry whose name
/// says nothing is left out, and so is what is beyond the most files that are listed.
fn files_for_the_person(held: &[u8]) -> Option<Vec<File>> {
    let four = |from: &[u8], at: usize| from.get(at..at + 4).map(|four| u32::from_le_bytes([four[0], four[1], four[2], four[3]]));
    let count = four(held, 0)? as usize;
    let entries = held.get(4..4usize.checked_add(count.checked_mul(ONE_FILE)?)?)?;
    let mut files = Vec::new();
    for entry in entries.as_chunks::<ONE_FILE>().0.iter().take(MOST_FILES) {
        let units: Vec<u16> =
            entry[NAME_AT..].as_chunks::<2>().0.iter().map(|pair| u16::from_le_bytes(*pair)).take_while(|unit| *unit != 0).collect();
        // the folders a file lies in, and its name: nothing that leads elsewhere
        let parts: Vec<String> = String::from_utf16_lossy(&units)
            .split(['\\', '/'])
            .filter(|part| !part.is_empty() && *part != "." && *part != ".." && !part.contains(':'))
            .map(str::to_string)
            .collect();
        if parts.is_empty() {
            continue;
        }
        let size = (u64::from(four(entry, 64)?) << 32) | u64::from(four(entry, 68)?);
        files.push(File { name: parts.join("/"), size, folder: four(entry, 36)? & A_FOLDER != 0 });
    }
    Some(files)
}

impl Clipboard {
    /// A clipboard that passes these ways.
    pub fn new(passes: Passes) -> Clipboard {
        Clipboard {
            passes,
            long_names: false,
            ready: false,
            theirs: None,
            asked: VecDeque::new(),
            files: false,
            machine_keeps: false,
            list: None,
            kept: None,
            keeps: 0,
            pieces: 0,
            asked_for_a_piece: None,
            formatting: None,
            their_formatting: None,
        }
    }

    /// The same, with the files that are copied on the machine passing to the person,
    /// where the clipboard passes from the machine at all.
    pub fn with_files(self) -> Clipboard {
        Clipboard { files: self.passes.from_the_machine, ..self }
    }

    /// What the client says it holds: text or a picture, where the person copied one
    /// that may pass to the machine, and nothing otherwise.
    fn holds(&self) -> Vec<u8> {
        let mut kinds = Vec::new();
        let kind = match (&self.theirs, self.passes.to_the_machine) {
            // text with its formatting is held as both: the formatting by its name
            (Some(Carried::Text(_)), true) if self.their_formatting.is_some() && self.long_names => {
                kinds.extend(THEIR_FORMATTING.to_le_bytes());
                kinds.extend(FORMATTING.encode_utf16().flat_map(u16::to_le_bytes));
                kinds.extend([0, 0]);
                Some(TEXT)
            }
            (Some(Carried::Text(_)), true) => Some(TEXT),
            (Some(Carried::Picture(_)), true) => Some(PICTURE),
            // files are held as their list and as what is in them, each kind by its name
            (Some(Carried::Files(_)), true) if self.long_names => {
                for (kind, name) in [(THEIR_LIST, LIST_OF_FILES), (WHAT_IS_IN_THEIRS, WHAT_IS_IN_FILES)] {
                    kinds.extend(kind.to_le_bytes());
                    kinds.extend(name.encode_utf16().flat_map(u16::to_le_bytes));
                    kinds.extend([0, 0]);
                }
                None
            }
            _ => None,
        };
        if let Some(kind) = kind {
            kinds.extend(kind.to_le_bytes());
            // the kind has no name: at full length that is one zero, otherwise 32 bytes of them
            kinds.extend(if self.long_names { vec![0; 2] } else { vec![0; 32] });
        }
        message(HOLDS, 0, &kinds)
    }

    /// Takes one message of the channel. Gives what to answer, each a message of the
    /// channel, and what was copied on the machine, where this message brought it.
    pub fn receive(&mut self, arrived: &[u8]) -> Result<(Vec<Vec<u8>>, Option<Carried>), Error> {
        const WRONG: Error = Error::NotRdp("a message about the clipboard is not built as one");
        let two = |at: usize| arrived.get(at..at + 2).map(|two| u16::from_le_bytes([two[0], two[1]]));
        let four = |from: &[u8], at: usize| from.get(at..at + 4).map(|four| u32::from_le_bytes([four[0], four[1], four[2], four[3]]));
        let (kind, how, length) = (two(0).ok_or(WRONG)?, two(2).ok_or(WRONG)?, four(arrived, 4).ok_or(WRONG)? as usize);
        let holds = arrived.get(8..8 + length).ok_or(WRONG)?;
        match kind {
            // what the machine can do: whether it writes the kinds' names at full length
            CAN_DO => {
                let mut sets = holds.get(4..).ok_or(WRONG)?;
                while sets.len() >= 4 {
                    let (set, length) = (u16::from_le_bytes([sets[0], sets[1]]), usize::from(u16::from_le_bytes([sets[2], sets[3]])));
                    let whole = sets.get(..length).filter(|whole| whole.len() >= 4).ok_or(WRONG)?;
                    if set == 1 {
                        let can = four(whole, 8).ok_or(WRONG)?;
                        self.long_names = can & LONG_NAMES != 0;
                        self.machine_keeps = can & KEEPS_WHAT_WAS_COPIED != 0;
                    }
                    sets = &sets[length..];
                }
                Ok((Vec::new(), None))
            }
            // the machine is ready: the client says what it can do, as the machine does
            // it, and what it holds
            READY => {
                self.ready = true;
                let mut can = vec![1, 0, 0, 0, 1, 0, 12, 0, 2, 0, 0, 0];
                // files pass where the machine names its kinds at full length: the list
                // of files has a name that is longer than a short one
                let of_files = if self.files && self.long_names { FILES_IN_PIECES | KEEPS_WHAT_WAS_COPIED | LARGE_FILES } else { 0 };
                can.extend((if self.long_names { LONG_NAMES } else { 0 } | of_files).to_le_bytes());
                Ok((vec![message(CAN_DO, 0, &can), self.holds()], None))
            }
            // the machine's clipboard holds something new: the client confirms, and asks
            // for it at once where it may pass: for its text, or for its picture where it
            // holds no text
            HOLDS => {
                let mut answers = vec![message(TOOK, WORKED, &[])];
                let mut kinds = holds;
                let (mut text, mut picture, mut list, mut formatting) = (false, false, None, None);
                while kinds.len() >= 4 {
                    text |= four(kinds, 0) == Some(TEXT);
                    picture |= four(kinds, 0) == Some(PICTURE);
                    // past the kind's name: at full length up to its zero, otherwise 32 bytes
                    let name = match self.long_names {
                        true => {
                            let units = kinds[4..].as_chunks::<2>().0.iter().position(|unit| *unit == [0, 0]).ok_or(WRONG)?;
                            let is =
                                |name: &str| name.encode_utf16().flat_map(u16::to_le_bytes).eq(kinds[4..4 + units * 2].iter().copied());
                            if is(LIST_OF_FILES) {
                                list = four(kinds, 0);
                            }
                            if is(FORMATTING) {
                                formatting = four(kinds, 0);
                            }
                            units * 2 + 2
                        }
                        false => 32,
                    };
                    kinds = kinds.get(4 + name..).ok_or(WRONG)?;
                }
                // the files that were listed before are no longer what the machine holds:
                // it need not keep them, and no piece of them is waited for
                if let Some(kept) = self.kept.take() {
                    answers.push(message(LETS_GO, 0, &kept.to_le_bytes()));
                }
                self.asked_for_a_piece = None;
                self.list = list.filter(|_| self.files);
                // what was asked for before is of what the machine held before
                self.asked.iter_mut().for_each(|asked| *asked = None);
                let wanted = match (text, self.list, picture) {
                    _ if !self.passes.from_the_machine => None,
                    (true, ..) => Some(TEXT),
                    (false, Some(list), _) => Some(list),
                    (false, None, true) => Some(PICTURE),
                    (false, None, false) => None,
                };
                if let Some(kind) = wanted {
                    self.asked.push_back(Some(kind));
                    answers.push(message(ASKS, 0, &kind.to_le_bytes()));
                }
                // text that has formatting too: the formatting is asked for after the text
                self.formatting = formatting.filter(|_| wanted == Some(TEXT));
                if let Some(kind) = self.formatting {
                    self.asked.push_back(Some(kind));
                    answers.push(message(ASKS, 0, &kind.to_le_bytes()));
                }
                Ok((answers, None))
            }
            // the machine pastes: it is handed what the person copied, in the kind the
            // client said it holds, where that may pass
            ASKS => {
                let handed = match (four(holds, 0), &self.theirs) {
                    (Some(TEXT), Some(Carried::Text(theirs))) if self.passes.to_the_machine => Some(for_the_machine(theirs)),
                    (Some(PICTURE), Some(Carried::Picture(theirs))) if self.passes.to_the_machine => Some(picture_for_the_machine(theirs)),
                    (Some(THEIR_LIST), Some(Carried::Files(theirs))) if self.passes.to_the_machine => Some(files_for_the_machine(theirs)),
                    (Some(THEIR_FORMATTING), Some(Carried::Text(_))) if self.passes.to_the_machine => {
                        self.their_formatting.as_deref().map(formatting_for_the_machine)
                    }
                    _ => None,
                };
                Ok((vec![handed.map(|handed| message(GIVES, WORKED, &handed)).unwrap_or_else(|| message(GIVES, FAILED, &[]))], None))
            }
            // what the client asked for: what was copied on the machine
            GIVES => {
                let mut answers = Vec::new();
                let copied = match self.asked.pop_front().flatten() {
                    Some(TEXT) if how & WORKED != 0 && holds.len() <= LONGEST => Some(Carried::Text(for_the_person(holds))),
                    Some(PICTURE) if how & WORKED != 0 && holds.len() <= LARGEST => picture_for_the_person(holds).map(Carried::Picture),
                    Some(formatting) if Some(formatting) == self.formatting && how & WORKED != 0 && holds.len() <= LONGEST => {
                        formatting_for_the_person(holds).map(Carried::Formatting)
                    }
                    // the list of the files: the machine is told to keep them, where it
                    // does, so that they can be had after something else was copied there
                    Some(list) if Some(list) == self.list && how & WORKED != 0 => {
                        let files = files_for_the_person(holds).filter(|files| !files.is_empty());
                        if files.is_some() && self.machine_keeps {
                            self.keeps += 1;
                            self.kept = Some(self.keeps);
                            answers.push(message(KEEPS, 0, &self.keeps.to_le_bytes()));
                        }
                        files.map(Carried::Files)
                    }
                    _ => None,
                };
                Ok((answers, copied))
            }
            // a piece of a file that the client asked for
            GIVES_A_FILE => {
                let piece = match (four(holds, 0), self.asked_for_a_piece) {
                    (Some(number), Some(asked)) if number == asked => {
                        self.asked_for_a_piece = None;
                        Some(if how & WORKED != 0 { Carried::Piece(holds[4..].to_vec()) } else { Carried::NoPiece })
                    }
                    _ => None,
                };
                Ok((Vec::new(), piece))
            }
            // the machine pastes a file the person copied: how large it is, is said at
            // once; a piece of it is for whoever holds the file to give. One that is not
            // among those the person copied, or a folder, is not given
            ASKS_FOR_A_FILE => {
                let number = holds.get(..4).ok_or(WRONG)?;
                let asked = (four(holds, 4), four(holds, 8), four(holds, 12), four(holds, 16), four(holds, 20));
                let file = match (&self.theirs, asked.0) {
                    (Some(Carried::Files(theirs)), Some(at)) if self.passes.to_the_machine => {
                        theirs.get(at as usize).filter(|file| !file.folder)
                    }
                    _ => None,
                };
                match (file, asked) {
                    (Some(file), (_, Some(1), ..)) => {
                        let mut says = number.to_vec();
                        says.extend(file.size.to_le_bytes());
                        Ok((vec![message(GIVES_A_FILE, WORKED, &says)], None))
                    }
                    (Some(_), (Some(at), Some(2), Some(low), Some(high), Some(length))) => {
                        let wants = Carried::Wants {
                            number: four(holds, 0).ok_or(WRONG)?,
                            file: at,
                            from: u64::from(low) | (u64::from(high) << 32),
                            length: length.min(LONGEST_PIECE * 4),
                        };
                        Ok((Vec::new(), Some(wants)))
                    }
                    _ => Ok((vec![message(GIVES_A_FILE, FAILED, number)], None)),
                }
            }
            // that the machine took what the client holds, and what is not Away's
            _ => Ok((Vec::new(), None)),
        }
    }

    /// The person copied this text. Gives what tells the machine so, once the machine is
    /// ready and where the text may pass to it: nothing before, and nothing for a text
    /// that is too long to pass.
    pub fn copied(&mut self, text: &str) -> Vec<Vec<u8>> {
        if text.len() * 2 > LONGEST {
            return Vec::new();
        }
        self.holds_now(Carried::Text(text.to_string()))
    }

    /// The person copied this picture. As for text: nothing where it may not pass, and
    /// nothing for a picture that is too large, or that is not as large as it says.
    pub fn copied_picture(&mut self, picture: Picture) -> Vec<Vec<u8>> {
        let (width, height) = (picture.width, picture.height);
        let right = width > 0 && height > 0 && width <= MOST_ACROSS && height <= MOST_ACROSS && picture.pixels.len() == width * height * 3;
        if !right || width * height * 4 + 40 > LARGEST {
            return Vec::new();
        }
        self.holds_now(Carried::Picture(picture))
    }

    /// The person copied these files, for the machine to paste: their names and sizes.
    /// What is in them is asked for when the machine pastes. Nothing where the
    /// clipboard doesn't pass to the machine, where the machine names no kinds at full
    /// length, or for more files than are listed.
    pub fn copied_files(&mut self, files: Vec<File>) -> Vec<Vec<u8>> {
        if files.is_empty() || files.len() > MOST_FILES || !self.long_names && self.ready {
            return Vec::new();
        }
        self.holds_now(Carried::Files(files))
    }

    /// Gives the machine the piece of a file it wanted, under the number it asked
    /// with, or says that there is none to give.
    pub fn gives_a_piece(&self, number: u32, piece: Option<&[u8]>) -> Vec<Vec<u8>> {
        let mut says = number.to_le_bytes().to_vec();
        says.extend_from_slice(piece.unwrap_or_default());
        vec![message(GIVES_A_FILE, if piece.is_some() { WORKED } else { FAILED }, &says)]
    }

    /// Asks the machine for a piece of a file it listed: the file by its place in the
    /// list, the piece by where it begins and how long it is at most. Nothing where no
    /// files are listed, or while another piece is waited for.
    pub fn asks_for_a_piece(&mut self, file: u32, from: u64, length: u32) -> Vec<Vec<u8>> {
        if self.list.is_none() || self.asked_for_a_piece.is_some() {
            return Vec::new();
        }
        self.pieces += 1;
        self.asked_for_a_piece = Some(self.pieces);
        let mut asks = Vec::with_capacity(28);
        asks.extend(self.pieces.to_le_bytes());
        asks.extend(file.to_le_bytes());
        // a range of the file, and not its size
        asks.extend(2u32.to_le_bytes());
        asks.extend((from as u32).to_le_bytes());
        asks.extend(((from >> 32) as u32).to_le_bytes());
        asks.extend(length.min(LONGEST_PIECE).to_le_bytes());
        asks.extend(self.kept.into_iter().flat_map(u32::to_le_bytes));
        vec![message(ASKS_FOR_A_FILE, 0, &asks)]
    }

    /// The person copied this text, which has formatting, written as a page of the web
    /// is. As for text: the machine pastes whichever of the two it takes.
    pub fn copied_formatted(&mut self, text: &str, formatted: &str) -> Vec<Vec<u8>> {
        if text.len() * 2 > LONGEST {
            return Vec::new();
        }
        let formatted = (formatted.len() <= LONGEST).then(|| formatted.to_string());
        self.holds_with(Carried::Text(text.to_string()), formatted)
    }

    fn holds_now(&mut self, theirs: Carried) -> Vec<Vec<u8>> {
        self.holds_with(theirs, None)
    }

    fn holds_with(&mut self, theirs: Carried, formatting: Option<String>) -> Vec<Vec<u8>> {
        if !self.passes.to_the_machine {
            return Vec::new();
        }
        self.their_formatting = formatting;
        self.theirs = Some(theirs);
        // what the client asked the machine for is no longer the newest
        self.asked.iter_mut().for_each(|asked| *asked = None);
        match self.ready {
            true => vec![self.holds()],
            false => Vec::new(),
        }
    }
}

/// What a machine says about the clipboard, for the stand-ins and the tests.
#[cfg(any(test, feature = "stand-in"))]
pub mod stand_in {
    use super::*;

    /// What the machine can do, and that it is ready.
    pub fn ready(long_names: bool) -> Vec<Vec<u8>> {
        let mut can = vec![1, 0, 0, 0, 1, 0, 12, 0, 2, 0, 0, 0];
        can.extend(if long_names { LONG_NAMES } else { 0 }.to_le_bytes());
        vec![message(CAN_DO, 0, &can), message(READY, 0, &[])]
    }

    /// The machine's clipboard holds something new: text, and a kind that is not Away's.
    pub fn holds_text(long_names: bool) -> Vec<u8> {
        let mut kinds = Vec::new();
        for (kind, name) in [(0xc004u32, "Native"), (TEXT, "")] {
            kinds.extend(kind.to_le_bytes());
            let mut written: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
            written.resize(if long_names { written.len() + 2 } else { 32 }, 0);
            kinds.extend(written);
        }
        message(HOLDS, 0, &kinds)
    }

    /// The machine's clipboard holds a picture, and a kind that is not Away's.
    pub fn holds_picture(long_names: bool) -> Vec<u8> {
        let mut kinds = Vec::new();
        for (kind, name) in [(0xc004u32, "Native"), (PICTURE, "")] {
            kinds.extend(kind.to_le_bytes());
            let mut written: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
            written.resize(if long_names { written.len() + 2 } else { 32 }, 0);
            kinds.extend(written);
        }
        message(HOLDS, 0, &kinds)
    }

    /// The machine gives the picture the client asked for, as a machine writes one with
    /// 24 bits a pixel: from the bottom row up, each row filled up to four bytes.
    pub fn gives_picture(picture: &Picture) -> Vec<u8> {
        let (width, height) = (picture.width, picture.height);
        let mut held = Vec::new();
        held.extend(40u32.to_le_bytes());
        held.extend((width as u32).to_le_bytes());
        held.extend((height as u32).to_le_bytes());
        held.extend(1u16.to_le_bytes());
        held.extend(24u16.to_le_bytes());
        held.extend([0; 24]);
        for row in (0..height).rev() {
            for pixel in picture.pixels[row * width * 3..][..width * 3].as_chunks::<3>().0 {
                held.extend([pixel[2], pixel[1], pixel[0]]);
            }
            held.resize(held.len() + (4 - width * 3 % 4) % 4, 0);
        }
        message(GIVES, WORKED, &held)
    }

    /// The machine pastes a picture: it asks for the client's.
    pub fn asks_for_picture() -> Vec<u8> {
        message(ASKS, 0, &PICTURE.to_le_bytes())
    }

    /// The picture a client gave, where it gave one.
    pub fn gave_picture(arrived: &[u8]) -> Option<Picture> {
        let worked = arrived.get(2..4).is_some_and(|how| u16::from_le_bytes([how[0], how[1]]) & WORKED != 0);
        arrived.get(8..).filter(|_| worked).and_then(picture_for_the_person)
    }

    /// The number the stand-in gives its list of files among the kinds it holds.
    pub const LIST: u32 = 0xc0a1;

    /// What the machine can do where it hands files over, and that it is ready.
    pub fn ready_with_files() -> Vec<Vec<u8>> {
        let mut can = vec![1, 0, 0, 0, 1, 0, 12, 0, 2, 0, 0, 0];
        can.extend((LONG_NAMES | FILES_IN_PIECES | KEEPS_WHAT_WAS_COPIED | LARGE_FILES).to_le_bytes());
        vec![message(CAN_DO, 0, &can), message(READY, 0, &[])]
    }

    /// The machine's clipboard holds files: their list, what is in them, and a kind
    /// that is not Away's.
    pub fn holds_files() -> Vec<u8> {
        let mut kinds = Vec::new();
        for (kind, name) in [(LIST, LIST_OF_FILES), (0xc0a2, "FileContents"), (0xc004, "Preferred DropEffect")] {
            kinds.extend(kind.to_le_bytes());
            kinds.extend(name.encode_utf16().flat_map(u16::to_le_bytes));
            kinds.extend([0, 0]);
        }
        message(HOLDS, 0, &kinds)
    }

    /// The machine gives the list of its files: each by its name as a machine writes
    /// it, its size, and whether it is a folder.
    pub fn gives_files(files: &[(&str, u64, bool)]) -> Vec<u8> {
        let mut held = (files.len() as u32).to_le_bytes().to_vec();
        for (name, size, folder) in files {
            let mut entry = vec![0u8; ONE_FILE];
            entry[..4].copy_from_slice(&0x0000_4044u32.to_le_bytes());
            entry[36..40].copy_from_slice(&if *folder { A_FOLDER } else { 0x80 }.to_le_bytes());
            entry[64..68].copy_from_slice(&((size >> 32) as u32).to_le_bytes());
            entry[68..72].copy_from_slice(&(*size as u32).to_le_bytes());
            for (at, unit) in name.encode_utf16().take(259).enumerate() {
                entry[NAME_AT + at * 2..][..2].copy_from_slice(&unit.to_le_bytes());
            }
            held.extend(entry);
        }
        message(GIVES, WORKED, &held)
    }

    /// The machine gives the piece of a file that was asked for under this number, or
    /// says that it gives none.
    pub fn gives_a_piece(number: u32, piece: Option<&[u8]>) -> Vec<u8> {
        let mut held = number.to_le_bytes().to_vec();
        held.extend_from_slice(piece.unwrap_or_default());
        message(GIVES_A_FILE, if piece.is_some() { WORKED } else { FAILED }, &held)
    }

    /// The machine pastes files: it asks for their list, by the number the client gave it.
    pub fn asks_for_their_files(list: u32) -> Vec<u8> {
        message(ASKS, 0, &list.to_le_bytes())
    }

    /// The machine asks for a file the client holds: how large it is, or a piece of it.
    pub fn asks_for_theirs(number: u32, file: u32, piece: Option<(u64, u32)>) -> Vec<u8> {
        let (from, length) = piece.unwrap_or((0, 8));
        let mut asks = number.to_le_bytes().to_vec();
        asks.extend(file.to_le_bytes());
        asks.extend(if piece.is_some() { 2u32 } else { 1 }.to_le_bytes());
        asks.extend((from as u32).to_le_bytes());
        asks.extend(((from >> 32) as u32).to_le_bytes());
        asks.extend(length.to_le_bytes());
        message(ASKS_FOR_A_FILE, 0, &asks)
    }

    /// The files a client listed, where what it gave is such a list.
    pub fn gave_files(arrived: &[u8]) -> Option<Vec<File>> {
        let worked = arrived.get(2..4).is_some_and(|how| u16::from_le_bytes([how[0], how[1]]) & WORKED != 0);
        arrived.get(8..).filter(|_| worked).and_then(files_for_the_person)
    }

    /// What a client gave of a file: the number it answers, and the bytes, or none.
    pub fn gave_of_a_file(arrived: &[u8]) -> Option<(u32, Option<Vec<u8>>)> {
        let kind = arrived.get(..2).map(|kind| u16::from_le_bytes([kind[0], kind[1]]))?;
        let worked = u16::from_le_bytes([*arrived.get(2)?, *arrived.get(3)?]) & WORKED != 0;
        let number = arrived.get(8..12).map(|four| u32::from_le_bytes([four[0], four[1], four[2], four[3]]))?;
        (kind == GIVES_A_FILE).then(|| (number, worked.then(|| arrived[12..].to_vec())))
    }

    /// The number the stand-in gives the formatting of the text it holds.
    pub const ITS_FORMATTING: u32 = 0xc0b1;

    /// The machine's clipboard holds text with its formatting, and a kind that is not Away's.
    pub fn holds_formatted() -> Vec<u8> {
        let mut kinds = Vec::new();
        for (kind, name) in [(0xc004u32, "Native"), (ITS_FORMATTING, FORMATTING), (TEXT, "")] {
            kinds.extend(kind.to_le_bytes());
            kinds.extend(name.encode_utf16().flat_map(u16::to_le_bytes));
            kinds.extend([0, 0]);
        }
        message(HOLDS, 0, &kinds)
    }

    /// The machine gives the formatting the client asked for, as a machine writes it.
    pub fn gives_formatting(formatted: &str) -> Vec<u8> {
        message(GIVES, WORKED, &formatting_for_the_machine(formatted))
    }

    /// The number a client gives the formatting of the text it says it holds, where it holds some.
    pub fn held_formatting(arrived: &[u8]) -> Option<u32> {
        let name: Vec<u8> = FORMATTING.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let at = arrived.get(8..)?.windows(name.len()).position(|here| here == name)?;
        arrived.get(8 + at - 4..8 + at).map(|four| u32::from_le_bytes([four[0], four[1], four[2], four[3]]))
    }

    /// The machine pastes formatted text: it asks for the client's formatting, by its number.
    pub fn asks_for_their_formatting(formatting: u32) -> Vec<u8> {
        message(ASKS, 0, &formatting.to_le_bytes())
    }

    /// The formatting a client gave, where it gave some.
    pub fn gave_formatting(arrived: &[u8]) -> Option<String> {
        let worked = arrived.get(2..4).is_some_and(|how| u16::from_le_bytes([how[0], how[1]]) & WORKED != 0);
        arrived.get(8..).filter(|_| worked).and_then(formatting_for_the_person)
    }

    /// The machine took what the client says it holds.
    pub fn took() -> Vec<u8> {
        message(TOOK, WORKED, &[])
    }

    /// The machine gives the text the client asked for.
    pub fn gives(text: &str) -> Vec<u8> {
        message(GIVES, WORKED, &for_the_machine(text))
    }

    /// The machine pastes: it asks for the client's text.
    pub fn asks() -> Vec<u8> {
        message(ASKS, 0, &TEXT.to_le_bytes())
    }

    /// What a client said on the clipboard's channel.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum Said {
        /// What it can do.
        CanDo,
        /// That it holds text, or nothing.
        Holds {
            /// Whether it holds text.
            text: bool,
        },
        /// That it holds a picture.
        HoldsPicture,
        /// That it holds files: the number it gives their list.
        HoldsFiles(u32),
        /// That it asks for the machine's picture.
        AsksForPicture,
        /// That it asks for the list of the machine's files.
        AsksForFiles,
        /// That it asks for the formatting of the machine's text.
        AsksForFormatting,
        /// That it asks for a piece of a file.
        AsksForAPiece {
            /// The number the answer is given under.
            number: u32,
            /// The file, by its place in the list.
            file: u32,
            /// Where the piece begins.
            from: u64,
            /// How long it is at most.
            length: u32,
            /// The number the machine keeps the files under, where the client named it.
            kept: Option<u32>,
        },
        /// That the machine is to keep what it holds, under this number.
        Keeps(u32),
        /// That it need not keep it any longer.
        LetsGo(u32),
        /// That it took what the machine holds.
        Took,
        /// That it asks for the machine's text.
        Asks,
        /// The text it gives, or that it gives none.
        Gives(Option<String>),
        /// Anything else.
        Other,
    }

    /// Reads what a client said.
    pub fn read(arrived: &[u8]) -> Said {
        let two = |at: usize| arrived.get(at..at + 2).map(|two| u16::from_le_bytes([two[0], two[1]]));
        let holds = arrived.get(8..).unwrap_or_default();
        match (two(0), two(2)) {
            (Some(CAN_DO), _) => Said::CanDo,
            (Some(HOLDS), _) if holds.len() >= 4 && holds[..4] == PICTURE.to_le_bytes() => Said::HoldsPicture,
            (Some(HOLDS), _)
                if holds.len() >= 4 + LIST_OF_FILES.len() * 2
                    && LIST_OF_FILES
                        .encode_utf16()
                        .flat_map(u16::to_le_bytes)
                        .eq(holds[4..4 + LIST_OF_FILES.len() * 2].iter().copied()) =>
            {
                Said::HoldsFiles(u32::from_le_bytes([holds[0], holds[1], holds[2], holds[3]]))
            }
            // text, also where the formatting it has is named before it
            (Some(HOLDS), _) => {
                Said::Holds { text: holds.len() >= 4 && (holds[..4] == TEXT.to_le_bytes() || holds[..4] == THEIR_FORMATTING.to_le_bytes()) }
            }
            (Some(TOOK), _) => Said::Took,
            (Some(ASKS), _) if holds == TEXT.to_le_bytes() => Said::Asks,
            (Some(ASKS), _) if holds == PICTURE.to_le_bytes() => Said::AsksForPicture,
            (Some(ASKS), _) if holds == LIST.to_le_bytes() => Said::AsksForFiles,
            (Some(ASKS), _) if holds == ITS_FORMATTING.to_le_bytes() => Said::AsksForFormatting,
            (Some(ASKS_FOR_A_FILE), _) if holds.len() >= 24 => {
                let four = |at: usize| u32::from_le_bytes([holds[at], holds[at + 1], holds[at + 2], holds[at + 3]]);
                Said::AsksForAPiece {
                    number: four(0),
                    file: four(4),
                    from: u64::from(four(12)) | (u64::from(four(16)) << 32),
                    length: four(20),
                    kept: (holds.len() >= 28).then(|| four(24)),
                }
            }
            (Some(KEEPS), _) if holds.len() >= 4 => Said::Keeps(u32::from_le_bytes([holds[0], holds[1], holds[2], holds[3]])),
            (Some(LETS_GO), _) if holds.len() >= 4 => Said::LetsGo(u32::from_le_bytes([holds[0], holds[1], holds[2], holds[3]])),
            (Some(GIVES), Some(how)) => Said::Gives((how & WORKED != 0).then(|| for_the_person(holds))),
            _ => Said::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::stand_in::Said;
    use super::*;

    fn said(answers: &[Vec<u8>]) -> Vec<Said> {
        answers.iter().map(|answer| stand_in::read(answer)).collect()
    }

    /// A clipboard whose machine is ready, and writes the kinds' names this way.
    fn ready(passes: Passes, long_names: bool) -> Clipboard {
        let mut clipboard = Clipboard::new(passes);
        for message in stand_in::ready(long_names) {
            clipboard.receive(&message).expect("the machine's first words");
        }
        clipboard
    }

    #[test]
    fn what_is_copied_on_the_machine_is_asked_for_at_once_and_reaches_the_person() {
        for long_names in [true, false] {
            let mut clipboard = Clipboard::new(Passes::BOTH);
            let [can_do, is_ready] = <[Vec<u8>; 2]>::try_from(stand_in::ready(long_names)).unwrap();
            assert_eq!(clipboard.receive(&can_do).unwrap(), (Vec::new(), None));
            // the client says what it can do, as the machine does it, and that it holds nothing
            let (answers, copied) = clipboard.receive(&is_ready).unwrap();
            assert_eq!((said(&answers), copied), (vec![Said::CanDo, Said::Holds { text: false }], None));
            assert_eq!(&answers[0][8..], &[1, 0, 0, 0, 1, 0, 12, 0, 2, 0, 0, 0, if long_names { 2 } else { 0 }, 0, 0, 0]);

            // the machine copies: the client confirms and asks for the text, and is given it
            let (answers, copied) = clipboard.receive(&stand_in::holds_text(long_names)).unwrap();
            assert_eq!((said(&answers), copied), (vec![Said::Took, Said::Asks], None));
            let (answers, copied) = clipboard.receive(&stand_in::gives("one line\nand a second: €")).unwrap();
            assert_eq!((answers.len(), copied.as_ref().and_then(Carried::text)), (0, Some("one line\nand a second: €")));
            // a text nobody asked for is not taken
            assert_eq!(clipboard.receive(&stand_in::gives("unasked")).unwrap(), (Vec::new(), None));
        }
    }

    #[test]
    fn what_the_person_copied_is_told_to_the_machine_and_handed_over_when_it_pastes() {
        // before the machine is ready nothing is said; once it is, the client holds text
        let mut clipboard = Clipboard::new(Passes::BOTH);
        assert!(clipboard.copied("from before").is_empty());
        let [can_do, is_ready] = <[Vec<u8>; 2]>::try_from(stand_in::ready(true)).unwrap();
        clipboard.receive(&can_do).unwrap();
        assert_eq!(said(&clipboard.receive(&is_ready).unwrap().0), [Said::CanDo, Said::Holds { text: true }]);

        // a new text is told at once, and handed over with the lines ended as Windows ends them
        let told = clipboard.copied("two\nlines");
        assert_eq!(said(&told), [Said::Holds { text: true }]);
        assert_eq!(&told[0][8..], &[13, 0, 0, 0, 0, 0]);
        let (answers, copied) = clipboard.receive(&stand_in::asks()).unwrap();
        assert_eq!((said(&answers), copied), (vec![Said::Gives(Some("two\nlines".to_string()))], None));
        let handed: Vec<u16> = answers[0][8..].as_chunks::<2>().0.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
        assert_eq!(String::from_utf16_lossy(&handed), "two\r\nlines\0");
        // lines that end as Windows ends them already stay as they are
        assert_eq!(for_the_machine("a\r\nb"), for_the_machine("a\nb"));

        // a machine that writes short names is told the kind with a name of 32 bytes
        let mut short = ready(Passes::BOTH, false);
        assert_eq!(short.copied("text")[0].len(), 8 + 4 + 32);
        // another kind than text is not given, and neither is a file
        let mut other = stand_in::asks();
        other[8] = 2;
        assert_eq!(said(&short.receive(&other).unwrap().0), [Said::Gives(None)]);
        let file = short.receive(&message(ASKS_FOR_A_FILE, 0, &[7, 0, 0, 0, 0, 0, 0, 0])).unwrap().0;
        assert_eq!(file, [message(GIVES_A_FILE, FAILED, &[7, 0, 0, 0])]);
    }

    /// A picture of three by two pixels, each of another colour.
    fn six_pixels() -> Picture {
        Picture { width: 3, height: 2, pixels: (1..=18).map(|byte| byte * 10).collect() }
    }

    #[test]
    fn a_picture_passes_both_ways_where_no_text_is_held() {
        let mut clipboard = ready(Passes::BOTH, true);
        // the machine copies a picture: the client asks for it, and is given it as a
        // machine writes one, from the bottom row up with its rows filled up
        let (answers, copied) = clipboard.receive(&stand_in::holds_picture(true)).unwrap();
        assert_eq!((said(&answers), copied), (vec![Said::Took, Said::AsksForPicture], None));
        let (answers, copied) = clipboard.receive(&stand_in::gives_picture(&six_pixels())).unwrap();
        assert_eq!((answers.len(), copied), (0, Some(Carried::Picture(six_pixels()))));
        // the machine copies twice before it answered: it answers in the order it was
        // asked, and what it gives of the first copy is taken for nothing
        clipboard.receive(&stand_in::holds_picture(true)).unwrap();
        clipboard.receive(&stand_in::holds_text(true)).unwrap();
        assert_eq!(clipboard.receive(&stand_in::gives_picture(&six_pixels())).unwrap(), (Vec::new(), None));
        assert_eq!(clipboard.receive(&stand_in::gives("the newer one")).unwrap().1, Some(Carried::Text("the newer one".to_string())));
        clipboard.receive(&stand_in::holds_picture(true)).unwrap();
        assert_eq!(clipboard.receive(&stand_in::gives_picture(&six_pixels())).unwrap().1, Some(Carried::Picture(six_pixels())));
        // one nobody asked for is not taken, and where the machine holds text too, the text is asked for
        assert_eq!(clipboard.receive(&stand_in::gives_picture(&six_pixels())).unwrap(), (Vec::new(), None));
        let mut both = stand_in::holds_text(true);
        both.extend(PICTURE.to_le_bytes());
        both.extend([0, 0]);
        let length = (both.len() - 8) as u32;
        both[4..8].copy_from_slice(&length.to_le_bytes());
        assert_eq!(said(&clipboard.receive(&both).unwrap().0), [Said::Took, Said::Asks]);

        // the person copies a picture: the machine is told, and is handed it when it pastes
        let told = clipboard.copied_picture(six_pixels());
        assert_eq!(said(&told), [Said::HoldsPicture]);
        let (answers, copied) = clipboard.receive(&stand_in::asks_for_picture()).unwrap();
        assert_eq!((answers.len(), copied), (1, None));
        assert_eq!(stand_in::gave_picture(&answers[0]), Some(six_pixels()));
        assert_eq!(&answers[0][8..24], &[40, 0, 0, 0, 3, 0, 0, 0, 2, 0, 0, 0, 1, 0, 32, 0]);
        // its text is not there to give while it holds a picture, and the other way round
        assert_eq!(said(&clipboard.receive(&stand_in::asks()).unwrap().0), [Said::Gives(None)]);
        clipboard.copied("text now");
        assert_eq!(stand_in::gave_picture(&clipboard.receive(&stand_in::asks_for_picture()).unwrap().0[0]), None);

        // where nothing passes to the machine, or the picture is not as large as it says, nothing is told
        let mut from_it = ready(Passes { to_the_machine: false, from_the_machine: true }, true);
        assert!(from_it.copied_picture(six_pixels()).is_empty());
        assert!(clipboard.copied_picture(Picture { width: 3, height: 3, pixels: vec![0; 18] }).is_empty());
        assert!(clipboard.copied_picture(Picture { width: 0, height: 0, pixels: Vec::new() }).is_empty());
    }

    #[test]
    fn a_picture_is_read_however_a_machine_writes_it_and_refused_where_it_is_not_one() {
        let picture = six_pixels();
        // from the top down, with a negative height, and 32 bits a pixel
        let mut top_down = picture_for_the_machine(&picture);
        top_down[8..12].copy_from_slice(&(-2i32).to_le_bytes());
        let rows = top_down.split_off(40);
        top_down.extend_from_slice(&rows[12..]);
        top_down.extend_from_slice(&rows[..12]);
        assert_eq!(picture_for_the_person(&top_down), Some(picture.clone()));
        // by masks, after a head of forty bytes: red first, then green, then blue
        let mut by_masks = picture_for_the_machine(&picture);
        by_masks[16..20].copy_from_slice(&BY_MASKS.to_le_bytes());
        let rows = by_masks.split_off(40);
        by_masks.extend([0xff, 0, 0, 0, 0, 0xff, 0, 0, 0, 0, 0xff, 0]);
        by_masks.extend(rows.as_chunks::<4>().0.iter().flat_map(|pixel| [pixel[2], pixel[1], pixel[0], pixel[3]]));
        assert_eq!(picture_for_the_person(&by_masks), Some(picture.clone()));
        // what the client writes itself is read back as it was
        assert_eq!(picture_for_the_person(&picture_for_the_machine(&picture)), Some(picture.clone()));

        // one that is cut anywhere, one with a table of colours for eight bits, one of no
        // size and one that says it is larger than a picture is
        let whole = picture_for_the_machine(&picture);
        for cut in 0..whole.len() {
            assert_eq!(picture_for_the_person(&whole[..cut]), None, "cut at {cut}");
        }
        let mut of_a_table = whole.clone();
        of_a_table[14] = 8;
        assert_eq!(picture_for_the_person(&of_a_table), None);
        let mut of_no_size = whole.clone();
        of_no_size[4..8].copy_from_slice(&0u32.to_le_bytes());
        assert_eq!(picture_for_the_person(&of_no_size), None);
        let mut too_wide = whole;
        too_wide[4..8].copy_from_slice(&(MOST_ACROSS as u32 + 1).to_le_bytes());
        assert_eq!(picture_for_the_person(&too_wide), None);
    }

    #[test]
    fn the_files_that_are_copied_on_the_machine_are_listed_and_had_a_piece_at_a_time() {
        let mut clipboard = Clipboard::new(Passes::BOTH).with_files();
        let [can_do, is_ready] = <[Vec<u8>; 2]>::try_from(stand_in::ready_with_files()).unwrap();
        clipboard.receive(&can_do).unwrap();
        // the client says that files pass in pieces, that it keeps and is kept for, and that large ones pass
        let (answers, _) = clipboard.receive(&is_ready).unwrap();
        assert_eq!(&answers[0][8..], &[1, 0, 0, 0, 1, 0, 12, 0, 2, 0, 0, 0, 0x3e, 0, 0, 0]);
        // no piece is asked for while no files are listed
        assert!(clipboard.asks_for_a_piece(0, 0, 100).is_empty());

        // the machine copies files: the client asks for their list, and tells the
        // machine to keep them
        let (answers, copied) = clipboard.receive(&stand_in::holds_files()).unwrap();
        assert_eq!((said(&answers), copied), (vec![Said::Took, Said::AsksForFiles], None));
        let listed = stand_in::gives_files(&[
            ("notes.txt", 11, false),
            ("folder", 0, true),
            ("folder\\data.bin", 5_000_000_000, false),
            ("..\\..\\C:\\elsewhere\\.\\secret.txt", 3, false),
            ("\\", 1, false),
        ]);
        let (answers, copied) = clipboard.receive(&listed).unwrap();
        assert_eq!(said(&answers), [Said::Keeps(1)]);
        assert_eq!(
            copied,
            Some(Carried::Files(vec![
                File { name: "notes.txt".to_string(), size: 11, folder: false },
                File { name: "folder".to_string(), size: 0, folder: true },
                File { name: "folder/data.bin".to_string(), size: 5_000_000_000, folder: false },
                // a name that leads elsewhere is its last parts alone, and one that says nothing is left out
                File { name: "elsewhere/secret.txt".to_string(), size: 3, folder: false },
            ]))
        );

        // a piece is asked for by the file's place, where it begins and how long it is,
        // under the number the machine keeps the files by; one at a time
        let asks = clipboard.asks_for_a_piece(2, 4_294_967_296 + 7, 1_000_000);
        assert_eq!(said(&asks), [Said::AsksForAPiece { number: 1, file: 2, from: 4_294_967_303, length: LONGEST_PIECE, kept: Some(1) }]);
        assert!(clipboard.asks_for_a_piece(0, 0, 100).is_empty());
        // a piece under another number is not taken; the one that was asked for is
        assert_eq!(clipboard.receive(&stand_in::gives_a_piece(9, Some(b"other"))).unwrap(), (Vec::new(), None));
        assert_eq!(
            clipboard.receive(&stand_in::gives_a_piece(1, Some(b"a piece"))).unwrap(),
            (Vec::new(), Some(Carried::Piece(b"a piece".to_vec())))
        );
        assert_eq!(clipboard.receive(&stand_in::gives_a_piece(1, Some(b"twice"))).unwrap(), (Vec::new(), None));
        // one the machine doesn't give, and the end of a file, which is a piece of nothing
        let asks = clipboard.asks_for_a_piece(0, 11, 100);
        assert_eq!(said(&asks), [Said::AsksForAPiece { number: 2, file: 0, from: 11, length: 100, kept: Some(1) }]);
        assert_eq!(clipboard.receive(&stand_in::gives_a_piece(2, Some(b""))).unwrap().1, Some(Carried::Piece(Vec::new())));
        clipboard.asks_for_a_piece(1, 0, 100);
        assert_eq!(clipboard.receive(&stand_in::gives_a_piece(3, None)).unwrap().1, Some(Carried::NoPiece));

        // the machine copies text: it need not keep the files any longer, and none is had
        let (answers, _) = clipboard.receive(&stand_in::holds_text(true)).unwrap();
        assert_eq!(said(&answers), [Said::Took, Said::LetsGo(1), Said::Asks]);
        assert!(clipboard.asks_for_a_piece(0, 0, 100).is_empty());

        // where files don't pass, the client says nothing of them, and asks for no list
        let mut without = Clipboard::new(Passes::BOTH);
        let [can_do, is_ready] = <[Vec<u8>; 2]>::try_from(stand_in::ready_with_files()).unwrap();
        without.receive(&can_do).unwrap();
        assert_eq!(&without.receive(&is_ready).unwrap().0[0][20..], &[2, 0, 0, 0]);
        assert_eq!(said(&without.receive(&stand_in::holds_files()).unwrap().0), [Said::Took]);
        // nor where nothing passes from the machine
        let mut to_it = Clipboard::new(Passes { to_the_machine: true, from_the_machine: false }).with_files();
        for message in stand_in::ready_with_files() {
            to_it.receive(&message).unwrap();
        }
        assert_eq!(said(&to_it.receive(&stand_in::holds_files()).unwrap().0), [Said::Took]);
        // a list that is cut is none
        let mut cut = Clipboard::new(Passes::BOTH).with_files();
        for message in stand_in::ready_with_files() {
            cut.receive(&message).unwrap();
        }
        cut.receive(&stand_in::holds_files()).unwrap();
        let mut short = listed.clone();
        short.truncate(listed.len() - 5);
        let length = (short.len() - 8) as u32;
        short[4..8].copy_from_slice(&length.to_le_bytes());
        assert_eq!(cut.receive(&short).unwrap(), (Vec::new(), None));
    }

    #[test]
    fn the_files_the_person_copied_are_listed_to_the_machine_and_given_as_it_pastes() {
        let mut clipboard = Clipboard::new(Passes::BOTH).with_files();
        for message in stand_in::ready_with_files() {
            clipboard.receive(&message).unwrap();
        }
        let file = |name: &str, size: u64| File { name: name.to_string(), size, folder: false };
        // the person copies files: the machine is told that the client holds a list of
        // files and what is in them, each kind by its name
        let told = clipboard.copied_files(vec![file("notes.txt", 11), file("a/b:c*d.bin", 5_000_000_000)]);
        assert_eq!(said(&told), [Said::HoldsFiles(THEIR_LIST)]);
        // the machine pastes: it asks for the list, and is given the names and the sizes
        let (answers, wants) = clipboard.receive(&stand_in::asks_for_their_files(THEIR_LIST)).unwrap();
        assert_eq!(wants, None);
        // a name holds nothing a machine reads as a folder or a drive
        assert_eq!(stand_in::gave_files(&answers[0]), Some(vec![file("notes.txt", 11), file("a_b_c_d.bin", 5_000_000_000)]));
        // how large a file is, is said at once
        let (answers, wants) = clipboard.receive(&stand_in::asks_for_theirs(7, 1, None)).unwrap();
        assert_eq!((stand_in::gave_of_a_file(&answers[0]), wants), (Some((7, Some(5_000_000_000u64.to_le_bytes().to_vec()))), None));
        // a piece of one is for whoever holds the file to give, under the machine's number
        let (answers, wants) = clipboard.receive(&stand_in::asks_for_theirs(8, 1, Some((4_294_967_296, 65_536)))).unwrap();
        assert_eq!((answers.len(), wants), (0, Some(Carried::Wants { number: 8, file: 1, from: 4_294_967_296, length: 65_536 })));
        assert_eq!(stand_in::gave_of_a_file(&clipboard.gives_a_piece(8, Some(b"a piece"))[0]), Some((8, Some(b"a piece".to_vec()))));
        assert_eq!(stand_in::gave_of_a_file(&clipboard.gives_a_piece(9, None)[0]), Some((9, None)));
        // a file that is not among them is not given
        let (answers, wants) = clipboard.receive(&stand_in::asks_for_theirs(10, 2, Some((0, 100)))).unwrap();
        assert_eq!((stand_in::gave_of_a_file(&answers[0]), wants), (Some((10, None)), None));

        // once they copied text, no file is given any more
        clipboard.copied("text now");
        let (answers, wants) = clipboard.receive(&stand_in::asks_for_theirs(11, 0, Some((0, 100)))).unwrap();
        assert_eq!((stand_in::gave_of_a_file(&answers[0]), wants), (Some((11, None)), None));
        // and where nothing passes to the machine, no files are told
        let mut from_it = ready(Passes { to_the_machine: false, from_the_machine: true }, true);
        assert!(from_it.copied_files(vec![file("notes.txt", 11)]).is_empty());
        // nor an empty list, nor to a machine that writes short names
        assert!(clipboard.copied_files(Vec::new()).is_empty());
        let mut short = ready(Passes::BOTH, false);
        assert!(short.copied_files(vec![file("notes.txt", 11)]).is_empty());
    }

    #[test]
    fn text_keeps_its_formatting_both_ways_where_it_has_some() {
        let mut clipboard = ready(Passes::BOTH, true);
        // the machine copies text that has formatting: the client asks for the text, and
        // then for the formatting, and is given each
        let (answers, copied) = clipboard.receive(&stand_in::holds_formatted()).unwrap();
        assert_eq!((said(&answers), copied), (vec![Said::Took, Said::Asks, Said::AsksForFormatting], None));
        assert_eq!(clipboard.receive(&stand_in::gives("bold and plain")).unwrap().1, Some(Carried::Text("bold and plain".to_string())));
        assert_eq!(
            clipboard.receive(&stand_in::gives_formatting("<b>bold</b> and plain: €")).unwrap().1,
            Some(Carried::Formatting("<b>bold</b> and plain: €".to_string()))
        );
        // as a machine writes it: a head with where the page and the text begin and end, then the page
        let written = String::from_utf8(formatting_for_the_machine("<i>x</i>")).unwrap();
        assert!(written.starts_with(
            "Version:0.9\r\nStartHTML:0000000105\r\nEndHTML:0000000185\r\nStartFragment:0000000141\r\nEndFragment:0000000149\r\n<html>"
        ));
        assert_eq!(&written[141..149], "<i>x</i>");
        assert!(written.ends_with("<!--EndFragment-->\r\n</body>\r\n</html>\0"));
        // a head that doesn't say where the text is, or says it is beyond the end, gives none
        assert_eq!(formatting_for_the_person(b"Version:0.9\r\n<html></html>"), None);
        assert_eq!(formatting_for_the_person(b"StartFragment:0000000900\r\nEndFragment:0000000990\r\n<p>short</p>"), None);
        assert_eq!(formatting_for_the_person(b"StartFragment:0000000060\r\nEndFragment:0000000050\r\n<p>turned round</p>"), None);

        // the person copies text with formatting: the machine is told of both, and is
        // handed whichever it asks for
        let told = clipboard.copied_formatted("the person's", "<u>the person's</u>");
        assert_eq!(said(&told), [Said::Holds { text: true }]);
        assert_eq!(stand_in::held_formatting(&told[0]), Some(THEIR_FORMATTING));
        let (answers, _) = clipboard.receive(&stand_in::asks()).unwrap();
        assert_eq!(said(&answers), [Said::Gives(Some("the person's".to_string()))]);
        let (answers, _) = clipboard.receive(&stand_in::asks_for_their_formatting(THEIR_FORMATTING)).unwrap();
        assert_eq!(stand_in::gave_formatting(&answers[0]), Some("<u>the person's</u>".to_string()));
        // plain text after it has no formatting to give
        let told = clipboard.copied("plain now");
        assert_eq!(stand_in::held_formatting(&told[0]), None);
        let (answers, _) = clipboard.receive(&stand_in::asks_for_their_formatting(THEIR_FORMATTING)).unwrap();
        assert_eq!(stand_in::gave_formatting(&answers[0]), None);
    }

    #[test]
    fn each_way_passes_only_where_it_is_let() {
        // nothing from the machine: what it copies is confirmed and not asked for
        let mut to_it = ready(Passes { to_the_machine: true, from_the_machine: false }, true);
        assert_eq!(said(&to_it.receive(&stand_in::holds_text(true)).unwrap().0), [Said::Took]);
        assert_eq!(to_it.receive(&stand_in::gives("not for the person")).unwrap(), (Vec::new(), None));
        assert_eq!(said(&to_it.copied("for the machine")), [Said::Holds { text: true }]);

        // nothing to the machine: the client never holds text, and gives none
        let mut from_it = ready(Passes { to_the_machine: false, from_the_machine: true }, true);
        assert!(from_it.copied("not for the machine").is_empty());
        assert_eq!(said(&from_it.receive(&stand_in::asks()).unwrap().0), [Said::Gives(None)]);
        assert_eq!(said(&from_it.receive(&stand_in::holds_text(true)).unwrap().0), [Said::Took, Said::Asks]);
        assert_eq!(from_it.receive(&stand_in::gives("for the person")).unwrap().1, Some(Carried::Text("for the person".to_string())));
        assert!(Passes::BOTH.at_all() && !Passes::NEITHER.at_all());
    }

    #[test]
    fn a_text_that_is_too_long_does_not_pass_and_what_is_not_built_as_a_message_is_refused() {
        let mut clipboard = ready(Passes::BOTH, true);
        let long = "x".repeat(LONGEST / 2 + 1);
        assert!(clipboard.copied(&long).is_empty());
        clipboard.receive(&stand_in::holds_text(true)).unwrap();
        assert_eq!(clipboard.receive(&stand_in::gives(&long)).unwrap(), (Vec::new(), None));

        // a message that is cut, one that says it is longer than it is, and a kind's name without its end
        let whole = stand_in::holds_text(true);
        for cut in 0..whole.len() {
            let _ = Clipboard::new(Passes::BOTH).receive(&whole[..cut]);
        }
        assert!(Clipboard::new(Passes::BOTH).receive(&[]).is_err());
        assert!(Clipboard::new(Passes::BOTH).receive(&[2, 0, 0, 0, 9, 0, 0, 0, 13, 0, 0, 0]).is_err());
        let mut clipboard = ready(Passes::BOTH, true);
        assert!(clipboard.receive(&message(HOLDS, 0, &[13, 0, 0, 0, b'a', 0])).is_err());
        // what is not Away's is passed over
        assert_eq!(clipboard.receive(&message(0x000a, 0, &[1, 0, 0, 0])).unwrap(), (Vec::new(), None));
    }
}
