//! Test support shared by the integration tests: port leases, the fake MTA,
//! test handlers, and a server on a leased port.
#![allow(dead_code, reason = "each test file uses a different part")]

pub mod logs;
pub mod mta;
pub mod ports;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use rust_milter::{
    Handler, HandlerContext, MessageContext, Result, Server, ServerConfig, ServerHandle,
    SessionContext, SessionEnd,
};

use mta::{Mta, WAIT};
use ports::PortLease;

/// A handler that implements no callbacks.
pub struct ContinueAll;

impl ContinueAll {
    pub fn new(_ctx: &HandlerContext<()>) -> Self {
        ContinueAll
    }
}

impl Handler for ContinueAll {
    type Session = ();
    type Message = ();
}

/// A server on a leased port, stopped by [`TestServer::stop`].
pub struct TestServer {
    pub handle: ServerHandle,
    task: tokio::task::JoinHandle<Result<()>>,
    lease: PortLease,
}

impl TestServer {
    /// Starts a server with the default configuration on a leased port.
    pub async fn start<H, F>(factory: F) -> TestServer
    where
        H: Handler,
        F: FnOnce(&HandlerContext<()>) -> H + 'static,
    {
        TestServer::start_with(|_| {}, factory).await
    }

    /// Starts a server on a leased port, with the configuration changed by
    /// `configure` first.
    pub async fn start_with<H, F>(
        configure: impl FnOnce(&mut ServerConfig),
        factory: F,
    ) -> TestServer
    where
        H: Handler,
        F: FnOnce(&HandlerContext<()>) -> H + 'static,
    {
        let lease = PortLease::acquire().await;
        let mut config = ServerConfig {
            bind_addresses: vec![lease.v4()],
            ..ServerConfig::default()
        };
        configure(&mut config);
        let server = Server::builder()
            .config(config)
            .handler(factory)
            .build()
            .expect("the test configuration is valid");
        let handle = server.handle();
        let task = tokio::spawn(server.run());
        TestServer {
            handle,
            task,
            lease,
        }
    }

    /// The server's address.
    pub fn address(&self) -> std::net::SocketAddr {
        self.lease.v4()
    }

    /// A new connection of the fake MTA.
    pub async fn mta(&self) -> Mta {
        Mta::connect(self.lease.v4()).await
    }

    /// Shuts the server down and waits for `run` to return `Ok`.
    pub async fn stop(self) {
        self.handle.shutdown();
        tokio::time::timeout(WAIT, self.task)
            .await
            .expect("the server did not stop in time")
            .expect("the server task panicked")
            .expect("run() failed");
        drop(self.lease);
    }
}

/// The callbacks a handler saw, in order.
#[derive(Clone, Default)]
pub struct Log(Arc<Mutex<Vec<String>>>);

impl Log {
    pub fn push(&self, event: impl Into<String>) {
        self.0.lock().unwrap().push(event.into());
    }

    pub fn events(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }

    /// Waits until `event` is logged, and returns the log at that point.
    pub async fn wait_for(&self, event: &str) -> Vec<String> {
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            let events = self.events();
            if events.iter().any(|e| e == event) {
                return events;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "no {event:?} in time; the log is {events:?}"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Waits a little and checks that nothing more was logged than `events`.
    pub async fn settles_at(&self, events: &[&str]) {
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(self.events(), events);
    }
}

/// A handler that logs every callback and always continues.
#[derive(Clone, Default)]
pub struct Recorder {
    pub log: Log,
}

impl Handler for Recorder {
    type Session = ();
    type Message = ();

    async fn connect(
        &self,
        _ctx: &mut SessionContext<Self>,
        _info: &rust_milter::ConnectInfo,
    ) -> Result<rust_milter::ConnectDecision> {
        self.log.push("connect");
        Ok(rust_milter::ConnectDecision::Continue)
    }

    async fn helo(
        &self,
        _ctx: &mut SessionContext<Self>,
        _name: &rust_milter::MilterText,
    ) -> Result<rust_milter::ConnectDecision> {
        self.log.push("helo");
        Ok(rust_milter::ConnectDecision::Continue)
    }

    async fn mail(
        &self,
        _ctx: &mut MessageContext<Self>,
        _sender: &rust_milter::MilterText,
        _args: &[rust_milter::MilterText],
    ) -> Result<rust_milter::Decision> {
        self.log.push("mail");
        Ok(rust_milter::Decision::Continue)
    }

    async fn rcpt(
        &self,
        _ctx: &mut MessageContext<Self>,
        _recipient: &rust_milter::MilterText,
        _args: &[rust_milter::MilterText],
    ) -> Result<rust_milter::Decision> {
        self.log.push("rcpt");
        Ok(rust_milter::Decision::Continue)
    }

    async fn data(&self, _ctx: &mut MessageContext<Self>) -> Result<rust_milter::Decision> {
        self.log.push("data");
        Ok(rust_milter::Decision::Continue)
    }

    async fn header(
        &self,
        _ctx: &mut MessageContext<Self>,
        _name: &rust_milter::MilterText,
        _value: &rust_milter::MilterText,
    ) -> Result<rust_milter::Decision> {
        self.log.push("header");
        Ok(rust_milter::Decision::Continue)
    }

    async fn end_of_headers(
        &self,
        _ctx: &mut MessageContext<Self>,
    ) -> Result<rust_milter::Decision> {
        self.log.push("end_of_headers");
        Ok(rust_milter::Decision::Continue)
    }

    async fn body(
        &self,
        _ctx: &mut MessageContext<Self>,
        _chunk: rust_milter::Bytes,
    ) -> Result<rust_milter::BodyDecision> {
        self.log.push("body");
        Ok(rust_milter::BodyDecision::Continue)
    }

    async fn end_of_message(
        &self,
        _ctx: &mut rust_milter::EomContext<Self>,
    ) -> Result<rust_milter::Decision> {
        self.log.push("end_of_message");
        Ok(rust_milter::Decision::Continue)
    }

    async fn abort(&self, _ctx: &mut MessageContext<Self>) -> Result<()> {
        self.log.push("abort");
        Ok(())
    }

    async fn unknown(
        &self,
        _ctx: &mut SessionContext<Self>,
        _command: &rust_milter::MilterText,
    ) -> Result<rust_milter::Decision> {
        self.log.push("unknown");
        Ok(rust_milter::Decision::Continue)
    }

    async fn session_end(&self, _ctx: &mut SessionContext<Self>, reason: SessionEnd) -> Result<()> {
        self.log.push(format!("session_end:{reason:?}"));
        Ok(())
    }
}
