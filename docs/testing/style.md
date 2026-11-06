# Test scenarios: Style

Scenarios for the style check, which keeps the Rust code in the procedural style of [ADR 0008](../adr/0008-rust-code-is-written-in-a-procedural-style.md). Conventions are in [README.md](README.md).

## One source file

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| STY-001 | unit | A file with a closure in a function body | the check reads the file | it reports the closure with its line | [test environment: Style check](../reference/test-environment.md#style-check) |
| STY-002 | unit | Closures passed to `handler`, `reload_with`, `call_once`, `thread::spawn`, and `std::thread::spawn`; a closure inside one of them; closures passed to `tokio::task::spawn_blocking` and another `spawn` | the check reads the file | it reports the inner closure and the two closures passed to the other calls, and nothing else | [test environment: Style check](../reference/test-environment.md#style-check) |
| STY-003 | unit | An `async move` block and an `async` closure | the check reads the file | it reports the `async` closure only | [test environment: Style check](../reference/test-environment.md#style-check) |
| STY-004 | unit | A closure in the arguments of `assert!` | the check reads the file | it reports the closure | [test environment: Style check](../reference/test-environment.md#style-check) |
| STY-005 | unit | `Fn` bounds in an `impl` parameter, a generic parameter, a `where` clause, a `dyn` type alias, and an `impl` return type | the check reads the file | it reports each bound with its trait and line | [test environment: Style check](../reference/test-environment.md#style-check) |
| STY-006 | unit | `FnOnce` bounds on the builder items `Factory` and `handler` of `src/server/mod.rs`, an `Fn` bound on another method there, and a function named `handler` in another file | the check reads each file | it reports the bound on the other method and the bound in the other file only | [test environment: Style check](../reference/test-environment.md#style-check) |
| STY-007 | unit | Files with `#![allow(clippy::disallowed_methods)]` at the top, with the lint in an item's `allow` among other lints, with it in an `expect` on a statement, with another lint's `allow`, and with no attribute | the check reads each file | it finds the lint allowed or expected in the first three files only | [test environment: Style check](../reference/test-environment.md#style-check) |

## The repository

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| STY-010 | unit | Every Rust file under `src/`, `tests/`, and `examples/` that is not on the exemption list | `cargo test` runs | no file has a closure or an `Fn` bound outside the exceptions; each finding fails the test with its path, line, and column | [test environment: Style check](../reference/test-environment.md#style-check) |
| STY-011 | unit | Every Rust file under `src/`, `tests/`, and `examples/` | `cargo test` runs | no file has an attribute that allows or expects `clippy::disallowed_methods` | [test environment: Style check](../reference/test-environment.md#style-check) |
| STY-012 | unit | The exemption list | `cargo test` runs | every listed file exists, and the list is sorted with no repeats | [test environment: Style check](../reference/test-environment.md#style-check) |

## Clippy's output

| ID | Level | Given | When | Then | Spec |
|----|-------|-------|------|------|------|
| STY-020 | unit | Clippy's short output with two disallowed method calls, one as a warning and one as an error, another warning, and Cargo's progress lines | the check reads the output | it gives the two calls with their file, line, column, and method, and nothing else | [test environment: Style check](../reference/test-environment.md#style-check) |
| STY-021 | unit | A call reported under a path through `..`, as for `tests/support` compiled into `tests/docs` | the check reads the output | the call has the path from the repository root | [test environment: Style check](../reference/test-environment.md#style-check) |
| STY-022 | unit | A call reported twice, with a call on an earlier line reported between the two | the check reads the output | it gives each call once, in source order | [test environment: Style check](../reference/test-environment.md#style-check) |
| STY-023 | unit | Calls in a listed file and in a file that is not listed | the check compares the calls with the list | it reports the call in the file that is not listed, with its place and method | [test environment: Style check](../reference/test-environment.md#style-check) |
| STY-024 | unit | Three listed files: one with a call, one with a closure and no call, and one with neither | the check compares the calls with the list | it reports the third file as having nothing left to convert | [test environment: Style check](../reference/test-environment.md#style-check) |
| STY-025 | unit | A call reported with terminal colors, as when `CARGO_TERM_COLOR` is `always` | the check reads the output | it gives the call as it would without colors | [test environment: Style check](../reference/test-environment.md#style-check) |
