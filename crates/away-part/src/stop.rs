//! The way a part stops: once, for everything it runs, when the cluster asks it to.

use tokio::sync::watch;

use crate::lines::Lines;

/// Tells a part to stop. Dropping it stops the part too.
pub struct Stopper(watch::Sender<bool>);

/// Learns that the part is stopping. Every listener and every task holds one.
#[derive(Clone)]
pub struct Stop(watch::Receiver<bool>);

/// A stopper and what it stops.
pub fn pair() -> (Stopper, Stop) {
    let (sender, receiver) = watch::channel(false);
    (Stopper(sender), Stop(receiver))
}

impl Stopper {
    /// Stop now.
    pub fn stop(&self) {
        let _ = self.0.send(true);
    }
}

impl Stop {
    /// Waits until the part is told to stop. Comes back at once if it was told already.
    pub async fn wait(&mut self) {
        while !*self.0.borrow() {
            if self.0.changed().await.is_err() {
                return;
            }
        }
    }

    /// Whether the part was told to stop.
    pub fn stopping(&self) -> bool {
        *self.0.borrow()
    }
}

/// Stops the part when the cluster ends it or somebody presses Ctrl + C, and says which.
#[cfg(unix)]
pub async fn on_signal(stopper: Stopper, lines: Lines) {
    use tokio::signal::unix::{signal, SignalKind};
    let (Ok(mut ended), Ok(mut interrupted)) = (signal(SignalKind::terminate()), signal(SignalKind::interrupt())) else {
        lines.error("could not wait for a signal to stop", &[]);
        return;
    };
    let signal = tokio::select! {
        _ = ended.recv() => "SIGTERM",
        _ = interrupted.recv() => "SIGINT",
    };
    lines.info("stopping", &[("signal", signal.into())]);
    stopper.stop();
}

/// Stops the part when somebody presses Ctrl + C.
#[cfg(not(unix))]
pub async fn on_signal(stopper: Stopper, lines: Lines) {
    if tokio::signal::ctrl_c().await.is_ok() {
        lines.info("stopping", &[("signal", "ctrl + c".into())]);
    }
    stopper.stop();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn everything_that_waits_learns_of_the_stop() {
        let (stopper, stop) = pair();
        let mut first = stop.clone();
        let mut second = stop.clone();
        assert!(!stop.stopping());
        let waiting = tokio::spawn(async move {
            first.wait().await;
            second.wait().await;
        });
        stopper.stop();
        waiting.await.expect("both learn of it");
        assert!(stop.stopping());
    }

    #[tokio::test]
    async fn a_stopper_that_is_gone_stops_too() {
        let (stopper, mut stop) = pair();
        drop(stopper);
        stop.wait().await;
    }
}
