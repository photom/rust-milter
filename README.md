# rust-milter

An asynchronous Milter protocol library for Rust, built for high throughput.

Status: in development, and not yet on crates.io. The library implements the documented design. The load and chaos tests and the milter client sample are not written yet.

## Overview

rust-milter implements the Milter (mail filter) protocol that Sendmail and Postfix use, in pure Rust. You build a mail filter by writing callbacks, one per protocol event, as with Sendmail's libmilter, without depending on libmilter's C library.

## Features

- Callback API: the library runs the protocol, and your handler answers each event with a decision, following libmilter's model.
- MTA compatibility: works with Sendmail and Postfix, protocol versions 2 to 6.
- Async first: runs on Tokio and handles connections concurrently, designed for 10,000+ concurrent connections (C10K).
- Graceful shutdown: drains open SMTP sessions before it stops.
- Configuration reload on `SIGHUP`, and configuration from environment variables for Docker and Kubernetes.
- Optional OpenTelemetry metrics, and `tracing` spans that export as OpenTelemetry traces.
- Pure Rust: no C dependencies and no FFI overhead.
- TCP only (IPv4 and IPv6); Unix domain sockets are not supported.

## Usage

The [getting started tutorial](docs/tutorials/getting-started.md) adds rust-milter to a project and builds a first milter.

## Documentation

The documentation follows the [Diátaxis](https://diataxis.fr/) framework, which splits it into four types. Example code appears only in the tutorials and the how-to guides.

### [Tutorials](docs/tutorials/) (learning)

Step-by-step lessons that teach the basics through guided practice. Start here if you are new to rust-milter or to the Milter protocol.

- [Getting started: build your first milter](docs/tutorials/getting-started.md)

### [How-to guides](docs/how-to/) (tasks)

Practical steps for solving a specific problem. Use them when you have a particular task to finish.

- [Reload the configuration without a restart](docs/how-to/reload-the-configuration.md)
- [Export metrics with OpenTelemetry](docs/how-to/export-metrics-with-opentelemetry.md)
- [Export traces with OpenTelemetry](docs/how-to/export-traces-with-opentelemetry.md)
- [Record the SMTP client's address](docs/how-to/record-the-smtp-client-address.md)
- [Make a milter work with both Sendmail and Postfix](docs/how-to/make-a-milter-work-with-sendmail-and-postfix.md)
- [Deploy a milter in production](docs/how-to/deploy-a-milter-in-production.md)
- [Run a milter in Docker or Kubernetes](docs/how-to/run-a-milter-in-docker-or-kubernetes.md)
- [Run the tests](docs/how-to/run-the-tests.md)
- [Write a network test](docs/how-to/write-a-network-test.md)

### [Reference](docs/reference/) (information)

Accurate and complete technical specifications. Use them to look up specific details.

- [Milter protocol specification](docs/reference/milter-protocol.md)
- [TCP server specification](docs/reference/tcp-server.md)
- [API specification](docs/reference/api-specification.md)
- [Configuration specification](docs/reference/configuration.md)
- [Milter client sample](docs/reference/milter-client.md) (design reference for the sample in `examples/`)
- [Configuration hot reload](docs/reference/hot-reload.md)
- [Telemetry (OpenTelemetry)](docs/reference/telemetry.md)
- [Test environment](docs/reference/test-environment.md)
- API reference: run `cargo doc --open`

### [Explanation](docs/explanation/) (understanding)

Background on the concepts, design decisions, and architecture.

- [Library architecture](docs/explanation/architecture.md)
- [Connection reuse and session isolation](docs/explanation/connection-reuse.md)
- [Configuration snapshots and reload](docs/explanation/configuration-and-reload.md)
- [Telemetry principles](docs/explanation/telemetry-principles.md)
- [Test ports](docs/explanation/test-ports.md)

## Test scenarios

The tests are listed as [test scenarios](docs/testing/), one table row per behavior, each linked to the specification rule it checks. Each scenario is written before its test.

## Architecture decisions

Decisions and their reasons are recorded as [architecture decision records](docs/adr/), outside the four documentation types above. Decisions about testing are kept apart, in [testing decision records](docs/testing/adr/).

## License

MIT
