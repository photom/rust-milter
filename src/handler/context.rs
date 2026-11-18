//! The contexts the callbacks receive.

use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use super::{Actions, Handler, Negotiated};
use crate::engine::{Macros, Outbound};
use crate::server::ConnectionInfo;
use crate::{Error, MilterText, Reply, Result};

/// The most body data one `SMFIR_REPLBODY` frame carries.
const MAX_BODY_CHUNK: usize = 65_535;

/// What a session callback can see: the session state, the macros, the MTA
/// connection, the negotiated options, and the shutdown token.
///
/// [`Handler::connect`], [`Handler::helo`], [`Handler::unknown`], and
/// [`Handler::session_end`] receive it. [`MessageContext`] and [`EomContext`]
/// dereference to it, so every message callback has the same methods.
pub struct SessionContext<H: Handler> {
    session: Option<H::Session>,
    pub(crate) macros: Macros,
    /// The stage byte of the command being processed, for macro lookup.
    pub(crate) stage: Option<u8>,
    connection: Arc<ConnectionInfo>,
    pub(crate) negotiated: Negotiated,
    shutdown: CancellationToken,
}

impl<H: Handler> SessionContext<H> {
    pub(crate) fn new(connection: Arc<ConnectionInfo>, shutdown: CancellationToken) -> Self {
        SessionContext {
            session: None,
            macros: Macros::default(),
            stage: None,
            connection,
            negotiated: Negotiated::default(),
            shutdown,
        }
    }

    /// The handler's state for this session, created with `Default` on the
    /// first call in a session. See [`Handler::Session`].
    pub fn session(&mut self) -> &mut H::Session {
        self.session.get_or_insert_default()
    }

    /// The value of a macro the MTA sent in this session, at this stage or an
    /// earlier one.
    ///
    /// A one-letter name matches with or without braces, as in libmilter: `i`
    /// and `{i}` name the same macro. The lookup starts at the current
    /// command's stage, then goes through the earlier stages in protocol
    /// order: `C`, `H`, `M`, `R`, `T`, `L`, `N`, `B`, `E`. In
    /// [`Handler::unknown`], [`Handler::abort`], and [`Handler::session_end`],
    /// it looks in every stage, the latest first, starting with `U`. Which
    /// macros the library keeps, and for how long, is in the Macros row of
    /// [Protocol engine behavior](Handler#protocol-engine-behavior).
    pub fn macro_value(&self, name: &str) -> Option<&MilterText> {
        self.macros.get(name, self.stage)
    }

    /// Facts about the MTA connection: its ID, addresses, and accept time.
    pub fn connection(&self) -> &ConnectionInfo {
        &self.connection
    }

    /// The result of option negotiation.
    pub fn negotiated(&self) -> &Negotiated {
        &self.negotiated
    }

    /// The server's shutdown token, cancelled when the shutdown grace period
    /// ends. A callback that does long work should watch it and return early
    /// once it is cancelled.
    pub fn shutdown(&self) -> &CancellationToken {
        &self.shutdown
    }

    /// Starts a new session: fresh session state and no macros.
    pub(crate) fn reset(&mut self) {
        self.session = None;
        self.macros = Macros::default();
    }
}

/// What a message callback can see: everything in [`SessionContext`], through
/// `Deref`, and the message state.
///
/// [`Handler::mail`], [`Handler::rcpt`], [`Handler::data`],
/// [`Handler::header`], [`Handler::end_of_headers`], [`Handler::body`], and
/// [`Handler::abort`] receive it.
pub struct MessageContext<H: Handler> {
    pub(crate) session: SessionContext<H>,
    pub(crate) message: Option<H::Message>,
}

impl<H: Handler> MessageContext<H> {
    /// The handler's state for this message. See [`Handler::Message`] for when
    /// it is created and dropped.
    pub fn message(&mut self) -> &mut H::Message {
        self.message.get_or_insert_default()
    }
}

impl<H: Handler> Deref for MessageContext<H> {
    type Target = SessionContext<H>;

    fn deref(&self) -> &SessionContext<H> {
        &self.session
    }
}

