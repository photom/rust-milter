# API specification

The public API of the rust-milter server library: its types, traits, errors, and usage contracts. Settings are specified in [configuration.md](configuration.md), telemetry in [telemetry.md](telemetry.md), and the milter client sample, which is not part of this API, in [milter-client.md](milter-client.md). Usage examples are in the [tutorial](../tutorials/) and the [how-to guides](../how-to/). The design principles behind the API are explained in [Library architecture](../explanation/architecture.md), and the choice of a callback model in [ADR 0002](../adr/0002-library-runs-the-protocol-handlers-decide.md).

## Contents

- [Public API surface](#public-api-surface)
- [Server, builder, and handle](#server-builder-and-handle)
- [Handler trait](#handler-trait)
- [Contexts](#contexts)
- [Decisions and reply codes](#decisions-and-reply-codes)
- [Protocol engine behavior](#protocol-engine-behavior)
- [Text from the MTA](#text-from-the-mta)
- [Connection information](#connection-information)
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
| `ServerBuilder<H, A, F>` | Core | Builds a `Server`. `F` is the type of the handler factory, `()` until `handler` is called |
| `ServerHandle<A>` | Core | Controls a running server: shutdown, reload, status |
| `ServerConfig` | Configuration | Server settings ([configuration.md](configuration.md)) |
| `ConfigWatch<A>` | Configuration | Read access to the current application configuration |
| `HandlerContext<A>` | Configuration | Passed to the handler factory at build time |
| `Reloaded<A>` | Configuration | A new `ServerConfig` and application configuration returned by a reload loader |
| `ReloadLoader<A>` | Trait | The reload loader the application implements on its own type |
| `ReloadReport` | Configuration | Result of a reload |
| `Status` | Observability | In-process snapshot of the status counters |
| `Handler` | Trait | The callbacks the user implements |
| `SessionContext<H>`, `MessageContext<H>`, `EomContext<H>` | Handler | Passed to the callbacks: session and message state, macros, and at end of message the modification methods |
| `ConnectInfo` | Handler | The SMTP client's hostname and address from `SMFIC_CONNECT` |
| `MilterText` | Handler | Text from the MTA: raw bytes, readable as UTF-8 when valid |
| `Events` | Handler | The protocol stages a handler wants to receive |
| `Actions` | Handler | The message modifications a handler may perform |
| `Negotiated` | Handler | The result of option negotiation: `version()`, `actions()`, `protocol()` for the flags in the milter's answer, and `mta_protocol()` for the flags the MTA offered |
| `ConnectDecision`, `Decision`, `BodyDecision` | Handler | The decisions a callback returns, per stage |
| `ReplyCode` | Handler | A validated custom SMTP reply |
| `SessionEnd` | Handler | Why an SMTP session ended |
| `ConnectionInfo` | I/O | Read-only facts about an MTA connection: ID, addresses, accept time |
| `Frame` | Protocol | Milter protocol frame (used by the milter client sample in `examples/` and by tests) |
| `Command` | Protocol | Commands from MTA to milter (`SMFIC_*`) |
| `AddressFamily` | Protocol | The SMTP client's address family in `Command::Connect` (`SMFIA_*`) |
| `Reply` | Protocol | Replies from milter to MTA (`SMFIR_*`) |
| `Error` | Error | Library error type |
| `ConfigError`, `ReloadError`, `ProtocolError`, `ReplyError`, `AddressError` | Error | Error types for specific areas |
| `Result<T>` | Error | `std::result::Result<T, Error>` |
| `protocol` (module) | Protocol | The constants from [milter-protocol.md](milter-protocol.md): `SMFIC_*`, `SMFIR_*`, `SMFIF_*`, `SMFIP_*`, `SMFIM_*`, `SMFIA_*`, and the `SMFI_*` versions and flag sets |

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
| `config(ServerConfig)` | Initial server configuration (required). Without it, `build()` validates the defaults, which have no bind address, and fails |
| `app_config(A)` | Initial application configuration; without it, `A` is `()`. Call it before `handler` and `reload_with`: the factory receives the configuration's type, and the loader returns it |
| `handler(f)` | Handler factory: a function that takes `&HandlerContext<A>` and returns the handler. It is called once at build time; all connections share the one handler. `build()` exists only after `handler`, so a builder without a handler does not compile |
| `reload_with(loader)` | Reload loader: a value of a type that implements `ReloadLoader<A>`. `SIGHUP` and `ServerHandle::reload()` run its `load` method |
| `meter_provider(provider)` | OpenTelemetry meter provider (feature `otel`) |
| `build()` | Validate the configuration and build the server. Returns `Result<Server<H, A>>` |

`HandlerContext<A>` gives the factory a `ConfigWatch<A>` (`config_watch()`) and the server's `CancellationToken` (`shutdown()`), so a handler can follow reloads and shutdown from the start.

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
| `shutdown()` | Start a graceful shutdown. Does nothing once a shutdown has started |
| `force_shutdown()` | Shut down without the grace period or the drain: close every connection at once. Also ends a graceful shutdown in progress |
| `wait()` | Wait until the server has stopped |
| `is_running()` | Whether the server is running |
| `status()` | In-process `Status` snapshot: the health check fields in [tcp-server.md](tcp-server.md#health-check), `config_generation` from [hot-reload.md](hot-reload.md#logging-and-metrics), and the status counters behind the metrics in [telemetry.md](telemetry.md#metrics) |
| `reload()` | Run the registered loader, as `SIGHUP` does. Returns `Result<ReloadReport, ReloadError>`. Called before `run()`, it waits until `run()` has bound the listeners |
| `reload_with_config(Reloaded<A>)` | Apply a configuration the caller built. Returns `Result<ReloadReport, ReloadError>` |
| `config()` | Current `Arc<ServerConfig>` |
| `config_watch()` | A `ConfigWatch<A>` for the application configuration |
| `config_generation()` | Current generation number |

`Status` is a snapshot; reading it takes no lock on the connection path. It is `#[non_exhaustive]`, so counters can be added.

| Field | Type | Description |
|-------|------|-------------|
| `accepting` | `bool` | The server is accepting connections: `run()` has bound and no shutdown has started |
| `connections` | `usize` | Open connections |
| `uptime` | `Duration` | Time since `run()` bound the listeners; zero before |
| `last_error` | `Option<String>` | The last listener error, such as a failed bind in `BestEffort` mode |
| `shutting_down` | `bool` | A shutdown has started |
| `listeners` | `Vec<ListenerStatus>` | One entry per address in `bind_addresses`, in their order |
| `config_generation` | `u64` | The current configuration generation, starting at 1 |
| `connections_accepted` | `u64` | Connections accepted since `run()` started, rejected ones included |
| `connections_rejected` | `u64` | Connections closed at once because `max_connections` or `max_memory` was reached |
| `bytes_received` | `u64` | Bytes read from MTA connections since `run()` started, counted as they arrive |
| `bytes_sent` | `u64` | Bytes written to MTA connections, counted as they are written |
| `connections_closed` | `Vec<(&'static str, Option<&'static str>, u64)>` | Closed connections by `milter.close_reason`, and for the reason `error` by `error.type` |
| `negotiations` | `Vec<(u32, u64)>` | Completed option negotiations, by negotiated protocol version |
| `commands` | `Vec<(&'static str, u64)>` | Commands received, by `milter.command` |
| `replies` | `Vec<(&'static str, &'static str, u64)>` | Replies sent, by `milter.command` and `milter.reply` |
| `reloads` | `Vec<(&'static str, &'static str, u64)>` | Reload requests, by `milter.reload_result` and `milter.reload_source` |
| `last_reload` | `Option<SystemTime>` | When the last reload was applied; `None` before the first |

The lists hold the counts that are not zero, with the attribute values of [telemetry.md](telemetry.md#attribute-values), in the order of its tables. Each count is the value of a metric in [telemetry.md](telemetry.md#status-counters), so `status()` gives an application without OpenTelemetry the same numbers ([ADR 0004](../adr/0004-metrics-come-from-opentelemetry-and-status.md)).

`ListenerStatus`, also `#[non_exhaustive]`:

| Field | Type | Description |
|-------|------|-------------|
| `address` | `SocketAddr` | The listener's address |
| `bound` | `bool` | The address was bound |
| `accepting` | `bool` | The listener is accepting connections |
| `connections_accepted` | `u64` | Connections this listener accepted, rejected ones included |
| `connections_active` | `usize` | Open connections this listener accepted |
| `connections_rejected` | `u64` | Connections this listener closed at once at a limit |

A listener's counts continue when a reload keeps its address, and start at zero for an address the server had no listener on.

## Handler trait

`Handler` is the trait the user implements. It follows libmilter's callback model: the library runs the protocol, and the handler answers one callback per event with a decision. A handler type must be `Send + Sync + 'static`. One handler value serves all connections, so its fields hold only data shared by all sessions, such as a `ConfigWatch`.

### Associated types

| Type | Bounds | Lifetime |
|------|--------|----------|
| `Session` | `Default + Send + 'static` | Created by the library with `Default` when an SMTP session starts (`SMFIC_CONNECT`), dropped when it ends (`SMFIC_QUIT`, `SMFIC_QUIT_NC`, or connection close) |
| `Message` | `Default + Send + 'static` | Created by the library with `Default` at `SMFIC_MAIL`, or at the first command of a message when the handler left `MAIL` out of `events()`; dropped after the final reply at end of message, after a decision that ends the message earlier (`Accept` from any message callback, or `Reject`, `TempFail`, `Reply`, or `Discard` from one other than `rcpt`, which reset the protocol state to `ST_HELO` as in libmilter), on `SMFIC_ABORT`, or when the session ends |

All per-session and per-message data belongs in these types. The library drops them at the end of each session and message, which enforces the session isolation rule.

### Negotiation methods

Called once per connection, during option negotiation. Both are synchronous and have defaults.

| Method | Returns | Default | libmilter equivalent |
|--------|---------|---------|----------------------|
| `events(&self)` | `Events` | all stages | the set of registered `xxfi_*` callbacks |
| `actions(&self)` | `Actions` | none | `xxfi_flags` |

The library asks the MTA not to send the stages missing from `events()` (`SMFIP_NO*` flags), and requests the actions in `actions()` (`SMFIF_*` flags). If the MTA does not offer every requested action, the library closes the connection, as libmilter does.

`Events` has one flag per optional stage. `end_of_message`, `abort`, and `session_end` are always delivered.

| Flag | Callback | Protocol flag when left out |
|------|----------|-----------------------------|
| `CONNECT` | `connect` | `SMFIP_NOCONNECT` |
| `HELO` | `helo` | `SMFIP_NOHELO` |
| `MAIL` | `mail` | `SMFIP_NOMAIL` |
| `RCPT` | `rcpt` | `SMFIP_NORCPT` |
| `DATA` | `data` | `SMFIP_NODATA` |
| `HEADER` | `header` | `SMFIP_NOHDRS` |
| `END_OF_HEADERS` | `end_of_headers` | `SMFIP_NOEOH` |
| `BODY` | `body` | `SMFIP_NOBODY` |
| `UNKNOWN` | `unknown` | `SMFIP_NOUNKNOWN` |

### Callbacks

Every callback returns a `Send` future that resolves to `Result<T>`, where `T` is the type in the Returns column (`()` for "nothing"). Each has a default that returns `Ok` with `Continue`, or does nothing. Implementations can write them as `async fn`.

| Callback | libmilter equivalent | Receives | Returns |
|----------|----------------------|----------|---------|
| `connect` | `xxfi_connect` | `&mut SessionContext<Self>`, `&ConnectInfo` | `ConnectDecision` |
| `helo` | `xxfi_helo` | `&mut SessionContext<Self>`, HELO name (`&MilterText`) | `ConnectDecision` |
| `mail` | `xxfi_envfrom` | `&mut MessageContext<Self>`, sender (`&MilterText`), ESMTP arguments (`&[MilterText]`) | `Decision` |
| `rcpt` | `xxfi_envrcpt` | `&mut MessageContext<Self>`, recipient (`&MilterText`), ESMTP arguments (`&[MilterText]`) | `Decision`; a reject applies to this recipient only |
| `data` | `xxfi_data` | `&mut MessageContext<Self>` | `Decision` |
| `header` | `xxfi_header` | `&mut MessageContext<Self>`, name and value (`&MilterText`) | `Decision` |
| `end_of_headers` | `xxfi_eoh` | `&mut MessageContext<Self>` | `Decision` |
| `body` | `xxfi_body` | `&mut MessageContext<Self>`, chunk (`Bytes`) | `BodyDecision` |
| `end_of_message` | `xxfi_eom` | `&mut EomContext<Self>` | `Decision` |
| `abort` | `xxfi_abort` | `&mut MessageContext<Self>` | nothing; the message state is dropped afterwards |
| `unknown` | `xxfi_unknown` | `&mut SessionContext<Self>`, the SMTP command (`&MilterText`) | `Decision` |
| `session_end` | `xxfi_close` | `&mut SessionContext<Self>`, `SessionEnd` | nothing; the session state is dropped afterwards |

The library calls a callback only for the stages in `events()`, and only if the negotiated protocol version has the stage: `unknown` needs version 3 or later, `data` version 4 or later.

Contract:
- Callbacks for one connection run one at a time, in protocol order. Callbacks for different connections run concurrently.
- A callback that returns an error closes the connection; the library calls `session_end` with `SessionEnd::Error` first. A handler turns its own error into `Error::Handler` with `Error::handler(err)`, which accepts any `std::error::Error + Send + Sync + 'static`.
- A callback should finish promptly when the shutdown token from its context is cancelled.
- Settings come from the `ConfigWatch<A>` the handler holds. Read the application configuration once per session (in `connect`) or once per message (in `mail`), and keep the value in `Session` or `Message`.

## Contexts

### SessionContext

| Method | Returns |
|--------|---------|
| `session()` | `&mut H::Session` |
| `macro_value(name)` | `Option<&MilterText>`: the value of a macro the MTA sent in this session, at this stage or an earlier one |
| `connection()` | `&ConnectionInfo` of the MTA connection |
| `negotiated()` | `&Negotiated` |
| `shutdown()` | the server's `CancellationToken`, cancelled when the shutdown grace period ends |

### MessageContext

Everything in `SessionContext`, plus:

| Method | Returns |
|--------|---------|
| `message()` | `&mut H::Message` |

### EomContext

Everything in `MessageContext`, plus the modification methods. They exist only on this type, so modifications are possible only at end of message, as libmilter's `mi_sendok` requires. Each is async and returns `Result<()>`; it fails with `Error::ActionNotNegotiated` if its action was not negotiated.

| Method | Action required | Sends |
|--------|-----------------|-------|
| `add_header(name, value)` | `ADD_HEADERS` | `SMFIR_ADDHEADER` |
| `insert_header(index, name, value)` | `ADD_HEADERS` | `SMFIR_INSHEADER` |
| `change_header(name, index, value)` | `CHANGE_HEADERS` | `SMFIR_CHGHEADER` |
| `delete_header(name, index)` | `CHANGE_HEADERS` | `SMFIR_CHGHEADER` with an empty value |
| `add_recipient(recipient)` | `ADD_RECIPIENTS` | `SMFIR_ADDRCPT` |
| `add_recipient_with_args(recipient, args)` | `ADD_RECIPIENTS_WITH_ARGS` | `SMFIR_ADDRCPT_PAR` |
| `delete_recipient(recipient)` | `DELETE_RECIPIENTS` | `SMFIR_DELRCPT` |
| `change_from(sender, args)` | `CHANGE_FROM` | `SMFIR_CHGFROM` |
| `replace_body(bytes)` | `CHANGE_BODY` | `SMFIR_REPLBODY`, split into chunks of at most 65,535 bytes |
| `quarantine(reason)` | `QUARANTINE` | `SMFIR_QUARANTINE` |
| `progress()` | none | `SMFIR_PROGRESS` |

`Actions` has one flag for each action in the table, matching the `SMFIF_*` flags in [milter-protocol.md](milter-protocol.md#action-flags-smfif_).

## Decisions and reply codes

| Type | Variants | Used by |
|------|----------|---------|
| `ConnectDecision` | `Continue`, `Accept`, `Reject`, `TempFail`, `Reply(ReplyCode)` | `connect`, `helo` |
| `Decision` | the `ConnectDecision` variants, plus `Discard` | the message callbacks and `unknown` |
| `BodyDecision` | the `Decision` variants, plus `Skip` | `body` |

The variants mean what libmilter's `SMFIS_*` codes mean:
- `Continue`: go on to the next stage.
- `Accept`: accept the message; at `connect` or `helo`, accept the whole connection. The MTA then sends no more events for what was accepted: Sendmail and Postfix end the milter session after a connect-level or HELO-level accept.
- `Reject`, `TempFail`: reply 5xx or 4xx.
- `Discard`: accept the message from the client and drop it. Not available at `connect` or `helo`, where Postfix does not allow it.
- `Reply(ReplyCode)`: a custom reply; a 4xx code acts as `TempFail`, a 5xx code as `Reject`.
- `Skip`: receive no more body chunks and go to end of message. If the MTA did not offer `SMFIP_SKIP`, the library sends `Continue` and stops calling `body` for this message.

`ReplyCode` is built with `ReplyCode::new(code, enhanced_code, text)` or `ReplyCode::multi_line(code, enhanced_code, lines)`. `code` is a `u16` and `enhanced_code` an `Option<&str>`. Both return `Result<ReplyCode, ReplyError>` and check, as libmilter's `smfi_setreply` and `smfi_setmlreply` do:
- the code is 4xx or 5xx
- the enhanced status code, if given, has the form `d.ddd.ddd` and the same first digit as the code
- without an enhanced status code, no line starts with a digit other than the code's first digit, which Postfix would reject as a mismatched enhanced status code
- no line contains CR, LF, or NUL, and no line is longer than 980 bytes with each `%` written as `%%`, the text libmilter's limit applies to
- a multi-line reply has at most 32 lines

The text is taken literally. The library writes each `%` as `%%` on the wire, so a reply shows on both MTAs exactly as written. `Reply::from(reply_code)` gives the `Reply::ReplyCode` that carries it.

## Protocol engine behavior

The library performs these protocol duties for every handler:

| Duty | Behavior |
|------|----------|
| Version negotiation | Closes the connection if the MTA offers a version below 2; otherwise answers with the lower of the MTA's version and 6 |
| Flags | Treats zero action or protocol flags from the MTA as the version 1 defaults; requests only flags the MTA offered |
| Stage filtering | Requests `SMFIP_NO*` for the stages missing from `events()` |
| Macros | Stores the macros of the stage bytes `C`, `H`, `M`, `R`, `T`, `L`, `N`, `B`, `E`, and `U`, including `L`, `B`, and `U` from Postfix, and drops those of any other byte, which no lookup reaches; never replies to `SMFIC_MACRO`; clears them when the session ends. Macros sent again for a stage replace that stage's earlier ones. `macro_value` looks in the current command's stage first, then in the earlier stages in protocol order (`C`, `H`, `M`, `R`, `T`, `L`, `N`, `B`, `E`). In `unknown`, `abort`, and `session_end`, it looks in every stage, the latest first, starting with `U` |
| Session start | A session starts with `SMFIC_CONNECT`. When the MTA leaves `SMFIC_CONNECT` out, the session starts with the first command after option negotiation, or after `SMFIC_QUIT_NC`, that is not `SMFIC_MACRO`, `SMFIC_ABORT`, or `SMFIC_QUIT` |
| State machine | Tracks the protocol state with libmilter's transition rules; closes the connection on `SMFIC_QUIT_NC` outside `ST_ENDM` and `ST_UNKN` ([ADR 0001](../adr/0001-commands-in-wrong-state-recover-like-libmilter-or-close.md)) |
| Commands in a wrong state | Recovers like libmilter for a command that fits from `ST_HELO` (`SMFIC_HELO`, `SMFIC_MAIL`, `SMFIC_UNKNOWN`, and later stages the handler switched off): calls `abort` if a message is open, then processes the command. Ignores `SMFIC_ABORT` and `SMFIC_QUIT`. Closes the connection with a protocol error on any other command ([ADR 0001](../adr/0001-commands-in-wrong-state-recover-like-libmilter-or-close.md)) |
| Replies | Encodes each decision as the matching `SMFIR_*` reply; sends no reply to `SMFIC_MACRO`, `SMFIC_ABORT`, `SMFIC_QUIT`, or `SMFIC_QUIT_NC` |
| Abort | Calls `abort` and drops the message state when a message is open; ignores `SMFIC_ABORT` outside a message. A message that ends without its final reply or a decision that ends it also gets `abort`: on recovery from a wrong state, on an `SMFIC_MAIL` that starts a new message while one is open, and before `session_end` when the session ends with a message open |
| Session end | Calls `session_end` and drops the session state on `SMFIC_QUIT` (`Quit`), on `SMFIC_QUIT_NC` (`QuitNewConnection`), on end of file between frames without `SMFIC_QUIT` (`ConnectionClosed`), before closing the connection for an error, a reset, an end of file in the middle of a frame, or a timeout (`Error`), and when shutdown closes the connection (`ServerShutdown`). A forced shutdown closes the connection without calling `abort` or `session_end`. A connection that closes before a session starts, or between `SMFIC_QUIT_NC` and the next session, calls no callback |

`SessionEnd`: `Quit`, `QuitNewConnection`, `ConnectionClosed`, `Error`, `ServerShutdown`.

`ConnectInfo`:

| Field | Type | Description |
|-------|------|-------------|
| `hostname` | `MilterText` | The SMTP client's hostname as the MTA sent it; `[<ip>]` when it did not resolve |
| `address` | `Option<SocketAddr>` | The SMTP client's address; `None` for families `SMFIA_UNKNOWN` and `SMFIA_UNIX`, and when the address text is not an IP address. An `IPv6:` prefix, as Sendmail can write it, is removed |

## Text from the MTA

`MilterText` holds text exactly as the MTA sent it. Envelope addresses, ESMTP arguments, HELO names, header names and values, macro values, and unknown commands can contain bytes that are not valid UTF-8, from SMTPUTF8 clients or legacy 8-bit mail, so the library never converts them.

| Method | Returns |
|--------|---------|
| `as_bytes()` | `&[u8]`: the raw bytes, without the terminating NUL |
| `as_str()` | `Option<&str>`: the text when it is valid UTF-8 |
| `to_string_lossy()` | `Cow<str>`: the text with invalid bytes replaced by U+FFFD, for logging and display |

`MilterText` shares the frame's buffer (`bytes::Bytes`), so creating one copies nothing.

## Connection information

`ConnectionInfo` describes the MTA connection, not the SMTP client:

| Field | Type | Description |
|-------|------|-------------|
| `id` | `u64` | Connection ID, unique within the server's lifetime |
| `peer_addr` | `SocketAddr` | The MTA's address |
| `local_addr` | `SocketAddr` | The listener address that accepted the connection |
| `accepted_at` | `Instant` | Time `accept()` returned |
| `config_generation` | `u64` | Generation of the configuration snapshot |

The connection itself, with its socket, buffers, and timeouts, is internal. It enforces `timeouts.connect`, `timeouts.read`, `timeouts.write`, `timeouts.idle`, and `per_connection_buffer` from its configuration snapshot ([tcp-server.md](tcp-server.md)).


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
| `OptNeg { version, actions, protocol }` | `SMFIC_OPTNEG`; three `u32` values as sent, zeros included |
| `Macro { stage, macros }` | `SMFIC_MACRO`; `stage` is the raw stage byte, including `L`, `B`, and `U` from Postfix, and `macros` the name and value pairs in order |
| `Connect { hostname, family, port, address }` | `SMFIC_CONNECT`; `family` is an `AddressFamily`, and `port` (`u16`) and `address` are `None` for `AddressFamily::Unknown` |
| `Helo(MilterText)` | `SMFIC_HELO` |
| `Mail(Vec<MilterText>)` | `SMFIC_MAIL` |
| `Rcpt(Vec<MilterText>)` | `SMFIC_RCPT` |
| `Data` | `SMFIC_DATA` |
| `Header { name, value }` | `SMFIC_HEADER` |
| `Eoh` | `SMFIC_EOH` |
| `Body(Bytes)` | `SMFIC_BODY` |
| `BodyEob(Bytes)` | `SMFIC_BODYEOB`, with optional trailing body data |
| `Abort` | `SMFIC_ABORT` |
| `Unknown(MilterText)` | `SMFIC_UNKNOWN` |
| `Quit` | `SMFIC_QUIT` |
| `QuitNewConnection` | `SMFIC_QUIT_NC` |

`AddressFamily`: `Unknown` (`SMFIA_UNKNOWN`), `Unix` (`SMFIA_UNIX`), `Inet` (`SMFIA_INET`), `Inet6` (`SMFIA_INET6`).

`Command::from_frame(frame)` checks the command code and the data format, and returns `Error::Protocol` on invalid input: `UnknownCommand` for an unknown command code, and `Malformed` for data that does not match the format, such as a missing null terminator, bytes after the last field, a short option negotiation, a macro name without a value, or a family byte that is not an `SMFIA_*` code. An option negotiation longer than 12 bytes is read from its first 12, as libmilter does. Text fields, including the hostname, header name and value, and macro names and values, are `MilterText`, so bytes that are not valid UTF-8 are kept.

## Reply

Type-safe form of the replies the milter sends.

| Variant | Reply |
|---------|-------|
| `OptNeg { version, actions, protocol, macros }` | `SMFIC_OPTNEG` response; `macros` is the optional symbol list, a `Vec` of `SMFIM_*` stage and space-separated macro names |
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
| `InsertHeader { index, name, value }` | `SMFIR_INSHEADER`; `index` is a `u32` |
| `ChangeHeader { index, name, value }` | `SMFIR_CHGHEADER`; an empty value deletes the header |
| `AddRecipient(String)` | `SMFIR_ADDRCPT` |
| `AddRecipientWithArgs { rcpt, args }` | `SMFIR_ADDRCPT_PAR`; `args` is an `Option<String>` |
| `DeleteRecipient(String)` | `SMFIR_DELRCPT` |
| `ChangeFrom { from, args }` | `SMFIR_CHGFROM`; `args` is an `Option<String>` |
| `ReplaceBody(Bytes)` | `SMFIR_REPLBODY` |

The enum has no variants for `SMFIR_SHUTDOWN`, `SMFIR_CONN_FAIL`, or `SMFIR_SETSYMLIST`: Sendmail rejects the first, neither MTA accepts the second, and the third is never sent as a packet. The macro list travels in `Reply::OptNeg`.

Text fields are `String`: the milter writes them, so they are always valid UTF-8.

`Reply` is the wire-level type, used by the milter client sample in `examples/` and by tests; handlers use the decision types and `ReplyCode` instead. `Reply::to_frame()` validates before encoding, and returns `Error::InvalidReply` on a failed check:
- `ReplyCode` text: a 4xx or 5xx code, the multi-line rules from [milter-protocol.md](milter-protocol.md#multi-line-reply-handling), and every literal `%` already written as `%%`
- header names in `AddHeader` and `InsertHeader`: not empty, printable ASCII (33 to 126), and no colon, as RFC 5322 defines a field name. `ChangeHeader` names a header the MTA delivered, which can hold non-ASCII bytes, so its name only must not be empty or hold a colon, whitespace, or control characters
- header values: no CR, and LF only to fold the value, followed by a space or tab and more than whitespace; any other line break would start another header or end the header block
- addresses, ESMTP arguments, and the quarantine reason: no CR or LF, and addresses and the reason not empty, as libmilter's `smfi_addrcpt`, `smfi_delrcpt`, `smfi_chgfrom`, and `smfi_quarantine` require
- text fields: no NUL byte, which would end the field early on the wire
- `ReplaceBody` data: at most 65,535 bytes per frame

Thread safety: `Send + Sync`.

## Configuration and reload API

All settings, defaults, valid ranges, and reload behavior are in [configuration.md](configuration.md). The reload procedure is in [hot-reload.md](hot-reload.md).

Checks on `ServerConfig`:

| Method | Returns |
|--------|---------|
| `ServerConfig::validate(&self)` | `Result<(), ConfigError>`: the [validation rules](configuration.md#validation-rules) that `build()` and every reload also run |
| `ServerConfig::parse_bind_address(text)` | `Result<SocketAddr, AddressError>`: `host:port`, `[v6]:port`, `inet:port@host`, `inet6:port@host`, `inet:port`, or `inet6:port`. The host is an IP address, and an IPv6 host may carry a numeric `%scope`; without a host, the address is every address of the family |

Environment variable helpers on `ServerConfig` ([configuration.md](configuration.md#environment-variables)):

| Method | Returns |
|--------|---------|
| `ServerConfig::from_env(prefix)` | `Result<ServerConfig, ConfigError>` |
| `ServerConfig::apply_env(&mut self, prefix)` | `Result<(), ConfigError>` |
| `ServerConfig::from_vars(prefix, vars)` | `Result<ServerConfig, ConfigError>`; `vars` is any iterator of name and value pairs |
| `ServerConfig::apply_vars(&mut self, prefix, vars)` | `Result<(), ConfigError>` |

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

`ReloadLoader<A>`, a trait that requires `Send + Sync + 'static`:

| Item | Description |
|------|-------------|
| `type Error` | Why a load failed. It implements `std::error::Error` and is `Send + Sync + 'static`. The reload then fails with `ReloadError::Load`, which keeps the error as its source |
| `load(&self)` | Reads the new configurations: an `async fn` that returns `Result<Reloaded<A>, Self::Error>`, whose future is `Send` |

An example loader is in [Reload the configuration without a restart](../how-to/reload-the-configuration.md#write-a-loader).

`ReloadReport`:

| Field | Type | Description |
|-------|------|-------------|
| `generation` | `u64` | Generation after the reload |
| `changed` | `Vec<&'static str>` | Names of changed settings |
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
| `ActionNotNegotiated(Actions)` | A modification method was called without its action negotiated | the method fails; the connection stays open |
| `Handler(Box<dyn std::error::Error + Send + Sync>)` | A callback returned an error | connection closed |
| `Timeout(TimeoutKind)` | `Connect`, `Read`, `Write`, or `Idle` timeout expired | connection closed |
| `ConnectionClosed` | The MTA closed the connection in the middle of a frame | connection closed |
| `ConnectionReset` | The MTA reset the connection | connection closed |
| `FrameTooLarge { length, max }` | Packet longer than `per_connection_buffer` | connection closed |
| `InvalidReply(ReplyError)` | `Reply::to_frame()` refused a reply that breaks a protocol or MTA rule | the reply is not sent |
| `Config(ConfigError)` | Invalid configuration at build time | `build()` fails |
| `Bind { addr, source }` | A listener could not bind | `run()` fails in `FailFast` mode |
| `Signals(std::io::Error)` | `run()` could not register the handlers for `SIGTERM`, `SIGINT`, `SIGQUIT`, and `SIGHUP` | `run()` fails |
| `ResourceLimit(ResourceKind)` | `MaxConnections`, `MaxFds`, or `MaxMemory` reached | connection rejected or closed |
| `ShuttingDown` | The operation is not allowed during shutdown | none |

### ConfigError

| Variant | Meaning |
|---------|---------|
| `OutOfRange { field, value, allowed }` | A value outside its range in [configuration.md](configuration.md); `field` is the setting's path, such as `timeouts.connect`, `value` the value as text, and `allowed` the range |
| `MissingBindAddress` | `bind_addresses` is empty |
| `DuplicateBindAddress(SocketAddr)` | The same address appears twice |
| `Address(AddressError)` | An address string could not be parsed |
| `Inconsistent { rule, detail }` | A cross-field rule failed, such as `max_fds` too small for `max_connections` |
| `Unsupported { field, reason }` | The platform does not support a setting, such as `reuse_port` |
| `UnknownEnvVariable(String)` | An environment variable with the configured prefix names no setting |
| `InvalidEnvValue { variable, reason }` | An environment variable's value cannot be parsed; the value itself is not included |

`AddressError`: `InvalidFormat` (including `unix:` and `local:` addresses and Postfix's `inet:host:port`), `InvalidHost` (not an IP address of the form's family, or a scope ID that is not a number), `InvalidPort` (not 1 to 65535).

### ReloadError

| Variant | Meaning |
|---------|---------|
| `NoLoader` | `reload()` was called without a registered loader |
| `Load(Box<dyn std::error::Error + Send + Sync>)` | The application's loader failed; its error is kept as the source |
| `Validation(ConfigError)` | The new configuration is invalid |
| `RestartRequired { settings }` | The new configuration changes settings that require a restart; `settings` (`Vec<&'static str>`) names them, in the order of [configuration.md](configuration.md#serverconfig-reference), and the message lists them. Nothing was applied |
| `Bind { addr, source }` | A new listener failed to bind in `FailFast` mode; the reload was rolled back |
| `NoListener` | The new configuration would leave no listener |
| `ShuttingDown` | Shutdown has started |

### ProtocolError

| Variant | Meaning |
|---------|---------|
| `ZeroLength` | A packet announced a length of 0 |
| `Malformed { command }` | A command's data does not match its format, such as a missing null terminator or a short option negotiation |
| `UnknownCommand(u8)` | An unknown command code |
| `WrongState { command }` | A command in a state where it closes the connection ([ADR 0001](../adr/0001-commands-in-wrong-state-recover-like-libmilter-or-close.md)) |
| `VersionTooOld(u32)` | The MTA offered a protocol version below 2 |
| `ActionsNotOffered(Actions)` | The MTA did not offer every action in `Handler::actions()` |

### ReplyError

| Variant | Meaning |
|---------|---------|
| `InvalidCode(u16)` | The reply code is not 4xx or 5xx |
| `InvalidEnhancedCode` | The enhanced status code is not of the form `d.ddd.ddd` |
| `EnhancedCodeMismatch` | The enhanced status code, or the text of a line without one, starts with a digit other than the reply code's first digit |
| `LineBreak` | A line of reply text, an address, ESMTP arguments, or a quarantine reason contains CR or LF |
| `LineTooLong { length }` | A line of reply text is longer than 980 bytes with each `%` written as `%%` |
| `LineCount(usize)` | A reply has no lines, or more than 32 |
| `MalformedReplyText` | `Reply::ReplyCode` text breaks the line rules: code, separator, or the same code on every line |
| `UnescapedPercent` | `Reply::ReplyCode` text contains a `%` not written as `%%` |
| `NulByte` | A text field contains a NUL byte |
| `InvalidHeaderName` | A header name is empty or contains a colon; a new header's name is also not printable ASCII, or a delivered header's name contains whitespace or a control character |
| `HeaderLineBreak` | A header value contains CR, or an LF that does not start a folded line of a space or tab and more than whitespace |
| `EmptyField` | An address or quarantine reason is empty |
| `BodyChunkTooLarge { length }` | A `ReplaceBody` chunk is longer than 65,535 bytes |

`ReloadError::Load` and `Error::Handler` box errors whose types belong to the application. They are the only boxed errors in the public API.

A failed reload leaves the server running with the previous generation.

## Async patterns

Async:
- every `Handler` callback
- the `EomContext` modification methods
- `Server::run()`
- `ServerHandle::wait()`, `reload()`, `reload_with_config()`
- `ConfigWatch::changed()`

Sync:
- `Handler::events()`, `Handler::actions()`
- `Frame::parse()`, `Command::from_frame()`, `Reply::to_frame()`, `ReplyCode::new()`
- `ServerConfig` construction and `ServerBuilder` setters
- `ConfigWatch::current()`, `ServerHandle::status()`, `config()`

Cancellation: the library stops waiting for the MTA's next command when the shutdown token is cancelled. A callback that does long work should watch `shutdown()` from its context and return early when it is cancelled.

## Thread safety

| Type | Send | Sync |
|------|------|------|
| `Server<H, A>` | yes | yes |
| `ServerHandle<A>` | yes | yes |
| `ConfigWatch<A>` | yes | yes |
| `ConnectionInfo` | yes | yes |
| `SessionContext`, `MessageContext`, `EomContext` | yes | no (exclusive to one connection) |
| `Frame`, `Command`, `Reply`, `ReplyCode`, `MilterText` | yes | yes |
| `Error` | yes | yes |

`A` must be `Send + Sync + 'static`. `H` must be `Send + Sync + 'static`; `H::Session` and `H::Message` must be `Default + Send + 'static`.

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

Zero copy: frame data, body chunks, and `MilterText` values share `bytes::Bytes` buffers. Text is copied only when a handler converts it, for example with `to_string_lossy()`.

Allocation:
- per connection: one read buffer and one write buffer, growing up to `per_connection_buffer`
- shared: the published configuration (`Arc`), the status counters

Latency targets per frame: P50 < 100 µs, P99 < 1 ms, P99.9 < 10 ms.

Reading the configuration costs one `Arc` clone at accept. Handlers reading `ConfigWatch::current()` take a short read lock with no contention from other readers. Each SMTP session allocates one `Session` value, and each message one `Message` value.

## Usage constraints

Runtime: Tokio, multi-threaded. The application creates the runtime. Other runtimes are not supported.

Toolchain and platforms:
- Rust 1.85 or later, edition 2024; the crate declares `rust-version`
- Linux, macOS, or BSD
- file descriptors: at least `max_connections` + listeners + 10

Recommended for production: 4 or more CPU cores; memory for up to two buffers of `per_connection_buffer` per connection (about 2.5 GB for 10,000 connections at the default 128 KB), or `max_memory` set to cap the total; and `ulimit -n` of at least `max_connections` plus the listeners plus 10 (10,011 for the defaults with one listener).

## Possible future additions

- TLS support
- authentication
- a low-level frame API for applications that need raw protocol access; this needs a new ADR ([ADR 0002](../adr/0002-library-runs-the-protocol-handlers-decide.md))

New traits will come with default method implementations where possible, and optional functionality will sit behind Cargo features.
