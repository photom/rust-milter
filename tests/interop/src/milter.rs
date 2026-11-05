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
    /// Starts a milter with `handler` and the default configuration, except
    /// for a grace period of zero. Starts the sink too, if it is not running.
    pub async fn start<H: Handler>(handler: H) -> Milter {
        Milter::start_with(|_| {}, handler).await
    }

    /// Starts a milter, with the configuration changed by `configure` first.
    pub async fn start_with<H: Handler>(
        configure: impl FnOnce(&mut ServerConfig),
        handler: H,
    ) -> Milter {
        sink::start();
        let mut config = ServerConfig {
            bind_addresses: vec![SocketAddr::from(([0, 0, 0, 0], PORT))],
            ..ServerConfig::default()
        };
        config.shutdown.grace_period = Duration::ZERO;
        configure(&mut config);
        let server = Server::builder()
            .config(config)
            .handler(move |_| handler)
            .build()
            .expect("the test configuration is valid");
        let handle = server.handle();
        let task = tokio::spawn(server.run());
        let milter = Milter { handle, task };
        milter.wait_for(|s| s.accepting).await;
        milter
    }

    /// The server's status.
    pub fn status(&self) -> Status {
        self.handle.status()
    }

    /// Waits until the milter has accepted at least `connections`
    /// connections and the MTAs have closed all of them.
    pub async fn wait_closed(&self, connections: u64) {
        self.wait_for(|s| s.connections_accepted >= connections && s.connections == 0)
            .await;
    }

    /// Shuts the milter down and waits for `run` to return `Ok`.
    pub async fn stop(self) {
        self.handle.shutdown();
        tokio::time::timeout(WAIT, self.task)
            .await
            .expect("the milter stops in time")
            .expect("the milter task does not panic")
            .expect("run() returns Ok");
    }

    async fn wait_for(&self, condition: impl Fn(&Status) -> bool) {
        let deadline = Instant::now() + WAIT;
        loop {
            let status = self.status();
            if condition(&status) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "the milter's status in time: {status:?}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}
