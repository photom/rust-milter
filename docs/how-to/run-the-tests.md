# Run the tests

Status: the test suite is incomplete. This guide follows the design in [test-environment.md](../reference/test-environment.md).

## Run the whole suite

1. Install [rustup](https://rustup.rs/). The first `cargo` command in the repository installs the toolchain that `rust-toolchain.toml` selects.

2. Run the tests:

   ```sh
   cargo test
   ```

Tests that listen on a port take turns on the 10 test ports, 61000 to 61009. With more tests than ports, some tests wait for a free port.

## Run one test or one group

Test names start with their scenario ID from `docs/testing/`, so you can select by ID or by prefix:

```sh
cargo test frm_
cargo test --test protocol_engine eng_051
```

## Run the documentation check

The Rust examples in the tutorials and how-to guides compile as doctests. Run only them:

```sh
cargo test --doc
```

## Run the style check

The style check reports closures, `Fn` bounds, and function pointer types outside the exceptions of [ADR 0008](../adr/0008-rust-code-is-written-in-a-procedural-style.md):

```sh
cargo test -p rust-milter-style
```

A plain `cargo clippy` does not report the disallowed methods of `clippy.toml`. To see them, run Clippy with the lint turned on and pass its output to the `disallowed-methods` binary, which skips the calls that a `derive` attribute generates:

```sh
cargo clippy --workspace --all-targets --all-features --message-format=short -- -W clippy::disallowed_methods 2> /tmp/clippy-style.txt
cargo run -p rust-milter-style --bin disallowed-methods -- /tmp/clippy-style.txt
```

## Run the suite with cargo-nextest

`cargo-nextest` runs each test in its own process. The port leases work across processes, so no extra setting is needed:

```sh
cargo nextest run
```

`cargo-nextest` does not run doctests. Run the documentation check separately with `cargo test --doc`.

## Run the CI checks before you push

Run the same checks as CI, except the interoperability tests and CodeQL:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo clippy --workspace --all-targets --all-features --message-format=short -- -W clippy::disallowed_methods 2> /tmp/clippy-style.txt
cargo run -p rust-milter-style --bin disallowed-methods -- /tmp/clippy-style.txt
cargo +1.85 check -p rust-milter --lib --all-features
cargo deny --all-features check
```

The MSRV check needs the 1.85 toolchain (`rustup toolchain install 1.85`), and the last command needs [cargo-deny](https://github.com/EmbarkStudios/cargo-deny).

## Run the interoperability tests

The interoperability tests run rust-milter behind real Postfix and Sendmail, in a Docker Compose stack. You need Docker with Compose v2.

1. Build the stack and run every interoperability test:

   ```sh
   docker compose -f tests/interop/compose.yaml run --build --rm runner
   ```

2. Run one scenario, for example INT-011:

   ```sh
   docker compose -f tests/interop/compose.yaml run --build --rm runner \
     cargo test -p rust-milter-interop -- --ignored --test-threads=1 int_011
   ```

3. Read the MTA logs when a test fails:

   ```sh
   docker compose -f tests/interop/compose.yaml logs postfix sendmail
   ```

4. Remove the stack when you are done:

   ```sh
   docker compose -f tests/interop/compose.yaml down
   ```

The stack and its rules are described in [test-environment.md](../reference/test-environment.md#interoperability-environment).

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
