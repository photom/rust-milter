# API Specification

## Overview

This document specifies the public API surface of the rust-milter library, including types, traits, error handling, and usage contracts.

## API Design Principles

1. **Type Safety**: Leverage Rust's type system for protocol correctness
2. **Async-First**: All I/O operations are async
3. **Zero-Cost Abstractions**: No runtime overhead for safety
4. **Ergonomic**: Easy to use correctly, hard to use incorrectly
5. **Composable**: Traits and generics for flexibility

## Public API Surface

### Exported Types

| Type | Category | Description |
|------|----------|-------------|
| `Server` | Core | TCP server instance |
| `ServerConfig` | Configuration | Server configuration builder |
| `Connection` | I/O | Per-connection handle |
| `Frame` | Protocol | Milter protocol frame |
| `Command` | Protocol | MTA→Milter commands (SMFIC_*) |
| `Reply` | Protocol | Milter→MTA replies (SMFIR_*) |
| `Handler` | Trait | User-implementable connection handler |
| `Error` | Error | Library error type |
| `Result<T>` | Error | Library result type alias |

### Exported Traits

| Trait | Purpose | Requirements |
|-------|---------|--------------|
| `Handler` | Connection processing | async, Send, Sync, Clone |

### Re-exported Types

| Type | Source | Purpose |
|------|--------|---------|
| `CancellationToken` | tokio-util | Graceful shutdown coordination |
| `Duration` | std::time | Timeout configuration |
| `SocketAddr` | std::net | Network address types |

## Type Specifications

### Server Type

**Purpose:** Main server type that manages listeners and connections

**Lifecycle:**
- Created via `Server::new(config)`
- Started via `server.run(handler)`
- Stopped via shutdown signal or error

**Ownership:**
- Owns listener sockets
- Spawns connection tasks
- Coordinates shutdown

**Thread Safety:** Send + Sync (can be shared across threads)

**Methods:**
- `new(config)` - Create server instance
- `run(handler)` - Start server (consumes self)
- `shutdown_token()` - Get cancellation token for shutdown

### ServerConfig Type

**Purpose:** Server configuration with builder pattern

**Mutability:** Consumed by Server::new()

**Validation:** All parameters validated on build

**Fields:**
- `bind_addresses` - Vec of socket addresses
- `backlog` - Listen queue size (u32)
- `max_connections` - Connection limit (usize)
- `read_timeout` - Read operation timeout
- `write_timeout` - Write operation timeout
- `idle_timeout` - Idle connection timeout
- `grace_period` - Shutdown grace period
- `drain_timeout` - Connection drain timeout

**Defaults:** Available via `Default` trait

**Constraints:**
- At least one bind address required
- Timeout values must be positive
- Backlog must be 1-65535
- Max connections 0 (unlimited) or positive

### Connection Type

**Purpose:** Per-connection I/O handle

**Ownership:** Exclusive ownership of socket

**Lifetime:** Lives for duration of connection

**Buffering:**
- Internal read buffer (BytesMut)
- BufWriter for writes
- Configurable buffer sizes

**Methods:**
- `read_frame()` - Read next frame (async)
- `write_frame(&frame)` - Write frame (async)
- `peer_addr()` - Get peer address
- `local_addr()` - Get local address

**Thread Safety:** Send (can be moved between tasks), NOT Sync

**Error Handling:** Returns Result on I/O errors

### Frame Type

**Purpose:** Milter protocol frame representation

**Structure:**
- `command`: u8 (protocol command code)
- `data`: Bytes (frame payload)

**Immutability:** All fields public and immutable

**Size Constraints:**
- Total frame size ≤ 65,535 bytes
- Command: 1 byte
- Data: 0-65,534 bytes

**Methods:**
- `new(command, data)` - Create frame
- `check(buffer)` - Validate frame completeness
- `parse(buffer)` - Parse frame from buffer
- `write_to(&buffer)` - Serialize to buffer

**Thread Safety:** Send + Sync (immutable)

**Zero-Copy:** Uses bytes::Bytes for data

### Command Enum

**Purpose:** Type-safe representation of MTA→Milter commands

