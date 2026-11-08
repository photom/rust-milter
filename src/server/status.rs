//! `ServerHandle::status()`: the health check fields and the status
//! counters. See "Health check" in `docs/reference/tcp-server.md`.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use crate::Error;
use crate::engine::Traffic;

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
    /// Connections accepted since `run()` started, rejected ones included.
    pub connections_accepted: u64,
    /// Connections closed at once because `max_connections` or
    /// `max_memory` was reached.
    pub connections_rejected: u64,
    /// Bytes read from MTA connections, counted as they arrive.
    pub bytes_received: u64,
    /// Bytes written to MTA connections, counted as they are written.
    pub bytes_sent: u64,
    /// Closed connections by `milter.close_reason`, and for the reason
    /// `error` by `error.type`.
    pub connections_closed: Vec<(&'static str, Option<&'static str>, u64)>,
    /// Completed option negotiations, by negotiated protocol version.
    pub negotiations: Vec<(u32, u64)>,
    /// Commands received, by `milter.command`.
    pub commands: Vec<(&'static str, u64)>,
    /// Replies sent, by `milter.command` and `milter.reply`.
    pub replies: Vec<(&'static str, &'static str, u64)>,
    /// Reload requests, by `milter.reload_result` and
    /// `milter.reload_source`.
    pub reloads: Vec<(&'static str, &'static str, u64)>,
    /// When the last reload was applied; `None` before the first.
    pub last_reload: Option<SystemTime>,
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
    /// Connections this listener accepted, rejected ones included.
    pub connections_accepted: u64,
    /// Open connections this listener accepted.
    pub connections_active: usize,
    /// Connections this listener closed at once at a limit.
    pub connections_rejected: u64,
}

/// The `milter.close_reason` values, in the order of telemetry.md.
pub(crate) const CLOSE_REASONS: [&str; 6] = [
    "client_closed",
    "server_shutdown",
    "timeout",
    "error",
    "reset",
    "limit_reached",
];

/// The `error.type` values: the variant names of `Error`.
const ERROR_TYPES: [&str; 13] = [
    "Io",
    "Protocol",
    "ActionNotNegotiated",
    "Handler",
    "Timeout",
    "ConnectionClosed",
    "ConnectionReset",
    "FrameTooLarge",
    "InvalidReply",
    "Config",
    "ResourceLimit",
    "Signals",
    "Bind",
];

/// Where a reload request came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Source {
    Sighup,
    Api,
}

const RELOAD_RESULTS: [&str; 2] = ["applied", "failed"];
const RELOAD_SOURCES: [&str; 2] = ["sighup", "api"];

/// The counts of one listener. The accept loop and its connections hold
/// them, so counting takes no lock.
#[derive(Debug, Default)]
pub(crate) struct ListenerCounts {
    accepted: AtomicU64,
    active: AtomicUsize,
    rejected: AtomicU64,
}

