//! The folder of files, and what is in force: the files are read when the server starts
//! and whenever they change, and a set that is refused changes nothing.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use away_part::lines::Lines;
use sha2::{Digest, Sha256};

use super::{InForce, Refused};

/// How many files a folder may hold, how large one may be, and how deep its folders go.
const MOST_FILES: usize = 1000;
const LARGEST: u64 = 1024 * 1024;
const DEEPEST: usize = 4;

/// Reads every file of machines and rules below a folder: each with its name inside
/// the folder, in the order of the names.
pub fn read_folder(folder: &Path) -> Result<Vec<(String, String)>, Refused> {
    let mut files = Vec::new();
    collect(folder, folder, 0, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect(root: &Path, folder: &Path, depth: usize, files: &mut Vec<(String, String)>) -> Result<(), Refused> {
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
                collect(root, &path, depth + 1, files)?;
            }
        } else if name.ends_with(".yaml") || name.ends_with(".yml") {
            let below = path.strip_prefix(root).unwrap_or(&path).display().to_string();
            if is.len() > LARGEST {
                return Err(Refused { file: below, line: 0, why: "the file is larger than a file of the register may be".to_string() });
            }
            if files.len() >= MOST_FILES {
                return Err(Refused { file: below, line: 0, why: format!("the folder holds more than {MOST_FILES} files") });
            }
            let text = std::fs::read(&path).map_err(|error| cant("the file", &path, error))?;
            let text =
                String::from_utf8(text).map_err(|_| Refused { file: below.clone(), line: 0, why: "the file is not text".to_string() })?;
            files.push((below, text));
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

struct State {
    in_force: Arc<InForce>,
    since: u64,
    refused: Option<Refusal>,
    /// What the files held when they were last looked at, as a sum.
    seen: Option<[u8; 32]>,
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
            state: RwLock::new(State { in_force: Arc::new(InForce::default()), since: now, refused: None, seen: None }),
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

    /// Looks at the files, and applies them where they changed: whole, or not at all.
    pub fn look(&self, now: u64) {
        let Some(folder) = &self.folder else { return };
        let read = read_folder(folder);
        let seen: [u8; 32] = match &read {
            Ok(files) => {
                let mut sum = Sha256::new();
                for (name, text) in files {
                    sum.update((name.len() as u64).to_le_bytes());
                    sum.update(name);
                    sum.update((text.len() as u64).to_le_bytes());
                    sum.update(text);
                }
                sum.finalize().into()
            }
            // a folder that can't be read is another state than any that can
            Err(refused) => Sha256::digest(format!("unread: {refused}")).into(),
        };
        if self.state.read().unwrap_or_else(|poisoned| poisoned.into_inner()).seen == Some(seen) {
            return;
        }
        let applied = read.and_then(|files| InForce::read(&files).map(|in_force| (in_force, files.len())));
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
                state.in_force = Arc::new(in_force);
                state.since = now;
                state.refused = None;
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
        let files = read_folder(&folder.0).unwrap();
        let names: Vec<&str> = files.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["machines/engineering.yaml", "machines/more.yml", "rules/access.yaml"]);
        assert_eq!(files[0].1, MACHINE);
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
