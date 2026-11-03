//! The server: listeners, one task per connection, resource limits, status,
//! and the handle that stops it.
//!
//! The rules are in `docs/reference/tcp-server.md`. Graceful shutdown,
//! signals, and reload are specified there and in `hot-reload.md`.

mod accept;
mod memory;
#[cfg(feature = "test-support")]
mod readback;
mod socket;
mod status;
mod watch;

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

pub(crate) use memory::Reservation;
#[cfg(feature = "test-support")]
pub use readback::SocketReadback;
pub use status::{ListenerStatus, Status};
pub use watch::ConfigWatch;

use memory::Budget;
use status::Counters;

use crate::engine::{self, Closed, Limits, Report};
use crate::{BindMode, Error, Handler, Result, ServerConfig};

/// The generation of the configuration a server is built with.
const FIRST_GENERATION: u64 = 1;

/// The milter server, generic over the handler `H` and the application
/// configuration `A`.
pub struct Server<H, A = ()> {
    config: Arc<ServerConfig>,
    handler: Arc<H>,
    handle: ServerHandle<A>,
}

impl Server<(), ()> {
    /// Starts a builder.
    pub fn builder() -> ServerBuilder<(), ()> {
        ServerBuilder {
            config: ServerConfig::default(),
            app: (),
            factory: Box::new(|_| ()),
        }
    }
}

/// Builds a [`Server`].
///
/// Set the application configuration with `app_config` before `handler`,
/// because the handler factory receives it. Without `app_config` it is `()`.
pub struct ServerBuilder<H, A = ()> {
    config: ServerConfig,
    app: A,
    factory: Factory<H, A>,
}

/// The handler factory, called once in `build`.
type Factory<H, A> = Box<dyn FnOnce(&HandlerContext<A>) -> H>;

impl<H, A> ServerBuilder<H, A> {
    /// The initial server configuration. Without it, the defaults are used,
    /// which have no bind address, so `build` fails.
    pub fn config(mut self, config: ServerConfig) -> Self {
        self.config = config;
        self
    }
}

impl<A> ServerBuilder<(), A> {
    /// The initial application configuration.
    pub fn app_config<B>(self, app: B) -> ServerBuilder<(), B> {
        ServerBuilder {
            config: self.config,
            app,
            factory: Box::new(|_| ()),
        }
    }

    /// The handler factory. It is called once, in `build`, and all
    /// connections share the handler it returns.
    pub fn handler<H, F>(self, factory: F) -> ServerBuilder<H, A>
    where
        F: FnOnce(&HandlerContext<A>) -> H + 'static,
    {
        ServerBuilder {
            config: self.config,
            app: self.app,
            factory: Box::new(factory),
        }
    }
}

impl<H: Handler, A: Send + Sync + 'static> ServerBuilder<H, A> {
    /// Validates the configuration and builds the server.
    pub fn build(self) -> Result<Server<H, A>> {
        self.config.validate()?;
        let shutdown = CancellationToken::new();
        let (watch, publisher) = ConfigWatch::new(self.app);
        let context = HandlerContext {
            watch: watch.clone(),
            shutdown: shutdown.clone(),
        };
        let handler = (self.factory)(&context);
        let counters = Counters::new(&self.config.bind_addresses);
        Ok(Server {
            handle: ServerHandle {
                inner: Arc::new(Shared {
                    shutdown,
                    stopped: CancellationToken::new(),
                    running: AtomicBool::new(false),
                    counters,
                    watch,
                    _publisher: publisher,
                }),
            },
            config: Arc::new(self.config),
            handler: Arc::new(handler),
        })
    }
}

impl<H: Handler, A: Send + Sync + 'static> Server<H, A> {
    /// A handle that controls the server. Can be called before `run`.
    pub fn handle(&self) -> ServerHandle<A> {
        self.handle.clone()
    }

    /// Binds the listeners as `bind_mode` says and serves until shutdown.
    pub async fn run(self) -> Result<()> {
        let shared = &self.handle.inner;
        tracing::info!(
            version = env!("CARGO_PKG_VERSION"),
            bind_addresses = ?self.config.bind_addresses,
            "Server starting"
        );
        let listeners = bind(&self.config, &shared.counters)?;
        let bound: Vec<SocketAddr> = listeners.iter().map(|(addr, _)| *addr).collect();
        shared.running.store(true, Ordering::SeqCst);
        shared.counters.set_accepting(true);
        tracing::info!(
            bind_addresses = ?bound,
            pid = std::process::id(),
            "Server started"
        );

        let server = Arc::new(Accepting {
            handler: Arc::clone(&self.handler),
            config: Arc::clone(&self.config),
            shared: Arc::clone(shared),
            budget: Budget::new(self.config.resource_limits.max_memory),
            connections: TaskTracker::new(),
            next_id: AtomicU64::new(1),
        });
        let accepting = TaskTracker::new();
        for (address, listener) in listeners {
            let server = Arc::clone(&server);
            accepting.spawn(async move {
                let shutdown = server.shared.shutdown.clone();
                accept::accept_loop(listener, address, &shutdown, |stream, peer| {
                    server.connection(stream, peer, address);
                })
                .await;
            });
        }
        accepting.close();
        accepting.wait().await;
        shared.counters.set_accepting(false);
        server.connections.close();
        server.connections.wait().await;

        shared.running.store(false, Ordering::SeqCst);
        shared.stopped.cancel();
        Ok(())
    }
}

