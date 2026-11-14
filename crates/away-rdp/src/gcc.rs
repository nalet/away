//! What the client and the machine say of themselves when the connection opens
//! (MS-RDPBCGR 2.2.1.3 and 2.2.1.4): the client its screen, its keyboard and the security
//! that was chosen, the machine the channels it gives.
//!
//! Both travel inside a conference request of a standard for conferences (T.124), of
//! which RDP uses the frame and nothing else.

use crate::Error;

/// What the client says of itself.
#[derive(Clone, Debug)]
pub struct Client {
    /// How wide the desktop should be, in pixels.
    pub width: u16,
    /// How high.
    pub height: u16,
    /// The security the machine chose in the first exchange, as the number it gave.
    pub security: u32,
    /// The name the client gives itself, of which the first 15 characters travel.
    pub name: String,
    /// The layout of the person's keyboard, as Windows numbers them: `0x0409` is the
    /// one of the United States. A machine gives the session this layout, so that a key
    /// writes there what it writes at the person's own device.
    pub layout: u32,
    /// The session a farm's broker sent the client on to, where this connection is to
    /// the machine it was sent to.
    pub sent_on_to: Option<u32>,
    /// The program to start in windows of its own, where the connection is for a program
    /// and not for the desktop.
    pub program: Option<crate::rail::Program>,
}

/// The keyboard layout of the United States, which a client has where nothing says otherwise.
pub const LAYOUT_US: u32 = 0x0409;

/// A length as the conference standard writes it: one byte, or two with the top bit set.
fn length(out: &mut Vec<u8>, length: usize) {
    if length > 0x7f {
        out.extend(((length as u16) | 0x8000).to_be_bytes());
    } else {
        out.push(length as u8);
    }
}

fn block(kind: u16, holds: &[u8]) -> Vec<u8> {
    let mut block = Vec::with_capacity(holds.len() + 4);
    block.extend(kind.to_le_bytes());
    block.extend(((holds.len() + 4) as u16).to_le_bytes());
    block.extend_from_slice(holds);
    block
}

/// The conference request that carries what the client says of itself.
pub fn request(client: &Client) -> Vec<u8> {
    // the core: the version it speaks, its screen, its keyboard, its name
    let mut core = Vec::with_capacity(212);
    core.extend(0x0008_0004u32.to_le_bytes());
    core.extend(client.width.to_le_bytes());
    core.extend(client.height.to_le_bytes());
    core.extend(0xca01u16.to_le_bytes()); // a colour depth that the later field replaces
    core.extend(0xaa03u16.to_le_bytes());
    core.extend(client.layout.to_le_bytes()); // the layout of the person's keyboard: keys travel by their place
    core.extend(1u32.to_le_bytes()); // the client's build
    let mut name: Vec<u8> = client.name.encode_utf16().take(15).flat_map(u16::to_le_bytes).collect();
    name.resize(32, 0);
    core.extend(name);
    core.extend(4u32.to_le_bytes()); // an ordinary keyboard
    core.extend(0u32.to_le_bytes());
    core.extend(12u32.to_le_bytes()); // with twelve function keys
    core.extend([0; 64]); // no input method
    core.extend(0xca01u16.to_le_bytes());
    core.extend(1u16.to_le_bytes());
    core.extend(0u32.to_le_bytes());
    core.extend(24u16.to_le_bytes()); // the most this field can say; 32 bits are asked for below
    core.extend(0x000au16.to_le_bytes()); // it takes 16 and 32 bits a pixel
                                          // it reads the machine's reason when the machine ends the connection, wants 32 bits a
                                          // pixel, and says what kind of line it is on
    core.extend(0x0023u16.to_le_bytes());
    core.extend([0; 64]);
    core.push(6); // a local network
    core.push(0);
    core.extend(client.security.to_le_bytes());

    let mut blocks = block(0xc001, &core);
    // no encryption of RDP's own: TLS secures the connection
    blocks.extend(block(0xc002, &[0; 8]));
    // the further channels: the one programs are started over, where a program is asked for
    let channels: Vec<[u8; 12]> = client.program.iter().map(|_| crate::channel::asked_for(crate::rail::CHANNEL)).collect();
    let mut further = (channels.len() as u32).to_le_bytes().to_vec();
    further.extend(channels.concat());
    blocks.extend(block(0xc003, &further));
    // it follows a farm's word on where the connection belongs, in the current form; on the
    // machine it was sent to, it says which session the broker named
    let mut farm = vec![if client.sent_on_to.is_some() { 0x0f } else { 0x0d }, 0, 0, 0];
    farm.extend(client.sent_on_to.unwrap_or(0).to_le_bytes());
    blocks.extend(block(0xc004, &farm));

    let mut request = vec![0x00, 0x05, 0x00, 0x14, 0x7c, 0x00, 0x01];
    length(&mut request, blocks.len() + 14);
    request.extend([0x00, 0x08, 0x00, 0x10, 0x00, 0x01, 0xc0, 0x00, b'D', b'u', b'c', b'a']);
    length(&mut request, blocks.len());
    request.extend(blocks);
    request
}

/// What the machine says of itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Machine {
    /// The version of RDP it speaks.
    pub version: u32,
    /// The channel everything of the desktop travels on.
    pub io_channel: u16,
    /// The channels it gave for what the client asked for, in the client's order.
    pub channels: Vec<u16>,
    /// The channel for messages about the connection itself, where it gives one.
    pub message_channel: Option<u16>,
}

