//! Milter protocol frames: a 4-byte length, a command byte, then the data.

mod parser;
mod writer;

use bytes::Bytes;

/// A Milter protocol frame, used by tests and by the planned milter client
/// sample.
///
/// On the wire a frame is a 4-byte length in network byte order, the command
/// byte, and the data. The length counts the command byte and the data.
///
/// The server limits the size of each packet it reads. The whole packet
/// (4 + 1 + data length) must fit in the connection's
/// `per_connection_buffer`, 128 KB by default, and the first packet of a
/// connection, the option negotiation, in 1,024 bytes ([ADR 0013]). A longer
/// packet fails with [`Error::FrameTooLarge`](crate::Error::FrameTooLarge)
/// before any memory is allocated for it. The default holds the largest
/// default-size milter packet, 65,540 bytes. [`Frame::check`] and
/// [`Frame::parse`] apply the limit given as `max`.
///
/// [ADR 0013]: https://github.com/photom/rust-milter/blob/main/docs/adr/0013-the-first-packet-is-limited-to-1-kb.md
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// The command or reply code.
    pub command: u8,
    /// The payload.
    pub data: Bytes,
}

impl Frame {
    /// Creates a frame.
    pub fn new(command: u8, data: Bytes) -> Frame {
        Frame { command, data }
    }
}
