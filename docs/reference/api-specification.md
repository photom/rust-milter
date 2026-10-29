# API specification

The public API of the rust-milter library: its types, traits, error handling, and usage contracts.

## Design principles

1. Type safety: the Rust type system enforces protocol correctness.
2. Async first: every I/O operation is async.
3. Zero-cost abstractions: safety adds no runtime overhead.
4. Ergonomics: the API is easy to use correctly and hard to use incorrectly.
5. Composability: traits and generics give flexibility.

## Public API surface

### Exported types

| Type | Category | Description |
|------|----------|-------------|
| `Server` | Core | TCP server instance |
| `ServerConfig` | Configuration | Server configuration builder |
| `Connection` | I/O | Per-connection handle |
| `Frame` | Protocol | Milter protocol frame |
| `Command` | Protocol | Commands from MTA to milter (SMFIC_*) |
| `Reply` | Protocol | Replies from milter to MTA (SMFIR_*) |
| `Handler` | Trait | Connection handler that the user implements |
| `Error` | Error | Library error type |
| `Result<T>` | Error | Library result type alias |

### Exported traits

| Trait | Purpose | Requirements |
|-------|---------|--------------|
| `Handler` | Connection processing | async, Send, Sync, Clone |

### Re-exported types

| Type | Source | Purpose |
|------|--------|---------|
| `CancellationToken` | tokio-util | Graceful shutdown coordination |
| `Duration` | std::time | Timeout configuration |
| `SocketAddr` | std::net | Network address types |

## Type specifications

### Server

The main server type. It manages the listeners and the connections.

Lifecycle:
- `Server::new(config)` creates it.
- `server.run(handler)` starts it.
- A shutdown signal or an error stops it.

Ownership: it owns the listener sockets, spawns the connection tasks, and coordinates shutdown.

Thread safety: `Send + Sync`, so threads can share it.

Methods:
- `new(config)`: creates a server instance.
- `run(handler)`: starts the server. Consumes `self`.
- `shutdown_token()`: returns the cancellation token for shutdown.

### ServerConfig

Server configuration, built with the builder pattern. `Server::new()` consumes it. The builder validates every parameter on build.

Fields:
- `bind_addresses`: `Vec` of socket addresses
- `backlog`: listen queue size (`u32`)
- `max_connections`: connection limit (`usize`)
- `read_timeout`: read operation timeout
- `write_timeout`: write operation timeout
- `idle_timeout`: idle connection timeout
- `grace_period`: shutdown grace period
- `drain_timeout`: connection drain timeout

Defaults come from the `Default` trait.

Constraints:
- At least one bind address is required.
- Timeout values must be positive.
- Backlog must be between 1 and 65535.
- Max connections is 0 (unlimited) or a positive number.

### Connection

The I/O handle for one connection. It owns its socket exclusively and lives as long as the connection.

Buffering:
- internal read buffer (`BytesMut`)
- `BufWriter` for writes
- configurable buffer sizes

Methods:
- `read_frame()`: reads the next frame (async).
- `write_frame(&frame)`: writes a frame (async).
- `peer_addr()`: returns the peer address.
- `local_addr()`: returns the local address.

Thread safety: `Send`, so it can move between tasks. It is not `Sync`.

Errors: methods return `Result` on I/O errors.

### Frame

A Milter protocol frame.

Structure:
- `command`: `u8`, the protocol command code
- `data`: `Bytes`, the frame payload

All fields are public and immutable.

Size constraints:
- total frame size ≤ 65,535 bytes
- command: 1 byte
- data: 0 to 65,534 bytes

Methods:
- `new(command, data)`: creates a frame.
- `check(buffer)`: checks whether the buffer holds a complete frame.
- `parse(buffer)`: parses a frame from the buffer.
- `write_to(&buffer)`: serializes the frame into the buffer.

Thread safety: `Send + Sync`, because the type is immutable.

Zero copy: the payload uses `bytes::Bytes`.

### Command

Type-safe representation of the commands the MTA sends to the milter.

