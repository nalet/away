//! The sign-ins the server holds: who each is, what renews it, and the sign-ins that
//! are under way.
//!
//! A browser is given a name for its sign-in and nothing else. The name is 32 random
//! bytes, so it can't be guessed, and everything it stands for stays here.
//!
//! They are held in memory: a server that starts again holds none. They move into the
//! database when Away has one.

use std::collections::HashMap;
use std::sync::Mutex;

use away_part::base64;
use away_part::json::Json;
use away_secret::Secret;
use sha2::{Digest, Sha256};

use crate::service::Signed;

/// How long a person has to come back from the sign-in service, in seconds.
const BACK_WITHIN: u64 = 600;
/// How many sign-ins may be under way at once. Beyond that the oldest is dropped.
const UNDER_WAY_AT_MOST: usize = 10_000;
/// How long before it runs out a sign-in is renewed, in seconds.
const RENEWED_BEFORE: u64 = 60;
/// How long a sign-in that couldn't be renewed, because the service didn't answer, is
/// still taken after it has run out, in seconds.
const OVERDUE_AT_MOST: u64 = 120;
/// How long a browser keeps the name of its sign-in, in seconds: thirty days. A
/// sign-in that is older than that has no browser left to name it, and is forgotten.
pub const KEPT_FOR: u64 = 30 * 24 * 3600;

/// A name nobody can guess: 32 random bytes, as letters.
pub fn random_name() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the system gives random bytes");
    base64::for_addresses(&bytes)
}

/// Who somebody is, as the sign-in service says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Person {
    /// What the service knows them by, which never changes.
    pub subject: String,
    /// Their name.
    pub name: String,
    /// Their address, where the service gives one.
    pub address: Option<String>,
    /// The groups they are in.
    pub groups: Vec<String>,
}

impl Person {
    /// Reads who somebody is from what the service signed. The groups are in the entry
    /// the operator named.
    pub fn read(says: &Json, groups_entry: &str) -> Option<Person> {
        let subject = says.text_of("sub")?.to_string();
        let address = says.text_of("email").map(str::to_string);
        let name = ["name", "preferred_username", "email"]
            .into_iter()
            .find_map(|entry| says.text_of(entry).filter(|text| !text.trim().is_empty()))
            .unwrap_or(&subject)
            .to_string();
        let groups = match says.get(groups_entry) {
            Some(Json::List(groups)) => groups.iter().filter_map(Json::text).map(str::to_string).collect(),
            Some(Json::Text(one)) => vec![one.clone()],
            _ => Vec::new(),
        };
        Some(Person { subject, name, address, groups })
    }

    /// The letters an avatar shows: the first of the first and of the last word of the
    /// name, or the first two of a name of one word.
    pub fn initials(&self) -> String {
        let words: Vec<&str> = self.name.split(|character: char| !character.is_alphanumeric()).filter(|word| !word.is_empty()).collect();
        let letters: String = match words.as_slice() {
            [] => String::new(),
            [one] => one.chars().take(2).collect(),
            [first, .., last] => first.chars().take(1).chain(last.chars().take(1)).collect(),
        };
        letters.to_uppercase()
    }
}

/// A sign-in that is under way: the person is at the sign-in service.
#[derive(Debug)]
pub struct Begun {
    /// What the browser that began it was given, and has to bring back.
    pub browser: String,
    /// The number the service has to sign.
    pub number: String,
    /// The proof that whoever ends the sign-in began it.
    pub proof: Secret,
    /// The page the person asked for.
    pub to: String,
    at: u64,
}

/// What a browser is sent off with when a sign-in begins.
#[derive(Debug)]
pub struct Beginning {
    /// What comes back with the person.
    pub state: String,
    /// What the browser keeps until it is back.
    pub browser: String,
    /// The number the service has to sign.
    pub number: String,
    /// What the proof will be checked against.
    pub challenge: String,
}

