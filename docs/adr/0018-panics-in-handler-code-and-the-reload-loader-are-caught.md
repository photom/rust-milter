# ADR 0018: Panics in handler code and the reload loader are caught

- Status: Accepted
- Date: 2026-11-09
- Affects: the protocol engine, the reload coordinator, `Error`, `ReloadError::Load`, telemetry

## Context

The library runs code the application writes: the `Handler` methods, on each connection's task, and the reload loader's `load`, on the reload coordinator's task. Rust code can panic, for example on an `unwrap()` of `None` or an index out of bounds.

Without handling, a panic ends the task it happens on, and Tokio keeps it from reaching the rest of the process:
- A panic in a callback ends the connection's task. The socket is dropped, so the MTA applies its failure policy, and the connection count and memory reservation are released. But no further callback runs, the "Connection closed" log is not written, and the close reason is not counted. The only trace is the panic message, which the default panic hook prints to standard error.
- A panic in the loader ends the reload coordinator. The reload that ran it fails with `ShuttingDown`, every later reload fails the same way while the server keeps running, `SIGHUP` is dropped, and `Server::run()` raises the panic again at shutdown.

A panic can also abort the process at once, when the application builds with `panic = "abort"`. The library cannot change that.

`std::panic::catch_unwind` takes a closure, which [ADR 0017](0017-rust-code-is-written-in-a-procedural-style.md) does not allow here. `futures-util`, already in the dependency tree through `tokio-util`, offers `FutureExt::catch_unwind`, a method without a function argument.

## Options considered

- (a) Catch nothing, and document that a handler and a loader must not panic.
- (b) Catch a panic in every `Handler` method and in the loader.
- (c) Catch a panic in the loader only, and document what a panic in a callback does.

## Decision

Option (b).

- Each call of a `Handler` method runs inside `catch_unwind`, through `futures-util`'s `FutureExt::catch_unwind`. That covers the callbacks and the synchronous `events()` and `actions()`.
- A panic in a callback ends the session and closes the connection with the new error `Error::HandlerPanic`. No further callback runs for that session: not `abort`, not `session_end`, because the handler's `Session` and `Message` values may be half-updated. The library drops them. The connection closes with the close reason `error`, `error.type` `HandlerPanic`, and the WARN form of "Connection closed".
- A panic in `abort` or `session_end` while a session ends is handled the same way, and stops the callbacks that would follow.
- A panic in `load` fails the reload with `ReloadError::Load`, whose source says that the loader panicked. The coordinator keeps running, the generation does not change, and later reloads run as usual.
- Neither the log nor the error carries the panic's message, which can hold data from an SMTP session. The application's panic hook, by default printing to standard error, still sees it.
- The handler factory runs once, in `build()`, on the application's own call. A panic there reaches the application as any panic in its own code does.

## Consequences

Positive:
- One bad configuration file no longer disables reloading until the process restarts.
- A panic in a handler shows up like any other connection failure: in the close log, the close-reason metrics, and the connection span.
- `Server::run()` returns normally at shutdown after a panic in a handler or the loader.

Negative:
- `futures-util` becomes a direct dependency. It was already compiled as a dependency of `tokio-util`.
- `Error` gets the variant `HandlerPanic`. `Error` is `#[non_exhaustive]`, so that is not a breaking change.
- A handler that keeps state shared across connections, such as a `Mutex`, can leave it poisoned or inconsistent after a panic. The library cannot see that state.
- A build with `panic = "abort"` still ends the process on any panic.

## References

- [api-specification.md, Callbacks](../reference/api-specification.md#callbacks)
- [api-specification.md, Errors](../reference/api-specification.md#errors)
- [hot-reload.md, Reload sequence](../reference/hot-reload.md#reload-sequence)
- [tcp-server.md, Connection lifecycle logs](../reference/tcp-server.md#connection-lifecycle-logs)
