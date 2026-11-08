//! Shutdown scenarios from the handle (SHT-001 to SHT-013) from
//! `docs/testing/shutdown.md`.

mod support;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rust_milter::protocol::*;
use rust_milter::{
    Bytes, Decision, EomContext, Handler, MessageContext, Result, ServerConfig, SessionContext,
    SessionEnd, ShutdownConfig,
};
use support::logs::Logs;
use support::mta::{Reach, strings};
use support::{ContinueAll, Log, Recorder, TestServer};

fn timings(grace: u64, drain: u64) -> ServerConfig {
    let mut config = support::config();
    config.shutdown.grace_period = Duration::from_millis(grace);
    config.shutdown.drain_timeout = Duration::from_millis(drain);
    config
}

fn default_timings() -> ServerConfig {
    let mut config = support::config();
    config.shutdown = ShutdownConfig::default();
    config
}

/// What `end_of_message` does.
#[derive(Clone, Copy)]
enum Eom {
    /// Sleeps this long, ignoring the shutdown token.
    Sleep(Duration),
    /// Waits for the shutdown token, then sleeps this long.
    AfterCancel(Duration),
}

/// Logs its callbacks; `end_of_message` takes its time as `eom` says.
#[derive(Clone)]
struct Slow {
    log: Log,
    eom: Eom,
}

impl Slow {
    fn new(eom: Eom) -> Slow {
        Slow {
            log: Log::default(),
            eom,
        }
    }
}

impl Handler for Slow {
    type Session = ();
    type Message = ();

    async fn end_of_message(&self, ctx: &mut EomContext<Self>) -> Result<Decision> {
        self.log.push("end_of_message");
        match self.eom {
            Eom::Sleep(time) => tokio::time::sleep(time).await,
            Eom::AfterCancel(time) => {
                ctx.shutdown().cancelled().await;
                tokio::time::sleep(time).await;
            }
        }
        self.log.push("end_of_message done");
        Ok(Decision::Continue)
    }

    async fn abort(&self, _ctx: &mut MessageContext<Self>) -> Result<()> {
        self.log.push("abort");
        Ok(())
    }

    async fn session_end(&self, _ctx: &mut SessionContext<Self>, reason: SessionEnd) -> Result<()> {
        self.log.push(format!("session_end:{reason:?}"));
        Ok(())
    }
}

/// Waits until the server has seen every connection close.
async fn no_connections(server: &TestServer) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while server.handle.status().connections > 0 {
        assert!(Instant::now() < deadline, "the closes were not seen");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// The sum of `field` over the "Connection closed" logs.
fn sum(logs: &Logs, field: &str) -> u64 {
    let mut total = 0;
    for event in logs.named("Connection closed") {
        total += event.field(field).unwrap().parse::<u64>().unwrap();
    }
    total
}

async fn recording(config: ServerConfig) -> (TestServer, Log) {
    let recorder = Recorder::default();
    let log = recorder.log.clone();
    (TestServer::start_with(config, recorder).await, log)
}

#[tokio::test]
async fn sht_001_a_server_without_connections_stops_at_once() {
    let (logs, _guard) = Logs::capture();
    let server = TestServer::start_with(default_timings(), ContinueAll).await;
    while !server.handle.status().accepting {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let started = Instant::now();

    server.handle.shutdown();
    server.handle.shutdown();

    server.join().await.expect("run() returns Ok");
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
    let requested = logs.one("Shutdown requested");
    assert_eq!(requested.level, tracing::Level::INFO);
    assert_eq!(requested.field("reason"), Some("manual"));
    assert_eq!(requested.field("force"), Some("false"));
}

#[tokio::test]
async fn sht_002_the_listeners_close_before_the_grace_period_ends() {
    let (server, _) = recording(timings(1_000, 5_000)).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Helo).await;
    let address = server.address();

    server.handle.shutdown();
    tokio::time::sleep(Duration::from_millis(100)).await;

    assert!(tokio::net::TcpStream::connect(address).await.is_err());
    server.join().await.unwrap();
}

#[tokio::test]
async fn sht_003_commands_in_the_grace_period_are_served() {
    let (server, log) = recording(timings(500, 5_000)).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Helo).await;
    let started = Instant::now();

    server.handle.shutdown();
    let status = server.handle.status();
    tokio::time::sleep(Duration::from_millis(200)).await;
    mta.ask_continue(SMFIC_MAIL, strings(&["<s@void.invalid>"]))
        .await;

    assert!(status.shutting_down && !status.accepting, "{status:?}");
    assert!(mta.closes().await);
    assert!(
        started.elapsed() >= Duration::from_millis(450),
        "{:?}",
        started.elapsed()
    );
    let events = log.wait_for("session_end:ServerShutdown").await;
    assert_eq!(
        events,
        [
            "connect",
            "helo",
            "mail",
            "abort",
            "session_end:ServerShutdown"
        ]
    );
    server.join().await.unwrap();
}

