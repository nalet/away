//! A file of the register as a tree: texts, lists, and things with named parts, each
//! with the line it is written in. The reading of YAML itself is a library's; this takes
//! what the library reads and keeps only what a file of machines and rules is made of.

use saphyr_parser::{Event, Parser};

/// How deep a file's lists and things may be inside each other, and how many values it
/// may hold: far more than any register needs, and little enough to cost nothing.
const DEEPEST: usize = 16;
const MOST: usize = 200_000;

/// What a value is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// A text. A number and a yes or no are texts too, until what holds them reads them.
    Text(String),
    /// A list.
    List(Vec<Node>),
    /// A thing with named parts, in the order they are written.
    Thing(Vec<(Node, Node)>),
}

/// A value, and the line it begins in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    /// What it is.
    pub value: Value,
    /// The line it begins in, counted from 1.
    pub line: usize,
}

/// Why a file can't be read: the line, and what is wrong there.
pub type Wrong = (usize, String);

/// Reads a file. Gives nothing for a file that holds nothing but comments.
pub fn read(text: &str) -> Result<Option<Node>, Wrong> {
    // what is being built: lists and things that have begun, innermost last
    enum Open {
        List(Vec<Node>, usize),
        Thing(Vec<(Node, Node)>, Option<Node>, usize),
    }
    let mut open: Vec<Open> = Vec::new();
    let mut whole: Option<Node> = None;
    let (mut documents, mut values) = (0, 0usize);

    for event in Parser::new_from_str(text) {
        let (event, span) = event.map_err(|error| (error.marker().line(), format!("it is not YAML: {}", error.info())))?;
        let line = span.start.line();
        let finished = match event {
            Event::DocumentStart(_) => {
                documents += 1;
                if documents > 1 {
                    return Err((line, "a file holds one document, and here a second begins".to_string()));
                }
                continue;
            }
            Event::Alias(_) => {
                return Err((line, "it refers to something written elsewhere (*), which these files don't take".to_string()))
            }
            Event::Scalar(_, _, anchor, _) | Event::SequenceStart(anchor, _) | Event::MappingStart(anchor, _) if anchor != 0 => {
                return Err((line, "it gives something a name to refer to (&), which these files don't take".to_string()));
            }
            Event::Scalar(_, _, _, Some(_)) | Event::SequenceStart(_, Some(_)) | Event::MappingStart(_, Some(_)) => {
                return Err((line, "it says of what kind a value is (!), which these files don't take".to_string()));
            }
            Event::Scalar(text, style, _, None) => {
                // a value that is left empty, or written as none, is an empty text
                let none = style == saphyr_parser::ScalarStyle::Plain && matches!(&*text, "~" | "null" | "Null" | "NULL");
                Node { value: Value::Text(if none { String::new() } else { text.into_owned() }), line }
            }
            Event::SequenceStart(..) | Event::MappingStart(..) => {
                if open.len() >= DEEPEST {
                    return Err((line, "its lists are deeper inside each other than these files go".to_string()));
                }
                open.push(match event {
                    Event::SequenceStart(..) => Open::List(Vec::new(), line),
                    _ => Open::Thing(Vec::new(), None, line),
                });
                continue;
            }
            Event::SequenceEnd | Event::MappingEnd => match open.pop() {
                Some(Open::List(holds, line)) => Node { value: Value::List(holds), line },
                Some(Open::Thing(holds, _, line)) => Node { value: Value::Thing(holds), line },
                None => return Err((line, "it is not YAML: something ends that never began".to_string())),
            },
            _ => continue,
        };
        values += 1;
        if values > MOST {
            return Err((finished.line, "it holds more than a file of the register may".to_string()));
        }
        match open.last_mut() {
            None => whole = Some(finished),
            Some(Open::List(holds, _)) => holds.push(finished),
            Some(Open::Thing(holds, name @ None, _)) => {
                let Value::Text(written) = &finished.value else {
                    return Err((finished.line, "a part's name is a text, and here it is a list or a thing".to_string()));
                };
                if holds.iter().any(|(name, _)| matches!(&name.value, Value::Text(other) if other == written)) {
                    return Err((finished.line, format!("“{written}” is written twice")));
                }
                *name = Some(finished);
            }
            Some(Open::Thing(holds, name, _)) => holds.push((name.take().expect("a name before its value"), finished)),
        }
    }
    Ok(whole)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(node: &Node) -> &str {
        match &node.value {
            Value::Text(text) => text,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_file_is_read_as_texts_lists_and_things_with_their_lines() {
        let file =
            "# the finance machines\nmachines:\n  - name: fin-app-02\n    port: 3389\n    groups: [finance, 'audit']\n    desktop: true\n";
        let whole = read(file).unwrap().unwrap();
        let Value::Thing(parts) = &whole.value else { panic!() };
        assert_eq!((text(&parts[0].0), parts[0].0.line), ("machines", 2));
        let Value::List(machines) = &parts[0].1.value else { panic!() };
        let Value::Thing(machine) = &machines[0].value else { panic!() };
        assert_eq!(machines[0].line, 3);
        let named: Vec<(&str, usize)> = machine.iter().map(|(name, _)| (text(name), name.line)).collect();
        assert_eq!(named, [("name", 3), ("port", 4), ("groups", 5), ("desktop", 6)]);
        // a number and a yes are texts, until what holds them reads them
        assert_eq!((text(&machine[1].1), text(&machine[3].1)), ("3389", "true"));
        let Value::List(groups) = &machine[2].1.value else { panic!() };
        assert_eq!(groups.iter().map(text).collect::<Vec<_>>(), ["finance", "audit"]);
    }

    #[test]
    fn a_file_of_nothing_holds_nothing_and_a_value_left_empty_is_an_empty_text() {
        assert_eq!(read(""), Ok(None));
        assert_eq!(read("# nothing yet\n"), Ok(None));
        let whole = read("name:\naddress: ~\nkept: 'null'\n").unwrap().unwrap();
        let Value::Thing(parts) = &whole.value else { panic!() };
        assert_eq!(parts.iter().map(|(_, value)| text(value)).collect::<Vec<_>>(), ["", "", "null"]);
    }

    #[test]
    fn what_is_not_yaml_is_refused_with_its_line() {
        let (line, why) = read("machines:\n  - name: a\n   address: b\n").unwrap_err();
        assert_eq!(line, 3);
        assert!(why.starts_with("it is not YAML: "), "{why}");
        assert!(read("a: [1, 2\nb: 3\n").is_err());
    }

    #[test]
    fn what_these_files_dont_take_is_refused_in_words() {
        assert_eq!(read("a: 1\na: 2\n"), Err((2, "“a” is written twice".to_string())));
        assert_eq!(read("a: &x 1\n").unwrap_err().1, "it gives something a name to refer to (&), which these files don't take");
        assert_eq!(read("a: 1\nb: *x\n").unwrap_err().0, 2);
        assert_eq!(read("a: !!int 1\n").unwrap_err().1, "it says of what kind a value is (!), which these files don't take");
        assert_eq!(read("a: 1\n---\nb: 2\n"), Err((2, "a file holds one document, and here a second begins".to_string())));
        assert_eq!(read("[a]: 1\n").unwrap_err().1, "a part's name is a text, and here it is a list or a thing");
        let deep = format!("{}1{}", "[".repeat(40), "]".repeat(40));
        assert_eq!(read(&deep).unwrap_err().1, "its lists are deeper inside each other than these files go");
    }
}
