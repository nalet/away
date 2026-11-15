//! The messages of Kerberos that a sign-in takes (RFC 4120, 5): asking for a ticket and
//! being given one, showing a ticket to a machine, and what a domain controller says
//! when it refuses. Each is written and read here as bytes; what is sealed inside them
//! is the part of [`super::crypto`].

use crate::der::{self, Reader, Value};
use crate::Error;

use super::crypto::{Key, Kind};

const WRONG: Error = Error::NotKerberos("a message is not built as Kerberos builds it");

/// The version of Kerberos every message names.
const VERSION: i64 = 5;

/// What kind of name a name is, as Kerberos numbers the kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameKind {
    /// The name of an account.
    Account = 1,
    /// The name of a service and the machine it runs on.
    Service = 2,
    /// The name of an account as its organisation writes it, with an `@` in it.
    Whole = 10,
}

/// A name, in the parts it is written in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Name {
    /// What kind of name it is, as its number.
    pub kind: i64,
    /// Its parts: one for an account, the service and the machine for a service.
    pub parts: Vec<String>,
}

impl Name {
    /// A name of this kind.
    pub fn new(kind: NameKind, parts: &[&str]) -> Name {
        Name { kind: kind as i64, parts: parts.iter().map(|part| part.to_string()).collect() }
    }

    pub(crate) fn write(&self) -> Vec<u8> {
        let parts: Vec<Vec<u8>> = self.parts.iter().map(|part| der::text(part)).collect();
        der::sequence(&[der::under(0, &der::number(self.kind)), der::under(1, &der::sequence(&parts))])
    }

    fn read(value: Value) -> Result<Name, Error> {
        let mut inside = expect(value, der::SEQUENCE)?.inside();
        let kind = inside.needed(0)?.number()?;
        let mut parts = Vec::new();
        let mut listed = expect(inside.needed(1)?, der::SEQUENCE)?.inside();
        while !listed.is_empty() {
            parts.push(listed.expect(der::GENERAL_STRING)?.text()?.to_string());
        }
        Ok(Name { kind, parts })
    }
}

fn expect(value: Value, kind: u8) -> Result<Value, Error> {
    if value.kind == kind {
        Ok(value)
    } else {
        Err(WRONG)
    }
}

/// Something sealed: which way of sealing, which of the key's versions, and the bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sealed {
    /// The way of sealing, as its number.
    pub kind: i64,
    /// Which version of the key it was sealed with, where the message says.
    pub version: Option<i64>,
    /// What is sealed.
    pub bytes: Vec<u8>,
}

impl Sealed {
    /// Seals a message with a key, for one use.
    pub fn with(key: &Key, usage: u32, begins_with: [u8; 16], message: &[u8]) -> Sealed {
        Sealed { kind: key.kind() as i64, version: None, bytes: key.seal(usage, begins_with, message) }
    }

    /// Opens it with a key, for one use.
    pub fn open(&self, key: &Key, usage: u32) -> Result<Vec<u8>, Error> {
        if self.kind != key.kind() as i64 {
            return Err(Error::NotOpened("it is sealed in another way than the key at hand seals"));
        }
        key.open(usage, &self.bytes)
    }

    fn write(&self) -> Vec<u8> {
        let mut fields = vec![der::under(0, &der::number(self.kind))];
        fields.extend(self.version.map(|version| der::under(1, &der::number(version))));
        fields.push(der::under(2, &der::bytes(&self.bytes)));
        der::sequence(&fields)
    }

    fn read(value: Value) -> Result<Sealed, Error> {
        let mut inside = expect(value, der::SEQUENCE)?.inside();
        let kind = inside.needed(0)?.number()?;
        let version = inside.under(1)?.map(|version| version.number()).transpose()?;
        let bytes = expect(inside.needed(2)?, der::OCTET_STRING)?.holds.to_vec();
        Ok(Sealed { kind, version, bytes })
    }
}

fn write_key(key: &Key) -> Vec<u8> {
    der::sequence(&[der::under(0, &der::number(key.kind() as i64)), der::under(1, &der::bytes(key.expose()))])
}

fn read_key(value: Value) -> Result<Key, Error> {
    let mut inside = expect(value, der::SEQUENCE)?.inside();
    let kind = Kind::of(inside.needed(0)?.number()?).ok_or(Error::NotKerberos("it gives a key of a kind that is not spoken here"))?;
    Key::new(kind, expect(inside.needed(1)?, der::OCTET_STRING)?.holds).ok_or(WRONG)
}

