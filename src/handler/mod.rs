//! The handler side of the protocol: the callbacks a milter implements, the
//! decisions they return, and what they are told about the session.

mod context;
mod flags;

use std::future::Future;
use std::net::SocketAddr;

use bytes::Bytes;

pub use context::{EomContext, MessageContext, SessionContext};
pub use flags::{Actions, Events};

use crate::{MilterText, ReplyCode, Result};

/// The callbacks of a milter, following libmilter's `xxfi_*` callbacks.
///
/// The library runs the Milter protocol and calls one callback per event.
/// Every callback has a default that continues, so a handler implements only
/// the events it needs. One handler value serves every connection; data per
/// session and per message belongs in [`Handler::Session`] and
/// [`Handler::Message`], which the library creates with `Default` and drops
/// when the session or message ends.
///
/// Callbacks for one connection run one at a time, in protocol order. A
/// callback that returns an error closes the connection, after `abort` for an
/// open message and `session_end` with [`SessionEnd::Error`].
///
/// Callbacks can be written as `async fn`.
pub trait Handler: Send + Sync + Sized + 'static {
    /// Data kept for one SMTP session, from `SMFIC_CONNECT` to its end.
    type Session: Default + Send + 'static;

    /// Data kept for one message, from `SMFIC_MAIL` to its final reply.
    type Message: Default + Send + 'static;

    /// The protocol stages this handler receives. The library asks the MTA
    /// not to send the others. The default is every stage.
    fn events(&self) -> Events {
        Events::ALL
    }

    /// The message modifications this handler may perform. The connection
    /// closes if the MTA does not offer all of them. The default is none.
    fn actions(&self) -> Actions {
        Actions::NONE
    }

    /// `SMFIC_CONNECT`: the SMTP client connected to the MTA.
    fn connect(
        &self,
        ctx: &mut SessionContext<Self>,
        info: &ConnectInfo,
    ) -> impl Future<Output = Result<ConnectDecision>> + Send {
        let _ = (ctx, info);
        async { Ok(ConnectDecision::Continue) }
    }

    /// `SMFIC_HELO`: the HELO or EHLO name.
    fn helo(
        &self,
        ctx: &mut SessionContext<Self>,
        name: &MilterText,
    ) -> impl Future<Output = Result<ConnectDecision>> + Send {
        let _ = (ctx, name);
        async { Ok(ConnectDecision::Continue) }
    }

    /// `SMFIC_MAIL`: the sender and its ESMTP arguments.
    fn mail(
        &self,
        ctx: &mut MessageContext<Self>,
        sender: &MilterText,
        args: &[MilterText],
    ) -> impl Future<Output = Result<Decision>> + Send {
        let _ = (ctx, sender, args);
        async { Ok(Decision::Continue) }
    }

    /// `SMFIC_RCPT`: one recipient and its ESMTP arguments. A reject applies
    /// to this recipient only.
    fn rcpt(
        &self,
        ctx: &mut MessageContext<Self>,
        recipient: &MilterText,
        args: &[MilterText],
    ) -> impl Future<Output = Result<Decision>> + Send {
        let _ = (ctx, recipient, args);
        async { Ok(Decision::Continue) }
    }

    /// `SMFIC_DATA`: the DATA command. Needs protocol version 4 or later.
    fn data(
        &self,
        ctx: &mut MessageContext<Self>,
    ) -> impl Future<Output = Result<Decision>> + Send {
        let _ = ctx;
        async { Ok(Decision::Continue) }
    }

    /// `SMFIC_HEADER`: one header field.
    fn header(
        &self,
        ctx: &mut MessageContext<Self>,
        name: &MilterText,
        value: &MilterText,
    ) -> impl Future<Output = Result<Decision>> + Send {
        let _ = (ctx, name, value);
        async { Ok(Decision::Continue) }
    }

    /// `SMFIC_EOH`: the end of the headers.
    fn end_of_headers(
        &self,
        ctx: &mut MessageContext<Self>,
    ) -> impl Future<Output = Result<Decision>> + Send {
        let _ = ctx;
        async { Ok(Decision::Continue) }
    }

    /// `SMFIC_BODY`: one body chunk.
    fn body(
        &self,
        ctx: &mut MessageContext<Self>,
        chunk: Bytes,
    ) -> impl Future<Output = Result<BodyDecision>> + Send {
        let _ = (ctx, chunk);
        async { Ok(BodyDecision::Continue) }
    }

    /// `SMFIC_BODYEOB`: the end of the message. The only callback that can
    /// modify the message, through [`EomContext`].
    fn end_of_message(
        &self,
        ctx: &mut EomContext<Self>,
    ) -> impl Future<Output = Result<Decision>> + Send {
        let _ = ctx;
        async { Ok(Decision::Continue) }
    }

    /// The open message ends without its final reply: `SMFIC_ABORT`, a
    /// command that restarts the conversation, or the end of the session.
    /// The message state is dropped afterwards.
    fn abort(&self, ctx: &mut MessageContext<Self>) -> impl Future<Output = Result<()>> + Send {
        let _ = ctx;
        async { Ok(()) }
    }

    /// `SMFIC_UNKNOWN`: an SMTP command the MTA does not know. Needs protocol
    /// version 3 or later.
    fn unknown(
        &self,
        ctx: &mut SessionContext<Self>,
        command: &MilterText,
    ) -> impl Future<Output = Result<Decision>> + Send {
        let _ = (ctx, command);
        async { Ok(Decision::Continue) }
    }

    /// The SMTP session ended. The session state is dropped afterwards.
    fn session_end(
        &self,
        ctx: &mut SessionContext<Self>,
        reason: SessionEnd,
    ) -> impl Future<Output = Result<()>> + Send {
        let _ = (ctx, reason);
        async { Ok(()) }
    }
}

