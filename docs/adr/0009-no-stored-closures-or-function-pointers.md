# ADR 0009: No stored closures or function pointers

- Status: Accepted; the exceptions are extended by [ADR 0010](0010-builder-factory-type-parameter-and-loader-trait.md)
- Date: 2026-11-08
- Affects: all Rust code in the repository, and the builder of `src/server`
- Supersedes: the exception of [ADR 0008](0008-rust-code-is-written-in-a-procedural-style.md) for the type aliases `Factory` and `Loader`

## Context

ADR 0008 bans closures and `Fn` bounds, with exceptions for the closures applications pass to `ServerBuilder::handler` and `ServerBuilder::reload_with`. It also lets the builder keep those closures: the type aliases `Factory` and `Loader` store them as `Box<dyn FnOnce(...)>` and `Box<dyn Fn(...)>`.

Two cases ADR 0008 left open came up while the rule was put into practice:
- A function pointer, such as `differs: fn(&ServerConfig, &ServerConfig) -> bool` in `src/config/reload.rs` or `type Setter = fn(&mut ServerConfig)` in the tests, carries code as a value, as a closure does. A table of function pointers hides which code runs behind an index or a field.
- Some methods take no function, such as `unwrap_or`, `ok`, `then_some`, `enumerate`, `skip`, `zip`, and `collect`. Neither check bans them, and ADR 0008 names only some of them.

## Options considered

For stored closures:
- (a) Keep the `Factory` and `Loader` exceptions.
- (b) Ban `Box<dyn Fn>`, `Box<dyn FnMut>`, and `Box<dyn FnOnce>` everywhere, the builder included.

For function pointers:
- (a) Allow them: a function pointer names a function and captures nothing.
- (b) Ban them, like closures.

For methods without a function argument:
- (a) Allow them.
- (b) Allow a single call, and ban chains of them.
- (c) Ban them as well.

## Decision

Option (b) for stored closures, option (b) for function pointers, and option (a) for methods without a function argument.

- No code stores a closure as `Box<dyn Fn>`, `Box<dyn FnMut>`, or `Box<dyn FnOnce>`. The style check stops allowing `Fn` bounds in `Factory` and `Loader`. Only the builder methods `handler` and `reload_with` keep their `Fn` bounds, because they take the closures applications pass.
- How the builder keeps the handler factory and the reload loader without a stored closure is decided when `src/server` is converted. Until then `src/server/mod.rs` and `src/server/reload.rs` stay on the exemption list.
- No code uses a function pointer type, as a parameter, a field, a type alias, or an element of a collection. A table of function pointers becomes an enum with a `match`. The style check reports every function pointer type.
- Methods that take no function stay allowed, alone or chained. The rule stays one sentence: a method that takes a function is banned.

## Consequences

Positive:
- Every call in the code base names the function it calls, or matches on a value to pick one.
- The rule for methods is mechanical, so Clippy checks it in full and reviewers do not judge chains.

Negative:
- The builder needs another way to keep the factory and the loader, which can change its public types or methods. The options are weighed when `src/server` is converted.
- Tables of function pointers, as in `src/config/reload.rs`, grow into an enum and a `match`.
- Chains of methods without a function argument, such as `iter().skip(1).zip(other).collect()`, stay allowed, though they read less like statements than a loop.

## References

- [ADR 0008](0008-rust-code-is-written-in-a-procedural-style.md)
- [test-environment.md, Style check](../reference/test-environment.md#style-check)
- [Write Rust in the procedural style](../how-to/write-procedural-rust.md)
