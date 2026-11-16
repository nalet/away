//! The windows of programs, as the machine tells of them along with what it draws
//! (MS-RDPERP 2.2.1.3): a window that is new or changed, one that is gone, which
//! window is in front, and the icons of the taskbar's notification area, the tray. The machine draws every window where it is on its desktop; these
//! words say where that is, and which parts of it can be seen.

use crate::Error;

const WRONG: Error = Error::NotRdp("the machine's word on a window is not built as one");

/// A part of a window that can be seen: its edges, counted from where the window's
/// seen parts begin. The right and the bottom edge are the first that are outside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Part {
    /// Its left edge.
    pub left: u16,
    /// Its top edge.
    pub top: u16,
    /// The first column to the right of it.
    pub right: u16,
    /// The first row below it.
    pub bottom: u16,
}

/// How a window shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shown {
    /// Not at all.
    Hidden,
    /// Made small.
    Minimized,
    /// As large as it gets.
    Maximized,
    /// At the size and the place it has.
    AsItIs,
}

/// What the machine says of one window: the window, and of it only what changed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Window {
    /// What names the window on the machine.
    pub id: u32,
    /// Whether the window is new.
    pub is_new: bool,
    /// The window it belongs to, where it is another window's: a dialogue, a menu.
    pub owner: Option<u32>,
    /// What kind of window it is, as Windows says it: its style and its further style.
    pub style: Option<(u32, u32)>,
    /// How it shows.
    pub shown: Option<Shown>,
    /// Its title.
    pub title: Option<String>,
    /// Where it is on the machine's desktop: its left and its top edge.
    pub at: Option<(i32, i32)>,
    /// How wide and how high it is.
    pub size: Option<(u32, u32)>,
    /// Where its seen parts begin on the machine's desktop.
    pub seen_at: Option<(i32, i32)>,
    /// The parts of it that can be seen.
    pub seen: Option<Vec<Part>>,
}

/// A window's icon, as the machine draws it in a taskbar: how wide and how high it is,
/// and its pixels from the top row down, four bytes each: red, green, blue, and how much
/// of what is behind it they cover.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Icon {
    /// How wide it is, in pixels.
    pub width: u16,
    /// How high.
    pub height: u16,
    /// Its pixels.
    pub pixels: Vec<u8>,
}

/// Where the machine keeps an icon to name it again later: its cache, and the place in it.
pub type Kept = (u8, u16);

/// One thing the machine says of the windows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// A window is new, or something of it changed.
    Window(Window),
    /// A window is gone.
    Gone(u32),
    /// Which window is in front: the one the person works in, or none.
    InFront(u32),
    /// The windows from the front one to the one furthest back.
    Order(Vec<u32>),
    /// A window's icon: the large one a taskbar shows, or the small one, and where the
    /// machine keeps it to name it again.
    Icon {
        /// The window.
        id: u32,
        /// Whether it is the large one.
        large: bool,
        /// The icon.
        icon: Icon,
        /// Where the machine keeps it, where it does.
        kept: Option<Kept>,
    },
    /// A window's icon is one the machine sent before, and keeps.
    KeptIcon {
        /// The window.
        id: u32,
        /// Whether it is the large one.
        large: bool,
        /// Where the machine keeps it.
        kept: Kept,
    },
    /// An icon of the tray is new, or something of it changed.
    Tray(TrayIcon),
    /// An icon of the tray is gone.
    TrayGone {
        /// The window the icon is of.
        window: u32,
        /// The icon, among those of its window.
        icon: u32,
    },
}

/// What the machine says of one icon of the tray: the icon, and of it only what changed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrayIcon {
    /// The window of the program the icon is of, which is told when the person uses it.
    pub window: u32,
    /// The icon, among those of its window.
    pub icon: u32,
    /// Whether it is new.
    pub is_new: bool,
    /// Which version of the tray's ways the program speaks: from 3 on it is told of a
    /// choice, and not of the buttons alone.
    pub version: Option<u32>,
    /// What it says when the pointer rests on it.
    pub tip: Option<String>,
    /// Whether the program hides it.
    pub hidden: Option<bool>,
    /// Its picture, and where the machine keeps it, where it does.
    pub picture: Option<(Icon, Option<Kept>)>,
    /// Its picture is one the machine sent before, and keeps.
    pub kept_picture: Option<Kept>,
}

