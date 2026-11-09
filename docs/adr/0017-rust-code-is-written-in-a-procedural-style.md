# ADR 0017: Rust code is written in a procedural style

- Status: Accepted
- Date: 2026-11-09
- Supersedes: [ADR 0008](0008-rust-code-is-written-in-a-procedural-style.md), [ADR 0009](0009-no-stored-closures-or-function-pointers.md), [ADR 0010](0010-builder-factory-type-parameter-and-loader-trait.md), and [ADR 0011](0011-the-disallowed-methods-run-stays-for-derive-code.md), which together held this decision
- Affects: all Rust code in the repository: the library, the tests, the test support code, the interop crate, and the examples; the builder API of `src/server`; the style check, CI, and the workspace lints

## Context

Rust code can branch and loop in two styles. One uses `match`, `if let`, and `for`. The other chains `Option` and `Result` combinators and iterator adapters, each with a closure: `map_err(|error| ...)`, `ok_or_else(|| ...)`, `iter().filter(|x| ...).any(|y| ...)`. A chain hides its branches and early returns inside closure arguments, and the maintainer finds such code hard to read and review. Clippy's default lints push toward the second style: `manual_map`, `manual_find`, `map_entry`, and similar lints turn an explicit `match` or loop into a combinator with a closure.

Code passed around as a value hides which code runs in the same way. A closure stored as `Box<dyn Fn>`, or a function pointer such as `fn(&ServerConfig, &ServerConfig) -> bool` in a table, puts the code behind a field or an index.

Three places need code from elsewhere:
- `ServerBuilder::handler` takes a handler factory, which `build` calls once with a `HandlerContext`.
- `ServerBuilder::reload_with` takes a reload loader, which the server calls on every `SIGHUP` and `ServerHandle::reload()` for its whole lifetime.
- OpenTelemetry's observable counters and gauges exist only through `with_callback`, and its API takes a closure there.

Clippy checks method calls, but it cannot skip a file by path, and serde's derive expands each `#[serde(with = ...)]` field into code that calls `Option::map`. Clippy reports that call on the line of the `derive` attribute in `src/config/mod.rs`. An `allow` or `expect` on the struct does not reach the derived code, and a module-wide `allow` would hide calls written by hand in the whole module.

## Options considered

For the rule itself:
- (a) No rule. Reviewers judge each change.
- (b) A written convention, checked only in review.
- (c) The procedural style, checked by Clippy and by a syntax check in `cargo test`.

For stored closures and function pointers:
- (a) Allow them where an API stores code.
- (b) Ban both, and name the cases with an enum and a `match`.

For methods that take no function, such as `unwrap_or`, `ok`, `then_some`, `enumerate`, `skip`, `zip`, and `collect`:
- (a) Allow them, alone or chained.
- (b) Allow a single call, and ban chains of them.
- (c) Ban them as well.

For the handler factory:
- (a) A type parameter on `ServerBuilder` that holds the closure until `build` calls it.
- (b) Call the factory inside `handler`, which would then need the `HandlerContext` before `config` and `app_config` are final.
- (c) Take a handler value instead of a factory.

For the reload loader:
- (a) A public trait that the application implements on its own type.
- (b) A type parameter on `Server`, `ServerBuilder`, and the reload coordinator.
- (c) A closure behind a private trait object.

For the OpenTelemetry callbacks:
- (a) Allow the closure given to `with_callback`, and name what each counter observes with an enum.
- (b) Use synchronous instruments, updated when each event happens.

For the method check and derive code:
- (a) Allow the lint in the workspace, and turn it on in a separate Clippy run whose output a small binary filters, dropping calls reported on `derive` lines.
- (b) Write `Serialize` and `Deserialize` for the configuration structs by hand, without `serde(with = ...)`, and turn the lint on everywhere.
- (c) Move the structs that derive serde into one module that allows the lint.

## Decision

Option (c) for the rule, (b) for stored closures and function pointers, and (a) for every other question.

