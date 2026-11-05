//! The names that spans and command events use for commands and replies,
//! and which commands `trace_commands` records. See "Traces" in
//! `docs/reference/telemetry.md`.

use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use crate::protocol::*;
use crate::{Reply, TraceCommands};

/// What the engine needs for its spans and events.
pub(crate) struct Trace {
    /// The server's session counter, for `milter.session.id`.
    pub(crate) sessions: Arc<AtomicU64>,
    /// Which commands become `milter.command` events.
    pub(crate) commands: TraceCommands,
}

/// The `milter.command` value of a command code.
pub(crate) fn command_name(code: u8) -> &'static str {
    match code {
        SMFIC_OPTNEG => "optneg",
        SMFIC_MACRO => "macro",
        SMFIC_CONNECT => "connect",
        SMFIC_HELO => "helo",
        SMFIC_MAIL => "mail",
        SMFIC_RCPT => "rcpt",
        SMFIC_DATA => "data",
        SMFIC_HEADER => "header",
        SMFIC_EOH => "eoh",
        SMFIC_BODY => "body",
        SMFIC_BODYEOB => "eom",
        SMFIC_ABORT => "abort",
        SMFIC_UNKNOWN => "unknown",
        SMFIC_QUIT => "quit",
        SMFIC_QUIT_NC => "quit_nc",
        // `Command::from_frame` rejects every other code before an event.
        _ => "unknown_command",
    }
}

/// The `milter.reply` value of a reply.
pub(crate) fn reply_name(reply: &Reply) -> &'static str {
    match reply {
        Reply::OptNeg { .. } => "optneg",
        Reply::Continue => "continue",
        Reply::Accept => "accept",
        Reply::Reject => "reject",
        Reply::TempFail => "tempfail",
        Reply::Discard => "discard",
        Reply::ReplyCode(_) => "replycode",
        Reply::Skip => "skip",
        Reply::Progress => "progress",
        Reply::Quarantine(_)
        | Reply::AddHeader { .. }
        | Reply::InsertHeader { .. }
        | Reply::ChangeHeader { .. }
        | Reply::AddRecipient(_)
        | Reply::AddRecipientWithArgs { .. }
        | Reply::DeleteRecipient(_)
        | Reply::ChangeFrom { .. }
        | Reply::ReplaceBody(_) => "modification",
    }
}

/// Whether `commands` records an event for the command `code`.
pub(crate) fn traced(commands: TraceCommands, code: u8) -> bool {
    match commands {
        TraceCommands::None => false,
        TraceCommands::Envelope => matches!(
            code,
            SMFIC_CONNECT
                | SMFIC_HELO
                | SMFIC_MAIL
                | SMFIC_RCPT
                | SMFIC_DATA
                | SMFIC_EOH
                | SMFIC_BODYEOB
                | SMFIC_ABORT
                | SMFIC_UNKNOWN
        ),
        TraceCommands::All => true,
    }
}
