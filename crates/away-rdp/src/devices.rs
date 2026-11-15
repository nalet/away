//! The client's devices that a machine reaches over the connection (MS-RDPEFS). Of these
//! Away has one: a smart card reader, whose card exists in memory alone.
//!
//! The machine and the client first say what they are and what they can do, and the
//! client names its reader. From then on the machine asks the reader, one request at a
//! time, and each request is answered once.

use crate::Error;

/// The channel devices are reached over.
pub const CHANNEL: &str = "rdpdr";
/// The channel of sound. A Windows machine says nothing about devices to a client that
/// did not ask for this channel too (MS-RDPEFS, appendix A, note 1). Away asks for it,
/// and says nothing on it.
pub const SOUND: &str = "rdpsnd";

/// The number the reader goes by on a connection.
pub const READER: u32 = 1;

/// The version of this part of RDP the client speaks: the twelfth of its first.
const VERSION: [u16; 2] = [1, 12];

const CORE: u16 = 0x4472;
const BEGINS: u16 = 0x496e;
const CONFIRMS: u16 = 0x4343;
const NAME: u16 = 0x434e;
const DEVICES: u16 = 0x4441;
const TOOK: u16 = 0x6472;
const ASKS: u16 = 0x4952;
const ANSWERS: u16 = 0x4943;
const MACHINE_CAN: u16 = 0x5350;
const CLIENT_CAN: u16 = 0x4350;
const SIGNED_IN: u16 = 0x554c;

/// A request to a device that is a control request: the only kind a reader takes.
const CONTROL: u32 = 0x0e;
/// A reader, among the kinds of devices.
const A_READER: u32 = 0x20;
/// The result of a request that did not work, as Windows numbers results, and of one
/// whose answer is longer than the machine has room for.
const DID_NOT_WORK: u32 = 0xc000_0001;
const NO_ROOM: u32 = 0xc000_0023;

const WRONG: Error = Error::NotRdp("a message about devices is not built as one");

/// What the machine says about devices.
#[derive(Debug, PartialEq, Eq)]
pub enum Said {
    /// It begins, or begins again after it went on to another session: which version it
    /// speaks, and the number it gives this client.
    Begins {
        /// The second number of its version.
        minor: u16,
        /// The number it gives the client.
        client: u32,
    },
    /// It says what it can do.
    CanDo,
    /// It takes the client's number.
    Confirms,
    /// The person is signed in on it.
    SignedIn,
    /// Its answer on a device the client named: whether it took it.
    Took {
        /// The device.
        device: u32,
        /// Zero where it took it, and its own reason where it did not.
        result: u32,
    },
    /// It asks a device for something.
    Asks(Asked),
    /// Something else, which a client with a reader alone has no use for.
    Other(u16),
}

/// A request of the machine's to a device.
#[derive(Debug, PartialEq, Eq)]
pub struct Asked {
    /// The device.
    pub device: u32,
    /// The number the answer goes back under.
    pub request: u32,
    /// What it asks for.
    pub what: What,
}

/// What a request asks for.
#[derive(Debug, PartialEq, Eq)]
pub enum What {
    /// A control request, with its code and what it carries.
    Control {
        /// Which request it is.
        code: u32,
        /// How much room the machine has for the answer.
        room: u32,
        /// What it carries.
        carried: Vec<u8>,
    },
    /// Another kind of request, by its number: a reader takes none of them.
    Other(u32),
}

fn head(kind: u16) -> Vec<u8> {
    let mut message = CORE.to_le_bytes().to_vec();
    message.extend(kind.to_le_bytes());
    message
}

fn number(message: &[u8], at: usize) -> Result<u32, Error> {
    message.get(at..at + 4).map(|four| u32::from_le_bytes([four[0], four[1], four[2], four[3]])).ok_or(WRONG)
}

fn short(message: &[u8], at: usize) -> Result<u16, Error> {
    message.get(at..at + 2).map(|two| u16::from_le_bytes([two[0], two[1]])).ok_or(WRONG)
}

