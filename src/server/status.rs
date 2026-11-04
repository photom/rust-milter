//! `ServerHandle::status()`: the health check fields and the status
//! counters. See "Health check" in `docs/reference/tcp-server.md`.

use std::net::SocketAddr;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// A snapshot of the server's state.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Status {
    /// `run()` has bound and no shutdown has started.
    pub accepting: bool,
    /// Open connections.
    pub connections: usize,
    /// Time since `run()` bound the listeners; zero before.
    pub uptime: Duration,
    /// The last listener error, such as a failed bind in `BestEffort` mode.
    pub last_error: Option<String>,
    /// A shutdown has started.
    pub shutting_down: bool,
    /// One entry per address in `bind_addresses`, in their order.
    pub listeners: Vec<ListenerStatus>,
    /// The current configuration generation, starting at 1.
    pub config_generation: u64,
    /// Settings that wait for a restart.
    pub pending_restart: Vec<&'static str>,
    /// Connections accepted since `run()` started, rejected ones included.
    pub connections_accepted: u64,
    /// Connections closed at once because `max_connections` or
    /// `max_memory` was reached.
    pub connections_rejected: u64,
}

/// The state of one listener.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ListenerStatus {
    /// The listener's address.
    pub address: SocketAddr,
    /// The address was bound.
    pub bound: bool,
    /// The listener is accepting connections.
    pub accepting: bool,
}

/// The live values behind [`Status`].
#[derive(Debug)]
pub(crate) struct Counters {
    started: Mutex<Option<Instant>>,
    shutting_down: AtomicBool,
    pub(crate) connections: AtomicUsize,
    pub(crate) accepted: AtomicU64,
    pub(crate) rejected: AtomicU64,
    /// Bytes read and written by the connections that have closed.
    pub(crate) bytes_read: AtomicU64,
    pub(crate) bytes_written: AtomicU64,
    last_error: Mutex<Option<String>>,
    listeners: Mutex<Vec<ListenerStatus>>,
}

impl Counters {
    pub(crate) fn new(addresses: &[SocketAddr]) -> Counters {
        let listeners = addresses
            .iter()
            .map(|&address| ListenerStatus {
                address,
                bound: false,
                accepting: false,
            })
            .collect();
        Counters {
            started: Mutex::new(None),
            shutting_down: AtomicBool::new(false),
            connections: AtomicUsize::new(0),
            accepted: AtomicU64::new(0),
            rejected: AtomicU64::new(0),
            bytes_read: AtomicU64::new(0),
            bytes_written: AtomicU64::new(0),
            last_error: Mutex::new(None),
            listeners: Mutex::new(listeners),
        }
    }

    /// Records the outcome of binding the listener at `index`.
    pub(crate) fn bound(&self, index: usize, error: Option<String>) {
        let bound = error.is_none();
        if let Some(listener) = lock(&self.listeners).get_mut(index) {
            listener.bound = bound;
        }
        if let Some(error) = error {
            *lock(&self.last_error) = Some(error);
        }
    }

    /// The bound listeners start or stop accepting.
    pub(crate) fn set_accepting(&self, accepting: bool) {
        if accepting {
            *lock(&self.started) = Some(Instant::now());
        }
        for listener in lock(&self.listeners).iter_mut() {
            listener.accepting = accepting && listener.bound;
        }
    }

    pub(crate) fn begin_shutdown(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        for listener in lock(&self.listeners).iter_mut() {
            listener.accepting = false;
        }
    }

    /// Time since `run()` bound the listeners; zero before.
    pub(crate) fn uptime(&self) -> Duration {
        lock(&self.started).map_or(Duration::ZERO, |started| started.elapsed())
    }

    pub(crate) fn snapshot(&self, config_generation: u64) -> Status {
        let listeners = lock(&self.listeners).clone();
        let shutting_down = self.shutting_down.load(Ordering::SeqCst);
        Status {
            accepting: listeners.iter().any(|l| l.accepting) && !shutting_down,
            connections: self.connections.load(Ordering::SeqCst),
            uptime: self.uptime(),
            last_error: lock(&self.last_error).clone(),
            shutting_down,
            listeners,
            config_generation,
            pending_restart: Vec::new(),
            connections_accepted: self.accepted.load(Ordering::SeqCst),
            connections_rejected: self.rejected.load(Ordering::SeqCst),
        }
    }
}

/// Locks a status mutex. A panic while holding one cannot leave a value
/// half written, because every update replaces whole values, so a poisoned
/// lock is still usable.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