/// A decision of `connect` or `helo`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConnectDecision {
    /// Go on to the next stage.
    Continue,
    /// Accept the whole connection.
    Accept,
    /// Reply 5xx.
    Reject,
    /// Reply 4xx.
    TempFail,
    /// A custom reply: 4xx acts as `TempFail`, 5xx as `Reject`.
    Reply(ReplyCode),
}

/// A decision of a message callback or of `unknown`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Decision {
    /// Go on to the next stage.
    Continue,
    /// Accept the message.
    Accept,
    /// Reply 5xx.
    Reject,
    /// Reply 4xx.
    TempFail,
    /// A custom reply: 4xx acts as `TempFail`, 5xx as `Reject`.
    Reply(ReplyCode),
    /// Accept the message from the client and drop it.
    Discard,
}

/// A decision of `body`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum BodyDecision {
    /// Go on to the next chunk.
    Continue,
    /// Accept the message.
    Accept,
    /// Reply 5xx.
    Reject,
    /// Reply 4xx.
    TempFail,
    /// A custom reply: 4xx acts as `TempFail`, 5xx as `Reject`.
    Reply(ReplyCode),
    /// Accept the message from the client and drop it.
    Discard,
    /// Receive no more body chunks and go on to the end of the message.
    Skip,
}

impl From<ConnectDecision> for Decision {
    fn from(decision: ConnectDecision) -> Decision {
        match decision {
            ConnectDecision::Continue => Decision::Continue,
            ConnectDecision::Accept => Decision::Accept,
            ConnectDecision::Reject => Decision::Reject,
            ConnectDecision::TempFail => Decision::TempFail,
            ConnectDecision::Reply(code) => Decision::Reply(code),
        }
    }
}

impl From<Decision> for BodyDecision {
    fn from(decision: Decision) -> BodyDecision {
        match decision {
            Decision::Continue => BodyDecision::Continue,
            Decision::Accept => BodyDecision::Accept,
            Decision::Reject => BodyDecision::Reject,
            Decision::TempFail => BodyDecision::TempFail,
            Decision::Reply(code) => BodyDecision::Reply(code),
            Decision::Discard => BodyDecision::Discard,
        }
    }
}

/// Why an SMTP session ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SessionEnd {
    /// `SMFIC_QUIT`.
    Quit,
    /// `SMFIC_QUIT_NC`: a new session follows on the same connection.
    QuitNewConnection,
    /// The MTA closed the connection between frames without `SMFIC_QUIT`.
    ConnectionClosed,
    /// An error closes the connection: a protocol error, a failed callback,
    /// a reset, an end of file in the middle of a frame, or a timeout.
    Error,
    /// The server is shutting down.
    ServerShutdown,
}

/// The SMTP client, as the MTA describes it in `SMFIC_CONNECT`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ConnectInfo {
    /// The client's hostname as the MTA sent it; `[<ip>]` when it did not
    /// resolve.
    pub hostname: MilterText,
    /// The client's address. `None` for `SMFIA_UNKNOWN` and `SMFIA_UNIX`, or
    /// when the address is not an IP address.
    pub address: Option<SocketAddr>,
}

/// The result of option negotiation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Negotiated {
    pub(crate) version: u32,
    pub(crate) actions: Actions,
    pub(crate) protocol: u32,
    pub(crate) mta_protocol: u32,
}

impl Negotiated {
    /// The protocol version both sides use.
    pub fn version(&self) -> u32 {
        self.version
    }

    /// The actions the handler may perform.
    pub fn actions(&self) -> Actions {
        self.actions
    }

    /// The `SMFIP_*` protocol flags in the milter's answer.
    pub fn protocol(&self) -> u32 {
        self.protocol
    }

    /// The `SMFIP_*` protocol flags the MTA offered, with 0 read as the
    /// version 1 defaults.
    pub fn mta_protocol(&self) -> u32 {
        self.mta_protocol
    }
}
