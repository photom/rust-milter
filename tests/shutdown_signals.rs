//! Signal and exit code scenarios SHT-020 to SHT-027 from
//! `docs/testing/shutdown.md`, and the `SIGHUP` scenarios RLD-043 and
//! RLD-044 from `docs/testing/reload.md`.
//!
//! Each test runs this test binary again as a child process that serves on
//! a leased port. The child prints `READY` once it accepts connections and
//! `GENERATION n` for each configuration it publishes, prints its log
//! events when `run()` returns, and exits with the code the specification
//! recommends: 0 after a shutdown, 1 when `run()` fails, and 2 when
//! `build()` fails. Only the parent sends signals, with `kill`.
#![cfg(unix)]

mod support;

use std::io::{self, BufRead, BufReader};
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use rust_milter::protocol::*;
use rust_milter::{
    Bytes, Decision, EomContext, Handler, ReloadLoader, Reloaded, Result, Server, ServerConfig,
};
use support::logs::Logs;
use support::mta::{Mta, Reach};
use support::ports::PortLease;

/// The child's port; set only in the child.
const PORT: &str = "RUST_MILTER_SHT_CHILD_PORT";
/// Which server the child runs.
const SETUP: &str = "RUST_MILTER_SHT_CHILD_SETUP";
/// The file the child's reload loader reads `max_connections` from; set
/// only for a child with a loader.
const FILE: &str = "RUST_MILTER_SHT_CHILD_FILE";

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
    let file = match std::env::var(FILE) {
        Ok(path) => Some(PathBuf::from(path)),
        Err(_) => None,
    };
    let code = if setup == "slow_eom" {
        serve(build(config, file, SlowEom)).await
    } else {
        serve(build(config, file, support::ContinueAll)).await
    };
    for event in logs.all() {
        let mut fields = Vec::new();
        for (name, value) in &event.fields {
            fields.push(format!("{name}={value}"));
        }
        println!(
            "\nLOG|{}|{}|{}",
            event.level,
            event.message,
            fields.join("|")
        );
    }
    std::process::exit(code);
}

/// Builds the child's server, with a loader that reads `file` when there
/// is one.
fn build<H: Handler>(config: ServerConfig, file: Option<PathBuf>, handler: H) -> Result<Server<H>> {
    let builder = Server::builder().config(config.clone());
    let builder = match file {
        Some(path) => builder.reload_with(FileLoader { path, base: config }),
        None => builder,
    };
    builder.handler(move |_| handler).build()
}

/// A loader that returns `base` with the `max_connections` that `path`
/// holds.
struct FileLoader {
    path: PathBuf,
    base: ServerConfig,
}

impl ReloadLoader<()> for FileLoader {
    type Error = io::Error;

    async fn load(&self) -> io::Result<Reloaded<()>> {
        tracing::info!("Loader called");
        let text = std::fs::read_to_string(&self.path)?;
        let max_connections = match text.trim().parse() {
            Ok(max_connections) => max_connections,
            Err(error) => return Err(io::Error::other(error)),
        };
        Ok(Reloaded {
            server: ServerConfig {
                max_connections,
                ..self.base.clone()
            },
            app: (),
        })
    }
}

/// Runs a built server and returns the exit code for the result.
async fn serve<H: Handler>(built: Result<Server<H>>) -> i32 {
    let Ok(server) = built else {
        return 2;
    };
    let handle = server.handle();
    let mut watch = handle.config_watch();
    let task = tokio::spawn(server.run());
    while !handle.status().accepting && !task.is_finished() {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    tokio::spawn(async move {
        loop {
            watch.changed().await;
            println!("\nGENERATION {}", watch.generation());
        }
    });
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
        Child::spawn(test, setup, port, None)
    }

    /// Starts the child with a reload loader that reads `file`.
    fn start_reloading(test: &str, setup: Setup, port: u16, file: &Path) -> Child {
        Child::spawn(test, setup, port, Some(file))
    }

    fn spawn(test: &str, setup: Setup, port: u16, file: Option<&Path>) -> Child {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([test, "--exact", "--nocapture", "--test-threads=1"])
            .env(PORT, port.to_string())
            .env(SETUP, setup.name());
        if let Some(file) = file {
            command.env(FILE, file);
        }
        let mut process = command
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
        self.prints("READY");
    }

    /// Waits until the child prints a line that contains `text`.
    fn prints(&self, text: &str) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let Ok(line) = self.lines.recv_timeout(left) else {
                panic!("the child prints {text:?}");
            };
            if line.contains(text) {
                return;
            }
        }
    }

    /// Whether the child is still running.
    fn running(&mut self) -> bool {
        self.process.try_wait().unwrap().is_none()
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
            let level = parts.next().unwrap().to_owned();
            let message = parts.next().unwrap().to_owned();
            let mut fields = Vec::new();
            for part in parts {
                fields.push(part.to_owned());
            }
            events.push(Event {
                level,
                message,
                fields,
            });
        }
        (status.code(), events)
    }
}

