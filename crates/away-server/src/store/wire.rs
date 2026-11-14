//! The messages the server and a PostgreSQL database send each other (the database's
//! frontend/backend protocol, version 3.0), as far as Away uses them: beginning,
//! signing in with a password that is never sent, one statement with its values, and
//! several statements at once for the database's shape.
//!
//! Nothing here opens a connection: bytes go in and messages come out, and the other
//! way round.

/// The longest message that is taken from a database.
pub const LONGEST: usize = 16 * 1024 * 1024;

/// Asks the database to secure the connection before anything else is said. It answers
/// with one byte: `S` where it will, `N` where it won't.
pub const SECURE_IT: [u8; 8] = [0, 0, 0, 8, 0x04, 0xd2, 0x16, 0x2f];

/// What a database sent that Away can't go on with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotThis(pub &'static str);

/// Why the database refused something, in its own words.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Refusal {
    /// How grave it is: `ERROR`, `FATAL`, `PANIC`.
    pub severity: String,
    /// The database's code for it, five characters.
    pub code: String,
    /// What it says.
    pub message: String,
}

/// What the database asks of whoever signs in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Asks {
    /// Nothing more: the account is signed in.
    Nothing,
    /// A proof of the password, in one of these ways.
    Proof(Vec<String>),
    /// Its answer to the first step of the proof.
    Goes(Vec<u8>),
    /// Its own proof, at the end.
    Ends(Vec<u8>),
    /// A way of signing in that Away doesn't use: the password itself, or its old hash.
    Other(u32),
}

/// One message of the database.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FromDatabase {
    /// A step of signing in.
    SignIn(Asks),
    /// It is ready for the next statement. The byte says whether a transaction is open.
    Ready(u8),
    /// It refused what was asked.
    Refused(Refusal),
    /// A row: each value as the text the database writes it as, or nothing.
    Row(Vec<Option<Vec<u8>>>),
    /// A statement is done: what it did, and to how many rows.
    Done(String),
    /// Something Away has no use for: a setting of the database, a notice, a step of a
    /// statement that needs no answer.
    Passed,
}

/// A whole message as it arrived: its kind, what it carries, and how many bytes it took.
pub type Framed<'arrived> = (u8, &'arrived [u8], usize);

/// The first whole message in what arrived. Nothing where the message is not whole yet.
pub fn frame(arrived: &[u8]) -> Result<Option<Framed<'_>>, NotThis> {
    let Some(head) = arrived.get(..5) else { return Ok(None) };
    let length = u32::from_be_bytes([head[1], head[2], head[3], head[4]]) as usize;
    if !(4..=LONGEST).contains(&length) {
        return Err(NotThis("a message of the database has a length that no message has"));
    }
    Ok(arrived.get(5..1 + length).map(|carried| (head[0], carried, 1 + length)))
}

/// Reads one message.
pub fn read(kind: u8, carried: &[u8]) -> Result<FromDatabase, NotThis> {
    let mut left = carried;
    Ok(match kind {
        b'R' => FromDatabase::SignIn(match number(&mut left)? {
            0 => Asks::Nothing,
            10 => {
                let mut ways = Vec::new();
                while let Some(way) = text(&mut left)?.filter(|way| !way.is_empty()) {
                    ways.push(way);
                }
                Asks::Proof(ways)
            }
            11 => Asks::Goes(left.to_vec()),
            12 => Asks::Ends(left.to_vec()),
            other => Asks::Other(other),
        }),
        b'Z' => FromDatabase::Ready(*left.first().ok_or(NotThis("the database says it is ready and not in which state"))?),
        b'E' => {
            let mut refusal = Refusal::default();
            while let Some((&field, rest)) = left.split_first().filter(|(field, _)| **field != 0) {
                left = rest;
                let said = text(&mut left)?.ok_or(NotThis("a refusal of the database ends in the middle"))?;
                match field {
                    b'S' => refusal.severity = said,
                    b'C' => refusal.code = said,
                    b'M' => refusal.message = said,
                    _ => {}
                }
            }
            FromDatabase::Refused(refusal)
        }
        b'D' => {
            let count = short(&mut left)?;
            let mut row = Vec::with_capacity(usize::from(count));
            for _ in 0..count {
                let length = number(&mut left)?;
                if length == u32::MAX {
                    row.push(None);
                    continue;
                }
                let value = left.get(..length as usize).ok_or(NotThis("a row of the database ends in the middle"))?;
                left = &left[length as usize..];
                row.push(Some(value.to_vec()));
            }
            FromDatabase::Row(row)
        }
        b'C' => FromDatabase::Done(text(&mut left)?.unwrap_or_default()),
        _ => FromDatabase::Passed,
    })
}

fn number(left: &mut &[u8]) -> Result<u32, NotThis> {
    let four = left.get(..4).ok_or(NotThis("a message of the database ends in the middle"))?;
    *left = &left[4..];
    Ok(u32::from_be_bytes([four[0], four[1], four[2], four[3]]))
}

