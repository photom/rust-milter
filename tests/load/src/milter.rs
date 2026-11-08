//! The milter under test, run as a child process: see testing ADR 0005.

use std::io::{BufRead, BufReader, Write};
use std::net::SocketAddr;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

/// The address the milter listens on, on loopback inside the stack.
pub const ADDRESS: &str = "127.0.0.1:8891";

/// The prefix of the variables that configure the milter under test, as for
/// `ServerConfig::from_env`.
pub const PREFIX: &str = "LOAD_MILTER_";

/// How long the milter may take to start, to stop, or to close its
/// connections after the load ends.
pub const WAIT: Duration = Duration::from_secs(30);

/// The parts of the milter's `status()` that the scenarios read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Snapshot {
    /// `run()` has bound and no shutdown has started.
    pub accepting: bool,
    /// Open connections.
    pub connections: usize,
    /// Connections accepted since `run()` started.
    pub accepted: u64,
    /// Connections closed at once because a limit was reached.
    pub rejected: u64,
    /// The configuration generation.
    pub generation: u64,
}

impl Snapshot {
    /// Parses the line the milter writes for a `status` request, such as
    /// `accepting=true connections=3 accepted=10 rejected=0 generation=1`.
    pub fn parse(line: &str) -> Option<Snapshot> {
        let mut snapshot = Snapshot::default();
        for pair in line.split_whitespace() {
            let (name, value) = pair.split_once('=')?;
            match name {
                "accepting" => snapshot.accepting = value.parse().ok()?,
                "connections" => snapshot.connections = value.parse().ok()?,
                "accepted" => snapshot.accepted = value.parse().ok()?,
                "rejected" => snapshot.rejected = value.parse().ok()?,
                "generation" => snapshot.generation = value.parse().ok()?,
                _ => return None,
            }
        }
        Some(snapshot)
    }

    /// The line that [`Snapshot::parse`] reads.
    pub fn line(&self) -> String {
        format!(
            "accepting={} connections={} accepted={} rejected={} generation={}",
            self.accepting, self.connections, self.accepted, self.rejected, self.generation
        )
    }
}

/// A running milter under test. Dropping it kills the process.
pub struct Milter {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Milter {
    /// Starts the program `program`, the `load-milter` binary, listening on
    /// [`ADDRESS`], with `settings` as further configuration variables: each
    /// is a variable name without [`PREFIX`], such as `MAX_CONNECTIONS`, and
    /// its value. Returns once the milter accepts connections.
    ///
    /// # Panics
    ///
    /// When the milter does not start or accept connections within [`WAIT`].
    pub fn start(program: &str, settings: &[(&str, &str)]) -> Milter {
        let mut command = Command::new(program);
        command
            .env(format!("{PREFIX}BIND_ADDRESSES"), ADDRESS)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        for (name, value) in settings {
            command.env(format!("{PREFIX}{name}"), value);
        }
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => panic!("load-milter starts: {error}"),
        };
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            panic!("load-milter has piped standard input and output");
        };
        let mut milter = Milter {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        };
        let deadline = Instant::now() + WAIT;
        while !milter.status().accepting {
            assert!(Instant::now() < deadline, "load-milter accepts in time");
            std::thread::sleep(Duration::from_millis(50));
        }
        milter
    }

    /// The address the milter listens on.
    pub fn address(&self) -> SocketAddr {
        match ADDRESS.parse() {
            Ok(address) => address,
            Err(error) => panic!("{ADDRESS} is an address: {error}"),
        }
    }

    /// The milter's `status()`, asked over its standard input and read from
    /// its standard output.
    ///
    /// # Panics
    ///
    /// When the milter has exited or answers with a line it cannot parse.
    pub fn status(&mut self) -> Snapshot {
        if let Err(error) = self.stdin.write_all(b"status\n") {
            panic!("load-milter reads its standard input: {error}");
        }
        if let Err(error) = self.stdin.flush() {
            panic!("load-milter reads its standard input: {error}");
        }
        let mut line = String::new();
        match self.stdout.read_line(&mut line) {
            Ok(0) => panic!("load-milter exited"),
            Ok(_) => {}
            Err(error) => panic!("load-milter writes its standard output: {error}"),
        }
        match Snapshot::parse(&line) {
            Some(snapshot) => snapshot,
            None => panic!("load-milter answered {line:?}"),
        }
    }

    /// Waits until the milter has no open connection.
    ///
    /// # Panics
    ///
    /// When connections stay open longer than `timeout`.
    pub fn wait_until_idle(&mut self, timeout: Duration) -> Snapshot {
        let deadline = Instant::now() + timeout;
        loop {
            let snapshot = self.status();
            if snapshot.connections == 0 {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "load-milter still has {} connections after {timeout:?}",
                snapshot.connections
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// The number of file descriptors the milter has open, from
    /// `/proc/<pid>/fd`.
    pub fn open_files(&self) -> usize {
        let path = format!("/proc/{}/fd", self.child.id());
        let entries = match std::fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(error) => panic!("{path} can be read: {error}"),
        };
        let mut count = 0;
        for entry in entries {
            if entry.is_ok() {
                count += 1;
            }
        }
        count
    }
}

impl Drop for Milter {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
