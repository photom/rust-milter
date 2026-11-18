//! The handler side of the protocol: the callbacks a milter implements, the
//! decisions they return, and what they are told about the session.

mod context;
mod flags;

use std::future::Future;
use std::net::SocketAddr;

use bytes::Bytes;

pub use context::{EomContext, MessageContext, SessionContext};
pub use flags::{Actions, Events, ProtocolOptions};

use crate::{MilterText, ReplyCode, Result};

/// The callbacks of a milter, following libmilter's `xxfi_*` callbacks.
///
/// `Handler` is the trait the application implements. The library runs the
/// Milter protocol, and the handler answers one callback per event with a
/// decision ([ADR 0002]). Every callback has a default that continues, so a
/// handler implements only the events it needs. Callbacks can be written as
/// `async fn`; the [tutorial] writes a first handler.
///
/// A handler type must be `Send + Sync + 'static`. One handler value serves
/// every connection, so its fields hold only data that all sessions share,
/// such as a [`ConfigWatch`](crate::ConfigWatch). All data of one session or
/// one message belongs in [`Handler::Session`] and [`Handler::Message`]. The
/// library drops them at the end of each session and message, which keeps
/// the data of one SMTP session out of the next.
///
/// # Negotiation methods
///
/// The library calls these methods once per connection, during option
/// negotiation. All three are synchronous and have defaults ([ADR 0020]).
///
/// | Method | Default | libmilter equivalent |
/// |--------|---------|----------------------|
/// | [`events`](Handler::events) | every stage | the set of registered `xxfi_*` callbacks |
/// | [`actions`](Handler::actions) | none | `xxfi_flags` |
/// | [`protocol_options`](Handler::protocol_options) | none | the protocol flags a milter sets in `xxfi_negotiate` |
///
/// The library asks the MTA not to send the stages missing from `events()`
/// (`SMFIP_NO*` flags), and requests the actions in `actions()` (`SMFIF_*`
/// flags) and the options in `protocol_options()` (`SMFIP_*` flags). If the
/// MTA does not offer every requested action, the library closes the
/// connection, as libmilter does. After the actions, it checks the options
/// the same way. An option counts as offered when the MTA's protocol flags
/// hold it and the negotiated version is 6.
///
/// # Callbacks
///
/// Every callback returns a `Send` future that resolves to [`Result<T>`],
/// where `T` is a decision type or `()`. Each has a default that returns `Ok`
/// with `Continue`, or does nothing.
///
/// | Callback | libmilter equivalent | Context |
/// |----------|----------------------|---------|
/// | [`connect`](Handler::connect) | `xxfi_connect` | [`SessionContext`] |
/// | [`helo`](Handler::helo) | `xxfi_helo` | [`SessionContext`] |
/// | [`mail`](Handler::mail) | `xxfi_envfrom` | [`MessageContext`] |
/// | [`rcpt`](Handler::rcpt) | `xxfi_envrcpt` | [`MessageContext`] |
/// | [`data`](Handler::data) | `xxfi_data` | [`MessageContext`] |
/// | [`header`](Handler::header) | `xxfi_header` | [`MessageContext`] |
/// | [`end_of_headers`](Handler::end_of_headers) | `xxfi_eoh` | [`MessageContext`] |
/// | [`body`](Handler::body) | `xxfi_body` | [`MessageContext`] |
/// | [`end_of_message`](Handler::end_of_message) | `xxfi_eom` | [`EomContext`] |
/// | [`abort`](Handler::abort) | `xxfi_abort` | [`MessageContext`] |
/// | [`unknown`](Handler::unknown) | `xxfi_unknown` | [`SessionContext`] |
/// | [`session_end`](Handler::session_end) | `xxfi_close` | [`SessionContext`] |
///
/// The library calls a callback only for the stages in `events()`, and only
/// if the negotiated protocol version has the stage: `unknown` needs version 3
/// or later, `data` version 4 or later.
///
/// Contract:
/// - Callbacks for one connection run one at a time, in protocol order.
///   Callbacks for different connections run concurrently.
/// - A callback that returns an error closes the connection. The library
///   calls `abort` first if a message is open, then `session_end` with
///   [`SessionEnd::Error`]. A handler turns its own error into
///   [`Error::Handler`](crate::Error::Handler) with
///   [`Error::handler`](crate::Error::handler), which accepts any
///   `std::error::Error + Send + Sync + 'static`. The library writes that
///   error and its source chain into the WARN "Connection closed" log, so its
///   text must not hold SMTP data
///   ([telemetry.md: Data that must never be recorded]).
/// - A callback that panics closes the connection with
///   [`Error::HandlerPanic`](crate::Error::HandlerPanic). No further callback
///   runs for that session, not even `abort` or `session_end`, and the library
///   drops its `Session` and `Message` values. The same holds for a panic in
///   `events()`, `actions()`, or `protocol_options()`, and in `abort` or
///   `session_end` while a session ends ([ADR 0018]).
/// - A callback should finish promptly when the shutdown token from its
///   context is cancelled.
/// - Settings come from the [`ConfigWatch`](crate::ConfigWatch) the handler
///   holds. Read the application configuration once per session (in
///   `connect`) or once per message (in `mail`), and keep the value in
///   `Session` or `Message`.
///
/// # Protocol engine behavior
///
/// The library performs these protocol duties for every handler:
///
/// | Duty | Behavior |
/// |------|----------|
/// | Version negotiation | Closes the connection if the MTA offers a version below 2; otherwise answers with the lower of the MTA's version and 6 |
/// | Flags | Treats zero action or protocol flags from the MTA as the version 1 defaults, and requests only flags the MTA offered. Closes the connection without a reply when the MTA did not offer every action in `actions()` ([`ActionsNotOffered`](crate::ProtocolError::ActionsNotOffered)). Then closes it the same way when the MTA did not offer every option in `protocol_options()`, or the version is below 6 ([`OptionsNotOffered`](crate::ProtocolError::OptionsNotOffered)) |
/// | Stage filtering | Requests `SMFIP_NO*` for the stages missing from `events()` |
/// | Macros | Stores the macros of the stage bytes `C`, `H`, `M`, `R`, `T`, `L`, `N`, `B`, `E`, and `U`, including `L`, `B`, and `U` from Postfix, and drops those of any other byte, which no lookup reaches. Never replies to `SMFIC_MACRO`. Macros sent again for a stage replace that stage's earlier ones. Each command of a stage drops the macros of the later stages, as libmilter does, so a message never sees an earlier message's RCPT, DATA, header, body, or end-of-message macros. It also drops the `U` macros, which belong to the unknown command they came with. All macros are cleared when the session ends. [`SessionContext::macro_value`] gives the lookup order. The stored macros are copies that count against the connection's memory ([ADR 0019]) |
/// | Session start | A session starts with `SMFIC_CONNECT`. When the MTA leaves `SMFIC_CONNECT` out, the session starts with the first command after option negotiation, or after `SMFIC_QUIT_NC`, that is not `SMFIC_MACRO`, `SMFIC_ABORT`, or `SMFIC_QUIT` |
/// | State machine | Tracks the protocol state with libmilter's transition rules, and closes the connection on `SMFIC_QUIT_NC` outside `ST_ENDM` and `ST_UNKN` ([ADR 0015]) |
/// | Commands in a wrong state | Recovers like libmilter for a command that fits from `ST_HELO` (`SMFIC_HELO`, `SMFIC_MAIL`, `SMFIC_UNKNOWN`, and later stages the handler switched off): calls `abort` if a message is open, then processes the command. Ignores `SMFIC_ABORT`. Ends the session on `SMFIC_QUIT`, as libmilter does. Closes the connection with a protocol error on any other command ([ADR 0015]) |
/// | Replies | Encodes each decision as the matching `SMFIR_*` reply. Sends no reply to `SMFIC_MACRO`, `SMFIC_ABORT`, `SMFIC_QUIT`, or `SMFIC_QUIT_NC` |
/// | Abort | Calls `abort` and drops the message state when a message is open, and ignores `SMFIC_ABORT` outside a message. A message that ends without its final reply or a decision that ends it also gets `abort`: on recovery from a wrong state, on an `SMFIC_MAIL` that starts a new message while one is open, and before `session_end` when the session ends with a message open |
/// | Session end | Calls `session_end` and drops the session state: on `SMFIC_QUIT` in any state, after `abort` if a message is open, then closes the connection ([`Quit`](SessionEnd::Quit)); on `SMFIC_QUIT_NC` ([`QuitNewConnection`](SessionEnd::QuitNewConnection)); on end of file between frames without `SMFIC_QUIT` ([`ConnectionClosed`](SessionEnd::ConnectionClosed)); before closing the connection for an error, a reset, an end of file in the middle of a frame, or a timeout ([`Error`](SessionEnd::Error)); and when shutdown closes the connection ([`ServerShutdown`](SessionEnd::ServerShutdown)). A forced shutdown closes the connection without calling `abort` or `session_end`. A connection that closes before a session starts, or between `SMFIC_QUIT_NC` and the next session, calls no callback. A panic in a callback ends the session without `abort` or `session_end` ([ADR 0018]) |
///
/// [ADR 0002]: https://github.com/photom/rust-milter/blob/main/docs/adr/0002-library-runs-the-protocol-handlers-decide.md
/// [ADR 0015]: https://github.com/photom/rust-milter/blob/main/docs/adr/0015-commands-in-wrong-state-follow-libmilter-or-close.md
/// [ADR 0018]: https://github.com/photom/rust-milter/blob/main/docs/adr/0018-panics-in-handler-code-and-the-reload-loader-are-caught.md
/// [ADR 0019]: https://github.com/photom/rust-milter/blob/main/docs/adr/0019-a-connection-counts-what-it-holds-until-each-command-ends.md
/// [ADR 0020]: https://github.com/photom/rust-milter/blob/main/docs/adr/0020-protocol-options-are-a-fixed-handler-method-and-close-when-missing.md
/// [tutorial]: https://github.com/photom/rust-milter/blob/main/docs/tutorials/getting-started.md
/// [telemetry.md: Data that must never be recorded]: https://github.com/photom/rust-milter/blob/main/docs/reference/telemetry.md#data-that-must-never-be-recorded
pub trait Handler: Send + Sync + Sized + 'static {
    /// Data kept for one SMTP session.
    ///
    /// The library creates it with `Default` the first time a callback calls
    /// [`SessionContext::session`] in an SMTP session, and drops it when the
    /// session ends: on `SMFIC_QUIT`, on `SMFIC_QUIT_NC`, or when the
    /// connection closes.
    type Session: Default + Send + 'static;

    /// Data kept for one message.
    ///
    /// The library creates it with `Default` at `SMFIC_MAIL`, or at the first
    /// command of a message when the handler left `MAIL` out of `events()`.
    /// It drops the value:
    /// - after the final reply at end of message
    /// - after a decision that ends the message earlier: `Accept` from any
    ///   message callback, or `Reject`, `TempFail`, `Reply`, or `Discard` from
    ///   one other than `rcpt`, which reset the protocol state to `ST_HELO` as
    ///   in libmilter
    /// - on `SMFIC_ABORT`
    /// - when the session ends
    type Message: Default + Send + 'static;

    /// The protocol stages this handler receives. The library asks the MTA
    /// not to send the others. The default is every stage.
    ///
    /// libmilter equivalent: the set of registered `xxfi_*` callbacks.
    fn events(&self) -> Events {
        Events::ALL
    }

    /// The message modifications this handler may perform at end of message.
    /// The connection closes if the MTA does not offer all of them, with
    /// [`ProtocolError::ActionsNotOffered`](crate::ProtocolError::ActionsNotOffered).
    /// The default is none.
    ///
    /// libmilter equivalent: `xxfi_flags`.
    fn actions(&self) -> Actions {
        Actions::NONE
    }

    /// The protocol options this handler requests. The connection closes if
    /// the MTA does not offer all of them at protocol version 6, with
    /// [`ProtocolError::OptionsNotOffered`](crate::ProtocolError::OptionsNotOffered).
    /// The default is none.
    ///
    /// libmilter equivalent: the protocol flags a milter sets in
    /// `xxfi_negotiate`.
    fn protocol_options(&self) -> ProtocolOptions {
        ProtocolOptions::NONE
    }

    /// `SMFIC_CONNECT`: the SMTP client connected to the MTA. `info` holds the
    /// client's hostname and address.
    ///
    /// `Accept` accepts the whole connection: Sendmail and Postfix end the
    /// milter session after it.
    fn connect(
        &self,
        ctx: &mut SessionContext<Self>,
        info: &ConnectInfo,
    ) -> impl Future<Output = Result<ConnectDecision>> + Send {
        let _ = (ctx, info);
        async { Ok(ConnectDecision::Continue) }
    }

    /// `SMFIC_HELO`: the HELO or EHLO name.
    ///
    /// `Accept` accepts the whole connection: Sendmail and Postfix end the
    /// milter session after it.
    fn helo(
        &self,
        ctx: &mut SessionContext<Self>,
        name: &MilterText,
    ) -> impl Future<Output = Result<ConnectDecision>> + Send {
        let _ = (ctx, name);
        async { Ok(ConnectDecision::Continue) }
    }

    /// `SMFIC_MAIL`: the sender and its ESMTP arguments. The first `mail` of
    /// a message gets a fresh [`Handler::Message`] value.
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

    /// `SMFIC_HEADER`: one header field. The value keeps its leading space
    /// only with [`ProtocolOptions::HEADER_LEADING_SPACE`] in force.
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
    ///
    /// [`BodyDecision::Skip`] asks the MTA for no more chunks.
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

    /// The open message ends without its final reply: on `SMFIC_ABORT`, on
    /// recovery from a command in a wrong state, on an `SMFIC_MAIL` that
    /// starts a new message, or before `session_end` when the session ends.
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

    /// The SMTP session ended, for the `reason` given. The session state is
    /// dropped afterwards.
    fn session_end(
        &self,
        ctx: &mut SessionContext<Self>,
        reason: SessionEnd,
    ) -> impl Future<Output = Result<()>> + Send {
        let _ = (ctx, reason);
        async { Ok(()) }
    }
}

