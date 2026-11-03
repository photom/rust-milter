//! An asynchronous Milter protocol server library for Sendmail and Postfix.
//!
//! The library runs the Milter protocol with the MTA and calls a handler for
//! each protocol event, following libmilter's callback model. The design is
//! documented in `docs/` of the repository.

mod config;
mod error;
mod frame;
mod message;
pub mod protocol;

pub use bytes::{Bytes, BytesMut};
pub use config::{
    BindMode, KeepaliveConfig, ResourceLimitsConfig, ServerConfig, ShutdownConfig, SocketOptions,
    TelemetryConfig, TimeoutConfig, TraceCommands,
};
pub use error::{AddressError, ConfigError, Error, ProtocolError, ReplyError, Result};
pub use frame::Frame;
pub use message::{AddressFamily, Command, MilterText, Reply, ReplyCode};
