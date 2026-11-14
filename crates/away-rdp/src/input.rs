//! Keys and the pointer, as a client tells a machine of them: the short form, which
//! every machine of today takes (MS-RDPBCGR 2.2.8.1.2).
//!
//! A key is named by where it is on the keyboard, not by what is printed on it: the
//! machine decides what a key writes, by the layout it is set to.

use crate::Error;

/// A button of the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    /// The one under the first finger.
    Left,
    /// The one that opens a menu.
    Right,
    /// The one in the middle, which is often the wheel itself.
    Middle,
    /// The one that goes back.
    Back,
    /// The one that goes forward.
    Forward,
}

/// One thing a person did with the keys or the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    /// A key went down or came up. The key is named by its place: its code in the first
    /// set of scan codes, and whether it is one of the keys that were added later.
    Key {
        /// The key's code.
        code: u8,
        /// Whether it is one of the added keys: the arrows, the keys on the right, and so on.
        extended: bool,
        /// Down, or up again.
        down: bool,
    },
    /// A character that the machine's layout may have no key for, as one unit of UTF-16.
    Character {
        /// The unit.
        unit: u16,
        /// Down, or up again.
        down: bool,
    },
    /// The pointer moved to this place.
    Moved {
        /// From the left, in pixels.
        x: u16,
        /// From the top.
        y: u16,
    },
    /// A button went down or came up, with the pointer at this place.
    Button {
        /// Which.
        button: Button,
        /// Down, or up again.
        down: bool,
        /// From the left, in pixels.
        x: u16,
        /// From the top.
        y: u16,
    },
    /// The wheel turned, with the pointer at this place.
    Wheel {
        /// By how much: 120 is one notch, away from the person or to the right; less
        /// than nothing is towards them or to the left.
        by: i16,
        /// Sideways, and not up and down.
        sideways: bool,
        /// From the left, in pixels.
        x: u16,
        /// From the top.
        y: u16,
    },
    /// Which locks are on, said once so that the machine's keys agree with the person's.
    Locks {
        /// The scroll lock.
        scroll: bool,
        /// The lock of the number keys.
        num: bool,
        /// The lock of the capitals.
        caps: bool,
    },
}

/// The most events one frame carries.
const MOST: usize = 15;

const KEY: u8 = 0;
const POINTER: u8 = 1;
const MORE_BUTTONS: u8 = 2;
const LOCKS: u8 = 3;
const CHARACTER: u8 = 4;

const MOVED: u16 = 0x0800;
const DOWN: u16 = 0x8000;
const WHEEL: u16 = 0x0200;
const WHEEL_SIDEWAYS: u16 = 0x0400;

/// The frames that tell a machine of these events, in their order.
pub fn write(events: &[Input]) -> Vec<u8> {
    let mut frames = Vec::new();
    for events in events.chunks(MOST) {
        let mut carried = Vec::new();
        for event in events {
            let head = |kind: u8, flags: u8| (kind << 5) | flags;
            let at = |flags: u16, x: u16, y: u16| [flags.to_le_bytes(), x.to_le_bytes(), y.to_le_bytes()].concat();
            match *event {
                Input::Key { code, extended, down } => {
                    carried.extend([head(KEY, u8::from(!down) | (u8::from(extended) << 1)), code]);
                }
                Input::Character { unit, down } => {
                    carried.push(head(CHARACTER, u8::from(!down)));
                    carried.extend(unit.to_le_bytes());
                }
                Input::Moved { x, y } => {
                    carried.push(head(POINTER, 0));
                    carried.extend(at(MOVED, x, y));
                }
                Input::Button { button, down, x, y } => {
                    let (kind, which) = match button {
                        Button::Left => (POINTER, 0x1000),
                        Button::Right => (POINTER, 0x2000),
                        Button::Middle => (POINTER, 0x4000),
                        Button::Back => (MORE_BUTTONS, 0x0001),
                        Button::Forward => (MORE_BUTTONS, 0x0002),
                    };
                    carried.push(head(kind, 0));
                    carried.extend(at(which | if down { DOWN } else { 0 }, x, y));
                }
                Input::Wheel { by, sideways, x, y } => {
                    // nine bits, the ninth saying that it is less than nothing
                    let turned = (by.clamp(-255, 255) as u16) & 0x01ff;
                    carried.push(head(POINTER, 0));
                    carried.extend(at(turned | if sideways { WHEEL_SIDEWAYS } else { WHEEL }, x, y));
                }
                Input::Locks { scroll, num, caps } => {
                    carried.push(head(LOCKS, u8::from(scroll) | (u8::from(num) << 1) | (u8::from(caps) << 2)));
                }
            }
        }
        // the head says how many events follow, and then how long the whole frame is: with
        // fifteen events of seven bytes at most, one byte always says it
        frames.push((events.len() as u8) << 2);
        frames.push((carried.len() + 2) as u8);
        frames.extend(carried);
    }
    frames
}

