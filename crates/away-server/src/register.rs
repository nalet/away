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
//! ```
//!
//! A set of files is taken whole or not at all: one thing wrong anywhere, and the set is
//! refused with the file, the line and the reason, while what was in force stays.

pub mod files;
pub mod yaml;

use std::collections::BTreeMap;
use std::sync::Arc;

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
}

/// The largest a picture of the files may be, in bytes.
pub const LARGEST_PICTURE: usize = 256 * 1024;

/// What a PNG picture begins with.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";

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
    /// The file it is written in, and the line.
    pub written_in: (String, usize),
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
}

/// The longest a name, an address or a group may be.
const LONGEST: usize = 120;

fn refused(file: &str, line: usize, why: impl Into<String>) -> Refused {
    Refused { file: file.to_string(), line, why: why.into() }
}

/// The same name, however its letters are written.
fn same(one: &str, other: &str) -> bool {
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
        for (file, text) in files {
            let Some(whole) = yaml::read(text).map_err(|(line, why)| refused(file, line, why))? else { continue };
            let holds = Parts::of(
                file,
                &whole,
                &["machines", "rules"],
                "a file",
                "a file holds machines, rules, or both, under “machines:” and “rules:”",
            )?;
            for (name, read) in [("machines", 0), ("rules", 1)] {
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
        // what a rule reaches is a machine or a group of machines, and it is clear which
        for rule in &rules {
            let (file, line) = &rule.written_in;
            for reached in &rule.reaches {
                let is_machine = machines.iter().any(|machine| machine.name == *reached);
                let is_group = machines.iter().any(|machine| machine.groups.contains(reached));
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
        Ok(InForce { machines, rules, pictures: named })
    }

    /// The machines that someone of these groups reaches, each with the rules that give
    /// it to them, in the order the files have the machines.
    pub fn reached_by(&self, groups: &[String]) -> Vec<(&Machine, Vec<&str>)> {
        let mut reached: BTreeMap<usize, Vec<&str>> = BTreeMap::new();
        for rule in self.rules.iter().filter(|rule| rule.who.iter().any(|who| groups.contains(who))) {
            for (nth, machine) in self.machines.iter().enumerate() {
                if rule.reaches.iter().any(|reached| *reached == machine.name || machine.groups.contains(reached)) {
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
        let (name, icon) = match &node.value {
            Value::Thing(_) => {
                let parts = Parts::of(file, node, &["name", "icon"], "a program", "")?;
                let Some((name, _)) = parts.text("name", &format!("a program of {of}"))? else {
                    return Err(refused(file, node.line, format!("a program of {of} has no name")));
                };
                let icon = parts.text("icon", &format!("the program {name}"))?;
                (name, icon)
            }
            Value::Text(text) if !text.trim().is_empty() && text.chars().count() <= LONGEST && !text.chars().any(char::is_control) => {
                (text.trim().to_string(), None)
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
                let sum: String = Sha256::digest(picture).iter().take(16).map(|byte| format!("{byte:02x}")).collect();
                named.entry(sum.clone()).or_insert_with(|| Arc::new(picture.clone()));
                Some(sum)
            }
        };
        programs.push(Program { name, icon });
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
    let may_hold = ["name", "kind", "address", "groups", "desktop", "programs", "shell", "collection", "served-by"];
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
    if let Some((other, line)) = served_by.iter().find(|(address, _)| !is_an_address(address)) {
        return Err(refused(file, *line, format!("“{other}” is no address of a machine")));
    }
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
        written_in: (file.to_string(), node.line),
    })
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
    /// The machine as a person who reaches it is told of it.
    pub fn for_a_person(&self) -> Json {
        let way = |over: Option<Over>, port: u16| {
            let mut way = vec![("port".to_string(), Json::from(u64::from(port)))];
            way.extend(over.map(|over| ("over".to_string(), over.written().into())));
            Json::Object(way)
        };
        Json::object([
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
        ])
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
            ("- name: a\n", 1, "a file holds machines, rules, or both, under “machines:” and “rules:”"),
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
