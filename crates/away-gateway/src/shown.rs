//! What a session shows: a desktop that is open, over whichever protocol the machine
//! speaks. The session passes its picture to the browser and what the person does to
//! the machine, and asks each kind of desktop for the same things.

use away_rdp::rail::Program;
use away_wire::{Did, Does, TrayDoes, TrayIcon, Window};

use crate::machine::{Area, Failure, Screen};

/// A desktop's picture as it is now: rows from the top down, four bytes a pixel, blue,
/// green, red, and a fourth that is always full.
pub struct Pixels<'a> {
    /// How wide it is, in pixels.
    pub width: usize,
    /// How high.
    pub height: usize,
    /// Its pixels.
    pub pixels: &'a [u8],
}

/// A desktop that is open. What only some desktops have, the windows of programs and
/// what goes with them, is nothing unless a desktop says otherwise.
pub(crate) trait Shown {
    /// The desktop's picture, as far as the machine has drawn it.
    fn picture(&self) -> Pixels<'_>;

    /// Whether the machine is told of keys by what they write, and not by their places.
    fn takes_keys_as_written(&self) -> bool {
        false
    }

    /// Waits for the machine to send something, and takes it in. It can be given up at
    /// any moment without anything being lost: what arrived is kept.
    async fn arrives(&mut self) -> Result<(), Failure>;

    /// Draws what arrived into the picture, answers the machine where it wants an
    /// answer, and says which areas of the picture are new since this was last asked.
    async fn take(&mut self) -> Result<Vec<Area>, Failure>;

    /// Tells the connection that time passed, where it counts time.
    async fn tick(&mut self) -> Result<(), Failure> {
        Ok(())
    }

    /// Whether the machine offered its desktop anew, at another size, since this was
    /// last asked: the picture is then a new one, of that size.
    fn resized(&mut self) -> bool;

    /// The text that was copied on the machine since this was last asked.
    fn copied(&mut self) -> Option<String>;

    /// Tells the machine of keys and of the pointer.
    async fn did(&mut self, did: &[Did]) -> Result<(), Failure>;

    /// Tells the machine that the person copied this text, so that it can be pasted there.
    async fn offer(&mut self, text: &str) -> Result<(), Failure>;

    /// Tells the machine that the person's screen is now this. Whether it was told: a
    /// machine that doesn't follow a screen is not.
    async fn resize(&mut self, _screen: Screen) -> Result<bool, Failure> {
        Ok(false)
    }

    /// The windows of the programs from the front one back, and the one in front, where
    /// the machine said something of them since this was last asked.
    fn windows(&mut self) -> Option<(Vec<Window>, Option<u32>)> {
        None
    }

    /// How many windows of their own are open.
    fn windows_listed(&self) -> usize {
        0
    }

    /// The icons of windows that changed since this was last asked, as PNG pictures.
    fn icons(&mut self) -> Vec<(u32, Vec<u8>)> {
        Vec::new()
    }

    /// The icons of the tray, where it changed since this was last asked.
    fn tray(&mut self) -> Option<Vec<TrayIcon>> {
        None
    }

    /// The pictures of icons of the tray that changed since this was last asked.
    fn tray_pictures(&mut self) -> Vec<(u32, u32, Vec<u8>)> {
        Vec::new()
    }

    /// Whether each program asked for since the first started, in the order they were asked
    /// for: one that didn't comes with why, in the machine's words.
    fn started_later(&mut self) -> Vec<Result<(), String>> {
        Vec::new()
    }

    /// Tells the machine what the person does with a window as a whole.
    async fn window(&mut self, _id: u32, _does: Does) -> Result<(), Failure> {
        Ok(())
    }

    /// Tells the machine what the person does with an icon of the tray.
    async fn use_tray(&mut self, _window: u32, _icon: u32, _does: TrayDoes) -> Result<(), Failure> {
        Ok(())
    }

    /// Asks the machine for another program, in the session that runs one.
    async fn start(&mut self, _program: &Program) -> Result<(), Failure> {
        Ok(())
    }

    /// Leaves. The machine keeps what runs on the desktop.
    async fn leave(self);
}
