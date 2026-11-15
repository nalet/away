//! The register: the machines, their groups and the rules, as the operator's files say
//! them ([ADR-0009](../../../spec/architecture/decisions/0009-one-database-and-files.md)).
//!
//! The files are YAML. Each holds machines, rules, or both:
//!
//! ```yaml
//! machines:
//!   - name: fin-app-02
//!     kind: windows
//!     address: fin-app-02.corp.example
//!     groups: [finance]
//!     desktop: true
//!     programs:
//!       - Ledger
//!       - name: Payroll
//!         icon: icons/payroll.png
//!   - name: build-01
//!     kind: linux
//!     address: build-01.corp.example
//!     groups: [engineering]
//!     shell: true
//! rules:
//!   - name: finance-desktops
//!     who: [finance]
//!     reaches: [finance]
//! farms:
//!   - name: Work Resources
//!     feed: https://broker.corp.example/RDWeb/Feed/webfeed.aspx
//!     groups: [finance]
//! ```
//!
//! A farm is a set of Windows machines that publishes desktops and programs through a
//! broker. Its entry names the farm's own feed, and Away asks the feed what there is.
//!
//! A set of files is taken whole or not at all: one thing wrong anywhere, and the set is
//! refused with the file, the line and the reason, while what was in force stays.

pub mod files;
pub mod yaml;

use std::collections::BTreeMap;
use std::sync::Arc;

use away_part::address::Address;
use away_part::json::Json;
use sha2::{Digest, Sha256};

use crate::register::yaml::{Node, Value};

/// What a machine is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A Windows machine.
    Windows,
    /// A Linux machine.
    Linux,
    /// A Mac.
    Mac,
}

impl Kind {
    fn written(self) -> &'static str {
        match self {
            Kind::Windows => "windows",
            Kind::Linux => "linux",
            Kind::Mac => "mac",
        }
    }
}

/// What a desktop is reached over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Over {
    /// RDP: a Windows machine's own, and that of desktops on Linux that speak it.
    Rdp,
    /// VNC: a Mac's own, and that of most desktops on Linux.
    Vnc,
}

impl Over {
    fn written(self) -> &'static str {
        match self {
            Over::Rdp => "rdp",
            Over::Vnc => "vnc",
        }
    }
}

/// A program of a Windows machine that opens in windows of its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    /// The name people know it by.
    pub name: String,
    /// Its icon, where the files give it one: the name the picture is asked for by,
    /// which is made from the picture itself.
    pub icon: Option<String>,
    /// What the machine is asked to start: the name the program is published under, with
    /// `||` before it, or the path of a program on the machine.
    pub run: String,
}

/// The largest a picture of the files may be, in bytes.
pub const LARGEST_PICTURE: usize = 256 * 1024;

/// What a PNG picture begins with.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Which ways the clipboard passes between a person's device and a machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clipboard {
    /// What the person copied can be pasted on the machine.
    pub to_the_machine: bool,
    /// What is copied on the machine reaches the person's device.
    pub from_the_machine: bool,
}

impl Clipboard {
    /// Both ways, as it passes unless an entry says otherwise.
    pub const BOTH: Clipboard = Clipboard { to_the_machine: true, from_the_machine: true };

    /// The word an entry says it with.
    pub fn word(self) -> &'static str {
        match (self.to_the_machine, self.from_the_machine) {
            (true, true) => "both",
            (true, false) => "to-the-machine",
            (false, true) => "from-the-machine",
            (false, false) => "none",
        }
    }
}

/// How a person signs in on a Windows machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignsIn {
    /// Away takes a name and a password from the browser.
    Typed,
    /// Away asks the issuer for a sign-in certificate for the person.
    Themselves,
    /// The person's Windows client opens the desktop in Restricted Admin mode.
    RestrictedAdmin,
    /// Away opens a direct desktop using its explicitly enabled Kerberos delegation policy.
    RestrictedAdminSso,
}

/// One machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Machine {
    /// The name people know it by. No two machines share one.
    pub name: String,
    /// What it is.
    pub kind: Kind,
    /// Where it is reached: a name or a number.
    pub address: String,
    /// The groups it is in, in the order of their names.
    pub groups: Vec<String>,
    /// Its desktop, where it offers one: what it is reached over, and on which port.
    pub desktop: Option<(Over, u16)>,
    /// The programs of a Windows machine that open in windows of their own.
    pub programs: Vec<Program>,
    /// Its shell, where it offers one: the port.
    pub shell: Option<u16>,
    /// The collection of a farm of Windows machines that this entry stands for.
    pub collection: Option<String>,
    /// The machines that serve the collection.
    pub served_by: Vec<String>,
    /// How people sign in on the machine.
    pub signs_in: SignsIn,
    /// Which ways the clipboard passes.
    pub clipboard: Clipboard,
    /// The farm that publishes it, where it is not written in a file itself but read
    /// from a farm's feed.
    pub farm: Option<String>,
    /// The file it is written in, and the line. For what a farm publishes: the farm's.
    pub written_in: (String, usize),
}

/// A farm of Windows machines: what it publishes is read from its own feed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Farm {
    /// The name administrators know it by, and people know its programs' machine by.
    pub name: String,
    /// Where its feed is.
    pub feed: String,
    /// The groups of machines that what it publishes is in.
    pub groups: Vec<String>,
    /// The machines that serve what it publishes, by their addresses: Away goes on to
    /// no other machine for it, whatever the farm's broker says.
    pub served_by: Vec<String>,
    /// Whether people are signed in as themselves on what it publishes, with nothing typed.
    pub themselves: bool,
    /// Which ways the clipboard passes on what it publishes.
    pub clipboard: Clipboard,
    /// How many machines of the files are written before it, in the files' order: what
    /// it publishes shows after them, where the farm itself is written.
    pub after: usize,
    /// The file it is written in, and the line.
    pub written_in: (String, usize),
}

/// The name a picture is asked for by: made from the picture itself, so that a browser
/// may keep it for as long as it likes.
pub fn picture_name(picture: &[u8]) -> String {
    Sha256::digest(picture).iter().take(16).map(|byte| format!("{byte:02x}")).collect()
}

/// Whether bytes are a PNG picture of a size the register takes.
pub fn is_a_picture(bytes: &[u8]) -> bool {
    bytes.starts_with(PNG) && bytes.len() <= LARGEST_PICTURE
}

/// One rule: groups of people, and what they reach.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    /// Its name. No two rules share one.
    pub name: String,
    /// The groups of people it is for, as the sign-in service names them.
    pub who: Vec<String>,
    /// What they reach: machines and groups of machines, by name.
    pub reaches: Vec<String>,
    /// The file it is written in, and the line.
    pub written_in: (String, usize),
}

/// Why a set of files is refused: the file, the line, and what is wrong there. The line
/// is 0 where it is the file as a whole, and the file empty where it is the set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused {
    /// The file, by its name inside the folder of files.
    pub file: String,
    /// The line, counted from 1.
    pub line: usize,
    /// What is wrong, as the rest of a sentence: `“prot” is not something a machine has`.
    pub why: String,
}

impl std::fmt::Display for Refused {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.file.as_str(), self.line) {
            ("", _) => write!(out, "{}", self.why),
            (file, 0) => write!(out, "{file}: {}", self.why),
            (file, line) => write!(out, "{file}, line {line}: {}", self.why),
        }
    }
}

/// What is in force: every machine and every rule, in the order the files have them. The
/// files come in the order of their names, and inside a file things keep their place.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InForce {
    /// The machines.
    pub machines: Vec<Machine>,
    /// The rules.
    pub rules: Vec<Rule>,
    /// The pictures the machines' programs name as their icons, each by the name it is
    /// asked for by.
    pub pictures: BTreeMap<String, Arc<Vec<u8>>>,
    /// The farms whose feeds say what they publish.
    pub farms: Vec<Farm>,
}

/// The longest a name, an address or a group may be.
const LONGEST: usize = 120;

fn refused(file: &str, line: usize, why: impl Into<String>) -> Refused {
    Refused { file: file.to_string(), line, why: why.into() }
}

