//! Sessions, on the server's side: who asked to open which machine, the ticket the
//! gateway takes a session over with, and the sessions that run.
//!
//! The server decides whether a person may open a machine and writes the record before
//! anything is opened. The gateway opens it, and tells the server what became of it
//! ([ADR-0003](../../../spec/architecture/decisions/0003-three-parts-with-their-own-rights.md)).

use std::collections::HashMap;
use std::sync::Mutex;

use crate::sign_ins::Person;

/// How long a ticket is good for, in seconds: the time a page takes to open its
/// connection to the gateway.
pub const TICKET_GOOD_FOR: u64 = 60;

/// A session somebody asked for, until the gateway takes it over.
#[derive(Clone, Debug)]
pub struct Asked {
    /// What names the session: the key of the record that it was asked for.
    pub session: String,
    /// Who asked.
    pub person: Person,
    /// The sign-in of the browser that asked: only that browser's connection takes it over.
    pub sign_in: String,
    /// The machine, by its name.
    pub machine: String,
    /// Where the machine is reached.
    pub address: String,
    /// On which port.
    pub port: u16,
    /// What the machine is reached over: RDP, or VNC for a desktop that speaks that.
    pub over: crate::register::Over,
    /// The collection of a farm that the machine stands for, where it stands for one.
    pub collection: Option<String>,
    /// The machines that serve the collection, by their addresses.
    pub served_by: Vec<String>,
    /// Whether the person is signed in on the machine as themselves, with nothing typed.
    pub themselves: bool,
    /// Whether the direct desktop requires browser Restricted Admin.
    pub restricted_admin: bool,
    /// The secret kept for the machine that signs the person in, by its name, where one does.
    pub kept: Option<String>,
    /// Which ways the clipboard passes on the machine.
    pub clipboard: crate::register::Clipboard,
    /// The program the session is for, where it is for one and not for the desktop: its
    /// name, and what the machine is asked to start.
    pub program: Option<(String, String)>,
    /// When it was asked for, in seconds since 1970.
    pub at: u64,
}

/// How the person of a session is signed in on its machine, as the parts of Away say it.
pub fn way_of_signing_in(restricted_admin: bool, themselves: bool, kept: Option<&str>) -> &'static str {
    if restricted_admin {
        "restricted-admin-sso"
    } else if themselves {
        "themselves"
    } else if kept.is_some() {
        "kept"
    } else {
        "typed"
    }
}

/// A session the gateway took over.
#[derive(Clone, Debug)]
pub struct Running {
    /// The destination and mode fixed when the session was admitted.
    pub address: String,
    /// The original RDP port.
    pub port: u16,
    /// The allowed collection hosts.
    pub served_by: Vec<String>,
    /// The collection originally requested.
    pub collection: Option<String>,
    /// The program and its command, if any.
    pub program: Option<(String, String)>,
    /// Whether this session may request a sign-in certificate.
    pub themselves: bool,
    /// Whether fresh authorization is required for browser Restricted Admin.
    pub restricted_admin: bool,
    /// The secret kept for the machine that signs the person in, by its name, where one does.
    pub kept: Option<String>,
    /// Whose it is.
    pub person: Person,
    /// The sign-in of the browser that asked for it.
    pub sign_in: String,
    /// The machine, by its name.
    pub machine: String,
    /// Since when, in seconds since 1970.
    pub since: u64,
}

/// The sessions the server knows of.
#[derive(Default)]
pub struct Sessions {
    asked: Mutex<HashMap<String, Asked>>,
    running: Mutex<HashMap<String, Running>>,
}