/// Reads the conference answer that carries what the machine says of itself.
pub fn read_answer(answer: &[u8]) -> Result<Machine, Error> {
    const WRONG: Error = Error::NotRdp("what the machine says of itself is not built as it is");
    let at = answer.windows(4).position(|part| part == b"McDn").ok_or(WRONG)? + 4;
    let mut rest = match answer.get(at..).ok_or(WRONG)? {
        [first, _, rest @ ..] if first & 0x80 != 0 => rest,
        [_, rest @ ..] => rest,
        _ => return Err(WRONG),
    };
    let mut machine = Machine::default();
    let number = |bytes: &[u8], at: usize| bytes.get(at..at + 2).map(|pair| u16::from_le_bytes([pair[0], pair[1]]));
    while let [k0, k1, l0, l1, ..] = rest {
        let kind = u16::from_le_bytes([*k0, *k1]);
        let length = usize::from(u16::from_le_bytes([*l0, *l1]));
        if length < 4 || rest.len() < length {
            return Err(WRONG);
        }
        let holds = &rest[4..length];
        match kind {
            0x0c01 => machine.version = holds.get(..4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok_or(WRONG)?,
            0x0c02 => {
                // the encryption of RDP's own: none, since TLS secures the connection
                if holds.get(..8).ok_or(WRONG)? != [0; 8] {
                    return Err(Error::NotYet("wants RDP's own old encryption on top of TLS"));
                }
            }
            0x0c03 => {
                machine.io_channel = number(holds, 0).ok_or(WRONG)?;
                let count = usize::from(number(holds, 2).ok_or(WRONG)?);
                for at in 0..count {
                    machine.channels.push(number(holds, 4 + 2 * at).ok_or(WRONG)?);
                }
            }
            0x0c04 => machine.message_channel = Some(number(holds, 0).ok_or(WRONG)?),
            _ => {}
        }
        rest = &rest[length..];
    }
    if machine.io_channel == 0 {
        return Err(WRONG);
    }
    Ok(machine)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> Client {
        Client { width: 1280, height: 800, security: 8, name: "away".to_string(), layout: LAYOUT_US, sent_on_to: None, program: None }
    }

    #[test]
    fn the_client_says_its_screen_its_name_and_the_security_that_was_chosen() {
        let request = request(&client());
        assert_eq!(&request[..7], &[0x00, 0x05, 0x00, 0x14, 0x7c, 0x00, 0x01]);
        // four blocks: 216, 12, 8 and 12 bytes
        assert_eq!(&request[7..9], &(0x8000u16 | (248 + 14)).to_be_bytes());
        assert_eq!(&request[9..21], &[0x00, 0x08, 0x00, 0x10, 0x00, 0x01, 0xc0, 0x00, 0x44, 0x75, 0x63, 0x61]);
        assert_eq!(&request[21..23], &(0x8000u16 | 248).to_be_bytes());
        let core = &request[23..23 + 216];
        assert_eq!(&core[..4], &[0x01, 0xc0, 0xd8, 0x00]);
        assert_eq!(&core[8..12], &[0x00, 0x05, 0x20, 0x03], "1280 by 800");
        assert_eq!(&core[24..34], &[b'a', 0, b'w', 0, b'a', 0, b'y', 0, 0, 0]);
        assert_eq!(&core[212..216], &[8, 0, 0, 0], "the security that was chosen");
        assert_eq!(&request[239..251], &[0x02, 0xc0, 0x0c, 0x00, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&request[251..259], &[0x03, 0xc0, 0x08, 0x00, 0, 0, 0, 0]);
        assert_eq!(&request[259..], &[0x04, 0xc0, 0x0c, 0x00, 0x0d, 0, 0, 0, 0, 0, 0, 0]);

        // for a program it asks for the channel programs are started over
        let program = crate::rail::Program { run: "||ledger".to_string(), arguments: String::new() };
        let request = super::request(&Client { program: Some(program), ..client() });
        assert_eq!(&request[251..259], &[0x03, 0xc0, 0x14, 0x00, 1, 0, 0, 0]);
        assert_eq!(&request[259..271], &[b'r', b'a', b'i', b'l', 0, 0, 0, 0, 0x00, 0x00, 0x20, 0xc0]);
        assert_eq!(&request[271..275], &[0x04, 0xc0, 0x0c, 0x00]);
    }

    #[test]
    fn the_machine_says_which_channels_it_gives() {
        let mut answer = vec![
            0x00, 0x05, 0x00, 0x14, 0x7c, 0x00, 0x01, 0x2a, 0x14, 0x76, 0x0a, 0x01, 0x01, 0x00, 0x01, 0xc0, 0x00, b'M', b'c', b'D', b'n',
        ];
        let mut blocks = block(0x0c01, &[0x04, 0x00, 0x08, 0x00, 0x0b, 0, 0, 0, 0x01, 0, 0, 0]);
        blocks.extend(block(0x0c02, &[0; 8]));
        blocks.extend(block(0x0c03, &[0xeb, 0x03, 0x02, 0x00, 0xec, 0x03, 0xed, 0x03]));
        blocks.extend(block(0x0c04, &[0xee, 0x03]));
        length(&mut answer, blocks.len());
        answer.extend(&blocks);
        assert_eq!(
            read_answer(&answer),
            Ok(Machine { version: 0x0008_0004, io_channel: 1003, channels: vec![1004, 1005], message_channel: Some(1006) })
        );
        for cut in 0..answer.len() {
            let _ = read_answer(&answer[..cut]);
        }
        let mut encrypted = answer.clone();
        let at = encrypted.windows(4).position(|part| part == [0x02, 0x0c, 0x0c, 0x00]).unwrap();
        encrypted[at + 4] = 2;
        assert_eq!(read_answer(&encrypted), Err(Error::NotYet("wants RDP's own old encryption on top of TLS")));
        assert!(read_answer(b"nothing of the kind").is_err());
    }
}
