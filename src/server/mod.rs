//! The server: listeners, one task per connection, resource limits, status,
//! and the handle that stops it.
//!
//! The rules are in `docs/reference/tcp-server.md`. Graceful shutdown,
//! signals, and reload are specified there and in `hot-reload.md`.

mod accept;
mod listeners;
mod memory;
#[cfg(feature = "otel")]
mod otel;
#[cfg(feature = "test-support")]
mod readback;
mod reload;
#[cfg(unix)]
mod signals;
mod socket;
mod status;
mod watch;

use std::marker::PhantomData;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tracing::Instrument;
use tracing::field::Empty;

pub(crate) use memory::Reservation;
#[cfg(feature = "otel")]
pub(crate) use otel::Histograms;
#[cfg(feature = "test-support")]
pub use readback::SocketReadback;
pub use reload::{ReloadLoader, ReloadReport, Reloaded};
pub use status::{ListenerStatus, Status};
pub use watch::ConfigWatch;

use listeners::Listeners;
use memory::Budget;
use reload::{Coordinator, Loader, Outcome, Request};
use status::{Counters, ListenerCounts, Source};

use crate::engine::{self, Closed, Limits, Report, Stop, Trace};
use crate::{BindMode, Error, Handler, ReloadError, Result, ServerConfig};

/// The generation of the configuration a server is built with.
const FIRST_GENERATION: u64 = 1;

/// The milter server, generic over the handler `H` and the application
/// configuration `A`.
pub struct Server<H, A = ()> {
    handler: Arc<H>,
    handle: ServerHandle<A>,
    loader: Option<Loader<A>>,
    requests: mpsc::UnboundedReceiver<Request<A>>,
}

impl Server<(), ()> {
    /// Starts a builder.
    pub fn builder() -> ServerBuilder<(), ()> {
        ServerBuilder {
            config: ServerConfig::default(),
            app: (),
            factory: (),
            handler: PhantomData,
            loader: None,
            #[cfg(feature = "otel")]
            meter_provider: None,
        }
    }
}

/// Builds a [`Server`].
///
/// Set the application configuration with `app_config` before `handler`
/// and `reload_with`: the handler factory receives it, and the loader
/// returns it. Without `app_config` it is `()`.
///
/// `F` is the handler factory that `handler` stores and `build` calls. It
/// is `()` until `handler` is called, so `build` does not exist before.
pub struct ServerBuilder<H, A = (), F = ()> {
    config: ServerConfig,
    app: A,
    factory: F,
    /// The handler type the factory returns.
    handler: PhantomData<H>,
    loader: Option<Loader<A>>,
    #[cfg(feature = "otel")]
    meter_provider: Option<SharedMeterProvider>,
}

/// A meter provider the builder was given.
#[cfg(feature = "otel")]
type SharedMeterProvider = Arc<dyn opentelemetry::metrics::MeterProvider + Send + Sync>;

impl<H, A, F> ServerBuilder<H, A, F> {
    /// The initial server configuration. Without it, the defaults are used,
    /// which have no bind address, so `build` fails.
    pub fn config(mut self, config: ServerConfig) -> Self {
        self.config = config;
        self
    }

    /// The reload loader, which `SIGHUP` and [`ServerHandle::reload`] run.
    /// It returns the new server and application configurations, or an
    /// error that fails the reload.
    pub fn reload_with<L: ReloadLoader<A>>(mut self, loader: L) -> Self
    where
        A: Send + 'static,
    {
        self.loader = Some(Box::new(loader));
        self
    }

    /// The OpenTelemetry meter provider for the server's metrics. Without
    /// it, the server uses `opentelemetry::global::meter_provider()`.
    #[cfg(feature = "otel")]
    pub fn meter_provider<P>(mut self, provider: P) -> Self
    where
        P: opentelemetry::metrics::MeterProvider + Send + Sync + 'static,
    {
        self.meter_provider = Some(Arc::new(provider));
        self
    }
}

impl<A> ServerBuilder<(), A> {
    /// The initial application configuration. Call it before
    /// `reload_with`: a loader registered earlier returns the old type, so
    /// it is dropped.
    pub fn app_config<B>(self, app: B) -> ServerBuilder<(), B> {
        ServerBuilder {
            config: self.config,
            app,
            factory: (),
            handler: PhantomData,
            loader: None,
            #[cfg(feature = "otel")]
            meter_provider: self.meter_provider,
        }
    }