/// Why signing in failed, for the page that says so.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    /// The kind: `no-answer` or `unusable`.
    pub why: &'static str,
    /// The technical reason, for whoever is asked for help.
    pub details: String,
}

/// One sign-in.
#[derive(Debug)]
struct Held {
    person: Person,
    renewal: Option<Secret>,
    /// When the person signed in, or signed in again.
    since: u64,
    good_until: u64,
    overdue_since: Option<u64>,
    ran_out: bool,
}

#[derive(Default)]
struct All {
    held: HashMap<String, Held>,
    under_way: HashMap<String, Begun>,
    failures: HashMap<String, (Failure, u64)>,
}

/// Every sign-in the server holds.
#[derive(Default)]
pub struct SignIns {
    all: Mutex<All>,
}

impl SignIns {
    /// No sign-ins yet.
    pub fn new() -> SignIns {
        SignIns::default()
    }

    fn all(&self) -> std::sync::MutexGuard<'_, All> {
        // a thread that failed while it held the sign-ins left them as they were
        self.all.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Begins a sign-in for a browser that asked for this page.
    pub fn begin(&self, to: &str, now: u64) -> Beginning {
        let proof = random_name();
        let beginning = Beginning {
            state: random_name(),
            browser: random_name(),
            number: random_name(),
            challenge: base64::for_addresses(&Sha256::digest(proof.as_bytes())),
        };
        let mut all = self.all();
        all.under_way.retain(|_, begun| now.saturating_sub(begun.at) <= BACK_WITHIN);
        all.failures.retain(|_, (_, at)| now.saturating_sub(*at) <= BACK_WITHIN);
        if all.under_way.len() >= UNDER_WAY_AT_MOST {
            if let Some(oldest) = all.under_way.iter().min_by_key(|(_, begun)| begun.at).map(|(state, _)| state.clone()) {
                all.under_way.remove(&oldest);
            }
        }
        all.under_way.insert(
            beginning.state.clone(),
            Begun {
                browser: beginning.browser.clone(),
                number: beginning.number.clone(),
                proof: Secret::new(proof),
                to: to.to_string(),
                at: now,
            },
        );
        beginning
    }

    /// The sign-in a person comes back from, if this browser began it and not too long
    /// ago. It can be come back from once.
    pub fn back(&self, state: &str, browser: Option<&str>, now: u64) -> Option<Begun> {
        let mut all = self.all();
        let begun = all.under_way.remove(state)?;
        (Some(begun.browser.as_str()) == browser && now.saturating_sub(begun.at) <= BACK_WITHIN).then_some(begun)
    }

    /// Keeps why a browser's sign-in failed, for its next page to say.
    pub fn failed(&self, browser: &str, failure: Failure, now: u64) {
        self.all().failures.insert(browser.to_string(), (failure, now));
    }

    /// Why a browser's sign-in failed, once.
    pub fn take_failure(&self, browser: &str) -> Option<Failure> {
        self.all().failures.remove(browser).map(|(failure, _)| failure)
    }

    /// Holds a new sign-in, and gives the name the browser is to know it by.
    pub fn hold(&self, person: Person, signed: &Signed, now: u64) -> String {
        let name = random_name();
        self.all().held.insert(
            name.clone(),
            Held {
                person,
                renewal: signed.renewal.clone(),
                since: now,
                good_until: now + signed.good_for,
                overdue_since: None,
                ran_out: false,
            },
        );
        name
    }

    /// Who a sign-in is, and whether it has run out and wants signing in again.
    pub fn who(&self, name: &str, now: u64) -> Option<(Person, bool)> {
        let mut all = self.all();
        let held = all.held.get_mut(name)?;
        // one that could neither be renewed nor asked about for too long counts as run out
        let unrenewed = held.renewal.is_none() || held.overdue_since.is_some_and(|since| now.saturating_sub(since) > OVERDUE_AT_MOST);
        if now >= held.good_until && unrenewed {
            held.ran_out = true;
        }
        Some((held.person.clone(), held.ran_out))
    }

