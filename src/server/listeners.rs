//! The listeners of a running server, and how a reload changes them. See
//! "Listener changes" in `docs/reference/hot-reload.md`.

use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use super::accept::{self, OnAccept};
use super::reload::{Outcome, ReloadReport, Reloaded, Target};
use super::status::ListenerCounts;
use super::{Accepting, error_chain, socket};
use crate::config::reload::{changed, requires_restart};
use crate::{BindMode, Handler, ReloadError};

/// How a reload changes the set of listen addresses.
#[derive(Debug, PartialEq, Eq)]
struct Changes {
    /// Only in the new set, in its order.
    added: Vec<SocketAddr>,
    /// Only in the old set, in its order.
    removed: Vec<SocketAddr>,
    /// In both, in the old set's order.
    kept: Vec<SocketAddr>,
}

/// Compares two address lists as sets.
fn compare(old: &[SocketAddr], new: &[SocketAddr]) -> Changes {
    let mut changes = Changes {
        added: Vec::new(),
        removed: Vec::new(),
        kept: Vec::new(),
    };
    for address in new {
        if !old.contains(address) {
            changes.added.push(*address);
        }
    }
    for address in old {
        if new.contains(address) {
            changes.kept.push(*address);
        } else {
            changes.removed.push(*address);
        }
    }
    changes
}

/// A listener that is accepting.
struct Listener {
    address: SocketAddr,
    /// Stops this listener's accept loop; a child of the server's `stop`.
    stop: CancellationToken,
}

/// Hands the connections of one listener to the server.
struct Handoff<H, A> {
    server: Arc<Accepting<H, A>>,
    address: SocketAddr,
    counts: Arc<ListenerCounts>,
}

impl<H: Handler, A: Send + Sync + 'static> OnAccept<TcpStream> for Handoff<H, A> {
    fn accepted(&mut self, stream: TcpStream, peer: SocketAddr) {
        self.server
            .connection(stream, peer, self.address, &self.counts);
    }
}

/// The listener set. Only the reload coordinator changes it once the
/// server runs.
pub(super) struct Listeners<H, A> {
    server: Arc<Accepting<H, A>>,
    tasks: TaskTracker,
    open: Vec<Listener>,
}

impl<H: Handler, A: Send + Sync + 'static> Listeners<H, A> {
    /// An empty set whose accept loops run on `tasks`.
    pub(super) fn new(server: Arc<Accepting<H, A>>, tasks: TaskTracker) -> Listeners<H, A> {
        Listeners {
            server,
            tasks,
            open: Vec::new(),
        }
    }

    /// Starts accepting on `listener`.
    pub(super) fn start(&mut self, address: SocketAddr, listener: TcpListener) {
        let stop = self.server.shared.stop.child_token();
        let token = stop.clone();
        let mut handoff = Handoff {
            server: Arc::clone(&self.server),
            address,
            counts: self.server.shared.counters.listener(address),
        };
        self.tasks.spawn(async move {
            accept::accept_loop(listener, address, &token, &mut handoff).await;
        });
        self.open.push(Listener { address, stop });
    }
}

impl<H: Handler, A: Send + Sync + 'static> Target<A> for Listeners<H, A> {
    /// Fails without a change if `new` changes a setting that requires a
    /// restart. Otherwise binds the added addresses, publishes, then stops
    /// the removed ones, so some listener accepts at every moment.
    async fn apply(&mut self, generation: u64, new: Reloaded<A>) -> Outcome {
        let Reloaded {
            server: config,
            app,
        } = new;
        let config = Arc::new(config);
        let server = Arc::clone(&self.server);
        let counters = &server.shared.counters;
        let (_, old) = server.shared.watch.server();
        let settings = requires_restart(&old, &config);
        if !settings.is_empty() {
            return Err(ReloadError::RestartRequired { settings });
        }
        let mut open = Vec::new();
        for listener in &self.open {
            open.push(listener.address);
        }
        let changes = compare(&open, &config.bind_addresses);

        let mut bound = Vec::new();
        let mut bind_failures = Vec::new();
        for &addr in &changes.added {
            match socket::listen(addr, &config.socket_options, config.backlog) {
                Ok(listener) => bound.push((addr, listener)),
                Err(source) => {
                    let error = ReloadError::Bind { addr, source };
                    let text = error_chain(&error);
                    counters.listener_error(text.clone());
                    if config.bind_mode == BindMode::FailFast {
                        // Dropping `bound` closes the listeners bound so far.
                        tracing::error!(address = %addr, error = %text, "Listener bind failed");
                        return Err(error);
                    }
                    tracing::warn!(address = %addr, error = %text, "Listener bind failed");
                    bind_failures.push((addr, text));
                }
            }
        }
        if changes.kept.is_empty() && bound.is_empty() {
            return Err(ReloadError::NoListener);
        }

        server.budget.set_max(config.resource_limits.max_memory);
        server
            .shared
            .publisher
            .publish(generation, Arc::clone(&config), app);

        let mut listeners_added = Vec::new();
        for (addr, listener) in bound {
            tracing::info!(address = %addr, backlog = config.backlog, "Listener bound");
            listeners_added.push(addr);
            self.start(addr, listener);
        }
        let mut still_open = Vec::new();
        for listener in self.open.drain(..) {
            if changes.removed.contains(&listener.address) {
                listener.stop.cancel();
            } else {
                still_open.push(listener);
            }
        }
        self.open = still_open;

        let mut statuses = Vec::new();
        for &address in &config.bind_addresses {
            let mut bound = false;
            for listener in &self.open {
                if listener.address == address {
                    bound = true;
                }
            }
            statuses.push((address, bound));
        }
        counters.replace_listeners(statuses);
        Ok(ReloadReport {
            generation,
            changed: changed(&old, &config),
            listeners_added,
            listeners_removed: changes.removed,
            bind_failures,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(port: u16) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], port))
    }

    #[test]
    fn rld_001_compares_the_address_sets() {
        let (a, b, c) = (addr(61001), addr(61002), addr(61003));

        let changes = compare(&[a, b], &[b, c]);

        assert_eq!(
            changes,
            Changes {
                added: vec![c],
                removed: vec![a],
                kept: vec![b],
            }
        );
    }
}
