//! Signal and exit code scenarios (SHT-020 to SHT-024, SHT-026, SHT-027)
//! from `docs/testing/shutdown.md`. SHT-025 needs a reload loader and comes
//! with reload.
//!
//! Each test runs this test binary again as a child process that serves on
//! a leased port. The child prints `READY` once it accepts connections,
//! prints its log events when `run()` returns, and exits with the code the
//! specification recommends: 0 after a shutdown, 1 when `run()` fails, and
//! 2 when `build()` fails. Only the parent sends signals, with `kill`.
#![cfg(unix)]

mod support;

use std::io::{BufRead, BufReader};
use std::net::{SocketAddr, TcpListener};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use rust_milter::protocol::*;
use rust_milter::{Bytes, Decision, EomContext, Handler, Result, Server, ServerConfig};
use support::logs::Logs;
use support::mta::{Mta, Reach};
use support::ports::PortLease;

/// The child's port; set only in the child.
const PORT: &str = "RUST_MILTER_SHT_CHILD_PORT";
/// Which server the child runs.
const SETUP: &str = "RUST_MILTER_SHT_CHILD_SETUP";

/// Held by each test for its whole run. A parallel test could start a
/// process while `PortLease` checks a port with a listening socket, and the
/// new process would hold a copy of that socket until it calls `exec`,
/// which makes a bind on the port fail for a moment.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// What the child serves.
#[derive(Clone, Copy)]
enum Setup {
    /// `ContinueAll` with a 100 ms grace period.
    Idle,
    /// An `end_of_message` that sleeps 10 s, with `drain_timeout` = 30 s.
    SlowEom,
    /// A configuration that fails validation.
    Invalid,
}

impl Setup {
    fn name(self) -> &'static str {
        match self {
            Setup::Idle => "idle",
            Setup::SlowEom => "slow_eom",
            Setup::Invalid => "invalid",
        }
    }
}

/// Sleeps 10 s in `end_of_message`, ignoring the shutdown token.
struct SlowEom;

impl Handler for SlowEom {
    type Session = ();
    type Message = ();

    async fn end_of_message(&self, _ctx: &mut EomContext<Self>) -> Result<Decision> {
        tokio::time::sleep(Duration::from_secs(10)).await;
        Ok(Decision::Continue)
    }
}

/// In the child, runs the server and exits; in the parent, returns.
async fn child_main() {
    let Ok(port) = std::env::var(PORT) else {
        return;
    };
    let (logs, _guard) = Logs::capture();
    let setup = std::env::var(SETUP).unwrap();
    let mut config = ServerConfig {
        bind_addresses: vec![SocketAddr::from(([127, 0, 0, 1], port.parse().unwrap()))],
        ..ServerConfig::default()
    };
    config.shutdown.grace_period = Duration::from_millis(100);
    if setup == "slow_eom" {
        config.shutdown.drain_timeout = Duration::from_secs(30);
    }
    if setup == "invalid" {
        config.backlog = 0;
    }
    let code = if setup == "slow_eom" {
        serve(
            Server::builder()
                .config(config)
                .handler(|_| SlowEom)
                .build(),
        )
        .await
    } else {
        let handler = support::ContinueAll::new;
        serve(Server::builder().config(config).handler(handler).build()).await
    };
    for event in logs.all() {
        let fields: Vec<String> = event
            .fields
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect();
        println!(
            "\nLOG|{}|{}|{}",
            event.level,
            event.message,
            fields.join("|")
        );
    }
    std::process::exit(code);
}

/// Runs a built server and returns the exit code for the result.
async fn serve<H: Handler>(built: Result<Server<H>>) -> i32 {
    let Ok(server) = built else {
        return 2;
    };
    let handle = server.handle();
    let task = tokio::spawn(server.run());
    while !handle.status().accepting && !task.is_finished() {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    println!("\nREADY");
    match task.await.unwrap() {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

/// A log event the child printed.
struct Event {
    level: String,
    message: String,
    fields: Vec<String>,
}

/// A child process running a server.
struct Child {
    process: std::process::Child,
    lines: mpsc::Receiver<String>,
}

impl Child {
    /// Starts the child for `test` with `setup`, on `port`.
    fn start(test: &str, setup: Setup, port: u16) -> Child {
        let mut process = Command::new(std::env::current_exe().unwrap())
            .args([test, "--exact", "--nocapture", "--test-threads=1"])
            .env(PORT, port.to_string())
            .env(SETUP, setup.name())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdout = process.stdout.take().unwrap();
        let (sender, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { return };
                if sender.send(line).is_err() {
                    return;
                }
            }
        });
        Child { process, lines }
    }

    /// Waits until the child accepts connections.
    fn ready(&self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let line = self.lines.recv_timeout(left).expect("the child is ready");
            if line.contains("READY") {
                return;
            }
        }
    }

    fn signal(&self, name: &str) {
        let status = Command::new("kill")
            .args(["-s", name, &self.process.id().to_string()])
            .status()
            .unwrap();
        assert!(status.success(), "kill -s {name}");
    }

    /// Waits up to `limit` for the child to exit, and returns its exit
    /// code, `None` if a signal ended it, and the events it logged.
    fn exit(mut self, limit: Duration) -> (Option<i32>, Vec<Event>) {
        let deadline = Instant::now() + limit;
        let status = loop {
            if let Some(status) = self.process.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = self.process.kill();
                panic!("the child did not exit within {limit:?}");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let mut events = Vec::new();
        while let Ok(line) = self.lines.recv_timeout(Duration::from_secs(5)) {
            let Some(start) = line.find("LOG|") else {
                continue;
            };
            let mut parts = line[start + 4..].split('|');
            events.push(Event {
                level: parts.next().unwrap().to_owned(),
                message: parts.next().unwrap().to_owned(),
                fields: parts.map(str::to_owned).collect(),
            });
        }
        (status.code(), events)
    }
}

/// The one event with `message`.
fn one<'a>(events: &'a [Event], message: &str) -> &'a Event {
    let found: Vec<&Event> = events.iter().filter(|e| e.message == message).collect();
    assert_eq!(found.len(), 1, "{message:?}");
    found[0]
}