/// The same name, however its letters are written.
pub(crate) fn same(one: &str, other: &str) -> bool {
    one.to_lowercase() == other.to_lowercase()
}

/// Reads what a file holds under names, and refuses a name that is not among those it
/// may hold.
struct Parts<'a> {
    file: &'a str,
    parts: &'a [(Node, Node)],
}

impl<'a> Parts<'a> {
    fn of(file: &'a str, node: &'a Node, may_hold: &[&str], what_it_is: &str, shape: &str) -> Result<Parts<'a>, Refused> {
        let Value::Thing(parts) = &node.value else { return Err(refused(file, node.line, shape)) };
        for (name, _) in parts {
            let Value::Text(written) = &name.value else { continue };
            if !may_hold.contains(&written.as_str()) {
                return Err(refused(file, name.line, format!("“{written}” is not something {what_it_is} has")));
            }
        }
        Ok(Parts { file, parts })
    }

    fn get(&self, name: &str) -> Option<&'a Node> {
        self.parts.iter().find(|(its, _)| matches!(&its.value, Value::Text(written) if written == name)).map(|(_, value)| value)
    }

    /// A text: one line of it, with something in it, and no longer than a name gets.
    fn text(&self, name: &str, of: &str) -> Result<Option<(String, usize)>, Refused> {
        let Some(node) = self.get(name) else { return Ok(None) };
        let Value::Text(text) = &node.value else {
            return Err(refused(self.file, node.line, format!("the {name} of {of} is a text, and here it is a list or a thing")));
        };
        let text = text.trim();
        match text {
            "" => Err(refused(self.file, node.line, format!("the {name} of {of} is empty"))),
            _ if text.chars().count() > LONGEST || text.chars().any(char::is_control) => Err(refused(
                self.file,
                node.line,
                format!("the {name} of {of} is longer than {LONGEST} characters, or holds what no text does"),
            )),
            _ => Ok(Some((text.to_string(), node.line))),
        }
    }

    /// Texts: a list of them, or one alone.
    fn texts(&self, name: &str, of: &str) -> Result<Vec<(String, usize)>, Refused> {
        let Some(node) = self.get(name) else { return Ok(Vec::new()) };
        let one = |node: &Node| -> Result<(String, usize), Refused> {
            match &node.value {
                Value::Text(text) if !text.trim().is_empty() && text.chars().count() <= LONGEST && !text.chars().any(char::is_control) => {
                    Ok((text.trim().to_string(), node.line))
                }
                Value::Text(_) => {
                    Err(refused(self.file, node.line, format!("one of the {name} of {of} is empty, or longer than {LONGEST} characters")))
                }
                _ => Err(refused(self.file, node.line, format!("the {name} of {of} are texts, and here one is a list or a thing"))),
            }
        };
        let mut texts: Vec<(String, usize)> = Vec::new();
        match &node.value {
            Value::List(listed) => {
                for node in listed {
                    let (text, line) = one(node)?;
                    if texts.iter().any(|(other, _)| same(other, &text)) {
                        return Err(refused(self.file, line, format!("“{text}” is twice among the {name} of {of}")));
                    }
                    texts.push((text, line));
                }
            }
            _ => texts.push(one(node)?),
        }
        Ok(texts)
    }

    /// A way in: left out, said with yes or no, or with a port and what it is reached over.
    fn way(&self, name: &str, of: &str, usual_port: u16, may_be_over: bool) -> Result<Option<(Option<Over>, u16)>, Refused> {
        let Some(node) = self.get(name) else { return Ok(None) };
        match &node.value {
            Value::Text(said) => match said.as_str() {
                "true" | "yes" => Ok(Some((None, usual_port))),
                "false" | "no" => Ok(None),
                _ => Err(refused(self.file, node.line, format!("the {name} of {of} is said with true or false, or with its port"))),
            },
            Value::List(_) => {
                Err(refused(self.file, node.line, format!("the {name} of {of} is said with true or false, or with its port")))
            }
            Value::Thing(_) => {
                let may_hold: &[&str] = if may_be_over { &["over", "port"] } else { &["port"] };
                let parts = Parts::of(self.file, node, may_hold, &format!("a {name}"), "")?;
                let port = match parts.text("port", &format!("the {name} of {of}"))? {
                    None => None,
                    Some((written, line)) => match written.parse::<u16>() {
                        Ok(port) if port != 0 => Some(port),
                        _ => return Err(refused(self.file, line, format!("“{written}” is no port: a port is a number from 1 to 65535"))),
                    },
                };
                let over = match parts.text("over", &format!("the {name} of {of}"))? {
                    None => None,
                    Some((written, line)) => Some(match written.as_str() {
                        "rdp" => Over::Rdp,
                        "vnc" => Over::Vnc,
                        _ => return Err(refused(self.file, line, format!("“{written}” is nothing a desktop is reached over: rdp or vnc"))),
                    }),
                };
                let usual = match over {
                    Some(Over::Rdp) => 3389,
                    Some(Over::Vnc) => 5900,
                    None => usual_port,
                };
                Ok(Some((over, port.unwrap_or(usual))))
            }
        }
    }
}

impl InForce {
    /// Reads a set of files: each with its name inside the folder, and what it holds.
    /// Gives what they say, or the first thing that is wrong with them.
    pub fn read(files: &[(String, String)]) -> Result<InForce, Refused> {
        InForce::read_with_pictures(files, &[])
    }

    /// Reads a set of files, and the pictures of the folder that the files may name as
    /// icons: each picture with its name inside the folder, and its bytes.
    pub fn read_with_pictures(files: &[(String, String)], pictures: &[(String, Vec<u8>)]) -> Result<InForce, Refused> {
        let mut machines: Vec<Machine> = Vec::new();
        let mut rules: Vec<Rule> = Vec::new();
        let mut named: BTreeMap<String, Arc<Vec<u8>>> = BTreeMap::new();
        let mut farms: Vec<Farm> = Vec::new();
        for (file, text) in files {
            let Some(whole) = yaml::read(text).map_err(|(line, why)| refused(file, line, why))? else { continue };
            let holds = Parts::of(
                file,
                &whole,
                &["machines", "rules", "farms"],
                "a file",
                "a file holds machines, rules and farms, under “machines:”, “rules:” and “farms:”",
            )?;
            for (name, read) in [("machines", 0), ("rules", 1), ("farms", 2)] {
                let Some(node) = holds.get(name) else { continue };
                let listed = match &node.value {
                    Value::List(listed) => listed.as_slice(),
                    // a name with nothing under it holds none
                    Value::Text(nothing) if nothing.is_empty() => &[],
                    _ => return Err(refused(file, node.line, format!("the {name} are a list, each beginning with “- name:”"))),
                };
                for node in listed {
                    if read == 0 {
                        let machine = machine(file, node, pictures, &mut named)?;
                        if let Some(other) = machines.iter().find(|other| same(&other.name, &machine.name)) {
                            let (in_file, line) = &other.written_in;
                            return Err(refused(
                                file,
                                node.line,
                                format!("the machine {} is also written in {in_file}, line {line}", machine.name),
                            ));
                        }
                        machines.push(machine);
                    } else if read == 2 {
                        let farm = farm(file, node)?;
                        if let Some(other) = farms.iter().find(|other| same(&other.name, &farm.name)) {
                            let (in_file, line) = &other.written_in;
                            return Err(refused(
                                file,
                                node.line,
                                format!("the farm {} is also written in {in_file}, line {line}", farm.name),
                            ));
                        }
                        farms.push(farm);
                    } else {
                        let rule = rule(file, node)?;
                        if let Some(other) = rules.iter().find(|other| same(&other.name, &rule.name)) {
                            let (in_file, line) = &other.written_in;
                            return Err(refused(
                                file,
                                node.line,
                                format!("the rule {} is also written in {in_file}, line {line}", rule.name),
                            ));
                        }
                        rules.push(rule);
                    }
                }
            }
        }
        // what a farm publishes shows where the farm is written: after the machines before it
        let place = |written_in: &(String, usize)| (files.iter().position(|(name, _)| *name == written_in.0), written_in.1);
        for farm in &mut farms {
            let here = place(&farm.written_in);
            farm.after = machines.iter().filter(|machine| place(&machine.written_in) < here).count();
        }
        // a farm's programs are known as a machine of the farm's name: no machine has it too
        for farm in &farms {
            if let Some(machine) = machines.iter().find(|machine| same(&machine.name, &farm.name)) {
                let (file, line) = &farm.written_in;
                let (in_file, at) = &machine.written_in;
                return Err(refused(
                    file,
                    *line,
                    format!("the farm {} has the name of a machine, which is written in {in_file}, line {at}", farm.name),
                ));
            }
        }
        // what a rule reaches is a machine, a farm or a group of machines, and it is clear which
        for rule in &rules {
            let (file, line) = &rule.written_in;
            for reached in &rule.reaches {
                let is_machine = machines.iter().any(|machine| machine.name == *reached) || farms.iter().any(|farm| farm.name == *reached);
                let is_group = machines.iter().any(|machine| machine.groups.contains(reached))
                    || farms.iter().any(|farm| farm.groups.contains(reached));
                match (is_machine, is_group) {
                    (true, true) => {
                        return Err(refused(
                            file,
                            *line,
                            format!(
                                "the rule {} reaches “{reached}”, which is the name of a machine and of a group of machines",
                                rule.name
                            ),
                        ))
                    }
                    (false, false) => {
                        return Err(refused(
                            file,
                            *line,
                            format!("the rule {} reaches “{reached}”, which is neither a machine nor a group of machines", rule.name),
                        ))
                    }
                    _ => {}
                }
            }
        }
        Ok(InForce { machines, rules, pictures: named, farms })
    }