/// Binds every address in `bind_addresses`. `FailFast` fails at the first
/// address that cannot bind; `BestEffort` serves the addresses that bound
/// and fails only if none did.
fn bind(config: &ServerConfig, counters: &Counters) -> Result<Vec<(SocketAddr, TcpListener)>> {
    let mut listeners = Vec::new();
    let mut first_failure = None;
    for (index, &addr) in config.bind_addresses.iter().enumerate() {
        match socket::listen(addr, &config.socket_options, config.backlog) {
            Ok(listener) => {
                tracing::info!(address = %addr, backlog = config.backlog, "Listener bound");
                counters.bound(index, None);
                listeners.push((addr, listener));
            }
            Err(source) => {
                let error = Error::Bind { addr, source };
                let text = error_chain(&error);
                counters.bound(index, Some(text.clone()));
                if config.bind_mode == BindMode::FailFast {
                    tracing::error!(address = %addr, error = %text, "Listener bind failed");
                    return Err(error);
                }
                tracing::warn!(address = %addr, error = %text, "Listener bind failed");
                first_failure.get_or_insert(error);
            }
        }
    }
    match first_failure {
        Some(error) if listeners.is_empty() => Err(error),
        _ => Ok(listeners),
    }
}

/// What the accept loops share while the server runs.
struct Accepting<H, A> {
    handler: Arc<H>,
    config: Arc<ServerConfig>,
    shared: Arc<Shared<A>>,
    budget: Arc<Budget>,
    connections: TaskTracker,
    next_id: AtomicU64,
}

impl<H: Handler, A: Send + Sync + 'static> Accepting<H, A> {
    /// Takes on an accepted connection, or closes it at once when a limit
    /// is reached.
    fn connection(&self, stream: TcpStream, peer_addr: SocketAddr, listener: SocketAddr) {
        let accepted_at = Instant::now();
        let counters = &self.shared.counters;
        counters.accepted.fetch_add(1, Ordering::Relaxed);
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        let max_connections = self.config.max_connections;
        if counters.connections.load(Ordering::SeqCst) >= max_connections {
            counters.rejected.fetch_add(1, Ordering::Relaxed);
            tracing::warn!(
                conn_id = id,
                remote_addr = %peer_addr,
                max_connections,
                "Connection rejected (limit)"
            );
            return;
        }
        let Some(memory) = Reservation::new(&self.budget) else {
            counters.rejected.fetch_add(1, Ordering::Relaxed);
            tracing::warn!(
                limit_type = "max_memory",
                current = self.budget.used(),
                max = self.budget.max(),
                "Resource limit reached"
            );
            return;
        };
        let Ok(local_addr) = stream.local_addr() else {
            return;
        };
        // A connection whose options cannot be set is closed before any
        // read, as a setup error.
        if let Err(error) = socket::configure(&stream, &self.config.socket_options) {
            tracing::debug!(conn_id = id, error = %error, "Socket setup failed");
            return;
        }
        #[cfg(feature = "test-support")]
        let Ok(probe) = readback::Probe::new(&stream) else {
            return;
        };
        let info = ConnectionInfo {
            id,
            peer_addr,
            local_addr,
            accepted_at,
            config_generation: FIRST_GENERATION,
            #[cfg(feature = "test-support")]
            probe,
        };
        tracing::debug!(
            conn_id = id,
            remote_addr = %peer_addr,
            local_addr = %local_addr,
            listener = %listener,
            "Connection accepted"
        );
        let open = Open::new(Arc::clone(&self.shared));
        self.connections.spawn(serve(
            stream,
            Arc::clone(&self.handler),
            info,
            Arc::clone(&self.config),
            memory,
            open,
        ));
    }
}

/// Counts a connection as open until it is dropped.
struct Open<A> {
    shared: Arc<Shared<A>>,
}

impl<A> Open<A> {
    fn new(shared: Arc<Shared<A>>) -> Open<A> {
        shared.counters.connections.fetch_add(1, Ordering::SeqCst);
        Open { shared }
    }
}

impl<A> Drop for Open<A> {
    fn drop(&mut self) {
        self.shared
            .counters
            .connections
            .fetch_sub(1, Ordering::SeqCst);
    }
}

