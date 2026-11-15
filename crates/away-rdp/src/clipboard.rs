//! The clipboard, shared between the person's device and the machine (MS-RDPECLIP), as
//! far as text goes. It travels on a further channel of its own.
//!
//! Each side says when its clipboard holds something new, by the kinds it holds it in,
//! and the other side asks for it in the kind it wants. Away takes text and nothing
//! else: what the machine copies is asked for at once, and what the person copied is
//! handed over when the machine asks for it. Each direction passes only where the
//! installation lets it.

use crate::Error;

/// The further channel the clipboard travels on.
pub const CHANNEL: &str = "cliprdr";

/// The longest text that passes, in the bytes it travels in: two for most characters.
pub const LONGEST: usize = 512 * 1024;

const READY: u16 = 0x0001;
const HOLDS: u16 = 0x0002;
const TOOK: u16 = 0x0003;
const ASKS: u16 = 0x0004;
const GIVES: u16 = 0x0005;
const CAN_DO: u16 = 0x0007;
const ASKS_FOR_A_FILE: u16 = 0x0008;
const GIVES_A_FILE: u16 = 0x0009;

const WORKED: u16 = 0x0001;
const FAILED: u16 = 0x0002;

/// Text, as Windows numbers the kinds a clipboard holds: in units of sixteen bits.
const TEXT: u32 = 13;
/// What a side says it can do: that it writes the kinds' names at their full length.
const LONG_NAMES: u32 = 0x0000_0002;

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

/// The clipboard of one connection, as far as the machine and the client are with it.
#[derive(Debug)]
pub struct Clipboard {
    passes: Passes,
    /// Whether the machine writes the kinds' names at their full length.
    long_names: bool,
    /// Whether the machine said that it is ready.
    ready: bool,
    /// What the person copied, for the machine to ask for.
    theirs: Option<String>,
    /// Whether the client asked the machine for its text and waits for it.
    asked: bool,
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

impl Clipboard {
    /// A clipboard that passes these ways.
    pub fn new(passes: Passes) -> Clipboard {
        Clipboard { passes, long_names: false, ready: false, theirs: None, asked: false }
    }

    /// What the client says it holds: text, where the person copied some that may pass
    /// to the machine, and nothing otherwise.
    fn holds(&self) -> Vec<u8> {
        let mut kinds = Vec::new();
        if self.theirs.is_some() && self.passes.to_the_machine {
            kinds.extend(TEXT.to_le_bytes());
            // the kind has no name: at full length that is one zero, otherwise 32 bytes of them
            kinds.extend(if self.long_names { vec![0; 2] } else { vec![0; 32] });
        }
        message(HOLDS, 0, &kinds)
    }

    /// Takes one message of the channel. Gives what to answer, each a message of the
    /// channel, and the text that was copied on the machine, where this message
    /// brought it.
    pub fn receive(&mut self, arrived: &[u8]) -> Result<(Vec<Vec<u8>>, Option<String>), Error> {
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
                        self.long_names = four(whole, 8).ok_or(WRONG)? & LONG_NAMES != 0;
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
                can.extend(if self.long_names { LONG_NAMES } else { 0 }.to_le_bytes());
                Ok((vec![message(CAN_DO, 0, &can), self.holds()], None))
            }
            // the machine's clipboard holds something new: the client confirms, and asks
            // for it at once where it is text that may pass
            HOLDS => {
                let mut answers = vec![message(TOOK, WORKED, &[])];
                let mut kinds = holds;
                let mut text = false;
                while kinds.len() >= 4 {
                    text |= four(kinds, 0) == Some(TEXT);
                    // past the kind's name: at full length up to its zero, otherwise 32 bytes
                    let name = match self.long_names {
                        true => {
                            kinds[4..].as_chunks::<2>().0.iter().position(|unit| *unit == [0, 0]).map(|units| units * 2 + 2).ok_or(WRONG)?
                        }
                        false => 32,
                    };
                    kinds = kinds.get(4 + name..).ok_or(WRONG)?;
                }
                self.asked = text && self.passes.from_the_machine;
                if self.asked {
                    answers.push(message(ASKS, 0, &TEXT.to_le_bytes()));
                }
                Ok((answers, None))
            }
            // the machine pastes: it is handed what the person copied, where that may pass
            ASKS => {
                let text = match (four(holds, 0), &self.theirs) {
                    (Some(TEXT), Some(theirs)) if self.passes.to_the_machine => Some(for_the_machine(theirs)),
                    _ => None,
                };
                Ok((vec![text.map(|text| message(GIVES, WORKED, &text)).unwrap_or_else(|| message(GIVES, FAILED, &[]))], None))
            }
            // what the client asked for: the text that was copied on the machine
            GIVES => {
                let asked = std::mem::take(&mut self.asked);
                let text = (asked && how & WORKED != 0 && holds.len() <= LONGEST).then(|| for_the_person(holds));
                Ok((Vec::new(), text))
            }
            // a file is never offered, so one that is asked for is not given
            ASKS_FOR_A_FILE => {
                let stream = holds.get(..4).ok_or(WRONG)?;
                Ok((vec![message(GIVES_A_FILE, FAILED, stream)], None))
            }
            // that the machine took what the client holds, and what is not Away's
            _ => Ok((Vec::new(), None)),
        }
    }

    /// The person copied this text. Gives what tells the machine so, once the machine is
    /// ready and where the text may pass to it: nothing before, and nothing for a text
    /// that is too long to pass.
    pub fn copied(&mut self, text: &str) -> Vec<Vec<u8>> {
        if !self.passes.to_the_machine || text.len() * 2 > LONGEST {
            return Vec::new();
        }
        self.theirs = Some(text.to_string());
        // what the client asked the machine for is no longer the newest
        self.asked = false;
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
            (Some(HOLDS), _) => Said::Holds { text: holds.len() >= 4 && holds[..4] == TEXT.to_le_bytes() },
            (Some(TOOK), _) => Said::Took,
            (Some(ASKS), _) if holds == TEXT.to_le_bytes() => Said::Asks,
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
            assert_eq!((answers.len(), copied.as_deref()), (0, Some("one line\nand a second: €")));
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
        assert_eq!(from_it.receive(&stand_in::gives("for the person")).unwrap().1.as_deref(), Some("for the person"));
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
