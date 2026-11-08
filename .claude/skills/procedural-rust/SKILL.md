---
name: procedural-rust
description: Write and convert rust-milter's Rust code in the procedural style of ADR 0008 and ADR 0009 (match, if let, let-else, ?, loops; no closures, Fn bounds, function pointers, or methods that take a function). Use when writing any Rust in this repository, when a style check or the disallowed-methods step fails, or when converting a file or area off tests/style/exemptions.txt.
---

# Procedural Rust in rust-milter

The rule is in ADR 0008 and ADR 0009. The rewrite for each pattern, with compiled examples, is in `docs/how-to/write-procedural-rust.md`. Read that guide before you convert code; this skill adds the workflow around it.

## The rule in one screen

- Branch with `match`, `if let`, `let ... else`; return errors with `?`; repeat with `for`, `while`, `loop`.
- No method that takes a function. `clippy.toml` lists them under `disallowed-methods`. Methods that take no function (`unwrap_or`, `ok`, `then_some`, `enumerate`, `skip`, `zip`, `collect`) stay allowed.
- No closures, except the argument of `.handler(...)`, `.reload_with(...)`, `.call_once(...)`, `thread::spawn(...)`, and `std::thread::spawn(...)`. A closure inside one of those is not allowed. `async move { }` blocks are fine; `async |x|` closures are not.
- No `Fn`, `FnMut`, `FnOnce` (or `AsyncFn*`) bounds, except on the builder methods `handler` and `reload_with` in `src/server/mod.rs`. No `Box<dyn Fn...>`.
- No function pointer types (`fn(u8) -> bool`) anywhere. Use an enum and a `match`.
- New code follows the rule at once. Only files on `tests/style/exemptions.txt` may still break it.

## Traps

- `src/` must build with Rust 1.85, which has no let chains: nest `if let` and the condition. Test crates (Rust 1.89) may use let chains, and Clippy's `collapsible_if` will ask for them there.
- Clippy's `question_mark` rejects `let Some(x) = x else { return None; };`. Write `let x = x?;`.
- A plain `cargo clippy` does not report the disallowed methods: the workspace allows the lint while the exemption list exists. Run the filtered Clippy command below.
- The style check reads only `.rs` files. Review the Rust blocks of `docs/tutorials/` and `docs/how-to/` yourself.
- The decision to replace a stored closure (`Factory`, `Loader` in `src/server`) or an OpenTelemetry `with_callback` closure changes the public API or the metrics. Stop and ask the user, with the options and a recommendation, before you convert them.

## Convert an area

Work one area per pull request, in this order: `src/handler`, `src/config`, `src/engine`, `src/server`, the test files, `tests/support` last. When the list is empty, a final pull request turns the lint on for everyone: it removes the `disallowed_methods = "allow"` line from the workspace lints, so the normal Clippy job reports the methods, and removes the "Disallowed methods" CI step. Ask the user whether the exemption list, the `disallowed-methods` binary, and their scenarios go too.

1. Remove the area's files from `tests/style/exemptions.txt`. The list stays sorted.
2. List what is left to convert:

   ```sh
   cargo test -p rust-milter-style --test repository sty_010
   cargo clippy --workspace --all-targets --all-features --message-format=short -- -W clippy::disallowed_methods 2> /tmp/clippy-style.txt
   cargo run -p rust-milter-style --bin disallowed-methods -- /tmp/clippy-style.txt
   ```

3. Rewrite each finding with the pattern from the how-to guide. Keep behavior unchanged; the existing scenarios are the safety net. If a rewrite needs a new behavior or a new test, add the scenario to `docs/testing/` first (canon-tdd).
4. Run every check:

   ```sh
   cargo fmt --all --check
   cargo clippy --workspace --all-targets --all-features -- -D warnings
   cargo clippy --workspace --all-targets --no-default-features -- -D warnings
   cargo +1.85 check -p rust-milter --lib --all-features
   cargo +1.85 check -p rust-milter --lib --no-default-features
   cargo test --workspace --all-features
   cargo test --workspace --no-default-features
   ```

   plus the three commands of step 2, which must now report nothing.
5. Review the diff as a reviewer would: same behavior on every path, no leftover helper, clear names, doc comments that still match. Fix what you find and rerun the checks.
6. Push with the github-mcp skill: a new branch from `main`, one signed commit per area when it fits, the full content of each changed file. Batch the files so one call stays readable. Compare the returned tree with `git read-tree <base>; git add -A .; git write-tree`.
7. Open the pull request from `.github/pull_request_template.md`. Say what was converted, that behavior is unchanged, and what the review changed.
8. Wait for every CI job, including Clippy's "Disallowed methods" step. Squash-merge only with every job green, and only when the user has allowed it.

## Write new code

Write it in the style from the start, and run the check commands of step 4 before you push. A new file never goes on the exemption list.
