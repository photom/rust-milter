//! The library's error types.

/// The library's error type.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The MTA sent a malformed frame, an unknown command, or a command in
    /// the wrong state.
    #[error("protocol error")]
    Protocol(#[from] ProtocolError),

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

    /// A line of reply text holds CR or LF.
    #[error("reply text contains CR or LF")]
    LineBreak,

    /// A line of reply text is longer than 980 bytes.
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

    /// A header name is empty or holds a colon, whitespace, or a control
    /// character.
    #[error("invalid header name")]
    InvalidHeaderName,

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
}

/// Why a bind address could not be parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AddressError {
    /// The text is not an address in a supported form; `unix:` and `local:`
    /// addresses are not supported.
    #[error("not an address of the form host:port, inet:port@host, or inet6:port@host")]
    InvalidFormat,

    /// The host is not an IP address of the form's family.
    #[error("the host is not an IP address")]
    InvalidHost,

    /// The port is not a number from 1 to 65535.
    #[error("the port is not a number from 1 to 65535")]
    InvalidPort,
}

/// `std::result::Result` with the library's [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