/// Starts an idle child, sends `signal`, and checks that it exits with 0.
async fn exits_on(test: &str, signal: &str) -> Vec<Event> {
    let lease = PortLease::acquire().await;
    let child = Child::start(test, Setup::Idle, lease.v4().port());
    child.ready();

    child.signal(signal);

    let (code, events) = child.exit(Duration::from_secs(5));
    assert_eq!(code, Some(0), "the child exits with 0 after {signal}");
    events
}

#[tokio::test]
async fn sht_020_sigterm_shuts_down() {
    child_main().await;
    let _serial = SERIAL.lock().await;

    let events = exits_on("sht_020_sigterm_shuts_down", "TERM").await;

    let received = one(&events, "Shutdown signal received");
    assert_eq!(received.level, "INFO");
    assert!(received.fields.contains(&"signal_name=SIGTERM".to_owned()));
    assert!(received.fields.contains(&"reason=signal".to_owned()));
    one(&events, "Shutdown complete");
}

#[tokio::test]
async fn sht_021_sigint_shuts_down() {
    child_main().await;
    let _serial = SERIAL.lock().await;

    let events = exits_on("sht_021_sigint_shuts_down", "INT").await;

    let received = one(&events, "Shutdown signal received");
    assert!(received.fields.contains(&"signal_name=SIGINT".to_owned()));
}

#[tokio::test]
async fn sht_022_sigquit_shuts_down_without_a_core_dump() {
    child_main().await;
    let _serial = SERIAL.lock().await;

    // Exit code 0 means the child ended by itself; the default action of
    // SIGQUIT would have ended it by the signal, with a core dump.
    let events = exits_on("sht_022_sigquit_shuts_down_without_a_core_dump", "QUIT").await;

    let received = one(&events, "Shutdown signal received");
    assert!(received.fields.contains(&"signal_name=SIGQUIT".to_owned()));
}

/// Starts a child whose `end_of_message` runs 10 s, keeps a message in it,
/// and sends `first`, then `second` 500 ms later.
async fn second_signal_forces(test: &str, first: &str, second: &str) {
    let lease = PortLease::acquire().await;
    let child = Child::start(test, Setup::SlowEom, lease.v4().port());
    child.ready();
    let mut mta = Mta::connect(lease.v4()).await;
    mta.reach(Reach::Body).await;
    mta.send(SMFIC_BODYEOB, Bytes::new()).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    child.signal(first);
    tokio::time::sleep(Duration::from_millis(500)).await;
    child.signal(second);
    let sent = Instant::now();

    let (code, events) = child.exit(Duration::from_secs(2));
    assert!(sent.elapsed() < Duration::from_secs(2));
    assert_eq!(code, Some(0));
    assert!(mta.closes().await, "the connection was force-closed");
    let forced = one(&events, "Second shutdown signal received");
    assert_eq!(forced.level, "WARN");
    assert!(forced.fields.contains(&format!("signal_name=SIG{second}")));
    assert!(forced.fields.contains(&"action=force shutdown".to_owned()));
}

#[tokio::test]
async fn sht_023_a_second_sigterm_forces_the_shutdown() {
    child_main().await;
    let _serial = SERIAL.lock().await;

    second_signal_forces(
        "sht_023_a_second_sigterm_forces_the_shutdown",
        "TERM",
        "TERM",
    )
    .await;
}

#[tokio::test]
async fn sht_024_any_second_shutdown_signal_forces_the_shutdown() {
    child_main().await;
    let _serial = SERIAL.lock().await;

    second_signal_forces(
        "sht_024_any_second_shutdown_signal_forces_the_shutdown",
        "TERM",
        "INT",
    )
    .await;
}

#[tokio::test]
async fn sht_026_an_invalid_configuration_exits_with_2() {
    child_main().await;
    let _serial = SERIAL.lock().await;
    let lease = PortLease::acquire().await;

    let child = Child::start(
        "sht_026_an_invalid_configuration_exits_with_2",
        Setup::Invalid,
        lease.v4().port(),
    );

    let (code, _) = child.exit(Duration::from_secs(5));
    assert_eq!(code, Some(2));
}

#[tokio::test]
async fn sht_027_a_failed_bind_exits_with_1() {
    child_main().await;
    let _serial = SERIAL.lock().await;
    let lease = PortLease::acquire().await;
    let _blocker = TcpListener::bind(lease.v4()).unwrap();

    let child = Child::start(
        "sht_027_a_failed_bind_exits_with_1",
        Setup::Idle,
        lease.v4().port(),
    );

    let (code, _) = child.exit(Duration::from_secs(5));
    assert_eq!(code, Some(1));
}
