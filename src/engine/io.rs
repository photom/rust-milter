//! Reading frames from the MTA and sending replies to it, with the
//! connection timeouts of ADR 0006.

use std::time::Duration;

use bytes::BytesMut;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::time::Instant;

use crate::server::Reservation;
use crate::{Error, Frame, Reply, ResourceKind, Result, TimeoutConfig, TimeoutKind};

/// Size of the length field that starts every frame.
const LENGTH_FIELD: usize = 4;

/// The receiving side of a connection.
pub(crate) struct Inbound<R> {
    reader: R,
    buf: BytesMut,
    /// The longest packet accepted, `per_connection_buffer`.
    max: usize,
    timeouts: TimeoutConfig,
    /// When `accept()` returned; the connect timeout runs from there.
    accepted_at: Instant,
    /// No command has arrived yet.
    first: bool,
    /// When the first byte of the frame in `buf` arrived.
    frame_started: Option<Instant>,
    /// When the wait for the next command began.
    idle_since: Option<Instant>,
    /// This connection's share of `max_memory`.
    memory: Reservation,
    /// Bytes received.
    pub(crate) bytes_read: u64,
    /// Frames received.
    pub(crate) frames: u64,
}

impl<R: AsyncRead + Unpin> Inbound<R> {
    pub(crate) fn new(
        reader: R,
        max: usize,
        timeouts: TimeoutConfig,
        accepted_at: Instant,
        memory: Reservation,
    ) -> Self {
        Inbound {
            reader,
            buf: BytesMut::with_capacity(8 * 1024),
            max,
            timeouts,
            accepted_at,
            first: true,
            frame_started: None,
            idle_since: None,
            memory,
            bytes_read: 0,
            frames: 0,
        }
    }

    /// The next frame, or `None` at end of file between frames.
    ///
    /// Before the first command the connect timeout applies, while waiting
    /// for a later command the idle timeout, and from the first byte of a
    /// frame until it is complete the read timeout.
    ///
    /// Cancel safe: the received bytes and the timers stay in `self`, so a
    /// cancelled call loses nothing.
    pub(crate) async fn next_frame(&mut self) -> Result<Option<Frame>> {
        loop {
            if let Some(frame) = Frame::parse(&mut self.buf, self.max)? {
                self.first = false;
                self.frames += 1;
                self.frame_started = (!self.buf.is_empty()).then(Instant::now);
                // The frame's data is the handler's now; only the next frame
                // needs room in the buffer.
                self.memory.resize(0);
                return Ok(Some(frame));
            }
            self.reserve_frame()?;
            let (deadline, kind) = self.deadline();
            let was_empty = self.buf.is_empty();
            let read = tokio::time::timeout_at(deadline, self.reader.read_buf(&mut self.buf))
                .await
                .map_err(|_| Error::Timeout(kind))?
                .map_err(io_error)?;
            self.bytes_read += read as u64;
            if read == 0 {
                if self.buf.is_empty() {
                    return Ok(None);
                }
                return Err(Error::ConnectionClosed);
            }
            if was_empty {
                self.idle_since = None;
                self.frame_started = Some(Instant::now());
            }
        }
    }

    /// Once a frame's length field has arrived, reserves the whole frame
    /// from `max_memory` and makes room for it, before its data arrives.
    fn reserve_frame(&mut self) -> Result<()> {
        let Some(length_field) = self.buf.first_chunk::<LENGTH_FIELD>() else {
            return Ok(());
        };
        // `Frame::parse` has checked the length against `max` already.
        let packet = LENGTH_FIELD + u32::from_be_bytes(*length_field) as usize;
        if !self.memory.resize(packet) {
            return Err(Error::ResourceLimit(ResourceKind::MaxMemory));
        }
        self.buf.reserve(packet.saturating_sub(self.buf.len()));
        Ok(())
    }

    /// When the current wait ends, and which timeout that is.
    fn deadline(&mut self) -> (Instant, TimeoutKind) {
        let connect = (
            self.accepted_at + self.timeouts.connect,
            TimeoutKind::Connect,
        );
        if self.buf.is_empty() {
            if self.first {
                return connect;
            }
            let since = *self.idle_since.get_or_insert_with(Instant::now);
            return (since + self.timeouts.idle, TimeoutKind::Idle);
        }
        let started = *self.frame_started.get_or_insert_with(Instant::now);
        let read = (started + self.timeouts.read, TimeoutKind::Read);
        // The connect timeout lasts until the first command is complete.
        if self.first && connect.0 < read.0 {
            connect
        } else {
            read
        }
    }
}

/// The sending side of a connection.
pub(crate) struct Outbound {
    writer: Box<dyn AsyncWrite + Send + Unpin>,
    buf: BytesMut,
    /// `timeouts.write`, for each reply.
    timeout: Duration,
    /// Bytes sent.
    pub(crate) bytes_written: u64,
}

impl Outbound {
    pub(crate) fn new(writer: impl AsyncWrite + Send + Unpin + 'static, timeout: Duration) -> Self {
        Outbound {
            writer: Box::new(writer),
            buf: BytesMut::new(),
            timeout,
            bytes_written: 0,
        }
    }

    /// Encodes and sends one reply within the write timeout.
    pub(crate) async fn send(&mut self, reply: &Reply) -> Result<()> {
        let frame = reply.to_frame()?;
        self.buf.clear();
        frame.write_to(&mut self.buf);
        let write = async {
            self.writer.write_all(&self.buf).await?;
            self.writer.flush().await
        };
        tokio::time::timeout(self.timeout, write)
            .await
            .map_err(|_| Error::Timeout(TimeoutKind::Write))?
            .map_err(io_error)?;
        self.bytes_written += self.buf.len() as u64;
        Ok(())
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