    /// The machines that someone of these groups reaches, each with the rules that give
    /// it to them, in the order the files have the machines.
    pub fn reached_by(&self, groups: &[String]) -> Vec<(&Machine, Vec<&str>)> {
        let mut reached: BTreeMap<usize, Vec<&str>> = BTreeMap::new();
        for rule in self.rules.iter().filter(|rule| rule.who.iter().any(|who| groups.contains(who))) {
            for (nth, machine) in self.machines.iter().enumerate() {
                if rule.reaches.iter().any(|reached| machine.is_reached_as(reached)) {
                    reached.entry(nth).or_default().push(&rule.name);
                }
            }
        }
        reached.into_iter().map(|(nth, rules)| (&self.machines[nth], rules)).collect()
    }
}

/// Reads the programs of a machine: each a name alone, or a name and an icon. An icon is
/// a PNG picture of the folder, named by where it is in the folder.
fn programs(
    file: &str,
    node: Option<&Node>,
    of: &str,
    pictures: &[(String, Vec<u8>)],
    named: &mut BTreeMap<String, Arc<Vec<u8>>>,
) -> Result<Vec<Program>, Refused> {
    let Some(node) = node else { return Ok(Vec::new()) };
    let listed = match &node.value {
        Value::List(listed) => listed.as_slice(),
        Value::Text(nothing) if nothing.is_empty() => &[],
        _ => return Err(refused(file, node.line, format!("the programs of {of} are a list: of names, or of names with their icons"))),
    };
    let mut programs: Vec<Program> = Vec::new();
    for node in listed {
        let (name, icon, run) = match &node.value {
            Value::Thing(_) => {
                let parts = Parts::of(file, node, &["name", "icon", "run"], "a program", "")?;
                let Some((name, _)) = parts.text("name", &format!("a program of {of}"))? else {
                    return Err(refused(file, node.line, format!("a program of {of} has no name")));
                };
                let icon = parts.text("icon", &format!("the program {name}"))?;
                let run = parts.text("run", &format!("the program {name}"))?.map(|(run, _)| run);
                (name, icon, run)
            }
            Value::Text(text) if !text.trim().is_empty() && text.chars().count() <= LONGEST && !text.chars().any(char::is_control) => {
                (text.trim().to_string(), None, None)
            }
            _ => return Err(refused(file, node.line, format!("a program of {of} is a name, or a name with its icon"))),
        };
        if programs.iter().any(|other| same(&other.name, &name)) {
            return Err(refused(file, node.line, format!("“{name}” is twice among the programs of {of}")));
        }
        let icon = match icon {
            None => None,
            Some((path, line)) => {
                let Some((_, picture)) = pictures.iter().find(|(its, _)| *its == path) else {
                    return Err(refused(file, line, format!("the icon of {name} is “{path}”, and the folder holds no picture of that name: a PNG picture, named by where it is in the folder")));
                };
                if !picture.starts_with(PNG) {
                    return Err(refused(file, line, format!("the icon of {name}, “{path}”, is no PNG picture")));
                }
                // a picture is asked for by a name made from the picture itself, so that a
                // browser may keep it for as long as it likes
                let sum = picture_name(picture);
                named.entry(sum.clone()).or_insert_with(|| Arc::new(picture.clone()));
                Some(sum)
            }
        };
        // a program is started as what the files say, or as what its name is published as
        let run = run.unwrap_or_else(|| format!("||{name}"));
        programs.push(Program { name, icon, run });
    }
    Ok(programs)
}

