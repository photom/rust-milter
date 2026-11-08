//! A milter that runs as a child process of the server lifecycle scenarios
//! (INT-050, INT-051), so that the test can send it real signals.
//!
//! It listens on port 8891 like [`Milter`], continues at every stage, and
//! has a reload loader that returns the same configuration, so that `SIGHUP`
//! reloads it. It exits with code 0 when `run` returns `Ok`.

use std::process::ExitCode;

use rust_milter::{Handler, ReloadLoader, Reloaded, Server, ServerConfig};
use rust_milter_interop::Milter;

/// Implements no callback, so every stage continues.
struct Continues;

impl Handler for Continues {
    type Session = ();
    type Message = ();
}

/// A loader that returns the configuration the milter started with.
struct SameConfig {
    config: ServerConfig,
}

impl ReloadLoader<()> for SameConfig {
    type Error = std::io::Error;

    async fn load(&self) -> Result<Reloaded<()>, std::io::Error> {
        Ok(Reloaded {
            server: self.config.clone(),
            app: (),
        })
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let config = Milter::config();
    let built = Server::builder()
        .config(config.clone())
        .handler(|_| Continues)
        .reload_with(SameConfig { config })
        .build();
    let server = match built {
        Ok(server) => server,
        Err(error) => {
            eprintln!("int-milter: the configuration is invalid: {error}");
            return ExitCode::FAILURE;
        }
    };
    match server.run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("int-milter: run() failed: {error}");
            ExitCode::FAILURE
        }
    }
}
