//! The windows of the programs of one session, as the gateway knows them: the machine
//! says what changed, and the gateway keeps what is so, to tell the browser of it.

use away_rdp::window::{Change, Part, Shown};
use away_wire::Window;

/// What Windows calls a window that belongs in no list of windows: a bar, a tip, a
/// window of the system's own.
const NOT_FOR_A_LIST: u32 = 0x0000_0080;

struct Known {
    id: u32,
    owner: Option<u32>,
    further_style: u32,
    shown: Shown,
    title: String,
    seen_at: (i32, i32),
    seen: Vec<Part>,
}

/// The windows that are open, and which is in front.
#[derive(Default)]
pub struct Windows {
    known: Vec<Known>,
    /// From the front one back, as the machine last said it.
    order: Vec<u32>,
    in_front: Option<u32>,
}

impl Windows {
    /// Takes what the machine says of the windows.
    pub fn take(&mut self, changes: Vec<Change>) {
        for change in changes {
            match change {
                Change::Window(said) => {
                    let at = match self.known.iter().position(|known| known.id == said.id) {
                        Some(at) => at,
                        None => {
                            self.known.push(Known {
                                id: said.id,
                                owner: None,
                                further_style: 0,
                                shown: Shown::AsItIs,
                                title: String::new(),
                                seen_at: (0, 0),
                                seen: Vec::new(),
                            });
                            self.known.len() - 1
                        }
                    };
                    let known = &mut self.known[at];
                    // of a window only what changed is said
                    if let Some(owner) = said.owner {
                        known.owner = Some(owner).filter(|owner| *owner != 0);
                    }
                    if let Some((_, further)) = said.style {
                        known.further_style = further;
                    }
                    if let Some(shown) = said.shown {
                        known.shown = shown;
                    }
                    if let Some(title) = said.title {
                        known.title = title;
                    }
                    if let Some(seen_at) = said.seen_at {
                        known.seen_at = seen_at;
                    }
                    if let Some(seen) = said.seen {
                        known.seen = seen;
                    }
                }
                Change::Gone(id) => {
                    self.known.retain(|known| known.id != id);
                    self.order.retain(|other| *other != id);
                    self.in_front = self.in_front.filter(|front| *front != id);
                }
                Change::InFront(id) => self.in_front = Some(id).filter(|id| *id != 0),
                Change::Order(order) => self.order = order,
            }
        }
    }

    /// The windows from the front one back, as the browser is told of them, and the one
    /// in front where it is one of them.
    pub fn told(&self) -> (Vec<Window>, Option<u32>) {
        // the machine's order first, and after it the windows it has not placed yet
        let placed = self.order.iter().filter_map(|id| self.known.iter().find(|known| known.id == *id));
        let unplaced = self.known.iter().filter(|known| !self.order.contains(&known.id));
        let windows: Vec<Window> = placed
            .chain(unplaced)
            .map(|known| {
                let shows = matches!(known.shown, Shown::AsItIs | Shown::Maximized);
                let seen = known.seen.iter().filter(|_| shows).filter(|part| part.right > part.left && part.bottom > part.top);
                Window {
                    id: known.id,
                    title: known.title.clone(),
                    shown: match known.shown {
                        Shown::Hidden => "hidden",
                        Shown::Minimized => "minimized",
                        Shown::Maximized => "maximized",
                        Shown::AsItIs => "as-it-is",
                    },
                    // a window of its own: it belongs to no other, has a title, and is no bar or tip
                    listed: known.owner.is_none()
                        && !known.title.is_empty()
                        && known.further_style & NOT_FOR_A_LIST == 0
                        && !matches!(known.shown, Shown::Hidden),
                    seen: seen
                        .map(|part| {
                            let (left, top) = known.seen_at;
                            [
                                left + i32::from(part.left),
                                top + i32::from(part.top),
                                left + i32::from(part.right),
                                top + i32::from(part.bottom),
                            ]
                        })
                        .collect(),
                }
            })
            .collect();
        let in_front = self.in_front.filter(|front| windows.iter().any(|window| window.id == *front));
        (windows, in_front)
    }

