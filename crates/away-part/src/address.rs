//! The address of a service that Away asks, such as the sign-in service's, and the way
//! texts are written inside an address and inside a form.

/// Where a service is, read from `https://sign-in.example.com/realms/staff`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Address {
    /// Whether the connection to it is secured. One that is not is taken only where
    /// the service is on this machine itself, which tests are.
    pub secured: bool,
    /// The name of its host.
    pub host: String,
    /// Its port.
    pub port: u16,
    /// What follows the host: the path, and what is asked after a question mark.
    pub path: String,
}

impl Address {
    /// Reads an address as it is written.
    pub fn read(written: &str) -> Result<Address, &'static str> {
        let (secured, rest) = match (written.strip_prefix("https://"), written.strip_prefix("http://")) {
            (Some(rest), _) => (true, rest),
            (_, Some(rest)) => (false, rest),
            _ => return Err("an address begins with https://"),
        };
        if rest.chars().any(|character| character.is_whitespace() || character.is_control() || character == '#') {
            return Err("an address holds no space and no part after a #");
        }
        let (authority, path) = match rest.find(['/', '?']) {
            Some(at) => rest.split_at(at),
            None => (rest, ""),
        };
        let (host, port) = match authority.rsplit_once(':') {
            Some((host, port)) => {
                (host, port.parse::<u16>().ok().filter(|port| *port != 0).ok_or("an address's port is a number from 1 to 65535")?)
            }
            None => (authority, if secured { 443 } else { 80 }),
        };
        let plain = |host: &str| {
            !host.is_empty() && host.chars().all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-'))
        };
        if !plain(host) {
            return Err("an address names its host with letters, digits, dots and hyphens");
        }
        if !secured && !matches!(host, "127.0.0.1" | "localhost") {
            return Err("an address that is not on this machine begins with https://");
        }
        let path = match path {
            "" => "/".to_string(),
            asked if asked.starts_with('?') => format!("/{asked}"),
            path => path.to_string(),
        };
        Ok(Address { secured, host: host.to_ascii_lowercase(), port, path })
    }

    /// The address up to its path: `https://sign-in.example.com`, with the port where it
    /// is not the usual one.
    pub fn origin(&self) -> String {
        let scheme = if self.secured { "https" } else { "http" };
        match (self.secured, self.port) {
            (true, 443) | (false, 80) => format!("{scheme}://{}", self.host),
            (_, port) => format!("{scheme}://{}:{port}", self.host),
        }
    }

    /// The address as it is written.
    pub fn written(&self) -> String {
        format!("{}{}", self.origin(), self.path)
    }

    /// The host as a request names it: with the port where it is not the usual one.
    pub fn host_named(&self) -> String {
        self.origin().split_once("://").map(|(_, host)| host.to_string()).unwrap_or_default()
    }

    /// The same address, with something more asked after its question mark.
    pub fn asking(&self, asked: &[(&str, &str)]) -> Address {
        let joins = if self.path.contains('?') { '&' } else { '?' };
        Address { path: format!("{}{joins}{}", self.path, form(asked)), ..self.clone() }
    }
}