    /// The handler factory. It is called once, in `build`, and all
    /// connections share the handler it returns.
    pub fn handler<H, F>(self, factory: F) -> ServerBuilder<H, A, F>
    where
        F: FnOnce(&HandlerContext<A>) -> H,
    {
        ServerBuilder {
            config: self.config,
            app: self.app,
            factory,
            handler: PhantomData,
            loader: self.loader,
            #[cfg(feature = "otel")]
            meter_provider: self.meter_provider,
        }
    }
}

impl<H: Handler, A: Send + Sync + 'static, F> ServerBuilder<H, A, F> {
    /// Validates the configuration and builds the server.
    pub fn build(self) -> Result<Server<H, A>>
    where
        F: FnOnce(&HandlerContext<A>) -> H,
    {
        self.config.validate()?;
        let shutdown = CancellationToken::new();
        let counters = Arc::new(Counters::new(&self.config.bind_addresses));
        let (watch, publisher) = ConfigWatch::new(self.config, self.app);
        #[cfg(feature = "otel")]
        let instruments = {
            let provider = match self.meter_provider {
                Some(provider) => provider,
                None => opentelemetry::global::meter_provider(),
            };
            otel::register(&otel::meter(&*provider), &counters, &watch)
        };
        let context = HandlerContext {
            watch: watch.clone(),
            shutdown: shutdown.clone(),
        };
        let handler = (self.factory)(&context);
        let (requests, receiver) = mpsc::unbounded_channel();
        Ok(Server {
            handle: ServerHandle {
                inner: Arc::new(Shared {
                    stop: CancellationToken::new(),
                    shutdown,
                    force: CancellationToken::new(),
                    stopped: CancellationToken::new(),
                    running: AtomicBool::new(false),
                    counters,
                    watch,
                    publisher,
                    requests,
                    has_loader: self.loader.is_some(),
                    #[cfg(feature = "otel")]
                    instruments,
                }),
            },
            handler: Arc::new(handler),
            loader: self.loader,
            requests: receiver,
        })
    }
}

impl<H: Handler, A: Send + Sync + 'static> Server<H, A> {
    /// A handle that controls the server. Can be called before `run`.
    pub fn handle(&self) -> ServerHandle<A> {
        self.handle.clone()
    }

    /// Binds the listeners as `bind_mode` says, registers the handlers for
    /// `SIGTERM`, `SIGINT`, `SIGQUIT`, and `SIGHUP` on Unix, and serves
    /// until shutdown. Once registered, these signals no longer end the
    /// process, even after `run()` returns.
    pub async fn run(self) -> Result<()> {
        let shared = &self.handle.inner;
        let (generation, config) = shared.watch.server();
        tracing::info!(
            version = env!("CARGO_PKG_VERSION"),
            bind_addresses = ?config.bind_addresses,
            "Server starting"
        );
        let listeners = bind(&config, &shared.counters)?;
        // Registered after the binds, so a failed bind leaves the signals
        // alone, and before any listener accepts.
        #[cfg(unix)]
        let signals = match signals::Signals::register() {
            Ok(signals) => signals,
            Err(error) => return Err(Error::Signals(error)),
        };
        let mut bound = Vec::new();
        for (addr, _) in &listeners {
            bound.push(*addr);
        }
        shared.running.store(true, Ordering::SeqCst);
        shared.counters.set_accepting(true);
        tracing::info!(
            bind_addresses = ?bound,
            pid = std::process::id(),
            "Server started"
        );

        let server = Arc::new(Accepting {
            handler: Arc::clone(&self.handler),
            shared: Arc::clone(shared),
            budget: Budget::new(config.resource_limits.max_memory),
            connections: TaskTracker::new(),
            next_id: AtomicU64::new(1),
            sessions: Arc::new(AtomicU64::new(1)),
        });
        #[cfg(unix)]
        let _signals = AbortOnDrop(tokio::spawn(signals.watch(Arc::clone(shared))));
        let accepting = TaskTracker::new();
        let mut set = Listeners::new(Arc::clone(&server), accepting.clone());
        for (address, listener) in listeners {
            set.start(address, listener);
        }
        let coordinator = Coordinator::new(
            self.loader,
            set,
            generation,
            shared.stop.clone(),
            Arc::clone(&shared.counters),
        );
        let coordinating = tokio::spawn(coordinator.run(self.requests));

        shared.stop.cancelled().await;
        // A reload that is applying finishes before the drain starts.
        if let Err(error) = coordinating.await {
            if error.is_panic() {
                std::panic::resume_unwind(error.into_panic());
            }
        }
        accepting.close();
        accepting.wait().await;
        shared.counters.set_accepting(false);
        server.connections.close();
        drain(shared, &server.connections).await;

        let counters = &shared.counters;
        tracing::info!(
            uptime_s = counters.uptime().as_secs(),
            total_connections = counters.accepted.load(Ordering::SeqCst),
            total_bytes_read = counters.traffic.bytes_received(),
            total_bytes_written = counters.traffic.bytes_sent(),
            "Shutdown complete"
        );
        shared.running.store(false, Ordering::SeqCst);
        shared.stopped.cancel();
        Ok(())
    }
}