impl<H: Handler> DerefMut for MessageContext<H> {
    fn deref_mut(&mut self) -> &mut SessionContext<H> {
        &mut self.session
    }
}

/// What [`Handler::end_of_message`] can see and do: everything in
/// [`MessageContext`], through `Deref`, and the message modifications.
///
/// The modification methods exist only on this type, so a handler can
/// modify a message only at end of message, as libmilter's `mi_sendok`
/// requires. Each one is async, returns `Result<()>`, and sends its reply to
/// the MTA at once. It fails with [`Error::ActionNotNegotiated`] when its
/// action is not in [`Handler::actions`] or the MTA did not offer it, and
/// with [`Error::InvalidReply`] when the MTA would refuse the values; the
/// [`ReplyError`](crate::ReplyError) variants list the rules.
///
/// | Method | Action required | Sends |
/// |--------|-----------------|-------|
/// | [`add_header`](EomContext::add_header) | [`ADD_HEADERS`](Actions::ADD_HEADERS) | `SMFIR_ADDHEADER` |
/// | [`insert_header`](EomContext::insert_header) | [`ADD_HEADERS`](Actions::ADD_HEADERS) | `SMFIR_INSHEADER` |
/// | [`change_header`](EomContext::change_header) | [`CHANGE_HEADERS`](Actions::CHANGE_HEADERS) | `SMFIR_CHGHEADER` |
/// | [`delete_header`](EomContext::delete_header) | [`CHANGE_HEADERS`](Actions::CHANGE_HEADERS) | `SMFIR_CHGHEADER` with an empty value |
/// | [`add_recipient`](EomContext::add_recipient) | [`ADD_RECIPIENTS`](Actions::ADD_RECIPIENTS) | `SMFIR_ADDRCPT` |
/// | [`add_recipient_with_args`](EomContext::add_recipient_with_args) | [`ADD_RECIPIENTS_WITH_ARGS`](Actions::ADD_RECIPIENTS_WITH_ARGS) | `SMFIR_ADDRCPT_PAR` |
/// | [`delete_recipient`](EomContext::delete_recipient) | [`DELETE_RECIPIENTS`](Actions::DELETE_RECIPIENTS) | `SMFIR_DELRCPT` |
/// | [`change_from`](EomContext::change_from) | [`CHANGE_FROM`](Actions::CHANGE_FROM) | `SMFIR_CHGFROM` |
/// | [`replace_body`](EomContext::replace_body) | [`CHANGE_BODY`](Actions::CHANGE_BODY) | `SMFIR_REPLBODY`, split into chunks of at most 65,535 bytes |
/// | [`quarantine`](EomContext::quarantine) | [`QUARANTINE`](Actions::QUARANTINE) | `SMFIR_QUARANTINE` |
/// | [`progress`](EomContext::progress) | none | `SMFIR_PROGRESS` |
///
/// The header methods send each value unchanged, and the MTA puts a space
/// between the colon and the value unless
/// [`ProtocolOptions::HEADER_LEADING_SPACE`](crate::ProtocolOptions::HEADER_LEADING_SPACE)
/// is in force. With the option in force, a handler that wants
/// `Name: value` passes `" value"`.
pub struct EomContext<H: Handler> {
    pub(crate) message: MessageContext<H>,
    pub(crate) outbound: Outbound,
}

impl<H: Handler> Deref for EomContext<H> {
    type Target = MessageContext<H>;

    fn deref(&self) -> &MessageContext<H> {
        &self.message
    }
}

impl<H: Handler> DerefMut for EomContext<H> {
    fn deref_mut(&mut self) -> &mut MessageContext<H> {
        &mut self.message
    }
}

impl<H: Handler> EomContext<H> {
    /// Adds a header at the end of the headers (`SMFIR_ADDHEADER`).
    pub async fn add_header(&mut self, name: &str, value: &str) -> Result<()> {
        self.send(
            Actions::ADD_HEADERS,
            Reply::AddHeader {
                name: name.to_owned(),
                value: value.to_owned(),
            },
        )
        .await
    }