    /// Ends a sign-in, and gives what it was renewed with, for the service to be told.
    pub fn end(&self, name: &str) -> Option<(Person, Option<Secret>)> {
        self.all().held.remove(name).map(|held| (held.person, held.renewal))
    }

    /// A sign-in that has run out is good again: the same person signed in anew.
    /// Gives whether it was taken: only for the person the sign-in is of.
    pub fn again(&self, name: &str, person: Person, signed: &Signed, now: u64) -> bool {
        let mut all = self.all();
        match all.held.get_mut(name) {
            Some(held) if held.person.subject == person.subject => {
                *held = Held {
                    person,
                    renewal: signed.renewal.clone(),
                    since: now,
                    good_until: now + signed.good_for,
                    overdue_since: None,
                    ran_out: false,
                };
                true
            }
            _ => false,
        }
    }

    /// Forgets the sign-ins no browser can name any more, and gives what each was
    /// renewed with, for the service to be told.
    pub fn forget_old(&self, now: u64) -> Vec<Secret> {
        let mut all = self.all();
        let old: Vec<String> =
            all.held.iter().filter(|(_, held)| now.saturating_sub(held.since) > KEPT_FOR).map(|(name, _)| name.clone()).collect();
        old.iter().filter_map(|name| all.held.remove(name)?.renewal).collect()
    }

    /// The sign-ins that are about to run out and can be renewed, with what renews them.
    pub fn due(&self, now: u64) -> Vec<(String, Secret)> {
        self.all()
            .held
            .iter()
            .filter(|(_, held)| !held.ran_out && held.good_until <= now + RENEWED_BEFORE)
            .filter_map(|(name, held)| Some((name.clone(), held.renewal.clone()?)))
            .collect()
    }

    /// A sign-in was renewed. Where the service said who the person is once more, it is
    /// taken anew, unless it is suddenly somebody else, which ends the sign-in.
    pub fn renewed(&self, name: &str, signed: &Signed, groups_entry: &str, now: u64) {
        let mut all = self.all();
        let Some(held) = all.held.get_mut(name) else { return };
        if let Some(says) = &signed.says {
            match Person::read(says, groups_entry) {
                Some(person) if person.subject == held.person.subject => held.person = person,
                _ => {
                    held.ran_out = true;
                    held.renewal = None;
                    return;
                }
            }
        }
        if let Some(renewal) = &signed.renewal {
            held.renewal = Some(renewal.clone());
        }
        held.good_until = now + signed.good_for;
        held.overdue_since = None;
    }

    /// The service refused to renew a sign-in: it has run out.
    pub fn refused(&self, name: &str) {
        if let Some(held) = self.all().held.get_mut(name) {
            held.ran_out = true;
            held.renewal = None;
        }
    }

    /// The service didn't answer when a sign-in was to be renewed.
    pub fn unanswered(&self, name: &str, now: u64) {
        if let Some(held) = self.all().held.get_mut(name) {
            held.overdue_since.get_or_insert(now);
        }
    }

    /// What a sign-in is renewed with, where it has not run out and can be renewed.
    pub fn renewal_of(&self, name: &str) -> Option<Secret> {
        self.all().held.get(name).filter(|held| !held.ran_out).and_then(|held| held.renewal.clone())
    }