/// Reads one machine.
fn machine(
    file: &str,
    node: &Node,
    pictures: &[(String, Vec<u8>)],
    named: &mut BTreeMap<String, Arc<Vec<u8>>>,
) -> Result<Machine, Refused> {
    let may_hold =
        ["name", "kind", "address", "groups", "desktop", "programs", "shell", "collection", "served-by", "signs-in", "clipboard"];
    let shape = "a machine is written as its name, its kind, its address and its ways in, beginning with “- name:”";
    let parts = Parts::of(file, node, &may_hold, "a machine", shape)?;
    let Some((name, _)) = parts.text("name", "a machine")? else { return Err(refused(file, node.line, "the machine has no name")) };
    // a name is part of an address in Away, and is said and typed by people
    if !name.chars().all(|character| character.is_alphanumeric() || " -_.()+&".contains(character)) {
        return Err(refused(
            file,
            node.line,
            format!(
                "the name “{name}” holds a character that names don't take: letters, digits, spaces and - _ . ( ) + & are what they take"
            ),
        ));
    }
    let of = name.as_str();
    let kind = match parts.text("kind", of)? {
        None => return Err(refused(file, node.line, format!("{name} has no kind: windows, linux or mac"))),
        Some((written, line)) => match written.as_str() {
            "windows" => Kind::Windows,
            "linux" => Kind::Linux,
            "mac" => Kind::Mac,
            _ => return Err(refused(file, line, format!("“{written}” is no kind of machine: windows, linux or mac"))),
        },
    };
    let Some((address, address_line)) = parts.text("address", of)? else {
        return Err(refused(file, node.line, format!("{name} has no address")));
    };
    if !is_an_address(&address) {
        return Err(refused(
            file,
            address_line,
            format!("“{address}” is no address of a machine: a name such as fin-app-02.corp.example, or a number"),
        ));
    }
    let mut groups: Vec<String> = parts.texts("groups", of)?.into_iter().map(|(group, _)| group).collect();
    groups.sort_by_key(|group| group.to_lowercase());

    let (usual, usual_port) = match kind {
        Kind::Windows => (Over::Rdp, 3389),
        Kind::Linux | Kind::Mac => (Over::Vnc, 5900),
    };
    let desktop = parts.way("desktop", of, usual_port, true)?.map(|(over, port)| (over.unwrap_or(usual), port));
    if let (Kind::Windows, Some((Over::Vnc, _))) | (Kind::Mac, Some((Over::Rdp, _))) = (kind, desktop) {
        let line = parts.get("desktop").map_or(node.line, |node| node.line);
        return Err(refused(file, line, format!("the desktop of {name} is reached over what a {} machine doesn't speak", kind.written())));
    }
    let shell = parts.way("shell", of, 22, false)?.map(|(_, port)| port);
    let programs = programs(file, parts.get("programs"), of, pictures, named)?;
    if !programs.is_empty() && kind != Kind::Windows {
        let line = parts.get("programs").map_or(node.line, |node| node.line);
        return Err(refused(file, line, format!("{name} names programs, which only a Windows machine has")));
    }
    if desktop.is_none() && shell.is_none() && programs.is_empty() {
        return Err(refused(file, node.line, format!("{name} offers no way in: a desktop, programs or a shell")));
    }

    let collection = parts.text("collection", of)?;
    let served_by = parts.texts("served-by", of)?;
    if let Some((_, line)) = &collection {
        if kind != Kind::Windows || (desktop.is_none() && programs.is_empty()) {
            return Err(refused(
                file,
                *line,
                format!("{name} names a collection, which is a farm of Windows machines with a desktop or programs"),
            ));
        }
    }
    if let (None, Some((_, line))) = (&collection, served_by.first()) {
        return Err(refused(file, *line, format!("{name} names machines that serve it, and no collection for them to serve")));
    }
    if let Some((other, line)) = served_by.iter().find(|(address, _)| !serves(address)) {
        return Err(refused(file, *line, format!("“{other}” is no address of a machine")));
    }
    let (signs_in, line) = signs_in(&parts, file, of)?.unwrap_or((SignsIn::Typed, node.line));
    match signs_in {
        SignsIn::Themselves => {
            let over_rdp = matches!(desktop, Some((Over::Rdp, _))) || !programs.is_empty();
            if kind != Kind::Windows || !over_rdp {
                return Err(refused(
                    file,
                    line,
                    format!("{name} signs people in as themselves, which is for a Windows machine with a desktop or programs"),
                ));
            }
            let named: Vec<&str> = std::iter::once(address.as_str()).chain(served_by.iter().map(|(address, _)| address.as_str())).collect();
            if let Some(number) = named.into_iter().find(|address| is_a_number(address)) {
                return Err(refused(
                    file,
                    line,
                    format!("{name} signs people in as themselves and is reached at {number}: a domain knows a machine by its name, and by no number"),
                ));
            }
        }
        SignsIn::RestrictedAdmin | SignsIn::RestrictedAdminSso => {
            if kind != Kind::Windows
                || !matches!(desktop, Some((Over::Rdp, _)))
                || collection.is_some()
                || !programs.is_empty()
                || !served_by.is_empty()
            {
                return Err(refused(file, line, format!("{name} needs a direct Windows desktop to use restricted-admin")));
            }
            if signs_in == SignsIn::RestrictedAdminSso && desktop != Some((Over::Rdp, 3389)) {
                return Err(refused(file, line, format!("{name} needs RDP port 3389 to use restricted-admin-sso")));
            }
            // The destination is used in a Windows Run command. Only a DNS name already
            // validated above and the parsed desktop port can be part of that command.
            if !address.contains('.')
                || is_a_number(&address)
                || !address.bytes().all(|byte| byte.is_ascii_alphanumeric() || b".-".contains(&byte))
            {
                return Err(refused(file, address_line, format!("{name} needs a fully qualified DNS name to use restricted-admin")));
            }
        }
        SignsIn::Typed => {}
    }
    let clipboard = clipboard(&parts, file, of)?;
    Ok(Machine {
        name,
        kind,
        address,
        groups,
        desktop,
        programs,
        shell,
        collection: collection.map(|(collection, _)| collection),
        served_by: served_by.into_iter().map(|(address, _)| address).collect(),
        signs_in,
        clipboard,
        farm: None,
        written_in: (file.to_string(), node.line),
    })
}

/// How people are signed in, and the line it is said in, where an entry says so.
fn signs_in(parts: &Parts, file: &str, of: &str) -> Result<Option<(SignsIn, usize)>, Refused> {
    match parts.text("signs-in", of)? {
        None => Ok(None),
        Some((way, line)) => match way.as_str() {
            "themselves" => Ok(Some((SignsIn::Themselves, line))),
            "typed" => Ok(Some((SignsIn::Typed, line))),
            "restricted-admin" => Ok(Some((SignsIn::RestrictedAdmin, line))),
            "restricted-admin-sso" => Ok(Some((SignsIn::RestrictedAdminSso, line))),
            _ => Err(refused(
                file,
                line,
                format!("“{way}” is no way people are signed in: themselves, typed, restricted-admin, or restricted-admin-sso"),
            )),
        },
    }
}

/// Which ways the clipboard passes, where an entry says so: both ways where it doesn't.
fn clipboard(parts: &Parts, file: &str, of: &str) -> Result<Clipboard, Refused> {
    match parts.text("clipboard", of)? {
        None => Ok(Clipboard::BOTH),
        Some((ways, line)) => match ways.as_str() {
            "both" => Ok(Clipboard::BOTH),
            "to-the-machine" => Ok(Clipboard { to_the_machine: true, from_the_machine: false }),
            "from-the-machine" => Ok(Clipboard { to_the_machine: false, from_the_machine: true }),
            "none" => Ok(Clipboard { to_the_machine: false, from_the_machine: false }),
            _ => {
                Err(refused(file, line, format!("“{ways}” is no way a clipboard passes: both, to-the-machine, from-the-machine, or none")))
            }
        },
    }
}

/// Whether an address is a number and no name, with or without a port after it.
fn is_a_number(address: &str) -> bool {
    let bare = |address: &str| address.trim_start_matches('[').trim_end_matches(']').parse::<std::net::IpAddr>().is_ok();
    bare(address) || address.rsplit_once(':').is_some_and(|(host, port)| port.bytes().all(|digit| digit.is_ascii_digit()) && bare(host))
}

/// Reads one farm.
fn farm(file: &str, node: &Node) -> Result<Farm, Refused> {
    let shape = "a farm is written as its name, its feed and its groups, beginning with “- name:”";
    let parts = Parts::of(file, node, &["name", "feed", "groups", "served-by", "signs-in", "clipboard"], "a farm", shape)?;
    let Some((name, _)) = parts.text("name", "a farm")? else { return Err(refused(file, node.line, "the farm has no name")) };
    if !name.chars().all(|character| character.is_alphanumeric() || " -_.()+&".contains(character)) {
        return Err(refused(
            file,
            node.line,
            format!(
                "the name “{name}” holds a character that names don't take: letters, digits, spaces and - _ . ( ) + & are what they take"
            ),
        ));
    }
    let Some((feed, line)) = parts.text("feed", &format!("the farm {name}"))? else {
        return Err(refused(file, node.line, format!("the farm {name} has no feed: the address where the farm says what it publishes")));
    };
    // a feed is asked over a secured connection, as everything is that Away signs in at
    if let Err(why) = Address::read(&feed) {
        return Err(refused(file, line, format!("the feed of the farm {name} is no address Away can use: {why}")));
    }
    let mut groups: Vec<String> = parts.texts("groups", &name)?.into_iter().map(|(group, _)| group).collect();
    groups.sort_by_key(|group| group.to_lowercase());
    let served_by = parts.texts("served-by", &name)?;
    if let Some((other, line)) = served_by.iter().find(|(address, _)| !serves(address)) {
        return Err(refused(file, *line, format!("“{other}” is no address of a machine")));
    }
    let themselves = match signs_in(&parts, file, &name)? {
        Some((SignsIn::Themselves, line)) => {
            if let Some((number, _)) = served_by.iter().find(|(address, _)| is_a_number(address)) {
                return Err(refused(
                    file,
                    line,
                    format!("the farm {name} signs people in as themselves and is served at {number}: a domain knows a machine by its name, and by no number"),
                ));
            }
            true
        }
        Some((SignsIn::RestrictedAdmin | SignsIn::RestrictedAdminSso, line)) => {
            return Err(refused(file, line, format!("the farm {name} cannot use restricted-admin: it is for a direct Windows desktop")));
        }
        _ => false,
    };
    let served_by = served_by.into_iter().map(|(address, _)| address).collect();
    let clipboard = clipboard(&parts, file, &name)?;
    Ok(Farm { name, feed, groups, served_by, themselves, clipboard, after: 0, written_in: (file.to_string(), node.line) })
}