const OF_A_WINDOW: u32 = 0x0100_0000;
const OF_AN_ICON_IN_THE_BAR: u32 = 0x0200_0000;
const OF_THE_DESKTOP: u32 = 0x0400_0000;
const NEW: u32 = 0x1000_0000;
const GONE: u32 = 0x2000_0000;
const ITS_ICON: u32 = 0x4000_0000;
const ITS_KEPT_ICON: u32 = 0x8000_0000;
/// Of a window's icon: the large one, which a taskbar shows.
const LARGE: u32 = 0x0000_2000;
/// The largest icon Away takes, each way: Windows draws them at 256 at most.
const LARGEST_ICON: u16 = 256;

struct Reader<'a> {
    rest: &'a [u8],
}

impl<'a> Reader<'a> {
    fn bytes(&mut self, count: usize) -> Result<&'a [u8], Error> {
        let (bytes, rest) = self.rest.split_at_checked(count).ok_or(WRONG)?;
        self.rest = rest;
        Ok(bytes)
    }

    fn byte(&mut self) -> Result<u8, Error> {
        Ok(self.bytes(1)?[0])
    }

    fn number16(&mut self) -> Result<u16, Error> {
        self.bytes(2).map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
    }

    fn number32(&mut self) -> Result<u32, Error> {
        self.bytes(4).map(|four| u32::from_le_bytes([four[0], four[1], four[2], four[3]]))
    }

    fn pair(&mut self) -> Result<(i32, i32), Error> {
        Ok((self.number32()? as i32, self.number32()? as i32))
    }

    /// A text with its length in bytes before it.
    fn text(&mut self) -> Result<String, Error> {
        let length = usize::from(self.number16()?);
        let units: Vec<u16> = self.bytes(length)?.as_chunks::<2>().0.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
        Ok(String::from_utf16_lossy(&units).trim_end_matches('\0').to_string())
    }

    fn parts(&mut self) -> Result<Vec<Part>, Error> {
        let count = self.number16()?;
        (0..count)
            .map(|_| Ok(Part { left: self.number16()?, top: self.number16()?, right: self.number16()?, bottom: self.number16()? }))
            .collect()
    }
}

/// Reads the machine's words on windows out of an update that carries instructions: how
/// many there are, and then each. Away asks for no drawing from instructions, so words
/// on windows are all such an update carries; anything else in it ends the reading.
pub fn read(count: u16, mut orders: &[u8]) -> Result<Vec<Change>, Error> {
    let mut changes = Vec::new();
    for _ in 0..count {
        // a word on a window: its kind in the first byte, then how long all of it is
        let [0x2e, low, high, ..] = orders else { break };
        let length = usize::from(u16::from_le_bytes([*low, *high]));
        let (order, rest) = orders.split_at_checked(length).filter(|_| length >= 7).ok_or(WRONG)?;
        orders = rest;
        let mut reader = Reader { rest: &order[3..] };
        let has = reader.number32()?;
        if has & OF_THE_DESKTOP != 0 {
            // a desktop that is not watched says nothing of windows
            if has & 0x01 != 0 {
                continue;
            }
            if has & 0x20 != 0 {
                changes.push(Change::InFront(reader.number32()?));
            }
            if has & 0x10 != 0 {
                let count = reader.byte()?;
                changes.push(Change::Order((0..count).map(|_| reader.number32()).collect::<Result<_, _>>()?));
            }
        } else if has & OF_AN_ICON_IN_THE_BAR != 0 && has & OF_A_WINDOW == 0 {
            let (window, icon) = (reader.number32()?, reader.number32()?);
            if has & GONE != 0 {
                changes.push(Change::TrayGone { window, icon });
                continue;
            }
            changes.push(Change::Tray(tray_icon(window, icon, has, &mut reader)?));
        } else if has & OF_A_WINDOW != 0 && has & OF_AN_ICON_IN_THE_BAR == 0 {
            let id = reader.number32()?;
            let large = has & LARGE != 0;
            if has & ITS_ICON != 0 {
                // an icon that is not built as Away reads one is passed over, as the rest
                // of what is said of the window is not about the icon
                if let Some((icon, kept)) = icon(&mut reader) {
                    changes.push(Change::Icon { id, large, icon, kept });
                }
            } else if has & ITS_KEPT_ICON != 0 {
                let entry = reader.number16()?;
                let cache = reader.byte()?;
                changes.push(Change::KeptIcon { id, large, kept: (cache, entry) });
            } else if has & GONE != 0 {
                changes.push(Change::Gone(id));
            } else {
                changes.push(Change::Window(window(id, has, &mut reader)?));
            }
        }
    }
    Ok(changes)
}

