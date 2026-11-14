//! The folder of files, and what is in force: the files are read when the server starts
//! and whenever they change, and a set that is refused changes nothing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use away_part::address::Address;
use away_part::lines::Lines;
use sha2::{Digest, Sha256};

use super::{same, Farm, InForce, Refused, LARGEST_PICTURE};
use crate::farm::{self, Item};

/// How many files a folder may hold, how large one may be, and how deep its folders go.
const MOST_FILES: usize = 1000;
const LARGEST: u64 = 1024 * 1024;
const DEEPEST: usize = 4;

/// What a folder holds: the files of machines and rules, and the pictures they may
/// name as icons, each with its name inside the folder, in the order of the names.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Folder {
    /// The files of machines and rules: `.yaml` and `.yml`.
    pub files: Vec<(String, String)>,
    /// The pictures: `.png`.
    pub pictures: Vec<(String, Vec<u8>)>,
}

/// Reads every file of machines and rules below a folder, and every picture.
pub fn read_folder(folder: &Path) -> Result<Folder, Refused> {
    let mut holds = Folder::default();
    collect(folder, folder, 0, &mut holds)?;
    holds.files.sort();
    holds.pictures.sort();
    Ok(holds)
}

fn collect(root: &Path, folder: &Path, depth: usize, holds: &mut Folder) -> Result<(), Refused> {
    let cant = |what: &str, path: &Path, error: std::io::Error| Refused {
        file: path.strip_prefix(root).unwrap_or(path).display().to_string(),
        line: 0,
        why: format!("{what} can't be read: {error}"),
    };
    let mut entries: Vec<PathBuf> = std::fs::read_dir(folder)
        .map_err(|error| cant("the folder of files", folder, error))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    entries.sort();
    for path in entries {
        let name = path.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default();
        // what begins with a dot is not the operator's: a cluster keeps its own there
        if name.starts_with('.') {
            continue;
        }
        // a link is followed: a cluster mounts every file as one
        let is = std::fs::metadata(&path).map_err(|error| cant("the file", &path, error))?;
        if is.is_dir() {
            if depth < DEEPEST {
                collect(root, &path, depth + 1, holds)?;
            }
        } else if name.ends_with(".yaml") || name.ends_with(".yml") {
            let below = path.strip_prefix(root).unwrap_or(&path).display().to_string();
            if is.len() > LARGEST {
                return Err(Refused { file: below, line: 0, why: "the file is larger than a file of the register may be".to_string() });
            }
            if holds.files.len() + holds.pictures.len() >= MOST_FILES {
                return Err(Refused { file: below, line: 0, why: format!("the folder holds more than {MOST_FILES} files") });
            }
            let text = std::fs::read(&path).map_err(|error| cant("the file", &path, error))?;
            let text =
                String::from_utf8(text).map_err(|_| Refused { file: below.clone(), line: 0, why: "the file is not text".to_string() })?;
            holds.files.push((below, text));
        } else if name.ends_with(".png") {
            // a picture, which a program may name as its icon
            let below = path.strip_prefix(root).unwrap_or(&path).display().to_string();
            if is.len() > LARGEST_PICTURE as u64 {
                return Err(Refused { file: below, line: 0, why: format!("the picture is larger than {} KiB", LARGEST_PICTURE / 1024) });
            }
            if holds.files.len() + holds.pictures.len() >= MOST_FILES {
                return Err(Refused { file: below, line: 0, why: format!("the folder holds more than {MOST_FILES} files") });
            }
            holds.pictures.push((below, std::fs::read(&path).map_err(|error| cant("the picture", &path, error))?));
        }
    }
    Ok(())
}

/// A change that was refused: when, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    /// When, in seconds since 1970.
    pub at: u64,
    /// Why.
    pub why: Refused,
}

/// What was read from a farm's feed, and when.
#[derive(Clone, Debug, Default)]
struct Read {
    /// The feed it was read from: a farm whose feed is changed is read anew.
    feed: String,
    /// What the farm publishes, as it was last read.
    items: Vec<Item>,
    /// When it was last read, and when it was last asked.
    at: Option<u64>,
    asked_at: u64,
    /// Why the last asking gave nothing.
    failed: Option<String>,
    /// What it publishes that is left out, because a machine of the files has its name.
    left_out: Vec<String>,
}

