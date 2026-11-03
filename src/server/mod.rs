//! The server: listeners, one task per connection, and the handle that
//! stops it.
//!
//! This is the minimal server the protocol engine needs. Socket options,
//! timeouts, bind modes, resource limits, status, signals, and reload are
//! specified in `docs/reference/tcp-server.md` and `hot-reload.md`.

mod watch;

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

pub use watch::ConfigWatch;

use crate::{Error, Handler, Result, ServerConfig, engine};

/// The milter server, generic over the handler `H` and the application
/// configuration `A`.
pub struct Server<H, A = ()> {
    config: ServerConfig,
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
        Ok(Server {
            config: self.config,
            handler: Arc::new(handler),
            handle: ServerHandle {
                inner: Arc::new(Shared {
                    shutdown,
                    stopped: CancellationToken::new(),
                    running: AtomicBool::new(false),
                    watch,
                    _publisher: publisher,
                }),
            },
        })
    }
}

impl<H: Handler, A: Send + Sync + 'static> Server<H, A> {
    /// A handle that controls the server. Can be called before `run`.
    pub fn handle(&self) -> ServerHandle<A> {
        self.handle.clone()
    }

    /// Binds the listeners and serves until shutdown.
    pub async fn run(self) -> Result<()> {
        let mut listeners = Vec::new();
        for &addr in &self.config.bind_addresses {
            let listener = TcpListener::bind(addr)
                .await
                .map_err(|source| Error::Bind { addr, source })?;
            listeners.push(listener);
        }
        let shared = &self.handle.inner;
        shared.running.store(true, Ordering::SeqCst);

        let connections = TaskTracker::new();
        let next_id = Arc::new(AtomicU64::new(1));
        let accepting = TaskTracker::new();
        for listener in listeners {
            accepting.spawn(accept(
                listener,
                Arc::clone(&self.handler),
                self.config.resource_limits.per_connection_buffer,
                shared.shutdown.clone(),
                connections.clone(),
                Arc::clone(&next_id),
            ));
        }
        accepting.close();
        accepting.wait().await;
        connections.close();
        connections.wait().await;

        shared.running.store(false, Ordering::SeqCst);
        shared.stopped.cancel();
        Ok(())
    }
}

/// Accepts connections on one listener until shutdown.
async fn accept<H: Handler>(
    listener: TcpListener,
    handler: Arc<H>,
    max_packet: usize,
    shutdown: CancellationToken,
    connections: TaskTracker,
    next_id: Arc<AtomicU64>,
) {
    loop {
        let accepted = tokio::select! {
            () = shutdown.cancelled() => return,
            accepted = listener.accept() => accepted,
        };
        let (stream, peer_addr) = match accepted {
            Ok(accepted) => accepted,
            Err(_) => {
                // Errors such as EMFILE pass; waiting a little keeps the
                // loop from spinning on them.
                tokio::time::sleep(Duration::from_millis(10)).await;
                continue;
            }
        };
        let Ok(local_addr) = stream.local_addr() else {
            continue;
        };
        let info = ConnectionInfo {
            id: next_id.fetch_add(1, Ordering::Relaxed),
            peer_addr,
            local_addr,
            accepted_at: Instant::now(),
            config_generation: 0,
        };
        connections.spawn(serve(
            stream,
            Arc::clone(&handler),
            info,
            max_packet,
            shutdown.clone(),
        ));
    }
}

async fn serve<H: Handler>(
    stream: TcpStream,
    handler: Arc<H>,
    info: ConnectionInfo,
    max_packet: usize,
    shutdown: CancellationToken,
) {
    let (reader, writer) = stream.into_split();
    // The connection's error is reported by telemetry, which is not built
    // yet; the handler has already seen `session_end`.
    let _ = engine::run(
        &*handler,
        reader,
        writer,
        Arc::new(info),
        max_packet,
        shutdown,
    )
    .await;
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
    watch: ConfigWatch<A>,
    _publisher: watch::Publisher<A>,
}

impl<A> ServerHandle<A> {
    /// Starts a shutdown: the listeners close, and each connection closes
    /// when it waits for the MTA's next command.
    pub fn shutdown(&self) {
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
}