/// What is said of an icon of the tray that is new or changed (MS-RDPERP 2.2.1.3.2.2.1),
/// each part where the word says it is there.
fn tray_icon(window: u32, icon_of_it: u32, has: u32, reader: &mut Reader) -> Result<TrayIcon, Error> {
    let mut said = TrayIcon { window, icon: icon_of_it, is_new: has & NEW != 0, ..TrayIcon::default() };
    if has & 0x0000_0008 != 0 {
        said.version = Some(reader.number32()?);
    }
    if has & 0x0000_0001 != 0 {
        said.tip = Some(reader.text()?);
    }
    // a message that pops up from it for a while: how long, what kind, its text and its title
    if has & 0x0000_0002 != 0 {
        reader.bytes(8)?;
        reader.text()?;
        reader.text()?;
    }
    if has & 0x0000_0004 != 0 {
        said.hidden = Some(reader.number32()? & 1 != 0);
    }
    if has & ITS_ICON != 0 {
        said.picture = icon(reader);
    } else if has & ITS_KEPT_ICON != 0 {
        let entry = reader.number16()?;
        said.kept_picture = Some((reader.byte()?, entry));
    }
    Ok(said)
}

/// What is said of a window that is new or changed, each part where the word says it is there.
fn window(id: u32, has: u32, reader: &mut Reader) -> Result<Window, Error> {
    let mut window = Window { id, is_new: has & NEW != 0, ..Window::default() };
    if has & 0x0000_0002 != 0 {
        window.owner = Some(reader.number32()?);
    }
    if has & 0x0000_0008 != 0 {
        window.style = Some((reader.number32()?, reader.number32()?));
    }
    if has & 0x0000_0010 != 0 {
        window.shown = Some(match reader.byte()? {
            0x00 => Shown::Hidden,
            0x02 => Shown::Minimized,
            0x03 => Shown::Maximized,
            _ => Shown::AsItIs,
        });
    }
    if has & 0x0000_0004 != 0 {
        window.title = Some(reader.text()?);
    }
    // where what the program itself draws begins and how large it is, and the margins to take hold of
    for (flag, bytes) in [(0x0000_4000, 8), (0x0001_0000, 8), (0x0000_0080, 8), (0x0800_0000, 8), (0x0002_0000, 1), (0x0004_0000, 4)] {
        if has & flag != 0 {
            reader.bytes(bytes)?;
        }
    }
    if has & 0x0000_0800 != 0 {
        window.at = Some(reader.pair()?);
    }
    if has & 0x0000_8000 != 0 {
        reader.bytes(8)?;
    }
    if has & 0x0000_0400 != 0 {
        window.size = Some((reader.number32()?, reader.number32()?));
    }
    // the window's own shape, which the seen parts say again
    if has & 0x0000_0100 != 0 {
        reader.parts()?;
    }
    if has & 0x0000_1000 != 0 {
        window.seen_at = Some(reader.pair()?);
    }
    if has & 0x0000_0200 != 0 {
        window.seen = Some(reader.parts()?);
    }
    // what follows is for a bar of the client's, which Away has none of
    Ok(window)
}