**Variants:**
- `Abort` - SMFIC_ABORT
- `Body(Bytes)` - SMFIC_BODY
- `Connect { hostname, family, port, address }` - SMFIC_CONNECT
- `Macro { stage, macros }` - SMFIC_MACRO
- `BodyEob` - SMFIC_BODYEOB
- `Helo(String)` - SMFIC_HELO
- `QuitNewConnection` - SMFIC_QUIT_NC
- `Header { name, value }` - SMFIC_HEADER
- `Mail(Vec<String>)` - SMFIC_MAIL
- `Eoh` - SMFIC_EOH
- `OptNeg { version, actions, protocol }` - SMFIC_OPTNEG
- `Quit` - SMFIC_QUIT
- `Rcpt(Vec<String>)` - SMFIC_RCPT
- `Data` - SMFIC_DATA
- `Unknown(String)` - SMFIC_UNKNOWN

**Conversion:**
- `from_frame(frame)` - Parse Frame into Command
- Validates command code and data format

**Error Handling:** Returns Error::Protocol on invalid data

**Thread Safety:** Send + Sync

### Reply Enum

**Purpose:** Type-safe representation of Milter→MTA replies

**Variants:**
- `Continue` - SMFIR_CONTINUE
- `Accept` - SMFIR_ACCEPT
- `Reject` - SMFIR_REJECT
- `Discard` - SMFIR_DISCARD
- `TempFail` - SMFIR_TEMPFAIL
- `ReplyCode(String)` - SMFIR_REPLYCODE
- `Progress` - SMFIR_PROGRESS
- `Quarantine(String)` - SMFIR_QUARANTINE
- `Skip` - SMFIR_SKIP
- `AddHeader { name, value }` - SMFIR_ADDHEADER
- `InsHeader { index, name, value }` - SMFIR_INSHEADER
- `ChangeHeader { index, name, value }` - SMFIR_CHGHEADER
- `AddRecipient(String)` - SMFIR_ADDRCPT
- `DelRecipient(String)` - SMFIR_DELRCPT
- `AddRecipientPar { rcpt, args }` - SMFIR_ADDRCPT_PAR
- `ChangeFrom { from, args }` - SMFIR_CHGFROM
- `ReplaceBody(Bytes)` - SMFIR_REPLBODY
- `OptNeg { version, actions, protocol }` - SMFIR_OPTNEG (response)

**Conversion:**
- `to_frame()` - Convert Reply to Frame

**Thread Safety:** Send + Sync

**Validation:** Header names/values validated for protocol compliance

## Handler Trait Specification

**Purpose:** User-implementable trait for connection processing

**Definition:**
- Trait name: `Handler`
- Method: `handle`
- Async: Yes
- Parameters: `&self, Connection, CancellationToken`
- Return: `Result<()>`

**Trait Bounds:**
- `Send` - Can be sent across threads
- `Sync` - Can be shared across threads
- `Clone` - Can be cloned for each connection
- `'static` - No borrowed references

**Contract:**
- Called once per connection
- Receives exclusive Connection ownership
- Must respect CancellationToken for shutdown
- Errors returned close connection
- Must implement full protocol exchange

**Lifecycle:**
- Cloned for each new connection
- Executed in separate task per connection
- Dropped when connection closes

**Concurrency:**
- Multiple handlers run concurrently
- Each handler has exclusive Connection
- No shared state between handlers (unless explicitly designed)

**Shutdown Handling:**
- Handler receives CancellationToken
- Must check token periodically
- Should complete current operation gracefully
- Return when cancellation detected

## Error Handling Specification

### Error Type

**Purpose:** Unified error type for all library operations

**Design:** Enum with variants for each error category

**Error Categories:**
- Protocol errors (malformed frames, invalid commands)
- I/O errors (connection failures, timeouts)
- Configuration errors (invalid parameters)
- Resource errors (limits exceeded)

**Error Variants:**
- `Io(io::Error)` - I/O errors
- `Protocol(String)` - Protocol violations
- `Timeout` - Operation timeout
- `ConnectionClosed` - Unexpected connection close
- `FrameTooLarge` - Frame exceeds size limit
- `InvalidConfig(String)` - Configuration error
- `ResourceLimitExceeded` - Limit exceeded

**Error Traits:**
- `std::error::Error` - Standard error trait
- `Display` - Human-readable error messages
- `Debug` - Debug formatting
- `Send + Sync` - Thread-safe

**Error Context:**
- Errors include contextual information
- Source errors preserved via error chain
- Integration with tracing for logging

### Result Type Alias

**Definition:** `type Result<T> = std::result::Result<T, Error>`

**Usage:** All fallible operations return `Result<T>`

**Error Propagation:** Use `?` operator throughout

## Configuration API

### ServerConfig Fields

**Required Fields:**
- `bind_addresses` - At least one address required

