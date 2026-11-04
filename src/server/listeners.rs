//! The listeners of a running server, and how a reload changes them. See
//! "Listener changes" in `docs/reference/hot-reload.md`.

use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use super::reload::{Outcome, ReloadReport, Reloaded, Target};
use super::{Accepting, accept, error_chain, socket};
use crate::config::reload::{ReloadClass, SETTINGS, changed};
use crate::{BindMode, Handler, ReloadError, ServerConfig};

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
    Changes {
        added: new.iter().filter(|a| !old.contains(a)).copied().collect(),
        removed: old.iter().filter(|a| !new.contains(a)).copied().collect(),
        kept: old.iter().filter(|a| new.contains(a)).copied().collect(),
    }
}

/// A listener that is accepting.
struct Listener {
    address: SocketAddr,
    /// Stops this listener's accept loop; a child of the server's `stop`.
    stop: CancellationToken,
    /// The configuration it was bound with.
    config: Arc<ServerConfig>,
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

    /// Starts accepting on a listener bound with `config`.
    pub(super) fn start(
        &mut self,
        address: SocketAddr,
        listener: TcpListener,
        config: Arc<ServerConfig>,
    ) {
        let stop = self.server.shared.stop.child_token();
        let server = Arc::clone(&self.server);
        let token = stop.clone();
        self.tasks.spawn(async move {
            accept::accept_loop(listener, address, &token, |stream, peer| {
                server.connection(stream, peer, address);
            })
            .await;
        });
        self.open.push(Listener {
            address,
            stop,
            config,
        });
    }

    /// The settings of class `new_listeners` in which `new` differs from
    /// what a listener that stays was bound with.
    fn pending_restart(&self, new: &ServerConfig) -> Vec<&'static str> {
        SETTINGS
            .iter()
            .filter(|setting| setting.class == ReloadClass::NewListeners)
            .filter(|setting| {
                self.open
                    .iter()
                    .any(|listener| setting.differs(&listener.config, new))
            })
            .map(|setting| setting.name)
            .collect()
    }
}

impl<H: Handler, A: Send + Sync + 'static> Target<A> for Listeners<H, A> {
    /// Binds the added addresses, publishes, then stops the removed ones,
    /// so some listener accepts at every moment.
    async fn apply(&mut self, generation: u64, new: Reloaded<A>) -> Outcome {
        let Reloaded {
            server: config,
            app,
        } = new;
        let config = Arc::new(config);
        let server = Arc::clone(&self.server);
        let counters = &server.shared.counters;
        let open: Vec<SocketAddr> = self.open.iter().map(|l| l.address).collect();
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

        let (_, old) = server.shared.watch.server();
        server.budget.set_max(config.resource_limits.max_memory);
        server
            .shared
            .publisher
            .publish(generation, Arc::clone(&config), app);

        let listeners_added: Vec<SocketAddr> = bound.iter().map(|(addr, _)| *addr).collect();
        for (addr, listener) in bound {
            tracing::info!(address = %addr, backlog = config.backlog, "Listener bound");
            self.start(addr, listener, Arc::clone(&config));
        }
        self.open.retain(|listener| {
            let removed = changes.removed.contains(&listener.address);
            if removed {
                listener.stop.cancel();
            }
            !removed
        });

        let pending_restart = self.pending_restart(&config);
        counters.replace_listeners(config.bind_addresses.iter().map(|&address| {
            let bound = self.open.iter().any(|l| l.address == address);
            (address, bound)
        }));
        counters.set_pending_restart(pending_restart.clone());
        Ok(ReloadReport {
            generation,
            changed: changed(&old, &config),
            pending_restart,
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
