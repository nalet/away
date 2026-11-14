//! JSON, as far as Away needs it: what a sign-in service answers, and what the pages and
//! the server say to each other.
//!
//! Reading is strict. Text that is not JSON as RFC 8259 writes it is refused, and so is
//! an object that names something twice, or anything nested deeper than [`DEEPEST`]. How
//! long a text may be is for whoever reads it from a connection to say.

use std::fmt::Write as _;

/// How deep lists and objects may be nested.
pub const DEEPEST: usize = 32;

/// A value.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    /// Nothing.
    Null,
    /// Yes or no.
    Flag(bool),
    /// A number.
    Number(f64),
    /// A text.
    Text(String),
    /// Values in their order.
    List(Vec<Json>),
    /// Values by their names, in the order they were written.
    Object(Vec<(String, Json)>),
}

/// Why a text is no JSON: what is wrong, and at which byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotJson {
    /// What is wrong.
    pub why: &'static str,
    /// At which byte of the text.
    pub at: usize,
}

impl std::fmt::Display for NotJson {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(out, "{}, at byte {}", self.why, self.at)
    }
}

impl Json {
    /// Reads a text that holds one value and nothing else.
    pub fn read(text: &str) -> Result<Json, NotJson> {
        let mut reader = Reader { bytes: text.as_bytes(), at: 0 };
        reader.blanks();
        let value = reader.value(0)?;
        reader.blanks();
        match reader.at == reader.bytes.len() {
            true => Ok(value),
            false => Err(reader.wrong("something follows the value")),
        }
    }

    /// What an object holds under a name.
    pub fn get(&self, name: &str) -> Option<&Json> {
        match self {
            Json::Object(held) => held.iter().find(|(its, _)| its == name).map(|(_, value)| value),
            _ => None,
        }
    }

    /// The text, of a text.
    pub fn text(&self) -> Option<&str> {
        match self {
            Json::Text(text) => Some(text),
            _ => None,
        }
    }

    /// The text an object holds under a name.
    pub fn text_of(&self, name: &str) -> Option<&str> {
        self.get(name).and_then(Json::text)
    }

    /// The number, of a number that counts something: whole, not below zero, and small
    /// enough to be exact.
    pub fn count(&self) -> Option<u64> {
        match self {
            Json::Number(number) if *number >= 0.0 && number.fract() == 0.0 && *number <= 9_007_199_254_740_992.0 => Some(*number as u64),
            _ => None,
        }
    }

    /// Yes or no, of a flag.
    pub fn flag(&self) -> Option<bool> {
        match self {
            Json::Flag(flag) => Some(*flag),
            _ => None,
        }
    }

    /// The values, of a list.
    pub fn list(&self) -> Option<&[Json]> {
        match self {
            Json::List(values) => Some(values),
            _ => None,
        }
    }

    /// An object of these names and values.
    pub fn object<const N: usize>(held: [(&str, Json); N]) -> Json {
        Json::Object(held.into_iter().map(|(name, value)| (name.to_string(), value)).collect())
    }

    /// The value as text, on one line.
    pub fn write(&self) -> String {
        let mut out = String::new();
        self.write_to(&mut out);
        out
    }

    fn write_to(&self, out: &mut String) {
        match self {
            Json::Null => out.push_str("null"),
            Json::Flag(flag) => out.push_str(if *flag { "true" } else { "false" }),
            // what is no number can't be written as one
            Json::Number(number) if !number.is_finite() => out.push_str("null"),
            Json::Number(number) => {
                let _ = write!(out, "{number}");
            }
            Json::Text(text) => write_text(out, text),
            Json::List(values) => {
                out.push('[');
                for (nth, value) in values.iter().enumerate() {
                    if nth > 0 {
                        out.push(',');
                    }
                    value.write_to(out);
                }
                out.push(']');
            }
            Json::Object(held) => {
                out.push('{');
                for (nth, (name, value)) in held.iter().enumerate() {
                    if nth > 0 {
                        out.push(',');
                    }
                    write_text(out, name);
                    out.push(':');
                    value.write_to(out);
                }
                out.push('}');
            }
        }
    }
}

impl From<&str> for Json {
    fn from(text: &str) -> Json {
        Json::Text(text.to_string())
    }
}

impl From<String> for Json {
    fn from(text: String) -> Json {
        Json::Text(text)
    }
}

impl From<bool> for Json {
    fn from(flag: bool) -> Json {
        Json::Flag(flag)
    }
}