/// A farm as those who administer see it: what the files say of it, and how its feed
/// was last read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FarmState {
    /// The farm, as the files have it.
    pub farm: Farm,
    /// When its feed was last read, in seconds since 1970.
    pub read_at: Option<u64>,
    /// Why the last asking gave nothing, where it gave nothing.
    pub failed: Option<String>,
    /// How many desktops and how many programs it publishes.
    pub desktops: usize,
    /// How many programs.
    pub programs: usize,
    /// What it publishes that is left out, because a machine of the files has its name.
    pub left_out: Vec<String>,
}

struct State {
    /// What the files say.
    from_files: Arc<InForce>,
    /// What is in force: what the files say, with what their farms publish.
    in_force: Arc<InForce>,
    since: u64,
    refused: Option<Refusal>,
    /// What the files held when they were last looked at, as a sum.
    seen: Option<[u8; 32]>,
    /// What was read from the farms' feeds, by the farms' names.
    farms: BTreeMap<String, Read>,
}

impl State {
    /// Puts together what is in force: what the files say, and what each of their farms
    /// was last read to publish. A machine of the files keeps its name: what a farm
    /// publishes under the same name is left out.
    fn put_together(&mut self) {
        let mut in_force = (*self.from_files).clone();
        for farm in &self.from_files.farms {
            let Some(read) = self.farms.get_mut(&farm.name).filter(|read| read.feed == farm.feed) else { continue };
            let host = Address::read(&farm.feed).map(|address| address.host).unwrap_or_default();
            let published = farm::publish(farm, &host, &read.items);
            read.left_out.clear();
            for machine in published.machines {
                if in_force.machines.iter().any(|other| same(&other.name, &machine.name)) {
                    read.left_out.push(machine.name);
                } else {
                    in_force.machines.push(machine);
                }
            }
            in_force.pictures.extend(published.pictures);
        }
        self.in_force = Arc::new(in_force);
    }
}

/// The register as the server holds it.
pub struct Register {
    folder: Option<PathBuf>,
    lines: Lines,
    state: RwLock<State>,
}

impl Register {
    /// The register of a folder of files, read once. Without a folder it holds nothing.
    pub fn new(folder: Option<PathBuf>, lines: Lines, now: u64) -> Register {
        let register = Register {
            folder,
            lines,
            state: RwLock::new(State {
                from_files: Arc::new(InForce::default()),
                in_force: Arc::new(InForce::default()),
                since: now,
                refused: None,
                seen: None,
                farms: BTreeMap::new(),
            }),
        };
        register.look(now);
        register
    }

    /// Whether a folder of files is named at all.
    pub fn has_files(&self) -> bool {
        self.folder.is_some()
    }

    /// What is in force, and since when.
    pub fn in_force(&self) -> (Arc<InForce>, u64) {
        let state = self.state.read().unwrap_or_else(|poisoned| poisoned.into_inner());
        (state.in_force.clone(), state.since)
    }

    /// The last change that was refused, where the files as they are now are refused.
    pub fn refused(&self) -> Option<Refusal> {
        self.state.read().unwrap_or_else(|poisoned| poisoned.into_inner()).refused.clone()
    }

    /// The farms whose feeds are to be read now: those that were never read, those whose
    /// feed the files changed, and those last asked longer ago than this many seconds.
    /// With each comes what was read from it before.
    pub fn farms_to_read(&self, now: u64, every: u64) -> Vec<(Farm, Vec<Item>)> {
        let state = self.state.read().unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut due = Vec::new();
        for farm in &state.from_files.farms {
            match state.farms.get(&farm.name).filter(|read| read.feed == farm.feed) {
                Some(read) if now.saturating_sub(read.asked_at) < every => {}
                Some(read) => due.push((farm.clone(), read.items.clone())),
                None => due.push((farm.clone(), Vec::new())),
            }
        }
        due
    }

    /// Takes what a farm's feed gave: what the farm publishes, or why it could not be
    /// read. What was read before stays where the feed could not be read now.
    pub fn farm_was_read(&self, farm: &Farm, outcome: Result<Vec<Item>, String>, now: u64) {
        let mut state = self.state.write().unwrap_or_else(|poisoned| poisoned.into_inner());
        if !state.from_files.farms.iter().any(|named| named.name == farm.name && named.feed == farm.feed) {
            return;
        }
        let before = state.farms.remove(&farm.name).filter(|read| read.feed == farm.feed).unwrap_or_default();
        let read = match outcome {
            Ok(items) => {
                if before.at.is_none() || before.failed.is_some() || before.items != items {
                    let desktops = items.iter().filter(|item| item.resource.desktop).count();
                    self.lines.info(
                        "a farm's feed was read",
                        &[
                            ("farm", farm.name.as_str().into()),
                            ("desktops", (desktops as u64).into()),
                            ("programs", ((items.len() - desktops) as u64).into()),
                        ],
                    );
                }
                Read { feed: farm.feed.clone(), items, at: Some(now), asked_at: now, failed: None, left_out: Vec::new() }
            }
            Err(why) => {
                if before.failed.as_deref() != Some(why.as_str()) {
                    self.lines.warn(
                        "a farm's feed could not be read, and what it published before stays",
                        &[("farm", farm.name.as_str().into()), ("why", why.as_str().into())],
                    );
                }
                Read { asked_at: now, failed: Some(why), feed: farm.feed.clone(), ..before }
            }
        };
        state.farms.insert(farm.name.clone(), read);
        state.put_together();
    }

