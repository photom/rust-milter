# Write a network test

This guide follows [test-environment.md](../reference/test-environment.md) and [api-specification.md](../reference/api-specification.md).

This guide shows how to write an integration test that starts a server on a leased test port, talks to it, and stops it.

## 1. Lease a port

Integration tests share the helper `tests/support/ports.rs`. Name the test after its scenario ID from `docs/testing/`, take a lease at the start of the test, and keep it in a variable until the end:

<!-- Not compiled by the documentation check: `mod support;` loads a file from tests/, which a doctest cannot reach. -->
```rust,ignore
mod support;

use support::ports::PortLease;

#[tokio::test]
async fn eng_001_server_answers_option_negotiation() {
    let lease = PortLease::acquire().await;
    let addr = lease.v4(); // 127.0.0.1:<leased port>
    // ...
}
```

## 2. Start the server on the leased port

`ContinueAll` is a test handler in `tests/support` that implements no callbacks, so the library negotiates options and answers every command with `Continue`.

<!-- doctest-setup
use rust_milter_docs::support::ContinueAll;
use rust_milter_docs::support::ports::PortLease;

async fn start(lease: PortLease) {
---
let _ = (handle, task, lease);
}
-->
```rust
use rust_milter::{Server, ServerConfig};

let config = ServerConfig {
    bind_addresses: vec![lease.v4()],
    ..ServerConfig::default()
};
let server = Server::builder()
    .config(config)
    .handler(ContinueAll::new)
    .build()
    .unwrap();
let handle = server.handle();
let task = tokio::spawn(server.run());
```

## 3. Talk to the server

Connect a plain TCP stream to the leased address and exchange frames:

<!-- doctest-setup
async fn negotiate(addr: std::net::SocketAddr) {
---
}
-->
```rust
use bytes::BytesMut;
use rust_milter::protocol::{SMFIC_OPTNEG, SMFI_CURR_ACTS, SMFI_CURR_PROT};
use rust_milter::Frame;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

let mut mta = TcpStream::connect(addr).await.unwrap();

let mut payload = BytesMut::new();
payload.extend_from_slice(&6u32.to_be_bytes());
payload.extend_from_slice(&SMFI_CURR_ACTS.to_be_bytes());
payload.extend_from_slice(&SMFI_CURR_PROT.to_be_bytes());
let mut out = BytesMut::new();
Frame::new(SMFIC_OPTNEG, payload.freeze()).write_to(&mut out);
mta.write_all(&out).await.unwrap();

let mut buf = BytesMut::with_capacity(1024);
let reply = loop {
    mta.read_buf(&mut buf).await.unwrap();
    if let Some(frame) = Frame::parse(&mut buf, 65_540).unwrap() {
        break frame;
    }
};
assert_eq!(reply.command, SMFIC_OPTNEG);
```

## 4. Stop the server before the lease ends

<!-- doctest-setup
use rust_milter_docs::support::ports::PortLease;

async fn stop(
    handle: rust_milter::ServerHandle,
    task: tokio::task::JoinHandle<rust_milter::Result<()>>,
    lease: PortLease,
) {
---
}
-->
```rust
handle.shutdown();
task.await.unwrap().unwrap();
drop(lease);
```

The lease must outlive the server. Dropping it earlier lets another test take the port while this server still listens on it.

## Test two ports at once

Take each lease separately. A test may hold up to 3:

<!-- doctest-setup
use rust_milter::ServerConfig;
use rust_milter_docs::support::ports::PortLease;

async fn two_listeners() {
---
let _ = (config, first, second);
}
-->
```rust
let first = PortLease::acquire().await;
let second = PortLease::acquire().await;
let config = ServerConfig {
    bind_addresses: vec![first.v4(), second.v4()],
    ..ServerConfig::default()
};
```

## Test "address already in use"

Bind the leased port yourself first, then start the server on it:

<!-- doctest-setup
use rust_milter::{Server, ServerConfig};
use rust_milter_docs::support::ContinueAll;
use rust_milter_docs::support::ports::PortLease;

async fn bind_fails() {
---
}
-->
```rust
let lease = PortLease::acquire().await;
let _blocker = std::net::TcpListener::bind(lease.v4()).unwrap();

let config = ServerConfig {
    bind_addresses: vec![lease.v4()],
    ..ServerConfig::default()
};
let result = Server::builder()
    .config(config)
    .handler(ContinueAll::new)
    .build()
    .unwrap()
    .run()
    .await;
assert!(matches!(result, Err(rust_milter::Error::Bind { .. })));
```

## Test IPv6

Ask the lease for the IPv6 address, and let the helper skip the test where IPv6 is unavailable:

<!-- doctest-setup
use rust_milter_docs::support::ports::PortLease;

async fn ipv6() {
---
let _ = addr;
}
-->
```rust
let lease = PortLease::acquire().await;
let Some(addr) = lease.v6_or_skip() else { return };
```

`v6_or_skip()` follows `RUST_MILTER_TEST_IPV6`: it returns `None` and prints a skip notice when IPv6 tests are off or `::1` cannot be bound, and panics when they are required but unavailable.
