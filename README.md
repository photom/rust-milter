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
- [Reload the configuration without a restart](docs/tutorials/reloading-configuration.md)
- [Export metrics with OpenTelemetry](docs/tutorials/exporting-telemetry.md)
- [Call a milter from Rust with the milter client](docs/tutorials/using-the-milter-client.md)

### [How-to guides](docs/how-to/) (tasks)

Practical steps for solving a specific problem. Use them when you have a particular task to finish.

Status: coming soon

### [Reference](docs/reference/) (information)

Accurate and complete technical specifications. Use them to look up specific details.

Available:
- [Milter protocol specification](docs/reference/milter-protocol.md)
- [TCP server specification](docs/reference/tcp-server.md)
- [Library architecture](docs/reference/architecture.md)
- [API specification](docs/reference/api-specification.md)
- [Configuration specification](docs/reference/configuration.md)
- [Milter client and connection pool](docs/reference/milter-client.md) (design)
- [Configuration hot reload](docs/reference/hot-reload.md) (design)
- [Telemetry (OpenTelemetry)](docs/reference/telemetry.md) (design)
- API reference: run `cargo doc --open`

### [Explanation](docs/explanation/) (understanding)

Background on the concepts, design decisions, and architecture.

Status: coming soon

## License

MIT
