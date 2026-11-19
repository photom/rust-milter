//! An asynchronous Milter protocol server library for Sendmail and Postfix.
//!
//! rust-milter runs the Milter protocol with the MTA and calls a [`Handler`]
//! once per protocol event, following libmilter's callback model: the library
//! runs the protocol, and the handler decides ([ADR 0002]). An application
//! implements [`Handler`], builds a [`Server`] with [`Server::builder`], and
//! runs it on a Tokio runtime. A [`ServerHandle`] controls the running server.
//!
//! The [tutorial] builds a first milter, and the [how-to guides] cover the
//! tasks that follow, from reloading the configuration to exporting metrics.
//! [Library architecture] explains the design. The settings are specified in
//! [configuration.md], the telemetry in [telemetry.md], and the wire protocol
//! in [milter-protocol.md]. The milter client sample of [milter-client.md] is
//! not part of this API.
//!
//! # Public API
//!
//! | Area | Items |
//! |------|-------|
//! | Server | [`Server`], [`ServerBuilder`], [`ServerHandle`], [`HandlerContext`], [`Status`], [`ListenerStatus`] |
//! | Configuration | [`ServerConfig`] and its parts, [`ConfigWatch`], [`ReloadLoader`], [`Reloaded`], [`ReloadReport`] |
//! | Handler | [`Handler`], its contexts [`SessionContext`], [`MessageContext`], and [`EomContext`], and [`ConnectInfo`], [`ConnectionInfo`], [`MilterText`], [`Negotiated`], [`SessionEnd`] |
//! | Negotiation | [`Events`], [`Actions`], [`ProtocolOptions`] |
//! | Decisions | [`ConnectDecision`], [`Decision`], [`BodyDecision`], [`ReplyCode`] |
//! | Protocol | [`Frame`], [`Command`], [`AddressFamily`], [`Reply`], and the constants in [`protocol`] |
//! | Errors | [`Error`], [`Result`], [`ConfigError`], [`ReloadError`], [`ProtocolError`], [`ReplyError`], [`AddressError`], [`TimeoutKind`], [`ResourceKind`] |
//!
//! [`Frame`], [`Command`], and [`Reply`] are wire-level types, for tests and
//! for the planned milter client sample. Handlers use the decision types and
//! [`ReplyCode`] instead.
//!
//! The crate re-exports [`CancellationToken`] from tokio-util, which
//! coordinates graceful shutdown, and [`Bytes`] and [`BytesMut`] from bytes,
//! which carry frame data without copying it.
//!
//! # Cargo features
//!
//! | Feature | Default | Adds |
//! |---------|---------|------|
//! | `otel` | off | OpenTelemetry metric instruments and `ServerBuilder::meter_provider` ([telemetry.md]) |
//! | `serde` | off | `Serialize` and `Deserialize` for [`ServerConfig`] and its parts ([configuration.md: File format]) |
//!
//! # Async and cancellation
//!
//! rust-milter runs on Tokio, in a runtime the application creates. Both the
//! multi-threaded and the current-thread runtime work. Use the multi-threaded
//! one in production, so that connections run on every core. Other async
//! runtimes are not supported.
//!
//! The library stops waiting for the MTA's next command when the shutdown
//! token is cancelled. A callback that does long work should watch
//! [`SessionContext::shutdown`] and return early once the token is cancelled.
//!
//! # Thread safety
//!
//! The application configuration type `A` must be `Send + Sync + 'static`. A
//! handler type must be `Send + Sync + 'static`, and its
//! [`Session`](Handler::Session) and [`Message`](Handler::Message) types
//! `Default + Send + 'static`. Each type's page lists its auto traits, such as
//! `Send` and `Sync`.
//!
//! # Versioning and stability
//!
//! rust-milter follows semantic versioning: a breaking change bumps MAJOR, a
//! new feature MINOR, and a fix PATCH. Before 1.0, any API can change. From
//! 1.0, the public types, traits, method signatures, error variants, and
//! configuration fields are stable.
//!
//! The error enums and the configuration enums ([`BindMode`],
//! [`TraceCommands`]) are `#[non_exhaustive]`, so adding a variant is not a
//! breaking change. The configuration structs keep public fields and are not
//! `#[non_exhaustive]`, because that attribute would forbid building them with
//! `..Default::default()` outside the crate. Adding a configuration field is a
//! MINOR change; code that names every field without `..Default::default()`
//! has to add the new one.
//!
//! To deprecate an item:
//! 1. Mark the item with `#[deprecated]`.
//! 2. Provide a migration path.
//! 3. Remove it in the next MAJOR version.
//! 4. Record it in the CHANGELOG.
//!
//! # Performance characteristics
//!
//! Frame data, body chunks, and [`MilterText`] values share [`Bytes`] buffers.
//! Text is copied only when a handler converts it, for example with
//! [`MilterText::to_string_lossy`].
//!
//! Each connection holds:
//! - one read buffer of 4 KB, which grows to fit a larger frame and goes back
//!   to 4 KB after it
//! - the command being processed, that is its frame and the strings decoded
//!   from it, which costs at most `per_connection_buffer` and stays counted
//!   until the command finishes
//! - the stored macros, which cost at most `per_connection_buffer`
//! - one write buffer, which holds one reply at a time. The largest reply the
//!   library builds is a 65,540-byte body chunk; a header or address that a
//!   handler sends can make a reply longer
//!
//! All but the write buffer count against `max_memory`. The connections share
//! the published configuration (`Arc`) and the status counters.
//!
//! Latency targets per frame: P50 below 100 µs, P99 below 1 ms, P99.9 below
//! 10 ms.
//!
//! Reading the configuration costs one `Arc` clone at accept. A handler that
//! reads [`ConfigWatch::current`] takes a short read lock with no contention
//! from other readers. Each SMTP session allocates one `Session` value, and
//! each message one `Message` value.
//!
//! # Usage constraints
//!
//! - Rust 1.85 or later, edition 2024; the crate declares `rust-version`
//! - Linux, macOS, or BSD
//! - file descriptors: at least `max_connections` + listeners + 10
//!
//! Recommended for production: 4 or more CPU cores; memory for one
//! `per_connection_buffer` and one 65,540-byte reply per connection (about
//! 2 GB for 10,000 connections at the default 128 KB), or `max_memory` set to
//! cap the read buffers, the commands being processed, and the stored macros;
//! and `ulimit -n` of at least `max_connections` plus the listeners plus 10
//! (10,011 for the defaults with one listener). The steps are in
//! [Deploy a milter in production].
//!
//! # Possible future additions
//!
//! - TLS support
//! - authentication
//! - a low-level frame API for applications that need raw protocol access;
//!   this needs a new ADR ([ADR 0002])
//! - a way for a handler to request the macros it needs for each stage, as
//!   libmilter's `smfi_setsymlist` does. The engine sends no macro list in its
//!   `SMFIC_OPTNEG` response, so the MTA sends the macros its own
//!   configuration lists ([milter-protocol.md: Symbol list]). The request
//!   would be one more negotiation method next to
//!   [`Handler::protocol_options`] ([ADR 0020])
//!
//! New traits will come with default method implementations where possible,
//! and optional functionality will sit behind Cargo features.
//!
//! [ADR 0002]: https://github.com/photom/rust-milter/blob/main/docs/adr/0002-library-runs-the-protocol-handlers-decide.md
//! [ADR 0020]: https://github.com/photom/rust-milter/blob/main/docs/adr/0020-protocol-options-are-a-fixed-handler-method-and-close-when-missing.md
//! [tutorial]: https://github.com/photom/rust-milter/blob/main/docs/tutorials/getting-started.md
//! [how-to guides]: https://github.com/photom/rust-milter/tree/main/docs/how-to
//! [Library architecture]: https://github.com/photom/rust-milter/blob/main/docs/explanation/architecture.md
//! [configuration.md]: https://github.com/photom/rust-milter/blob/main/docs/reference/configuration.md
//! [configuration.md: File format]: https://github.com/photom/rust-milter/blob/main/docs/reference/configuration.md#file-format-serde-feature
//! [telemetry.md]: https://github.com/photom/rust-milter/blob/main/docs/reference/telemetry.md
//! [milter-protocol.md]: https://github.com/photom/rust-milter/blob/main/docs/reference/milter-protocol.md
//! [milter-protocol.md: Symbol list]: https://github.com/photom/rust-milter/blob/main/docs/reference/milter-protocol.md#symbol-list-in-option-negotiation-optional
//! [milter-client.md]: https://github.com/photom/rust-milter/blob/main/docs/reference/milter-client.md
//! [Deploy a milter in production]: https://github.com/photom/rust-milter/blob/main/docs/how-to/deploy-a-milter-in-production.md

mod config;
mod engine;
mod error;
mod frame;
mod handler;
mod message;
pub mod protocol;
mod server;

pub use bytes::{Bytes, BytesMut};
pub use config::{
    BindMode, KeepaliveConfig, ResourceLimitsConfig, ServerConfig, ShutdownConfig, SocketOptions,
    TelemetryConfig, TimeoutConfig, TraceCommands,
};
pub use error::{
    AddressError, ConfigError, Error, ProtocolError, ReloadError, ReplyError, ResourceKind, Result,
    TimeoutKind,
};
pub use frame::Frame;
pub use handler::{
    Actions, BodyDecision, ConnectDecision, ConnectInfo, Decision, EomContext, Events, Handler,
    MessageContext, Negotiated, ProtocolOptions, SessionContext, SessionEnd,
};
pub use message::{AddressFamily, Command, MilterText, Reply, ReplyCode};
#[cfg(feature = "test-support")]
#[doc(hidden)]
pub use server::SocketReadback;
pub use server::{
    ConfigWatch, ConnectionInfo, HandlerContext, ListenerStatus, ReloadLoader, ReloadReport,
    Reloaded, Server, ServerBuilder, ServerHandle, Status,
};
pub use tokio_util::sync::CancellationToken;