impl ListenerCounts {
    pub(crate) fn accepted(&self) {
        self.accepted.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn rejected(&self) {
        self.rejected.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn opened(&self) {
        self.active.fetch_add(1, Ordering::SeqCst);
    }

    pub(crate) fn closed(&self) {
        self.active.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The live values behind [`Status`].
#[derive(Debug)]
pub(crate) struct Counters {
    started: Mutex<Option<Instant>>,
    shutting_down: AtomicBool,
    pub(crate) connections: AtomicUsize,
    pub(crate) accepted: AtomicU64,
    pub(crate) rejected: AtomicU64,
    /// The counts the connections add to.
    pub(crate) traffic: Arc<Traffic>,
    closed: [AtomicU64; CLOSE_REASONS.len()],
    /// The closes with the reason `error`, by `error.type`.
    closed_errors: [AtomicU64; ERROR_TYPES.len()],
    reloads: [[AtomicU64; RELOAD_SOURCES.len()]; RELOAD_RESULTS.len()],
    last_reload: Mutex<Option<SystemTime>>,
    last_error: Mutex<Option<String>>,
    listeners: Mutex<Vec<ListenerStatus>>,
    /// The counts of every address that had a listener, kept when a reload
    /// removes it, so they continue if it comes back.
    listener_counts: Mutex<Vec<(SocketAddr, Arc<ListenerCounts>)>>,
}

impl Counters {
    pub(crate) fn new(addresses: &[SocketAddr]) -> Counters {
        let mut listeners = Vec::new();
        for &address in addresses {
            listeners.push(ListenerStatus {
                address,
                bound: false,
                accepting: false,
                connections_accepted: 0,
                connections_active: 0,
                connections_rejected: 0,
            });
        }
        Counters {
            started: Mutex::new(None),
            shutting_down: AtomicBool::new(false),
            connections: AtomicUsize::new(0),
            accepted: AtomicU64::new(0),
            rejected: AtomicU64::new(0),
            traffic: Arc::default(),
            closed: Default::default(),
            closed_errors: Default::default(),
            reloads: Default::default(),
            last_reload: Mutex::new(None),
            last_error: Mutex::new(None),
            listeners: Mutex::new(listeners),
            listener_counts: Mutex::new(Vec::new()),
        }
    }

    /// The counts of the listener on `address`.
    pub(crate) fn listener(&self, address: SocketAddr) -> Arc<ListenerCounts> {
        let mut counts = lock(&self.listener_counts);
        for (known, existing) in counts.iter() {
            if *known == address {
                return Arc::clone(existing);
            }
        }
        let created = Arc::new(ListenerCounts::default());
        counts.push((address, Arc::clone(&created)));
        created
    }

    /// Counts a closed connection with its `milter.close_reason`, and its
    /// error when the reason is `error`.
    pub(crate) fn closed(&self, reason: &str, error: Option<&Error>) {
        for (index, known) in CLOSE_REASONS.iter().enumerate() {
            if *known == reason {
                self.closed[index].fetch_add(1, Ordering::Relaxed);
            }
        }
        if let (Some(error), "error") = (error, reason) {
            for (index, known) in ERROR_TYPES.iter().enumerate() {
                if *known == error.variant() {
                    self.closed_errors[index].fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }

    /// Counts a reload request that returned its result.
    pub(crate) fn reloaded(&self, applied: bool, source: Source) {
        let result = usize::from(!applied);
        let source = match source {
            Source::Sighup => 0,
            Source::Api => 1,
        };
        self.reloads[result][source].fetch_add(1, Ordering::Relaxed);
    }

    /// Records that a reload was applied now.
    pub(crate) fn applied_now(&self) {
        *lock(&self.last_reload) = Some(SystemTime::now());
    }

    /// Records the outcome of binding the listener at `index`.
    pub(crate) fn bound(&self, index: usize, error: Option<String>) {
        let bound = error.is_none();
        if let Some(listener) = lock(&self.listeners).get_mut(index) {
            listener.bound = bound;
        }
        if let Some(error) = error {
            self.listener_error(error);
        }
    }

    /// Records a listener error, such as a failed bind.
    pub(crate) fn listener_error(&self, error: String) {
        *lock(&self.last_error) = Some(error);
    }

    /// Replaces the listener entries after a reload: one per address in the
    /// new `bind_addresses`, with whether it is bound.
    pub(crate) fn replace_listeners(&self, listeners: Vec<(SocketAddr, bool)>) {
        let mut entries = lock(&self.listeners);
        // Read under the lock, so that a shutdown starting now is seen
        // either here or by `begin_shutdown` after the replacement.
        let accepting = !self.shutting_down.load(Ordering::SeqCst);
        entries.clear();
        for (address, bound) in listeners {
            entries.push(ListenerStatus {
                address,
                bound,
                accepting: accepting && bound,
                connections_accepted: 0,
                connections_active: 0,
                connections_rejected: 0,
            });
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
        let mut listeners = lock(&self.listeners);
        self.shutting_down.store(true, Ordering::SeqCst);
        for listener in listeners.iter_mut() {
            listener.accepting = false;
        }
    }

    /// Time since `run()` bound the listeners; zero before.
    pub(crate) fn uptime(&self) -> Duration {
        match *lock(&self.started) {
            Some(started) => started.elapsed(),
            None => Duration::ZERO,
        }
    }

    pub(crate) fn snapshot(&self, config_generation: u64) -> Status {
        let mut listeners = lock(&self.listeners).clone();
        {
            let counts = lock(&self.listener_counts);
            for listener in &mut listeners {
                for (address, count) in counts.iter() {
                    if *address == listener.address {
                        listener.connections_accepted = count.accepted.load(Ordering::Relaxed);
                        listener.connections_active = count.active.load(Ordering::SeqCst);
                        listener.connections_rejected = count.rejected.load(Ordering::Relaxed);
                    }
                }
            }
        }
        let shutting_down = self.shutting_down.load(Ordering::SeqCst);
        let mut any_accepting = false;
        for listener in &listeners {
            if listener.accepting {
                any_accepting = true;
            }
        }
        Status {
            accepting: any_accepting && !shutting_down,
            connections: self.connections.load(Ordering::SeqCst),
            uptime: self.uptime(),
            last_error: lock(&self.last_error).clone(),
            shutting_down,
            listeners,
            config_generation,
            connections_accepted: self.accepted.load(Ordering::SeqCst),
            connections_rejected: self.rejected.load(Ordering::SeqCst),
            bytes_received: self.traffic.bytes_received(),
            bytes_sent: self.traffic.bytes_sent(),
            connections_closed: self.connections_closed(),
            negotiations: self.traffic.negotiations(),
            commands: self.traffic.commands(),
            replies: self.traffic.replies(),
            reloads: self.reload_counts(),
            last_reload: *lock(&self.last_reload),
        }
    }

    fn connections_closed(&self) -> Vec<(&'static str, Option<&'static str>, u64)> {
        let mut closed = Vec::new();
        for (reason, count) in CLOSE_REASONS.iter().zip(&self.closed) {
            if *reason == "error" {
                for (kind, count) in ERROR_TYPES.iter().zip(&self.closed_errors) {
                    let count = count.load(Ordering::Relaxed);
                    if count > 0 {
                        closed.push((*reason, Some(*kind), count));
                    }
                }
                continue;
            }
            let count = count.load(Ordering::Relaxed);
            if count > 0 {
                closed.push((*reason, None, count));
            }
        }
        closed
    }

    fn reload_counts(&self) -> Vec<(&'static str, &'static str, u64)> {
        let mut reloads = Vec::new();
        for (result, counts) in RELOAD_RESULTS.iter().zip(&self.reloads) {
            for (source, count) in RELOAD_SOURCES.iter().zip(counts) {
                let count = count.load(Ordering::Relaxed);
                if count > 0 {
                    reloads.push((*result, *source, count));
                }
            }
        }
        reloads
    }
}

/// Locks a status mutex. A panic while holding one cannot leave a value
/// half written, because every update replaces whole values, so a poisoned
/// lock is still usable.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}