impl From<u64> for Json {
    fn from(number: u64) -> Json {
        Json::Number(number as f64)
    }
}

/// Writes a text between quotes, with what JSON can't hold as it is written around.
pub fn write_text(out: &mut String, text: &str) {
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // the rest of what can't be seen, and the two that end a line in a script
            control if (control as u32) < 0x20 || matches!(control, '\u{2028}' | '\u{2029}') => {
                let _ = write!(out, "\\u{:04x}", control as u32);
            }
            other => out.push(other),
        }
    }
    out.push('"');
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn wrong(&self, why: &'static str) -> NotJson {
        NotJson { why, at: self.at }
    }

    fn blanks(&mut self) {
        while matches!(self.bytes.get(self.at), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn word(&mut self, word: &str, value: Json) -> Result<Json, NotJson> {
        match self.bytes[self.at..].starts_with(word.as_bytes()) {
            true => {
                self.at += word.len();
                Ok(value)
            }
            false => Err(self.wrong("a word that is none of true, false and null")),
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, NotJson> {
        match self.bytes.get(self.at) {
            None => Err(self.wrong("the text ends where a value should be")),
            Some(b'n') => self.word("null", Json::Null),
            Some(b't') => self.word("true", Json::Flag(true)),
            Some(b'f') => self.word("false", Json::Flag(false)),
            Some(b'"') => self.text().map(Json::Text),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(b'[' | b'{') if depth >= DEEPEST => Err(self.wrong("it is nested too deep")),
            Some(b'[') => {
                self.at += 1;
                let mut values = Vec::new();
                self.blanks();
                if self.bytes.get(self.at) == Some(&b']') {
                    self.at += 1;
                    return Ok(Json::List(values));
                }
                loop {
                    self.blanks();
                    values.push(self.value(depth + 1)?);
                    self.blanks();
                    match self.bytes.get(self.at) {
                        Some(b',') => self.at += 1,
                        Some(b']') => {
                            self.at += 1;
                            return Ok(Json::List(values));
                        }
                        _ => return Err(self.wrong("a list goes on with neither a comma nor its end")),
                    }
                }
            }
            Some(b'{') => {
                self.at += 1;
                let mut held: Vec<(String, Json)> = Vec::new();
                self.blanks();
                if self.bytes.get(self.at) == Some(&b'}') {
                    self.at += 1;
                    return Ok(Json::Object(held));
                }
                loop {
                    self.blanks();
                    if self.bytes.get(self.at) != Some(&b'"') {
                        return Err(self.wrong("an object's name is no text"));
                    }
                    let name = self.text()?;
                    if held.iter().any(|(its, _)| *its == name) {
                        return Err(self.wrong("an object names something twice"));
                    }
                    self.blanks();
                    if self.bytes.get(self.at) != Some(&b':') {
                        return Err(self.wrong("an object's name is followed by no colon"));
                    }
                    self.at += 1;
                    self.blanks();
                    held.push((name, self.value(depth + 1)?));
                    self.blanks();
                    match self.bytes.get(self.at) {
                        Some(b',') => self.at += 1,
                        Some(b'}') => {
                            self.at += 1;
                            return Ok(Json::Object(held));
                        }
                        _ => return Err(self.wrong("an object goes on with neither a comma nor its end")),
                    }
                }
            }
            Some(_) => Err(self.wrong("this begins no value")),
        }
    }

    fn number(&mut self) -> Result<Json, NotJson> {
        let from = self.at;
        let digits = |reader: &mut Self| {
            let before = reader.at;
            while matches!(reader.bytes.get(reader.at), Some(b'0'..=b'9')) {
                reader.at += 1;
            }
            reader.at - before
        };
        if self.bytes.get(self.at) == Some(&b'-') {
            self.at += 1;
        }
        // a whole part: a zero alone, or digits that begin with none
        match self.bytes.get(self.at) {
            Some(b'0') => self.at += 1,
            Some(b'1'..=b'9') => {
                digits(self);
            }
            _ => return Err(self.wrong("a number has no digits")),
        }
        if self.bytes.get(self.at) == Some(&b'.') {
            self.at += 1;
            if digits(self) == 0 {
                return Err(self.wrong("a number's point is followed by no digit"));
            }
        }
        if matches!(self.bytes.get(self.at), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.bytes.get(self.at), Some(b'+' | b'-')) {
                self.at += 1;
            }
            if digits(self) == 0 {
                return Err(self.wrong("a number's exponent has no digits"));
            }
        }
        let written = std::str::from_utf8(&self.bytes[from..self.at]).unwrap_or_default();
        match written.parse::<f64>() {
            Ok(number) if number.is_finite() => Ok(Json::Number(number)),
            _ => Err(NotJson { why: "a number is too large", at: from }),
        }
    }

    /// Four digits of sixteen, after `\u`.
    fn unit(&mut self) -> Result<u32, NotJson> {
        let digits = self.bytes.get(self.at..self.at + 4).ok_or(self.wrong("an escape is cut short"))?;
        let mut unit = 0;
        for digit in digits {
            unit = unit * 16 + (*digit as char).to_digit(16).ok_or(self.wrong("an escape holds what is no digit"))?;
        }
        self.at += 4;
        Ok(unit)
    }

    fn text(&mut self) -> Result<String, NotJson> {
        self.at += 1;
        let mut text = Vec::new();
        loop {
            let from = self.at;
            while !matches!(self.bytes.get(self.at), None | Some(b'"' | b'\\' | 0..=0x1f)) {
                self.at += 1;
            }
            text.extend_from_slice(&self.bytes[from..self.at]);
            match self.bytes.get(self.at) {
                None => return Err(self.wrong("a text has no end")),
                Some(b'"') => {
                    self.at += 1;
                    // the text was valid before, and every escape adds whole characters
                    return String::from_utf8(text).map_err(|_| self.wrong("a text is cut inside a character"));
                }
                Some(b'\\') => {
                    self.at += 1;
                    let escaped = *self.bytes.get(self.at).ok_or(self.wrong("an escape is cut short"))?;
                    self.at += 1;
                    let character = match escaped {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let first = self.unit()?;
                            let point = match first {
                                // the first half of a pair, which the second half has to follow
                                0xd800..=0xdbff => {
                                    if self.bytes.get(self.at..self.at + 2) != Some(b"\\u") {
                                        return Err(self.wrong("half a character stands alone"));
                                    }
                                    self.at += 2;
                                    let second = self.unit()?;
                                    if !(0xdc00..=0xdfff).contains(&second) {
                                        return Err(self.wrong("half a character stands alone"));
                                    }
                                    0x10000 + ((first - 0xd800) << 10) + (second - 0xdc00)
                                }
                                0xdc00..=0xdfff => return Err(self.wrong("half a character stands alone")),
                                other => other,
                            };
                            char::from_u32(point).ok_or(self.wrong("an escape names no character"))?
                        }
                        _ => {
                            self.at -= 1;
                            return Err(self.wrong("an escape that JSON doesn't have"));
                        }
                    };
                    text.extend_from_slice(character.encode_utf8(&mut [0; 4]).as_bytes());
                }
                Some(_) => return Err(self.wrong("a text holds a character that has to be escaped")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_of_every_kind_are_read() {
        let read = Json::read(r#" {"name":"alex", "groups":["staff","finance"], "age": 41, "ratio": -1.5e2, "active": true, "none": null, "in": {"deep": []}} "#)
            .unwrap();
        assert_eq!(read.text_of("name"), Some("alex"));
        assert_eq!(read.get("groups").and_then(Json::list).map(<[Json]>::len), Some(2));
        assert_eq!(read.get("age").and_then(Json::count), Some(41));
        assert_eq!(read.get("ratio"), Some(&Json::Number(-150.0)));
        assert_eq!(read.get("ratio").and_then(Json::count), None, "what is below zero counts nothing");
        assert_eq!(read.get("active").and_then(Json::flag), Some(true));
        assert_eq!(read.get("none"), Some(&Json::Null));
        assert_eq!(read.get("in").and_then(|inner| inner.get("deep")), Some(&Json::List(Vec::new())));
        assert_eq!(read.get("absent"), None);
        assert_eq!(Json::read("0").unwrap(), Json::Number(0.0));
        assert_eq!(Json::read("\"\"").unwrap(), Json::Text(String::new()));
        assert_eq!(Json::read("{}").unwrap(), Json::Object(Vec::new()));
    }

    #[test]
    fn texts_are_read_with_their_escapes() {
        assert_eq!(Json::read(r#""a\"b\\c\/d\b\f\n\r\t""#).unwrap().text(), Some("a\"b\\c/d\u{8}\u{c}\n\r\t"));
        assert_eq!(Json::read(r#""\u00e9\u20ac""#).unwrap().text(), Some("é€"));
        assert_eq!(Json::read(r#""\ud83d\ude00""#).unwrap().text(), Some("😀"), "a character written as its two halves");
        assert_eq!(Json::read("\"Zürich 😀\"").unwrap().text(), Some("Zürich 😀"));
        for wrong in [
            r#""\ud83d""#,
            r#""\ud83dx""#,
            r#""\ude00""#,
            r#""\ud83d\u0041""#,
            r#""\x41""#,
            r#""\u12""#,
            r#""\u12g4""#,
            "\"a\nb\"",
            "\"open",
        ] {
            assert!(Json::read(wrong).is_err(), "{wrong}");
        }
    }

    #[test]
    fn what_is_not_json_is_refused_and_says_where() {
        for wrong in [
            "",
            " ",
            "nul",
            "True",
            "'text'",
            "01",
            "1.",
            ".5",
            "-",
            "1e",
            "1e+",
            "+1",
            "0x10",
            "1e999",
            "[1,]",
            "[1 2]",
            "[",
            "{\"a\"}",
            "{\"a\":}",
            "{\"a\":1,}",
            "{a:1}",
            "{\"a\":1 \"b\":2}",
            "{",
            "1 2",
            "[] []",
            "\u{feff}1",
            "NaN",
            "Infinity",
        ] {
            assert!(Json::read(wrong).is_err(), "{wrong:?}");
        }
        assert_eq!(Json::read("[1, 2, x]"), Err(NotJson { why: "this begins no value", at: 7 }));
        assert_eq!(Json::read("[1, 2, x]").unwrap_err().to_string(), "this begins no value, at byte 7");
    }

    #[test]
    fn an_object_that_names_something_twice_is_refused() {
        assert_eq!(Json::read(r#"{"sub":"alex","sub":"root"}"#).unwrap_err().why, "an object names something twice");
        // written differently, and the same name all the same
        assert_eq!(Json::read(r#"{"sub":"alex","s\u0075b":"root"}"#).unwrap_err().why, "an object names something twice");
    }

    #[test]
    fn what_is_nested_too_deep_is_refused() {
        let nested = |depth: usize| format!("{}1{}", "[".repeat(depth), "]".repeat(depth));
        assert!(Json::read(&nested(DEEPEST)).is_ok());
        assert_eq!(Json::read(&nested(DEEPEST + 1)).unwrap_err().why, "it is nested too deep");
        // and it takes no memory to say so
        assert!(Json::read(&"[".repeat(1_000_000)).is_err());
        assert!(Json::read(&"{\"a\":".repeat(100_000)).is_err());
    }

    #[test]
    fn values_are_written_as_json_reads_them() {
        let value = Json::object([
            ("name", "Zoë \"Z\" O'Neil\\\n".into()),
            ("groups", Json::List(vec!["staff".into(), "finance".into()])),
            ("sessions", 2u64.into()),
            ("ratio", Json::Number(-0.5)),
            ("admin", false.into()),
            ("contact", Json::Null),
        ]);
        let written = value.write();
        assert_eq!(
            written,
            r#"{"name":"Zoë \"Z\" O'Neil\\\n","groups":["staff","finance"],"sessions":2,"ratio":-0.5,"admin":false,"contact":null}"#
        );
        assert_eq!(Json::read(&written).unwrap(), value);
        // what can't be seen is written as its number, and what is no number as nothing
        assert_eq!(Json::Text("\u{1}\u{2028}".to_string()).write(), r#""\u0001\u2028""#);
        assert_eq!(Json::Number(f64::NAN).write(), "null");
        assert_eq!(Json::Number(1e21).write(), "1000000000000000000000");
        assert_eq!(Json::read(&Json::Number(1e21).write()).unwrap(), Json::Number(1e21));
    }

    #[test]
    fn a_count_is_whole_and_exact() {
        assert_eq!(Json::read("1700000000").unwrap().count(), Some(1_700_000_000));
        assert_eq!(Json::read("1.0").unwrap().count(), Some(1));
        assert_eq!(Json::read("1.5").unwrap().count(), None);
        assert_eq!(Json::read("-1").unwrap().count(), None);
        assert_eq!(Json::read("1e300").unwrap().count(), None);
        assert_eq!(Json::read("\"1\"").unwrap().count(), None);
    }
}
