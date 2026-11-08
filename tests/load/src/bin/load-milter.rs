//! The milter under test of the load and chaos scenarios, started by each
//! test as a child process (testing ADR 0005).
//!
//! It continues at every stage. Its configuration is the default one with
//! the variables that start with `LOAD_MILTER_` applied. Each line `status`
//! on standard input gets one line on standard output with the server's
//! status, as [`Snapshot`] writes it. When standard input closes, the milter
//! shuts down. It exits with code 0 when `run` returns `Ok`.

use std::io::{BufRead, Write};
use std::process::ExitCode;

use rust_milter::{Handler, Server, ServerConfig, ServerHandle};
use rust_milter_load::Snapshot;
use rust_milter_load::milter::PREFIX;

/// Implements no callback, so every stage continues.
struct Continues;

impl Handler for Continues {
    type Session = ();
    type Message = ();
}

#[tokio::main]
async fn main() -> ExitCode {
    let config = match ServerConfig::from_env(PREFIX) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("load-milter: the configuration variables are invalid: {error}");
            return ExitCode::FAILURE;
        }
    };
    let built = Server::builder()
        .config(config)
        .handler(|_| Continues)
        .build();
    let server = match built {
        Ok(server) => server,
        Err(error) => {
            eprintln!("load-milter: the configuration is invalid: {error}");
            return ExitCode::FAILURE;
        }
    };
    let handle = server.handle();
    std::thread::spawn(move || answer(handle));
    match server.run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("load-milter: run() failed: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Answers `status` requests on standard input until it closes, then shuts
/// the server down. Runs on a thread of its own, because a blocking read of
/// standard input must not hold up the runtime.
fn answer(handle: ServerHandle) {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            break;
        };
        if line.trim() != "status" {
            eprintln!("load-milter: unknown request {line:?}");
            continue;
        }
        let status = handle.status();
        let snapshot = Snapshot {
            accepting: status.accepting,
            connections: status.connections,
            accepted: status.connections_accepted,
            rejected: status.connections_rejected,
            generation: status.config_generation,
        };
        if writeln!(stdout, "{}", snapshot.line()).is_err() || stdout.flush().is_err() {
            break;
        }
    }
    handle.shutdown();
}
