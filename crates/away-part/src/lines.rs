//! The lines a part writes: one a happening, on its output, as an object a log collector
//! reads.
//!
//! ```text
//! {"time":"2026-11-14T09:14:02.120Z","level":"info","part":"server","event":"listening","on":"0.0.0.0:8080"}
//! ```
//!
//! Every line has its time, its level, the part that wrote it and what happened, in that
//! order, then the facts that belong to it. A line is written whole or not at all.

use std::fmt::Write as _;
use std::io::Write as _;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::time;

/// One fact of a line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value<'a> {
    /// A text.
    Text(&'a str),
    /// A count or a size.
    Number(u64),
    /// Yes or no.
    Flag(bool),
}

impl<'a> From<&'a str> for Value<'a> {
    fn from(text: &'a str) -> Self {
        Value::Text(text)
    }
}

impl<'a> From<&'a String> for Value<'a> {
    fn from(text: &'a String) -> Self {
        Value::Text(text)
    }
}

impl From<u64> for Value<'_> {
    fn from(number: u64) -> Self {
        Value::Number(number)
    }
}

impl From<bool> for Value<'_> {
    fn from(flag: bool) -> Self {
        Value::Flag(flag)
    }
}

/// Where the lines go.
enum Sink {
    /// The part's own output.
    Output,
    /// A list, for a test to read.
    Kept(Mutex<Vec<String>>),
}

/// The lines of one part.
#[derive(Clone)]
pub struct Lines {
    part: &'static str,
    sink: Arc<Sink>,
}

impl Lines {
    /// Lines that go to the part's output.
    pub fn to_output(part: &'static str) -> Self {
        Lines { part, sink: Arc::new(Sink::Output) }
    }

    /// Lines that are kept, for a test to read with [`Lines::kept`].
    pub fn kept_for(part: &'static str) -> Self {
        Lines { part, sink: Arc::new(Sink::Kept(Mutex::new(Vec::new()))) }
    }

    /// The lines kept so far. Lines that go to the output are not kept.
    pub fn kept(&self) -> Vec<String> {
        match &*self.sink {
            Sink::Output => Vec::new(),
            Sink::Kept(lines) => lines.lock().map(|lines| lines.clone()).unwrap_or_default(),
        }
    }

    /// Something happened, as expected.
    pub fn info(&self, event: &str, facts: &[(&str, Value)]) {
        self.write("info", event, facts);
    }

    /// Something happened that somebody should look at, and the part goes on.
    pub fn warn(&self, event: &str, facts: &[(&str, Value)]) {
        self.write("warn", event, facts);
    }

    /// Something failed.
    pub fn error(&self, event: &str, facts: &[(&str, Value)]) {
        self.write("error", event, facts);
    }

    fn write(&self, level: &str, event: &str, facts: &[(&str, Value)]) {
        let mut line = line(time::now(), level, self.part, event, facts);
        match &*self.sink {
            Sink::Output => {
                line.push('\n');
                // one write for the whole line, so two parts of a line never part ways. a
                // line that can't be written has nowhere else to go.
                let _ = std::io::stdout().lock().write_all(line.as_bytes());
            }
            Sink::Kept(lines) => {
                if let Ok(mut lines) = lines.lock() {
                    lines.push(line);
                }
            }
        }
    }
}

/// One line, without its end.
fn line(at: Duration, level: &str, part: &str, event: &str, facts: &[(&str, Value)]) -> String {
    let mut out = String::with_capacity(160);
    out.push_str("{\"time\":\"");
    out.push_str(&time::rfc3339(at));
    out.push_str("\",\"level\":");
    text(&mut out, level);
    out.push_str(",\"part\":");
    text(&mut out, part);
    out.push_str(",\"event\":");
    text(&mut out, event);
    for (name, value) in facts {
        out.push(',');
        text(&mut out, name);
        out.push(':');
        match value {
            Value::Text(value) => text(&mut out, value),
            Value::Number(value) => {
                let _ = write!(out, "{value}");
            }
            Value::Flag(value) => out.push_str(if *value { "true" } else { "false" }),
        }
    }
    out.push('}');
    out
}

/// A text as JSON writes it: in quotes, with what would end it or break the line escaped.
fn text(out: &mut String, text: &str) {
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if (control as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", control as u32);
            }
            other => out.push(other),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_has_its_four_fields_first_and_then_its_facts() {
        let written = line(
            Duration::new(1_700_000_000, 120_000_000),
            "info",
            "server",
            "listening",
            &[("on", Value::Text("0.0.0.0:8080")), ("connections", Value::Number(3)), ("ready", Value::Flag(true))],
        );
        assert_eq!(
            written,
            r#"{"time":"2023-11-14T22:13:20.120Z","level":"info","part":"server","event":"listening","on":"0.0.0.0:8080","connections":3,"ready":true}"#
        );
    }

    #[test]
    fn what_would_break_a_line_is_escaped() {
        let mut out = String::new();
        text(&mut out, "a \"quoted\" \\ path\nsecond line\ttab \u{1} é 日本");
        assert_eq!(out, r#""a \"quoted\" \\ path\nsecond line\ttab \u0001 é 日本""#);
    }

    #[test]
    fn kept_lines_can_be_read_back() {
        let lines = Lines::kept_for("gateway");
        lines.info("started", &[]);
        lines.warn("slow", &[("seconds", 4.into())]);
        let kept = lines.kept();
        assert_eq!(kept.len(), 2);
        assert!(kept[0].contains(r#""level":"info","part":"gateway","event":"started"}"#));
        assert!(kept[1].ends_with(r#""level":"warn","part":"gateway","event":"slow","seconds":4}"#));
    }
}