/// Reads one rule.
fn rule(file: &str, node: &Node) -> Result<Rule, Refused> {
    let shape = "a rule is written as its name, who it is for and what they reach, beginning with “- name:”";
    let parts = Parts::of(file, node, &["name", "who", "reaches"], "a rule", shape)?;
    let Some((name, _)) = parts.text("name", "a rule")? else { return Err(refused(file, node.line, "the rule has no name")) };
    let of = format!("the rule {name}");
    let who: Vec<String> = parts.texts("who", &of)?.into_iter().map(|(who, _)| who).collect();
    if who.is_empty() {
        return Err(refused(file, node.line, format!("the rule {name} is for nobody: “who:” names the groups of people it is for")));
    }
    let reaches: Vec<String> = parts.texts("reaches", &of)?.into_iter().map(|(reached, _)| reached).collect();
    if reaches.is_empty() {
        return Err(refused(file, node.line, format!("the rule {name} reaches nothing: “reaches:” names machines and groups of machines")));
    }
    Ok(Rule { name, who, reaches, written_in: (file.to_string(), node.line) })
}

/// Whether a text can be where a machine that serves a collection is: an address, or an
/// address with the port its desktop is opened on after a colon.
fn serves(text: &str) -> bool {
    is_an_address(text)
        || text.rsplit_once(':').is_some_and(|(address, port)| {
            is_an_address(address) && port.bytes().all(|digit| digit.is_ascii_digit()) && port.parse::<u16>().is_ok_and(|port| port != 0)
        })
}

/// Whether a text can be where a machine is: a name of the network, or a number. What
/// the name leads to is found out when the machine is opened.
fn is_an_address(text: &str) -> bool {
    let inside = text.strip_prefix('[').and_then(|rest| rest.strip_suffix(']'));
    match inside {
        Some(number) => !number.is_empty() && number.chars().all(|character| character.is_ascii_hexdigit() || ":.%".contains(character)),
        None => {
            text.len() <= 253
                && text.split('.').all(|label| {
                    !label.is_empty()
                        && label.len() <= 63
                        && label.chars().all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '_')
                        && !label.starts_with('-')
                        && !label.ends_with('-')
                })
        }
    }
}

impl Machine {
    /// Whether a rule that reaches this name reaches the machine: its own name, one of
    /// its groups, or the farm that publishes it.
    pub fn is_reached_as(&self, reached: &str) -> bool {
        self.name == reached || self.groups.iter().any(|group| group == reached) || self.farm.as_deref() == Some(reached)
    }