/// Something that goes with a request or an answer: its kind, and what it holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Extra {
    /// Its kind, as its number.
    pub kind: i64,
    /// What it holds.
    pub holds: Vec<u8>,
}

/// The certificate that proves who asks.
pub const WITH_A_CERTIFICATE: i64 = 16;
/// The domain controller's part of the proof with a certificate.
pub const WITH_A_CERTIFICATE_ANSWER: i64 = 17;
/// The ticket that asks for another ticket.
pub const WITH_A_TICKET: i64 = 1;
/// Whether the ticket should say what the account may do in the domain.
pub const WITH_WHAT_THE_ACCOUNT_MAY: i64 = 128;
/// The time, sealed with the key an account has from its password: the proof of who
/// asks, where a password is what they have.
pub const WITH_THE_TIME: i64 = 2;
/// How the key an account has from its password is made.
pub const HOW_THE_KEY_IS_MADE: i64 = 19;

/// What is asked of a domain controller: a ticket for a service, for an account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asked {
    /// Whose the ticket is to be. Asking with a ticket names nobody: the ticket does.
    pub account: Option<Name>,
    /// The realm: the domain's name, in capitals.
    pub realm: String,
    /// What the ticket is for.
    pub service: Name,
    /// Until when the ticket should be good, in seconds since the beginning of 1970.
    pub until: u64,
    /// A number used once, which the answer repeats.
    pub nonce: u32,
    /// The ways of sealing the client speaks, the one it likes best first.
    pub kinds: Vec<Kind>,
}

/// The ticket may be passed on, may be made good for longer, and the names in the
/// answer may be the domain's own way of writing them.
const OPTIONS: [u8; 4] = [0x40, 0x81, 0x00, 0x00];

impl Asked {
    /// What is asked, as the part of the request that a check is made over.
    pub fn write(&self) -> Vec<u8> {
        let mut fields = vec![der::under(0, &der::bits(&OPTIONS))];
        fields.extend(self.account.as_ref().map(|account| der::under(1, &account.write())));
        fields.push(der::under(2, &der::text(&self.realm)));
        fields.push(der::under(3, &self.service.write()));
        fields.push(der::under(5, &der::time(self.until)));
        fields.push(der::under(7, &der::number(i64::from(self.nonce))));
        let kinds: Vec<Vec<u8>> = self.kinds.iter().map(|kind| der::number(*kind as i64)).collect();
        fields.push(der::under(8, &der::sequence(&kinds)));
        der::sequence(&fields)
    }
}

/// A request for a ticket: the first, which proves who asks, or a later one, which shows
/// the ticket that was given. `asked` is what [`Asked::write`] gave.
pub fn request(first: bool, extras: &[Extra], asked: &[u8]) -> Vec<u8> {
    let (kind, number) = if first { (10, 10) } else { (12, 12) };
    let extras: Vec<Vec<u8>> = extras
        .iter()
        .map(|extra| der::sequence(&[der::under(1, &der::number(extra.kind)), der::under(2, &der::bytes(&extra.holds))]))
        .collect();
    let fields = [
        der::under(1, &der::number(VERSION)),
        der::under(2, &der::number(number)),
        der::under(3, &der::sequence(&extras)),
        der::under(4, asked),
    ];
    der::write(der::message(kind), &der::sequence(&fields))
}

/// Whether the ticket is asked to say what the account may do in the domain: yes.
pub fn with_what_the_account_may() -> Extra {
    Extra { kind: WITH_WHAT_THE_ACCOUNT_MAY, holds: der::sequence(&[der::under(0, &der::write(der::BOOLEAN, &[0xff]))]) }
}

/// The proof of who asks that a password gives: the time, sealed with the account's key.
pub fn with_the_time(key: &Key, now: (u64, u32), begins_with: [u8; 16]) -> Extra {
    let time = der::sequence(&[der::under(0, &der::time(now.0)), der::under(1, &der::number(i64::from(now.1)))]);
    Extra { kind: WITH_THE_TIME, holds: Sealed::with(key, 1, begins_with, &time).write() }
}