/// A window's icon as the machine writes it (MS-RDPERP 2.2.1.2.3): where it keeps it,
/// how many bits a pixel, its size, then a mask of one bit a pixel that says which
/// pixels are not drawn, the colours of an icon of few bits, and the pixels; each kind of
/// rows from the bottom one up. Nothing for an icon of no size, of a depth Windows doesn't
/// draw icons in, or whose parts are shorter than they say.
fn icon(reader: &mut Reader) -> Option<(Icon, Option<Kept>)> {
    let entry = reader.number16().ok()?;
    let cache = reader.byte().ok()?;
    let bits = reader.byte().ok()?;
    let (width, height) = (reader.number16().ok()?, reader.number16().ok()?);
    let table = if matches!(bits, 1 | 4 | 8) { reader.number16().ok()? } else { 0 };
    let (mask, colour) = (reader.number16().ok()?, reader.number16().ok()?);
    let mask = reader.bytes(usize::from(mask)).ok()?;
    let table = reader.bytes(usize::from(table)).ok()?;
    let colour = reader.bytes(usize::from(colour)).ok()?;
    if width == 0 || height == 0 || width > LARGEST_ICON || height > LARGEST_ICON || !matches!(bits, 1 | 4 | 8 | 16 | 24 | 32) {
        return None;
    }
    let (w, h) = (usize::from(width), usize::from(height));
    // each row is as long as its part divided by the rows: rows are padded as the machine pads them
    let colour_row = colour.len() / h;
    let mask_row = mask.len() / h;
    if colour_row < (w * usize::from(bits)).div_ceil(8) || (!mask.is_empty() && mask_row < w.div_ceil(8)) {
        return None;
    }
    let from_table = |index: usize| -> Option<[u8; 3]> {
        let entry = table.get(index * 4..index * 4 + 4)?;
        Some([entry[2], entry[1], entry[0]])
    };
    let mut pixels = Vec::with_capacity(w * h * 4);
    let mut has_alpha = false;
    for y in 0..h {
        // the rows are written from the bottom one up
        let row = &colour[(h - 1 - y) * colour_row..][..colour_row];
        for x in 0..w {
            let (rgb, alpha) = match bits {
                32 => {
                    let p = &row[x * 4..x * 4 + 4];
                    has_alpha |= p[3] != 0;
                    ([p[2], p[1], p[0]], p[3])
                }
                24 => {
                    let p = &row[x * 3..x * 3 + 3];
                    ([p[2], p[1], p[0]], 0xff)
                }
                16 => {
                    // five bits for each of red, green and blue
                    let v = u16::from_le_bytes([row[x * 2], row[x * 2 + 1]]);
                    let five = |shift: u16| ((((v >> shift) & 0x1f) as u32 * 255 + 15) / 31) as u8;
                    ([five(10), five(5), five(0)], 0xff)
                }
                _ => {
                    let per_byte = 8 / usize::from(bits);
                    let byte = row[x / per_byte];
                    let shift = 8 - usize::from(bits) * (x % per_byte + 1);
                    let index = usize::from((byte >> shift) & ((1u16 << bits) - 1) as u8);
                    (from_table(index)?, 0xff)
                }
            };
            pixels.extend(rgb);
            pixels.push(alpha);
        }
    }
    // where the icon says nothing of how much each pixel covers, its mask says which it covers
    if !has_alpha {
        for y in 0..h {
            for x in 0..w {
                let drawn = match mask.is_empty() {
                    true => true,
                    false => mask[(h - 1 - y) * mask_row + x / 8] & (0x80 >> (x % 8)) == 0,
                };
                pixels[(y * w + x) * 4 + 3] = if drawn { 0xff } else { 0 };
            }
        }
    }
    let kept = (entry != 0xffff).then_some((cache, entry));
    Some((Icon { width, height, pixels }, kept))
}

