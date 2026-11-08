# Testing ADR 0005: Dependency advisories are checked every day

- Status: Accepted
- Date: 2026-11-08
- Affects: CI
- Extends: [testing ADR 0002](0002-ci-scans-with-cargo-deny-clippy-and-codeql.md), which runs cargo-deny on every push and pull request

## Context

Testing ADR 0002 runs cargo-deny's checks, the RustSec advisory database among them, on every push to `main` and on every pull request. A new advisory against a dependency in `Cargo.lock` appears without any change to the repository, so it stays unseen until the next push. Dependabot's update pull requests come once a week, and only when a newer version exists, so a vulnerable version can stay for days without anyone noticing.

## Options considered

- (a) A scheduled workflow that runs cargo-deny's advisory check every day, and on demand.
- (b) The same, every week, together with CodeQL's weekly run.
- (c) Rely on GitHub's Dependabot security alerts, a repository setting outside the code.

## Decision

Option (a). `.github/workflows/advisories.yml` runs `cargo deny --all-features check advisories` every day at 05:23 UTC, and when started by hand. It checks advisories only: licenses, bans, and sources change only with the code, and `ci.yml` checks them then.

## Consequences

Positive:
- A new advisory fails a CI run within a day, with the advisory ID in the job log.
- The check is cheap: it reads `Cargo.lock` and the advisory database and builds nothing.

Negative:
- GitHub pauses scheduled workflows in a repository with no activity for 60 days, so a dormant repository stops getting the daily check until someone pushes or starts the workflow by hand.
- GitHub sends the failure notice of a scheduled run only to the person who last changed the workflow's schedule. The run opens no issue or pull request.

## References

- [test-environment.md, CI](../../reference/test-environment.md#ci)
- [testing ADR 0002](0002-ci-scans-with-cargo-deny-clippy-and-codeql.md)
