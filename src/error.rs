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
}

/// A violation of the Milter protocol by the MTA.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProtocolError {
    /// A packet announced a length of 0.
    #[error("a packet announced a length of 0")]
    ZeroLength,
}

/// `std::result::Result` with the library's [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
