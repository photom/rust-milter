# ADR 0011: The disallowed-methods run stays for derive code

- Status: Accepted
- Date: 2026-11-08
- Affects: the style check, CI, the workspace lints
- Supersedes: the end of the transition in [ADR 0008](0008-rust-code-is-written-in-a-procedural-style.md), where the workspace stops allowing `clippy::disallowed_methods` and the separate Clippy run goes away

## Context

ADR 0008 converted the code file by file. The files not converted yet were listed in `tests/style/exemptions.txt`. Clippy cannot skip a file by path, so the workspace allowed `clippy::disallowed_methods`, and a separate Clippy run in CI turned the lint on and passed its output to the `disallowed-methods` binary of `tests/style`, which dropped the calls in listed files. ADR 0008 planned that, once the list was empty, the workspace would stop allowing the lint and the separate run would go away.

The list is now empty. Turning the lint on for the whole workspace makes Clippy fail with the `serde` feature. serde's derive expands each `#[serde(with = ...)]` field into code that calls `Option::map`, and Clippy reports that call on the line of the `derive` attribute in `src/config/mod.rs`. An `allow` or `expect` on the struct does not reach the derived code, and a module-wide `allow` would hide calls written by hand in the whole module. The binary already ignores calls reported on `derive` lines (STY-026).

## Options considered

- (a) Keep the workspace allow, the separate run, and the binary. Remove the exemption list, its code, and its scenarios.
- (b) Write `Serialize` and `Deserialize` for the configuration structs by hand, without `serde(with = ...)`, then remove the allow, the run, the binary, and the list.
- (c) Move the structs that derive serde into a module that allows the lint, and let the style check accept that one module.

## Decision

Option (a).

- The workspace keeps `disallowed_methods = "allow"`, and the "Disallowed methods" step of the CI Clippy job stays.
- The `disallowed-methods` binary fails on every disallowed method call except one that Clippy reports on the lines of a `derive` attribute.
- `tests/style/exemptions.txt` and the code that reads it are removed. STY-012 and STY-024, which checked the list, are withdrawn.
- No source file allows or expects `clippy::disallowed_methods` (STY-011).

## Consequences

Positive:
- New code is checked as the converted code was, with no list to keep.
- serde's derive keeps the configuration's file format with no hand-written serializers.

Negative:
- A plain `cargo clippy` still does not report the disallowed methods. Contributors run the separate Clippy command, or wait for CI.
- CI runs Clippy one more time than it would without the binary.

## References

- [ADR 0008](0008-rust-code-is-written-in-a-procedural-style.md)
- [test-environment.md, Style check](../reference/test-environment.md#style-check)
- [Run the tests, Run the style check](../how-to/run-the-tests.md#run-the-style-check)
