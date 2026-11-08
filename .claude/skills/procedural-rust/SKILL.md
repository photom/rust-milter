---
name: procedural-rust
description: Write rust-milter's Rust code in the procedural style of ADRs 0008 to 0011 (match, if let, let-else, ?, loops; no closures, Fn bounds, function pointers, or methods that take a function), and run its checks. Use when writing any Rust in this repository, or when the style check or the disallowed-methods step fails.
---

# Procedural Rust in rust-milter

The rule is in ADRs 0008, 0009, and 0010; ADR 0011 says how the methods are checked. The rewrite for each pattern, with compiled examples, is in `docs/how-to/write-procedural-rust.md`. Read that guide before you write or fix code; this skill adds the checks and the workflow around it.

## The rule in one screen

- Branch with `match`, `if let`, `let ... else`; return errors with `?`; repeat with `for`, `while`, `loop`.
- No method that takes a function. `clippy.toml` lists them under `disallowed-methods`. Methods that take no function (`unwrap_or`, `ok`, `then_some`, `enumerate`, `skip`, `zip`, `collect`) stay allowed. Passing a function name instead of a closure, as in `.map(PortLease::v4)`, breaks the rule too, even where no check catches it.
- No closures, except the argument of `.handler(...)`, `.call_once(...)`, `.with_callback(...)`, `thread::spawn(...)`, and `std::thread::spawn(...)`. A closure inside one of those is not allowed. `async move { }` blocks are fine; `async |x|` closures are not. A reload loader is a type that implements `ReloadLoader`, not a closure.
- No `Fn`, `FnMut`, `FnOnce` (or `AsyncFn*`) bounds, except on the builder methods `handler` and `build` in `src/server/mod.rs`. No `Box<dyn Fn...>`.
- No function pointer types (`fn(u8) -> bool`) anywhere. Use an enum and a `match`, or a table of values and a `match` on its key.
- No `allow` or `expect` of `clippy::disallowed_methods` in any file.
- Every Rust file follows the rule; there is no exemption list.

## Traps

- `src/` must build with Rust 1.85, which has no let chains: nest `if let` and the condition. Test crates (Rust 1.89) may use let chains, and Clippy's `collapsible_if` will ask for them there.
- Clippy's `question_mark` rejects `let Some(x) = x else { return None; };`. Write `let x = x?;`.
- A plain `cargo clippy` does not report the disallowed methods: the workspace allows the lint, because serde's derive generates `Option::map` for `#[serde(with = ...)]` fields (ADR 0011). Run the separate Clippy command below.
- `sort_by_key` takes a function. Derive `Ord` and call `sort`, or keep the order with an ordered insert in a loop.
- Test helpers take values, not closures: `TestServer::start(handler)`, `TestServer::start_with(config, handler)` with `support::config()`, and `support::wait_until(&handle, Until::...)` (testing ADR 0004). A handler that needs the `HandlerContext` passes its closure straight to `ServerBuilder::handler`.
- proptest's `prop_map` takes a function. Draw the raw values and build the input in the test body.
- The style check reads only `.rs` files. Review the Rust blocks of `docs/tutorials/` and `docs/how-to/` yourself.
- A change that needs a public API or behavior decision, as the builder's factory and loader did (ADR 0010), is the user's to make. Stop and ask, with the options and a recommendation.

## Check your code

```sh
cargo test -p rust-milter-style
cargo clippy --workspace --all-targets --all-features --message-format=short -- -W clippy::disallowed_methods 2> /tmp/clippy-style.txt
cargo run -p rust-milter-style --bin disallowed-methods -- /tmp/clippy-style.txt
```

Then run the rest of the checks before you push:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo clippy --workspace --all-targets --no-default-features -- -D warnings
cargo +1.85 check -p rust-milter --lib --all-features
cargo +1.85 check -p rust-milter --lib --no-default-features
cargo test --workspace --all-features
cargo test --workspace --no-default-features
```

## Fix a finding

1. Rewrite the finding with the pattern from the how-to guide. Keep behavior unchanged; the existing scenarios are the safety net. If a rewrite needs a new behavior or a new test, add the scenario to `docs/testing/` first (canon-tdd).
2. Run every check above until it reports nothing.
3. Review the diff as a reviewer would: same behavior on every path, no leftover helper, clear names, doc comments that still match. Fix what you find and rerun the checks.
4. Wait for every CI job, including the "Disallowed methods" step of the Clippy job, before a merge.