    /// The farms, as those who administer see them.
    pub fn farms(&self) -> Vec<FarmState> {
        let state = self.state.read().unwrap_or_else(|poisoned| poisoned.into_inner());
        state
            .from_files
            .farms
            .iter()
            .map(|farm| {
                let read = state.farms.get(&farm.name).filter(|read| read.feed == farm.feed).cloned().unwrap_or_default();
                let desktops = read.items.iter().filter(|item| item.resource.desktop).count();
                FarmState {
                    farm: farm.clone(),
                    read_at: read.at,
                    failed: read.failed,
                    desktops,
                    programs: read.items.len() - desktops,
                    left_out: read.left_out,
                }
            })
            .collect()
    }

    /// Looks at the files, and applies them where they changed: whole, or not at all.
    pub fn look(&self, now: u64) {
        let Some(folder) = &self.folder else { return };
        let read = read_folder(folder);
        let seen: [u8; 32] = match &read {
            Ok(folder) => {
                let mut sum = Sha256::new();
                let texts = folder.files.iter().map(|(name, text)| (name, text.as_bytes()));
                for (name, holds) in texts.chain(folder.pictures.iter().map(|(name, bytes)| (name, bytes.as_slice()))) {
                    sum.update((name.len() as u64).to_le_bytes());
                    sum.update(name);
                    sum.update((holds.len() as u64).to_le_bytes());
                    sum.update(holds);
                }
                sum.finalize().into()
            }
            // a folder that can't be read is another state than any that can
            Err(refused) => Sha256::digest(format!("unread: {refused}")).into(),
        };
        if self.state.read().unwrap_or_else(|poisoned| poisoned.into_inner()).seen == Some(seen) {
            return;
        }
        let applied = read
            .and_then(|folder| InForce::read_with_pictures(&folder.files, &folder.pictures).map(|in_force| (in_force, folder.files.len())));
        let mut state = self.state.write().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.seen = Some(seen);
        match applied {
            Ok((in_force, files)) => {
                self.lines.info(
                    "the files were applied",
                    &[
                        ("files", (files as u64).into()),
                        ("machines", (in_force.machines.len() as u64).into()),
                        ("rules", (in_force.rules.len() as u64).into()),
                    ],
                );
                state.from_files = Arc::new(in_force);
                state.since = now;
                state.refused = None;
                // a farm the files no longer name is forgotten
                let named: Vec<String> = state.from_files.farms.iter().map(|farm| farm.name.clone()).collect();
                state.farms.retain(|name, _| named.contains(name));
                state.put_together();
            }
            Err(why) => {
                self.lines.warn(
                    "a change to the files was refused, and what was in force stays",
                    &[("file", why.file.as_str().into()), ("line", (why.line as u64).into()), ("why", why.why.as_str().into())],
                );
                state.refused = Some(Refusal { at: now, why });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of its own for a test, removed when the test ends.
    struct Folder(PathBuf);

    impl Folder {
        fn new(name: &str) -> Folder {
            let path = std::env::temp_dir().join(format!("away-register-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("a folder");
            Folder(path)
        }

        fn write(&self, name: &str, text: &str) {
            let path = self.0.join(name);
            std::fs::create_dir_all(path.parent().expect("a folder")).expect("a folder");
            std::fs::write(path, text).expect("a file");
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const MACHINE: &str =
        "machines:\n  - name: build-01\n    kind: linux\n    address: build-01.corp.example\n    groups: [engineering]\n    shell: true\n";
    const RULE: &str = "rules:\n  - name: engineering-machines\n    who: [engineering]\n    reaches: [engineering]\n";

    #[test]
    fn a_folder_is_read_with_its_folders_and_what_is_not_the_operators_is_left() {
        let folder = Folder::new("read");
        folder.write("rules/access.yaml", RULE);
        folder.write("machines/engineering.yaml", MACHINE);
        folder.write("machines/more.yml", "# later\n");
        folder.write("README.md", "not a file of the register");
        folder.write("..data/machines/engineering.yaml", "what a cluster keeps for itself");
        folder.write(".hidden.yaml", "machines: wrong\n");
        std::fs::write(folder.0.join("machines/build.png"), b"\x89PNG\r\n\x1a\nthe rest of a picture").expect("a picture");
        let holds = read_folder(&folder.0).unwrap();
        let names: Vec<&str> = holds.files.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["machines/engineering.yaml", "machines/more.yml", "rules/access.yaml"]);
        assert_eq!(holds.files[0].1, MACHINE);
        // a picture is taken too, for a program to name as its icon
        assert_eq!(holds.pictures.len(), 1);
        assert_eq!((holds.pictures[0].0.as_str(), holds.pictures[0].1.len()), ("machines/build.png", 29));
        // one that is larger than a picture of the files may be refuses the set
        std::fs::write(folder.0.join("vast.png"), vec![0u8; LARGEST_PICTURE + 1]).expect("a picture");
        assert_eq!(read_folder(&folder.0).unwrap_err().to_string(), "vast.png: the picture is larger than 256 KiB");
        std::fs::remove_file(folder.0.join("vast.png")).expect("removed");
        // a folder that is not there
        let missing = read_folder(&folder.0.join("nothing-here")).unwrap_err();
        assert!(missing.why.starts_with("the folder of files can't be read: "), "{}", missing.why);
    }

    #[test]
    fn a_change_is_applied_whole_and_a_wrong_one_changes_nothing() {
        let folder = Folder::new("change");
        folder.write("machines.yaml", MACHINE);
        folder.write("rules.yaml", RULE);
        let lines = Lines::kept_for("server");
        let register = Register::new(Some(folder.0.clone()), lines.clone(), 100);
        let (in_force, since) = register.in_force();
        assert_eq!((in_force.machines.len(), in_force.rules.len(), since, register.refused()), (1, 1, 100, None));

        // nothing changed: nothing is applied anew
        register.look(200);
        assert_eq!(register.in_force().1, 100);

        // a change with a mistake in it: refused, with the reason, and what was in force stays
        folder.write("machines.yaml", &MACHINE.replace("address:", "adress:"));
        register.look(300);
        let (in_force, since) = register.in_force();
        assert_eq!((in_force.machines.len(), since), (1, 100));
        let refusal = register.refused().expect("a refusal");
        assert_eq!((refusal.at, refusal.why.to_string().as_str()), (300, "machines.yaml, line 4: “adress” is not something a machine has"));
        // looked at again, the same mistake is not refused anew
        register.look(400);
        assert_eq!(register.refused().expect("a refusal").at, 300);

        // the mistake is put right, with a machine more: applied, and nothing is refused any more
        folder.write("machines.yaml", &format!("{MACHINE}  - name: build-02\n    kind: linux\n    address: build-02.corp.example\n    groups: [engineering]\n    shell: true\n"));
        register.look(500);
        let (in_force, since) = register.in_force();
        assert_eq!((in_force.machines.len(), since, register.refused()), (2, 500, None));

        // every outcome is a line
        let said = lines.kept().join("\n");
        assert_eq!(said.matches("\"event\":\"the files were applied\"").count(), 2, "{said}");
        assert_eq!(said.matches("\"event\":\"a change to the files was refused, and what was in force stays\"").count(), 1, "{said}");
        assert!(said.contains("\"file\":\"machines.yaml\",\"line\":4,\"why\":\"“adress” is not something a machine has\""), "{said}");
    }

    #[test]
    fn files_that_are_wrong_from_the_start_put_nothing_in_force_and_no_folder_holds_nothing() {
        let folder = Folder::new("start");
        folder.write("rules.yaml", RULE);
        let register = Register::new(Some(folder.0.clone()), Lines::kept_for("server"), 100);
        assert!(register.in_force().0.machines.is_empty());
        assert_eq!(
            register.refused().expect("a refusal").why.why,
            "the rule engineering-machines reaches “engineering”, which is neither a machine nor a group of machines"
        );
        let none = Register::new(None, Lines::kept_for("server"), 100);
        assert_eq!((none.has_files(), none.in_force().0.machines.len(), none.refused()), (false, 0, None));
        none.look(200);
        // a folder that is named and not there
        let missing = Register::new(Some(folder.0.join("nothing-here")), Lines::kept_for("server"), 100);
        assert!(missing.refused().is_some());
    }
}
