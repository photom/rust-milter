//! The names that spans, command events, and metrics use for commands and
//! replies, which commands `trace_commands` records, and the counters the
//! engine keeps for `status()`. See "Traces" and "Metrics" in
//! `docs/reference/telemetry.md`.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::protocol::*;
use crate::{Reply, TraceCommands};

/// What the engine needs for its spans, events, and counters.
pub(crate) struct Trace {
    /// The server's session counter, for `milter.session.id`.
    pub(crate) sessions: Arc<AtomicU64>,
    /// Which commands become `milter.command` events.
    pub(crate) commands: TraceCommands,
    /// The server's command, reply, byte, and negotiation counts.
    pub(crate) traffic: Arc<Traffic>,
    /// The message histograms.
    #[cfg(feature = "otel")]
    pub(crate) histograms: Arc<crate::server::Histograms>,
}

/// Every command code with its `milter.command` value, in the order of
/// telemetry.md.
pub(crate) const COMMANDS: [(u8, &str); 15] = [
    (SMFIC_OPTNEG, "optneg"),
    (SMFIC_MACRO, "macro"),
    (SMFIC_CONNECT, "connect"),
    (SMFIC_HELO, "helo"),
    (SMFIC_MAIL, "mail"),
    (SMFIC_RCPT, "rcpt"),
    (SMFIC_DATA, "data"),
    (SMFIC_HEADER, "header"),
    (SMFIC_EOH, "eoh"),
    (SMFIC_BODY, "body"),
    (SMFIC_BODYEOB, "eom"),
    (SMFIC_ABORT, "abort"),
    (SMFIC_UNKNOWN, "unknown"),
    (SMFIC_QUIT, "quit"),
    (SMFIC_QUIT_NC, "quit_nc"),
];

/// Every `milter.reply` value of a reply that is sent, in the order of
/// telemetry.md.
pub(crate) const REPLIES: [&str; 10] = [
    "optneg",
    "continue",
    "accept",
    "reject",
    "tempfail",
    "discard",
    "replycode",
    "skip",
    "progress",
    "modification",
];

/// The protocol versions a negotiation can end with.
pub(crate) const VERSIONS: [u32; 5] = [2, 3, 4, 5, 6];

/// The position of a command code in [`COMMANDS`].
pub(crate) fn command_index(code: u8) -> Option<usize> {
    for (index, (known, _)) in COMMANDS.iter().enumerate() {
        if *known == code {
            return Some(index);
        }
    }
    None
}

/// The `milter.command` value of a command code.
pub(crate) fn command_name(code: u8) -> &'static str {
    // `Command::from_frame` rejects every other code before an event.
    match command_index(code) {
        Some(index) => COMMANDS[index].1,
        None => "unknown_command",
    }
}

/// The position of a reply's `milter.reply` value in [`REPLIES`].
fn reply_index(reply: &Reply) -> usize {
    match reply {
        Reply::OptNeg { .. } => 0,
        Reply::Continue => 1,
        Reply::Accept => 2,
        Reply::Reject => 3,
        Reply::TempFail => 4,
        Reply::Discard => 5,
        Reply::ReplyCode(_) => 6,
        Reply::Skip => 7,
        Reply::Progress => 8,
        Reply::Quarantine(_)
        | Reply::AddHeader { .. }
        | Reply::InsertHeader { .. }
        | Reply::ChangeHeader { .. }
        | Reply::AddRecipient(_)
        | Reply::AddRecipientWithArgs { .. }
        | Reply::DeleteRecipient(_)
        | Reply::ChangeFrom { .. }
        | Reply::ReplaceBody(_) => 9,
    }
}

/// The `milter.reply` value of a reply.
pub(crate) fn reply_name(reply: &Reply) -> &'static str {
    REPLIES[reply_index(reply)]
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

/// The counts the connections add to as they run: one atomic increment
/// per event, read by `status()` and the observable instruments.
#[derive(Debug, Default)]
pub(crate) struct Traffic {
    received: AtomicU64,
    sent: AtomicU64,
    negotiations: [AtomicU64; VERSIONS.len()],
    commands: [AtomicU64; COMMANDS.len()],
    replies: [[AtomicU64; REPLIES.len()]; COMMANDS.len()],
}

impl Traffic {
    pub(crate) fn received(&self, bytes: usize) {
        self.received.fetch_add(bytes as u64, Ordering::Relaxed);
    }

    pub(crate) fn sent(&self, bytes: usize) {
        self.sent.fetch_add(bytes as u64, Ordering::Relaxed);
    }

    pub(crate) fn negotiated(&self, version: u32) {
        for (index, known) in VERSIONS.iter().enumerate() {
            if *known == version {
                self.negotiations[index].fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    pub(crate) fn command(&self, index: usize) {
        self.commands[index].fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn reply(&self, command: usize, reply: &Reply) {
        self.replies[command][reply_index(reply)].fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn bytes_received(&self) -> u64 {
        self.received.load(Ordering::Relaxed)
    }

    pub(crate) fn bytes_sent(&self) -> u64 {
        self.sent.load(Ordering::Relaxed)
    }

    /// The negotiations that happened, by version.
    pub(crate) fn negotiations(&self) -> Vec<(u32, u64)> {
        let mut negotiations = Vec::new();
        for (version, count) in VERSIONS.iter().zip(&self.negotiations) {
            let count = count.load(Ordering::Relaxed);
            if count > 0 {
                negotiations.push((*version, count));
            }
        }
        negotiations
    }

    /// The commands received, by command.
    pub(crate) fn commands(&self) -> Vec<(&'static str, u64)> {
        let mut commands = Vec::new();
        for ((_, name), count) in COMMANDS.iter().zip(&self.commands) {
            let count = count.load(Ordering::Relaxed);
            if count > 0 {
                commands.push((*name, count));
            }
        }
        commands
    }

    /// The replies sent, by command and reply.
    pub(crate) fn replies(&self) -> Vec<(&'static str, &'static str, u64)> {
        let mut replies = Vec::new();
        for ((_, command), counts) in COMMANDS.iter().zip(&self.replies) {
            for (reply, count) in REPLIES.iter().zip(counts) {
                let count = count.load(Ordering::Relaxed);
                if count > 0 {
                    replies.push((*command, *reply, count));
                }
            }
        }
        replies
    }
}
