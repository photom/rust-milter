# Test environment specification

Status: design, not implemented.

The fixed TCP ports that rust-milter's tests listen on, how tests share them, and the other rules a test environment must meet. Why the tests use these ports and leases is explained in [Test ports](../explanation/test-ports.md). How to run the tests is in [Run the tests](../how-to/run-the-tests.md); how to write a test that listens on a port is in [Write a network test](../how-to/write-a-network-test.md).

## Contents

- [Test ports](#test-ports)
- [Port leases](#port-leases)
- [Environment variables](#environment-variables)
- [Rules for tests](#rules-for-tests)
- [Documentation check](#documentation-check)
- [Interoperability environment](#interoperability-environment)
- [CI](#ci)
- [Toolchain](#toolchain)

## Test ports

| Item | Value |
|------|-------|
| Port range | 61000 to 61009 (10 ports) |
| Addresses | `127.0.0.1` and `::1` |
| Protocol | TCP |

Tests listen only on loopback addresses and only on these ports. No test binds port 0, and no test uses a port outside the range.

A test that listens on IPv4 and IPv6 at the same time uses the same port number on both loopback addresses. One lease covers both.

## Port leases

A test that needs a port takes a lease on it, and holds the lease until the test ends. Leases make 10 ports enough for a test suite that runs many tests in parallel.

| Property | Behavior |
|----------|----------|
| Lock | An exclusive file lock on `<temp dir>/rust-milter-test-ports/<port>.lock`, where `<temp dir>` is the system temporary directory |
| Scope | Every test process on the machine, including other checkouts of the repository and `cargo-nextest`, which runs each test in its own process |
| Order | The lease tries the ports from the lowest up and takes the first one whose lock is free |
| Free check | After locking, the lease binds the port on `127.0.0.1` once and closes it at once. If the bind fails, another program uses the port; the lease releases the lock and tries the next port |
| Waiting | If all 10 ports are leased, the lease waits and retries every 100 ms, for up to 60 seconds. It then fails the test with a message that names the busy ports |
| Release | Dropping the lease releases the lock. If the test process crashes, the operating system releases the lock when the process exits, so no stale lock remains |
| Several ports | A test can take up to 3 leases at once, for example to test a partial bind failure |

A lease does not keep the port bound. The server under test binds it after the test has taken the lease.

## Environment variables

| Variable | Default | Effect |
|----------|---------|--------|
| `RUST_MILTER_TEST_PORT_BASE` | 61000 | First port of the range |
| `RUST_MILTER_TEST_PORT_COUNT` | 10 | Number of ports in the range; between 3 and 100 |
| `RUST_MILTER_TEST_IPV6` | `auto` | `auto` runs IPv6 tests if `::1` can be bound and skips them otherwise; `on` fails them if it cannot; `off` skips them |

## Rules for tests

1. Take a lease before binding a port, and keep it until the server under test has stopped.
2. Bind only the leased port, and only on `127.0.0.1` or `::1`.
3. Use the server's default `reuse_addr = true`. A test that stops a server and starts another on the same port in quick succession would otherwise fail on connections left in `TIME_WAIT`.
4. Stop servers through `ServerHandle::shutdown()` and wait for them before the lease is dropped. Only process-level signal tests send real signals.
5. A timeout test finishes in under 10 seconds. It sets the timeout under test to a short value, such as 100 ms (`timeouts.idle` has a minimum of 1 s), or runs on Tokio's paused clock (`tokio::time::pause`) wherever it does not need real network delays.
6. Tests that need IPv6 follow `RUST_MILTER_TEST_IPV6`. When IPv6 is unavailable and not required, they print a skip notice and return. The Rust test harness has no skipped result, so such a test counts as passed; the notice tells the reader it did not run.

## Documentation check

`cargo test` compiles the Rust code blocks in the tutorials and how-to guides, and CI runs it on every push and pull request.

| Item | Behavior |
|------|----------|
| Documents checked | Every Markdown file in `docs/tutorials/` and `docs/how-to/`, found automatically; a new file needs no registration |
| Code blocks checked | Blocks marked as Rust; shell, TOML, YAML, and other blocks are not compiled |
| Mechanism | The documents are included as doctests, so they remain the only copy of the examples |
| Fragments | A fragment that cannot compile alone gets hidden setup lines, which do not appear in the rendered document |
| Exceptions | A block that cannot compile at all, even with setup lines, is marked as not compiled, with the reason stated next to it |
| Failure | A block that does not compile fails the test run, and with it the CI build |

## Interoperability environment

The interoperability scenarios (`INT`) run in a Docker Compose stack defined in `tests/interop/compose.yaml`, on every pull request ([testing ADR 0001](../testing/adr/0001-interop-tests-run-on-distro-mtas-in-compose-on-every-pr.md)).

```mermaid
flowchart LR
    subgraph net["Compose network interop (internal, no route outside)"]
        runner["runner<br/>INT tests, milter under test on port 8891,<br/>SMTP sink on port 2525"]
        postfix["postfix<br/>(Debian package)"]
        sendmail["sendmail<br/>(Debian package)"]
        short["sendmail-short<br/>(short milter timeouts)"]
    end
    runner -->|"SMTP"| postfix
    runner -->|"SMTP"| sendmail
    runner -->|"SMTP"| short
    postfix -->|"milter, and mail to void.invalid"| runner
    sendmail -->|"milter, and mail to void.invalid"| runner
    short -->|"milter, and mail to void.invalid"| runner
```

| Service | Image | Role |
|---------|-------|------|
| `runner` | `rust:1.89`, with the repository mounted | Runs the `INT` tests one at a time. Each test starts its own milter on port 8891 with the scenario's handler, sends mail through an MTA, and stops the milter. The runner also holds an SMTP sink on port 2525 that keeps every delivered message for the test to inspect |
| `postfix` | The pinned Debian release with the `postfix` package | MTA under test |
| `sendmail` | The same Debian release with the `sendmail` package | MTA under test |
| `sendmail-short` | The `sendmail` image with the filter timeouts `T=E:3s;R:2s` | MTA for the Sendmail timeout scenario |

All MTA images use one Debian release, pinned by tag. Each MTA container prints its version when it starts, and CI keeps the Compose logs, so a failure can be traced to a package upgrade.

Postfix listens on several ports. Each port is an `smtpd` service in `master.cf` that overrides the settings in the table:

| Port | Overrides | Used by |
|------|-----------|---------|
| 25 | none | most scenarios |
| 2602, 2603, 2604 | `milter_protocol` = 2, 3, 4 | INT-002 |
| 2625 | `milter_content_timeout = 2s` | INT-040 |
| 2626 | `smtpd_peername_lookup = no`, so the client's hostname arrives as `[<address>]` | INT-031 |

Settings common to both MTAs:

| Setting | Value |
|---------|-------|
| Milter | `inet:8891@runner` for Sendmail; `inet:runner:8891` in Postfix's `smtpd_milters` |
| Milter protocol | 6, except on the Postfix ports above |
| Failure policy | tempfail: Postfix `milter_default_action = tempfail`, Sendmail filter flag `F=T` |
| Test addresses | Only the domain `void.invalid`. Both MTAs route it to the sink on `runner` port 2525: Postfix through `transport_maps`, Sendmail through its mailertable. Sendmail accepts the unresolvable domain (`FEATURE(accept_unresolvable_domains)`) |
| Relaying | Both MTAs accept mail for `void.invalid` from the Compose network only |

Rules:
1. The Compose network is internal. No container in it can reach the internet or the host's network, so test mail cannot leave even if an MTA routes it wrongly.
2. Every email address in a test uses the domain `void.invalid`, which can never resolve.
3. The `INT` tests run one at a time (`--test-threads=1`), because every MTA points at the one milter address `runner:8891`.
4. Inside the stack the milter listens on the runner's network address, not on loopback. The loopback rule in [Rules for tests](#rules-for-tests) applies to `cargo test` only.
5. The `INT` tests live in their own crate, `tests/interop`, and are `#[ignore]`d so that `cargo test` outside the stack skips them.
6. A timeout scenario finishes in under 10 seconds, as in `cargo test`.

How to run the stack is in [Run the tests](../how-to/run-the-tests.md#run-the-interoperability-tests).

## CI

GitHub Actions runs these jobs on every push to `main` and on every pull request. The tools are chosen in [testing ADR 0002](../testing/adr/0002-ci-scans-with-cargo-deny-clippy-and-codeql.md).

| Job | Workflow | Checks |
|-----|----------|--------|
| Format | `ci.yml` | `cargo fmt --check` |
| Clippy | `ci.yml` | Clippy with warnings as errors, with all features and with none |
| Test | `ci.yml` | `cargo test` on Linux and macOS, with all features and with none, and `RUST_MILTER_TEST_IPV6=on` |
| MSRV | `ci.yml` | The library builds with Rust 1.85, with all features and with none |
| Rustdoc | `ci.yml` | The API documentation builds without warnings |
| Dependencies | `ci.yml` | cargo-deny: known vulnerabilities, yanked crates, licenses, and sources, as set in `deny.toml` |
| Interoperability | `ci.yml` | The `INT` tests in the [interoperability environment](#interoperability-environment) |
| CodeQL | `codeql.yml` | CodeQL's Rust analysis; also runs weekly |

Every action is pinned to a full commit SHA. Dependabot proposes updates to Cargo dependencies, the pinned actions, and the interop base images every week (`.github/dependabot.yml`).

## Toolchain

The library's minimum supported Rust version stays 1.85. The test support code locks files with `std::fs::File::try_lock`, stable since Rust 1.89, so running the tests needs Rust 1.89 or later. `rust-toolchain.toml` selects Rust 1.89 for work in the repository. The MSRV check in CI builds the library without its tests.
