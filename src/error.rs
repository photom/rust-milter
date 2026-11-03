//! The library's error types.

use crate::Actions;

/// The library's error type.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A socket read or write failed.
    #[error("socket error")]
    Io(#[source] std::io::Error),

    /// The MTA sent a malformed frame, an unknown command, or a command in
    /// the wrong state.
    #[error("protocol error")]
    Protocol(#[from] ProtocolError),

    /// A modification method was called without its action negotiated.
    #[error("the action {0:?} was not negotiated")]
    ActionNotNegotiated(Actions),

    /// A callback returned an error. Build it with [`Error::handler`].
    #[error("the handler failed")]
    Handler(#[source] Box<dyn std::error::Error + Send + Sync>),

    /// The MTA closed the connection in the middle of a frame.
    #[error("the connection closed in the middle of a frame")]
    ConnectionClosed,

    /// The MTA reset the connection.
    #[error("the connection was reset")]
    ConnectionReset,

    /// A packet is longer than `per_connection_buffer`.
    #[error("a {length}-byte packet exceeds the {max}-byte limit")]
    FrameTooLarge {
        /// Length of the whole packet, length field included.
        length: usize,
        /// The limit, `per_connection_buffer`.
        max: usize,
    },

    /// A reply cannot be encoded: it breaks the protocol's format rules or
    /// a rule that Sendmail or Postfix enforces.
    #[error("invalid reply")]
    InvalidReply(#[from] ReplyError),

    /// The configuration is invalid.
    #[error("invalid configuration")]
    Config(#[from] ConfigError),

    /// A listener could not bind its address.
    #[error("cannot bind {addr}")]
    Bind {
        /// The address.
        addr: std::net::SocketAddr,
        /// Why the bind failed.
        #[source]
        source: std::io::Error,
    },
}

impl Error {
    /// Wraps a handler's own error as [`Error::Handler`].
    pub fn handler(error: impl std::error::Error + Send + Sync + 'static) -> Error {
        Error::Handler(Box::new(error))
    }
}

/// Why a reply or reply code is invalid.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ReplyError {
    /// The reply code is not 4xx or 5xx.
    #[error("reply code {0} is not 4xx or 5xx")]
    InvalidCode(u16),

    /// The enhanced status code is not of the form `d.ddd.ddd`.
    #[error("the enhanced status code is not of the form d.ddd.ddd")]
    InvalidEnhancedCode,

    /// A line of reply text, an address, ESMTP arguments, or a quarantine
    /// reason holds CR or LF.
    #[error("text that must be one line contains CR or LF")]
    LineBreak,

    /// A line of reply text, with each `%` written as `%%`, is longer than 980
    /// bytes.
    #[error("a {length}-byte line of reply text exceeds 980 bytes")]
    LineTooLong {
        /// Length of the line, in bytes.
        length: usize,
    },

    /// A reply has no lines, or more than 32.
    #[error("a reply has {0} lines; it needs 1 to 32")]
    LineCount(usize),

    /// A text field holds a NUL byte, which would end it early on the wire.
    #[error("a text field contains a NUL byte")]
    NulByte,

    /// A header name is empty or holds a colon. A new header's name must also
    /// be printable ASCII; a delivered header's name must not hold whitespace
    /// or control characters.
    #[error("invalid header name")]
    InvalidHeaderName,

    /// A header value holds CR, or an LF that does not start a folded line of
    /// a space or tab and more than whitespace.
    #[error("a header value contains CR, or LF that does not fold the value")]
    HeaderLineBreak,

    /// An address or quarantine reason is empty.
    #[error("an address or reason is empty")]
    EmptyField,

    /// A body chunk is longer than one frame can carry.
    #[error("a {length}-byte body chunk exceeds 65,535 bytes")]
    BodyChunkTooLarge {
        /// Length of the chunk.
        length: usize,
    },

    /// Reply text breaks the SMTP line rules: each line needs a 4xx or 5xx
    /// code, the same code on every line, then `-` on a line that is not the
    /// last and a space on the last.
    #[error("reply text breaks the SMTP reply line rules")]
    MalformedReplyText,

    /// An enhanced status code, or the text of a line without one, starts with
    /// a digit other than the reply code's first digit. Postfix rejects such a
    /// reply.
    #[error("the enhanced status code does not match the reply code")]
    EnhancedCodeMismatch,

    /// Reply text holds a `%` that is not written as `%%`.
    #[error("reply text contains a single %; write it as %%")]
    UnescapedPercent,
}

/// A violation of the Milter protocol by the MTA.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProtocolError {
    /// A packet announced a length of 0.
    #[error("a packet announced a length of 0")]
    ZeroLength,

    /// A command's data does not match its format, such as a missing null
    /// terminator or a short option negotiation.
    #[error("malformed data in command {}", char::from(*.command))]
    Malformed {
        /// The command code.
        command: u8,
    },

    /// An unknown command code.
    #[error("unknown command code 0x{0:02x}")]
    UnknownCommand(u8),

    /// A command in a state where it closes the connection (ADR 0001).
    #[error("command {} in a wrong state", char::from(*.command))]
    WrongState {
        /// The command code.
        command: u8,
    },

    /// The MTA offered a protocol version below 2.
    #[error("protocol version {0} is below 2")]
    VersionTooOld(u32),

    /// The MTA did not offer every action in `Handler::actions()`.
    #[error("the MTA did not offer the actions {0:?}")]
    ActionsNotOffered(Actions),
}

/// Why a configuration is invalid.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// `bind_addresses` is empty.
    #[error("bind_addresses is empty")]
    MissingBindAddress,

    /// A value is outside its range in `docs/reference/configuration.md`.
    #[error("{field} is {value}; allowed: {allowed}")]
    OutOfRange {
        /// The setting's path, such as `timeouts.connect`.
        field: &'static str,
        /// The rejected value, as text.
        value: String,
        /// The allowed range.
        allowed: &'static str,
    },

    /// The same address appears twice in `bind_addresses`.
    #[error("{0} appears twice in bind_addresses")]
    DuplicateBindAddress(std::net::SocketAddr),

    /// A rule across several settings failed.
    #[error("{rule}: {detail}")]
    Inconsistent {
        /// The rule, such as `max_fds`.
        rule: &'static str,
        /// What the values are and what the rule needs.
        detail: String,
    },

    /// The platform does not support a setting.
    #[error("{field} is not supported: {reason}")]
    Unsupported {
        /// The setting's path.
        field: &'static str,
        /// Why the platform cannot apply it.
        reason: &'static str,
    },

    /// An environment variable with the configured prefix names no setting.
    #[error("{0} names no setting")]
    UnknownEnvVariable(String),

    /// An environment variable's value cannot be parsed. The error names the
    /// variable and the reason but never repeats the value.
    #[error("{variable}: {reason}")]
    InvalidEnvValue {
        /// The variable's name.
        variable: String,
        /// Why the value was rejected.
        reason: String,
    },
}

/// Why a bind address could not be parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AddressError {
    /// The text is not an address in a supported form. `unix:` and `local:`
    /// addresses are not supported, nor is Postfix's `inet:host:port`.
    #[error("not an address of the form host:port, inet:port@host, or inet6:port@host")]
    InvalidFormat,

    /// The host is not an IP address of the form's family, or its scope ID is
    /// not a number.
    #[error("the host is not an IP address")]
    InvalidHost,

    /// The port is not a number from 1 to 65535.
    #[error("the port is not a number from 1 to 65535")]
    InvalidPort,
}

/// `std::result::Result` with the library's [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
