# Test environment specification

Status: design, not implemented.

The fixed TCP ports that rust-milter's tests listen on, how tests share them, and the other rules a test environment must meet. Why the tests use these ports and leases is explained in [Test ports](../explanation/test-ports.md). How to run the tests is in [Run the tests](../how-to/run-the-tests.md); how to write a test that listens on a port is in [Write a network test](../how-to/write-a-network-test.md).

## Contents

- [Test ports](#test-ports)
- [Port leases](#port-leases)
- [Environment variables](#environment-variables)
- [Rules for tests](#rules-for-tests)
- [Toolchain](#toolchain)

## Test ports

| Item | Value |
|------|-------|
| Port range | 61000 to 61009 (10 ports) |
| Addresses | `127.0.0.1` and `::1` |
| Protocol | TCP |

Tests listen only on loopback addresses and only on these ports. No test binds port 0, and no test uses a port outside the range.

A test that listens on IPv4 and IPv6 at the same time uses the same port number on both loopback addresses. One lease covers both.

## Port leases

A test that needs a port takes a lease on it, and holds the lease until the test ends. Leases make 10 ports enough for a test suite that runs many tests in parallel.

| Property | Behavior |
|----------|----------|
| Lock | An exclusive file lock on `<temp dir>/rust-milter-test-ports/<port>.lock`, where `<temp dir>` is the system temporary directory |
| Scope | Every test process on the machine, including other checkouts of the repository and `cargo-nextest`, which runs each test in its own process |
| Order | The lease tries the ports from the lowest up and takes the first one whose lock is free |
| Free check | After locking, the lease binds the port on `127.0.0.1` once and closes it at once. If the bind fails, another program uses the port; the lease releases the lock and tries the next port |
| Waiting | If all 10 ports are leased, the lease waits and retries every 100 ms, for up to 60 seconds. It then fails the test with a message that names the busy ports |
| Release | Dropping the lease releases the lock. If the test process crashes, the operating system releases the lock when the process exits, so no stale lock remains |
| Several ports | A test can take up to 3 leases at once, for example to test a partial bind failure |

A lease does not keep the port bound. The server under test binds it after the test has taken the lease.

## Environment variables

| Variable | Default | Effect |
|----------|---------|--------|
| `RUST_MILTER_TEST_PORT_BASE` | 61000 | First port of the range |
| `RUST_MILTER_TEST_PORT_COUNT` | 10 | Number of ports in the range; between 3 and 100 |
| `RUST_MILTER_TEST_IPV6` | `auto` | `auto` runs IPv6 tests if `::1` can be bound and skips them otherwise; `on` fails them if it cannot; `off` skips them |

## Rules for tests

1. Take a lease before binding a port, and keep it until the server under test has stopped.
2. Bind only the leased port, and only on `127.0.0.1` or `::1`.
3. Use the server's default `reuse_addr = true`. A test that stops a server and starts another on the same port in quick succession would otherwise fail on connections left in `TIME_WAIT`.
4. Stop servers through `ServerHandle::shutdown()` and wait for them before the lease is dropped. Only process-level signal tests send real signals.
5. Run timeouts on Tokio's paused clock (`tokio::time::pause`) wherever the test does not need real network delays, so that long timeouts take no real time.
6. Tests that need IPv6 follow `RUST_MILTER_TEST_IPV6` and report a skip, not a pass, when IPv6 is unavailable.

## Toolchain

The library's minimum supported Rust version stays 1.85. The test support code locks files with `std::fs::File::try_lock`, stable since Rust 1.89, so running the tests needs Rust 1.89 or later. The MSRV check in CI builds the library without its tests.
