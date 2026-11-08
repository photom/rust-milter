# ADR 0010: The builder keeps the handler factory in a type parameter and takes a loader trait

- Status: Accepted
- Date: 2026-11-08
- Affects: the builder API of `src/server`, the style check
- Supersedes: the `reload_with` exceptions of [ADR 0008](0008-rust-code-is-written-in-a-procedural-style.md); adds to the exceptions of [ADR 0009](0009-no-stored-closures-or-function-pointers.md)

## Context

ADR 0009 bans stored closures and left open how the builder keeps the two closures applications pass to it. `ServerBuilder::handler` takes a handler factory, which `build` calls once with a `HandlerContext`. `ServerBuilder::reload_with` takes a reload loader, which the server calls on every `SIGHUP` and `ServerHandle::reload` for its whole lifetime. Both were stored as `Box<dyn Fn...>`.

The OpenTelemetry metrics raise a third case. Observable counters and gauges exist only through `with_callback`, and OpenTelemetry's API takes a closure there. `src/server/otel.rs` has four such callbacks and a table of `Box<dyn Fn>` values that each counter observes.

## Options considered

For the handler factory:
- (a) A type parameter on `ServerBuilder` that stores the closure, so `build` calls it.
- (b) Call the factory inside `handler`, which would then need the `HandlerContext` before `config` and `app_config` are final.
- (c) Take a handler value instead of a factory.

For the reload loader:
- (a) A public trait that the application implements on its own type.
- (b) A type parameter on `Server`, `ServerBuilder`, and the reload coordinator.
- (c) Keep the closure behind a private trait object.

For the OpenTelemetry callbacks:
- (a) Allow the closure given to `with_callback`, and replace the table with an enum.
- (b) Use synchronous instruments, updated when each event happens.

## Decision

Option (a) in each case.

- `ServerBuilder` gets a third type parameter that holds the handler factory, so no closure is stored as a trait object. The closure given to `handler` stays allowed, and `build`, which calls it, joins `handler` as a builder method that may have an `FnOnce` bound.
- The reload loader is a public trait, `ReloadLoader<A>`, with an `Error` type and an async `load` method. `reload_with` takes a value of a type that implements it. The server keeps it behind a private object-safe trait of its own. `reload_with` takes no closure, so it leaves the closure and `Fn`-bound exceptions.
- The closure given to OpenTelemetry's `with_callback` is allowed. What each counter observes becomes an enum with a `match`.

The allowed closure calls are now `handler`, `call_once`, `with_callback`, `thread::spawn`, and `std::thread::spawn`. Only `handler` and `build` in `src/server/mod.rs` may have `Fn` bounds.

## Consequences

Positive:
- No closure is stored as a trait object anywhere.
- A reload loader is a named type, which can hold its own state, such as a file path, as fields.
- The metrics keep their current behavior.

Negative:
- `reload_with` changes its signature: applications write a small type and an `impl ReloadLoader` instead of a closure. rust-milter has no release yet, so no published API breaks.
- Code that writes out the `ServerBuilder` type gets a third parameter.
- `with_callback` closures remain, because the OpenTelemetry API requires them.

## References

- [api-specification.md, Configuration and reload API](../reference/api-specification.md#configuration-and-reload-api)
- [test-environment.md, Style check](../reference/test-environment.md#style-check)
- [Write Rust in the procedural style](../how-to/write-procedural-rust.md)