/// Aborts a task when dropped, so it ends with `run()`.
#[cfg(unix)]
struct AbortOnDrop(tokio::task::JoinHandle<()>);

#[cfg(unix)]
impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Steps 4 to 7 of the shutdown sequence, once the listeners are closed:
/// the grace period, then the drain, then the forced close of what is left.
/// A forced shutdown skips the first two. The timings are those of the
/// current generation.
async fn drain<A>(shared: &Shared<A>, connections: &TaskTracker) {
    let (_, config) = shared.watch.server();
    let timings = &config.shutdown;
    let force = &shared.force;
    if !force.is_cancelled() {
        tracing::info!(
            grace_period_ms = millis(timings.grace_period),
            "Shutdown grace period started"
        );
        // Commands on their way can still arrive; with no connection open,
        // none can.
        tokio::select! {
            biased;
            () = force.cancelled() => {}
            () = connections.wait() => {}
            () = tokio::time::sleep(timings.grace_period) => {}
        }
    }
    shared.shutdown.cancel();
    if !force.is_cancelled() {
        tracing::info!(
            active_connections = shared.counters.connections.load(Ordering::SeqCst),
            drain_timeout_ms = millis(timings.drain_timeout),
            "Shutdown draining started"
        );
        tokio::select! {
            biased;
            () = force.cancelled() => {}
            () = connections.wait() => {}
            () = tokio::time::sleep(timings.drain_timeout) => {}
        }
    }
    force.cancel();
    connections.wait().await;
}

fn millis(duration: std::time::Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
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
    shared: Arc<Shared<A>>,
    budget: Arc<Budget>,
    connections: TaskTracker,
    next_id: AtomicU64,
    /// The next `milter.session.id`.
    sessions: Arc<AtomicU64>,
}

/// Records a connection's duration, with the `otel` feature.
fn record_duration<A>(shared: &Shared<A>, accepted_at: Instant, reason: &'static str) {
    #[cfg(feature = "otel")]
    shared
        .instruments
        .histograms
        .connection(accepted_at.elapsed(), reason);
    #[cfg(not(feature = "otel"))]
    let _ = (shared, accepted_at, reason);
}

