//! Reading frames from the MTA and sending replies to it.

use bytes::BytesMut;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::{Error, Frame, Reply, Result};

/// The receiving side of a connection.
pub(crate) struct Inbound<R> {
    reader: R,
    buf: BytesMut,
    /// The longest packet accepted, `per_connection_buffer`.
    max: usize,
}

impl<R: AsyncRead + Unpin> Inbound<R> {
    pub(crate) fn new(reader: R, max: usize) -> Self {
        Inbound {
            reader,
            buf: BytesMut::with_capacity(8 * 1024),
            max,
        }
    }

    /// The next frame, or `None` at end of file between frames.
    ///
    /// Cancel safe: the received bytes stay in the buffer, so a cancelled
    /// call loses nothing.
    pub(crate) async fn next_frame(&mut self) -> Result<Option<Frame>> {
        loop {
            if let Some(frame) = Frame::parse(&mut self.buf, self.max)? {
                return Ok(Some(frame));
            }
            let read = self
                .reader
                .read_buf(&mut self.buf)
                .await
                .map_err(io_error)?;
            if read == 0 {
                if self.buf.is_empty() {
                    return Ok(None);
                }
                return Err(Error::ConnectionClosed);
            }
        }
    }
}

/// The sending side of a connection.
pub(crate) struct Outbound {
    writer: Box<dyn AsyncWrite + Send + Unpin>,
    buf: BytesMut,
}

impl Outbound {
    pub(crate) fn new(writer: impl AsyncWrite + Send + Unpin + 'static) -> Self {
        Outbound {
            writer: Box::new(writer),
            buf: BytesMut::new(),
        }
    }

    /// Encodes and sends one reply.
    pub(crate) async fn send(&mut self, reply: &Reply) -> Result<()> {
        let frame = reply.to_frame()?;
        self.buf.clear();
        frame.write_to(&mut self.buf);
        self.writer.write_all(&self.buf).await.map_err(io_error)?;
        self.writer.flush().await.map_err(io_error)
    }
}

/// Tells a reset apart from other socket errors.
fn io_error(error: std::io::Error) -> Error {
    use std::io::ErrorKind;
    match error.kind() {
        ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted | ErrorKind::BrokenPipe => {
            Error::ConnectionReset
        }
        _ => Error::Io(error),
    }
}
