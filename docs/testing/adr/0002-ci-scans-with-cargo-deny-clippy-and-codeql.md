# Testing ADR 0002: CI scans dependencies with cargo-deny, and code with Clippy and CodeQL

- Status: Accepted
- Date: 2026-11-01
- Affects: CI

## Context

The working rules in `.claude/CLAUDE.md` require CI to scan third-party dependencies for known vulnerabilities (CVEs) and to run the standard static analysis tools for the language. rust-milter is written in Rust, hosted on GitHub, and runs its CI on GitHub Actions.

## Options considered

Dependency scanning:
- (a) cargo-deny: the RustSec advisory database, plus licenses, banned or duplicate crates, and allowed sources, from one `deny.toml`.
- (b) cargo-audit: the RustSec advisory database only.

Static analysis:
- (a) Clippy with warnings as errors, plus GitHub CodeQL's Rust analysis.
- (b) Clippy, plus Semgrep's Rust rules.
- (c) Clippy only.

## Decision

- Dependency scanning: option (a), cargo-deny, on every push and pull request. Dependabot also opens weekly update pull requests for Cargo dependencies, GitHub Actions, and the interop base images.
- Static analysis: option (a). Clippy runs with `-D warnings` for every feature set on every push and pull request. CodeQL analyzes the Rust code on every push, pull request, and once a week, and reports to the repository's Security tab.
- Every GitHub Action is pinned to a full commit SHA, with its release tag in a comment, so a moved tag cannot change what CI runs.

## Consequences

Positive:
- The license check keeps every dependency compatible with distributing rust-milter under MIT.
- CodeQL results stay in GitHub next to Dependabot alerts.

Negative:
- CodeQL runs only on GitHub. A move to another CI service needs another SAST tool.
- Pinned actions need Dependabot's pull requests to stay current.

## References

- [test-environment.md, CI](../../reference/test-environment.md#ci)
- `deny.toml`, `.github/workflows/ci.yml`, `.github/workflows/codeql.yml`, `.github/dependabot.yml`