Variants:
- `Abort`: SMFIC_ABORT
- `Body(Bytes)`: SMFIC_BODY
- `Connect { hostname, family, port, address }`: SMFIC_CONNECT
- `Macro { stage, macros }`: SMFIC_MACRO
- `BodyEob`: SMFIC_BODYEOB
- `Helo(String)`: SMFIC_HELO
- `QuitNewConnection`: SMFIC_QUIT_NC
- `Header { name, value }`: SMFIC_HEADER
- `Mail(Vec<String>)`: SMFIC_MAIL
- `Eoh`: SMFIC_EOH
- `OptNeg { version, actions, protocol }`: SMFIC_OPTNEG
- `Quit`: SMFIC_QUIT
- `Rcpt(Vec<String>)`: SMFIC_RCPT
- `Data`: SMFIC_DATA
- `Unknown(String)`: SMFIC_UNKNOWN

Conversion: `from_frame(frame)` parses a `Frame` into a `Command` and validates the command code and the data format.

Errors: returns `Error::Protocol` on invalid data.

Thread safety: `Send + Sync`.

### Reply

Type-safe representation of the replies the milter sends to the MTA.

Variants:
- `Continue`: SMFIR_CONTINUE
- `Accept`: SMFIR_ACCEPT
- `Reject`: SMFIR_REJECT
- `Discard`: SMFIR_DISCARD
- `TempFail`: SMFIR_TEMPFAIL
- `ReplyCode(String)`: SMFIR_REPLYCODE
- `Progress`: SMFIR_PROGRESS
- `Quarantine(String)`: SMFIR_QUARANTINE
- `Skip`: SMFIR_SKIP
- `AddHeader { name, value }`: SMFIR_ADDHEADER
- `InsHeader { index, name, value }`: SMFIR_INSHEADER
- `ChangeHeader { index, name, value }`: SMFIR_CHGHEADER
- `AddRecipient(String)`: SMFIR_ADDRCPT
- `DelRecipient(String)`: SMFIR_DELRCPT
- `AddRecipientPar { rcpt, args }`: SMFIR_ADDRCPT_PAR
- `ChangeFrom { from, args }`: SMFIR_CHGFROM
- `ReplaceBody(Bytes)`: SMFIR_REPLBODY
- `OptNeg { version, actions, protocol }`: SMFIR_OPTNEG (response)

Conversion: `to_frame()` converts a `Reply` into a `Frame`.

Thread safety: `Send + Sync`.

Validation: header names and values are checked against the protocol rules.

## Handler trait

The trait the user implements to process connections.

Definition:
- trait name: `Handler`
- method: `handle`
- async: yes
- parameters: `&self, Connection, CancellationToken`
- return type: `Result<()>`

Trait bounds:
- `Send`: the handler can move across threads.
- `Sync`: threads can share it.
- `Clone`: the server clones it for each connection.
- `'static`: it holds no borrowed references.

Contract:
- The server calls `handle` once per connection.
- The handler receives exclusive ownership of the `Connection`.
- The handler must respect the `CancellationToken` for shutdown.
- A returned error closes the connection.
- The handler must implement the full protocol exchange.

Lifecycle: the server clones the handler for each new connection, runs it in a separate task per connection, and drops it when the connection closes.

Concurrency:
- Multiple handlers run concurrently.
- Each handler has exclusive use of its `Connection`.
- Handlers share no state unless the implementation adds it on purpose.

Shutdown:
- The handler receives a `CancellationToken`.
- It must check the token periodically.
- It should finish the current operation cleanly.
- It returns once it detects cancellation.

## Error handling

### Error type

One error type, an enum, covers all library operations, with variants for each error category.

Categories:
- protocol errors: malformed frames, invalid commands
- I/O errors: connection failures, timeouts
- configuration errors: invalid parameters
- resource errors: limits exceeded

Variants:
- `Io(io::Error)`: I/O error
- `Protocol(String)`: protocol violation
- `Timeout`: operation timed out
- `ConnectionClosed`: the connection closed unexpectedly
- `FrameTooLarge`: the frame exceeds the size limit
- `InvalidConfig(String)`: configuration error
- `ResourceLimitExceeded`: a limit was exceeded

Implemented traits:
- `std::error::Error`
- `Display`, for human-readable messages
- `Debug`
- `Send + Sync`

Context:
- Errors carry context about where they happened.
- The error chain preserves the source error.
- Errors integrate with `tracing` for logging.

### Result type alias

Definition: `type Result<T> = std::result::Result<T, Error>`