#[tokio::test]
async fn sht_004_a_callback_in_progress_completes_and_is_answered() {
    let handler = Slow::new(Eom::Sleep(Duration::from_secs(1)));
    let log = handler.log.clone();
    let server = TestServer::start_with(timings(100, 5_000), handler).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Body).await;
    mta.send(SMFIC_BODYEOB, Bytes::new()).await;
    log.wait_for("end_of_message").await;

    server.handle.shutdown();

    assert_eq!(mta.recv().await.unwrap().command, SMFIR_CONTINUE);
    assert!(mta.closes().await);
    server.join().await.unwrap();
    assert_eq!(
        log.events(),
        [
            "end_of_message",
            "end_of_message done",
            "session_end:ServerShutdown"
        ]
    );
}

#[tokio::test]
async fn sht_005_the_drain_timeout_force_closes_a_connection() {
    let handler = Slow::new(Eom::Sleep(Duration::from_secs(10)));
    let log = handler.log.clone();
    let server = TestServer::start_with(timings(100, 1_000), handler).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Body).await;
    mta.send(SMFIC_BODYEOB, Bytes::new()).await;
    log.wait_for("end_of_message").await;
    let started = Instant::now();

    server.handle.shutdown();

    assert!(mta.closes().await);
    let closed = started.elapsed();
    assert!(
        closed >= Duration::from_millis(1_000) && closed < Duration::from_millis(1_600),
        "{closed:?}"
    );
    server.join().await.unwrap();
    log.settles_at(&["end_of_message"]).await;
}

/// Records when the shutdown token is cancelled, from `end_of_message`.
#[derive(Clone, Default)]
struct WatchesToken(Arc<Mutex<Option<Instant>>>);

impl Handler for WatchesToken {
    type Session = ();
    type Message = ();

    async fn end_of_message(&self, ctx: &mut EomContext<Self>) -> Result<Decision> {
        ctx.shutdown().cancelled().await;
        *self.0.lock().unwrap() = Some(Instant::now());
        Ok(Decision::Continue)
    }
}

#[tokio::test]
async fn sht_006_the_token_is_cancelled_when_the_grace_period_ends() {
    let handler = WatchesToken::default();
    let cancelled = Arc::clone(&handler.0);
    let server = TestServer::start_with(timings(300, 5_000), handler).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Body).await;
    mta.send(SMFIC_BODYEOB, Bytes::new()).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    let started = Instant::now();

    server.handle.shutdown();

    assert_eq!(mta.recv().await.unwrap().command, SMFIR_CONTINUE);
    let after = cancelled.lock().unwrap().expect("the token was cancelled") - started;
    assert!(
        after >= Duration::from_millis(280) && after < Duration::from_millis(800),
        "{after:?}"
    );
    server.join().await.unwrap();
}

#[tokio::test]
async fn sht_007_force_shutdown_closes_every_connection_at_once() {
    let (logs, _guard) = Logs::capture();
    let (server, log) = recording(default_timings()).await;
    let mut first = server.mta().await;
    let mut second = server.mta().await;
    first.reach(Reach::Helo).await;
    second.reach(Reach::Mail).await;
    let before = log.events();
    let started = Instant::now();

    server.handle.force_shutdown();

    assert!(first.closes().await);
    assert!(second.closes().await);
    server.join().await.unwrap();
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(log.events(), before, "a forced close calls no callback");
    let requested = logs.one("Shutdown requested");
    assert_eq!(requested.field("force"), Some("true"));
}