/// How the key an account has from its password is made, in one of the ways of sealing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HowAKeyIsMade {
    /// The way of sealing, as its number.
    pub kind: i64,
    /// What the password is salted with, where the domain says.
    pub salt: Option<Vec<u8>>,
    /// How many times over it is stretched, where the domain says.
    pub times: Option<u32>,
}

/// Reads what a domain controller sends along with a refusal that asks for a proof: how
/// the account's key is made, in each way of sealing it offers, the one it likes best
/// first.
pub fn read_how_keys_are_made(more: &[u8]) -> Result<Vec<HowAKeyIsMade>, Error> {
    let mut ways = Vec::new();
    let mut listed = Reader::new(more).expect(der::SEQUENCE)?.inside();
    while !listed.is_empty() {
        let mut extra = listed.expect(der::SEQUENCE)?.inside();
        if extra.needed(1)?.number()? != HOW_THE_KEY_IS_MADE {
            continue;
        }
        let holds = expect(extra.needed(2)?, der::OCTET_STRING)?.holds;
        let mut entries = Reader::new(holds).expect(der::SEQUENCE)?.inside();
        while !entries.is_empty() {
            let mut entry = entries.expect(der::SEQUENCE)?.inside();
            let kind = entry.needed(0)?.number()?;
            let salt = entry.under(1)?.map(|salt| salt.holds.to_vec());
            // the times it is stretched over, as four bytes
            let times = entry.under(2)?.and_then(|times| times.holds.try_into().ok().map(u32::from_be_bytes));
            ways.push(HowAKeyIsMade { kind, salt, times });
        }
    }
    Ok(ways)
}

/// A domain controller's answer to a request for a ticket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answered {
    /// What goes with the answer.
    pub extras: Vec<Extra>,
    /// The realm of the account, as the domain writes it.
    pub realm: String,
    /// The account, as the domain writes it.
    pub account: Name,
    /// The ticket, as it is shown to whoever it is for: sealed for them, and not read here.
    pub ticket: Vec<u8>,
    /// What is sealed for the client: the key that goes with the ticket, and more.
    pub sealed: Sealed,
}

/// What a domain controller says when it refuses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused {
    /// Why, as Kerberos numbers the reasons.
    pub code: i64,
    /// Why, in the domain controller's words, where it gives some.
    pub text: Option<String>,
    /// The domain controller's clock, in seconds since the beginning of 1970.
    pub its_time: u64,
    /// What it sends along, where it sends something.
    pub more: Option<Vec<u8>>,
}

/// What came back for a request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    /// An answer, with a ticket.
    Answered(Answered),
    /// A refusal.
    Refused(Refused),
}

/// Reads what a domain controller sent back for a request for a ticket.
pub fn read_reply(reply: &[u8]) -> Result<Reply, Error> {
    let mut outside = Reader::new(reply);
    let message = outside.value()?;
    let mut inside = message.inside().expect(der::SEQUENCE)?.inside();
    match message.kind {
        // an answer to the first request, or to a later one
        kind if kind == der::message(11) || kind == der::message(13) => {
            if inside.needed(0)?.number()? != VERSION {
                return Err(WRONG);
            }
            inside.needed(1)?;
            let mut extras = Vec::new();
            if let Some(listed) = inside.under(2)? {
                let mut listed = expect(listed, der::SEQUENCE)?.inside();
                while !listed.is_empty() {
                    let mut extra = listed.expect(der::SEQUENCE)?.inside();
                    let kind = extra.needed(1)?.number()?;
                    extras.push(Extra { kind, holds: expect(extra.needed(2)?, der::OCTET_STRING)?.holds.to_vec() });
                }
            }
            let realm = expect(inside.needed(3)?, der::GENERAL_STRING)?.text()?.to_string();
            let account = Name::read(inside.needed(4)?)?;
            let ticket = expect(inside.needed(5)?, der::message(1))?.whole.to_vec();
            let sealed = Sealed::read(inside.needed(6)?)?;
            Ok(Reply::Answered(Answered { extras, realm, account, ticket, sealed }))
        }
        kind if kind == der::message(30) => {
            inside.skip_to(4)?;
            let its_time = der::read_time(expect(inside.needed(4)?, der::GENERALIZED_TIME)?.holds)?;
            inside.skip_to(6)?;
            let code = inside.needed(6)?.number()?;
            inside.skip_to(11)?;
            let text = inside.under(11)?.map(|text| text.text().map(str::to_string)).transpose()?;
            let more = inside.under(12)?.map(|more| more.holds.to_vec());
            Ok(Reply::Refused(Refused { code, text, its_time, more }))
        }
        _ => Err(WRONG),
    }
}

