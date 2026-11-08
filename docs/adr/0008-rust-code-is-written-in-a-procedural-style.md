# ADR 0008: Rust code is written in a procedural style

- Status: Accepted; the exception for `Factory` and `Loader`, and the allowance of function pointers, are superseded by [ADR 0009](0009-no-stored-closures-or-function-pointers.md), the `reload_with` exceptions by [ADR 0010](0010-builder-factory-type-parameter-and-loader-trait.md), and the end of the transition, where the separate Clippy run goes away, by [ADR 0011](0011-the-disallowed-methods-run-stays-for-derive-code.md)
- Date: 2026-11-05
- Affects: all Rust code in the repository: the library, the tests, the test support code, the interop crate, and the examples

## Context

The code mixes two styles. Some functions branch with `match` and loop with `for`. Others chain `Option` and `Result` combinators and iterator adapters, each with a closure: `map_err(|error| ...)`, `ok_or_else(|| ...)`, `iter().filter(|x| ...).any(|y| ...)`. A chain hides its branches and early returns inside closure arguments, and the maintainer finds such code hard to read and review.

Clippy's default lints push the other way. `manual_map`, `manual_find`, `map_entry` and similar lints turn an explicit `match` or loop into the combinator with a closure.

## Options considered

- (a) No rule. Reviewers judge each change.
- (b) A written convention for the procedural style, checked only in review.
- (c) The procedural style, checked by Clippy and a syntax check in `cargo test`, with existing code converted one area at a time.

## Decision

Option (c).

- Code branches with `match`, `if let`, and `let ... else`, returns errors with `?`, and repeats with `for`, `while`, and `loop`.
- Code calls no method that takes a function as an argument: the `Option` and `Result` combinators, `bool::then`, the `Iterator` adapters and consumers that take a closure, and the `retain`, `dedup_by`, and `Entry` methods of the collections. `clippy.toml` lists them under `disallowed-methods`. Methods without a function argument stay allowed, such as `unwrap_or`, `ok`, `is_some`, `enumerate`, and `collect`.
- Code writes no closures, with four exceptions: the closure given to `ServerBuilder::handler`, to `ServerBuilder::reload_with`, to `std::thread::spawn`, and to `Once::call_once`. An `async` block is not a closure and stays allowed. An `async` closure counts as a closure.
- Code writes no bound on `Fn`, `FnMut`, or `FnOnce`, whether as an `impl` parameter, a generic bound, or a `dyn` type. The exceptions are the builder items that take or store the two builder closures: `handler`, `reload_with`, and the type aliases `Factory` and `Loader`.
- The workspace allows the Clippy lints whose fix uses a method with a closure: `manual_filter`, `manual_filter_map`, `manual_find`, `manual_find_map`, `manual_map`, `manual_retain`, and `map_entry`.
- Clippy checks the methods. A workspace crate, `tests/style`, parses every Rust file with `syn` and checks the closures and the `Fn` bounds.
- Files that are not converted yet are listed in `tests/style/exemptions.txt`, and both checks skip them. Clippy cannot skip a file by path, so the workspace allows `clippy::disallowed_methods`. A separate Clippy run in CI turns the lint on, and `tests/style` drops the calls in listed files from its output. Source files carry no lint attributes for the rule.
- Each pull request converts one area and removes its files from the list. New files follow the rule from the start, and the list only shrinks: a listed file with nothing left to convert fails the check. When the list is empty, the workspace stops allowing the lint and the separate run goes away.
- The rule covers the example code in `docs/tutorials/` and `docs/how-to/` as well. The style check reads only `.rs` files, so review checks the examples.

## Consequences

Positive:
- Branches and early returns appear as statements, where a reader can follow them.
- The code base ends up with a single style.
- CI fails on a violation in a converted file, so reviewers do not have to catch it.

Negative:
- Some code gets longer: a five-line `match` replaces a one-line `map`, and a loop replaces `any`.
- Applications still pass closures to `handler` and `reload_with`, since the builder API takes them.
- The style check sees syntax, not types. It misses closures in macros whose arguments do not parse as expressions, such as `tokio::select!` and `macro_rules!`.
- A plain `cargo clippy` does not report the methods while the list exists. Contributors run the separate Clippy command, or wait for CI.
- Function pointer parameters, such as `fn(&mut ServerConfig)`, fall outside both checks.

## References

- [test-environment.md, Style check](../reference/test-environment.md#style-check)
- [Test scenarios: Style](../testing/style.md)