impl Sessions {
    /// Keeps a session that was asked for, and gives the ticket for it: random, good
    /// once and for a short time.
    pub fn ask(&self, asked: Asked) -> Result<String, String> {
        let mut random = [0u8; 32];
        getrandom::fill(&mut random).map_err(|error| format!("the system gave no random bytes: {error}"))?;
        let ticket: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        let mut all = self.asked.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        // tickets nobody came for are forgotten
        let now = asked.at;
        all.retain(|_, asked| now.saturating_sub(asked.at) <= TICKET_GOOD_FOR);
        all.insert(ticket.clone(), asked);
        Ok(ticket)
    }

    /// Takes a session over with its ticket, once: only for the sign-in that asked for
    /// it, and only while the ticket is good.
    pub fn take(&self, ticket: &str, sign_in: &str, now: u64) -> Option<Asked> {
        let asked = {
            let mut all = self.asked.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            // a ticket is spent by whoever shows it, whether it is theirs or not
            all.remove(ticket)?
        };
        if asked.sign_in != sign_in || now.saturating_sub(asked.at) > TICKET_GOOD_FOR {
            return None;
        }
        let running = Running {
            address: asked.address.clone(),
            port: asked.port,
            served_by: asked.served_by.clone(),
            collection: asked.collection.clone(),
            program: asked.program.clone(),
            themselves: asked.themselves,
            restricted_admin: asked.restricted_admin,
            kept: asked.kept.clone(),
            person: asked.person.clone(),
            sign_in: asked.sign_in.clone(),
            machine: asked.machine.clone(),
            since: now,
        };
        self.running.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(asked.session.clone(), running);
        Some(asked)
    }

    /// A session that runs.
    pub fn running(&self, session: &str) -> Option<Running> {
        self.running.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get(session).cloned()
    }

    /// Takes a session off the ones that run, as it ends.
    pub fn end(&self, session: &str) -> Option<Running> {
        self.running.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(session)
    }

    /// How many sessions run.
    pub fn count(&self) -> usize {
        self.running.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asked(at: u64) -> Asked {
        Asked {
            session: "5e55".to_string(),
            person: Person { subject: "7c1f".to_string(), name: "Alex Urban".to_string(), address: None, groups: Vec::new() },
            sign_in: "the browser's sign-in".to_string(),
            machine: "fin-app-02".to_string(),
            address: "fin-app-02.corp.example".to_string(),
            port: 3389,
            over: crate::register::Over::Rdp,
            collection: None,
            served_by: Vec::new(),
            themselves: false,
            restricted_admin: false,
            kept: None,
            clipboard: crate::register::Clipboard::BOTH,
            program: None,
            at,
        }
    }

    #[test]
    fn a_ticket_is_good_once_for_the_sign_in_that_asked_and_for_a_short_time() {
        let sessions = Sessions::default();
        let ticket = sessions.ask(asked(1000)).unwrap();
        assert_eq!(ticket.len(), 64);
        assert_ne!(ticket, sessions.ask(asked(1000)).unwrap());
        // another browser's sign-in doesn't take it, and spends it
        assert!(sessions.take(&ticket, "another sign-in", 1001).is_none());
        assert!(sessions.take(&ticket, "the browser's sign-in", 1001).is_none());

        let ticket = sessions.ask(asked(1000)).unwrap();
        assert!(sessions.take("no ticket", "the browser's sign-in", 1001).is_none());
        let taken = sessions.take(&ticket, "the browser's sign-in", 1001).expect("its own sign-in takes it");
        assert_eq!((taken.machine.as_str(), taken.port), ("fin-app-02", 3389));
        // and only once
        assert!(sessions.take(&ticket, "the browser's sign-in", 1002).is_none());
        assert_eq!(sessions.running("5e55").map(|running| running.since), Some(1001));
        assert_eq!(sessions.count(), 1);
        assert!(sessions.end("5e55").is_some());
        assert!(sessions.end("5e55").is_none());

        // a ticket nobody came for in time is worth nothing
        let late = sessions.ask(asked(2000)).unwrap();
        assert!(sessions.take(&late, "the browser's sign-in", 2000 + TICKET_GOOD_FOR + 1).is_none());
    }
}