/// Reads one whole message of the machine's.
pub fn read(message: &[u8]) -> Result<Said, Error> {
    if short(message, 0)? != CORE {
        // a part of this protocol that is not Away's: printers have one
        return Ok(Said::Other(short(message, 2)?));
    }
    Ok(match short(message, 2)? {
        BEGINS => Said::Begins { minor: short(message, 6)?, client: number(message, 8)? },
        MACHINE_CAN => Said::CanDo,
        CONFIRMS => Said::Confirms,
        SIGNED_IN => Said::SignedIn,
        TOOK => Said::Took { device: number(message, 4)?, result: number(message, 8)? },
        ASKS => {
            let (device, request, kind) = (number(message, 4)?, number(message, 12)?, number(message, 16)?);
            let what = if kind == CONTROL {
                let (room, length, code) = (number(message, 24)?, number(message, 28)? as usize, number(message, 32)?);
                // twenty bytes that mean nothing lie between the code and what is carried
                What::Control { code, room, carried: message.get(56..56 + length).ok_or(WRONG)?.to_vec() }
            } else {
                What::Other(kind)
            };
            Said::Asks(Asked { device, request, what })
        }
        other => Said::Other(other),
    })
}

/// The client's answer to the machine's beginning: its version, and the number the
/// machine gave it.
pub fn begins(client: u32) -> Vec<u8> {
    let mut message = head(CONFIRMS);
    message.extend(VERSION[0].to_le_bytes());
    message.extend(VERSION[1].to_le_bytes());
    message.extend(client.to_le_bytes());
    message
}

/// The name of the client's computer.
pub fn name(computer: &str) -> Vec<u8> {
    let written: Vec<u8> = computer.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect();
    let mut message = head(NAME);
    // the name is in Unicode, on no code page
    message.extend(1u32.to_le_bytes());
    message.extend(0u32.to_le_bytes());
    message.extend((written.len() as u32).to_le_bytes());
    message.extend(written);
    message
}

/// What the client can do: the requests there are, and smart cards. Of the devices a
/// machine reaches before anybody is signed in on it, the client has one.
pub fn can_do() -> Vec<u8> {
    let mut message = head(CLIENT_CAN);
    message.extend(2u16.to_le_bytes());
    message.extend(0u16.to_le_bytes());
    // in general, in the second version of saying so, which counts those devices
    message.extend([1, 0, 44, 0, 2, 0, 0, 0]);
    // the system the client runs on and its version: a machine reads neither
    message.extend([0; 8]);
    message.extend(VERSION[0].to_le_bytes());
    message.extend(VERSION[1].to_le_bytes());
    // every kind of request; the machine may tell that a device is gone and that the
    // person is signed in; nothing is asked twice at once
    message.extend(0xffffu32.to_le_bytes());
    message.extend(0u32.to_le_bytes());
    message.extend(7u32.to_le_bytes());
    message.extend(0u32.to_le_bytes());
    message.extend(0u32.to_le_bytes());
    message.extend(1u32.to_le_bytes());
    // smart cards
    message.extend([5, 0, 8, 0, 1, 0, 0, 0]);
    message
}

/// The client's devices: the reader.
pub fn reader() -> Vec<u8> {
    let mut message = head(DEVICES);
    message.extend(1u32.to_le_bytes());
    message.extend(A_READER.to_le_bytes());
    message.extend(READER.to_le_bytes());
    message.extend(b"SCARD\0\0\0");
    message.extend(0u32.to_le_bytes());
    message
}

/// The answer to a control request: what the device gives back.
pub fn answer(device: u32, request: u32, gives: &[u8]) -> Vec<u8> {
    let mut message = head(ANSWERS);
    message.extend(device.to_le_bytes());
    message.extend(request.to_le_bytes());
    message.extend(0u32.to_le_bytes());
    message.extend((gives.len() as u32).to_le_bytes());
    message.extend_from_slice(gives);
    message
}

