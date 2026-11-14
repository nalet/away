//! XML, as far as a farm's feed is written in it: elements with their attributes, inside
//! each other. Text between the elements is left out, since the feed holds none that
//! matters. What a feed has no reason to hold is refused: a grammar of its own, and
//! elements deeper or more numerous than any feed's.

/// How deep elements may be inside each other, and how many there may be.
const DEEPEST: usize = 32;
const MOST: usize = 100_000;

/// One element: its name without what stands before a colon, its attributes, and the
/// elements inside it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Element {
    /// Its name.
    pub name: String,
    /// Its attributes, in the order they are written.
    pub attributes: Vec<(String, String)>,
    /// The elements inside it.
    pub inside: Vec<Element>,
}

impl Element {
    /// What an attribute holds.
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes.iter().find(|(its, _)| its == name).map(|(_, value)| value.as_str())
    }

    /// Every element of this name inside it, however deep, in the order they are written.
    pub fn all<'a>(&'a self, name: &str) -> Vec<&'a Element> {
        let mut found = Vec::new();
        let mut look: Vec<&Element> = self.inside.iter().rev().collect();
        while let Some(element) = look.pop() {
            if element.name == name {
                found.push(element);
            }
            look.extend(element.inside.iter().rev());
        }
        found
    }
}

/// What a name written as `&name;` stands for.
fn named(name: &str) -> Option<char> {
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        _ => {
            let number = name.strip_prefix('#')?;
            let code = match number.strip_prefix('x') {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => number.parse().ok()?,
            };
            char::from_u32(code)?
        }
    })
}

/// An attribute's value, with what is written as `&…;` put back.
fn value(written: &str) -> Result<String, String> {
    let mut value = String::with_capacity(written.len());
    let mut rest = written;
    while let Some(at) = rest.find('&') {
        value.push_str(&rest[..at]);
        let end = rest[at..].find(';').ok_or("a value has an & that nothing ends")?;
        value.push(named(&rest[at + 1..at + end]).ok_or("a value has a character written in a way XML doesn't know")?);
        rest = &rest[at + end + 1..];
    }
    value.push_str(rest);
    Ok(value)
}

/// A name without what stands before its colon.
fn local(name: &str) -> String {
    name.rsplit(':').next().unwrap_or(name).to_string()
}

