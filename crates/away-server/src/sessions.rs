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
    /// When it was asked for, in seconds since 1970.
    pub at: u64,
}

/// A session the gateway took over.
#[derive(Clone, Debug)]
pub struct Running {
    /// Whose it is.
    pub person: Person,
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
        let running = Running { person: asked.person.clone(), machine: asked.machine.clone(), since: now };
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