fn short(left: &mut &[u8]) -> Result<u16, NotThis> {
    let two = left.get(..2).ok_or(NotThis("a message of the database ends in the middle"))?;
    *left = &left[2..];
    Ok(u16::from_be_bytes([two[0], two[1]]))
}

/// A text that ends with a zero byte. Nothing where no bytes are left.
fn text(left: &mut &[u8]) -> Result<Option<String>, NotThis> {
    if left.is_empty() {
        return Ok(None);
    }
    let end = left.iter().position(|byte| *byte == 0).ok_or(NotThis("a text of the database has no end"))?;
    let read = String::from_utf8_lossy(&left[..end]).into_owned();
    *left = &left[end + 1..];
    Ok(Some(read))
}

/// A message to the database: its kind, and what it carries.
fn message(kind: u8, carried: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(5 + carried.len());
    out.push(kind);
    out.extend_from_slice(&(carried.len() as u32 + 4).to_be_bytes());
    out.extend_from_slice(carried);
    out
}

/// A text as the protocol carries it, which ends with a zero byte and so holds none.
fn ended(out: &mut Vec<u8>, text: &str) -> Result<(), NotThis> {
    if text.as_bytes().contains(&0) {
        return Err(NotThis("a text for the database holds a zero byte, which no text there may"));
    }
    out.extend_from_slice(text.as_bytes());
    out.push(0);
    Ok(())
}

/// The first message: which account, and which database of the server.
pub fn begin(account: &str, database: &str) -> Result<Vec<u8>, NotThis> {
    let mut carried = Vec::new();
    carried.extend_from_slice(&196_608u32.to_be_bytes());
    for (name, value) in [("user", account), ("database", database), ("client_encoding", "UTF8"), ("application_name", "away")] {
        ended(&mut carried, name)?;
        ended(&mut carried, value)?;
    }
    carried.push(0);
    let mut out = Vec::with_capacity(4 + carried.len());
    out.extend_from_slice(&(carried.len() as u32 + 4).to_be_bytes());
    out.extend_from_slice(&carried);
    Ok(out)
}

/// The first step of proving the password: the way chosen, and what the client says.
pub fn proof_begins(way: &str, says: &[u8]) -> Result<Vec<u8>, NotThis> {
    let mut carried = Vec::new();
    ended(&mut carried, way)?;
    carried.extend_from_slice(&(says.len() as u32).to_be_bytes());
    carried.extend_from_slice(says);
    Ok(message(b'p', &carried))
}

/// The second step of proving the password.
pub fn proof_goes_on(says: &[u8]) -> Vec<u8> {
    message(b'p', says)
}

/// Several statements at once, without values: for the database's shape.
pub fn run(statements: &str) -> Result<Vec<u8>, NotThis> {
    let mut carried = Vec::new();
    ended(&mut carried, statements)?;
    Ok(message(b'Q', &carried))
}

/// One statement with its values, each as text or nothing, never written into the
/// statement itself: the database takes the statement and the values apart. The rows
/// come back as text.
pub fn ask(statement: &str, values: &[Option<&str>]) -> Result<Vec<u8>, NotThis> {
    let count = u16::try_from(values.len()).map_err(|_| NotThis("a statement has more values than a statement may"))?;
    // the statement, nameless, with no kinds given for its values
    let mut parse = vec![0];
    ended(&mut parse, statement)?;
    parse.extend_from_slice(&0u16.to_be_bytes());
    // its values: all as text, into the nameless portal, and the rows as text
    let mut bind = vec![0, 0];
    bind.extend_from_slice(&0u16.to_be_bytes());
    bind.extend_from_slice(&count.to_be_bytes());
    for value in values {
        match value {
            None => bind.extend_from_slice(&u32::MAX.to_be_bytes()),
            Some(value) => {
                if value.as_bytes().contains(&0) {
                    return Err(NotThis("a value for the database holds a zero byte, which no text there may"));
                }
                bind.extend_from_slice(&(value.len() as u32).to_be_bytes());
                bind.extend_from_slice(value.as_bytes());
            }
        }
    }
    bind.extend_from_slice(&0u16.to_be_bytes());
    // run it, every row, and end
    let mut execute = vec![0];
    execute.extend_from_slice(&0u32.to_be_bytes());
    let mut out = message(b'P', &parse);
    out.extend(message(b'B', &bind));
    out.extend(message(b'E', &execute));
    out.extend(message(b'S', &[]));
    Ok(out)
}

/// The last message: the server leaves.
pub fn leave() -> Vec<u8> {
    message(b'X', &[])
}