impl<H: Handler, A: Send + Sync + 'static> Accepting<H, A> {
    /// Takes on an accepted connection with the current configuration, or
    /// closes it at once when a limit is reached.
    fn connection(
        &self,
        stream: TcpStream,
        peer_addr: SocketAddr,
        listener: SocketAddr,
        counts: &Arc<ListenerCounts>,
    ) {
        let accepted_at = Instant::now();
        let (generation, config) = self.shared.watch.server();
        let counters = &self.shared.counters;
        counters.accepted.fetch_add(1, Ordering::Relaxed);
        counts.accepted();
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let span = tracing::info_span!(
            "milter.connection",
            otel.kind = "server",
            network.transport = "tcp",
            network.type = if peer_addr.is_ipv4() { "ipv4" } else { "ipv6" },
            network.peer.address = %peer_addr.ip(),
            network.peer.port = i64::from(peer_addr.port()),
            server.address = %listener.ip(),
            server.port = i64::from(listener.port()),
            milter.connection.id = engine::attribute(id),
            milter.protocol.version = Empty,
            error.type = Empty,
            otel.status_code = Empty,
        );
        let entered = span.enter();

        let max_connections = config.max_connections;
        if counters.connections.load(Ordering::SeqCst) >= max_connections {
            self.reject(counts, accepted_at);
            tracing::warn!(
                conn_id = id,
                remote_addr = %peer_addr,
                max_connections,
                "Connection rejected (limit)"
            );
            return;
        }
        let Some(memory) = Reservation::new(&self.budget) else {
            self.reject(counts, accepted_at);
            tracing::warn!(
                limit_type = "max_memory",
                current = self.budget.used(),
                max = self.budget.max(),
                "Resource limit reached"
            );
            return;
        };
        let local_addr = match stream.local_addr() {
            Ok(local_addr) => local_addr,
            Err(error) => return self.setup_failed(accepted_at, error),
        };
        // A connection whose options cannot be set is closed before any
        // read, as a setup error.
        if let Err(error) = socket::configure(&stream, &config.socket_options) {
            tracing::debug!(conn_id = id, error = %error, "Socket setup failed");
            return self.setup_failed(accepted_at, error);
        }
        #[cfg(feature = "test-support")]
        let probe = match readback::Probe::new(&stream) {
            Ok(probe) => probe,
            Err(error) => return self.setup_failed(accepted_at, error),
        };
        let info = ConnectionInfo {
            id,
            peer_addr,
            local_addr,
            accepted_at,
            config_generation: generation,
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
        let open = Open::new(Arc::clone(&self.shared), Arc::clone(counts));
        let trace = Trace {
            sessions: Arc::clone(&self.sessions),
            commands: config.telemetry.trace_commands,
            traffic: Arc::clone(&counters.traffic),
            #[cfg(feature = "otel")]
            histograms: Arc::clone(&self.shared.instruments.histograms),
        };
        drop(entered);
        self.connections.spawn(
            serve(
                stream,
                Arc::clone(&self.handler),
                info,
                config,
                memory,
                open,
                trace,
            )
            .instrument(span),
        );
    }

    /// Counts a connection closed at once at a limit.
    fn reject(&self, counts: &ListenerCounts, accepted_at: Instant) {
        let counters = &self.shared.counters;
        counters.rejected.fetch_add(1, Ordering::Relaxed);
        counts.rejected();
        counters.closed("limit_reached", None);
        record_duration(&self.shared, accepted_at, "limit_reached");
    }

    /// Counts a connection closed because its socket could not be set up.
    fn setup_failed(&self, accepted_at: Instant, error: std::io::Error) {
        self.shared
            .counters
            .closed("error", Some(&Error::Io(error)));
        record_duration(&self.shared, accepted_at, "error");
    }
}

/// Counts a connection as open, on the server and on its listener, until
/// it is dropped.
struct Open<A> {
    shared: Arc<Shared<A>>,
    listener: Arc<ListenerCounts>,
}

impl<A> Open<A> {
    fn new(shared: Arc<Shared<A>>, listener: Arc<ListenerCounts>) -> Open<A> {
        shared.counters.connections.fetch_add(1, Ordering::SeqCst);
        listener.opened();
        Open { shared, listener }
    }
}

impl<A> Drop for Open<A> {
    fn drop(&mut self) {
        self.listener.closed();
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
    trace: Trace,
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
    let stop = Stop {
        shutdown: open.shared.shutdown.clone(),
        force: open.shared.force.clone(),
    };
    let report = engine::run(
        &*handler,
        reader,
        writer,
        Arc::new(info),
        limits,
        stop,
        trace,
    )
    .await;
    let reason = close_reason(&report.outcome);
    open.shared
        .counters
        .closed(reason, report.outcome.as_ref().err());
    record_duration(&open.shared, accepted_at, reason);
    if let (Err(error), "error") = (&report.outcome, reason) {
        let span = tracing::Span::current();
        span.record("error.type", error.variant());
        span.record("otel.status_code", "error");
    }
    log_close(id, peer_addr, accepted_at, &report, reason, &config);
    drop(open);
}

/// The `milter.close_reason` of a connection's outcome.
fn close_reason(outcome: &Result<Closed>) -> &'static str {
    match outcome {
        Ok(Closed::ByClient) => "client_closed",
        Ok(Closed::ByShutdown) => "server_shutdown",
        Err(Error::Timeout(_)) => "timeout",
        Err(Error::ConnectionReset) => "reset",
        Err(Error::ResourceLimit(_)) => "limit_reached",
        Err(_) => "error",
    }
}

