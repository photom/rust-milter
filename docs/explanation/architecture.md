# Library architecture

How the rust-milter library is put together and why: its components, their responsibilities and interactions, and the module boundaries. The public API is in [api-specification.md](../reference/api-specification.md) and every setting is in [configuration.md](../reference/configuration.md).

## Architecture principles

1. Separation of concerns: protocol handling, I/O, configuration, and business logic live in separate components.
2. Async first: all I/O is asynchronous and runs on Tokio.
3. Type safety: protocol messages are strongly typed.
4. Zero copy: `bytes::Bytes` keeps data copying to a minimum.
5. Trait-based extension: users extend the library by implementing the `Handler` trait, one callback per protocol event, as in libmilter ([ADR 0002](../adr/0002-library-runs-the-protocol-handlers-decide.md)).
6. Immutable configuration: each configuration is published once as a numbered snapshot; a reload publishes a new snapshot and never edits the old one.
7. Session isolation: no data from one SMTP session reaches the next, in memory, in logs, or in telemetry.
8. Zero-cost abstractions: the type safety and validation add no runtime overhead on the hot path.
9. Ergonomics: the API is easy to use correctly and hard to use incorrectly; for example, the library enforces the `SMFIC_QUIT_NC` rule so that handlers cannot get it wrong.
10. Composability: traits and generics let applications combine the server, their own configuration, and their handlers freely.

## Component architecture

```mermaid
flowchart TB
    subgraph control["Control plane"]
        signals["Signal handling<br/>(SIGTERM, SIGINT, SIGQUIT, SIGHUP)"]
        reload["Reload coordinator"]
        shutdown["Shutdown coordinator"]
        config["Configuration<br/>(published snapshots, ConfigWatch)"]
    end
    subgraph data["Data plane"]
        server["Server<br/>(listeners, accept loop)"]
        connection["Connection"]
        frame["Frame"]
        message["Message<br/>(Command, Reply)"]
        engine["Protocol engine<br/>(negotiation, state machine,<br/>macros, replies)"]
        handler["Handler<br/>(user callbacks)"]
    end
    telemetry["Telemetry<br/>(status counters, tracing, OpenTelemetry)"]
    error["Error types"]

    signals -->|"SIGHUP"| reload
    signals -->|"SIGTERM, SIGINT, SIGQUIT"| shutdown
    reload -->|"publishes generation N+1"| config
    reload -->|"binds and stops listeners"| server
    shutdown -->|"stop accepting, cancel"| server
    shutdown -->|"CancellationToken"| engine
    config -->|"snapshot at accept"| server
    config -->|"ConfigWatch"| handler
    server -->|"spawns task per connection"| connection
    connection -->|"reads, writes"| frame
    frame <-->|"decode, encode"| message
    message <-->|"commands, replies"| engine
    engine -->|"callbacks"| handler
    handler -->|"decisions, modifications"| engine
    server -.-> telemetry
    connection -.-> telemetry
    engine -.-> telemetry
    reload -.-> telemetry
    data -.-> error
    control -.-> error
```

The control plane decides what the server does: which listeners exist, which configuration applies, and when to stop. The data plane moves milter traffic: the connection carries frames, the protocol engine runs the conversation, and the handler only makes decisions. The control plane acts on the data plane through the listener set, the configuration snapshot each connection takes when it is accepted, the shutdown token, and the `ConfigWatch` the handler reads.

## Component specifications

### 1. Server

Responsibilities:
- bind and listen on TCP sockets (IPv4, IPv6); Unix domain sockets are not supported
- run the accept loop and spawn one task per accepted connection
- give each new connection the current configuration snapshot
- enforce `max_connections`: past the limit, accept and close at once
- track active connections for the shutdown coordinator and the status counters

Dependencies:
- Tokio `TcpListener`, `socket2` for socket options set before `bind()`
- the Configuration, Connection, and Protocol engine components

Concurrency: one accept task per listener, one task per connection.

The transport-level behavior (address formats, accept errors, backpressure, transport states) is specified in [tcp-server.md](../reference/tcp-server.md).

