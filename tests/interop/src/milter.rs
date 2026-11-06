//! The milter under test, on port 8891 of the runner.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use rust_milter::{Handler, Result, Server, ServerConfig, ServerHandle, Status};
use tokio::task::JoinHandle;

use crate::{WAIT, sink};

/// The port every MTA in the stack connects to.
pub const PORT: u16 = 8891;

/// A running milter, stopped by [`Milter::stop`].
pub struct Milter {
    /// The server's handle.
    pub handle: ServerHandle,
    task: JoinHandle<Result<()>>,
}

impl Milter {
    /// The configuration [`Milter::start`] uses: port 8891 on every address,
    /// and a grace period of zero. A test changes it and passes it to
    /// [`Milter::start_with`].
    pub fn config() -> ServerConfig {
        let mut config = ServerConfig {
            bind_addresses: vec![SocketAddr::from(([0, 0, 0, 0], PORT))],
            ..ServerConfig::default()
        };
        config.shutdown.grace_period = Duration::ZERO;
        config
    }

    /// Starts a milter with `handler` and [`Milter::config`]. Starts the
    /// sink too, if it is not running.
    pub async fn start<H: Handler>(handler: H) -> Milter {
        Milter::start_with(Milter::config(), handler).await
    }

    /// Starts a milter with `handler` and `config`.
    pub async fn start_with<H: Handler>(config: ServerConfig, handler: H) -> Milter {
        sink::start();
        let server = Server::builder()
            .config(config)
            .handler(move |_| handler)
            .build()
            .expect("the test configuration is valid");
        let handle = server.handle();
        let task = tokio::spawn(server.run());
        let milter = Milter { handle, task };
        milter.wait_accepting().await;
        milter
    }

    /// The server's status.
    pub fn status(&self) -> Status {
        self.handle.status()
    }

    /// Waits until the milter has accepted at least `connections`
    /// connections and the MTAs have closed all of them.
    pub async fn wait_closed(&self, connections: u64) {
        let deadline = Instant::now() + WAIT;
        loop {
            let status = self.status();
            if status.connections_accepted >= connections && status.connections == 0 {
                return;
            }
            pause_until(deadline, &status).await;
        }
    }

    /// Shuts the milter down and waits for `run` to return `Ok`.
    pub async fn stop(self) {
        self.handle.shutdown();
        let joined = match tokio::time::timeout(WAIT, self.task).await {
            Ok(joined) => joined,
            Err(_) => panic!("the milter did not stop in time"),
        };
        let result = match joined {
            Ok(result) => result,
            Err(error) => panic!("the milter task panicked: {error}"),
        };
        if let Err(error) = result {
            panic!("run() failed: {error}");
        }
    }

    async fn wait_accepting(&self) {
        let deadline = Instant::now() + WAIT;
        loop {
            let status = self.status();
            if status.accepting {
                return;
            }
            pause_until(deadline, &status).await;
        }
    }
}

/// Sleeps briefly before the next look at the status, or fails the test if
/// `deadline` has passed.
async fn pause_until(deadline: Instant, status: &Status) {
    assert!(
        Instant::now() < deadline,
        "the milter's status in time: {status:?}"
    );
    tokio::time::sleep(Duration::from_millis(20)).await;
}