/// What a domain controller seals for the client with a ticket: the key that goes with
/// the ticket, and what the ticket says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Given {
    /// The key that goes with the ticket.
    pub key: Key,
    /// The number of the request, repeated.
    pub nonce: u32,
    /// Until when the ticket is good, in seconds since the beginning of 1970.
    pub until: u64,
    /// The realm of what the ticket is for.
    pub realm: String,
    /// What the ticket is for.
    pub service: Name,
}

/// Reads what was sealed with a ticket, once it is opened.
pub fn read_given(opened: &[u8]) -> Result<Given, Error> {
    let message = Reader::new(opened).value()?;
    // the two kinds of answer number it differently, and some write the one for the other
    if message.kind != der::message(25) && message.kind != der::message(26) {
        return Err(WRONG);
    }
    let mut inside = message.inside().expect(der::SEQUENCE)?.inside();
    let key = read_key(inside.needed(0)?)?;
    inside.skip_to(2)?;
    let nonce = u32::try_from(inside.needed(2)?.number()?).map_err(|_| WRONG)?;
    inside.skip_to(7)?;
    let until = der::read_time(expect(inside.needed(7)?, der::GENERALIZED_TIME)?.holds)?;
    inside.skip_to(9)?;
    let realm = expect(inside.needed(9)?, der::GENERAL_STRING)?.text()?.to_string();
    let service = Name::read(inside.needed(10)?)?;
    Ok(Given { key, nonce, until, realm, service })
}

/// What proves, with a ticket, that whoever shows it is the one it was given to: who
/// they are and what time it is, sealed with the ticket's key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proof {
    /// The realm of the account.
    pub realm: String,
    /// The account.
    pub account: Name,
    /// A check that goes with it: its kind, and the check.
    pub check: Option<(i64, Vec<u8>)>,
    /// The time, in seconds since the beginning of 1970, and the millionths of a second.
    pub time: (u64, u32),
    /// A key for what follows, where the client makes one.
    pub key: Option<Key>,
    /// The number the client counts its messages from.
    pub counts_from: Option<u32>,
}

impl Proof {
    /// The proof, as it is sealed.
    pub fn write(&self) -> Vec<u8> {
        let mut fields =
            vec![der::under(0, &der::number(VERSION)), der::under(1, &der::text(&self.realm)), der::under(2, &self.account.write())];
        if let Some((kind, check)) = &self.check {
            fields.push(der::under(3, &der::sequence(&[der::under(0, &der::number(*kind)), der::under(1, &der::bytes(check))])));
        }
        fields.push(der::under(4, &der::number(i64::from(self.time.1))));
        fields.push(der::under(5, &der::time(self.time.0)));
        fields.extend(self.key.as_ref().map(|key| der::under(6, &write_key(key))));
        fields.extend(self.counts_from.map(|from| der::under(7, &der::number(i64::from(from)))));
        der::write(der::message(2), &der::sequence(&fields))
    }
}

/// A ticket shown to whoever it is for, with the proof that goes with it. `mutual` asks
/// the other side to prove itself in turn.
pub fn show(ticket: &[u8], proof: &Sealed, mutual: bool) -> Vec<u8> {
    let options = [if mutual { 0x20 } else { 0 }, 0, 0, 0];
    let fields = [
        der::under(0, &der::number(VERSION)),
        der::under(1, &der::number(14)),
        der::under(2, &der::bits(&options)),
        der::under(3, ticket),
        der::under(4, &proof.write()),
    ];
    der::write(der::message(14), &der::sequence(&fields))
}

/// What a machine answers to a ticket and its proof, sealed with the ticket's key: the
/// time of the proof, repeated, and what it adds for what follows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Returned {
    /// The time of the proof, repeated.
    pub time: (u64, u32),
    /// A key for what follows, where the machine makes one.
    pub key: Option<Key>,
    /// The number the machine counts its messages from.
    pub counts_from: Option<u32>,
}

/// Reads a machine's answer to a ticket: what is sealed in it.
pub fn read_shown(answer: &[u8]) -> Result<Sealed, Error> {
    let message = Reader::new(answer).expect(der::message(15))?;
    let mut inside = message.inside().expect(der::SEQUENCE)?.inside();
    inside.skip_to(2)?;
    Sealed::read(inside.needed(2)?)
}