    /// How many sign-ins are held.
    pub fn count(&self) -> usize {
        self.all().held.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn says(subject: &str) -> Json {
        Json::object([
            ("sub", subject.into()),
            ("name", "Alex Urban".into()),
            ("email", "alex.urban@example.com".into()),
            ("groups", Json::List(vec!["staff".into(), "finance".into()])),
        ])
    }

    fn signed(renewal: Option<&str>, good_for: u64) -> Signed {
        Signed { says: None, renewal: renewal.map(Secret::new), good_for, token: None }
    }

    #[test]
    fn a_person_is_read_from_what_the_service_signed() {
        let person = Person::read(&says("7c1f"), "groups").unwrap();
        assert_eq!(
            person,
            Person {
                subject: "7c1f".into(),
                name: "Alex Urban".into(),
                address: Some("alex.urban@example.com".into()),
                groups: vec!["staff".into(), "finance".into()]
            }
        );
        // the groups are where the operator says they are, and nowhere else
        assert_eq!(Person::read(&says("7c1f"), "roles").unwrap().groups, Vec::<String>::new());
        // a name falls back to what else names the person
        let unnamed = Json::object([("sub", "7c1f".into()), ("preferred_username", "alex.urban".into())]);
        assert_eq!(Person::read(&unnamed, "groups").unwrap().name, "alex.urban");
        assert_eq!(Person::read(&Json::object([("sub", "7c1f".into()), ("name", " ".into())]), "groups").unwrap().name, "7c1f");
        assert_eq!(Person::read(&Json::object([("name", "Nobody".into())]), "groups"), None);
    }

    #[test]
    fn initials_are_of_the_first_and_the_last_word() {
        let named = |name: &str| Person { subject: "s".into(), name: name.into(), address: None, groups: Vec::new() }.initials();
        assert_eq!(named("Alex Urban"), "AU");
        assert_eq!(named("alex.urban@example.com"), "AC");
        assert_eq!(named("Zoë van der Berg"), "ZB");
        assert_eq!(named("alex"), "AL");
        assert_eq!(named("é"), "É");
        assert_eq!(named(" - "), "");
    }

    #[test]
    fn a_sign_in_is_come_back_from_once_by_the_browser_that_began_it_and_in_time() {
        let sign_ins = SignIns::new();
        let beginning = sign_ins.begin("/machines/fin-app-02", 1_000);
        assert_eq!(beginning.challenge.len(), 43);
        assert_ne!(beginning.state, beginning.browser);

        // another browser, which has the state from a link it was sent
        assert!(sign_ins.back(&beginning.state, Some("another"), 1_010).is_none());
        // and with that the sign-in is gone for the right browser too
        assert!(sign_ins.back(&beginning.state, Some(&beginning.browser), 1_010).is_none());

        let beginning = sign_ins.begin("/machines/fin-app-02", 1_000);
        assert!(sign_ins.back(&beginning.state, None, 1_010).is_none());

        let beginning = sign_ins.begin("/machines/fin-app-02", 1_000);
        assert!(sign_ins.back(&beginning.state, Some(&beginning.browser), 1_000 + BACK_WITHIN + 1).is_none(), "too late");

        let beginning = sign_ins.begin("/machines/fin-app-02", 1_000);
        let begun = sign_ins.back(&beginning.state, Some(&beginning.browser), 1_000 + BACK_WITHIN).unwrap();
        assert_eq!(begun.to, "/machines/fin-app-02");
        assert_eq!(begun.number, beginning.number);
        // the proof is what the challenge was made from
        assert_eq!(base64::for_addresses(&Sha256::digest(begun.proof.expose().as_bytes())), beginning.challenge);
        assert!(sign_ins.back(&beginning.state, Some(&beginning.browser), 1_020).is_none(), "once");
        assert!(sign_ins.back("never begun", Some(&beginning.browser), 1_020).is_none());
    }

    #[test]
    fn sign_ins_under_way_are_forgotten_when_they_are_old_or_too_many() {
        let sign_ins = SignIns::new();
        let old = sign_ins.begin("/", 1_000);
        sign_ins.begin("/", 1_000 + BACK_WITHIN + 1);
        assert!(sign_ins.back(&old.state, Some(&old.browser), 1_000).is_none(), "an old one is dropped when another begins");

        let first = sign_ins.begin("/", 5_000);
        for nth in 0..UNDER_WAY_AT_MOST as u64 {
            sign_ins.begin("/", 5_001 + nth / 100);
        }
        assert!(sign_ins.all().under_way.len() <= UNDER_WAY_AT_MOST);
        assert!(sign_ins.back(&first.state, Some(&first.browser), 5_010).is_none(), "the oldest gave way");
    }

    #[test]
    fn a_sign_in_is_held_renewed_and_ended() {
        let sign_ins = SignIns::new();
        let person = Person::read(&says("7c1f"), "groups").unwrap();
        let name = sign_ins.hold(person.clone(), &signed(Some("r1"), 300), 1_000);
        assert_eq!(sign_ins.who(&name, 1_100), Some((person.clone(), false)));
        assert_eq!(sign_ins.who("another name", 1_100), None);
        assert_eq!(sign_ins.count(), 1);

        // nothing is due until a minute before it runs out
        assert!(sign_ins.due(1_000 + 300 - RENEWED_BEFORE - 1).is_empty());
        let due = sign_ins.due(1_000 + 300 - RENEWED_BEFORE);
        assert_eq!(due.len(), 1);
        assert_eq!((due[0].0.as_str(), due[0].1.expose()), (name.as_str(), "r1"));

        // renewed: good for longer, renewed with something new, and in a group less
        let mut said = says("7c1f");
        if let Json::Object(held) = &mut said {
            held.retain(|(entry, _)| entry != "groups");
            held.push(("groups".to_string(), Json::List(vec!["staff".into()])));
        }
        sign_ins.renewed(
            &name,
            &Signed { says: Some(said), renewal: Some(Secret::new("r2")), good_for: 300, token: None },
            "groups",
            1_250,
        );
        assert!(sign_ins.due(1_300).is_empty());
        assert_eq!(sign_ins.who(&name, 1_300).unwrap().0.groups, ["staff"]);
        assert_eq!(sign_ins.due(1_500)[0].1.expose(), "r2");

        // a renewal that says nothing of the person keeps who they are
        sign_ins.renewed(&name, &signed(None, 300), "groups", 1_500);
        assert_eq!(sign_ins.who(&name, 1_600).unwrap().0.groups, ["staff"]);
        assert_eq!(sign_ins.due(1_750)[0].1.expose(), "r2", "and what it was renewed with");

        let (ended, renewal) = sign_ins.end(&name).unwrap();
        assert_eq!(ended.subject, "7c1f");
        assert_eq!(renewal.unwrap().expose(), "r2");
        assert_eq!(sign_ins.who(&name, 1_600), None);
        assert!(sign_ins.end(&name).is_none());
    }

    #[test]
    fn a_sign_in_runs_out_when_it_is_refused_or_can_be_renewed_no_longer() {
        let sign_ins = SignIns::new();
        let person = Person::read(&says("7c1f"), "groups").unwrap();

        // refused by the service: run out at once, and never due again
        let refused = sign_ins.hold(person.clone(), &signed(Some("r1"), 300), 1_000);
        sign_ins.refused(&refused);
        assert_eq!(sign_ins.who(&refused, 1_001), Some((person.clone(), true)));
        assert!(sign_ins.due(2_000).is_empty());

        // nothing to renew it with: run out when what was signed runs out
        let short = sign_ins.hold(person.clone(), &signed(None, 300), 1_000);
        assert!(!sign_ins.who(&short, 1_299).unwrap().1);
        assert!(sign_ins.who(&short, 1_300).unwrap().1);

        // the service doesn't answer: taken a little longer, and then no more
        let unanswered = sign_ins.hold(person.clone(), &signed(Some("r1"), 300), 1_000);
        sign_ins.unanswered(&unanswered, 1_250);
        sign_ins.unanswered(&unanswered, 1_265);
        assert!(!sign_ins.who(&unanswered, 1_300 + 60).unwrap().1);
        assert_eq!(sign_ins.due(1_300).len(), 1, "and it is still tried");
        assert!(sign_ins.who(&unanswered, 1_250 + OVERDUE_AT_MOST + 1).unwrap().1);

        // the service answers again in time: as good as before
        let late = sign_ins.hold(person.clone(), &signed(Some("r1"), 300), 1_000);
        sign_ins.unanswered(&late, 1_250);
        sign_ins.renewed(&late, &signed(Some("r2"), 300), "groups", 1_320);
        assert!(!sign_ins.who(&late, 1_600).unwrap().1);

        // renewed as somebody else: that is no renewal
        let other = sign_ins.hold(person.clone(), &signed(Some("r1"), 300), 1_000);
        sign_ins.renewed(
            &other,
            &Signed { says: Some(says("another")), renewal: Some(Secret::new("r2")), good_for: 300, token: None },
            "groups",
            1_250,
        );
        assert_eq!(sign_ins.who(&other, 1_251), Some((person, true)));
    }

    #[test]
    fn a_sign_in_that_ran_out_is_good_again_for_the_same_person_alone() {
        let sign_ins = SignIns::new();
        let person = Person::read(&says("7c1f"), "groups").unwrap();
        let name = sign_ins.hold(person.clone(), &signed(Some("r1"), 300), 1_000);
        sign_ins.refused(&name);
        let another = Person::read(&says("another"), "groups").unwrap();
        assert!(!sign_ins.again(&name, another, &signed(Some("r9"), 300), 2_000));
        assert!(sign_ins.who(&name, 2_000).unwrap().1);
        assert!(sign_ins.again(&name, person.clone(), &signed(Some("r2"), 300), 2_000));
        assert_eq!(sign_ins.who(&name, 2_100), Some((person.clone(), false)));
        assert_eq!(sign_ins.due(2_300)[0].1.expose(), "r2");
        assert!(!sign_ins.again("no such sign-in", person, &signed(None, 300), 2_000));
    }

    #[test]
    fn a_sign_in_no_browser_can_name_any_more_is_forgotten() {
        let sign_ins = SignIns::new();
        let person = Person::read(&says("7c1f"), "groups").unwrap();
        let old = sign_ins.hold(person.clone(), &signed(Some("r1"), 300), 1_000);
        let again = sign_ins.hold(person.clone(), &signed(Some("r2"), 300), 1_000);
        let ran_out = sign_ins.hold(person.clone(), &signed(None, 300), 1_000);
        // one of them signed in again on the way
        sign_ins.refused(&again);
        assert!(sign_ins.again(&again, person.clone(), &signed(Some("r3"), 300), 1_000 + KEPT_FOR));
        assert!(sign_ins.forget_old(1_000 + KEPT_FOR).is_empty(), "thirty days are not over yet");
        let told: Vec<String> = sign_ins.forget_old(1_000 + KEPT_FOR + 1).iter().map(|renewal| renewal.expose().to_string()).collect();
        assert_eq!(told, ["r1"], "what could still renew a forgotten sign-in is given back");
        assert_eq!(sign_ins.who(&old, 1_000 + KEPT_FOR + 1), None);
        assert_eq!(sign_ins.who(&ran_out, 1_000 + KEPT_FOR + 1), None);
        assert!(sign_ins.who(&again, 1_000 + KEPT_FOR + 1).is_some());
        assert_eq!(sign_ins.count(), 1);
    }

    #[test]
    fn why_a_sign_in_failed_is_said_once() {
        let sign_ins = SignIns::new();
        let failure = Failure { why: "no-answer", details: "The sign-in service: it didn't answer in time.".into() };
        sign_ins.failed("browser-1", failure.clone(), 1_000);
        assert_eq!(sign_ins.take_failure("browser-2"), None);
        assert_eq!(sign_ins.take_failure("browser-1"), Some(failure));
        assert_eq!(sign_ins.take_failure("browser-1"), None);
    }

    #[test]
    fn names_are_long_and_never_the_same() {
        let names: std::collections::HashSet<String> = (0..1_000).map(|_| random_name()).collect();
        assert_eq!(names.len(), 1_000);
        assert!(names.iter().all(|name| name.len() == 43));
    }
}