/// A text as it is written inside an address or a form: letters, digits and four signs
/// as they are, and every other byte as a percent sign and its number.
pub fn written_inside(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => out.push(byte as char),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Names and values as a form sends them: `name=value&name=value`.
pub fn form(pairs: &[(&str, &str)]) -> String {
    pairs.iter().map(|(name, value)| format!("{}={}", written_inside(name), written_inside(value))).collect::<Vec<_>>().join("&")
}

/// Reads what a form sent, or what an address asks after its question mark. Gives
/// nothing where a percent sign is followed by no number, or the bytes are no text.
pub fn read_form(sent: &str) -> Option<Vec<(String, String)>> {
    let read = |written: &str| -> Option<String> {
        let mut bytes = Vec::with_capacity(written.len());
        let mut rest = written.as_bytes();
        while let [first, after @ ..] = rest {
            match first {
                b'%' => {
                    let number = std::str::from_utf8(after.get(..2)?).ok()?;
                    if !number.bytes().all(|digit| digit.is_ascii_hexdigit()) {
                        return None;
                    }
                    bytes.push(u8::from_str_radix(number, 16).ok()?);
                    rest = &after[2..];
                    continue;
                }
                b'+' => bytes.push(b' '),
                other => bytes.push(*other),
            }
            rest = after;
        }
        String::from_utf8(bytes).ok()
    };
    sent.split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            Some((read(name)?, read(value)?))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_is_read_into_its_parts() {
        let address = Address::read("https://Sign-In.example.com/realms/staff?x=1").unwrap();
        assert_eq!(address, Address { secured: true, host: "sign-in.example.com".into(), port: 443, path: "/realms/staff?x=1".into() });
        assert_eq!(address.origin(), "https://sign-in.example.com");
        assert_eq!(address.written(), "https://sign-in.example.com/realms/staff?x=1");
        assert_eq!(address.host_named(), "sign-in.example.com");
        let other = Address::read("https://sign-in.example.com:8443").unwrap();
        assert_eq!((other.port, other.path.as_str()), (8443, "/"));
        assert_eq!(other.origin(), "https://sign-in.example.com:8443");
        assert_eq!(other.host_named(), "sign-in.example.com:8443");
        assert_eq!(Address::read("https://sign-in.example.com?x=1").unwrap().path, "/?x=1");
    }

    #[test]
    fn an_address_that_is_not_secured_is_taken_on_this_machine_alone() {
        assert_eq!(Address::read("http://127.0.0.1:8080/x").unwrap().origin(), "http://127.0.0.1:8080");
        assert_eq!(Address::read("http://localhost/x").unwrap().port, 80);
        assert_eq!(Address::read("http://sign-in.example.com/"), Err("an address that is not on this machine begins with https://"));
        assert_eq!(Address::read("http://127.0.0.1.example.com/"), Err("an address that is not on this machine begins with https://"));
    }

    #[test]
    fn what_is_no_address_is_refused() {
        for wrong in [
            "",
            "sign-in.example.com",
            "ftp://sign-in.example.com",
            "https://",
            "https:///path",
            "https://user@sign-in.example.com/",
            "https://user:pw@sign-in.example.com/",
            "https://sign-in.example.com:0/",
            "https://sign-in.example.com:99999/",
            "https://sign-in.example.com:port/",
            "https://sign in.example.com/",
            "https://sign-in.example.com/a b",
            "https://sign-in.example.com/a#b",
            "https://sign-in.example.com/\r\nx: y",
            "https://[::1]/",
            "https://exämple.com/",
        ] {
            assert!(Address::read(wrong).is_err(), "{wrong:?}");
        }
    }

    #[test]
    fn something_more_is_asked_of_an_address() {
        let address = Address::read("https://sign-in.example.com/authorize").unwrap();
        let asked = address.asking(&[("state", "a b&c"), ("to", "https://away.example.com/back")]);
        assert_eq!(asked.path, "/authorize?state=a%20b%26c&to=https%3A%2F%2Faway.example.com%2Fback");
        assert_eq!(asked.asking(&[("more", "1")]).path, format!("{}&more=1", asked.path));
    }

    #[test]
    fn a_form_is_written_and_read() {
        let written = form(&[("name", "Zoë O'Neil"), ("sign", "a+b=c&d"), ("empty", "")]);
        assert_eq!(written, "name=Zo%C3%AB%20O%27Neil&sign=a%2Bb%3Dc%26d&empty=");
        assert_eq!(
            read_form(&written).unwrap(),
            [
                ("name".to_string(), "Zoë O'Neil".to_string()),
                ("sign".to_string(), "a+b=c&d".to_string()),
                ("empty".to_string(), String::new())
            ]
        );
        // a plus is a space, a name may stand alone, and nothing between two signs is nothing
        assert_eq!(read_form("a=b+c&&flag").unwrap(), [("a".to_string(), "b c".to_string()), ("flag".to_string(), String::new())]);
        assert_eq!(read_form("").unwrap(), []);
        for wrong in ["a=%", "a=%2", "a=%zz", "a=%+1", "a=%FF", "%C3=1"] {
            assert_eq!(read_form(wrong), None, "{wrong}");
        }
    }
}
