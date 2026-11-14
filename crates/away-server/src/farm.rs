//! A farm of Windows machines, as Away learns of it by itself: the server asks the farm's
//! own feed what it publishes, and offers that
//! ([ADR-0014](../../../spec/architecture/decisions/0014-a-farm-says-what-it-publishes.md)).
//!
//! A farm's desktops become machines of the register, each by its own name. Its programs
//! become the programs of one machine that has the farm's name. All of them are in the
//! farm's groups, so the rules give them like everything else.

pub mod feed;
pub mod reader;
pub mod xml;

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::register::{picture_name, Farm, Kind, Machine, Over, Program};

/// One thing read from a farm: what the feed lists, how it is reached, and its icon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// What the feed lists.
    pub resource: feed::Resource,
    /// How it is reached, as its file says.
    pub reached: feed::Reached,
    /// Its icon: a PNG picture.
    pub icon: Option<Arc<Vec<u8>>>,
}

/// What a farm publishes, as the register takes it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Published {
    /// Its desktops, each a machine, and one machine more that holds its programs.
    pub machines: Vec<Machine>,
    /// The icons, each by the name it is asked for by.
    pub pictures: BTreeMap<String, Arc<Vec<u8>>>,
    /// How many desktops it publishes.
    pub desktops: usize,
    /// How many programs.
    pub programs: usize,
}

/// A name as a machine's name takes it: what a name doesn't take becomes a dash.
fn as_a_name(title: &str) -> String {
    title.chars().map(|character| if character.is_alphanumeric() || " -_.()+&".contains(character) { character } else { '-' }).collect()
}

/// Turns what was read from a farm into machines of the register. `host` is the farm's
/// own, for what names no address.
pub fn publish(farm: &Farm, host: &str, items: &[Item]) -> Published {
    let mut published = Published::default();
    let mut programs: Vec<Program> = Vec::new();
    let mut programs_at: Option<(String, Option<String>)> = None;
    for item in items {
        let address = item.reached.address.clone().unwrap_or_else(|| host.to_string());
        if item.resource.desktop {
            published.desktops += 1;
            published.machines.push(Machine {
                name: as_a_name(&item.resource.title),
                kind: Kind::Windows,
                address,
                groups: farm.groups.clone(),
                desktop: Some((Over::Rdp, 3389)),
                programs: Vec::new(),
                shell: None,
                collection: item.reached.collection.clone(),
                served_by: farm.served_by.clone(),
                farm: Some(farm.name.clone()),
                written_in: farm.written_in.clone(),
            });
        } else {
            published.programs += 1;
            programs_at.get_or_insert((address, item.reached.collection.clone()));
            let icon = item.icon.as_ref().map(|picture| {
                let name = picture_name(picture);
                published.pictures.entry(name.clone()).or_insert_with(|| picture.clone());
                name
            });
            // the farm starts it as what its file names, or as its title
            let run = format!("||{}", item.reached.program.as_deref().unwrap_or(&item.resource.title));
            programs.push(Program { name: item.resource.title.clone(), icon, run });
        }
    }
    // the programs are those of one machine, which has the farm's name
    if let Some((address, collection)) = programs_at {
        published.machines.push(Machine {
            name: farm.name.clone(),
            kind: Kind::Windows,
            address,
            groups: farm.groups.clone(),
            desktop: None,
            programs,
            shell: None,
            collection,
            served_by: farm.served_by.clone(),
            farm: Some(farm.name.clone()),
            written_in: farm.written_in.clone(),
        });
    }
    published
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(title: &str, desktop: bool, collection: &str, icon: Option<&[u8]>) -> Item {
        Item {
            resource: feed::Resource {
                title: title.to_string(),
                alias: title.to_lowercase(),
                desktop,
                changed: String::new(),
                icon: None,
                file: None,
            },
            reached: feed::Reached {
                address: Some("broker.corp.example".to_string()),
                collection: Some(collection.to_string()),
                program: None,
            },
            icon: icon.map(|bytes| Arc::new(bytes.to_vec())),
        }
    }

    #[test]
    fn a_farms_desktops_are_machines_and_its_programs_those_of_one_machine_with_its_name() {
        let farm = Farm {
            name: "Work Resources".to_string(),
            feed: "https://broker.corp.example/RDWeb/Feed/webfeed.aspx".to_string(),
            groups: vec!["finance".to_string()],
            served_by: vec!["rdsh-01.corp.example".to_string()],
            after: 0,
            written_in: ("farms.yaml".to_string(), 2),
        };
        let items = [
            item("Ledger", false, "Finance", Some(b"\x89PNG\r\n\x1a\none")),
            item("Desktop: Finance", true, "Desktops", Some(b"\x89PNG\r\n\x1a\ntwo")),
            item("Pay & Benefits", false, "Finance", None),
            item("Reports", false, "Finance", Some(b"\x89PNG\r\n\x1a\none")),
        ];
        let published = publish(&farm, "broker.corp.example", &items);
        assert_eq!((published.desktops, published.programs, published.machines.len()), (1, 3, 2));
        let desktop = &published.machines[0];
        // a name takes no colon: it becomes a dash
        assert_eq!(
            (desktop.name.as_str(), desktop.desktop, desktop.collection.as_deref()),
            ("Desktop- Finance", Some((Over::Rdp, 3389)), Some("Desktops"))
        );
        assert_eq!(
            (desktop.farm.as_deref(), &desktop.groups[..], desktop.written_in.clone()),
            (Some("Work Resources"), &["finance".to_string()][..], ("farms.yaml".to_string(), 2))
        );
        let programs = &published.machines[1];
        assert_eq!(
            (programs.name.as_str(), programs.desktop, programs.collection.as_deref(), programs.address.as_str()),
            ("Work Resources", None, Some("Finance"), "broker.corp.example")
        );
        assert_eq!(
            programs.programs.iter().map(|program| program.name.as_str()).collect::<Vec<_>>(),
            ["Ledger", "Pay & Benefits", "Reports"]
        );
        // a picture two programs share is held once; the desktop's own is not a program's
        assert_eq!(programs.programs[0].icon, programs.programs[2].icon);
        assert_eq!((programs.programs[1].icon.as_ref(), published.pictures.len()), (None, 1));
        assert!(desktop.is_reached_as("Work Resources") && desktop.is_reached_as("finance") && programs.is_reached_as("Work Resources"));
        // a farm that publishes nothing gives nothing
        assert_eq!(publish(&farm, "broker.corp.example", &[]), Published::default());
    }
}
