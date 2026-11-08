# Testing ADR 0004: The test server takes values and waits for named states

- Status: Accepted
- Date: 2026-11-08
- Affects: `tests/support`, the integration tests

## Context

[ADR 0008](../../adr/0008-rust-code-is-written-in-a-procedural-style.md) bans closures and `Fn` bounds in all Rust code of the repository, tests included. Three helpers of `tests/support/mod.rs` took closures, and about 70 calls in the integration tests passed them one:
- `TestServer::start` took a handler factory, such as `move |_| handler`.
- `TestServer::start_with` took a factory and a function that changed the configuration, such as `|config| config.max_connections = 2`.
- `support::wait_until` polled a condition given as a closure, such as `|| handle.status().accepting`.

No test used the `HandlerContext` the factory receives. The conditions passed to `wait_until` were four: the server accepts, no connection is open, the server rejected a number of connections, and the configuration generation reached a number.

## Options considered

For the handler and the configuration:
- (a) `TestServer::start` takes a handler value, and `start_with` takes a handler value and a `ServerConfig`. `support::config()` gives the configuration of a test, which the test changes before it starts the server.
- (b) Keep a configuration step as a trait that each test implements on a small type.

For the waiting condition:
- (a) An enum of the states, which `wait_until` checks with a `match`.
- (b) A polling loop with its deadline in each test that waits.

## Decision

Option (a) in both cases.

- `support::config()` returns `ServerConfig::default()` with a grace period of zero and no bind address. `TestServer::start(handler)` starts a server with it, and `TestServer::start_with(config, handler)` with the configuration the test gives. Both replace the bind addresses with the leased port. Inside, `TestServer` passes `move |_| handler` to `ServerBuilder::handler`, which [ADR 0010](../../adr/0010-builder-factory-type-parameter-and-loader-trait.md) allows.
- `support::wait_until(&handle, until)` waits for one of the states of the enum `Until`: `Accepting`, `Idle`, `Rejected(n)`, and `Generation(n)`. A test that needs another state adds a variant.

## Consequences

Positive:
- Tests start servers with values, so no test needs a closure to start one.
- A test that changes the configuration names each setting on a `ServerConfig` it builds, as an application does.
- The states a test can wait for are listed in one place.

Negative:
- A test whose handler needs the `HandlerContext` cannot use `TestServer`. It builds its server with `Server::builder()`, as the metrics tests already do.
- A new waiting condition needs a variant of `Until` and an arm of its `match`.