/// Writes the "Connection closed" log, at WARN only for an error.
fn log_close(
    id: u64,
    peer_addr: SocketAddr,
    accepted_at: Instant,
    report: &Report,
    reason: &'static str,
    config: &ServerConfig,
) {
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
    let duration_ms = millis(accepted_at.elapsed());
    let error = match &report.outcome {
        Ok(_) => None,
        Err(error) => Some(error_chain(error)),
    };
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
    /// Cancelled when a shutdown starts: the listeners close.
    stop: CancellationToken,
    /// Cancelled when the grace period ends; the handlers' token.
    shutdown: CancellationToken,
    /// Cancelled to force-close every connection.
    force: CancellationToken,
    stopped: CancellationToken,
    running: AtomicBool,
    counters: Arc<Counters>,
    watch: ConfigWatch<A>,
    publisher: watch::Publisher<A>,
    /// The reload coordinator's requests.
    requests: mpsc::UnboundedSender<Request<A>>,
    has_loader: bool,
    #[cfg(feature = "otel")]
    instruments: otel::Instruments,
}

impl<A> ServerHandle<A> {
    /// Starts a graceful shutdown: the listeners close at once, and after
    /// `shutdown.grace_period` each connection closes when it waits for the
    /// MTA's next command. Does nothing once a shutdown has started.
    pub fn shutdown(&self) {
        if self.inner.stop.is_cancelled() {
            return;
        }
        tracing::info!(reason = "manual", force = false, "Shutdown requested");
        self.inner.begin();
    }

    /// Shuts down without the grace period or the drain: every connection
    /// closes at once, and the callbacks in progress are dropped. Also ends
    /// a graceful shutdown in progress.
    pub fn force_shutdown(&self) {
        if self.inner.force.is_cancelled() {
            return;
        }
        tracing::info!(reason = "manual", force = true, "Shutdown requested");
        self.inner.force();
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

    /// The current server configuration.
    pub fn config(&self) -> Arc<ServerConfig> {
        self.inner.watch.server().1
    }

    /// The current configuration generation.
    pub fn config_generation(&self) -> u64 {
        self.inner.watch.generation()
    }

    /// Runs the registered loader and applies its result, as `SIGHUP`
    /// does. Requests made while a reload runs are folded into one more
    /// reload. Called before `run()`, it waits until `run()` has bound the
    /// listeners.
    pub async fn reload(&self) -> std::result::Result<ReloadReport, ReloadError> {
        let outcome = self.load().await;
        self.counted(outcome)
    }

    async fn load(&self) -> Outcome {
        if self.inner.stop.is_cancelled() {
            return Err(ReloadError::ShuttingDown);
        }
        if !self.inner.has_loader {
            return Err(ReloadError::NoLoader);
        }
        tracing::info!(
            source = "api",
            generation = self.config_generation(),
            "Reload requested"
        );
        let (reply, outcome) = oneshot::channel();
        self.request(Request::Load(Some(reply)), outcome).await
    }

    /// Counts the result of a reload request made through the API.
    fn counted(&self, outcome: Outcome) -> Outcome {
        self.inner.counters.reloaded(outcome.is_ok(), Source::Api);
        outcome
    }

    /// Applies a configuration the caller built, with the same checks as a
    /// reload through the loader.
    pub async fn reload_with_config(
        &self,
        new: Reloaded<A>,
    ) -> std::result::Result<ReloadReport, ReloadError> {
        let outcome = if self.inner.stop.is_cancelled() {
            Err(ReloadError::ShuttingDown)
        } else {
            tracing::info!(
                source = "api",
                generation = self.config_generation(),
                "Reload requested"
            );
            let (reply, outcome) = oneshot::channel();
            self.request(Request::Apply(Box::new(new), reply), outcome)
                .await
        };
        self.counted(outcome)
    }

    /// Sends a request to the coordinator and waits for its outcome. A
    /// coordinator that has stopped, or a server dropped without running,
    /// answers `ShuttingDown`.
    async fn request(&self, request: Request<A>, outcome: oneshot::Receiver<Outcome>) -> Outcome {
        if self.inner.requests.send(request).is_err() {
            return Err(ReloadError::ShuttingDown);
        }
        outcome.await.unwrap_or(Err(ReloadError::ShuttingDown))
    }
}

impl<A> Shared<A> {
    /// Starts a graceful shutdown.
    fn begin(&self) {
        self.counters.begin_shutdown();
        self.stop.cancel();
    }

    /// Forces a shutdown, or turns a graceful one into a forced one.
    fn force(&self) {
        self.force.cancel();
        self.begin();
    }
}

/// What the handler factory receives at build time.
pub struct HandlerContext<A> {
    watch: ConfigWatch<A>,
    shutdown: CancellationToken,
}

impl<A> HandlerContext<A> {
    /// Read access to the application configuration.
    pub fn config_watch(&self) -> ConfigWatch<A> {
        self.watch.clone()
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