/// The events of one frame, as a machine reads them: for the tests' stand-in for one.
/// The head is the frame's first byte, and what it carries follows its length.
pub fn read(head: u8, carried: &[u8]) -> Result<Vec<Input>, Error> {
    const WRONG: Error = Error::NotRdp("the client's keys and pointer are not told as they are");
    let mut left = carried;
    let count = match (head >> 2) & 0x0f {
        // none in the head: the number is the first byte of what follows
        0 => {
            let (count, rest) = left.split_first().ok_or(WRONG)?;
            left = rest;
            usize::from(*count)
        }
        count => usize::from(count),
    };
    let mut events = Vec::with_capacity(count);
    for _ in 0..count {
        let (first, rest) = left.split_first().ok_or(WRONG)?;
        left = rest;
        let (kind, flags) = (first >> 5, first & 0x1f);
        let mut take = |length: usize| -> Result<&[u8], Error> {
            let taken = left.get(..length).ok_or(WRONG)?;
            left = &left[length..];
            Ok(taken)
        };
        let number = |pair: &[u8]| u16::from_le_bytes([pair[0], pair[1]]);
        events.push(match kind {
            KEY => Input::Key { code: take(1)?[0], extended: flags & 0x02 != 0, down: flags & 0x01 == 0 },
            CHARACTER => Input::Character { unit: number(take(2)?), down: flags & 0x01 == 0 },
            LOCKS => Input::Locks { scroll: flags & 0x01 != 0, num: flags & 0x02 != 0, caps: flags & 0x04 != 0 },
            POINTER | MORE_BUTTONS => {
                let told = take(6)?;
                let (flags, x, y) = (number(told), number(&told[2..]), number(&told[4..]));
                let down = flags & DOWN != 0;
                match (kind, flags) {
                    (MORE_BUTTONS, _) if flags & 0x0001 != 0 => Input::Button { button: Button::Back, down, x, y },
                    (MORE_BUTTONS, _) if flags & 0x0002 != 0 => Input::Button { button: Button::Forward, down, x, y },
                    (MORE_BUTTONS, _) => return Err(WRONG),
                    _ if flags & (WHEEL | WHEEL_SIDEWAYS) != 0 => {
                        // nine bits, of which the ninth is the sign
                        let by = if flags & 0x0100 != 0 { (flags & 0x01ff) as i16 - 0x0200 } else { (flags & 0x00ff) as i16 };
                        Input::Wheel { by, sideways: flags & WHEEL_SIDEWAYS != 0, x, y }
                    }
                    _ if flags & 0x1000 != 0 => Input::Button { button: Button::Left, down, x, y },
                    _ if flags & 0x2000 != 0 => Input::Button { button: Button::Right, down, x, y },
                    _ if flags & 0x4000 != 0 => Input::Button { button: Button::Middle, down, x, y },
                    _ if flags & MOVED != 0 => Input::Moved { x, y },
                    _ => return Err(WRONG),
                }
            }
            _ => return Err(WRONG),
        });
    }
    if !left.is_empty() {
        return Err(WRONG);
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tpkt::{Frame, Frames};

    #[test]
    fn keys_and_the_pointer_travel_as_the_protocol_writes_them() {
        // the key A down and up: its code is 0x1e
        assert_eq!(write(&[Input::Key { code: 0x1e, extended: false, down: true }]), [0x04, 0x04, 0x00, 0x1e]);
        assert_eq!(write(&[Input::Key { code: 0x1e, extended: false, down: false }]), [0x04, 0x04, 0x01, 0x1e]);
        // the arrow to the right is one of the added keys
        assert_eq!(write(&[Input::Key { code: 0x4d, extended: true, down: true }]), [0x04, 0x04, 0x02, 0x4d]);
        // the pointer moves to 640, 400
        assert_eq!(write(&[Input::Moved { x: 640, y: 400 }]), [0x04, 0x09, 0x20, 0x00, 0x08, 0x80, 0x02, 0x90, 0x01]);
        // the left button down there, and up
        let down = Input::Button { button: Button::Left, down: true, x: 640, y: 400 };
        let up = Input::Button { button: Button::Left, down: false, x: 640, y: 400 };
        assert_eq!(write(&[down, up]), [0x08, 0x10, 0x20, 0x00, 0x90, 0x80, 0x02, 0x90, 0x01, 0x20, 0x00, 0x10, 0x80, 0x02, 0x90, 0x01]);
        // one of the buttons at the side
        assert_eq!(
            write(&[Input::Button { button: Button::Back, down: true, x: 1, y: 2 }]),
            [0x04, 0x09, 0x40, 0x01, 0x80, 0x01, 0x00, 0x02, 0x00]
        );
        // the wheel, one notch each way, and sideways
        assert_eq!(write(&[Input::Wheel { by: 120, sideways: false, x: 0, y: 0 }])[2..5], [0x20, 0x78, 0x02]);
        assert_eq!(write(&[Input::Wheel { by: -120, sideways: false, x: 0, y: 0 }])[2..5], [0x20, 0x88, 0x03]);
        assert_eq!(write(&[Input::Wheel { by: 120, sideways: true, x: 0, y: 0 }])[2..5], [0x20, 0x78, 0x04]);
        // a character, and the locks
        assert_eq!(write(&[Input::Character { unit: 0x20ac, down: true }]), [0x04, 0x05, 0x80, 0xac, 0x20]);
        assert_eq!(write(&[Input::Locks { scroll: false, num: true, caps: true }]), [0x04, 0x03, 0x66]);
        assert!(write(&[]).is_empty());
    }

    #[test]
    fn what_is_written_is_read_back_the_same_through_the_frames_a_machine_cuts() {
        let all: Vec<Input> = [
            Input::Locks { scroll: true, num: false, caps: true },
            Input::Key { code: 0x1d, extended: true, down: true },
            Input::Key { code: 0x1d, extended: true, down: false },
            Input::Character { unit: 0x00e9, down: true },
            Input::Character { unit: 0x00e9, down: false },
            Input::Moved { x: 0, y: 0 },
            Input::Moved { x: 3839, y: 2159 },
            Input::Button { button: Button::Right, down: true, x: 12, y: 34 },
            Input::Button { button: Button::Middle, down: false, x: 56, y: 78 },
            Input::Button { button: Button::Forward, down: true, x: 9, y: 9 },
            Input::Wheel { by: -255, sideways: false, x: 5, y: 6 },
            Input::Wheel { by: 255, sideways: true, x: 7, y: 8 },
            Input::Wheel { by: -1, sideways: true, x: 7, y: 8 },
        ]
        .into_iter()
        .cycle()
        .take(40)
        .collect();
        // more than one frame holds
        let written = write(&all);
        let mut frames = Frames::new();
        frames.push(&written);
        let mut read_back = Vec::new();
        let mut count = 0;
        while let Some(frame) = frames.next_of_either_kind().unwrap() {
            let Frame::Fast(head, carried) = frame else { panic!("a frame of the short kind") };
            read_back.extend(read(head, &carried).unwrap());
            count += 1;
        }
        assert_eq!((count, frames.left().len()), (3, 0));
        assert_eq!(read_back, all);
        // a turn of the wheel beyond what nine bits hold is as far as they go
        let far = write(&[Input::Wheel { by: -3000, sideways: false, x: 0, y: 0 }]);
        assert_eq!(read(far[0], &far[2..]).unwrap(), [Input::Wheel { by: -255, sideways: false, x: 0, y: 0 }]);
    }

    #[test]
    fn events_that_are_cut_or_of_no_kind_are_refused() {
        for (head, carried) in [
            (0x04u8, &[0x00u8][..]),
            (0x04, &[0x20, 0x00, 0x08, 0x01]),
            (0x04, &[0xe0]),
            (0x08, &[0x00, 0x1e]),
            (0x04, &[0x00, 0x1e, 0x00]),
        ] {
            assert!(read(head, carried).is_err(), "{head:#x} {carried:?}");
        }
    }
}