#[tokio::test]
async fn sht_008_a_handle_answers_after_the_server_stops() {
    let server = TestServer::start(ContinueAll).await;
    let handle = server.handle.clone();
    server.stop().await;

    tokio::time::timeout(Duration::from_millis(100), handle.wait())
        .await
        .expect("wait() completes at once");
    assert!(!handle.is_running());
    let status = handle.status();
    assert!(status.shutting_down && !status.accepting, "{status:?}");
}

#[tokio::test]
async fn sht_009_logs_the_shutdown_phases() {
    let (logs, _guard) = Logs::capture();
    let server = TestServer::start_with(timings(100, 2_000), ContinueAll).await;
    for _ in 0..3 {
        let mut mta = server.mta().await;
        mta.reach(Reach::Endm).await;
        mta.send(SMFIC_QUIT, Bytes::new()).await;
        assert!(mta.closes().await);
    }
    no_connections(&server).await;

    server.stop().await;

    let grace = logs.one("Shutdown grace period started");
    assert_eq!(grace.level, tracing::Level::INFO);
    assert_eq!(grace.field("grace_period_ms"), Some("100"));
    let draining = logs.one("Shutdown draining started");
    assert_eq!(draining.level, tracing::Level::INFO);
    assert_eq!(draining.field("active_connections"), Some("0"));
    assert_eq!(draining.field("drain_timeout_ms"), Some("2000"));
    let complete = logs.one("Shutdown complete");
    assert_eq!(complete.level, tracing::Level::INFO);
    assert!(complete.field("uptime_s").is_some());
    assert_eq!(complete.field("total_connections"), Some("3"));
    assert_eq!(
        complete.field("total_bytes_read"),
        Some(sum(&logs, "bytes_read").to_string().as_str())
    );
    assert_eq!(
        complete.field("total_bytes_written"),
        Some(sum(&logs, "bytes_written").to_string().as_str())
    );
    assert_ne!(complete.field("total_bytes_read"), Some("0"));
}

#[tokio::test]
async fn sht_010_a_frame_in_progress_is_finished_and_answered() {
    let (server, log) = recording(timings(300, 5_000)).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Helo).await;
    let data = strings(&["<s@void.invalid>"]);
    let mut frame = u32::try_from(data.len() + 1)
        .unwrap()
        .to_be_bytes()
        .to_vec();
    frame.push(SMFIC_MAIL);
    frame.extend_from_slice(&data);
    let (start, rest) = frame.split_at(8);
    mta.write_raw(start).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    server.handle.shutdown();
    tokio::time::sleep(Duration::from_millis(500)).await;
    mta.write_raw(rest).await;

    assert_eq!(mta.recv().await.unwrap().command, SMFIR_CONTINUE);
    assert!(mta.closes().await);
    let events = log.wait_for("session_end:ServerShutdown").await;
    assert_eq!(
        events,
        [
            "connect",
            "helo",
            "mail",
            "abort",
            "session_end:ServerShutdown"
        ]
    );
    server.join().await.unwrap();
}

/// How far a connection gets before the shutdown in SHT-011.
#[derive(Debug, Clone, Copy)]
enum Step {
    Accepted,
    Negotiated,
    At(Reach),
}

#[tokio::test]
async fn sht_011_shutdown_closes_a_connection_in_every_state() {
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
        let (server, log) = recording(timings(100, 5_000)).await;
        let mut mta = server.mta().await;
        match step {
            Step::Accepted => {}
            Step::Negotiated => {
                mta.negotiate().await;
            }
            Step::At(state) => mta.reach(state).await,
        }
        let before = log.events().len();

        server.handle.shutdown();

        assert!(mta.closes().await, "{step:?}");
        server.join().await.unwrap();
        let expected: &[&str] = match step {
            Step::Accepted | Step::Negotiated => &[],
            Step::At(state) if Reach::IN_MESSAGE.contains(&state) => {
                &["abort", "session_end:ServerShutdown"]
            }
            Step::At(_) => &["session_end:ServerShutdown"],
        };
        assert_eq!(log.events()[before..], *expected, "{step:?}");
    }
}