/// How many rows a statement that is done changed: the number its word ends with.
pub fn changed(done: &str) -> u64 {
    done.rsplit(' ').next().and_then(|count| count.parse().ok()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(bytes: &[u8]) -> FromDatabase {
        let (kind, carried, took) = frame(bytes).unwrap().expect("a whole message");
        assert_eq!(took, bytes.len());
        read(kind, carried).unwrap()
    }

    #[test]
    fn the_first_message_names_the_account_and_the_database() {
        let first = begin("away", "records").unwrap();
        assert_eq!(&first[..8], [0, 0, 0, first.len() as u8, 0, 3, 0, 0]);
        assert_eq!(&first[8..], b"user\0away\0database\0records\0client_encoding\0UTF8\0application_name\0away\0\0");
        assert_eq!(begin("a\0b", "c"), Err(NotThis("a text for the database holds a zero byte, which no text there may")));
    }

    #[test]
    fn a_statement_goes_apart_from_its_values() {
        let asked = ask("select $1, $2", &[Some("it's"), None]).unwrap();
        let mut expected = Vec::new();
        expected.extend_from_slice(b"P\0\0\0\x15\0select $1, $2\0\0\0");
        expected.extend_from_slice(b"B\0\0\0\x18\0\0\0\0\0\x02\0\0\0\x04it's\xff\xff\xff\xff\0\0");
        expected.extend_from_slice(b"E\0\0\0\x09\0\0\0\0\0");
        expected.extend_from_slice(b"S\0\0\0\x04");
        assert_eq!(asked, expected);
        assert!(ask("select $1", &[Some("a\0b")]).is_err());
        assert_eq!(run("begin; commit").unwrap(), b"Q\0\0\0\x12begin; commit\0");
        assert_eq!(leave(), b"X\0\0\0\x04");
    }

    #[test]
    fn the_steps_of_signing_in_are_read() {
        assert_eq!(one(b"R\0\0\0\x08\0\0\0\0"), FromDatabase::SignIn(Asks::Nothing));
        assert_eq!(
            one(b"R\0\0\0\x2a\0\0\0\x0aSCRAM-SHA-256-PLUS\0SCRAM-SHA-256\0\0"),
            FromDatabase::SignIn(Asks::Proof(vec!["SCRAM-SHA-256-PLUS".to_string(), "SCRAM-SHA-256".to_string()]))
        );
        assert_eq!(one(b"R\0\0\0\x0b\0\0\0\x0br=x"), FromDatabase::SignIn(Asks::Goes(b"r=x".to_vec())));
        assert_eq!(one(b"R\0\0\0\x0b\0\0\0\x0cv=y"), FromDatabase::SignIn(Asks::Ends(b"v=y".to_vec())));
        assert_eq!(one(b"R\0\0\0\x08\0\0\0\x03"), FromDatabase::SignIn(Asks::Other(3)));
        assert_eq!(proof_begins("SCRAM-SHA-256", b"n,,n=,r=abc").unwrap(), b"p\0\0\0\x21SCRAM-SHA-256\0\0\0\0\x0bn,,n=,r=abc");
        assert_eq!(proof_goes_on(b"c=biws"), b"p\0\0\0\x0ac=biws");
    }

    #[test]
    fn rows_refusals_and_what_a_statement_did_are_read() {
        assert_eq!(
            one(b"D\0\0\0\x15\0\x03\0\0\0\x02ab\xff\xff\xff\xff\0\0\0\x01c"),
            FromDatabase::Row(vec![Some(b"ab".to_vec()), None, Some(b"c".to_vec())])
        );
        assert_eq!(one(b"C\0\0\0\x0fINSERT 0 3\0"), FromDatabase::Done("INSERT 0 3".to_string()));
        assert_eq!((changed("INSERT 0 3"), changed("DELETE 12"), changed("CREATE TABLE"), changed("")), (3, 12, 0, 0));
        assert_eq!(one(b"Z\0\0\0\x05I"), FromDatabase::Ready(b'I'));
        let refused = one(b"E\0\0\0\x2eSERROR\0VERROR\0C23505\0Mduplicate key\0Fx.c\0\0");
        assert_eq!(
            refused,
            FromDatabase::Refused(Refusal {
                severity: "ERROR".to_string(),
                code: "23505".to_string(),
                message: "duplicate key".to_string()
            })
        );
        // what Away has no use for passes: a setting, a notice, a step that needs no answer
        for passed in [&b"S\0\0\0\x08a\0b\0"[..], b"1\0\0\0\x04", b"2\0\0\0\x04", b"N\0\0\0\x05\0"] {
            assert_eq!(one(passed), FromDatabase::Passed);
        }
    }

    #[test]
    fn a_message_is_taken_when_it_is_whole_and_refused_when_it_is_cut_or_far_too_long() {
        let whole = b"C\0\0\0\x0dSELECT 1\0Z\0\0\0\x05I";
        for cut in 0..14 {
            assert_eq!(frame(&whole[..cut]), Ok(None), "cut at {cut}");
        }
        assert_eq!(frame(whole).unwrap().map(|(kind, _, took)| (kind, took)), Some((b'C', 14)));
        assert!(frame(b"C\0\0\0\x03").is_err());
        assert!(frame(b"C\xff\xff\xff\xff").is_err());
        // a message that says more than it holds is no message
        for broken in [&b"R\0\0\0\x06\0\0"[..], b"D\0\0\0\x0a\0\x01\0\0\0\x09", b"E\0\0\0\x07Sab", b"Z\0\0\0\x04"] {
            let (kind, carried, _) = frame(broken).unwrap().unwrap();
            assert!(read(kind, carried).is_err(), "{broken:?}");
        }
    }
}