/// The machine's side of these words, for the tests' stand-in for one.
#[cfg(any(test, feature = "stand-in"))]
pub mod stand_in {
    use super::*;

    fn order(has: u32, holds: &[u8]) -> Vec<u8> {
        let mut order = vec![0x2e];
        order.extend(((holds.len() + 7) as u16).to_le_bytes());
        order.extend(has.to_le_bytes());
        order.extend_from_slice(holds);
        order
    }

    /// A window that is new, or of which everything is said again: its title, where it
    /// is, how large it is, and that all of it can be seen.
    pub fn window(id: u32, is_new: bool, title: &str, at: (i32, i32), size: (u16, u16)) -> Vec<u8> {
        let title: Vec<u8> = title.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut holds = id.to_le_bytes().to_vec();
        // an ordinary window with a title, shown as it is
        holds.extend(0x14cf_0000u32.to_le_bytes());
        holds.extend(0x0000_0100u32.to_le_bytes());
        holds.push(5);
        holds.extend((title.len() as u16).to_le_bytes());
        holds.extend(title);
        for number in [at.0, at.1] {
            holds.extend(number.to_le_bytes());
        }
        for number in [size.0, size.1] {
            holds.extend(u32::from(number).to_le_bytes());
        }
        for number in [at.0, at.1] {
            holds.extend(number.to_le_bytes());
        }
        holds.extend(1u16.to_le_bytes());
        for number in [0, 0, size.0, size.1] {
            holds.extend(number.to_le_bytes());
        }
        let has = OF_A_WINDOW | if is_new { NEW } else { 0 } | 0x08 | 0x10 | 0x04 | 0x0800 | 0x0400 | 0x1000 | 0x0200;
        order(has, &holds)
    }

    /// A window is gone.
    pub fn gone(id: u32) -> Vec<u8> {
        order(OF_A_WINDOW | GONE, &id.to_le_bytes())
    }

    /// Which window is in front, and the windows from the front one back.
    pub fn in_front(front: u32, windows: &[u32]) -> Vec<u8> {
        let mut holds = front.to_le_bytes().to_vec();
        holds.push(windows.len() as u8);
        holds.extend(windows.iter().flat_map(|window| window.to_le_bytes()));
        order(OF_THE_DESKTOP | 0x20 | 0x10, &holds)
    }

    /// The icon of a window, the large one or the small one, kept by the client where it
    /// says: of this size, its pixels as red, green, blue and how much each covers, from
    /// the top row down. It travels as a Windows machine writes one of 32 bits a pixel,
    /// with a mask that hides nothing.
    pub fn icon(id: u32, large: bool, kept: Option<Kept>, icon: &Icon) -> Vec<u8> {
        let (cache, entry) = kept.unwrap_or((0, 0xffff));
        let (w, h) = (usize::from(icon.width), usize::from(icon.height));
        let mut holds = id.to_le_bytes().to_vec();
        holds.extend(entry.to_le_bytes());
        holds.extend([cache, 32]);
        holds.extend(icon.width.to_le_bytes());
        holds.extend(icon.height.to_le_bytes());
        // rows of the mask are as long as a number of pairs of bytes
        let mask = vec![0u8; w.div_ceil(16) * 2 * h];
        let mut colour = Vec::with_capacity(w * h * 4);
        for row in icon.pixels.chunks(w * 4).rev() {
            colour.extend(row.as_chunks::<4>().0.iter().flat_map(|p| [p[2], p[1], p[0], p[3]]));
        }
        holds.extend((mask.len() as u16).to_le_bytes());
        holds.extend((colour.len() as u16).to_le_bytes());
        holds.extend(mask);
        holds.extend(colour);
        order(OF_A_WINDOW | ITS_ICON | if large { LARGE } else { 0 }, &holds)
    }