#[cfg(feature = "test-support")]
mod sockets {
    use rust_milter::{KeepaliveConfig, MilterText, SocketReadback};

    use super::*;

    type Reads = Arc<Mutex<Vec<(String, SocketReadback)>>>;

    /// Reads its socket's options in `mail`, in `end_of_message` once the
    /// shutdown token is cancelled, and in `session_end`.
    #[derive(Clone, Default)]
    struct ReadsOptions(Reads);

    impl ReadsOptions {
        fn read<H: Handler>(&self, callback: &str, ctx: &SessionContext<H>) {
            let options = ctx
                .connection()
                .read_socket_options()
                .expect("the socket options can be read");
            self.0.lock().unwrap().push((callback.to_owned(), options));
        }
    }

    impl Handler for ReadsOptions {
        type Session = ();
        type Message = ();

        async fn mail(
            &self,
            ctx: &mut MessageContext<Self>,
            _sender: &MilterText,
            _args: &[MilterText],
        ) -> Result<Decision> {
            self.read("mail", ctx);
            Ok(Decision::Continue)
        }

        async fn end_of_message(&self, ctx: &mut EomContext<Self>) -> Result<Decision> {
            ctx.shutdown().cancelled().await;
            self.read("end_of_message", ctx);
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

    #[tokio::test]
    async fn sht_012_socket_options_hold_in_the_grace_period_and_the_drain() {
        let handler = ReadsOptions::default();
        let reads = Arc::clone(&handler.0);
        let server = TestServer::start_with(timings(200, 5_000), handler).await;
        let mut mta = server.mta().await;
        mta.reach(Reach::Helo).await;

        server.handle.shutdown();
        mta.ask_continue(SMFIC_MAIL, strings(&["<s@void.invalid>"]))
            .await;
        mta.ask_continue(SMFIC_RCPT, strings(&["<r@void.invalid>"]))
            .await;
        mta.ask_continue(SMFIC_BODYEOB, Bytes::new()).await;

        assert!(mta.closes().await);
        server.join().await.unwrap();
        let reads = reads.lock().unwrap().clone();
        let mut callbacks = Vec::new();
        for (name, _) in &reads {
            callbacks.push(name.as_str());
        }
        assert_eq!(
            callbacks,
            ["mail", "end_of_message", "session_end:ServerShutdown"]
        );
        for (callback, options) in &reads {
            assert!(options.tcp_nodelay, "in {callback}");
            assert_eq!(
                options.keepalive,
                Some(KeepaliveConfig::default()),
                "in {callback}"
            );
        }
    }
}

#[tokio::test]
async fn sht_013_a_reset_connection_leaves_the_shutdown_at_once() {
    // During the grace period.
    let (server, log) = recording(timings(1_000, 5_000)).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Helo).await;
    let started = Instant::now();
    server.handle.shutdown();
    tokio::time::sleep(Duration::from_millis(100)).await;

    mta.reset();

    server.join().await.unwrap();
    assert!(
        started.elapsed() < Duration::from_millis(700),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(log.events().last().unwrap(), "session_end:Error");

    // During the drain, while `end_of_message` runs.
    let handler = Slow::new(Eom::AfterCancel(Duration::from_millis(300)));
    let log = handler.log.clone();
    let server = TestServer::start_with(timings(100, 5_000), handler).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Body).await;
    mta.send(SMFIC_BODYEOB, Bytes::new()).await;
    log.wait_for("end_of_message").await;
    let started = Instant::now();
    server.handle.shutdown();
    tokio::time::sleep(Duration::from_millis(200)).await;

    mta.reset();

    server.join().await.unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(
        log.events(),
        [
            "end_of_message",
            "end_of_message done",
            "abort",
            "session_end:Error"
        ]
    );
}
