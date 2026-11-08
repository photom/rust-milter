//! The integration scenarios of `docs/testing/reload.md`. The unit
//! scenarios are next to the code, in `src/server/reload.rs`,
//! `src/server/listeners.rs`, and `src/config/reload.rs`; the `SIGHUP`
//! scenarios RLD-043 and RLD-044 are in `shutdown_signals.rs`.

mod support;

use std::collections::VecDeque;
use std::io;
use std::net::{SocketAddr, TcpListener};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rust_milter::protocol::*;
use rust_milter::{
    BindMode, Bytes, CancellationToken, ConfigError, ConfigWatch, Decision, EomContext, Handler,
    MessageContext, MilterText, ReloadError, ReloadLoader, Reloaded, Result, Server, ServerBuilder,
    ServerConfig, ServerHandle,
};
use support::logs::Logs;
use support::mta::{Mta, connect_data, strings};
use support::ports::PortLease;
use support::{ContinueAll, quick_shutdown};

/// Prepared loader results, returned one per call.
type Script<A> = Arc<Mutex<VecDeque<std::result::Result<Reloaded<A>, io::Error>>>>;

fn script<A>(results: Vec<std::result::Result<Reloaded<A>, io::Error>>) -> Script<A> {
    Arc::new(Mutex::new(results.into()))
}

/// The server configuration a reload returns, with an empty application
/// configuration.
fn plain(server: ServerConfig) -> std::result::Result<Reloaded<()>, io::Error> {
    Ok(Reloaded { server, app: () })
}

/// The default configuration on `addresses`, with no grace period.
fn on(addresses: &[SocketAddr]) -> ServerConfig {
    ServerConfig {
        bind_addresses: addresses.to_vec(),
        shutdown: quick_shutdown(),
        ..ServerConfig::default()
    }
}

/// A loader that returns the prepared results in turn.
struct ScriptLoader<A> {
    script: Script<A>,
}

impl<A: Send + Sync + 'static> ReloadLoader<A> for ScriptLoader<A> {
    type Error = io::Error;

    async fn load(&self) -> std::result::Result<Reloaded<A>, io::Error> {
        let next = self.script.lock().unwrap().pop_front();
        match next {
            Some(next) => next,
            None => panic!("a prepared configuration"),
        }
    }
}

/// A running server, built with an optional loader.
struct Running<A = ()> {
    handle: ServerHandle<A>,
    task: tokio::task::JoinHandle<Result<()>>,
}

/// A builder with `config`, `app`, and the loader, if there is one; the
/// caller adds the handler.
fn builder<A: Send + Sync + 'static>(
    config: ServerConfig,
    app: A,
    loader: Option<Script<A>>,
) -> ServerBuilder<(), A> {
    let builder = Server::builder().config(config).app_config(app);
    match loader {
        Some(script) => builder.reload_with(ScriptLoader { script }),
        None => builder,
    }
}

/// Starts a server with `handler`, built as [`builder`] says.
async fn start<A: Send + Sync + 'static, H: Handler>(
    config: ServerConfig,
    app: A,
    loader: Option<Script<A>>,
    handler: H,
) -> Running<A> {
    let server = builder(config, app, loader)
        .handler(move |_| handler)
        .build()
        .expect("a valid configuration");
    run(server).await
}

/// Runs `server` and waits until it accepts.
async fn run<A: Send + Sync + 'static, H: Handler>(server: Server<H, A>) -> Running<A> {
    let handle = server.handle();
    let task = tokio::spawn(server.run());
    while !handle.status().accepting {
        assert!(!task.is_finished(), "run() ended early");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Running { handle, task }
}

impl<A> Running<A> {
    async fn stop(self) {
        self.handle.shutdown();
        tokio::time::timeout(support::mta::WAIT, self.task)
            .await
            .expect("the server did not stop in time")
            .unwrap()
            .unwrap();
    }
}

/// Whether a connection to `addr` is accepted now.
async fn accepts(addr: SocketAddr) -> bool {
    tokio::net::TcpStream::connect(addr).await.is_ok()
}

/// Holds `addr` bound, so the server cannot bind it.
fn occupy(addr: SocketAddr) -> TcpListener {
    TcpListener::bind(addr).expect("the test can bind its leased port")
}

