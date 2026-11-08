//! Status and log scenarios (SRV-041, SRV-042, SRV-070 to SRV-072) from
//! `docs/testing/server.md`.

mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use rust_milter::protocol::*;
use rust_milter::{
    Bytes, ConnectDecision, ConnectInfo, Handler, Result, Server, ServerConfig, SessionContext,
};
use support::logs::Logs;
use support::mta::{Mta, Reach, WAIT};
use support::ports::PortLease;
use support::{ContinueAll, TestServer};

/// Logs the connection ID it sees in `connect`.
#[derive(Clone, Default)]
struct RecordsIds(Arc<Mutex<Vec<u64>>>);

impl Handler for RecordsIds {
    type Session = ();
    type Message = ();

    async fn connect(
        &self,
        ctx: &mut SessionContext<Self>,
        _info: &ConnectInfo,
    ) -> Result<ConnectDecision> {
        self.0.lock().unwrap().push(ctx.connection().id);
        Ok(ConnectDecision::Continue)
    }
}

#[tokio::test]
async fn srv_041_connection_ids_grow_and_match_the_accept_log() {
    let (logs, _guard) = Logs::capture();
    let handler = RecordsIds::default();
    let ids = Arc::clone(&handler.0);
    let server = TestServer::start(handler).await;

    for _ in 0..100 {
        let mut mta = server.mta().await;
        mta.reach(Reach::Conn).await;
    }
    server.stop().await;

    let ids = ids.lock().unwrap().clone();
    assert_eq!(ids.len(), 100);
    for pair in ids.windows(2) {
        assert!(pair[0] < pair[1], "{ids:?}");
    }
    let mut logged = Vec::new();
    for event in logs.named("Connection accepted") {
        logged.push(event.field("conn_id").unwrap().to_owned());
    }
    let mut seen = Vec::new();
    for id in &ids {
        seen.push(id.to_string());
    }
    assert_eq!(logged, seen);
}

#[tokio::test]
async fn srv_042_a_protocol_error_close_is_a_warning() {
    let (logs, _guard) = Logs::capture();
    let server = TestServer::start(ContinueAll).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Helo).await;

    mta.send(b'Z', Bytes::new()).await;
    assert!(mta.closes().await);
    server.stop().await;

    let closed = logs.one("Connection closed");
    assert_eq!(closed.level, tracing::Level::WARN);
    assert_eq!(closed.field("reason"), Some("error"));
    let error = closed.field("error").unwrap();
    assert!(error.contains("unknown command"), "{error}");
}

#[tokio::test]
async fn srv_070_status_follows_the_server() {
    let lease = PortLease::acquire().await;
    let config = ServerConfig {
        bind_addresses: vec![lease.v4()],
        shutdown: support::quick_shutdown(),
        ..ServerConfig::default()
    };
    let server = Server::builder()
        .config(config)
        .handler(ContinueAll::new)
        .build()
        .unwrap();
    let handle = server.handle();

    let before = handle.status();
    assert!(!before.accepting);
    assert_eq!(before.connections, 0);
    assert_eq!(before.config_generation, 1);

    let task = tokio::spawn(server.run());
    let mut first = Mta::connect(lease.v4()).await;
    let mut second = Mta::connect(lease.v4()).await;
    first.reach(Reach::Helo).await;
    second.reach(Reach::Helo).await;
    let running = handle.status();
    tokio::time::sleep(Duration::from_millis(50)).await;
    let later = handle.status();

    assert!(running.accepting);
    assert_eq!(running.connections, 2);
    assert!(!running.shutting_down);
    assert!(later.uptime > running.uptime);
    assert_eq!(running.config_generation, 1);
    handle.shutdown();
    tokio::time::timeout(WAIT, task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn srv_071_logs_the_accept_and_the_close_of_a_connection() {
    let (logs, _guard) = Logs::capture();
    let server = TestServer::start(ContinueAll).await;
    let listener = server.address().to_string();
    let mut mta = server.mta().await;
    mta.reach(Reach::Endm).await;

    mta.send(SMFIC_QUIT, Bytes::new()).await;
    assert!(mta.closes().await);
    server.stop().await;

    let accepted = logs.one("Connection accepted");
    assert_eq!(accepted.level, tracing::Level::DEBUG);
    for field in ["conn_id", "remote_addr", "local_addr", "listener"] {
        assert!(accepted.field(field).is_some(), "{field} in {accepted:?}");
    }
    assert_eq!(accepted.field("listener"), Some(listener.as_str()));
    let closed = logs.one("Connection closed");
    assert_eq!(closed.level, tracing::Level::DEBUG);
    assert_eq!(closed.field("reason"), Some("client_closed"));
    for field in [
        "conn_id",
        "remote_addr",
        "duration_ms",
        "bytes_read",
        "bytes_written",
        "commands",
    ] {
        assert!(closed.field(field).is_some(), "{field} in {closed:?}");
    }
    assert_ne!(closed.field("bytes_read"), Some("0"));
    assert_ne!(closed.field("bytes_written"), Some("0"));
}

#[tokio::test]
async fn srv_072_logs_the_server_start() {
    let (logs, _guard) = Logs::capture();
    let first = PortLease::acquire().await;
    let second = PortLease::acquire().await;
    let config = ServerConfig {
        bind_addresses: vec![first.v4(), second.v4()],
        backlog: 128,
        shutdown: support::quick_shutdown(),
        ..ServerConfig::default()
    };
    let server = Server::builder()
        .config(config)
        .handler(ContinueAll::new)
        .build()
        .unwrap();
    let handle = server.handle();
    let task = tokio::spawn(server.run());
    Mta::connect(first.v4()).await;
    handle.shutdown();
    tokio::time::timeout(WAIT, task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    let starting = logs.one("Server starting");
    assert_eq!(starting.level, tracing::Level::INFO);
    assert!(starting.field("version").is_some());
    let bound = logs.named("Listener bound");
    assert_eq!(bound.len(), 2);
    for (event, addr) in bound.iter().zip([first.v4(), second.v4()]) {
        assert_eq!(event.level, tracing::Level::INFO);
        assert_eq!(event.field("address"), Some(addr.to_string().as_str()));
        assert_eq!(event.field("backlog"), Some("128"));
    }
    let started = logs.one("Server started");
    assert!(started.field("pid").is_some());
    let mut order = Vec::new();
    for event in logs.all() {
        order.push(event.message);
    }
    let starting = position(&order, "Server starting");
    let bound = position(&order, "Listener bound");
    assert!(starting < bound);
    assert!(bound < position(&order, "Server started"));
}

/// The index of the first `message` in `order`.
fn position(order: &[String], message: &str) -> usize {
    for (index, seen) in order.iter().enumerate() {
        if seen == message {
            return index;
        }
    }
    panic!("no {message:?} in {order:?}");
}