### 2. Connection

Responsibilities:
- own the socket of one MTA connection
- read and write frames, with buffering
- enforce the `connect`, `read`, `write`, and `idle` timeouts and the `per_connection_buffer` limit from its snapshot
- track the transport state (`CONNECTED`, `IDLE`, `ACTIVE`, `DRAINING`, `ERROR`, `CLOSED`)
- report its close reason to telemetry and to the protocol engine

Dependencies: Tokio `TcpStream`, the Frame component, `bytes::BytesMut`.

Concurrency: one connection per task, with exclusive ownership of the socket.

### 3. Frame

Responsibilities:
- parse Milter protocol frames: a 4-byte length, a 1-byte command, then the data
- reject a packet longer than `per_connection_buffer` before allocating memory for it
- handle incomplete frames by waiting for more data
- serialize frames

Dependencies: `bytes` only. Parsing is synchronous and pure.

### 4. Message

Responsibilities:
- define every protocol command (`SMFIC_*`) and reply (`SMFIR_*`) as Rust types
- convert a `Frame` into a `Command`, and a `Reply` into a `Frame`
- validate replies before encoding: reply code format and `%` escaping, header names, body chunk size

Dependencies: the Frame component and the protocol constants.

### 5. Protocol engine

The protocol engine runs the Milter conversation for each connection, as libmilter's `mi_engine` does.

Responsibilities:
- negotiate options: protocol version (2 to 6), action flags from `Handler::actions()`, protocol flags from `Handler::events()`
- track the protocol state with libmilter's transition rules, recover from a command in a wrong state where libmilter recovers, and close the connection where libmilter would leave the MTA waiting ([ADR 0001](../adr/0001-commands-in-wrong-state-recover-like-libmilter-or-close.md))
- store macros per stage and clear them when the session ends
- create the handler's `Session` value at `SMFIC_CONNECT` and its `Message` value at `SMFIC_MAIL` (or at the first command of a message when the handler leaves `MAIL` out of `events()`), and drop them when the session or message ends
- call the matching handler callback for each command, and turn its decision into the protocol reply, or into no reply where the protocol forbids one
- offer the modification methods only at end of message, and check that their actions were negotiated

Dependencies: the Connection, Message, and Handler components.

### 6. Handler

The `Handler` trait is where user business logic plugs in. It has one callback per protocol event, matching libmilter's `xxfi_*` callbacks, and each callback returns a decision. One handler value serves all connections; per-session and per-message data lives in the handler's `Session` and `Message` types, which the protocol engine creates and drops.

