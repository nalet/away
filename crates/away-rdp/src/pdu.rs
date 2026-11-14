//! The messages of an open connection (MS-RDPBCGR 2.2.1.11 to 2.2.1.22): who signs in on
//! the desktop, the licence, what each side can do, and the last steps before the
//! machine draws.

use crate::secret::Secret;
use crate::Error;

fn wide(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn number16(bytes: &[u8], at: usize) -> Option<u16> {
    bytes.get(at..at + 2).map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
}

fn number32(bytes: &[u8], at: usize) -> Option<u32> {
    bytes.get(at..at + 4).map(|four| u32::from_le_bytes([four[0], four[1], four[2], four[3]]))
}

/// Who the desktop is for, and how the client would like it: the message that follows
/// the channels. With the sign-in before the desktop the machine already has the name and
/// the password, so no password travels here. A machine that signs the account in itself
/// is given the password, and asked to sign in with it without showing its own screen.
pub fn client_info(domain: &str, name: &str, password: Option<&Secret>) -> Vec<u8> {
    let mut info = Vec::with_capacity(320);
    // this message is the client's information
    info.extend(0x0040u16.to_le_bytes());
    info.extend(0u16.to_le_bytes());
    info.extend(0u32.to_le_bytes());
    // a mouse with a wheel, the Windows key, names in Unicode, the shell as large as the
    // desktop, word of how signing in went, and no sound
    let signs_in = if password.is_some() { 0x0008 } else { 0 };
    info.extend((0x000b_0173u32 | signs_in).to_le_bytes());
    let texts = [wide(domain), wide(name), password.map(|password| wide(password.expose())).unwrap_or_default(), Vec::new(), Vec::new()];
    for text in &texts {
        info.extend((text.len() as u16).to_le_bytes());
    }
    for text in &texts {
        info.extend(text);
        info.extend([0, 0]);
    }
    // where the client is, and where its program is: neither says anything of a person
    info.extend(2u16.to_le_bytes());
    for text in ["0.0.0.0", "away"] {
        let text = wide(text);
        info.extend(((text.len() + 2) as u16).to_le_bytes());
        info.extend(text);
        info.extend([0, 0]);
    }
    info.extend([0; 172]); // the time zone: none, the machine keeps its own
    info.extend(0u32.to_le_bytes());
    info.extend(0x0000_0080u32.to_le_bytes()); // smooth fonts, and everything else as at the machine
    info.extend(0u16.to_le_bytes());
    info
}

/// Reads the machine's word on a licence. A workstation takes a client as it is; a
/// machine that hands out licences does not, and that is not built yet.
pub fn read_licence(message: &[u8]) -> Result<(), Error> {
    const WRONG: Error = Error::NotRdp("the machine's word on a licence is not built as one");
    if number16(message, 0).ok_or(WRONG)? & 0x0080 == 0 {
        return Err(WRONG);
    }
    match message.get(4) {
        // an error message that is none: the client is valid, and nothing changes
        Some(0xff) if number32(message, 8) == Some(7) => Ok(()),
        Some(0xff) => Err(Error::NotYet("refuses the client for want of a licence")),
        Some(_) => Err(Error::NotYet("hands out licences, and asks the client to take one")),
        None => Err(WRONG),
    }
}

/// A message with the head every message of an open connection has.
pub(crate) fn share(kind: u16, user: u16, holds: &[u8]) -> Vec<u8> {
    let mut message = Vec::with_capacity(holds.len() + 6);
    message.extend(((holds.len() + 6) as u16).to_le_bytes());
    message.extend((kind | 0x10).to_le_bytes());
    message.extend(user.to_le_bytes());
    message.extend_from_slice(holds);
    message
}

/// A message of the kind that carries data, with its second head.
pub(crate) fn share_data(user: u16, share_id: u32, kind: u8, holds: &[u8]) -> Vec<u8> {
    let mut data = Vec::with_capacity(holds.len() + 12);
    data.extend(share_id.to_le_bytes());
    data.extend([0, 1]);
    data.extend(((holds.len() + 4) as u16).to_le_bytes());
    data.extend([kind, 0, 0, 0]);
    data.extend_from_slice(holds);
    share(7, user, &data)
}

pub(crate) fn capability(kind: u16, holds: &[u8]) -> Vec<u8> {
    let mut set = Vec::with_capacity(holds.len() + 4);
    set.extend(kind.to_le_bytes());
    set.extend(((holds.len() + 4) as u16).to_le_bytes());
    set.extend_from_slice(holds);
    set
}

pub(crate) fn words(numbers: &[u16]) -> Vec<u8> {
    numbers.iter().flat_map(|number| number.to_le_bytes()).collect()
}

/// The client's answer to what the machine can do: what the client can do. It takes
/// pictures and nothing that has to be drawn from instructions, at 32 bits a pixel, with
/// every pixel as the machine has it.
pub fn confirm(user: u16, share_id: u32, width: u16, height: u16) -> Vec<u8> {
    let mut sets = Vec::new();
    // in general: the short frames for what is drawn, and pictures without a head of their own
    sets.push(capability(0x0001, &words(&[1, 3, 0x0200, 0, 0, 0x041d, 0, 0, 0, 0])));
    // pictures: 32 bits a pixel, any size, compressed, and never with colours left out
    let mut pictures = words(&[32, 1, 1, 1, width, height, 0, 1, 1]);
    pictures.extend([0, 0x08]);
    pictures.extend(words(&[1, 0]));
    sets.push(capability(0x0002, &pictures));
    // drawing from instructions: none of them, so the machine sends pictures
    let mut orders = vec![0; 16];
    orders.extend(0u32.to_le_bytes());
    orders.extend(words(&[1, 20, 0, 1, 0, 0x002a]));
    orders.extend([0; 32]);
    orders.extend(words(&[0, 0]));
    orders.extend(0u32.to_le_bytes());
    orders.extend(0u32.to_le_bytes());
    orders.extend(words(&[0, 0, 0, 0]));
    sets.push(capability(0x0003, &orders));
    // the caches of pictures that such drawing would use: their usual sizes, unused
    let mut caches = vec![0; 24];
    caches.extend(words(&[200, 256, 600, 1024, 1000, 4096]));
    sets.push(capability(0x0004, &caches));
    // the pointer: in colour, twenty kept
    sets.push(capability(0x0008, &words(&[1, 20, 20])));
    // keys as their codes and as characters, the mouse's further buttons, short frames
    let mut input = words(&[0x003d, 0]);
    input.extend(0x0409u32.to_le_bytes());
    input.extend(4u32.to_le_bytes());
    input.extend(0u32.to_le_bytes());
    input.extend(12u32.to_le_bytes());
    input.extend([0; 64]);
    sets.push(capability(0x000d, &input));
    // no brushes, no kept characters, no pictures kept off the screen
    sets.push(capability(0x000f, &0u32.to_le_bytes()));
    let mut glyphs = vec![0; 40];
    glyphs.extend(0u32.to_le_bytes());
    glyphs.extend(words(&[0, 0]));
    sets.push(capability(0x0010, &glyphs));
    sets.push(capability(0x0011, &[0; 8]));
    // further channels in pieces of 1600 bytes, no sound, and what every client says
    sets.push(capability(0x0014, &[0, 0, 0, 0, 0x40, 0x06, 0, 0]));
    sets.push(capability(0x000c, &words(&[0, 0])));
    sets.push(capability(0x0009, &words(&[0, 0])));
    sets.push(capability(0x000e, &words(&[1, 0])));
    sets.push(capability(0x0005, &words(&[0, 0, 2, 2])));
    sets.push(capability(0x0007, &words(&[0, 0, 0, 0])));
    // a picture may arrive in pieces, of up to this many bytes together
    sets.push(capability(0x001a, &(crate::update::LARGEST as u32).to_le_bytes()));

    let named = b"AWAY\0";
    let all: Vec<u8> = sets.concat();
    let mut holds = Vec::with_capacity(all.len() + 32);
    holds.extend(share_id.to_le_bytes());
    holds.extend(1002u16.to_le_bytes());
    holds.extend((named.len() as u16).to_le_bytes());
    holds.extend(((all.len() + 4) as u16).to_le_bytes());
    holds.extend(named);
    holds.extend((sets.len() as u16).to_le_bytes());
    holds.extend(0u16.to_le_bytes());
    holds.extend(all);
    share(3, user, &holds)
}

/// The last messages before the machine draws: the client is in step, takes part, asks
/// for control, and has no fonts of its own to name.
pub fn finish(user: u16, share_id: u32) -> [Vec<u8>; 4] {
    [
        share_data(user, share_id, 0x1f, &words(&[1, 1002])),
        share_data(user, share_id, 0x14, &[4, 0, 0, 0, 0, 0, 0, 0]),
        share_data(user, share_id, 0x14, &[1, 0, 0, 0, 0, 0, 0, 0]),
        share_data(user, share_id, 0x27, &words(&[0, 0, 3, 50])),
    ]
}

/// One message of an open connection, as the machine sends it.
#[derive(Debug, PartialEq, Eq)]
pub enum FromMachine<'a> {
    /// What the machine can do, and the desktop it offers.
    CanDo {
        /// The number of this sharing of the desktop, which every later message names.
        share_id: u32,
        /// How wide the desktop is that it offers.
        width: u16,
        /// How high.
        height: u16,
        /// How many bits a pixel.
        bits: u16,
    },
    /// What the machine can do is void: it will say it again.
    Again,
    /// Data of this kind.
    Data(u8, &'a [u8]),
    /// A message of a kind this code doesn't act on.
    Other,
}

/// Cuts what arrived on the desktop's channel into its messages, and reads each.
pub fn read_from_machine(mut arrived: &[u8]) -> Result<Vec<FromMachine<'_>>, Error> {
    const WRONG: Error = Error::NotRdp("a message of the open connection is not built as one");
    let mut messages = Vec::new();
    while !arrived.is_empty() {
        let length = usize::from(number16(arrived, 0).ok_or(WRONG)?);
        // a message of four bytes says only its own length: it is left out
        if length == 0x8000 || length < 6 {
            if arrived.len() <= 4 {
                break;
            }
            return Err(WRONG);
        }
        let message = arrived.get(..length).ok_or(WRONG)?;
        let kind = number16(message, 2).ok_or(WRONG)? & 0x0f;
        let holds = &message[6..];
        messages.push(match kind {
            1 => {
                let share_id = number32(holds, 0).ok_or(WRONG)?;
                let named = usize::from(number16(holds, 4).ok_or(WRONG)?);
                let count = number16(holds, 8 + named).ok_or(WRONG)?;
                let mut sets = holds.get(12 + named..).ok_or(WRONG)?;
                let (mut width, mut height, mut bits) = (0, 0, 0);
                for _ in 0..count {
                    let set_length = usize::from(number16(sets, 2).ok_or(WRONG)?);
                    let set = sets.get(..set_length).filter(|set| set.len() >= 4).ok_or(WRONG)?;
                    if number16(set, 0) == Some(2) {
                        bits = number16(set, 4).ok_or(WRONG)?;
                        width = number16(set, 12).ok_or(WRONG)?;
                        height = number16(set, 14).ok_or(WRONG)?;
                    }
                    sets = &sets[set_length..];
                }
                FromMachine::CanDo { share_id, width, height, bits }
            }
            6 => FromMachine::Again,
            7 => {
                // the number of the sharing, how it travels, its length, then its kind and
                // whether it is compressed
                if holds.len() < 12 {
                    return Err(WRONG);
                }
                if holds[9] & 0x20 != 0 {
                    return Err(Error::NotYet("compresses its messages"));
                }
                FromMachine::Data(holds[8], &holds[12..])
            }
            _ => FromMachine::Other,
        });
        arrived = &arrived[length..];
    }
    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clients_information_names_the_account_and_carries_no_password() {
        let info = client_info("CORP", "alex.urban", None);
        assert_eq!(&info[..4], &[0x40, 0, 0, 0]);
        assert_eq!(&info[8..12], &0x000b_0173u32.to_le_bytes());
        // the lengths of the domain, the name, and three empty texts
        assert_eq!(&info[12..22], &[8, 0, 20, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&info[22..32], &[b'C', 0, b'O', 0, b'R', 0, b'P', 0, 0, 0]);
        assert_eq!(info.len(), 22 + 10 + 22 + 6 + 2 + (2 + 16) + (2 + 10) + 172 + 4 + 4 + 2);
    }

    #[test]
    fn a_machine_that_signs_in_itself_is_given_the_password_and_asked_to_use_it() {
        let info = client_info("", "alex.urban", Some(&Secret::new("pw")));
        assert_eq!(&info[8..12], &0x000b_017bu32.to_le_bytes(), "sign in with what follows");
        assert_eq!(&info[12..22], &[0, 0, 20, 0, 4, 0, 0, 0, 0, 0]);
        // an empty domain, the name, then the password
        assert_eq!(&info[22 + 2 + 22..22 + 2 + 22 + 6], &[b'p', 0, b'w', 0, 0, 0]);
        // two empty texts follow, then where the client is
        assert_eq!(info.len(), 22 + 2 + 22 + 6 + 4 + 2 + (2 + 16) + (2 + 10) + 172 + 4 + 4 + 2);
    }

    #[test]
    fn a_workstation_takes_the_client_as_it_is() {
        let mut valid = vec![0x80, 0, 0, 0, 0xff, 0x03, 0x10, 0x00];
        valid.extend(7u32.to_le_bytes());
        valid.extend(2u32.to_le_bytes());
        valid.extend([4, 0, 0, 0]);
        assert_eq!(read_licence(&valid), Ok(()));
        let mut asks = valid.clone();
        asks[4] = 0x01;
        assert_eq!(read_licence(&asks), Err(Error::NotYet("hands out licences, and asks the client to take one")));
        assert!(read_licence(&[0, 0, 0, 0, 0xff]).is_err());
        assert!(read_licence(&[]).is_err());
    }

    #[test]
    fn the_clients_answer_holds_its_sets_and_says_their_lengths() {
        let answer = confirm(1007, 0x0001_03ea, 1280, 800);
        assert_eq!(usize::from(u16::from_le_bytes([answer[0], answer[1]])), answer.len());
        assert_eq!(&answer[2..6], &[0x13, 0x00, 0xef, 0x03]);
        assert_eq!(&answer[6..12], &[0xea, 0x03, 0x01, 0x00, 0xea, 0x03]);
        let combined = usize::from(u16::from_le_bytes([answer[14], answer[15]]));
        assert_eq!(&answer[16..21], b"AWAY\0");
        assert_eq!(answer.len(), 21 + combined);
        // every set says its own length, and together they are what the answer says
        let count = u16::from_le_bytes([answer[21], answer[22]]);
        let mut sets = &answer[25..];
        let mut kinds = Vec::new();
        while !sets.is_empty() {
            kinds.push(u16::from_le_bytes([sets[0], sets[1]]));
            sets = &sets[usize::from(u16::from_le_bytes([sets[2], sets[3]]))..];
        }
        assert_eq!(kinds.len(), usize::from(count));
        assert_eq!(kinds, [1, 2, 3, 4, 8, 13, 15, 16, 17, 20, 12, 9, 14, 5, 7, 26]);
    }

    #[test]
    fn the_machines_messages_are_cut_apart_and_read() {
        // what it can do: one set, the one about pictures, then the session's number
        let mut can = Vec::new();
        can.extend(0x0001_03eau32.to_le_bytes());
        can.extend(words(&[4, 32]));
        can.extend(b"RDP\0");
        can.extend(words(&[1, 0]));
        can.extend(capability(2, &words(&[32, 1, 1, 1, 1280, 800, 0, 1, 1, 0, 1, 0])));
        can.extend(0u32.to_le_bytes());
        let mut arrived = share(1, 1002, &can);
        arrived.extend(share_data(1002, 0x0001_03ea, 0x28, &[0; 8]));
        arrived.extend(share(6, 1002, &[0; 8]));
        assert_eq!(
            read_from_machine(&arrived).unwrap(),
            [
                FromMachine::CanDo { share_id: 0x0001_03ea, width: 1280, height: 800, bits: 32 },
                FromMachine::Data(0x28, &[0; 8]),
                FromMachine::Again
            ]
        );
        for cut in 1..arrived.len() {
            let _ = read_from_machine(&arrived[..cut]);
        }
    }

    #[test]
    fn the_last_messages_are_four() {
        let last = finish(1007, 0x0001_03ea);
        assert_eq!(
            last[0],
            [
                0x16, 0x00, 0x17, 0x00, 0xef, 0x03, 0xea, 0x03, 0x01, 0x00, 0x00, 0x01, 0x08, 0x00, 0x1f, 0x00, 0x00, 0x00, 0x01, 0x00,
                0xea, 0x03
            ]
        );
        assert_eq!(last[1][14], 0x14);
        assert_eq!(last[3][14], 0x27);
    }
}