    /// How many windows of their own are open: those a list of windows names.
    pub fn listed(&self) -> usize {
        self.told().0.iter().filter(|window| window.listed).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use away_rdp::window::Window as Said;

    fn said(id: u32, title: &str, at: (i32, i32), size: (u16, u16)) -> Said {
        Said {
            id,
            is_new: true,
            style: Some((0x14cf_0000, 0x100)),
            shown: Some(Shown::AsItIs),
            title: Some(title.to_string()),
            seen_at: Some(at),
            seen: Some(vec![Part { left: 0, top: 0, right: size.0, bottom: size.1 }]),
            ..Said::default()
        }
    }

    #[test]
    fn the_windows_are_kept_as_the_machine_says_them_and_told_from_the_front_one_back() {
        let mut windows = Windows::default();
        windows.take(vec![
            Change::Window(said(1, "Ledger", (20, 10), (160, 100))),
            Change::Window(said(2, "Payroll", (-8, 30), (100, 50))),
            Change::InFront(2),
            Change::Order(vec![2, 1]),
        ]);
        let (told, in_front) = windows.told();
        assert_eq!(in_front, Some(2));
        assert_eq!(
            told,
            [
                Window { id: 2, title: "Payroll".to_string(), shown: "as-it-is", listed: true, seen: vec![[-8, 30, 92, 80]] },
                Window { id: 1, title: "Ledger".to_string(), shown: "as-it-is", listed: true, seen: vec![[20, 10, 180, 110]] },
            ]
        );
        assert_eq!(windows.listed(), 2);

        // of a window only what changed is said: it moved, and the other is partly behind it
        windows.take(vec![
            Change::Window(Said { id: 2, seen_at: Some((0, 0)), ..Said::default() }),
            Change::Window(Said {
                id: 1,
                seen: Some(vec![Part { left: 80, top: 0, right: 160, bottom: 100 }, Part { left: 0, top: 0, right: 0, bottom: 100 }]),
                ..Said::default()
            }),
        ]);
        let (told, _) = windows.told();
        assert_eq!((told[0].seen.clone(), told[0].title.as_str()), (vec![[0, 0, 100, 50]], "Payroll"));
        // a part of no width is none
        assert_eq!(told[1].seen, [[100, 10, 180, 110]]);

        // a dialogue of a window, a tip and a window without a title are in no list, and are shown
        windows.take(vec![
            Change::Window(Said { owner: Some(1), ..said(3, "Save as", (40, 40), (60, 30)) }),
            Change::Window(Said { style: Some((0, 0x88)), ..said(4, "a tip", (0, 0), (10, 10)) }),
            Change::Window(said(5, "", (0, 0), (10, 10))),
        ]);
        let (told, _) = windows.told();
        assert_eq!(
            told.iter().map(|window| (window.id, window.listed, window.seen.len())).collect::<Vec<_>>(),
            [(2, true, 1), (1, true, 1), (3, false, 1), (4, false, 1), (5, false, 1)]
        );

        // one that is made small shows nothing and stays in the list; one that is hidden is in none
        windows.take(vec![
            Change::Window(Said { id: 2, shown: Some(Shown::Minimized), ..Said::default() }),
            Change::Window(Said { id: 1, shown: Some(Shown::Hidden), ..Said::default() }),
        ]);
        let (told, _) = windows.told();
        assert_eq!((told[0].shown, told[0].listed, told[0].seen.len()), ("minimized", true, 0));
        assert_eq!((told[1].shown, told[1].listed, told[1].seen.len()), ("hidden", false, 0));
        assert_eq!(windows.listed(), 1);

        // a window that is gone is gone from everything, and with the front one nothing is in front
        windows.take(vec![Change::Gone(2), Change::Gone(9)]);
        let (told, in_front) = windows.told();
        assert_eq!((told.iter().map(|window| window.id).collect::<Vec<_>>(), in_front), (vec![1, 3, 4, 5], None));
        // no window in front is said as the window nought
        windows.take(vec![Change::InFront(1)]);
        assert_eq!(windows.told().1, Some(1));
        windows.take(vec![Change::InFront(0)]);
        assert_eq!(windows.told().1, None);
    }
}