/// Reads what a machine sealed in its answer to a ticket, once it is opened.
pub fn read_returned(opened: &[u8]) -> Result<Returned, Error> {
    let message = Reader::new(opened).expect(der::message(27))?;
    let mut inside = message.inside().expect(der::SEQUENCE)?.inside();
    let seconds = der::read_time(expect(inside.needed(0)?, der::GENERALIZED_TIME)?.holds)?;
    let millionths = u32::try_from(inside.needed(1)?.number()?).map_err(|_| WRONG)?;
    let key = inside.under(2)?.map(read_key).transpose()?;
    let counts_from = inside.under(3)?.map(|from| from.number().map(|from| from as u32)).transpose()?;
    Ok(Returned { time: (seconds, millionths), key, counts_from })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn bytes(hex: &str) -> Vec<u8> {
        let hex: String = hex.chars().filter(|character| !character.is_whitespace()).collect();
        (0..hex.len()).step_by(2).map(|at| u8::from_str_radix(&hex[at..at + 2], 16).expect("hex")).collect()
    }

    fn asked() -> Asked {
        Asked {
            account: Some(Name::new(NameKind::Account, &["alex.urban"])),
            realm: "CORP.EXAMPLE".to_string(),
            service: Name::new(NameKind::Service, &["krbtgt", "CORP.EXAMPLE"]),
            until: 1_794_787_199,
            nonce: 0x1234_5678,
            kinds: vec![Kind::Aes256, Kind::Aes128],
        }
    }

    #[test]
    fn a_request_is_written_field_by_field_as_the_standard_numbers_them() {
        let body = asked().write();
        let expected = bytes(
            "307a
             a0070305 0040810000
             a1173015 a003020101 a10e300c 1b0a616c65782e757262616e
             a20e1b0c 434f52502e4558414d504c45
             a3213 01f a003020102 a1183016 1b066b7262746774 1b0c434f52502e4558414d504c45
             a511180f 32303236313131353233353935395a
             a7060204 12345678
             a8083006 020112 020111",
        );
        assert_eq!(body, expected);

        let request = request(true, &[with_what_the_account_may()], &body);
        // the first request, version 5, its kind, one thing that goes with it, and what is asked
        // another implementation of the standard reads these bytes as this request, and
        // writes the same bytes from what it read
        assert_eq!(
            request,
            bytes(
                "6a81a230819fa103020105a20302010aa31530133011a10402020080a20904073005a0030101ffa47c307aa00703050040810000a1173015a003020101a10e300c1b0a616c65782e757262616ea20e1b0c434f52502e4558414d504c45a321301fa003020102a11830161b066b72627467741b0c434f52502e4558414d504c45a511180f32303236313131353233353935395aa706020412345678a8083006020112020111"
            )
        );
        let mut inside = Reader::new(&request).expect(der::message(10)).unwrap().inside().expect(der::SEQUENCE).unwrap().inside();
        assert_eq!((inside.needed(1).unwrap().number(), inside.needed(2).unwrap().number()), (Ok(5), Ok(10)));
        assert_eq!(inside.needed(3).unwrap().whole, bytes("3013 3011 a104 02020080 a209 0407 3005 a003 0101ff"));
        assert_eq!(inside.needed(4).unwrap().whole, &body[..]);
        // a later request has the other kind and number
        let later = super::request(false, &[], &body);
        assert_eq!(later[0], 0x6c);
    }

    /// An answer as a domain controller writes it, around what is sealed for the client.
    pub(crate) fn answer(first: bool, extras: &[Extra], ticket: &[u8], sealed: &Sealed) -> Vec<u8> {
        let extras: Vec<Vec<u8>> = extras
            .iter()
            .map(|extra| der::sequence(&[der::under(1, &der::number(extra.kind)), der::under(2, &der::bytes(&extra.holds))]))
            .collect();
        let mut fields = vec![der::under(0, &der::number(5)), der::under(1, &der::number(if first { 11 } else { 13 }))];
        if !extras.is_empty() {
            fields.push(der::under(2, &der::sequence(&extras)));
        }
        fields.push(der::under(3, &der::text("CORP.EXAMPLE")));
        fields.push(der::under(4, &Name::new(NameKind::Account, &["alex.urban"]).write()));
        fields.push(der::under(5, ticket));
        fields.push(der::under(6, &sealed.write()));
        der::write(der::message(if first { 11 } else { 13 }), &der::sequence(&fields))
    }

    /// What a domain controller seals for the client with a ticket.
    pub(crate) fn given(first: bool, key: &Key, nonce: u32, service: &Name) -> Vec<u8> {
        let fields = [
            der::under(0, &write_key(key)),
            der::under(1, &der::sequence(&[])),
            der::under(2, &der::number(i64::from(nonce))),
            der::under(4, &der::bits(&[0x40, 0xe1, 0, 0])),
            der::under(5, &der::time(1_794_700_800)),
            der::under(7, &der::time(1_794_736_800)),
            der::under(9, &der::text("CORP.EXAMPLE")),
            der::under(10, &service.write()),
        ];
        der::write(der::message(if first { 25 } else { 26 }), &der::sequence(&fields))
    }

    /// A ticket, as far as a client looks at one: something of the right kind.
    pub(crate) fn ticket() -> Vec<u8> {
        der::write(der::message(1), &der::sequence(&[der::under(0, &der::number(5)), der::under(1, &der::text("CORP.EXAMPLE"))]))
    }

    #[test]
    fn an_answer_and_what_is_sealed_in_it_are_read() {
        let reply_key = Key::new(Kind::Aes256, &[9; 32]).unwrap();
        let session = Key::new(Kind::Aes256, &[5; 32]).unwrap();
        let service = Name::new(NameKind::Service, &["krbtgt", "CORP.EXAMPLE"]);
        let sealed = Sealed::with(&reply_key, 3, [1; 16], &given(true, &session, 0x1234_5678, &service));
        let extras = [Extra { kind: WITH_A_CERTIFICATE_ANSWER, holds: vec![1, 2, 3] }];
        let Reply::Answered(answered) = read_reply(&answer(true, &extras, &ticket(), &sealed)).unwrap() else { panic!("an answer") };
        assert_eq!(answered.extras, extras);
        assert_eq!((answered.realm.as_str(), &answered.account), ("CORP.EXAMPLE", &Name::new(NameKind::Account, &["alex.urban"])));
        assert_eq!(answered.ticket, ticket());
        let opened = answered.sealed.open(&reply_key, 3).unwrap();
        assert_eq!(
            read_given(&opened),
            Ok(Given {
                key: session.clone(),
                nonce: 0x1234_5678,
                until: 1_794_736_800,
                realm: "CORP.EXAMPLE".to_string(),
                service: service.clone()
            })
        );
        // an answer to a later request, and what it seals under the other number
        let later = Sealed::with(&session, 8, [2; 16], &given(false, &reply_key, 7, &service));
        let Reply::Answered(answered) = read_reply(&answer(false, &[], &ticket(), &later)).unwrap() else { panic!("an answer") };
        assert!(answered.extras.is_empty());
        assert_eq!(read_given(&answered.sealed.open(&session, 8).unwrap()).unwrap().nonce, 7);
        // sealed in another way than the key seals, it is not opened
        let other = Key::new(Kind::Aes128, &[9; 16]).unwrap();
        assert_eq!(answered.sealed.open(&other, 8), Err(Error::NotOpened("it is sealed in another way than the key at hand seals")));

        // cut anywhere, an answer is refused
        let whole = answer(true, &extras, &ticket(), &sealed);
        for cut in 0..whole.len() {
            assert!(read_reply(&whole[..cut]).is_err(), "cut at {cut}");
        }
        assert!(read_given(&opened[..opened.len() - 1]).is_err());
    }

    #[test]
    fn a_refusal_says_why_and_what_time_the_domain_controller_has() {
        let fields = [
            der::under(0, &der::number(5)),
            der::under(1, &der::number(30)),
            der::under(4, &der::time(1_794_700_800)),
            der::under(5, &der::number(12)),
            der::under(6, &der::number(37)),
            der::under(9, &der::text("CORP.EXAMPLE")),
            der::under(10, &Name::new(NameKind::Service, &["krbtgt", "CORP.EXAMPLE"]).write()),
            der::under(11, &der::text("the clocks differ")),
            der::under(12, &der::bytes(&[4, 5])),
        ];
        let refusal = der::write(der::message(30), &der::sequence(&fields));
        assert_eq!(
            read_reply(&refusal),
            Ok(Reply::Refused(Refused {
                code: 37,
                text: Some("the clocks differ".to_string()),
                its_time: 1_794_700_800,
                more: Some(vec![4, 5])
            }))
        );
        // without words and without anything sent along
        let plain = der::write(der::message(30), &der::sequence(&fields[..7]));
        assert_eq!(read_reply(&plain), Ok(Reply::Refused(Refused { code: 37, text: None, its_time: 1_794_700_800, more: None })));
        assert!(read_reply(&der::write(der::message(9), &der::sequence(&[]))).is_err());
    }

    #[test]
    fn a_ticket_is_shown_with_its_proof_and_the_machines_answer_is_read() {
        let session = Key::new(Kind::Aes256, &[5; 32]).unwrap();
        let ours = Key::new(Kind::Aes256, &[6; 32]).unwrap();
        let proof = Proof {
            realm: "CORP.EXAMPLE".to_string(),
            account: Name::new(NameKind::Account, &["alex.urban"]),
            check: Some((0x8003, vec![0x10, 0, 0, 0])),
            time: (1_794_700_800, 123_456),
            key: Some(ours.clone()),
            counts_from: Some(42),
        };
        let written = proof.write();
        // under its own number: the version, the realm, the account, the check, the time, the key, the count
        let mut inside = Reader::new(&written).expect(der::message(2)).unwrap().inside().expect(der::SEQUENCE).unwrap().inside();
        assert_eq!(inside.needed(0).unwrap().number(), Ok(5));
        assert_eq!(inside.needed(1).unwrap().text(), Ok("CORP.EXAMPLE"));
        assert_eq!(Name::read(inside.needed(2).unwrap()), Ok(proof.account.clone()));
        assert_eq!(inside.needed(3).unwrap().whole, bytes("300f a0050203 008003 a1060404 10000000"));
        assert_eq!(inside.needed(4).unwrap().number(), Ok(123_456));
        assert_eq!(inside.needed(5).unwrap().holds, b"20261115000000Z");
        assert_eq!(read_key(inside.needed(6).unwrap()), Ok(ours.clone()));
        assert_eq!(inside.needed(7).unwrap().number(), Ok(42));

        let sealed = Sealed::with(&session, 11, [3; 16], &written);
        let shown = show(&ticket(), &sealed, true);
        let mut inside = Reader::new(&shown).expect(der::message(14)).unwrap().inside().expect(der::SEQUENCE).unwrap().inside();
        assert_eq!((inside.needed(0).unwrap().number(), inside.needed(1).unwrap().number()), (Ok(5), Ok(14)));
        assert_eq!(inside.needed(2).unwrap().holds, [0, 0x20, 0, 0, 0]);
        assert_eq!(inside.needed(3).unwrap().whole, ticket());
        assert_eq!(Sealed::read(inside.needed(4).unwrap()), Ok(sealed));
        // without asking the other side to prove itself
        let plain = super::show(&ticket(), &Sealed::with(&session, 11, [3; 16], &written), false);
        let mut inside = Reader::new(&plain).expect(der::message(14)).unwrap().inside().expect(der::SEQUENCE).unwrap().inside();
        inside.skip_to(2).unwrap();
        assert_eq!(inside.needed(2).unwrap().holds, [0, 0, 0, 0, 0]);

        // the machine's answer: the time repeated, a key of its own, and where it counts from
        let theirs = Key::new(Kind::Aes256, &[7; 32]).unwrap();
        let returned = der::write(
            der::message(27),
            &der::sequence(&[
                der::under(0, &der::time(1_794_700_800)),
                der::under(1, &der::number(123_456)),
                der::under(2, &write_key(&theirs)),
                der::under(3, &der::number(i64::from(u32::MAX))),
            ]),
        );
        let answer = der::write(
            der::message(15),
            &der::sequence(&[
                der::under(0, &der::number(5)),
                der::under(1, &der::number(15)),
                der::under(2, &Sealed::with(&session, 12, [4; 16], &returned).write()),
            ]),
        );
        let opened = read_shown(&answer).unwrap().open(&session, 12).unwrap();
        assert_eq!(read_returned(&opened), Ok(Returned { time: (1_794_700_800, 123_456), key: Some(theirs), counts_from: Some(u32::MAX) }));
        assert!(read_shown(&answer[..answer.len() - 1]).is_err());
    }
}
