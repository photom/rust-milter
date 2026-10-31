# Test ports

Why rust-milter's tests listen on fixed ports between 61000 and 61009, and why they lease those ports before binding them. The rules are in [test-environment.md](../reference/test-environment.md).

## Why fixed ports

A common way to avoid port conflicts in tests is to bind port 0 and let the operating system pick a free port. rust-milter's tests use fixed ports instead. A fixed range is predictable: a developer who sees a failing test knows which ports to inspect, a firewall or container setup can open exactly those ports, and the configuration rules stay the same in tests as in production, where port 0 is not a valid listen address.

## Why 61000 to 61009

The range has to avoid two kinds of collision.

The first is with registered services. Ports from 49152 to 65535 are the IANA dynamic range, which no service registers, so no daemon on a developer machine expects them.

The second is subtler. When a program opens an outgoing connection, the operating system picks a local source port from its ephemeral range. On Linux that range is 32768 to 60999 by default. A test port inside it could be taken at any moment by an unrelated outgoing connection, for example a browser or a package manager, and the test would fail with "address in use" for no reason anyone could reproduce. Starting at 61000 keeps the test ports above Linux's range while staying inside IANA's.

macOS and the BSDs assign outgoing ports from 49152 to 65535, so there the collision is unlikely but possible. That is why the range can be moved with an environment variable, and why a lease checks that the port is free before handing it out.

## Why leases

Ten ports are far fewer than the number of tests that listen on a port, and test runners execute tests in parallel. Two tests that bind the same port at the same time would both fail, at random.

A lease turns the ports into a shared pool. A test takes a free port, holds it while its server runs, and gives it back. When all ports are taken, the next test waits. The suite runs a little slower under heavy parallelism, but it never flakes over ports.

## Why file locks

The lease must work across processes, not only across threads. `cargo-nextest` runs each test in its own process, and two checkouts of the repository can run their test suites at the same time on one machine. An in-memory pool cannot coordinate those.

An exclusive file lock per port in the system temporary directory can. The operating system enforces it across every process, and it releases the lock when the process exits, even after a crash. A test that dies halfway leaves no stale reservation behind.

The lease does not keep the port bound while the test runs, because the server under test has to bind it. Binding the port briefly before handing it out catches the case where some other program, outside the test suite, is already using it.
