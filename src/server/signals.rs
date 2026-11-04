//! The shutdown signals and `SIGHUP`. See "Signal handling requirements"
//! in `docs/reference/tcp-server.md` and "Concurrency rules" in
//! `docs/reference/hot-reload.md`.

use std::io;
use std::sync::Arc;

use tokio::signal::unix::{Signal, SignalKind, signal};

use super::Shared;
use super::reload::Request;

/// The handlers for `SIGTERM`, `SIGINT`, `SIGQUIT`, and `SIGHUP`.
pub(super) struct Signals {
    term: Signal,
    int: Signal,
    quit: Signal,
    hup: Signal,
}

impl Signals {
    /// Registers the handlers. From here on these signals no longer end the
    /// process, for as long as it runs.
    pub(super) fn register() -> io::Result<Signals> {
        Ok(Signals {
            term: signal(SignalKind::terminate())?,
            int: signal(SignalKind::interrupt())?,
            quit: signal(SignalKind::quit())?,
            hup: signal(SignalKind::hangup())?,
        })
    }

    /// Starts a graceful shutdown at the first shutdown signal, and forces
    /// it at the next one. `SIGHUP` requests a reload.
    pub(super) async fn watch<A>(mut self, shared: Arc<Shared<A>>) {
        loop {
            let (kind, name) = tokio::select! {
                Some(()) = self.term.recv() => (SignalKind::terminate(), "SIGTERM"),
                Some(()) = self.int.recv() => (SignalKind::interrupt(), "SIGINT"),
                Some(()) = self.quit.recv() => (SignalKind::quit(), "SIGQUIT"),
                Some(()) = self.hup.recv() => {
                    reload(&shared);
                    continue;
                }
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

/// Requests a reload through the loader, without waiting for its result;
/// the coordinator logs it.
fn reload<A>(shared: &Shared<A>) {
    let reason = if shared.stop.is_cancelled() {
        "shutting_down"
    } else if !shared.has_loader {
        "no_loader"
    } else {
        tracing::info!(
            source = "sighup",
            generation = shared.watch.generation(),
            "Reload requested"
        );
        // A coordinator that has stopped drops the request.
        let _ = shared.requests.send(Request::Load(None));
        return;
    };
    tracing::warn!(reason, "SIGHUP ignored");
}
