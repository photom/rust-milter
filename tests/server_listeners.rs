//! Listener scenarios (SRV-001 to SRV-009) from `docs/testing/server.md`,
//! and CFG-010 from `docs/testing/configuration.md`.

mod support;

use std::net::{SocketAddr, TcpListener};
use std::time::Duration;

use rust_milter::{BindMode, Error, Server, ServerConfig};
use support::ContinueAll;
use support::logs::Logs;
use support::mta::{Mta, WAIT};
use support::ports::PortLease;

fn config(addresses: Vec<SocketAddr>) -> ServerConfig {
    ServerConfig {
        bind_addresses: addresses,
        shutdown: support::quick_shutdown(),
        ..ServerConfig::default()
    }
}

/// Runs a server with `config` until the test calls `stop`.
struct Running {
    handle: rust_milter::ServerHandle,
    task: tokio::task::JoinHandle<rust_milter::Result<()>>,
}

impl Running {
    fn start(config: ServerConfig) -> Running {
        let server = Server::builder()
            .config(config)
            .handler(ContinueAll::new)
            .build()
            .unwrap();
        let handle = server.handle();
        Running {
            handle,
            task: tokio::spawn(server.run()),
        }
    }

    async fn stop(self) {
        self.handle.shutdown();
        tokio::time::timeout(WAIT, self.task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}

/// Runs `config` to the end and returns what `run()` returned.
async fn run_result(config: ServerConfig) -> rust_milter::Result<()> {
    let server = Server::builder()
        .config(config)
        .handler(ContinueAll::new)
        .build()
        .unwrap();
    let handle = server.handle();
    let task = tokio::spawn(server.run());
    // A server that bound runs until shutdown; give it the chance to fail.
    tokio::time::sleep(Duration::from_millis(200)).await;
    handle.shutdown();
    tokio::time::timeout(WAIT, task).await.unwrap().unwrap()
}

#[tokio::test]
async fn cfg_010_build_opens_no_socket_for_an_invalid_configuration() {
    let lease = PortLease::acquire().await;
    let mut invalid = config(vec![lease.v4()]);
    invalid.backlog = 0;

    let built = Server::builder()
        .config(invalid)
        .handler(ContinueAll::new)
        .build();

    assert!(matches!(built, Err(Error::Config(_))));
    TcpListener::bind(lease.v4()).expect("the port is still free");
}

#[tokio::test]
async fn srv_001_serves_ipv4() {
    let lease = PortLease::acquire().await;
    let server = Running::start(config(vec![lease.v4()]));

    let mut mta = Mta::connect(lease.v4()).await;

    assert_eq!(mta.negotiate().await.version, 6);
    server.stop().await;
}

#[tokio::test]
async fn srv_002_serves_ipv6() {
    let lease = PortLease::acquire().await;
    let Some(v6) = lease.v6_or_skip() else { return };
    let server = Running::start(config(vec![v6]));

    let mut mta = Mta::connect(v6).await;

    assert_eq!(mta.negotiate().await.version, 6);
    server.stop().await;
}

#[tokio::test]
async fn srv_003_serves_every_listener() {
    let first = PortLease::acquire().await;
    let second = PortLease::acquire().await;
    let server = Running::start(config(vec![first.v4(), second.v4()]));

    for addr in [first.v4(), second.v4()] {
        let mut mta = Mta::connect(addr).await;
        assert_eq!(mta.negotiate().await.version, 6, "{addr}");
    }

    let listeners = server.handle.status().listeners;
    assert_eq!(listeners.len(), 2);
    for (listener, addr) in listeners.iter().zip([first.v4(), second.v4()]) {
        assert_eq!(listener.address, addr);
        assert!(listener.bound && listener.accepting, "{listener:?}");
    }
    server.stop().await;
}

#[tokio::test]
async fn srv_005_binds_no_address_beyond_bind_addresses() {
    let lease = PortLease::acquire().await;
    let Some(v6) = lease.v6_or_skip() else { return };
    let server = Running::start(config(vec![lease.v4()]));
    Mta::connect(lease.v4()).await;

    let result = tokio::net::TcpStream::connect(v6).await;

    assert!(result.is_err(), "nothing listens on {v6}");
    server.stop().await;
}

#[tokio::test]
async fn srv_006_fail_fast_stops_at_a_blocked_address() {
    let free = PortLease::acquire().await;
    let blocked = PortLease::acquire().await;
    let blocker = TcpListener::bind(blocked.v4()).unwrap();

    let result = run_result(config(vec![free.v4(), blocked.v4()])).await;

    assert!(
        matches!(result, Err(Error::Bind { addr, .. }) if addr == blocked.v4()),
        "{result:?}"
    );
    drop(blocker);
    TcpListener::bind(free.v4()).expect("the free port was released");
}

#[tokio::test]
async fn srv_007_best_effort_serves_the_addresses_that_bound() {
    let free = PortLease::acquire().await;
    let blocked = PortLease::acquire().await;
    let _blocker = TcpListener::bind(blocked.v4()).unwrap();
    let mut both = config(vec![free.v4(), blocked.v4()]);
    both.bind_mode = BindMode::BestEffort;
    let (logs, _guard) = Logs::capture();
    let server = Running::start(both);

    let mut mta = Mta::connect(free.v4()).await;

    assert_eq!(mta.negotiate().await.version, 6);
    let status = server.handle.status();
    let blocked_status = &status.listeners[1];
    assert_eq!(blocked_status.address, blocked.v4());
    assert!(!blocked_status.bound && !blocked_status.accepting);
    assert!(status.last_error.is_some());
    let warning = logs.one("Listener bind failed");
    assert_eq!(warning.level, tracing::Level::WARN);
    assert_eq!(
        warning.field("address"),
        Some(blocked.v4().to_string().as_str())
    );
    server.stop().await;
}

#[tokio::test]
async fn srv_008_best_effort_fails_when_no_address_binds() {
    let first = PortLease::acquire().await;
    let second = PortLease::acquire().await;
    let _blockers = [
        TcpListener::bind(first.v4()).unwrap(),
        TcpListener::bind(second.v4()).unwrap(),
    ];
    let mut both = config(vec![first.v4(), second.v4()]);
    both.bind_mode = BindMode::BestEffort;

    let result = run_result(both).await;

    assert!(matches!(result, Err(Error::Bind { .. })), "{result:?}");
}

#[tokio::test]
async fn srv_009_a_new_server_binds_the_port_of_a_stopped_one_at_once() {
    let lease = PortLease::acquire().await;
    let server = Running::start(config(vec![lease.v4()]));
    let mut mta = Mta::connect(lease.v4()).await;
    mta.negotiate().await;
    // The server closes the connection first, so its side waits in
    // TIME_WAIT.
    server.stop().await;
    assert!(mta.closes().await);

    let again = Running::start(config(vec![lease.v4()]));
    let mut mta = Mta::connect(lease.v4()).await;

    assert_eq!(mta.negotiate().await.version, 6);
    again.stop().await;
}
