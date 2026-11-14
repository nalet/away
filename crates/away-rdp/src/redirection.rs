//! A farm's word on where a connection belongs: a client that reaches a farm through
//! its broker is sent on to the machine that serves it (MS-RDPBCGR 2.2.13.1). The
//! client leaves the broker and reaches that machine, saying what the broker gave it.

use crate::Error;

/// Where a broker sends a client.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Redirection {
    /// The number of the session the client is to ask the machine for.
    pub session: u32,
    /// The machine's address on the network, as a number.
    pub address: Option<String>,
    /// The machine's whole name.
    pub name: Option<String>,
    /// The machine's short name.
    pub short_name: Option<String>,
    /// What the client is to say first to the machine it is sent to, so that the farm
    /// knows the connection as the one it sent on.
    pub token: Option<Vec<u8>>,
    /// The account the broker names for the machine, where it names one.
    pub account: Option<String>,
    /// The account's domain.
    pub domain: Option<String>,
}

const ADDRESS: u32 = 0x0001;
const TOKEN: u32 = 0x0002;
const ACCOUNT: u32 = 0x0004;
const DOMAIN: u32 = 0x0008;
const PASSWORD: u32 = 0x0010;
const NAME: u32 = 0x0100;
const SHORT_NAME: u32 = 0x0200;
const ADDRESSES: u32 = 0x0800;
const FARM_ADDRESS: u32 = 0x1000;
const GUID: u32 = 0x8000;
const CERTIFICATE: u32 = 0x0001_0000;

const WRONG: Error = Error::NotRdp("a farm's word on where the connection belongs is not built as one");

/// A text of the packet: UTF-16, ended by a zero.
fn text(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|pair| u16::from_le_bytes(*pair)).take_while(|unit| *unit != 0).collect();
    String::from_utf16_lossy(&units)
}

/// Reads where a broker sends the client.
pub fn read(packet: &[u8]) -> Result<Redirection, Error> {
    let number = |at: usize| packet.get(at..at + 4).map(|four| u32::from_le_bytes([four[0], four[1], four[2], four[3]])).ok_or(WRONG);
    // what it is, its length, the session, and which of its fields are there
    if packet.get(..2) != Some(&[0x00, 0x04]) {
        return Err(WRONG);
    }
    let session = number(4)?;
    let has = number(8)?;
    let mut at = 12;
    let mut field = |is_there: bool| -> Result<Option<&[u8]>, Error> {
        if !is_there {
            return Ok(None);
        }
        let length = number(at)? as usize;
        let field = packet.get(at + 4..at + 4 + length).ok_or(WRONG)?;
        at += 4 + length;
        Ok(Some(field))
    };
    let address = field(has & ADDRESS != 0)?.map(text);
    let token = field(has & TOKEN != 0)?.map(<[u8]>::to_vec);
    let account = field(has & ACCOUNT != 0)?.map(text);
    let domain = field(has & DOMAIN != 0)?.map(text);
    // a password the broker hands on is not used: the person's own proves them to the machine
    field(has & PASSWORD != 0)?;
    let name = field(has & NAME != 0)?.map(text);
    let short_name = field(has & SHORT_NAME != 0)?.map(text);
    for further in [FARM_ADDRESS, GUID, CERTIFICATE, ADDRESSES] {
        field(has & further != 0)?;
    }
    Ok(Redirection { session, address, name, short_name, token, account, domain })
}

/// Writes where a broker sends a client, as a broker does: for the tests' stand-in for one.
#[cfg(any(test, feature = "stand-in"))]
pub fn write(redirection: &Redirection) -> Vec<u8> {
    let wide = |text: &str| -> Vec<u8> { text.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect() };
    let mut has = 0;
    let mut fields = Vec::new();
    let mut field = |flag: u32, holds: Option<Vec<u8>>| {
        if let Some(holds) = holds {
            has |= flag;
            fields.extend((holds.len() as u32).to_le_bytes());
            fields.extend(holds);
        }
    };
    field(ADDRESS, redirection.address.as_deref().map(wide));
    field(TOKEN, redirection.token.clone());
    field(ACCOUNT, redirection.account.as_deref().map(wide));
    field(DOMAIN, redirection.domain.as_deref().map(wide));
    field(NAME, redirection.name.as_deref().map(wide));
    field(SHORT_NAME, redirection.short_name.as_deref().map(wide));
    let mut packet = vec![0x00, 0x04];
    packet.extend(((12 + fields.len()) as u16).to_le_bytes());
    packet.extend(redirection.session.to_le_bytes());
    packet.extend(has.to_le_bytes());
    packet.extend(fields);
    packet
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_brokers_word_is_read_with_every_field_it_may_carry() {
        let sent = Redirection {
            session: 7,
            address: Some("10.20.0.17".to_string()),
            name: Some("rdsh-01.corp.example".to_string()),
            short_name: Some("RDSH-01".to_string()),
            token: Some(b"Cookie: msts=285216778.15629.0000\r\n".to_vec()),
            account: Some("alex.urban".to_string()),
            domain: Some("CORP".to_string()),
        };
        assert_eq!(read(&write(&sent)), Ok(sent.clone()));
        // with nothing but the session and an address
        let little = Redirection { session: 2, address: Some("10.20.0.18".to_string()), ..Redirection::default() };
        let packet = write(&little);
        assert_eq!(&packet[..12], [0x00, 0x04, 12 + 4 + 22, 0, 2, 0, 0, 0, 1, 0, 0, 0]);
        assert_eq!(read(&packet), Ok(little));
        // a password, and what a newer client would be given, are passed over
        let mut more = write(&Redirection { session: 3, ..Redirection::default() });
        more[8..12].copy_from_slice(&(PASSWORD | GUID | ADDRESSES).to_le_bytes());
        for holds in [&b"sealed"[..], &[1, 2, 3, 4], &[9; 10]] {
            more.extend((holds.len() as u32).to_le_bytes());
            more.extend(holds);
        }
        assert_eq!(read(&more), Ok(Redirection { session: 3, ..Redirection::default() }));
    }

    #[test]
    fn a_word_that_is_cut_or_is_something_else_is_refused() {
        let packet = write(&Redirection { session: 7, address: Some("10.20.0.17".to_string()), ..Redirection::default() });
        for cut in [0, 1, 5, 11, 13, 20, packet.len() - 1] {
            assert_eq!(read(&packet[..cut]), Err(WRONG), "cut at {cut}");
        }
        let mut other = packet.clone();
        other[1] = 0x08;
        assert_eq!(read(&other), Err(WRONG));
        // a field that says it is longer than what is there
        let mut long = packet;
        long[12] = 0xff;
        assert_eq!(read(&long), Err(WRONG));
    }
}