fn failed(device: u32, request: u32, result: u32) -> Vec<u8> {
    let mut message = head(ANSWERS);
    message.extend(device.to_le_bytes());
    message.extend(request.to_le_bytes());
    message.extend(result.to_le_bytes());
    message.extend(0u32.to_le_bytes());
    message
}

/// The answer to a request the device does not take: that it did not work. Every
/// request is answered, or the machine waits for the answer for good.
pub fn did_not_work(device: u32, request: u32) -> Vec<u8> {
    failed(device, request, DID_NOT_WORK)
}

/// The answer to a request whose answer is longer than the machine has room for. The
/// machine then asks again, with more room.
pub fn no_room(device: u32, request: u32) -> Vec<u8> {
    failed(device, request, NO_ROOM)
}

/// The machine's side of it, for the tests' stand-in for one.
#[cfg(any(test, feature = "stand-in"))]
pub mod stand_in {
    use super::*;

    /// What a client says about devices.
    #[derive(Debug, PartialEq, Eq)]
    pub enum Client {
        /// Its version, and the number it goes by.
        Begins {
            /// The second number of its version.
            minor: u16,
            /// The number it goes by.
            client: u32,
        },
        /// The name of its computer.
        Name(String),
        /// What it can do: whether smart cards are among it, and how many devices it has
        /// for a machine nobody is signed in on yet.
        CanDo {
            /// Whether it can do smart cards.
            smart_cards: bool,
            /// How many devices it has before anybody is signed in.
            before_sign_in: u32,
        },
        /// Its devices: the kind, the number and the name of each.
        Devices(Vec<(u32, u32, String)>),
        /// Its answer to a request.
        Answers {
            /// The device.
            device: u32,
            /// The request's number.
            request: u32,
            /// Zero where it worked.
            result: u32,
            /// What the device gives back.
            gives: Vec<u8>,
        },
    }

    /// Reads one whole message of the client's.
    pub fn read(message: &[u8]) -> Result<Client, Error> {
        if short(message, 0)? != CORE {
            return Err(WRONG);
        }
        Ok(match short(message, 2)? {
            CONFIRMS => Client::Begins { minor: short(message, 6)?, client: number(message, 8)? },
            NAME => {
                let length = number(message, 12)? as usize;
                let written = message.get(16..16 + length).ok_or(WRONG)?;
                let units: Vec<u16> =
                    written.as_chunks::<2>().0.iter().map(|two| u16::from_le_bytes(*two)).take_while(|unit| *unit != 0).collect();
                Client::Name(String::from_utf16(&units).map_err(|_| WRONG)?)
            }
            CLIENT_CAN => {
                let (mut smart_cards, mut before_sign_in, mut at) = (false, 0, 8);
                for _ in 0..short(message, 4)? {
                    let (kind, length) = (short(message, at)?, usize::from(short(message, at + 2)?));
                    match kind {
                        1 if number(message, at + 4)? == 2 => before_sign_in = number(message, at + 40)?,
                        5 => smart_cards = true,
                        _ => {}
                    }
                    at += length.max(8);
                }
                Client::CanDo { smart_cards, before_sign_in }
            }
            DEVICES => {
                let (mut devices, mut at) = (Vec::new(), 8);
                for _ in 0..number(message, 4)? {
                    let name = message.get(at + 8..at + 16).ok_or(WRONG)?;
                    let name: String = name.iter().take_while(|byte| **byte != 0).map(|byte| *byte as char).collect();
                    devices.push((number(message, at)?, number(message, at + 4)?, name));
                    at += 20 + number(message, at + 16)? as usize;
                }
                Client::Devices(devices)
            }
            ANSWERS => {
                let length = number(message, 16)? as usize;
                Client::Answers {
                    device: number(message, 4)?,
                    request: number(message, 8)?,
                    result: number(message, 12)?,
                    gives: message.get(20..20 + length).ok_or(WRONG)?.to_vec(),
                }
            }
            _ => return Err(WRONG),
        })
    }