/// A decision of [`Handler::connect`] or [`Handler::helo`].
///
/// The variants mean what libmilter's `SMFIS_*` codes mean. There is no
/// `Discard`, because Postfix does not allow it at these stages.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConnectDecision {
    /// Go on to the next stage.
    Continue,
    /// Accept the whole connection. The MTA then sends no more events for
    /// it: Sendmail and Postfix end the milter session after a connect-level
    /// or HELO-level accept.
    Accept,
    /// Reply 5xx.
    Reject,
    /// Reply 4xx.
    TempFail,
    /// A custom reply: a 4xx code acts as `TempFail`, a 5xx code as `Reject`.
    Reply(ReplyCode),
}

/// A decision of a message callback or of [`Handler::unknown`]: the
/// [`ConnectDecision`] variants, plus `Discard`.
///
/// The variants mean what libmilter's `SMFIS_*` codes mean. A reject from
/// [`Handler::rcpt`] applies to that recipient only.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Decision {
    /// Go on to the next stage.
    Continue,
    /// Accept the message. The MTA then sends no more events for it.
    Accept,
    /// Reply 5xx.
    Reject,
    /// Reply 4xx.
    TempFail,
    /// A custom reply: a 4xx code acts as `TempFail`, a 5xx code as `Reject`.
    Reply(ReplyCode),
    /// Accept the message from the client and drop it.
    Discard,
}

