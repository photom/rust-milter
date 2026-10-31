# rust-milter

An asynchronous Milter protocol library for Rust, built for high throughput.

## Overview

rust-milter implements the Milter (mail filter) protocol that Sendmail and Postfix use, in pure Rust. Its async, type-safe API lets you build mail filters without depending on libmilter's C library.

## Features

- Async first: runs on Tokio and handles connections concurrently.
- Typed protocol: frames, commands, and replies are parsed into Rust types.
- MTA compatibility: works with Sendmail and Postfix, protocol versions 2 to 6.
- Scale: designed for 10,000+ concurrent connections (C10K).
- Graceful shutdown: closes connections cleanly on shutdown.
- Pure Rust: no C dependencies and no FFI overhead.

## Usage

The [getting started tutorial](docs/tutorials/getting-started.md) adds rust-milter to a project and builds a first milter.

## Documentation

The documentation follows the [Diátaxis](https://diataxis.fr/) framework, which splits it into four types.

### [Tutorials](docs/tutorials/) (learning)

Step-by-step lessons that teach the basics through guided practice. Start here if you are new to rust-milter or to the Milter protocol. Example code lives only in the tutorials and the how-to guides.

Available (written against the designed API; the library is not implemented yet):
- [Getting started: build your first milter](docs/tutorials/getting-started.md)

### [How-to guides](docs/how-to/) (tasks)

Practical steps for solving a specific problem. Use them when you have a particular task to finish.

Available (written against the designed API; the library is not implemented yet):
- [Reload the configuration without a restart](docs/how-to/reload-the-configuration.md)
- [Export metrics with OpenTelemetry](docs/how-to/export-metrics-with-opentelemetry.md)
- [Call a milter with the milter client](docs/how-to/call-a-milter-with-the-client.md)
- [Make a milter work with both Sendmail and Postfix](docs/how-to/make-a-milter-work-with-sendmail-and-postfix.md)
- [Deploy a milter in production](docs/how-to/deploy-a-milter-in-production.md)
- [Run the tests](docs/how-to/run-the-tests.md)
- [Write a network test](docs/how-to/write-a-network-test.md)

### [Reference](docs/reference/) (information)

Accurate and complete technical specifications. Use them to look up specific details.

Available:
- [Milter protocol specification](docs/reference/milter-protocol.md)
- [TCP server specification](docs/reference/tcp-server.md)
- [API specification](docs/reference/api-specification.md)
- [Configuration specification](docs/reference/configuration.md)
- [Milter client and connection pool](docs/reference/milter-client.md) (design)
- [Configuration hot reload](docs/reference/hot-reload.md) (design)
- [Telemetry (OpenTelemetry)](docs/reference/telemetry.md) (design)
- [Test environment](docs/reference/test-environment.md) (design)
- API reference: run `cargo doc --open`

### [Explanation](docs/explanation/) (understanding)

Background on the concepts, design decisions, and architecture.

Available:
- [Library architecture](docs/explanation/architecture.md)
- [Connection reuse and session isolation](docs/explanation/connection-reuse.md)
- [Configuration snapshots and reload](docs/explanation/configuration-and-reload.md)
- [Telemetry principles](docs/explanation/telemetry-principles.md)
- [Test ports](docs/explanation/test-ports.md)

## Architecture decisions

Decisions and their reasons are recorded as [architecture decision records](docs/adr/), outside the four documentation types above.

## License

MIT