/// The one event with `message`.
fn one<'a>(events: &'a [Event], message: &str) -> &'a Event {
    let mut found = Vec::new();
    for event in events {
        if event.message == message {
            found.push(event);
        }
    }
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

/// A file for a child's loader, removed when dropped.
struct LoaderFile(PathBuf);

impl LoaderFile {
    fn new(test: &str, max_connections: usize) -> LoaderFile {
        let path = std::env::temp_dir().join(format!("rust-milter-{test}-{}", std::process::id()));
        let file = LoaderFile(path);
        file.write(max_connections);
        file
    }

    fn write(&self, max_connections: usize) {
        std::fs::write(&self.0, max_connections.to_string()).unwrap();
    }
}

impl Drop for LoaderFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Whether one of `event`'s fields is `field`.
fn has(event: &Event, field: &str) -> bool {
    event.fields.contains(&field.to_owned())
}

#[tokio::test]
async fn sht_025_sighup_during_a_drain_is_ignored() {
    const TEST: &str = "sht_025_sighup_during_a_drain_is_ignored";
    child_main().await;
    let _serial = SERIAL.lock().await;
    let lease = PortLease::acquire().await;
    let file = LoaderFile::new(TEST, 100);
    let child = Child::start_reloading(TEST, Setup::SlowEom, lease.v4().port(), &file.0);
    child.ready();
    let mut mta = Mta::connect(lease.v4()).await;
    mta.reach(Reach::Body).await;
    mta.send(SMFIC_BODYEOB, Bytes::new()).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    child.signal("TERM");
    // Past the 100 ms grace period, in the drain.
    tokio::time::sleep(Duration::from_millis(400)).await;

    child.signal("HUP");
    tokio::time::sleep(Duration::from_millis(200)).await;
    child.signal("TERM");

    let (code, events) = child.exit(Duration::from_secs(5));
    assert_eq!(code, Some(0));
    let ignored = one(&events, "SIGHUP ignored");
    assert_eq!(ignored.level, "WARN");
    assert!(has(ignored, "reason=shutting_down"));
    for event in &events {
        assert_ne!(event.message, "Loader called");
        assert_ne!(event.message, "Reload requested");
    }
}

#[tokio::test]
async fn rld_043_sighup_reloads_through_the_loader() {
    const TEST: &str = "rld_043_sighup_reloads_through_the_loader";
    child_main().await;
    let _serial = SERIAL.lock().await;
    let lease = PortLease::acquire().await;
    let file = LoaderFile::new(TEST, 100);
    let child = Child::start_reloading(TEST, Setup::Idle, lease.v4().port(), &file.0);
    child.ready();

    file.write(200);
    child.signal("HUP");

    child.prints("GENERATION 2");
    child.signal("TERM");
    let (code, events) = child.exit(Duration::from_secs(5));
    assert_eq!(code, Some(0));
    let requested = one(&events, "Reload requested");
    assert!(has(requested, "source=sighup"));
    assert!(has(requested, "generation=1"));
    let applied = one(&events, "Reload applied");
    assert!(has(applied, "generation=2"));
    assert!(has(applied, r#"changed=["max_connections"]"#));
}

#[tokio::test]
async fn rld_044_sighup_without_a_loader_is_ignored() {
    child_main().await;
    let _serial = SERIAL.lock().await;
    let lease = PortLease::acquire().await;
    let mut child = Child::start(
        "rld_044_sighup_without_a_loader_is_ignored",
        Setup::Idle,
        lease.v4().port(),
    );
    child.ready();

    child.signal("HUP");

    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(child.running(), "SIGHUP does not end the child");
    Mta::connect(lease.v4()).await.negotiate().await;
    child.signal("TERM");
    let (code, events) = child.exit(Duration::from_secs(5));
    assert_eq!(code, Some(0));
    let ignored = one(&events, "SIGHUP ignored");
    assert_eq!(ignored.level, "WARN");
    assert!(has(ignored, "reason=no_loader"));
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