**Optional Fields (with defaults):**
- `backlog` - Default: 1024
- `max_connections` - Default: 10000
- `read_timeout` - Default: 30 seconds
- `write_timeout` - Default: 30 seconds
- `idle_timeout` - Default: 300 seconds
- `grace_period` - Default: 5 seconds
- `drain_timeout` - Default: 30 seconds

**Builder Methods:**
- Chainable setters for all fields
- Validation on build
- Returns Result<ServerConfig>

**Validation Rules:**
- Timeouts must be positive
- Backlog 1-65535
- Max connections ≥ 0
- At least one bind address

## Async/Await Patterns

### Async Functions

**All I/O Operations:**
- `Connection::read_frame()` - Async
- `Connection::write_frame()` - Async
- `Handler::handle()` - Async
- `Server::run()` - Async

**Sync Operations:**
- `Frame::parse()` - Sync
- `Command::from_frame()` - Sync
- `Reply::to_frame()` - Sync
- Configuration builders - Sync

### Cancellation Safety

**Safe Operations:**
- `Connection::read_frame()` - Safe to drop mid-await
- `Connection::write_frame()` - Buffered, safe to drop

**Unsafe Operations:**
- None - all operations designed for cancellation safety

**Pattern:** Use `tokio::select!` with CancellationToken

## Type Safety Guarantees

### Protocol Correctness

**Type System Enforcement:**
- Commands are validated at parse time
- Invalid command codes generate errors
- Data formats checked during conversion
- Reply types match protocol requirements

**State Machine Safety:**
- Connection state tracked internally
- Invalid state transitions prevented
- Protocol sequence validated

### Memory Safety

**Rust Guarantees:**
- No null pointers
- No use-after-free
- No data races
- No buffer overflows

**Zero-Copy Safety:**
- bytes::Bytes provides safe shared buffers
- Reference counting for memory management
- No unsafe code in public API

## Versioning and Stability

### Semantic Versioning

**Version Format:** MAJOR.MINOR.PATCH

**Stability:**
- Breaking changes: MAJOR version bump
- New features: MINOR version bump
- Bug fixes: PATCH version bump

### API Stability Guarantees

**Stable (v1.0+):**
- Public types and traits
- Method signatures
- Error variants
- Configuration fields

**Unstable (v0.x):**
- All APIs subject to change
- Breaking changes allowed

### Deprecation Policy

**Process:**
- Mark deprecated with `#[deprecated]`
- Provide migration path
- Remove in next MAJOR version
- Document in CHANGELOG

## Thread Safety

### Send Types

**Can be sent across threads:**
- Server
- Connection
- Frame
- Command
- Reply
- Error

### Sync Types

**Can be shared across threads:**
- Server
- Frame
- Command
- Reply
- Error

**NOT Sync:**
- Connection (exclusive I/O ownership)

### Handler Requirements

**Must be:** Send + Sync + Clone
- Send: Can move to worker threads
- Sync: Can share references
- Clone: Per-connection instance

## Performance Characteristics

### Zero-Copy Operations

**Zero-Copy:**
- Frame data (bytes::Bytes)
- Body chunks
- Header values

**Minimal Copy:**
- String conversions (when required)
- Configuration cloning

### Allocation Patterns

**Per-Connection Allocations:**
- Read buffer (4KB default)
- Write buffer (4KB default)
- Frame storage (as needed)

**Shared Allocations:**
- Configuration (Arc-wrapped)
- Handler (cloned per connection)

### Latency Targets

**P50:** < 100μs per frame
**P99:** < 1ms per frame
**P99.9:** < 10ms per frame

## Usage Constraints

### Required Runtime

**Tokio Multi-threaded Runtime:**
- Library requires Tokio runtime
- Will not work with other runtimes
- Application must initialize runtime

### Resource Requirements

**Minimum:**
- Rust 1.70+ (edition 2021)
- Linux, macOS, or BSD
- File descriptors: max_connections + 10

**Recommended:**
- 4+ CPU cores for production
- 1GB+ RAM for 10K connections
- ulimit -n ≥ 10010

## Future API Considerations

**Potential Additions:**
- TLS support trait
- Custom metrics trait
- Authentication trait
- Connection pool API

**Backward Compatibility:**
- New traits added without breaking existing code
- Default implementations where possible
- Feature flags for optional functionality

## References

- [Library Architecture](architecture.md)
- [Milter Protocol Specification](milter-protocol.md)
- [TCP Server Specification](tcp-server.md)