    /// Inserts a header at `index`; 0 inserts before the first header
    /// (`SMFIR_INSHEADER`).
    pub async fn insert_header(&mut self, index: u32, name: &str, value: &str) -> Result<()> {
        self.send(
            Actions::ADD_HEADERS,
            Reply::InsertHeader {
                index,
                name: name.to_owned(),
                value: value.to_owned(),
            },
        )
        .await
    }

    /// Changes the `index`th header called `name`, counting from 1
    /// (`SMFIR_CHGHEADER`).
    pub async fn change_header(&mut self, name: &str, index: u32, value: &str) -> Result<()> {
        self.send(
            Actions::CHANGE_HEADERS,
            Reply::ChangeHeader {
                index,
                name: name.to_owned(),
                value: value.to_owned(),
            },
        )
        .await
    }

    /// Deletes the `index`th header called `name`, counting from 1
    /// (`SMFIR_CHGHEADER` with an empty value).
    pub async fn delete_header(&mut self, name: &str, index: u32) -> Result<()> {
        self.change_header(name, index, "").await
    }

    /// Adds a recipient (`SMFIR_ADDRCPT`).
    pub async fn add_recipient(&mut self, recipient: &str) -> Result<()> {
        self.send(
            Actions::ADD_RECIPIENTS,
            Reply::AddRecipient(recipient.to_owned()),
        )
        .await
    }

    /// Adds a recipient with ESMTP arguments (`SMFIR_ADDRCPT_PAR`).
    pub async fn add_recipient_with_args(
        &mut self,
        recipient: &str,
        args: Option<&str>,
    ) -> Result<()> {
        self.send(
            Actions::ADD_RECIPIENTS_WITH_ARGS,
            Reply::AddRecipientWithArgs {
                rcpt: recipient.to_owned(),
                args: owned(args),
            },
        )
        .await
    }

    /// Removes a recipient (`SMFIR_DELRCPT`).
    pub async fn delete_recipient(&mut self, recipient: &str) -> Result<()> {
        self.send(
            Actions::DELETE_RECIPIENTS,
            Reply::DeleteRecipient(recipient.to_owned()),
        )
        .await
    }

    /// Changes the sender, with optional ESMTP arguments (`SMFIR_CHGFROM`).
    pub async fn change_from(&mut self, sender: &str, args: Option<&str>) -> Result<()> {
        self.send(
            Actions::CHANGE_FROM,
            Reply::ChangeFrom {
                from: sender.to_owned(),
                args: owned(args),
            },
        )
        .await
    }

    /// Replaces the body (`SMFIR_REPLBODY`, in chunks of at most 65,535
    /// bytes). An empty `body` sends nothing, as in libmilter.
    pub async fn replace_body(&mut self, body: &[u8]) -> Result<()> {
        self.require(Actions::CHANGE_BODY)?;
        for chunk in body.chunks(MAX_BODY_CHUNK) {
            let reply = Reply::ReplaceBody(bytes::Bytes::copy_from_slice(chunk));
            self.outbound.send(&reply).await?;
        }
        Ok(())
    }

    /// Quarantines the message with a reason (`SMFIR_QUARANTINE`).
    pub async fn quarantine(&mut self, reason: &str) -> Result<()> {
        self.send(Actions::QUARANTINE, Reply::Quarantine(reason.to_owned()))
            .await
    }

    /// Tells the MTA that the milter is still working (`SMFIR_PROGRESS`), so
    /// it does not time out.
    pub async fn progress(&mut self) -> Result<()> {
        self.outbound.send(&Reply::Progress).await
    }

    async fn send(&mut self, action: Actions, reply: Reply) -> Result<()> {
        self.require(action)?;
        self.outbound.send(&reply).await
    }

    fn require(&self, action: Actions) -> Result<()> {
        if self.negotiated().actions().contains(action) {
            Ok(())
        } else {
            Err(Error::ActionNotNegotiated(action))
        }
    }
}

/// `text` as an owned string, or `None`.
fn owned(text: Option<&str>) -> Option<String> {
    match text {
        Some(text) => Some(text.to_owned()),
        None => None,
    }
}