    /// The machine begins: its version, and the number it gives the client.
    pub fn begins(client: u32) -> Vec<u8> {
        let mut message = head(BEGINS);
        message.extend([1, 0, 13, 0]);
        message.extend(client.to_le_bytes());
        message
    }

    /// What the machine can do: in general, and smart cards.
    pub fn can_do() -> Vec<u8> {
        let mut message = head(MACHINE_CAN);
        message.extend([2, 0, 0, 0]);
        message.extend([1, 0, 44, 0, 2, 0, 0, 0]);
        message.extend([2, 0, 0, 0, 0, 0, 0, 0, 1, 0, 13, 0]);
        message.extend(0xffffu32.to_le_bytes());
        message.extend([0; 4]);
        message.extend(7u32.to_le_bytes());
        message.extend([0; 12]);
        message.extend([5, 0, 8, 0, 1, 0, 0, 0]);
        message
    }

    /// The machine takes the client's number.
    pub fn confirms(client: u32) -> Vec<u8> {
        let mut message = head(CONFIRMS);
        message.extend([1, 0, 13, 0]);
        message.extend(client.to_le_bytes());
        message
    }

    /// The person is signed in.
    pub fn signed_in() -> Vec<u8> {
        head(SIGNED_IN)
    }

    /// The machine takes a device.
    pub fn took(device: u32) -> Vec<u8> {
        let mut message = head(TOOK);
        message.extend(device.to_le_bytes());
        message.extend(0u32.to_le_bytes());
        message
    }

    /// A control request to a device.
    pub fn asks(device: u32, request: u32, code: u32, carried: &[u8]) -> Vec<u8> {
        let mut message = head(ASKS);
        message.extend(device.to_le_bytes());
        // the file the request is about: a reader has none
        message.extend(0u32.to_le_bytes());
        message.extend(request.to_le_bytes());
        message.extend(CONTROL.to_le_bytes());
        message.extend(0u32.to_le_bytes());
        message.extend(2048u32.to_le_bytes());
        message.extend((carried.len() as u32).to_le_bytes());
        message.extend(code.to_le_bytes());
        message.extend([0; 20]);
        message.extend_from_slice(carried);
        message
    }