- Code branches with `match`, `if let`, and `let ... else`, returns errors with `?`, and repeats with `for`, `while`, and `loop`.
- Code calls no method that takes a function as an argument: the `Option` and `Result` combinators, `bool::then`, the `Iterator` adapters and consumers that take a closure, `iter::from_fn`, `repeat_with`, and `successors`, and the `retain`, `dedup_by`, and `Entry` methods of the collections. `clippy.toml` lists them under `disallowed-methods`. Methods that take no function stay allowed, alone or chained.
- Code writes a closure only as the argument of `ServerBuilder::handler`, OpenTelemetry's `with_callback`, `std::thread::spawn`, or `Once::call_once`. A closure inside one of these closures is not allowed. An `async` block is not a closure; an `async` closure is.
- Code writes no bound on `Fn`, `FnMut`, `FnOnce`, `AsyncFn`, `AsyncFnMut`, or `AsyncFnOnce`, except on the builder methods `handler` and `build` in `src/server/mod.rs`.
- No code stores a closure as `Box<dyn Fn>`, `Box<dyn FnMut>`, or `Box<dyn FnOnce>`, and no code uses a function pointer type, as a parameter, a field, a return type, a type alias, or an element type. A table of code becomes an enum with a `match`.
- `ServerBuilder` has a third type parameter that holds the handler factory, so the factory is never stored as a trait object.
- The reload loader is the public trait `ReloadLoader<A>`, with an `Error` type and an async `load` method. `reload_with` takes a value of a type that implements it, and the server keeps that value behind a private object-safe trait of its own.
- What each OpenTelemetry counter observes is an enum with a `match`, not a table of closures.
- The workspace allows the Clippy lints whose fix uses a method with a closure: `manual_filter`, `manual_filter_map`, `manual_find`, `manual_find_map`, `manual_map`, `manual_retain`, and `map_entry`.
- The unpublished workspace crate `tests/style` parses every Rust file with `syn` and reports closures, `Fn` bounds, function pointer types, and attributes that allow or expect `clippy::disallowed_methods`.
- The workspace allows `clippy::disallowed_methods`. The CI Clippy job runs Clippy again with the lint turned on, and the `disallowed-methods` binary of `tests/style` reads its output and fails on every call except one reported on the lines of a `derive` attribute. No source file allows or expects the lint.
- No file is exempt: the checks have no list of files to skip.
- The rule covers the example code in `docs/tutorials/` and `docs/how-to/` as well. The style check reads only `.rs` files, so review checks the examples.

## Consequences

Positive:
- Branches and early returns appear as statements, where a reader can follow them, and the code base has a single style.
- Every call names the function it calls, or matches on a value to pick one. No closure is stored as a trait object anywhere.
- The method rule is mechanical, so Clippy checks it in full and reviewers do not judge chains.
- CI fails on a violation, so reviewers do not have to catch it.
- A reload loader is a named type, which can hold its own state, such as a file path, as fields.
- serde's derive keeps the configuration's file format with no hand-written serializers.

Negative:
- Some code gets longer: a five-line `match` replaces a one-line `map`, and a loop replaces `any`.
- Applications write a small type and an `impl ReloadLoader` instead of passing a closure, and code that writes out the `ServerBuilder` type names three parameters.
- `with_callback` closures remain, because the OpenTelemetry API requires them.
- The style check sees syntax, not types. It misses closures in macros whose arguments do not parse as expressions, such as `tokio::select!` and `macro_rules!`.
- A plain `cargo clippy` does not report the disallowed methods. Contributors run the separate Clippy command, or wait for CI, and CI runs Clippy one more time than it would without the binary.
- Chains of methods without a function argument, such as `iter().skip(1).zip(other).collect()`, stay allowed, though they read less like statements than a loop.

## References

- [test-environment.md, Style check](../reference/test-environment.md#style-check)
- [api-specification.md, Configuration and reload API](../reference/api-specification.md#configuration-and-reload-api)
- [Write Rust in the procedural style](../how-to/write-procedural-rust.md)
- [Run the tests, Run the style check](../how-to/run-the-tests.md#run-the-style-check)
- [Test scenarios: Style](../testing/style.md)
