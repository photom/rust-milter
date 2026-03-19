# rust-milter

A high-performance, asynchronous Milter protocol library for Rust.

## Overview

**rust-milter** is a pure Rust implementation of the Milter (Mail Filter) protocol used by Sendmail and Postfix MTAs. It provides a type-safe, async-first API for building high-performance mail filters without dependencies on libmilter's C library.

## Features

- **Async-First Design**: Built on Tokio for high-performance concurrent operations
- **Type-Safe Protocol**: Strongly-typed frame parsing and command/reply handling
- **MTA Compatibility**: Supports both Sendmail and Postfix with protocol version 2-6
- **High Performance**: Designed to handle 10,000+ concurrent connections (C10K)
- **Graceful Shutdown**: Built-in support for clean connection termination
- **Pure Rust**: No C dependencies or FFI overhead

## Usage

Add rust-milter to your `Cargo.toml`:

```toml
[dependencies]
rust-milter = "0.1"
tokio = { version = "1", features = ["full"] }
```

## Documentation

This project follows the [Diátaxis](https://diataxis.fr/) documentation framework, organizing content into four distinct types:

### 📖 [Tutorials](docs/tutorials/) - *Learning-Oriented*

Step-by-step lessons to help you learn the fundamentals.

- **Goal**: Achieve basic competence through guided practice
- **When to use**: When you're new to rust-milter or the Milter protocol

**Status**: Coming soon

### 🔧 [How-To Guides](docs/how-to/) - *Problem-Oriented*

Practical guides to solve specific problems.

- **Goal**: Show how to solve a specific problem
- **When to use**: When you have a specific task to accomplish

**Status**: Coming soon

### 📋 [Reference](docs/reference/) - *Information-Oriented*

Complete technical specifications and API documentation.

- **Goal**: Provide accurate, complete technical information
- **When to use**: When you need to look up specific technical details

**Available:**
- [Milter Protocol Specification](docs/reference/milter-protocol.md)
- [TCP Server Specification](docs/reference/tcp-server.md)
- API Reference - Run `cargo doc --open`

### 💡 [Explanation](docs/explanation/) - *Understanding-Oriented*

In-depth explanations of concepts, design decisions, and architecture.

- **Goal**: Clarify and illuminate concepts
- **When to use**: When you want to understand the bigger picture

**Status**: Coming soon

## License

MIT