Every fallible operation returns `Result<T>`. Propagate errors with `?`.

## Configuration API

### ServerConfig fields

Required:
- `bind_addresses`: at least one address

Optional, with defaults:
- `backlog`: 1024
- `max_connections`: 10000
- `read_timeout`: 30 seconds
- `write_timeout`: 30 seconds
- `idle_timeout`: 300 seconds
- `grace_period`: 5 seconds
- `drain_timeout`: 30 seconds

Builder:
- chainable setters for every field
- validation on build
- `build` returns `Result<ServerConfig>`

Validation rules:
- Timeouts must be positive.
- Backlog is between 1 and 65535.
- Max connections ≥ 0.
- At least one bind address.

## Async patterns

### Async and sync functions

Async (all I/O):
- `Connection::read_frame()`
- `Connection::write_frame()`
- `Handler::handle()`
- `Server::run()`

Sync:
- `Frame::parse()`
- `Command::from_frame()`
- `Reply::to_frame()`
- the configuration builders

### Cancellation safety

- `Connection::read_frame()` is safe to drop in the middle of an await.
- `Connection::write_frame()` is buffered and safe to drop.

No operation is cancellation-unsafe; the library designs every operation for cancellation safety.

Pattern: use `tokio::select!` with the `CancellationToken`.

## Type safety guarantees

### Protocol correctness

Type system enforcement:
- Commands are validated when they are parsed.
- Invalid command codes produce errors.
- Data formats are checked during conversion.
- Reply types match the protocol requirements.

State machine:
- The connection tracks its state internally.
- Invalid state transitions are rejected.
- The protocol sequence is validated.

### Memory safety

Rust guarantees no null pointers, no use-after-free, no data races, and no buffer overflows.

Zero-copy buffers:
- `bytes::Bytes` provides safe shared buffers with reference counting.
- The public API contains no unsafe code.

## Versioning and stability

### Semantic versioning

Version format: MAJOR.MINOR.PATCH

- Breaking changes bump MAJOR.
- New features bump MINOR.
- Bug fixes bump PATCH.

### Stability guarantees

From v1.0, these are stable:
- public types and traits
- method signatures
- error variants
- configuration fields

In v0.x, any API can change, including breaking changes.

### Deprecation policy

1. Mark the item with `#[deprecated]`.
2. Provide a migration path.
3. Remove it in the next MAJOR version.
4. Record it in the CHANGELOG.

## Thread safety

| Type | Send | Sync |
|------|------|------|
| `Server` | yes | yes |
| `Connection` | yes | no (exclusive I/O ownership) |
| `Frame` | yes | yes |
| `Command` | yes | yes |
| `Reply` | yes | yes |
| `Error` | yes | yes |

A `Handler` must be `Send + Sync + Clone`:
- `Send` so it can move to worker threads
- `Sync` so threads can share references to it
- `Clone` so each connection gets its own instance

## Performance characteristics

### Copying

Zero copy:
- frame data (`bytes::Bytes`)
- body chunks
- header values

Minimal copying:
- string conversions, where required
- configuration cloning

### Allocation

Per connection:
- read buffer (4KB default)
- write buffer (4KB default)
- frame storage, as needed

Shared:
- configuration (wrapped in `Arc`)
- handler (cloned per connection)

### Latency targets

| Percentile | Target per frame |
|------------|------------------|
| P50 | < 100μs |
| P99 | < 1ms |
| P99.9 | < 10ms |

## Usage constraints

### Runtime

The library requires the Tokio multi-threaded runtime and does not work with other runtimes. The application must initialize the runtime.

### Resources

Minimum:
- Rust 1.70+ (edition 2021)
- Linux, macOS, or BSD
- file descriptors: max_connections + 10

Recommended:
- 4+ CPU cores for production
- 1GB+ RAM for 10K connections
- `ulimit -n` ≥ 10010

## Possible future additions

- TLS support trait
- custom metrics trait
- authentication trait
- connection pool API

Backward compatibility:
- New traits can be added without breaking existing code.
- Traits get default implementations where possible.
- Optional functionality goes behind feature flags.

## References

- [Library architecture](architecture.md)
- [Milter protocol specification](milter-protocol.md)
- [TCP server specification](tcp-server.md)
