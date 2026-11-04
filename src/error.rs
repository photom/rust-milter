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

    /// A connection timeout expired.
    #[error("the {0} timeout expired")]
    Timeout(TimeoutKind),

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

    /// A resource limit was reached.
    #[error("the {0} limit was reached")]
    ResourceLimit(ResourceKind),

    /// `run()` could not register the handlers for the shutdown signals and
    /// `SIGHUP`.
    #[error("cannot register the signal handlers")]
    Signals(#[source] std::io::Error),

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

    /// The variant's name, the value of the `error.type` attribute.
    pub(crate) fn variant(&self) -> &'static str {
        match self {
            Error::Io(_) => "Io",
            Error::Protocol(_) => "Protocol",
            Error::ActionNotNegotiated(_) => "ActionNotNegotiated",
            Error::Handler(_) => "Handler",
            Error::Timeout(_) => "Timeout",
            Error::ConnectionClosed => "ConnectionClosed",
            Error::ConnectionReset => "ConnectionReset",
            Error::FrameTooLarge { .. } => "FrameTooLarge",
            Error::InvalidReply(_) => "InvalidReply",
            Error::Config(_) => "Config",
            Error::ResourceLimit(_) => "ResourceLimit",
            Error::Signals(_) => "Signals",
            Error::Bind { .. } => "Bind",
        }
    }
}

/// Which connection timeout expired; see `TimeoutConfig`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TimeoutKind {
    /// `timeouts.connect`: no first command after `accept()`.
    Connect,
    /// `timeouts.read`: a frame did not complete in time.
    Read,
    /// `timeouts.write`: a reply could not be written in time.
    Write,
    /// `timeouts.idle`: no next command in time.
    Idle,
}

impl std::fmt::Display for TimeoutKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            TimeoutKind::Connect => "connect",
            TimeoutKind::Read => "read",
            TimeoutKind::Write => "write",
            TimeoutKind::Idle => "idle",
        })
    }
}

/// Which resource limit was reached; see `ResourceLimitsConfig`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ResourceKind {
    /// `max_connections`.
    MaxConnections,
    /// `resource_limits.max_fds`.
    MaxFds,
    /// `resource_limits.max_memory`.
    MaxMemory,
}

impl std::fmt::Display for ResourceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ResourceKind::MaxConnections => "max_connections",
            ResourceKind::MaxFds => "max_fds",
            ResourceKind::MaxMemory => "max_memory",
        })
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

/// Why a reload failed. Every variant leaves the server running with the
/// previous configuration generation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReloadError {
    /// `reload()` was called without a registered loader.
    #[error("no reload loader is registered")]
    NoLoader,

    /// The application's loader failed.
    #[error("the reload loader failed")]
    Load(#[source] Box<dyn std::error::Error + Send + Sync>),

    /// The new configuration is invalid.
    #[error("invalid configuration")]
    Validation(#[source] ConfigError),

    /// The new configuration changes settings that require a restart;
    /// nothing was applied.
    #[error("changing {} requires a restart; nothing was applied", .settings.join(", "))]
    RestartRequired {
        /// The changed settings, in the order of the configuration
        /// reference.
        settings: Vec<&'static str>,
    },

    /// A new listener could not bind in `FailFast` mode; the reload was
    /// rolled back.
    #[error("cannot bind {addr}")]
    Bind {
        /// The address.
        addr: std::net::SocketAddr,
        /// Why the bind failed.
        #[source]
        source: std::io::Error,
    },

    /// The new configuration would leave no listener.
    #[error("the new configuration leaves no listener")]
    NoListener,

    /// A shutdown has started.
    #[error("the server is shutting down")]
    ShuttingDown,
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
