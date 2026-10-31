# Run the tests

Status: the test suite is not written yet. This guide follows the design in [test-environment.md](../reference/test-environment.md).

## Run the whole suite

1. Check the toolchain. The tests need Rust 1.89 or later:

   ```sh
   rustc --version
   ```

2. Run the tests:

   ```sh
   cargo test
   ```

Tests that listen on a port take turns on the 10 test ports, 61000 to 61009. With more tests than ports, some tests wait for a free port.

## Run one test or one group

Pass part of the test name:

```sh
cargo test shutdown
cargo test --test server shutdown_drains_open_connections
```

## Run the suite with cargo-nextest

`cargo-nextest` runs each test in its own process. The port leases work across processes, so no extra setting is needed:

```sh
cargo nextest run
```

## Skip or require the IPv6 tests

Skip them on a machine without IPv6:

```sh
RUST_MILTER_TEST_IPV6=off cargo test
```

Make them fail, not skip, when IPv6 is missing, for example in CI where IPv6 must work:

```sh
RUST_MILTER_TEST_IPV6=on cargo test
```

## Move the test ports

If another program on your machine uses ports 61000 to 61009, move the range:

```sh
RUST_MILTER_TEST_PORT_BASE=62000 cargo test
```

On macOS and the BSDs the operating system may assign these ports to outgoing connections. If tests fail with "address in use" there, move the range above the busy ports the same way.

## When a test reports "no free test port"

The message lists the ports that were busy. Find what holds them:

```sh
ss -ltnp 'sport >= :61000 and sport <= :61009'
```

On macOS:

```sh
lsof -nP -iTCP:61000-61009 -sTCP:LISTEN
```

If another test run is using them, wait for it to finish. If another program is using them, stop it or move the range.

Locks from a crashed test run disappear by themselves: the operating system releases them when the process exits. The lock files in the temporary directory can stay; they are harmless and reused.