/// Reads a document. Gives its outermost element.
pub fn read(text: &str) -> Result<Element, String> {
    let mut rest = text.trim_start_matches('\u{feff}');
    let mut open: Vec<Element> = Vec::new();
    let mut whole: Option<Element> = None;
    let mut count = 0usize;
    while let Some(at) = rest.find('<') {
        rest = &rest[at..];
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = &after[after.find("-->").ok_or("a comment is not ended")? + 3..];
        } else if let Some(after) = rest.strip_prefix("<![CDATA[") {
            rest = &after[after.find("]]>").ok_or("a piece of text is not ended")? + 3..];
        } else if let Some(after) = rest.strip_prefix("<?") {
            rest = &after[after.find("?>").ok_or("an instruction is not ended")? + 2..];
        } else if rest.starts_with("<!") {
            return Err("it declares a grammar of its own, which a feed has no reason to".to_string());
        } else if let Some(after) = rest.strip_prefix("</") {
            let end = after.find('>').ok_or("an element's end is not ended")?;
            let name = local(after[..end].trim());
            let element = open.pop().filter(|element| element.name == name).ok_or(format!("{name} ends where it never began"))?;
            match open.last_mut() {
                Some(around) => around.inside.push(element),
                None if whole.is_none() => whole = Some(element),
                None => return Err("it holds more than one outermost element".to_string()),
            }
            rest = &after[end + 1..];
        } else {
            // an element begins: its name, its attributes, and whether it ends at once
            let mut after = &rest[1..];
            let name_end = after.find(|c: char| c.is_whitespace() || c == '>' || c == '/').ok_or("an element is not ended")?;
            if name_end == 0 {
                return Err("an element has no name".to_string());
            }
            let mut element = Element { name: local(&after[..name_end]), ..Element::default() };
            after = &after[name_end..];
            let ends_at_once = loop {
                after = after.trim_start();
                if let Some(ended) = after.strip_prefix("/>") {
                    after = ended;
                    break true;
                }
                if let Some(ended) = after.strip_prefix('>') {
                    after = ended;
                    break false;
                }
                let equals = after.find('=').ok_or("an attribute has no value")?;
                let name = after[..equals].trim();
                if name.is_empty() || name.contains(['<', '>', '/', '"', '\'']) {
                    return Err("an attribute is not written as one".to_string());
                }
                let written = after[equals + 1..].trim_start();
                let quote =
                    written.chars().next().filter(|quote| matches!(quote, '"' | '\'')).ok_or("an attribute's value is not in quotes")?;
                let end = written[1..].find(quote).ok_or("an attribute's value is not ended")?;
                // what declares where names come from is left out: the names are read without it
                if name != "xmlns" && !name.starts_with("xmlns:") {
                    element.attributes.push((local(name), value(&written[1..1 + end])?));
                }
                after = &written[end + 2..];
            };
            count += 1;
            if count > MOST || open.len() >= DEEPEST {
                return Err("it holds more elements, or deeper ones, than a feed does".to_string());
            }
            if ends_at_once {
                match open.last_mut() {
                    Some(around) => around.inside.push(element),
                    None if whole.is_none() => whole = Some(element),
                    None => return Err("it holds more than one outermost element".to_string()),
                }
            } else {
                open.push(element);
            }
            rest = after;
        }
    }
    match (open.pop(), whole) {
        (Some(left), _) => Err(format!("{} is not ended", left.name)),
        (None, Some(whole)) => Ok(whole),
        (None, None) => Err("it holds no element".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elements_are_read_with_their_attributes_and_what_is_inside_them() {
        let document = "\u{feff}<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!-- a feed -->\n<a:Outer One=\"1\" xmlns=\"http://example.com/ns\" xmlns:a=\"x\">\n  text that is left out\n  <Inner\n      Name='a &amp; b &lt;c&gt; &#65;&#x42;' />\n  <Inner Name=\"second\"><Deep/></Inner>\n  <![CDATA[ <NotAnElement/> ]]>\n</a:Outer>\n";
        let outer = read(document).unwrap();
        assert_eq!((outer.name.as_str(), outer.attribute("One"), outer.attributes.len()), ("Outer", Some("1"), 1));
        let inner = outer.all("Inner");
        assert_eq!(inner.iter().map(|element| element.attribute("Name")).collect::<Vec<_>>(), [Some("a & b <c> AB"), Some("second")]);
        assert_eq!(outer.all("Deep").len(), 1);
        assert_eq!(outer.all("NotAnElement").len(), 0);
        assert_eq!(outer.inside.len(), 2);
    }

    #[test]
    fn what_is_not_xml_or_what_a_feed_has_no_reason_to_hold_is_refused() {
        for (document, why) in [
            ("", "it holds no element"),
            ("no element here", "it holds no element"),
            ("<a><b></a>", "a ends where it never began"),
            ("<a>", "a is not ended"),
            ("<a/><b/>", "it holds more than one outermost element"),
            ("<!DOCTYPE a [<!ENTITY x \"y\">]><a/>", "it declares a grammar of its own, which a feed has no reason to"),
            ("<a b=c/>", "an attribute's value is not in quotes"),
            ("<a b=\"c/>", "an attribute's value is not ended"),
            ("<a b=\"&nothing;\"/>", "a value has a character written in a way XML doesn't know"),
            ("<a b=\"&amp\"/>", "a value has an & that nothing ends"),
            ("<a b/>", "an attribute has no value"),
            ("<!-- never ended", "a comment is not ended"),
            ("< a/>", "an element has no name"),
        ] {
            assert_eq!(read(document), Err(why.to_string()), "{document}");
        }
        let deep = format!("{}{}", "<a>".repeat(40), "</a>".repeat(40));
        assert_eq!(read(&deep), Err("it holds more elements, or deeper ones, than a feed does".to_string()));
    }
}
