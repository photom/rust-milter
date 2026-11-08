# Testing ADR 0005: Load and chaos tests run weekly, against a milter started as a child process

- Status: Accepted
- Date: 2026-11-08
- Affects: test environment, CI

## Context

[load-and-chaos.md](../load-and-chaos.md) lists 4 load scenarios (`LOD`) and 8 chaos scenarios (`CHS`). They run outside `cargo test`, against a release build with a handler that continues at every stage. Several of them reach past the library's API:

- CHS-003 lowers the process's file descriptor limit, and CHS-005 and CHS-006 send `SIGTERM` and `SIGHUP`. A test can only do that to a separate process.
- LOD-001 compares resident memory over an hour, and LOD-002 counts open file descriptors. Both read the process's entries in `/proc`.
- CHS-001 compares the server's `status()` with the generator's count of open connections.
- CHS-002 and CHS-008 drop every packet between the generator and the server, which needs `iptables` or `tc` and the `NET_ADMIN` capability.

The scenarios take time: LOD-001 alone runs for an hour, and a full run takes about two. LOD-003 sets latency limits of 100 µs at p50, 1 ms at p99, and 10 ms at p99.9, and GitHub's hosted runners are shared 4-vCPU machines.

The maintainer decided some points on 2026-11-05: the tests run in a Docker Compose stack like the interoperability one, with `NET_ADMIN`; a scheduled GitHub Actions workflow runs them weekly and on demand, not on every pull request; and the load generator is a crate of its own under `tests/`, separate from the milter client sample. Four questions were left open:

- Where the milter under test runs.
- How the scenarios are driven.
- What the scheduled run does with the latency limits.
- How a change to the tests gets checked before it reaches `main`. GitHub runs `schedule` and `workflow_dispatch` only for a workflow file that is already on the default branch.

## Options considered

Topology:
- (a) One Compose service. The generator starts the milter under test as a child process, as INT-050 does with `int-milter`.
- (b) Two services, one for the milter and one for the generator, with a control port on the milter for status, signals, and memory readings.

Driver:
- (a) `#[ignore]`d Rust tests named by scenario ID, as for the `INT` tests.
- (b) A generator binary with one subcommand per scenario, which the workflow calls one by one.

Latency limits:
- (a) Fail the run when a percentile misses its limit.
- (b) Report the percentiles, and fail only on a gross regression.

Triggers:
- (a) Weekly and on demand at full length, plus a short run on pull requests that change the load tests.
- (b) The same, with the full run on those pull requests.
- (c) Weekly and on demand only, with a temporary trigger during development.

## Decision

Topology (a), driver (a), latency (a), and triggers (a).

- A crate `tests/load`, not published, holds the load generator as its library, the milter under test as a binary, and the `LOD` and `CHS` tests. The generator speaks the MTA side of the protocol with the library's `Frame`.
- The tests are `#[ignore]`d, their names start with the scenario ID (`lod_001_...`, `chs_001_...`), and they run one at a time, in release mode.
- Each test starts the milter as a child process. It sends signals with `kill`, sets the file descriptor limit when it starts the child, reads memory and file descriptors from `/proc`, and asks for `status()` over the child's standard input and output.
- The stack has one service, with `NET_ADMIN`, a file descriptor limit high enough for both ends of 10,000 connections, and no network: the generator and the milter talk over loopback, and a partition is an `iptables` rule that drops loopback packets to and from the milter's port.
- LOD-003 fails when a percentile misses its limit. LOD-003 and LOD-004 print the percentiles they measure.
- The workflow `load.yml` runs every scenario at full length weekly and on manual dispatch. It also runs on pull requests that change `tests/load/` or the workflow file, in a short mode that cuts the load scenarios' durations to a few minutes and keeps their connection counts and pass criteria. The chaos scenarios already take two minutes or less, and run as written in both modes.

## Consequences

Positive:
- A test controls the milter's process with ordinary tools, with no control protocol to write and test.
- Scenario IDs, assertions, and helpers stay in Rust, as in the rest of the suite, and `cargo test -p rust-milter-load -- --ignored chs_005` runs one scenario.
- With no network in the stack, a load test cannot reach anything outside it.
- A change to the load tests is checked in CI before it merges, in minutes instead of hours.
- The latency limits in the specification are checked as written, so a miss is visible.

Negative:
- The generator and the milter share the runner's CPUs, so the latency measured includes the generator's own scheduling. If LOD-003 fails from runner noise, the specification or the runner has to change, with the measured percentiles as evidence.
- Loopback skips the network stack below TCP, so a partition on loopback does not show how a real network fails, only that the milter's timeouts and keepalive notice a silent peer.
- A pull request that changes only the library does not run the load tests. A regression shows up in the next weekly run, up to a week later.
- The short mode checks that the scenarios work, not the full durations: a slow leak can pass a five-minute LOD-001 and fail the weekly run.

## References

- [Load and chaos scenarios](../load-and-chaos.md)
- [Testing ADR 0001](0001-interop-tests-run-on-distro-mtas-in-compose-on-every-pr.md), the interoperability stack this one follows
- [tcp-server.md, Load tests](../../reference/tcp-server.md#load-tests)
- [api-specification.md, Performance characteristics](../../reference/api-specification.md#performance-characteristics)