    /// An icon of the tray, new, of a window, with its tip and its picture, of a program
    /// that speaks this version of the tray's ways.
    pub fn tray(window: u32, icon: u32, tip: &str, version: u32, picture: &Icon) -> Vec<u8> {
        let tip: Vec<u8> = tip.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut holds = [window.to_le_bytes(), icon.to_le_bytes(), version.to_le_bytes()].concat();
        holds.extend((tip.len() as u16).to_le_bytes());
        holds.extend(tip);
        holds.extend(0u32.to_le_bytes());
        // the picture as a window's icon has it, after its window
        let written = self::icon(0, false, None, picture);
        holds.extend_from_slice(&written[11..]);
        order(OF_AN_ICON_IN_THE_BAR | NEW | 0x08 | 0x01 | 0x04 | ITS_ICON, &holds)
    }

    /// An icon of the tray is gone.
    pub fn tray_gone(window: u32, icon: u32) -> Vec<u8> {
        order(OF_AN_ICON_IN_THE_BAR | GONE, &[window.to_le_bytes(), icon.to_le_bytes()].concat())
    }

    /// A window shows an icon the client keeps.
    pub fn kept_icon(id: u32, large: bool, (cache, entry): Kept) -> Vec<u8> {
        let mut holds = id.to_le_bytes().to_vec();
        holds.extend(entry.to_le_bytes());
        holds.push(cache);
        order(OF_A_WINDOW | ITS_KEPT_ICON | if large { LARGE } else { 0 }, &holds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_window_one_that_changed_and_one_that_is_gone_are_read() {
        // an icon of no size, which is passed over
        let no_icon = [&[0x2e, 31, 0][..], &(OF_A_WINDOW | ITS_ICON).to_le_bytes(), &0x1_0001u32.to_le_bytes(), &[0; 20]].concat();
        let orders = [
            stand_in::window(0x1_0001, true, "Ledger", (20, 10), (160, 100)),
            no_icon,
            stand_in::in_front(0x1_0001, &[0x1_0001, 0x2_0002]),
            stand_in::gone(0x2_0002),
        ]
        .concat();
        let read = read(4, &orders).unwrap();
        assert_eq!(
            read,
            [
                Change::Window(Window {
                    id: 0x1_0001,
                    is_new: true,
                    owner: None,
                    style: Some((0x14cf_0000, 0x100)),
                    shown: Some(Shown::AsItIs),
                    title: Some("Ledger".to_string()),
                    at: Some((20, 10)),
                    size: Some((160, 100)),
                    seen_at: Some((20, 10)),
                    seen: Some(vec![Part { left: 0, top: 0, right: 160, bottom: 100 }]),
                }),
                Change::InFront(0x1_0001),
                Change::Order(vec![0x1_0001, 0x2_0002]),
                Change::Gone(0x2_0002),
            ]
        );
        // fewer than the update says it carries is all there is
        assert_eq!(super::read(9, &orders).unwrap().len(), 4);
        // and only as many as it says are read
        assert_eq!(super::read(1, &orders).unwrap().len(), 1);
    }

    #[test]
    fn icons_are_read_in_the_depths_windows_draws_them_in() {
        // of 32 bits a pixel, with how much each pixel covers: as it was written, row by row
        let pixels: Vec<u8> = (0..2 * 3).flat_map(|at| [at * 40, 255 - at * 40, 7, if at == 0 { 0 } else { 255 }]).collect();
        let written = Icon { width: 2, height: 3, pixels };
        let orders = [stand_in::icon(5, true, Some((1, 4)), &written), stand_in::kept_icon(6, false, (1, 4))].concat();
        assert_eq!(
            read(2, &orders),
            Ok(vec![
                Change::Icon { id: 5, large: true, icon: written.clone(), kept: Some((1, 4)) },
                Change::KeptIcon { id: 6, large: false, kept: (1, 4) },
            ])
        );
        // one that the client is not to keep says so
        assert!(matches!(
            &read(1, &stand_in::icon(5, false, None, &written)).unwrap()[..],
            [Change::Icon { kept: None, large: false, .. }]
        ));

        // of one bit a pixel, with two colours, and a mask that hides the first pixel of the
        // top row: rows from the bottom one up, each as long as two bytes
        let mut one_bit = 7u32.to_le_bytes().to_vec();
        one_bit.extend([0xff, 0xff, 0, 1, 8, 0, 2, 0]);
        one_bit.extend([8, 0, 4, 0, 4, 0]);
        one_bit.extend([0x00, 0, 0x80, 0]); // the mask: the bottom row, then the top one
        one_bit.extend([0, 0, 0, 0, 0xff, 0xff, 0xff, 0]); // black and white
        one_bit.extend([0x0f, 0, 0xf0, 0]); // the bottom row black then white, the top one the other way
        let order = [&[0x2e, 0, 0][..], &(OF_A_WINDOW | ITS_ICON).to_le_bytes(), &one_bit].concat();
        let mut order = order;
        let length = order.len() as u16;
        order[1..3].copy_from_slice(&length.to_le_bytes());
        let [Change::Icon { id: 7, icon, kept: None, .. }] = &read(1, &order).unwrap()[..] else { panic!("an icon of one bit a pixel") };
        let (white, black) = ([255, 255, 255, 255], [0, 0, 0, 255]);
        let top: Vec<[u8; 4]> = icon.pixels[..32].as_chunks::<4>().0.to_vec();
        let bottom: Vec<[u8; 4]> = icon.pixels[32..].as_chunks::<4>().0.to_vec();
        assert_eq!(top, [[255, 255, 255, 0], white, white, white, black, black, black, black]);
        assert_eq!(bottom, [black, black, black, black, white, white, white, white]);
    }

    #[test]
    fn the_icons_of_the_tray_are_read_with_their_tips_and_pictures() {
        let picture = Icon { width: 2, height: 2, pixels: vec![9; 16] };
        let orders = [stand_in::tray(0x1_0001, 7, "Ledger – 2 reminders", 4, &picture), stand_in::tray_gone(0x1_0001, 7)].concat();
        assert_eq!(
            read(2, &orders),
            Ok(vec![
                Change::Tray(TrayIcon {
                    window: 0x1_0001,
                    icon: 7,
                    is_new: true,
                    version: Some(4),
                    tip: Some("Ledger – 2 reminders".to_string()),
                    hidden: Some(false),
                    picture: Some((picture, None)),
                    kept_picture: None,
                }),
                Change::TrayGone { window: 0x1_0001, icon: 7 },
            ])
        );
        // one that only shows another picture the machine keeps, and one hidden with a message popping up from it
        let mut kept = vec![0x2e, 0, 0];
        kept.extend((OF_AN_ICON_IN_THE_BAR | ITS_KEPT_ICON).to_le_bytes());
        kept.extend([1, 0, 0, 0, 7, 0, 0, 0, 3, 0, 2]);
        let mut hidden = vec![0x2e, 0, 0];
        hidden.extend((OF_AN_ICON_IN_THE_BAR | 0x02 | 0x04).to_le_bytes());
        hidden.extend([1, 0, 0, 0, 8, 0, 0, 0]);
        hidden.extend([0; 8]);
        hidden.extend([2, 0, b'!', 0, 0, 0, 1, 0, 0, 0]);
        for order in [&mut kept, &mut hidden] {
            let length = order.len() as u16;
            order[1..3].copy_from_slice(&length.to_le_bytes());
        }
        assert_eq!(
            read(2, &[kept, hidden].concat()),
            Ok(vec![
                Change::Tray(TrayIcon { window: 1, icon: 7, kept_picture: Some((2, 3)), ..TrayIcon::default() }),
                Change::Tray(TrayIcon { window: 1, icon: 8, hidden: Some(true), ..TrayIcon::default() }),
            ])
        );
    }

    #[test]
    fn every_part_a_word_on_a_window_may_carry_is_passed_over_or_read() {
        // a window that only moved, to the left of the desktop's edge
        let mut moved = vec![0x2e, 15, 0];
        moved.extend((OF_A_WINDOW | 0x0800).to_le_bytes());
        moved.extend(7u32.to_le_bytes());
        moved.extend((-8i32).to_le_bytes());
        moved.extend(30i32.to_le_bytes());
        moved[1] = moved.len() as u8;
        assert_eq!(read(1, &moved), Ok(vec![Change::Window(Window { id: 7, at: Some((-8, 30)), ..Window::default() })]));

        // everything at once, in the order it travels in
        let has = OF_A_WINDOW
            | 0x02
            | 0x08
            | 0x10
            | 0x04
            | 0x4000
            | 0x0001_0000
            | 0x80
            | 0x0800_0000
            | 0x0002_0000
            | 0x0004_0000
            | 0x0800
            | 0x8000
            | 0x0400
            | 0x0100
            | 0x1000
            | 0x0200
            | 0x0040_0000
            | 0x0080_0000
            | 0x0008_0000
            | 0x40
            | 0x01;
        let mut all = vec![0x2e, 0, 0];
        all.extend(has.to_le_bytes());
        all.extend(9u32.to_le_bytes());
        all.extend(3u32.to_le_bytes()); // whose it is
        all.extend([0x11; 8]); // its style
        all.push(2); // made small
        all.extend([4, 0, b'A', 0, 0, 0]); // a title that ends in a zero
        all.extend([0x22; 8 + 8 + 8 + 8 + 1 + 4]);
        all.extend(5i32.to_le_bytes());
        all.extend(6i32.to_le_bytes());
        all.extend([0x33; 8]);
        all.extend(70u32.to_le_bytes());
        all.extend(80u32.to_le_bytes());
        all.extend([1, 0, 0, 0, 0, 0, 70, 0, 80, 0]);
        all.extend(5i32.to_le_bytes());
        all.extend(6i32.to_le_bytes());
        all.extend([2, 0, 0, 0, 0, 0, 70, 0, 40, 0, 0, 0, 40, 0, 30, 0, 80, 0]);
        all.extend([2, 0, b'!', 0, 1, 1, 1, 3]);
        let length = all.len() as u16;
        all[1..3].copy_from_slice(&length.to_le_bytes());
        let read = read(1, &all).unwrap();
        assert_eq!(
            read,
            [Change::Window(Window {
                id: 9,
                is_new: false,
                owner: Some(3),
                style: Some((0x1111_1111, 0x1111_1111)),
                shown: Some(Shown::Minimized),
                title: Some("A".to_string()),
                at: Some((5, 6)),
                size: Some((70, 80)),
                seen_at: Some((5, 6)),
                seen: Some(vec![Part { left: 0, top: 0, right: 70, bottom: 40 }, Part { left: 0, top: 40, right: 30, bottom: 80 }]),
            })]
        );
        // cut anywhere inside what it reads, it is refused and never read past its end
        for cut in 7..all.len() - 8 {
            let mut short = all[..cut].to_vec();
            short[1..3].copy_from_slice(&(cut as u16).to_le_bytes());
            assert_eq!(super::read(1, &short), Err(WRONG), "cut at {cut}");
        }
    }

    #[test]
    fn what_is_no_word_on_a_window_ends_the_reading_and_a_wrong_length_is_refused() {
        let window = stand_in::window(1, true, "A", (0, 0), (10, 10));
        // an instruction to draw, which Away never asks for
        let other = [&window[..], &[0x01, 0x02, 0x03]].concat();
        assert_eq!(read(2, &other).unwrap().len(), 1);
        // a desktop that is not watched says nothing
        let mut unwatched = vec![0x2e, 7, 0];
        unwatched.extend((OF_THE_DESKTOP | 0x01).to_le_bytes());
        assert_eq!(read(1, &unwatched), Ok(Vec::new()));
        // a length that is more than what is there, and one that is less than a word has
        let mut long = window.clone();
        long[1] += 1;
        assert_eq!(read(1, &long), Err(WRONG));
        assert_eq!(read(1, &[0x2e, 6, 0, 0, 0, 0]), Err(WRONG));
        assert_eq!(read(1, &[]), Ok(Vec::new()));
    }
}
