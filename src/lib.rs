//! An asynchronous Milter protocol server library for Sendmail and Postfix.
//!
//! The library runs the Milter protocol with the MTA and calls a handler for
//! each protocol event, following libmilter's callback model. The design is
//! documented in `docs/` of the repository.

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
    MessageContext, Negotiated, SessionContext, SessionEnd,
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
