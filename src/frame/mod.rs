//! Milter protocol frames: a 4-byte length, a command byte, then the data.

mod parser;
mod writer;

use bytes::Bytes;

/// A Milter protocol frame.
///
/// On the wire a frame is a 4-byte length in network byte order, the command
/// byte, and the data. The length counts the command byte and the data.
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
