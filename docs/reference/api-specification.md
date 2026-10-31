# API specification

The public API of the rust-milter server library: its types, traits, errors, and usage contracts. Settings are specified in [configuration.md](configuration.md), telemetry in [telemetry.md](telemetry.md), and the milter client in [milter-client.md](milter-client.md). Usage examples are in the [tutorial](../tutorials/) and the [how-to guides](../how-to/). The design principles behind the API are explained in [Library architecture](../explanation/architecture.md).

## Contents

- [Public API surface](#public-api-surface)
- [Server, builder, and handle](#server-builder-and-handle)
- [Handler trait](#handler-trait)
- [Connection](#connection)
- [Frame](#frame)
- [Command](#command)
- [Reply](#reply)
- [Configuration and reload API](#configuration-and-reload-api)
- [Errors](#errors)
- [Async patterns](#async-patterns)
- [Thread safety](#thread-safety)
- [Versioning and stability](#versioning-and-stability)
- [Performance characteristics](#performance-characteristics)
- [Usage constraints](#usage-constraints)
- [Possible future additions](#possible-future-additions)

## Public API surface

### Types

| Type | Category | Description |
|------|----------|-------------|
| `Server<H, A>` | Core | The milter server, generic over the handler `H` and the application configuration `A` (default `()`) |
| `ServerBuilder<H, A>` | Core | Builds a `Server` |
| `ServerHandle<A>` | Core | Controls a running server: shutdown, reload, status |
| `ServerConfig` | Configuration | Server settings ([configuration.md](configuration.md)) |
| `ConfigWatch<A>` | Configuration | Read access to the current application configuration |
| `HandlerContext<A>` | Configuration | Passed to the handler factory at build time |
| `Reloaded<A>` | Configuration | A new `ServerConfig` and application configuration returned by a reload loader |
| `ReloadReport` | Configuration | Result of a reload |
| `Status` | Observability | In-process snapshot of the status counters |
| `Connection` | I/O | Per-connection handle |
| `ConnectionInfo` | I/O | Read-only facts about a connection: ID, addresses, accept time |
| `DisconnectReason` | I/O | Why a connection closed |
| `Frame` | Protocol | Milter protocol frame |
| `Command` | Protocol | Commands from MTA to milter (`SMFIC_*`) |
| `Reply` | Protocol | Replies from milter to MTA (`SMFIR_*`) |
| `Handler` | Trait | Connection handler that the user implements |
| `Error` | Error | Library error type |
| `ConfigError`, `ReloadError`, `ProtocolError`, `AddressError` | Error | Error types for specific areas |
| `Result<T>` | Error | `std::result::Result<T, Error>` |
| `protocol` (module) | Protocol | The constants from [milter-protocol.md](milter-protocol.md): `SMFIC_*`, `SMFIR_*`, `SMFIF_*`, `SMFIP_*`, `SMFIM_*`, `SMFIA_*` |

### Re-exported types

| Type | Source | Purpose |
|------|--------|---------|
| `CancellationToken` | tokio-util | Graceful shutdown coordination |
| `Bytes`, `BytesMut` | bytes | Frame data without copying |

### Cargo features

| Feature | Default | Adds |
|---------|---------|------|
| `otel` | off | OpenTelemetry metric instruments and `ServerBuilder::meter_provider` ([telemetry.md](telemetry.md)) |
| `serde` | off | `Serialize` and `Deserialize` for `ServerConfig` and its parts ([configuration.md](configuration.md#file-format-serde-feature)) |

## Server, builder, and handle

### ServerBuilder

| Method | Description |
|--------|-------------|
| `Server::builder()` | Start a builder |
| `config(ServerConfig)` | Initial server configuration (required) |
| `app_config(A)` | Initial application configuration (default `A::default()` when `A: Default`) |
| `handler(f)` | Handler factory: a function that takes `&HandlerContext<A>` and returns the handler. It is called once at build time, and the handler is cloned for each connection |
| `reload_with(loader)` | Reload loader: an async function that returns `Reloaded<A>` or an error. The error type must implement `std::error::Error` and be `Send + Sync + 'static` |
| `meter_provider(provider)` | OpenTelemetry meter provider (feature `otel`) |
| `build()` | Validate the configuration and build the server. Returns `Result<Server<H, A>>` |

`HandlerContext<A>` gives the factory a `ConfigWatch<A>` and the server's `CancellationToken`, so a handler can follow reloads and shutdown from the start.

### Server

| Method | Description |
|--------|-------------|
| `handle()` | Return a `ServerHandle<A>`. Can be called before `run()` |
| `run()` | Bind the listeners, register the signal handlers, and serve until shutdown. Consumes the server. Returns `Result<()>` |

Lifecycle:
1. `build()` validates the configuration. A validation error is returned here, before any socket is opened.
2. `run()` binds the listeners according to `bind_mode`, registers the `SIGTERM`, `SIGINT`, `SIGQUIT`, and `SIGHUP` handlers, and starts accepting.
3. A shutdown signal, `ServerHandle::shutdown()`, or a fatal error ends `run()`.

### ServerHandle

`ServerHandle<A>` is `Clone + Send + Sync` and stays valid after the server stops.

| Method | Description |
|--------|-------------|
| `shutdown()` | Start a graceful shutdown |
| `force_shutdown()` | Shut down without draining |
| `wait()` | Wait until the server has stopped |
| `is_running()` | Whether the server is running |
| `status()` | In-process `Status` snapshot ([telemetry.md](telemetry.md)) |
| `reload()` | Run the registered loader, as `SIGHUP` does. Returns `Result<ReloadReport, ReloadError>` |
| `reload_with_config(Reloaded<A>)` | Apply a configuration the caller built. Returns `Result<ReloadReport, ReloadError>` |
| `config()` | Current `Arc<ServerConfig>` |
| `config_watch()` | A `ConfigWatch<A>` for the application configuration |
| `config_generation()` | Current generation number |

## Handler trait

`Handler` is the trait the user implements. A handler type must be `Clone + Send + Sync + 'static`.

| Method | Parameters | Returns | Required | Description |
|--------|------------|---------|----------|-------------|
| `handle` | the `Connection`; the shutdown `CancellationToken` | a `Send` future of `Result<()>` | yes | Runs the whole milter conversation on one connection |
| `on_connect` | `&ConnectionInfo` | a `Send` future of `Result<()>` | no; the default returns `Ok(())` | Called after accept, before `handle`. An error closes the connection |
| `on_disconnect` | `&ConnectionInfo`; `DisconnectReason` | nothing | no; the default does nothing | Called once after the connection has closed |

The trait declares `handle` and `on_connect` as methods that return a `Send` future, not as `async fn`, so the server can run them on the multi-threaded runtime. Implementations can still write them as `async fn`.

Contract:
- `handle` runs once per connection, in its own task, with exclusive ownership of the `Connection`.
- `handle` implements the protocol exchange, starting with `SMFIC_OPTNEG`, and must follow [milter-protocol.md](milter-protocol.md). rust-milter supports protocol versions 2 through 6.
- `handle` must stop when `shutdown` is cancelled: finish the current command, then return.
- An error returned from `on_connect` or `handle` closes the connection.
- On `SMFIC_QUIT_NC`, `handle` must drop all per-session state, including macro values, before it processes the next `SMFIC_CONNECT`. Only the negotiated options may survive. This is the session isolation rule from [milter-client.md](milter-client.md).
- Settings come from the `ConfigWatch<A>` in the handler. Read the application configuration once per SMTP session (at `SMFIC_CONNECT`) or once per message (at `SMFIC_MAIL`), and keep that value until the session or message ends.

Lifecycle: the server clones the handler for each new connection and drops the clone when the connection closes. Handlers share no state unless the implementation adds it on purpose.

## Connection

The I/O handle for one MTA connection. It owns its socket and lives as long as the connection.

| Method | Description |
|--------|-------------|
| `read_frame()` | Read the next frame (async). Returns `Result<Option<Frame>>`; `None` means the MTA closed the connection cleanly |
| `write_frame(&Frame)` | Write a frame (async) |
| `flush()` | Flush buffered writes (async) |
| `info()` | `&ConnectionInfo` |
| `config()` | The `Arc<ServerConfig>` snapshot this connection was accepted with |

`ConnectionInfo`:

| Field | Type | Description |
|-------|------|-------------|
| `id` | `u64` | Connection ID, unique within the server's lifetime |
| `peer_addr` | `SocketAddr` | The MTA's address |
| `local_addr` | `SocketAddr` | The listener address that accepted the connection |
| `accepted_at` | `Instant` | Time `accept()` returned |
| `config_generation` | `u64` | Generation of the configuration snapshot |

`Connection` tracks the protocol state from the command codes it reads and the reply codes it writes, with libmilter's transition rules. When `SMFIC_QUIT_NC` arrives outside `ST_ENDM` and `ST_UNKN`, `read_frame` returns `Error::Protocol` and the connection closes; the handler never sees that command. A handler therefore only has to handle `SMFIC_QUIT_NC` in the states where it is valid.

`read_frame` and `write_frame` enforce the connection's `timeouts.connect`, `timeouts.read`, `timeouts.write`, and `timeouts.idle`, and the `per_connection_buffer` limit, all from the snapshot. The byte-level operations, buffers, and transport states in [tcp-server.md](tcp-server.md) are internal.

`DisconnectReason`: `ClientClosed`, `ServerShutdown`, `Timeout`, `Error`, `Reset`, `LimitReached`.

Thread safety: `Send`, not `Sync`.

## Frame

A Milter protocol frame.

| Field | Type | Description |
|-------|------|-------------|
| `command` | `u8` | Command or reply code |
| `data` | `Bytes` | Payload |

On the wire a frame is a 4-byte length (network byte order), the command byte, and the data. The length counts the command byte and the data.

Size: the whole packet (4 + 1 + data length) must fit in the connection's `per_connection_buffer`, 128 KB by default. A longer packet fails with `Error::FrameTooLarge` before any memory is allocated for it. The default holds the largest default-size milter packet, 65,540 bytes.

| Method | Description |
|--------|-------------|
| `new(command, data)` | Create a frame |
| `check(&[u8], max)` | Whether the buffer holds a complete frame, and its length; fails if the length exceeds `max` |
| `parse(&mut BytesMut, max)` | Remove and return one frame from the buffer |
| `write_to(&mut BytesMut)` | Serialize the frame |

Fields are public and the type is immutable. Thread safety: `Send + Sync`.

## Command

Type-safe form of the commands the MTA sends.

| Variant | Command |
|---------|---------|
| `OptNeg { version, actions, protocol }` | `SMFIC_OPTNEG` |
| `Macro { stage, macros }` | `SMFIC_MACRO`; `stage` is the raw stage byte, including `L`, `B`, and `U` from Postfix |
| `Connect { hostname, family, port, address }` | `SMFIC_CONNECT` |
| `Helo(String)` | `SMFIC_HELO` |
| `Mail(Vec<String>)` | `SMFIC_MAIL` |
| `Rcpt(Vec<String>)` | `SMFIC_RCPT` |
| `Data` | `SMFIC_DATA` |
| `Header { name, value }` | `SMFIC_HEADER` |
| `Eoh` | `SMFIC_EOH` |
| `Body(Bytes)` | `SMFIC_BODY` |
| `BodyEob(Bytes)` | `SMFIC_BODYEOB`, with optional trailing body data |
| `Abort` | `SMFIC_ABORT` |
| `Unknown(String)` | `SMFIC_UNKNOWN` |
| `Quit` | `SMFIC_QUIT` |
| `QuitNewConnection` | `SMFIC_QUIT_NC` |

`Command::from_frame(frame)` checks the command code and the data format, and returns `Error::Protocol` on invalid input. An unknown command code is a protocol error. Strings are decoded as UTF-8 where valid; otherwise the raw bytes are kept, because SMTPUTF8 and legacy 8-bit data both occur.

## Reply

Type-safe form of the replies the milter sends.

| Variant | Reply |
|---------|-------|
| `OptNeg { version, actions, protocol, macros }` | `SMFIC_OPTNEG` response; `macros` is the optional symbol list per `SMFIM_*` stage |
| `Continue` | `SMFIR_CONTINUE` |
| `Accept` | `SMFIR_ACCEPT` |
| `Reject` | `SMFIR_REJECT` |
| `TempFail` | `SMFIR_TEMPFAIL` |
| `Discard` | `SMFIR_DISCARD` |
| `ReplyCode(String)` | `SMFIR_REPLYCODE` |
| `Skip` | `SMFIR_SKIP` |
| `Progress` | `SMFIR_PROGRESS` |
| `Quarantine(String)` | `SMFIR_QUARANTINE` |
| `AddHeader { name, value }` | `SMFIR_ADDHEADER` |
| `InsertHeader { index, name, value }` | `SMFIR_INSHEADER` |
| `ChangeHeader { index, name, value }` | `SMFIR_CHGHEADER`; an empty value deletes the header |
| `AddRecipient(String)` | `SMFIR_ADDRCPT` |
| `AddRecipientWithArgs { rcpt, args }` | `SMFIR_ADDRCPT_PAR` |
| `DeleteRecipient(String)` | `SMFIR_DELRCPT` |
| `ChangeFrom { from, args }` | `SMFIR_CHGFROM` |
| `ReplaceBody(Bytes)` | `SMFIR_REPLBODY` |

The enum has no variants for `SMFIR_SHUTDOWN`, `SMFIR_CONN_FAIL`, or `SMFIR_SETSYMLIST`: Sendmail rejects the first, neither MTA accepts the second, and the third is never sent as a packet. The macro list travels in `Reply::OptNeg`.

`Reply::to_frame()` validates before encoding:
- `ReplyCode` text: a 4xx or 5xx code, the multi-line rules from [milter-protocol.md](milter-protocol.md), and every literal `%` written as `%%`
- header names: no colon, no whitespace, no control characters
- `ReplaceBody` data: at most 65,535 bytes per frame

Thread safety: `Send + Sync`.

## Configuration and reload API

All settings, defaults, valid ranges, and reload behavior are in [configuration.md](configuration.md). The reload procedure is in [hot-reload.md](hot-reload.md).

`ConfigWatch<A>`:

| Method | Description |
|--------|-------------|
| `current()` | Current `Arc<A>`; never blocks |
| `generation()` | Generation of the current value |
| `changed()` | Async; completes when a new generation is published |

`Reloaded<A>`:

| Field | Type |
|-------|------|
| `server` | `ServerConfig` |
| `app` | `A` |

`ReloadReport`:

| Field | Type | Description |
|-------|------|-------------|
| `generation` | `u64` | Generation after the reload |
| `changed` | `Vec<&'static str>` | Names of changed settings |
| `pending_restart` | `Vec<&'static str>` | Changed settings that need a restart |
| `listeners_added` | `Vec<SocketAddr>` | Listeners bound by the reload |
| `listeners_removed` | `Vec<SocketAddr>` | Listeners stopped by the reload |
| `bind_failures` | `Vec<(SocketAddr, String)>` | Failed binds (`BestEffort` only) |

## Errors

Errors are `thiserror` enums. Every error type implements `std::error::Error`, `Display`, and `Debug`, is `Send + Sync + 'static`, and keeps its cause in `source()`.

### Error

| Variant | Meaning | Typical effect |
|---------|---------|----------------|
| `Io(std::io::Error)` | Socket read or write failed | connection closed |
| `Protocol(ProtocolError)` | Malformed frame, unknown command, invalid state | connection closed |
| `Timeout(TimeoutKind)` | `Connect`, `Read`, `Write`, or `Idle` timeout expired | connection closed |
| `ConnectionClosed` | The MTA closed the connection in the middle of a frame | connection closed |
| `ConnectionReset` | The MTA reset the connection | connection closed |
| `FrameTooLarge { length, max }` | Packet longer than `per_connection_buffer` | connection closed |
| `Config(ConfigError)` | Invalid configuration at build time | `build()` fails, exit code 2 |
| `Bind { addr, source }` | A listener could not bind | `run()` fails in `FailFast` mode |
| `PartialBind(Vec<(SocketAddr, std::io::Error)>)` | Some listeners failed in `BestEffort` mode | reported; the server keeps running |
| `ResourceLimit(ResourceKind)` | `MaxConnections`, `MaxFds`, or `MaxMemory` reached | connection rejected or closed |
| `ShuttingDown` | The operation is not allowed during shutdown | none |

### ConfigError

| Variant | Meaning |
|---------|---------|
| `OutOfRange { field, value, allowed }` | A value outside its range in [configuration.md](configuration.md) |
| `MissingBindAddress` | `bind_addresses` is empty |
| `DuplicateBindAddress(SocketAddr)` | The same address appears twice |
| `Address(AddressError)` | An address string could not be parsed |
| `Inconsistent { rule, detail }` | A cross-field rule failed, such as `max_fds` too small for `max_connections` |
| `Unsupported { field, reason }` | The platform does not support a setting, such as `reuse_port` |

`AddressError`: `InvalidFormat` (including `unix:` and `local:` addresses), `InvalidHost`, `InvalidPort`.

### ReloadError

| Variant | Meaning |
|---------|---------|
| `NoLoader` | `reload()` was called without a registered loader |
| `Load(Box<dyn std::error::Error + Send + Sync>)` | The application's loader failed; its error is kept as the source |
| `Validation(ConfigError)` | The new configuration is invalid |
| `Bind { addr, source }` | A new listener failed to bind in `FailFast` mode; the reload was rolled back |
| `NoListener` | The new configuration would leave no listener |
| `ShuttingDown` | Shutdown has started |

`ReloadError::Load` boxes the loader's error because its type belongs to the application. It is the only boxed error in the public API.

A failed reload leaves the server running with the previous generation.

## Async patterns

Async:
- `Connection::read_frame()`, `write_frame()`, `flush()`
- `Handler::handle()`, `Handler::on_connect()`
- `Server::run()`
- `ServerHandle::wait()`, `reload()`, `reload_with_config()`
- `ConfigWatch::changed()`

Sync:
- `Frame::parse()`, `Command::from_frame()`, `Reply::to_frame()`
- `ServerConfig` construction and `ServerBuilder` setters
- `ConfigWatch::current()`, `ServerHandle::status()`, `config()`

Cancellation safety:
- `Connection::read_frame()` is cancellation-safe: if the future is dropped, no bytes are lost and the next call continues.
- `Connection::write_frame()` is not: if the future is dropped, part of the frame may have been written. Drop it only when the connection is about to close.
- Use `tokio::select!` with the `CancellationToken` around `read_frame()`, the point where a handler waits for the MTA.

## Thread safety

| Type | Send | Sync |
|------|------|------|
| `Server<H, A>` | yes | yes |
| `ServerHandle<A>` | yes | yes |
| `ConfigWatch<A>` | yes | yes |
| `Connection` | yes | no (exclusive I/O ownership) |
| `ConnectionInfo` | yes | yes |
| `Frame`, `Command`, `Reply` | yes | yes |
| `Error` | yes | yes |

`A` must be `Send + Sync + 'static`. `H` must be `Clone + Send + Sync + 'static`.

## Versioning and stability

rust-milter follows semantic versioning: breaking changes bump MAJOR, new features MINOR, and fixes PATCH.

Before 1.0, any API can change. From 1.0, public types, traits, method signatures, error variants, and configuration fields are stable.

Error enums and the configuration enums (`BindMode`, `TraceCommands`) are `#[non_exhaustive]`, so adding a variant is not a breaking change. Configuration structs keep public fields and are not `#[non_exhaustive]`, because that attribute would forbid building them with `..Default::default()` outside the crate. Adding a configuration field is a MINOR change; code that names every field without `..Default::default()` has to add the new one.

Deprecation:
1. Mark the item with `#[deprecated]`.
2. Provide a migration path.
3. Remove it in the next MAJOR version.
4. Record it in the CHANGELOG.

## Performance characteristics

Zero copy: frame data, body chunks, and header values use `bytes::Bytes`. Strings are copied only where conversion requires it.

Allocation:
- per connection: one read buffer and one write buffer, growing up to `per_connection_buffer`
- shared: the published configuration (`Arc`), the status counters

Latency targets per frame: P50 < 100 µs, P99 < 1 ms, P99.9 < 10 ms.

Reading the configuration costs one `Arc` clone at accept. Handlers reading `ConfigWatch::current()` take a short read lock with no contention from other readers.

## Usage constraints

Runtime: Tokio, multi-threaded. The application creates the runtime. Other runtimes are not supported.

Toolchain and platforms:
- Rust 1.85 or later, edition 2024; the crate declares `rust-version`
- Linux, macOS, or BSD
- file descriptors: at least `max_connections` + listeners + 10

Recommended for production: 4 or more CPU cores, 1 GB or more of RAM for 10,000 connections, and `ulimit -n` of at least 10,010.

## Possible future additions

- TLS support
- authentication
- a higher-level session API that runs the protocol and calls per-command callbacks, so handlers do not drive frames themselves

New traits will come with default method implementations where possible, and optional functionality will sit behind Cargo features.
