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

/// What a session callback can see: the session state, the macros, and the
/// connection.
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

    /// The handler's state for this session.
    pub fn session(&mut self) -> &mut H::Session {
        self.session.get_or_insert_default()
    }

    /// The value of a macro the MTA sent in this session, at this stage or
    /// an earlier one. `{x}` and `x` name the same one-letter macro.
    pub fn macro_value(&self, name: &str) -> Option<&MilterText> {
        self.macros.get(name, self.stage)
    }

    /// The MTA connection.
    pub fn connection(&self) -> &ConnectionInfo {
        &self.connection
    }

    /// The result of option negotiation.
    pub fn negotiated(&self) -> &Negotiated {
        &self.negotiated
    }

    /// The server's shutdown token.
    pub fn shutdown(&self) -> &CancellationToken {
        &self.shutdown
    }

    /// Starts a new session: fresh session state and no macros.
    pub(crate) fn reset(&mut self) {
        self.session = None;
        self.macros = Macros::default();
    }
}

/// What a message callback can see: everything in [`SessionContext`], and
/// the message state.
pub struct MessageContext<H: Handler> {
    pub(crate) session: SessionContext<H>,
    pub(crate) message: Option<H::Message>,
}

impl<H: Handler> MessageContext<H> {
    /// The handler's state for this message.
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

/// What `end_of_message` can see and do: everything in [`MessageContext`],
/// and the message modifications.
///
/// Each modification sends its reply to the MTA at once. It fails with
/// [`Error::ActionNotNegotiated`] when its action was not negotiated, and
/// with [`Error::InvalidReply`] when the MTA would refuse the values.
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