The trait itself is defined in [api-specification.md](../reference/api-specification.md#handler-trait).

### 7. Configuration

Responsibilities:
- validate a `ServerConfig` and the application configuration, at startup and on reload
- publish each valid configuration as an immutable `Arc` with an increasing generation number, through a `tokio::sync::watch` channel
- give readers the current snapshot without blocking

Who reads what:

| Reader | Reads | When |
|--------|-------|------|
| Accept loop | `ServerConfig` | at every accept; the snapshot goes to the new connection |
| Connection | its own snapshot | for its whole life |
| Reload coordinator | old and new `ServerConfig` | to compute listener changes and the report |
| Shutdown coordinator | `shutdown.*` | when a shutdown starts |
| Handler | application configuration via `ConfigWatch` | once per session or per message |

What a changed setting does on reload, and which settings need a restart, is specified in [configuration.md](../reference/configuration.md#reload-classes). In short: the listener set, `max_connections`, the shutdown timings, `max_fds`, and `max_memory` apply at once; timeouts, socket options, `per_connection_buffer`, and `trace_commands` apply to new connections; `backlog`, `reuse_addr`, `reuse_port`, `ipv6_only`, and everything set only on the builder require a restart, and a reload that changes one of those settings fails.

### 8. Reload coordinator

Responsibilities:
- receive reload requests from `SIGHUP` and `ServerHandle::reload()`
- run one reload at a time, and fold requests that arrive meanwhile into one more reload
- call the application's loader, validate, bind added listeners, publish, then stop removed listeners
- roll back added listeners when a bind fails in `FailFast` mode
- report changed settings, and reject a reload that changes a setting that requires a restart

It is the only component that changes the listener set after startup. The procedure is specified in [hot-reload.md](../reference/hot-reload.md).

### 9. Shutdown coordinator

Responsibilities:
- handle `SIGTERM`, `SIGINT`, and `SIGQUIT`, and a second signal that forces shutdown
- stop the accept loops, wait `grace_period`, cancel the shared `CancellationToken`, and wait up to `drain_timeout`
- force-close remaining connections and end `Server::run()`
- stop the reload coordinator, so no reload starts during shutdown

### 10. Telemetry

Responsibilities:
- keep the status counters (atomics) behind `ServerHandle::status()` and the health check
- emit `tracing` spans for connections, sessions, and messages
- with the `otel` feature, expose the counters and record histograms through OpenTelemetry instruments

Telemetry never records SMTP session data. The metric names, span structure, and forbidden data are in [telemetry.md](../reference/telemetry.md).

### 11. Error types

Responsibilities:
- define typed error enums: `Error`, `ConfigError`, `ReloadError`, `ProtocolError`, `AddressError`
- keep the cause of each error in `source()`

The variants are listed in [api-specification.md](../reference/api-specification.md#errors).

### Milter client sample

A reference implementation of the MTA side, with a connection pool, will live in `examples/milter_client/`. It is not part of the library: it uses only the library's public protocol types (`Frame`, `Command`, `Reply`), and CI builds it. Its design is recorded in [milter-client.md](../reference/milter-client.md).

## Module structure

```mermaid
flowchart LR
    root["rust-milter"]

    root --> server
    root --> connection
    root --> frame
    root --> message
    root --> engine
    root --> handler
    root --> config
    root --> reload
    root --> telemetry
    root --> error

    server --> listener
    server --> acceptor
    server --> shutdown

    connection --> transport_state["state"]
    connection --> timeout

    frame --> parser
    frame --> writer

    message --> command
    message --> reply

    config --> validate
    config --> watch

    telemetry --> status
    telemetry --> otel["otel (feature)"]

```

Module boundaries follow the components. `config` knows nothing about sockets; `frame` and `message` know nothing about Tokio; `telemetry::otel` compiles only with the `otel` feature.

## Data flow

### Inbound: MTA to milter

```mermaid
sequenceDiagram
    participant Socket
    participant Connection
    participant Frame
    participant Message
    participant Engine as Protocol engine
    participant Handler

    Socket->>Connection: bytes
    Connection->>Frame: parse (length checked against per_connection_buffer)
    Frame->>Message: decode to Command
    Message->>Engine: Command
    Engine->>Engine: check state, store macros
    Engine->>Handler: callback (for example header)
    Handler-->>Engine: decision
```

1. Connection reads bytes from the socket.
2. The frame parser checks the length against `per_connection_buffer` and extracts a frame.
3. The message decoder converts it into a `Command`.
4. The protocol engine checks the command against the protocol state and stores any macros.
5. For an event the handler subscribed to, the engine calls the matching callback, which returns a decision.

### Outbound: milter to MTA

```mermaid
sequenceDiagram
    participant Handler
    participant Engine as Protocol engine
    participant Message
    participant Frame
    participant Connection
    participant Socket

    Handler->>Engine: decision
    Engine->>Message: Reply, or none where the protocol forbids a reply
    Message->>Message: validate (reply code, header names, body size)
    Message->>Frame: encode
    Frame->>Connection: bytes
    Connection->>Socket: write and flush
```

### Configuration reload

```mermaid
sequenceDiagram
    participant Op as Operator
    participant Reload as Reload coordinator
    participant Config as Configuration
    participant Server as Server (accept loop)
    participant Old as Open connections
    participant New as New connections

    Op->>Reload: SIGHUP
    Reload->>Reload: load and validate
    Reload->>Server: bind added listeners
    Reload->>Config: publish generation N+1
    Reload->>Server: stop removed listeners
    Note over Old: keep the generation N snapshot until they close
    Server->>New: accept with the generation N+1 snapshot
    Reload-->>Op: report (changed, listeners added and removed)
```

A failed load, validation, or bind leaves generation N in place; see [hot-reload.md](../reference/hot-reload.md).

### Shutdown

```mermaid
sequenceDiagram
    participant Signal
    participant Shutdown as Shutdown coordinator
    participant Server as Server (accept loop)
    participant Handlers
    participant Connections

    Signal->>Shutdown: SIGTERM, SIGINT, or SIGQUIT
    Shutdown->>Server: stop accepting
    Shutdown->>Shutdown: wait grace_period
    Shutdown->>Handlers: cancel the CancellationToken
    Handlers->>Connections: finish the current command, close
    Shutdown->>Shutdown: wait up to drain_timeout
    Shutdown->>Connections: force-close the rest
    Shutdown-->>Signal: Server::run() returns
```

## Concurrency model

### Tasks

The library runs on the Tokio multi-threaded runtime that the application creates:
- one worker thread per CPU core by default; the application sets the number on its runtime
- one accept task per listener
- one task per connection
- one reload coordinator task and one shutdown coordinator task

### Shared state

Shared between tasks:
- the configuration channel (`tokio::sync::watch`), written only by the reload coordinator
- the status counters (atomics)
- the shutdown `CancellationToken`

Isolated per connection:
- the socket, the buffers, and the configuration snapshot
- the protocol state and macros, held by the protocol engine
- the handler's `Session` and `Message` values, created and dropped by the protocol engine

### Async and sync operations

Async: socket I/O, handler callbacks, timeouts, reload, and shutdown coordination.

Sync: frame parsing, message encoding and decoding, configuration validation.

## State management

Two separate state machines track each connection:

| State machine | Tracks | Specified in |
|---------------|--------|--------------|
| Transport | `CONNECTED`, `IDLE`, `ACTIVE`, `DRAINING`, `ERROR`, `CLOSED`, driven by socket events, timeouts, and shutdown | [tcp-server.md](../reference/tcp-server.md#connection-lifecycle) |
| Protocol | `ST_INIT` to `ST_QUIT`, driven by Milter commands, as libmilter defines them | [milter-protocol.md](../reference/milter-protocol.md#protocol-states) |

The Connection component owns the transport state. The protocol engine owns the protocol state and validates every command against it. For a command in a wrong state it follows libmilter where libmilter's behavior works: it recovers for `SMFIC_HELO`, `SMFIC_MAIL`, and `SMFIC_UNKNOWN`, and ignores `SMFIC_ABORT` and `SMFIC_QUIT`. Where libmilter would ignore a command and leave the MTA waiting for a reply, including `SMFIC_QUIT_NC`, it closes the connection ([ADR 0001](../adr/0001-commands-in-wrong-state-recover-like-libmilter-or-close.md)). Handlers never see protocol state.

## Resource management

### Memory

- each connection has one read buffer and one write buffer that grow up to `per_connection_buffer` (default 128 KB, minimum 65,540 bytes, the largest default-size milter packet)
- frame data is shared without copying through `bytes::Bytes`
- `max_memory` caps the total of all connection buffers

### File descriptors

- one FD per open connection and one per listener
- `max_fds` must leave room for `max_connections` plus the listeners plus 10
- on `EMFILE` or `ENFILE` the accept loop backs off and retries

All limits and defaults are in [configuration.md](../reference/configuration.md).

## Implementation approach

Practices the implementation follows inside the data plane:
- `tokio::select!` with the shutdown token wherever a task waits, so cancellation is prompt and safe.
- Buffered I/O on each connection, with buffers reused for the life of the connection and bounded by `per_connection_buffer`.
- `bytes::Bytes` for frame data, so body chunks and header values are shared rather than copied.
- No blocking calls on runtime threads; CPU-heavy work, if a handler needs it, goes to `tokio::task::spawn_blocking`.
- File descriptors released as soon as a connection closes; on exhaustion the accept loop backs off instead of failing.

## Error handling strategy

### Error classes

Fatal for the connection, which is closed:
- protocol violations and malformed frames
- frames longer than `per_connection_buffer`
- expired timeouts
- I/O errors

Recoverable within the connection:
- incomplete frames, which need more data
- transient I/O errors, which are retried

Fatal for startup:
- invalid configuration, returned by `build()`
- a bind failure in `FailFast` mode, returned by `run()`

Not fatal on reload:
- a failed load, an invalid configuration, a changed setting that requires a restart, or a bind failure leaves the running configuration in place and is reported as a `ReloadError`

### Propagation

- Use `Result<T, Error>` throughout, with typed error enums.
- Convert errors at component boundaries and keep the cause in `source()`.
- Record errors in `tracing` without SMTP data (see [Data that must never be recorded](../reference/telemetry.md#data-that-must-never-be-recorded)).

## Security architecture

### Trust boundaries

- network input from the MTA: untrusted
- handler implementation: trusted
- configuration: trusted, but validated before use

Input validation:
- frame length, before any allocation
- command code and data format
- reply contents before encoding
- resource limits

### Attack mitigation

Denial of service: connection limits, timeouts from accept on, and buffer limits. rust-milter does not rate-limit per source; a firewall does that.

Network exposure: the milter protocol has no authentication or encryption. Bind to loopback unless the MTA is on another host, and restrict access with firewall rules when it is.

Data exposure:
- session isolation: per-session state is dropped at the end of each SMTP session, including on `SMFIC_QUIT_NC`
- telemetry and logs carry no envelope, header, body, or macro data (see [telemetry.md](../reference/telemetry.md))
- configuration values are never logged; reloads log the names of changed settings only

## Component dependencies

```mermaid
flowchart TD
    Server --> Connection
    Server --> Engine
    Server --> Config
    Reload --> Config
    Reload --> Server
    Shutdown --> Server
    Engine --> Connection
    Engine --> Handler
    Engine --> Message
    Handler --> Config
    Connection --> Frame
    Connection --> Message
    Message --> Frame
    Server --> Telemetry
    Connection --> Telemetry
    Reload --> Telemetry
    Server --> Error
    Connection --> Error
    Engine --> Error
    Engine --> Telemetry
    Frame --> Error
    Message --> Error
    Config --> Error
    Reload --> Error
```

Rules:
- no circular dependencies
- lower layers (Frame, Message, Config, Error) do not depend on higher layers
- traits define the boundaries
- the error types are shared

## Extension points

The `Handler` trait is the main extension point. The user implements the filtering logic in it, and an application can have several implementations.

The reload loader is the second one: the application decides where configuration comes from.

Possible future extension traits:
- health check provider
- authentication provider

Metrics need no extension trait: they come from OpenTelemetry and `ServerHandle::status()` ([ADR 0004](../adr/0004-metrics-come-from-opentelemetry-and-status.md)).

## Testing architecture

### Unit tests

All synchronous:
- frame parsing, including the length limit
- message encoding and decoding, including reply validation
- configuration validation and the reload class of every setting
- error type conversions

### Integration tests

Connection:
- socket communication and frame I/O
- protocol flows, including `SMFIC_QUIT_NC` with session isolation
- timeout behavior, from accept on

Server:
- many concurrent connections
- graceful shutdown
- resource limits
- reload during traffic: open connections keep their snapshot, new connections get the new one
- error recovery

### Load tests

Performance targets:
- 10,000 concurrent connections
- p99 latency per frame under 1 millisecond
- no memory leaks under load

## References

- [API specification](../reference/api-specification.md)
- [Configuration specification](../reference/configuration.md)
- [TCP server specification](../reference/tcp-server.md)
- [Milter protocol specification](../reference/milter-protocol.md)
- [Configuration hot reload](../reference/hot-reload.md)
- [Telemetry](../reference/telemetry.md)
- [Milter client sample](../reference/milter-client.md)