/// Waits until the server counts `count` open connections.
async fn open_connections<A>(handle: &ServerHandle<A>, count: usize) {
    let deadline = Instant::now() + support::mta::WAIT;
    while handle.status().connections != count {
        assert!(Instant::now() < deadline, "{count} connections in time");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

#[tokio::test]
async fn rld_010_a_reload_reports_the_changed_settings() {
    let (logs, _guard) = Logs::capture();
    let lease = PortLease::acquire().await;
    let config = on(&[lease.v4()]);
    let new = ServerConfig {
        max_connections: 500,
        ..config.clone()
    };
    let server = start(config, (), Some(script(vec![plain(new)])), ContinueAll).await;

    let report = server.handle.reload().await.unwrap();

    assert_eq!(report.generation, 2);
    assert_eq!(report.changed, ["max_connections"]);
    assert_eq!(server.handle.status().config_generation, 2);
    assert_eq!(server.handle.config_generation(), 2);
    assert_eq!(server.handle.config().max_connections, 500);
    let applied = logs.one("Reload applied");
    assert_eq!(applied.field("generation"), Some("2"));
    assert_eq!(applied.field("changed"), Some(r#"["max_connections"]"#));
    let requested = logs.one("Reload requested");
    assert_eq!(requested.field("source"), Some("api"));
    assert_eq!(requested.field("generation"), Some("1"));
    server.stop().await;
}

#[tokio::test]
async fn rld_011_open_connections_keep_their_timeouts() {
    let lease = PortLease::acquire().await;
    let mut config = on(&[lease.v4()]);
    config.timeouts.idle = Duration::from_secs(1);
    let mut new = config.clone();
    new.timeouts.idle = Duration::from_secs(3);
    let server = start(config, (), Some(script(vec![plain(new)])), ContinueAll).await;
    let mut first = Mta::connect(lease.v4()).await;
    first.negotiate().await;
    let first_idle = Instant::now();

    server.handle.reload().await.unwrap();
    let mut second = Mta::connect(lease.v4()).await;
    second.negotiate().await;
    let second_idle = Instant::now();

    let (first_closed, second_closed) = tokio::join!(
        async {
            assert!(first.closes().await);
            first_idle.elapsed()
        },
        async {
            assert!(second.closes().await);
            second_idle.elapsed()
        }
    );
    assert!(
        (Duration::from_millis(900)..Duration::from_millis(2_000)).contains(&first_closed),
        "the first connection closed after {first_closed:?}"
    );
    assert!(
        (Duration::from_millis(2_900)..Duration::from_millis(4_500)).contains(&second_closed),
        "the second connection closed after {second_closed:?}"
    );
    server.stop().await;
}

#[tokio::test]
async fn rld_012_a_lower_limit_closes_no_open_connection() {
    let lease = PortLease::acquire().await;
    let mut config = on(&[lease.v4()]);
    config.max_connections = 3;
    let new = ServerConfig {
        max_connections: 1,
        ..config.clone()
    };
    let server = start(config, (), Some(script(vec![plain(new)])), ContinueAll).await;
    let mut open = Vec::new();
    for _ in 0..3 {
        let mut mta = Mta::connect(lease.v4()).await;
        mta.negotiate().await;
        open.push(mta);
    }

    server.handle.reload().await.unwrap();

    for mta in &mut open {
        mta.ask_continue(
            SMFIC_CONNECT,
            connect_data("[192.0.2.1]", "192.0.2.1", 4711),
        )
        .await;
    }
    while let Some(mta) = open.pop() {
        mta.close().await;
        open_connections(&server.handle, open.len()).await;
        let mut new = Mta::connect(lease.v4()).await;
        if open.is_empty() {
            new.negotiate().await;
        } else {
            assert!(new.closes().await, "with {} still open", open.len());
        }
    }
    server.stop().await;
}

#[tokio::test]
async fn rld_013_a_backlog_change_fails_and_applies_nothing() {
    let (logs, _guard) = Logs::capture();
    let lease = PortLease::acquire().await;
    let config = ServerConfig {
        max_connections: 2,
        ..on(&[lease.v4()])
    };
    let new = ServerConfig {
        backlog: 2048,
        max_connections: 1,
        ..config.clone()
    };
    let server = start(config, (), Some(script(vec![plain(new)])), ContinueAll).await;

    let error = server.handle.reload().await.unwrap_err();

    let ReloadError::RestartRequired { settings } = &error else {
        panic!("{error:?}");
    };
    assert_eq!(settings, &["backlog"]);
    let message = "changing backlog requires a restart; nothing was applied";
    assert_eq!(error.to_string(), message);
    let failed = logs.one("Reload failed");
    assert_eq!(failed.level, tracing::Level::ERROR);
    assert_eq!(failed.field("error"), Some(message));
    assert_eq!(server.handle.config_generation(), 1);
    let mut first = Mta::connect(lease.v4()).await;
    first.negotiate().await;
    let mut second = Mta::connect(lease.v4()).await;
    second.negotiate().await;
    server.stop().await;
}

#[tokio::test]
async fn rld_014_a_reuse_port_change_fails_even_with_an_added_address() {
    let (a, b) = (PortLease::acquire().await, PortLease::acquire().await);
    let config = on(&[a.v4()]);
    let mut new = on(&[a.v4(), b.v4()]);
    new.socket_options.reuse_port = true;
    let server = start(config, (), Some(script(vec![plain(new)])), ContinueAll).await;

    let error = server.handle.reload().await.unwrap_err();

    assert!(
        matches!(&error, ReloadError::RestartRequired { settings }
            if settings == &["socket_options.reuse_port"]),
        "{error:?}"
    );
    assert!(!accepts(b.v4()).await, "B was bound");
    let mut listeners = Vec::new();
    for listener in server.handle.status().listeners {
        listeners.push(listener.address);
    }
    assert_eq!(listeners, [a.v4()]);
    server.stop().await;
}

#[tokio::test]
async fn rld_015_restart_settings_are_named_in_document_order() {
    let lease = PortLease::acquire().await;
    let config = on(&[lease.v4()]);
    let mut changed = ServerConfig {
        backlog: 2048,
        max_connections: 500,
        ..config.clone()
    };
    changed.socket_options.reuse_addr = false;
    changed.socket_options.reuse_port = true;
    changed.socket_options.ipv6_only = true;
    let reverted = ServerConfig {
        max_connections: 500,
        ..config.clone()
    };
    let server = start(
        config,
        (),
        Some(script(vec![plain(changed), plain(reverted)])),
        ContinueAll,
    )
    .await;

    let error = server.handle.reload().await.unwrap_err();
    let report = server.handle.reload().await.unwrap();

    assert!(
        matches!(&error, ReloadError::RestartRequired { settings } if settings == &[
            "backlog",
            "socket_options.reuse_addr",
            "socket_options.reuse_port",
            "socket_options.ipv6_only",
        ]),
        "{error:?}"
    );
    assert_eq!(report.generation, 2);
    assert_eq!(report.changed, ["max_connections"]);
    server.stop().await;
}

#[tokio::test]
async fn rld_016_logs_name_the_settings_without_their_values() {
    let (logs, _guard) = Logs::capture();
    let lease = PortLease::acquire().await;
    let config = ServerConfig {
        max_connections: 777_777,
        ..on(&[lease.v4()])
    };
    let new = ServerConfig {
        max_connections: 888_888,
        ..config.clone()
    };
    let server = start(config, (), Some(script(vec![plain(new)])), ContinueAll).await;

    server.handle.reload().await.unwrap();
    server.stop().await;

    let mut text = String::new();
    for event in logs.all() {
        text.push_str(&format!("{} {:?}\n", event.message, event.fields));
    }
    assert!(text.contains("max_connections"), "{text}");
    assert!(!text.contains("777777"), "{text}");
    assert!(!text.contains("888888"), "{text}");
}

#[tokio::test]
async fn rld_050_a_new_memory_limit_applies_to_the_next_accept() {
    let lease = PortLease::acquire().await;
    let config = on(&[lease.v4()]);
    let mut new = config.clone();
    new.resource_limits.max_memory = Some(65_540);
    new.resource_limits.per_connection_buffer = 65_540;
    let server = start(config, (), Some(script(vec![plain(new)])), ContinueAll).await;

    server.handle.reload().await.unwrap();

    let mut open = Vec::new();
    for _ in 0..16 {
        let mut mta = Mta::connect(lease.v4()).await;
        mta.negotiate().await;
        open.push(mta);
    }
    let mut over = Mta::connect(lease.v4()).await;
    assert!(over.closes().await, "the 17th connection passes max_memory");
    assert_eq!(server.handle.status().connections_rejected, 1);
    drop(open);
    server.stop().await;
}

#[tokio::test]
async fn rld_051_a_shutdown_uses_the_current_timings() {
    let lease = PortLease::acquire().await;
    let config = on(&[lease.v4()]);
    let mut new = config.clone();
    new.shutdown.grace_period = Duration::from_millis(500);
    let server = start(config, (), Some(script(vec![plain(new)])), ContinueAll).await;
    let mut mta = Mta::connect(lease.v4()).await;
    mta.reach(support::mta::Reach::Helo).await;

    server.handle.reload().await.unwrap();
    server.handle.shutdown();
    let started = Instant::now();

    tokio::time::sleep(Duration::from_millis(200)).await;
    mta.ask_continue(SMFIC_MAIL, strings(&["<sender@void.invalid>"]))
        .await;
    assert!(mta.closes().await);
    let closed = started.elapsed();
    assert!(
        (Duration::from_millis(450)..Duration::from_millis(1_500)).contains(&closed),
        "closed after {closed:?}"
    );
    server.stop().await;
}

#[tokio::test]
async fn rld_020_moving_the_listen_address_leaves_no_gap() {
    let (a, b) = (PortLease::acquire().await, PortLease::acquire().await);
    let (a, b, _leases) = (a.v4(), b.v4(), (a, b));
    let server = start(
        on(&[a]),
        (),
        Some(script(vec![plain(on(&[b]))])),
        ContinueAll,
    )
    .await;
    let mut open = Mta::connect(a).await;
    open.negotiate().await;
    let done = CancellationToken::new();
    let probing = tokio::spawn({
        let done = done.clone();
        async move {
            let mut gaps = 0;
            let mut probes = 0;
            while !done.is_cancelled() {
                if !accepts(a).await && !accepts(b).await {
                    gaps += 1;
                }
                probes += 1;
                tokio::task::yield_now().await;
            }
            (gaps, probes)
        }
    });

    let report = server.handle.reload().await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    done.cancel();

    let (gaps, probes) = probing.await.unwrap();
    assert_eq!(gaps, 0, "in {probes} probes");
    assert_eq!(report.listeners_added, [b]);
    assert_eq!(report.listeners_removed, [a]);
    assert!(!accepts(a).await, "A refuses new connections");
    open.ask_continue(
        SMFIC_CONNECT,
        connect_data("[192.0.2.1]", "192.0.2.1", 4711),
    )
    .await;
    server.stop().await;
}

#[tokio::test]
async fn rld_021_fail_fast_rolls_back_a_failed_bind() {
    let leases = [
        PortLease::acquire().await,
        PortLease::acquire().await,
        PortLease::acquire().await,
    ];
    let [a, b, c] = [leases[0].v4(), leases[1].v4(), leases[2].v4()];
    let _held = occupy(c);
    let server = start(
        on(&[a]),
        (),
        Some(script(vec![plain(on(&[a, b, c]))])),
        ContinueAll,
    )
    .await;

    let error = server.handle.reload().await.unwrap_err();

    assert!(
        matches!(error, ReloadError::Bind { addr, .. } if addr == c),
        "{error:?}"
    );
    assert!(!accepts(b).await, "B is closed again");
    assert_eq!(server.handle.config_generation(), 1);
    Mta::connect(a).await.negotiate().await;
    server.stop().await;
}

#[tokio::test]
async fn rld_022_best_effort_publishes_the_listeners_that_bound() {
    let (logs, _guard) = Logs::capture();
    let leases = [
        PortLease::acquire().await,
        PortLease::acquire().await,
        PortLease::acquire().await,
    ];
    let [a, b, c] = [leases[0].v4(), leases[1].v4(), leases[2].v4()];
    let _held = occupy(c);
    let new = ServerConfig {
        bind_mode: BindMode::BestEffort,
        ..on(&[a, b, c])
    };
    let server = start(on(&[a]), (), Some(script(vec![plain(new)])), ContinueAll).await;

    let report = server.handle.reload().await.unwrap();

    assert_eq!(report.generation, 2);
    assert_eq!(report.listeners_added, [b]);
    let mut failed = Vec::new();
    for (addr, _) in &report.bind_failures {
        failed.push(*addr);
    }
    assert_eq!(failed, [c]);
    let warning = logs.one("Listener bind failed");
    assert_eq!(warning.level, tracing::Level::WARN);
    assert_eq!(warning.field("address"), Some(c.to_string().as_str()));
    let listeners = server.handle.status().listeners;
    let mut bound = Vec::new();
    for listener in &listeners {
        bound.push((listener.address, listener.bound));
    }
    assert_eq!(bound, [(a, true), (b, true), (c, false)]);
    Mta::connect(b).await.negotiate().await;
    server.stop().await;
}

#[tokio::test]
async fn rld_023_a_reload_never_leaves_no_listener() {
    let (a, c) = (PortLease::acquire().await, PortLease::acquire().await);
    let (a, c, _leases) = (a.v4(), c.v4(), (a, c));
    let _held = occupy(c);
    let new = ServerConfig {
        bind_mode: BindMode::BestEffort,
        ..on(&[c])
    };
    let server = start(on(&[a]), (), Some(script(vec![plain(new)])), ContinueAll).await;

    let error = server.handle.reload().await.unwrap_err();

    assert!(matches!(error, ReloadError::NoListener), "{error:?}");
    assert_eq!(server.handle.config_generation(), 1);
    Mta::connect(a).await.negotiate().await;
    server.stop().await;
}

/// Reads the application configuration in `mail` and records the value it
/// read in `end_of_message`.
struct Threshold {
    config: ConfigWatch<u32>,
    seen: Arc<Mutex<Vec<u32>>>,
}

impl Handler for Threshold {
    type Session = ();
    type Message = Option<Arc<u32>>;

    async fn mail(
        &self,
        ctx: &mut MessageContext<Self>,
        _sender: &MilterText,
        _args: &[MilterText],
    ) -> Result<Decision> {
        *ctx.message() = Some(self.config.current());
        Ok(Decision::Continue)
    }

    async fn end_of_message(&self, ctx: &mut EomContext<Self>) -> Result<Decision> {
        let value = ctx.message().clone().expect("read in mail");
        self.seen.lock().unwrap().push(*value);
        Ok(Decision::Continue)
    }
}

/// One message from MAIL to end of message, with a step in between.
async fn message(mta: &mut Mta, between: impl std::future::Future<Output = ()>) {
    mta.ask_continue(SMFIC_MAIL, strings(&["<sender@void.invalid>"]))
        .await;
    between.await;
    mta.ask_continue(SMFIC_RCPT, strings(&["<rcpt@void.invalid>"]))
        .await;
    mta.ask_continue(SMFIC_BODYEOB, Bytes::new()).await;
}

#[tokio::test]
async fn rld_030_a_message_sees_one_application_configuration() {
    let lease = PortLease::acquire().await;
    let config = on(&[lease.v4()]);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&seen);
    let server = builder(config.clone(), 1_u32, None)
        .handler(move |ctx| Threshold {
            config: ctx.config_watch(),
            seen: recorded,
        })
        .build()
        .expect("a valid configuration");
    let server = run(server).await;
    let mut mta = Mta::connect(lease.v4()).await;
    mta.reach(support::mta::Reach::Helo).await;

    let handle = server.handle.clone();
    message(&mut mta, async {
        handle
            .reload_with_config(Reloaded {
                server: config,
                app: 2,
            })
            .await
            .unwrap();
    })
    .await;
    message(&mut mta, async {}).await;

    assert_eq!(*seen.lock().unwrap(), [1, 2]);
    server.stop().await;
}

#[tokio::test]
async fn rld_031_a_waiting_task_sees_the_new_generation() {
    let lease = PortLease::acquire().await;
    let config = on(&[lease.v4()]);
    let server = start(config.clone(), 1_u32, None, ContinueAll).await;
    let mut watch = server.handle.config_watch();
    let waiting = tokio::spawn(async move {
        watch.changed().await;
        (*watch.current(), watch.generation())
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!waiting.is_finished());

    server
        .handle
        .reload_with_config(Reloaded {
            server: config,
            app: 7,
        })
        .await
        .unwrap();

    assert_eq!(waiting.await.unwrap(), (7, 2));
    server.stop().await;
}

#[tokio::test]
async fn rld_032_an_application_check_failure_keeps_both_configurations() {
    let lease = PortLease::acquire().await;
    let config = on(&[lease.v4()]);
    let rejected = io::Error::other("threshold must be below 100");
    let server = start(
        config,
        1_u32,
        Some(script(vec![Err(rejected)])),
        ContinueAll,
    )
    .await;

    let error = server.handle.reload().await.unwrap_err();

    assert!(matches!(error, ReloadError::Load(_)), "{error:?}");
    assert_eq!(server.handle.config_generation(), 1);
    assert_eq!(*server.handle.config_watch().current(), 1);
    server.stop().await;
}

#[tokio::test]
async fn rld_033_a_rejected_reload_publishes_no_application_configuration() {
    let lease = PortLease::acquire().await;
    let config = on(&[lease.v4()]);
    let rejected = Reloaded {
        server: ServerConfig {
            backlog: 2048,
            ..config.clone()
        },
        app: 2_u32,
    };
    let server = start(config, 1_u32, Some(script(vec![Ok(rejected)])), ContinueAll).await;
    let mut watch = server.handle.config_watch();
    let waiting = tokio::spawn(async move { watch.changed().await });

    let error = server.handle.reload().await.unwrap_err();

    assert!(
        matches!(error, ReloadError::RestartRequired { .. }),
        "{error:?}"
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!waiting.is_finished(), "changed() completed");
    assert_eq!(*server.handle.config_watch().current(), 1);
    waiting.abort();
    server.stop().await;
}

#[tokio::test]
async fn rld_040_an_invalid_configuration_is_rejected() {
    let (logs, _guard) = Logs::capture();
    let lease = PortLease::acquire().await;
    let config = on(&[lease.v4()]);
    let new = ServerConfig {
        backlog: 0,
        ..config.clone()
    };
    let server = start(config, (), Some(script(vec![plain(new)])), ContinueAll).await;

    let error = server.handle.reload().await.unwrap_err();

    assert!(
        matches!(
            error,
            ReloadError::Validation(ConfigError::OutOfRange {
                field: "backlog",
                ..
            })
        ),
        "{error:?}"
    );
    assert_eq!(server.handle.config_generation(), 1);
    let failed = logs.one("Reload failed");
    assert_eq!(failed.level, tracing::Level::ERROR);
    assert_eq!(failed.field("generation"), Some("1"));
    server.stop().await;
}

#[tokio::test]
async fn rld_041_a_loader_error_is_kept_as_the_source() {
    let lease = PortLease::acquire().await;
    let missing = io::Error::new(io::ErrorKind::NotFound, "milter.toml");
    let server = start(
        on(&[lease.v4()]),
        (),
        Some(script(vec![Err(missing)])),
        ContinueAll,
    )
    .await;

    let error = server.handle.reload().await.unwrap_err();

    let source = std::error::Error::source(&error).expect("a source");
    let io = source.downcast_ref::<io::Error>().expect("the I/O error");
    assert_eq!(io.kind(), io::ErrorKind::NotFound);
    assert!(server.handle.is_running());
    Mta::connect(lease.v4()).await.negotiate().await;
    server.stop().await;
}

#[tokio::test]
async fn rld_042_without_a_loader_only_an_explicit_configuration_reloads() {
    let lease = PortLease::acquire().await;
    let config = on(&[lease.v4()]);
    let new = ServerConfig {
        max_connections: 42,
        ..config.clone()
    };
    let server = start(config, (), None, ContinueAll).await;

    let without = server.handle.reload().await.unwrap_err();
    let explicit = server
        .handle
        .reload_with_config(Reloaded {
            server: new,
            app: (),
        })
        .await
        .unwrap();

    assert!(matches!(without, ReloadError::NoLoader), "{without:?}");
    assert_eq!(explicit.changed, ["max_connections"]);
    assert_eq!(server.handle.config().max_connections, 42);
    server.stop().await;
}

#[tokio::test]
async fn rld_045_no_reload_during_shutdown() {
    let lease = PortLease::acquire().await;
    let mut config = on(&[lease.v4()]);
    config.shutdown.grace_period = Duration::from_secs(1);
    let server = start(
        config.clone(),
        (),
        Some(script(vec![plain(config.clone())])),
        ContinueAll,
    )
    .await;
    let mut mta = Mta::connect(lease.v4()).await;
    mta.negotiate().await;
    server.handle.shutdown();

    let loaded = server.handle.reload().await.unwrap_err();
    let explicit = server
        .handle
        .reload_with_config(Reloaded {
            server: config,
            app: (),
        })
        .await
        .unwrap_err();

    assert!(matches!(loaded, ReloadError::ShuttingDown), "{loaded:?}");
    assert!(
        matches!(explicit, ReloadError::ShuttingDown),
        "{explicit:?}"
    );
    assert!(server.handle.is_running(), "still in the grace period");
    drop(mta);
    server.stop().await;
}

#[tokio::test]
async fn rld_046_a_reload_before_run_waits_for_the_listeners() {
    let lease = PortLease::acquire().await;
    let config = on(&[lease.v4()]);
    let new = ServerConfig {
        max_connections: 42,
        ..config.clone()
    };
    let server = Server::builder()
        .config(config)
        .handler(ContinueAll::new)
        .build()
        .unwrap();
    let handle = server.handle();
    let reloading = tokio::spawn({
        let handle = handle.clone();
        async move {
            handle
                .reload_with_config(Reloaded {
                    server: new,
                    app: (),
                })
                .await
        }
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!reloading.is_finished(), "the reload waits for run()");

    let task = tokio::spawn(server.run());

    let report = reloading.await.unwrap().unwrap();
    assert_eq!(report.generation, 2);
    assert!(handle.status().accepting);
    handle.shutdown();
    task.await.unwrap().unwrap();
}

#[cfg(feature = "test-support")]
mod sockets {
    use rust_milter::{ConnectDecision, ConnectInfo, KeepaliveConfig, SessionContext, SessionEnd};
    use support::mta::Reach;

    use super::*;

    /// What a callback saw: its name, the socket's keepalive settings, and
    /// the connection's configuration generation.
    type Seen = (String, Option<KeepaliveConfig>, u64);

    /// Records every callback with its socket's keepalive settings.
    #[derive(Clone, Default)]
    struct Keepalive(Arc<Mutex<Vec<Seen>>>);

    impl Keepalive {
        fn read<H: Handler>(&self, callback: &str, ctx: &SessionContext<H>) {
            let connection = ctx.connection();
            let options = connection
                .read_socket_options()
                .expect("the socket options can be read");
            self.0.lock().unwrap().push((
                callback.to_owned(),
                options.keepalive,
                connection.config_generation,
            ));
        }

        fn seen(&self) -> Vec<Seen> {
            self.0.lock().unwrap().clone()
        }
    }

    impl Handler for Keepalive {
        type Session = ();
        type Message = ();

        async fn connect(
            &self,
            ctx: &mut SessionContext<Self>,
            _info: &ConnectInfo,
        ) -> Result<ConnectDecision> {
            self.read("connect", ctx);
            Ok(ConnectDecision::Continue)
        }

        async fn helo(
            &self,
            ctx: &mut SessionContext<Self>,
            _name: &MilterText,
        ) -> Result<ConnectDecision> {
            self.read("helo", ctx);
            Ok(ConnectDecision::Continue)
        }

        async fn mail(
            &self,
            ctx: &mut MessageContext<Self>,
            _sender: &MilterText,
            _args: &[MilterText],
        ) -> Result<Decision> {
            self.read("mail", ctx);
            Ok(Decision::Continue)
        }

        async fn rcpt(
            &self,
            ctx: &mut MessageContext<Self>,
            _recipient: &MilterText,
            _args: &[MilterText],
        ) -> Result<Decision> {
            self.read("rcpt", ctx);
            Ok(Decision::Continue)
        }

        async fn data(&self, ctx: &mut MessageContext<Self>) -> Result<Decision> {
            self.read("data", ctx);
            Ok(Decision::Continue)
        }

        async fn header(
            &self,
            ctx: &mut MessageContext<Self>,
            _name: &MilterText,
            _value: &MilterText,
        ) -> Result<Decision> {
            self.read("header", ctx);
            Ok(Decision::Continue)
        }

        async fn end_of_headers(&self, ctx: &mut MessageContext<Self>) -> Result<Decision> {
            self.read("end_of_headers", ctx);
            Ok(Decision::Continue)
        }

        async fn body(
            &self,
            ctx: &mut MessageContext<Self>,
            _chunk: Bytes,
        ) -> Result<rust_milter::BodyDecision> {
            self.read("body", ctx);
            Ok(rust_milter::BodyDecision::Continue)
        }

        async fn end_of_message(&self, ctx: &mut EomContext<Self>) -> Result<Decision> {
            self.read("end_of_message", ctx);
            Ok(Decision::Continue)
        }

        async fn abort(&self, ctx: &mut MessageContext<Self>) -> Result<()> {
            self.read("abort", ctx);
            Ok(())
        }

        async fn unknown(
            &self,
            ctx: &mut SessionContext<Self>,
            _command: &MilterText,
        ) -> Result<Decision> {
            self.read("unknown", ctx);
            Ok(Decision::Continue)
        }

        async fn session_end(
            &self,
            ctx: &mut SessionContext<Self>,
            reason: SessionEnd,
        ) -> Result<()> {
            self.read(&format!("session_end:{reason:?}"), ctx);
            Ok(())
        }
    }

    fn longer_keepalive(config: &ServerConfig) -> ServerConfig {
        let mut new = config.clone();
        new.socket_options.keepalive = Some(KeepaliveConfig {
            time: Duration::from_secs(120),
            interval: Duration::from_secs(20),
            retries: 3,
        });
        new
    }

    #[tokio::test]
    async fn rld_017_open_connections_keep_their_keepalive_settings() {
        let lease = PortLease::acquire().await;
        let config = on(&[lease.v4()]);
        let new = longer_keepalive(&config);
        let handler = Keepalive::default();
        let recorded = handler.clone();
        let loader = script(vec![plain(new.clone())]);
        let server = start(config, (), Some(loader), recorded).await;
        let mut first = Mta::connect(lease.v4()).await;
        first.reach(Reach::Helo).await;

        server.handle.reload().await.unwrap();
        let mut second = Mta::connect(lease.v4()).await;
        second.reach(Reach::Helo).await;
        let before = handler.seen().len();
        first
            .ask_continue(SMFIC_MAIL, strings(&["<first@void.invalid>"]))
            .await;
        second
            .ask_continue(SMFIC_MAIL, strings(&["<second@void.invalid>"]))
            .await;

        let seen = handler.seen()[before..].to_vec();
        assert_eq!(
            seen,
            [
                ("mail".to_owned(), Some(KeepaliveConfig::default()), 1),
                ("mail".to_owned(), new.socket_options.keepalive, 2),
            ]
        );
        server.stop().await;
    }

    /// Where a connection is when the reload happens.
    #[derive(Debug, Clone, Copy)]
    enum Step {
        Accepted,
        Negotiated,
        At(Reach),
    }

    /// Sends the command that follows `step`, and returns the callback it
    /// calls.
    async fn next_command(mta: &mut Mta, step: Step) -> &'static str {
        let connect = connect_data("[192.0.2.1]", "192.0.2.1", 4711);
        let (command, data, callback) = match step {
            Step::Accepted => {
                mta.negotiate().await;
                (SMFIC_CONNECT, connect, "connect")
            }
            Step::Negotiated => (SMFIC_CONNECT, connect, "connect"),
            Step::At(Reach::Conn) => (SMFIC_HELO, strings(&["client.void.invalid"]), "helo"),
            Step::At(Reach::Helo | Reach::Endm | Reach::Unkn) => {
                (SMFIC_MAIL, strings(&["<next@void.invalid>"]), "mail")
            }
            Step::At(Reach::Mail) => (SMFIC_RCPT, strings(&["<next@void.invalid>"]), "rcpt"),
            Step::At(Reach::Rcpt) => (SMFIC_DATA, Bytes::new(), "data"),
            Step::At(Reach::Data) => (SMFIC_HEADER, strings(&["Subject", "next"]), "header"),
            Step::At(Reach::Hdrs) => (SMFIC_EOH, Bytes::new(), "end_of_headers"),
            Step::At(Reach::Eohs) => (SMFIC_BODY, Bytes::from_static(b"next\r\n"), "body"),
            Step::At(Reach::Body) => (SMFIC_BODYEOB, Bytes::new(), "end_of_message"),
        };
        mta.ask_continue(command, data).await;
        callback
    }

    #[tokio::test]
    async fn rld_018_a_reload_leaves_a_connection_in_every_state_alone() {
        let steps = [
            Step::Accepted,
            Step::Negotiated,
            Step::At(Reach::Conn),
            Step::At(Reach::Helo),
            Step::At(Reach::Mail),
            Step::At(Reach::Rcpt),
            Step::At(Reach::Data),
            Step::At(Reach::Hdrs),
            Step::At(Reach::Eohs),
            Step::At(Reach::Body),
            Step::At(Reach::Endm),
            Step::At(Reach::Unkn),
        ];
        for step in steps {
            let lease = PortLease::acquire().await;
            let config = on(&[lease.v4()]);
            let mut new = longer_keepalive(&config);
            new.max_connections = 5;
            new.timeouts.idle = Duration::from_secs(60);
            let handler = Keepalive::default();
            let recorded = handler.clone();
            let server = start(config, (), Some(script(vec![plain(new)])), recorded).await;
            let mut mta = Mta::connect(lease.v4()).await;
            match step {
                Step::Accepted => {}
                Step::Negotiated => {
                    mta.negotiate().await;
                }
                Step::At(state) => mta.reach(state).await,
            }
            // The client's connect can finish before the server accepts the
            // connection. Wait until the server holds it, so that it was
            // accepted with the configuration from before the reload.
            open_connections(&server.handle, 1).await;
            let before = handler.seen().len();

            server.handle.reload().await.unwrap();
            tokio::time::sleep(Duration::from_millis(50)).await;
            assert_eq!(handler.seen().len(), before, "no callback at {step:?}");
            let callback = next_command(&mut mta, step).await;

            let seen = handler.seen()[before..].to_vec();
            assert_eq!(
                seen,
                [(callback.to_owned(), Some(KeepaliveConfig::default()), 1)],
                "{step:?}"
            );
            drop(mta);
            server.stop().await;
        }
    }
}