    /// The machine as a person who reaches it is told of it.
    pub fn for_a_person(&self) -> Json {
        let way = |over: Option<Over>, port: u16| {
            let mut way = vec![("port".to_string(), Json::from(u64::from(port)))];
            way.extend(over.map(|over| ("over".to_string(), over.written().into())));
            Json::Object(way)
        };
        let mut machine = Json::object([
            ("name", self.name.as_str().into()),
            ("kind", self.kind.written().into()),
            ("address", self.address.as_str().into()),
            ("groups", Json::List(self.groups.iter().map(|group| group.as_str().into()).collect())),
            ("desktop", self.desktop.map_or(Json::Null, |(over, port)| way(Some(over), port))),
            (
                "programs",
                Json::List(
                    self.programs
                        .iter()
                        .map(|program| {
                            let icon = program.icon.as_ref().map_or(Json::Null, |sum| format!("/api/icons/{sum}.png").into());
                            Json::object([("name", program.name.as_str().into()), ("icon", icon)])
                        })
                        .collect(),
                ),
            ),
            ("shell", self.shell.map_or(Json::Null, |port| way(None, port))),
            ("collection", self.collection.as_deref().map_or(Json::Null, Json::from)),
        ]);
        if let (SignsIn::RestrictedAdmin, Some((Over::Rdp, port)), Json::Object(fields)) = (self.signs_in, self.desktop, &mut machine) {
            let command = format!("mstsc.exe /restrictedAdmin /v:{}:{port}", self.address);
            fields.push(("native_rdp".to_string(), command.into()));
        }
        machine
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MACHINES: &str = "\
# the machines of the finance department
machines:
  - name: fin-app-02
    kind: windows
    address: fin-app-02.corp.example
    groups: [finance]
    desktop: true
    programs: [Ledger, Payroll, Reports, Mail]
  - name: hr-app-01
    kind: windows
    address: hr-app-01.corp.example
    groups: [finance]
    desktop:
      port: 3390
";
    const MORE: &str = "\
machines:
  - name: build-01
    kind: linux
    address: build-01.corp.example
    groups: [engineering]
    shell: true
  - name: dev-linux-03
    kind: linux
    address: 10.20.0.13
    groups: [engineering]
    desktop: { over: rdp }
    shell: { port: 2222 }
  - name: mac-design-01
    kind: mac
    address: mac-design-01.corp.example
    groups: design
    desktop: true
  - name: Desktops
    kind: windows
    address: farm.corp.example
    desktop: true
    collection: Desktops
    served-by: [rdsh-01.corp.example, rdsh-02.corp.example]
";
    const RULES: &str = "\
rules:
  - name: finance-desktops
    who: [finance]
    reaches: [finance]
  - name: engineering-machines
    who: engineering
    reaches: engineering
  - name: design-mac
    who: [design]
    reaches: [mac-design-01]
  - name: audit-finance
    who: [away-audit]
    reaches: [fin-app-02]
  - name: everyone-desktops
    who: [staff]
    reaches: [Desktops]
";

    fn files(files: &[(&str, &str)]) -> Vec<(String, String)> {
        files.iter().map(|(name, text)| (name.to_string(), text.to_string())).collect()
    }

    fn in_force() -> InForce {
        InForce::read(&files(&[("machines/finance.yaml", MACHINES), ("machines/more.yaml", MORE), ("rules/access.yaml", RULES)])).unwrap()
    }

    fn why(file: &str) -> Refused {
        InForce::read(&files(&[("machines/finance.yaml", MACHINES), ("changed.yaml", file)])).unwrap_err()
    }

    #[test]
    fn the_files_say_the_machines_and_the_rules() {
        let in_force = in_force();
        let names: Vec<&str> = in_force.machines.iter().map(|machine| machine.name.as_str()).collect();
        // in the order of the files' names, and inside a file as they are written
        assert_eq!(names, ["fin-app-02", "hr-app-01", "build-01", "dev-linux-03", "mac-design-01", "Desktops"]);
        let named = |name: &str| in_force.machines.iter().find(|machine| machine.name == name).unwrap();
        let fin = named("fin-app-02");
        assert_eq!((fin.kind, fin.address.as_str(), fin.desktop), (Kind::Windows, "fin-app-02.corp.example", Some((Over::Rdp, 3389))));
        assert_eq!(fin.programs.iter().map(|program| program.name.as_str()).collect::<Vec<_>>(), ["Ledger", "Payroll", "Reports", "Mail"]);
        assert!(fin.programs.iter().all(|program| program.icon.is_none()) && in_force.pictures.is_empty());
        assert_eq!(fin.written_in, ("machines/finance.yaml".to_string(), 3));
        assert_eq!(named("hr-app-01").desktop, Some((Over::Rdp, 3390)));
        assert_eq!((named("build-01").desktop, named("build-01").shell), (None, Some(22)));
        // a Linux desktop that speaks RDP, and a shell on a port of its own
        assert_eq!((named("dev-linux-03").desktop, named("dev-linux-03").shell), (Some((Over::Rdp, 3389)), Some(2222)));
        assert_eq!(
            (named("mac-design-01").desktop, &named("mac-design-01").groups[..]),
            (Some((Over::Vnc, 5900)), &["design".to_string()][..])
        );
        let farm = named("Desktops");
        assert_eq!((farm.collection.as_deref(), farm.served_by.len(), farm.groups.is_empty()), (Some("Desktops"), 2, true));
        assert_eq!(in_force.rules.len(), 5);
        assert_eq!(in_force.rules[0].name, "finance-desktops");
        assert_eq!(in_force.rules[1].written_in, ("rules/access.yaml".to_string(), 5));
    }

    /// Every person of the tests against every machine: exactly the machines a rule
    /// gives them, and by which rule.
    #[test]
    fn every_person_reaches_exactly_the_machines_a_rule_gives_them() {
        let in_force = in_force();
        // who they are, the groups they are in, and each machine they reach with the rules that give it
        type Someone = (&'static str, &'static [&'static str], &'static [(&'static str, &'static [&'static str])]);
        let people: [Someone; 7] = [
            ("in finance", &["finance"], &[("fin-app-02", &["finance-desktops"]), ("hr-app-01", &["finance-desktops"])]),
            ("in engineering", &["engineering"], &[("build-01", &["engineering-machines"]), ("dev-linux-03", &["engineering-machines"])]),
            ("in design", &["design"], &[("mac-design-01", &["design-mac"])]),
            (
                "in finance and in design",
                &["design", "finance", "staff"],
                &[
                    ("Desktops", &["everyone-desktops"]),
                    ("fin-app-02", &["finance-desktops"]),
                    ("hr-app-01", &["finance-desktops"]),
                    ("mac-design-01", &["design-mac"]),
                ],
            ),
            (
                "who reads the records of finance",
                &["away-audit", "finance"],
                &[("fin-app-02", &["finance-desktops", "audit-finance"]), ("hr-app-01", &["finance-desktops"])],
            ),
            ("in marketing", &["marketing"], &[]),
            ("in no group", &[], &[]),
        ];
        for (who, groups, reaches) in people {
            let groups: Vec<String> = groups.iter().map(|group| group.to_string()).collect();
            let reached = in_force.reached_by(&groups);
            // for each machine there is: reached exactly when it is listed, and by the rules listed
            for machine in &in_force.machines {
                let listed = reaches.iter().find(|(name, _)| *name == machine.name);
                let found = reached.iter().find(|(found, _)| found.name == machine.name);
                match (listed, found) {
                    (Some((_, rules)), Some((_, by))) => assert_eq!(by, rules, "someone {who}, {}", machine.name),
                    (None, None) => {}
                    (listed, _) => panic!("someone {who} and {}: listed {}, reached {}", machine.name, listed.is_some(), found.is_some()),
                }
            }
            assert_eq!(reached.len(), reaches.len(), "someone {who}");
        }
        // a group of people is its whole name: a part of one gives nothing
        assert!(in_force.reached_by(&["fin".to_string(), "Finance".to_string(), "finance ".to_string()]).is_empty());
    }

    #[test]
    fn one_thing_wrong_refuses_the_set_with_the_file_the_line_and_the_reason() {
        let wrong = why(
            "machines:\n  - name: fin-app-03\n    kind: windows\n    address: fin-app-03.corp.example\n    prot: 3389\n    desktop: true\n",
        );
        assert_eq!(wrong, Refused { file: "changed.yaml".to_string(), line: 5, why: "“prot” is not something a machine has".to_string() });
        assert_eq!(wrong.to_string(), "changed.yaml, line 5: “prot” is not something a machine has");

        let reasons: [(&str, usize, &str); 22] = [
            ("machines:\n  - kind: windows\n", 2, "the machine has no name"),
            ("machines:\n  - name: a\n    address: a.example\n    desktop: true\n", 2, "a has no kind: windows, linux or mac"),
            ("machines:\n  - name: a\n    kind: solaris\n", 3, "“solaris” is no kind of machine: windows, linux or mac"),
            ("machines:\n  - name: a\n    kind: linux\n    shell: true\n", 2, "a has no address"),
            (
                "machines:\n  - name: a\n    kind: linux\n    address: 'not an address'\n    shell: true\n",
                4,
                "“not an address” is no address of a machine: a name such as fin-app-02.corp.example, or a number",
            ),
            ("machines:\n  - name: a\n    kind: linux\n    address: a.example\n", 2, "a offers no way in: a desktop, programs or a shell"),
            (
                "machines:\n  - name: a\n    kind: linux\n    address: a.example\n    programs: [Editor]\n",
                5,
                "a names programs, which only a Windows machine has",
            ),
            (
                "machines:\n  - name: a\n    kind: linux\n    address: a.example\n    shell: { port: 70000 }\n",
                5,
                "“70000” is no port: a port is a number from 1 to 65535",
            ),
            (
                "machines:\n  - name: a\n    kind: linux\n    address: a.example\n    shell: maybe\n",
                5,
                "the shell of a is said with true or false, or with its port",
            ),
            (
                "machines:\n  - name: a\n    kind: linux\n    address: a.example\n    desktop: { over: x11 }\n",
                5,
                "“x11” is nothing a desktop is reached over: rdp or vnc",
            ),
            (
                "machines:\n  - name: a\n    kind: windows\n    address: a.example\n    desktop: { over: vnc }\n",
                5,
                "the desktop of a is reached over what a windows machine doesn't speak",
            ),
            (
                "machines:\n  - name: a\n    kind: linux\n    address: a.example\n    shell: { over: rdp }\n",
                5,
                "“over” is not something a shell has",
            ),
            (
                "machines:\n  - name: FIN-APP-02\n    kind: linux\n    address: a.example\n    shell: true\n",
                2,
                "the machine FIN-APP-02 is also written in machines/finance.yaml, line 3",
            ),
            (
                "machines:\n  - name: a/b\n    kind: linux\n    address: a.example\n    shell: true\n",
                2,
                "the name “a/b” holds a character that names don't take: letters, digits, spaces and - _ . ( ) + & are what they take",
            ),
            (
                "machines:\n  - name: a\n    kind: linux\n    address: a.example\n    shell: true\n    collection: Farm\n",
                6,
                "a names a collection, which is a farm of Windows machines with a desktop or programs",
            ),
            (
                "machines:\n  - name: a\n    kind: windows\n    address: a.example\n    desktop: true\n    served-by: [b.example]\n",
                6,
                "a names machines that serve it, and no collection for them to serve",
            ),
            (
                "machines:\n  - name: a\n    kind: windows\n    address: a.example\n    desktop: true\n    groups: [x, X]\n",
                6,
                "“X” is twice among the groups of a",
            ),
            ("machines: nothing\n", 1, "the machines are a list, each beginning with “- name:”"),
            ("settings:\n  colour: blue\n", 1, "“settings” is not something a file has"),
            ("- name: a\n", 1, "a file holds machines, rules and farms, under “machines:”, “rules:” and “farms:”"),
            (
                "rules:\n  - name: r\n    who: [finance]\n    reaches: [nothing-there]\n",
                2,
                "the rule r reaches “nothing-there”, which is neither a machine nor a group of machines",
            ),
            ("rules:\n  - name: r\n    reaches: [finance]\n", 2, "the rule r is for nobody: “who:” names the groups of people it is for"),
        ];
        for (file, line, reason) in reasons {
            let wrong = why(file);
            assert_eq!((wrong.file.as_str(), wrong.line, wrong.why.as_str()), ("changed.yaml", line, reason), "{file}");
        }
        // a name that is a machine and a group of machines: a rule could mean either
        let both = why("machines:\n  - name: finance\n    kind: linux\n    address: a.example\n    shell: true\nrules:\n  - name: r\n    who: x\n    reaches: finance\n");
        assert_eq!(both.why, "the rule r reaches “finance”, which is the name of a machine and of a group of machines");
        let twice = InForce::read(&files(&[("a.yaml", RULES), ("b.yaml", "rules:\n  - name: design-mac\n    who: x\n    reaches: y\n")]))
            .unwrap_err();
        assert_eq!((twice.file.as_str(), twice.why.as_str()), ("b.yaml", "the rule design-mac is also written in a.yaml, line 8"));
        // and a file that is not YAML at all
        assert!(why("machines:\n  - name: a\n   kind: linux\n").why.starts_with("it is not YAML: "));
    }

    #[test]
    fn a_program_names_its_icon_and_the_picture_is_held_under_a_name_made_from_itself() {
        let machine = "machines:\n  - name: fin-app-03\n    kind: windows\n    address: a.example\n    programs:\n      - name: Ledger\n        icon: icons/ledger.png\n      - Payroll\n      - name: Reports\n        icon: icons/ledger.png\n";
        let picture = [PNG, b"the rest of a picture"].concat();
        let pictures = vec![("icons/ledger.png".to_string(), picture.clone()), ("icons/unused.png".to_string(), PNG.to_vec())];
        let in_force = InForce::read_with_pictures(&files(&[("a.yaml", machine)]), &pictures).unwrap();
        let programs = &in_force.machines[0].programs;
        let sum = programs[0].icon.clone().expect("an icon");
        assert_eq!((sum.len(), sum.chars().all(|c| c.is_ascii_hexdigit())), (32, true));
        assert_eq!((programs[1].icon.as_ref(), programs[2].icon.as_ref()), (None, Some(&sum)));
        // one picture, held once, and none that no program names
        assert_eq!(in_force.pictures.len(), 1);
        assert_eq!(in_force.pictures[&sum].as_ref(), &picture);
        let told = in_force.machines[0].for_a_person().write();
        assert!(
            told.contains(&format!(r#""programs":[{{"name":"Ledger","icon":"/api/icons/{sum}.png"}},{{"name":"Payroll","icon":null}},"#)),
            "{told}"
        );

        let wrong =
            |file: &str, pictures: &[(String, Vec<u8>)]| InForce::read_with_pictures(&files(&[("a.yaml", file)]), pictures).unwrap_err();
        let reasons: [(&str, usize, &str); 6] = [
            (
                "icons/ledgr.png",
                7,
                "the icon of Ledger is “icons/ledgr.png”, and the folder holds no picture of that name: a PNG picture, named by where it is in the folder",
            ),
            ("icons/text.png", 7, "the icon of Ledger, “icons/text.png”, is no PNG picture"),
            ("", 7, "the icon of the program Ledger is empty"),
            ("ICN", 7, "“icn” is not something a program has"),
            ("TWICE", 8, "“ledger” is twice among the programs of fin-app-03"),
            ("NONAME", 6, "a program of fin-app-03 has no name"),
        ];
        let text = vec![("icons/text.png".to_string(), b"not a picture".to_vec())];
        for (case, line, reason) in reasons {
            let file = match case {
                "ICN" => {
                    machine.replace("        icon: icons/ledger.png\n      - Payroll", "        icn: icons/ledger.png\n      - Payroll")
                }
                "TWICE" => machine.replace("      - Payroll", "      - ledger"),
                "NONAME" => machine.replace("      - name: Ledger\n        icon: icons/ledger.png", "      - icon: icons/ledger.png"),
                path => machine.replacen("icon: icons/ledger.png", &format!("icon: {path}"), 1),
            };
            let refused = wrong(&file, if case == "icons/text.png" { &text } else { &pictures });
            assert_eq!((refused.line, refused.why.as_str()), (line, reason), "{case}");
        }
        assert_eq!(
            wrong("machines:\n  - name: a\n    kind: windows\n    address: a.example\n    programs: Ledger\n", &[]).why,
            "the programs of a are a list: of names, or of names with their icons"
        );
    }

    #[test]
    fn an_entry_says_which_ways_the_clipboard_passes_and_left_out_it_passes_both() {
        let machine = |name: &str, more: &str| {
            format!("  - name: {name}\n    kind: windows\n    address: {name}.corp.example\n    desktop: true\n{more}")
        };
        let written = format!(
            "machines:\n{}{}{}{}{}farms:\n  - name: Work Resources\n    feed: https://broker.corp.example/feed\n    clipboard: from-the-machine\n  - name: Others\n    feed: https://other.corp.example/feed\n",
            machine("unsaid", ""),
            machine("both", "    clipboard: both\n"),
            machine("to-it", "    clipboard: to-the-machine\n"),
            machine("from-it", "    clipboard: from-the-machine\n"),
            machine("neither", "    clipboard: none\n"),
        );
        let in_force = InForce::read(&files(&[("ways.yaml", &written)])).unwrap();
        let ways: Vec<(&str, &str)> = in_force.machines.iter().map(|machine| (machine.name.as_str(), machine.clipboard.word())).collect();
        assert_eq!(
            ways,
            [("unsaid", "both"), ("both", "both"), ("to-it", "to-the-machine"), ("from-it", "from-the-machine"), ("neither", "none")]
        );
        let neither = in_force.machines.last().expect("a machine").clipboard;
        assert!(!neither.to_the_machine && !neither.from_the_machine);
        let ways: Vec<(&str, &str)> = in_force.farms.iter().map(|farm| (farm.name.as_str(), farm.clipboard.word())).collect();
        assert_eq!(ways, [("Work Resources", "from-the-machine"), ("Others", "both")]);
        // what a farm publishes passes the clipboard as the farm says
        let farm = &in_force.farms[0];
        assert_eq!(farm.clipboard, Clipboard { to_the_machine: false, from_the_machine: true });

        let refused =
            why("machines:\n  - name: a\n    kind: windows\n    address: a.example\n    desktop: true\n    clipboard: sometimes\n");
        assert_eq!(
            (refused.line, refused.why.as_str()),
            (6, "“sometimes” is no way a clipboard passes: both, to-the-machine, from-the-machine, or none")
        );
        let refused = why("farms:\n  - name: A\n    feed: https://a.example/feed\n    clipboard: [both]\n");
        assert_eq!(refused.line, 4);
    }

    #[test]
    fn restricted_admin_only_offers_a_direct_windows_desktop_at_a_dns_name() {
        let machine = |kind, address, more| {
            format!(
                "machines:\n  - name: admin\n    kind: {kind}\n    address: {address}\n    desktop: true\n    signs-in: restricted-admin\n{more}"
            )
        };
        for written in [
            machine("linux", "admin.corp.example", ""),
            machine("windows", "admin.corp.example", "    collection: Desktops\n"),
            machine("windows", "admin.corp.example", "    programs: [Ledger]\n"),
        ] {
            assert!(why(&written).why.contains("needs a direct Windows desktop"));
        }
        for address in ["10.20.0.1", "admin", "admin_host.corp.example"] {
            assert!(why(&machine("windows", address, "")).why.contains("needs a fully qualified DNS name"));
        }
        let farm = "farms:\n  - name: Apps\n    feed: https://broker.corp.example/feed\n    signs-in: restricted-admin\n";
        assert!(why(farm).why.contains("the farm Apps cannot use restricted-admin"));
    }

    #[test]
    fn an_entry_says_how_people_are_signed_in_and_left_out_they_type() {
        let machine = |name: &str, more: &str| {
            format!("  - name: {name}\n    kind: windows\n    address: {name}.corp.example\n    desktop: true\n{more}")
        };
        let written = format!(
            "machines:\n{}{}{}farms:\n  - name: Work Resources\n    feed: https://broker.corp.example/feed\n    served-by: [rdsh-01.corp.example]\n    signs-in: themselves\n  - name: Others\n    feed: https://other.corp.example/feed\n",
            machine("as-oneself", "    signs-in: themselves\n"),
            machine("by-typing", "    signs-in: typed\n"),
            machine("unsaid", ""),
        );
        let in_force = InForce::read(&files(&[("ways.yaml", &written)])).unwrap();
        let ways: Vec<(&str, SignsIn)> = in_force.machines.iter().map(|machine| (machine.name.as_str(), machine.signs_in)).collect();
        assert_eq!(ways, [("as-oneself", SignsIn::Themselves), ("by-typing", SignsIn::Typed), ("unsaid", SignsIn::Typed)]);
        let ways: Vec<(&str, bool)> = in_force.farms.iter().map(|farm| (farm.name.as_str(), farm.themselves)).collect();
        assert_eq!(ways, [("Work Resources", true), ("Others", false)]);

        // as themselves: on a Windows machine with a desktop or programs, which a domain knows by its name
        let reasons: [(&str, usize, &str); 5] = [
            (
                "machines:\n  - name: a\n    kind: windows\n    address: a.example\n    desktop: true\n    signs-in: somehow\n",
                6,
                "“somehow” is no way people are signed in: themselves, typed, restricted-admin, or restricted-admin-sso",
            ),
            (
                "machines:\n  - name: a\n    kind: linux\n    address: a.example\n    shell: true\n    signs-in: themselves\n",
                6,
                "a signs people in as themselves, which is for a Windows machine with a desktop or programs",
            ),
            (
                "machines:\n  - name: a\n    kind: windows\n    address: 10.0.0.7\n    desktop: true\n    signs-in: themselves\n",
                6,
                "a signs people in as themselves and is reached at 10.0.0.7: a domain knows a machine by its name, and by no number",
            ),
            (
                "machines:\n  - name: a\n    kind: windows\n    address: broker.example\n    desktop: true\n    collection: A\n    served-by: ['10.0.0.8:3390']\n    signs-in: themselves\n",
                8,
                "a signs people in as themselves and is reached at 10.0.0.8:3390: a domain knows a machine by its name, and by no number",
            ),
            (
                "farms:\n  - name: A\n    feed: https://a.example/feed\n    served-by: ['[fd00::8]']\n    signs-in: themselves\n",
                5,
                "the farm A signs people in as themselves and is served at [fd00::8]: a domain knows a machine by its name, and by no number",
            ),
        ];
        for (file, line, reason) in reasons {
            let refused = why(file);
            assert_eq!((refused.line, refused.why.as_str()), (line, reason), "{file}");
        }
        // typed, a machine is reached by a number as well
        assert!(InForce::read(&files(&[(
            "a.yaml",
            "machines:\n  - name: a\n    kind: windows\n    address: 10.0.0.7\n    desktop: true\n    signs-in: typed\n"
        )]))
        .is_ok());
    }

    #[test]
    fn a_farm_has_its_place_among_the_machines_in_the_files_order() {
        let machine = |name: &str| format!("  - name: {name}\n    kind: linux\n    address: {name}.example\n    shell: true\n");
        let farm = |name: &str| format!("  - name: {name}\n    feed: https://{name}.example/feed\n");
        // a file with a machine and then a farm; one with a farm between two machines'
        // lines, written before them; one with nothing but a farm
        let first = format!("machines:\n{}farms:\n{}", machine("one"), farm("after-one"));
        let second = format!("farms:\n{}machines:\n{}{}", farm("before-two"), machine("two"), machine("three"));
        let third = format!("farms:\n{}", farm("last"));
        let in_force = InForce::read(&files(&[("1.yaml", &first), ("2.yaml", &second), ("3.yaml", &third)])).unwrap();
        let places: Vec<(&str, usize)> = in_force.farms.iter().map(|farm| (farm.name.as_str(), farm.after)).collect();
        assert_eq!(places, [("after-one", 1), ("before-two", 1), ("last", 3)]);
    }

    #[test]
    fn a_farm_names_its_feed_and_rules_reach_it_by_its_name_or_its_groups() {
        let farm = "farms:\n  - name: Work Resources\n    feed: https://broker.corp.example/RDWeb/Feed/webfeed.aspx\n    groups: [published]\nrules:\n  - name: staff-published\n    who: [staff]\n    reaches: [published]\n  - name: admins-farm\n    who: [away-admins]\n    reaches: [Work Resources]\n";
        let in_force = InForce::read(&files(&[("farm.yaml", farm)])).unwrap();
        assert_eq!(in_force.farms.len(), 1);
        let read = &in_force.farms[0];
        assert_eq!(
            (read.name.as_str(), read.feed.as_str(), &read.groups[..]),
            ("Work Resources", "https://broker.corp.example/RDWeb/Feed/webfeed.aspx", &["published".to_string()][..])
        );
        assert_eq!(read.written_in, ("farm.yaml".to_string(), 2));
        assert!(read.served_by.is_empty());
        // a farm may name the machines that serve what it publishes
        let named =
            farm.replace("    groups: [published]\n", "    groups: [published]\n    served-by: [rdsh-01.corp.example, 10.20.0.18:3390]\n");
        let in_force = InForce::read(&files(&[("farm.yaml", &named)])).unwrap();
        assert_eq!(in_force.farms[0].served_by, ["rdsh-01.corp.example", "10.20.0.18:3390"]);
        // what a farm publishes is not in the files: nobody reaches anything until its feed is read
        assert!(in_force.machines.is_empty() && in_force.reached_by(&["staff".to_string()]).is_empty());

        let wrong = |file: &str| InForce::read(&files(&[("machines/finance.yaml", MACHINES), ("farm.yaml", file)])).unwrap_err();
        let reasons: [(&str, usize, &str); 8] = [
            ("farms:\n  - feed: https://a.example/feed\n", 2, "the farm has no name"),
            ("farms:\n  - name: A\n", 2, "the farm A has no feed: the address where the farm says what it publishes"),
            ("farms:\n  - name: A\n    feed: http://a.example/feed\n", 3, "the feed of the farm A is no address Away can use: "),
            ("farms:\n  - name: A\n    feed: https://a.example/feed\n    account: svc\n", 4, "“account” is not something a farm has"),
            (
                "farms:\n  - name: A\n    feed: https://a.example/feed\n    served-by: [rdsh-01.corp.example, rdsh 02]\n",
                4,
                "“rdsh 02” is no address of a machine",
            ),
            (
                "farms:\n  - name: A\n    feed: https://a.example/feed\n  - name: a\n    feed: https://b.example/feed\n",
                4,
                "the farm a is also written in farm.yaml, line 2",
            ),
            (
                "farms:\n  - name: FIN-APP-02\n    feed: https://a.example/feed\n",
                2,
                "the farm FIN-APP-02 has the name of a machine, which is written in machines/finance.yaml, line 3",
            ),
            (
                "farms:\n  - name: finance\n    feed: https://a.example/feed\nrules:\n  - name: r\n    who: x\n    reaches: finance\n",
                5,
                "the rule r reaches “finance”, which is the name of a machine and of a group of machines",
            ),
        ];
        for (file, line, reason) in reasons {
            let refused = wrong(file);
            assert_eq!(refused.line, line, "{file}");
            assert!(refused.why.starts_with(reason), "{}: {file}", refused.why);
        }
    }

    #[test]
    fn files_of_nothing_hold_nothing() {
        assert_eq!(InForce::read(&[]), Ok(InForce::default()));
        assert_eq!(
            InForce::read(&files(&[("a.yaml", ""), ("b.yaml", "# later\n"), ("c.yaml", "machines:\nrules:\n")])),
            Ok(InForce::default())
        );
    }

    #[test]
    fn an_address_is_a_name_of_the_network_or_a_number() {
        for address in ["fin-app-02.corp.example", "srv", "10.20.0.13", "[fd00::13]", "a_b.example", "xn--mnchen-3ya.example"] {
            assert!(is_an_address(address), "{address}");
        }
        for address in ["", "a b", "a..b", "-a.example", "a-.example", "a/b", "http://a.example", "a.example:3389", "[]", &"a".repeat(64)] {
            assert!(!is_an_address(address), "{address}");
        }
        // a machine that serves a collection may name the port its desktop is opened on
        for address in ["rdsh-01.corp.example", "rdsh-01.corp.example:3390", "10.20.0.17:3389", "[fd00::17]", "[fd00::17]:3390"] {
            assert!(serves(address), "{address}");
        }
        for address in [
            "",
            "rdsh-01.corp.example:",
            "rdsh-01.corp.example:0",
            "rdsh-01.corp.example:65536",
            "rdsh-01.corp.example:rdp",
            ":3389",
            "a:1:2",
            "fd00::17",
        ] {
            assert!(!serves(address), "{address}");
        }
    }

    #[test]
    fn a_person_is_told_what_a_machine_is_and_not_where_it_is_written() {
        let in_force = in_force();
        let told = in_force.machines.iter().find(|machine| machine.name == "dev-linux-03").unwrap().for_a_person().write();
        assert_eq!(
            told,
            r#"{"name":"dev-linux-03","kind":"linux","address":"10.20.0.13","groups":["engineering"],"desktop":{"port":3389,"over":"rdp"},"programs":[],"shell":{"port":2222},"collection":null}"#
        );
    }
}
