//! An asynchronous Milter protocol server library for Sendmail and Postfix.
//!
//! The library runs the Milter protocol with the MTA and calls a handler for
//! each protocol event, following libmilter's callback model. The design is
//! documented in `docs/` of the repository.

mod error;
mod frame;
mod message;
pub mod protocol;

pub use bytes::{Bytes, BytesMut};
pub use error::{Error, ProtocolError, ReplyError, Result};
pub use frame::Frame;
pub use message::{AddressFamily, Command, MilterText, Reply, ReplyCode};
