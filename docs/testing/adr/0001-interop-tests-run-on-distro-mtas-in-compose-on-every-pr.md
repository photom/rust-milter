# Testing ADR 0001: Interoperability tests run against distribution MTAs, in Docker Compose, on every pull request

- Status: Accepted
- Date: 2026-11-01
- Affects: test environment, CI

## Context

The interoperability scenarios (`INT`) run rust-milter behind real Postfix and Sendmail. The protocol reference was checked against the sources of Postfix 3.12 and Sendmail 8.19.0.2. Linux distributions ship older releases of both MTAs.

Three questions needed answers: which MTA builds to test against, how to drive the tests, and when CI runs them.

## Options considered

MTA builds:
- (a) Build the verified versions from source in the test images.
- (b) Install the packages of a pinned distribution release.
- (c) Both.

Test driver:
- (a) Rust tests in a separate test crate, against a Docker Compose stack.
- (b) Shell scripts that send mail with `swaks` and search the results.

CI trigger:
- (a) Every pull request.
- (b) Nightly and on demand.
- (c) Manually, before a release.

## Decision

- MTA builds: option (b). The images install Postfix and Sendmail from the packages of one pinned Debian release.
- Test driver: option (a). A test crate holds the `INT` tests, `#[ignore]`d by default, named by scenario ID like every other test. The tests run inside a Docker Compose stack.
- CI trigger: option (a). Every pull request runs the interoperability tests next to `cargo test`.

## Consequences

Positive:
- The packaged MTAs are what most operators run, so the tests check the versions rust-milter meets in production.
- The images build in seconds from packages, which keeps a run on every pull request affordable.
- A protocol regression fails the pull request that causes it.
- Scenario IDs, assertions, and test helpers stay in one language with the rest of the suite.

Negative:
- The tested MTA versions differ from the versions the protocol reference was checked against. A difference between them shows up as a failing scenario, and is then recorded in the reference.
- The tested versions change when the pinned Debian release changes. Each run logs both MTA versions, so a new failure can be traced to an upgrade.
- Every pull request waits for the Compose stack to build and run.

## References

- [test-environment.md, Interoperability environment](../../reference/test-environment.md#interoperability-environment)
- [Interoperability scenarios](../interoperability.md)
- [milter-protocol.md](../../reference/milter-protocol.md)