/// A decision of [`Handler::body`]: the [`Decision`] variants, plus `Skip`.
///
/// The variants mean what libmilter's `SMFIS_*` codes mean.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum BodyDecision {
    /// Go on to the next chunk.
    Continue,
    /// Accept the message. The MTA then sends no more events for it.
    Accept,
    /// Reply 5xx.
    Reject,
    /// Reply 4xx.
    TempFail,
    /// A custom reply: a 4xx code acts as `TempFail`, a 5xx code as `Reject`.
    Reply(ReplyCode),
    /// Accept the message from the client and drop it.
    Discard,
    /// Receive no more body chunks and go on to the end of the message. If
    /// the MTA did not offer `SMFIP_SKIP`, the library sends `Continue` and
    /// stops calling `body` for this message.
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

/// Why an SMTP session ended, as [`Handler::session_end`] receives it.
///
/// The Session end row of
/// [Protocol engine behavior](Handler#protocol-engine-behavior) lists the
/// events that end a session with each reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SessionEnd {
    /// `SMFIC_QUIT`, in any state. The connection closes after it.
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

/// The SMTP client's hostname and address, as the MTA describes them in
/// `SMFIC_CONNECT`. [`Handler::connect`] receives it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ConnectInfo {
    /// The SMTP client's hostname as the MTA sent it; `[<ip>]` when it did
    /// not resolve.
    pub hostname: MilterText,
    /// The SMTP client's address. `None` for the families `SMFIA_UNKNOWN`
    /// and `SMFIA_UNIX`, and when the address text is not an IP address. An
    /// `IPv6:` prefix, as Sendmail can write it, is removed.
    pub address: Option<SocketAddr>,
}

/// The result of option negotiation, from [`SessionContext::negotiated`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Negotiated {
    pub(crate) version: u32,
    pub(crate) actions: Actions,
    pub(crate) options: ProtocolOptions,
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

    /// The protocol options in force.
    pub fn options(&self) -> ProtocolOptions {
        self.options
    }

    /// The `SMFIP_*` protocol flags in the milter's answer, options included.
    pub fn protocol(&self) -> u32 {
        self.protocol
    }

    /// The `SMFIP_*` protocol flags the MTA offered, with 0 read as the
    /// version 1 defaults.
    pub fn mta_protocol(&self) -> u32 {
        self.mta_protocol
    }
}