    /// A request of a kind a reader does not take: to open a file on it.
    pub fn asks_to_open(device: u32, request: u32) -> Vec<u8> {
        let mut message = head(ASKS);
        message.extend(device.to_le_bytes());
        message.extend(0u32.to_le_bytes());
        message.extend(request.to_le_bytes());
        message.extend([0; 8]);
        message.extend([0; 32]);
        message
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(hex: &str) -> Vec<u8> {
        let hex: String = hex.chars().filter(|character| !character.is_whitespace()).collect();
        (0..hex.len()).step_by(2).map(|at| u8::from_str_radix(&hex[at..at + 2], 16).expect("hex")).collect()
    }

    #[test]
    fn the_first_messages_are_read_and_written_as_the_specification_shows_them() {
        // the machine begins (MS-RDPEFS 4.3), and the client answers with the same number (4.4)
        assert_eq!(read(&bytes("72 44 6e 49 01 00 0c 00 01 00 00 00")), Ok(Said::Begins { minor: 12, client: 1 }));
        assert_eq!(begins(1), bytes("72 44 43 43 01 00 0c 00 01 00 00 00"));
        // the name of the client's computer (4.5)
        assert_eq!(
            name("TSDEV-SELFHOST"),
            bytes(
                "72 44 4e 43 01 00 00 00 00 00 00 00 1e 00 00 00 54 00 53 00 44 00 45 00 56 00 2d 00 53 00 45 00
                 4c 00 46 00 48 00 4f 00 53 00 54 00 00 00"
            )
        );
        // the machine takes the number (4.7), says that the person is signed in (4.6), and takes a device (4.2)
        assert_eq!(read(&bytes("72 44 43 43 01 00 0c 00 01 00 00 00")), Ok(Said::Confirms));
        assert_eq!(read(&bytes("72 44 4c 55")), Ok(Said::SignedIn));
        assert_eq!(read(&bytes("72 44 72 64 01 00 00 00 00 00 00 00")), Ok(Said::Took { device: 1, result: 0 }));
    }

    #[test]
    fn the_client_can_do_smart_cards_and_names_one_reader() {
        let can = can_do();
        assert_eq!(can.len(), 60);
        assert_eq!(&can[..8], &bytes("72 44 50 43 02 00 00 00")[..]);
        assert_eq!(&can[52..], &bytes("05 00 08 00 01 00 00 00")[..]);
        assert_eq!(stand_in::read(&can), Ok(stand_in::Client::CanDo { smart_cards: true, before_sign_in: 1 }));
        // the reader: its kind, its number, the name the specification gives a reader, and nothing more
        assert_eq!(reader(), bytes("72 44 41 44 01 00 00 00 20 00 00 00 01 00 00 00 53 43 41 52 44 00 00 00 00 00 00 00"));
        assert_eq!(stand_in::read(&reader()), Ok(stand_in::Client::Devices(vec![(0x20, READER, "SCARD".to_string())])));
        assert_eq!(stand_in::read(&name("away")), Ok(stand_in::Client::Name("away".to_string())));
        assert_eq!(stand_in::read(&begins(7)), Ok(stand_in::Client::Begins { minor: 12, client: 7 }));
        // what the machine says of itself is read for what it is, whatever it can do
        assert_eq!(read(&stand_in::begins(7)), Ok(Said::Begins { minor: 13, client: 7 }));
        assert_eq!(read(&stand_in::can_do()), Ok(Said::CanDo));
        assert_eq!(read(&stand_in::confirms(7)), Ok(Said::Confirms));
        assert_eq!(read(&stand_in::took(READER)), Ok(Said::Took { device: READER, result: 0 }));
    }

    #[test]
    fn a_request_to_a_device_is_read_and_answered_once() {
        // a control request as the specification shows one (4.20), and the answer that it did not work (4.21)
        let asked = bytes(
            "72 44 52 49 01 00 00 00 f8 01 00 00 08 00 00 00 0e 00 00 00 00 00 00 00 00 40 00 00 00 00 00 00
             a8 00 09 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00",
        );
        assert_eq!(
            read(&asked),
            Ok(Said::Asks(Asked { device: 1, request: 8, what: What::Control { code: 0x0009_00a8, room: 0x4000, carried: Vec::new() } }))
        );
        assert_eq!(did_not_work(1, 8), bytes("72 44 43 49 01 00 00 00 08 00 00 00 01 00 00 c0 00 00 00 00"));

        // a request that carries something, and its answer
        let asked = stand_in::asks(READER, 21, 0x0009_0014, &[1, 2, 3, 4, 5]);
        assert_eq!(
            read(&asked),
            Ok(Said::Asks(Asked {
                device: READER,
                request: 21,
                what: What::Control { code: 0x0009_0014, room: 2048, carried: vec![1, 2, 3, 4, 5] }
            }))
        );
        let answered = answer(READER, 21, &[9, 8, 7]);
        assert_eq!(answered, bytes("72 44 43 49 01 00 00 00 15 00 00 00 00 00 00 00 03 00 00 00 09 08 07"));
        assert_eq!(
            stand_in::read(&answered),
            Ok(stand_in::Client::Answers { device: READER, request: 21, result: 0, gives: vec![9, 8, 7] })
        );
        // another kind of request is said to be one, and a message that is cut is refused
        assert_eq!(read(&stand_in::asks_to_open(READER, 22)), Ok(Said::Asks(Asked { device: READER, request: 22, what: What::Other(0) })));
        for cut in 0..asked.len() {
            assert!(read(&asked[..cut]).is_err(), "cut at {cut}");
        }
        // what belongs to printers is not Away's
        assert_eq!(read(&bytes("52 50 43 50 00 00 00 00")), Ok(Said::Other(0x5043)));
    }
}