async fn serve<H: Handler, A>(
    stream: TcpStream,
    handler: Arc<H>,
    info: ConnectionInfo,
    config: Arc<ServerConfig>,
    memory: Reservation,
    open: Open<A>,
) {
    let (reader, writer) = stream.into_split();
    let id = info.id;
    let peer_addr = info.peer_addr;
    let accepted_at = info.accepted_at;
    let limits = Limits {
        max_packet: config.resource_limits.per_connection_buffer,
        timeouts: &config.timeouts,
        memory,
    };
    let shutdown = open.shared.shutdown.clone();
    let report = engine::run(&*handler, reader, writer, Arc::new(info), limits, shutdown).await;
    log_close(id, peer_addr, accepted_at, &report, &config);
    drop(open);
}

/// Writes the "Connection closed" log, at WARN only for an error.
fn log_close(
    id: u64,
    peer_addr: SocketAddr,
    accepted_at: Instant,
    report: &Report,
    config: &ServerConfig,
) {
    let reason = match &report.outcome {
        Ok(Closed::ByClient) => "client_closed",
        Ok(Closed::ByShutdown) => "server_shutdown",
        Err(Error::Timeout(_)) => "timeout",
        Err(Error::ConnectionReset) => "reset",
        Err(Error::ResourceLimit(_)) => "limit_reached",
        Err(_) => "error",
    };
    if let Err(Error::Timeout(kind)) = &report.outcome {
        let value = match kind {
            crate::TimeoutKind::Connect => config.timeouts.connect,
            crate::TimeoutKind::Read => config.timeouts.read,
            crate::TimeoutKind::Write => config.timeouts.write,
            crate::TimeoutKind::Idle => config.timeouts.idle,
        };
        tracing::debug!(
            conn_id = id,
            timeout_type = %kind,
            timeout_value = ?value,
            "Connection timeout"
        );
    }
    let duration_ms = u64::try_from(accepted_at.elapsed().as_millis()).unwrap_or(u64::MAX);
    let error = report.outcome.as_ref().err().map(|e| error_chain(e));
    macro_rules! closed {
        ($level:ident) => {
            tracing::$level!(
                conn_id = id,
                remote_addr = %peer_addr,
                duration_ms,
                bytes_read = report.bytes_read,
                bytes_written = report.bytes_written,
                commands = report.commands,
                reason,
                error = error.as_deref(),
                "Connection closed"
            )
        };
    }
    if reason == "error" {
        closed!(warn);
    } else {
        closed!(debug);
    }
}

/// An error and its `source()` chain, joined by `: `.
fn error_chain(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

/// Controls a running server. Clones control the same server, and a handle
/// stays valid after the server stops.
pub struct ServerHandle<A = ()> {
    inner: Arc<Shared<A>>,
}

impl<A> Clone for ServerHandle<A> {
    fn clone(&self) -> Self {
        ServerHandle {
            inner: Arc::clone(&self.inner),
        }
    }
}

struct Shared<A> {
    shutdown: CancellationToken,
    stopped: CancellationToken,
    running: AtomicBool,
    counters: Counters,
    watch: ConfigWatch<A>,
    _publisher: watch::Publisher<A>,
}

impl<A> ServerHandle<A> {
    /// Starts a shutdown: the listeners close, and each connection closes
    /// when it waits for the MTA's next command.
    pub fn shutdown(&self) {
        self.inner.counters.begin_shutdown();
        self.inner.shutdown.cancel();
    }

    /// Waits until the server has stopped.
    pub async fn wait(&self) {
        self.inner.stopped.cancelled().await;
    }

    /// Whether the server is running.
    pub fn is_running(&self) -> bool {
        self.inner.running.load(Ordering::SeqCst)
    }

    /// A snapshot of the server's state and counters.
    pub fn status(&self) -> Status {
        self.inner.counters.snapshot(self.inner.watch.generation())
    }

    /// A [`ConfigWatch`] for the application configuration.
    pub fn config_watch(&self) -> ConfigWatch<A> {
        self.inner.watch.clone()
    }
}

/// What the handler factory receives at build time.
pub struct HandlerContext<A> {
    watch: ConfigWatch<A>,
    shutdown: CancellationToken,
}

impl<A> HandlerContext<A> {
    /// Read access to the application configuration.
    pub fn config_watch(&self) -> &ConfigWatch<A> {
        &self.watch
    }

    /// The server's shutdown token.
    pub fn shutdown(&self) -> &CancellationToken {
        &self.shutdown
    }
}

/// Facts about an MTA connection.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ConnectionInfo {
    /// Connection ID, unique within the server's lifetime.
    pub id: u64,
    /// The MTA's address.
    pub peer_addr: SocketAddr,
    /// The listener address that accepted the connection.
    pub local_addr: SocketAddr,
    /// When `accept()` returned.
    pub accepted_at: Instant,
    /// The generation of the configuration the connection uses.
    pub config_generation: u64,
    /// A duplicate handle of the socket, for reading its options in
    /// tests. A clone of this value kept after the connection ends keeps
    /// the socket open, so tests must not keep one.
    #[cfg(feature = "test-support")]
    probe: readback::Probe,
}
