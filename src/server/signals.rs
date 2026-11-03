//! The shutdown signals. See "Signal handling requirements" in
//! `docs/reference/tcp-server.md`.

use std::io;
use std::sync::Arc;

use tokio::signal::unix::{Signal, SignalKind, signal};

use super::Shared;

/// The handlers for `SIGTERM`, `SIGINT`, and `SIGQUIT`.
pub(super) struct Signals {
    term: Signal,
    int: Signal,
    quit: Signal,
}

impl Signals {
    /// Registers the handlers. From here on these signals no longer end the
    /// process, for as long as it runs.
    pub(super) fn register() -> io::Result<Signals> {
        Ok(Signals {
            term: signal(SignalKind::terminate())?,
            int: signal(SignalKind::interrupt())?,
            quit: signal(SignalKind::quit())?,
        })
    }

    /// Starts a graceful shutdown at the first signal, and forces it at the
    /// next one.
    pub(super) async fn watch<A>(mut self, shared: Arc<Shared<A>>) {
        loop {
            let (kind, name) = tokio::select! {
                Some(()) = self.term.recv() => (SignalKind::terminate(), "SIGTERM"),
                Some(()) = self.int.recv() => (SignalKind::interrupt(), "SIGINT"),
                Some(()) = self.quit.recv() => (SignalKind::quit(), "SIGQUIT"),
                else => return,
            };
            let number = kind.as_raw_value();
            if !shared.stop.is_cancelled() {
                tracing::info!(
                    signal = number,
                    signal_name = name,
                    reason = "signal",
                    "Shutdown signal received"
                );
                shared.begin();
            } else if !shared.force.is_cancelled() {
                tracing::warn!(
                    signal = number,
                    signal_name = name,
                    action = "force shutdown",
                    "Second shutdown signal received"
                );
                shared.force();
            }
        }
    }
}
