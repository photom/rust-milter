//! The MTA side of the protocol: one connection of the load generator.

use std::fmt;
use std::net::SocketAddr;
use std::time::Duration;

use bytes::{BufMut, Bytes, BytesMut};
use rust_milter::Frame;
use rust_milter::protocol::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// How long the generator waits for one reply.
pub const REPLY_WAIT: Duration = Duration::from_secs(10);

/// The largest reply frame the generator reads.
const MAX_REPLY: usize = 1 << 20;

/// Why a connection or a session failed.
#[derive(Debug)]
pub enum Failure {
    /// The connection could not be opened, read, or written.
    Io(std::io::Error),
    /// The milter closed the connection before it replied.
    Closed,
    /// No reply came within [`REPLY_WAIT`].
    Timeout,
    /// The milter sent a frame the generator cannot parse.
    BadFrame(rust_milter::Error),
    /// The milter answered `command` with `reply` instead of the expected
    /// reply.
    Unexpected {
        /// The command sent.
        command: u8,
        /// The reply received.
        reply: u8,
    },
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Failure::Io(error) => write!(f, "I/O error: {error}"),
            Failure::Closed => write!(f, "the milter closed the connection"),
            Failure::Timeout => write!(f, "no reply within {REPLY_WAIT:?}"),
            Failure::BadFrame(error) => write!(f, "bad reply frame: {error}"),
            Failure::Unexpected { command, reply } => write!(
                f,
                "the milter answered {} with {}",
                char::from(*command),
                char::from(*reply)
            ),
        }
    }
}

/// One connection to the milter under test, as an MTA opens it.
pub struct Mta {
    stream: TcpStream,
    buf: BytesMut,
}

impl Mta {
    /// Opens a connection.
    pub async fn connect(address: SocketAddr) -> Result<Mta, Failure> {
        match TcpStream::connect(address).await {
            Ok(stream) => Ok(Mta {
                stream,
                buf: BytesMut::new(),
            }),
            Err(error) => Err(Failure::Io(error)),
        }
    }

    /// Option negotiation, offering version 6, every action, and every
    /// protocol flag, as the fake MTA of `cargo test` does.
    pub async fn negotiate(&mut self) -> Result<(), Failure> {
        let mut data = BytesMut::with_capacity(12);
        data.put_u32(6);
        data.put_u32(SMFI_CURR_ACTS);
        data.put_u32(SMFI_CURR_PROT);
        self.expect(SMFIC_OPTNEG, data.freeze(), SMFIC_OPTNEG).await
    }

    /// One SMTP session with one message, from `SMFIC_CONNECT` to end of
    /// message. Every command must get `SMFIR_CONTINUE`.
    pub async fn session(&mut self) -> Result<(), Failure> {
        let steps: [(u8, Bytes); 9] = [
            (SMFIC_CONNECT, connect_data()),
            (SMFIC_HELO, strings(&["client.void.invalid"])),
            (SMFIC_MAIL, strings(&["<sender@void.invalid>"])),
            (SMFIC_RCPT, strings(&["<rcpt@void.invalid>"])),
            (SMFIC_DATA, Bytes::new()),
            (SMFIC_HEADER, strings(&["Subject", "load"])),
            (SMFIC_EOH, Bytes::new()),
            (SMFIC_BODY, Bytes::from_static(b"body\r\n")),
            (SMFIC_BODYEOB, Bytes::new()),
        ];
        for (command, data) in steps {
            self.expect(command, data, SMFIR_CONTINUE).await?;
        }
        Ok(())
    }

    /// Sends `SMFIC_QUIT`, which gets no reply, and closes the connection.
    pub async fn quit(mut self) -> Result<(), Failure> {
        self.send(SMFIC_QUIT, Bytes::new()).await?;
        if let Err(error) = self.stream.shutdown().await {
            return Err(Failure::Io(error));
        }
        Ok(())
    }

    /// Sends one frame.
    async fn send(&mut self, command: u8, data: Bytes) -> Result<(), Failure> {
        let mut out = BytesMut::new();
        Frame::new(command, data).write_to(&mut out);
        match self.stream.write_all(&out).await {
            Ok(()) => Ok(()),
            Err(error) => Err(Failure::Io(error)),
        }
    }

    /// Sends `command` and checks that the reply is `reply`.
    async fn expect(&mut self, command: u8, data: Bytes, reply: u8) -> Result<(), Failure> {
        self.send(command, data).await?;
        let Ok(received) = tokio::time::timeout(REPLY_WAIT, self.read_frame()).await else {
            return Err(Failure::Timeout);
        };
        let frame = received?;
        if frame.command != reply {
            return Err(Failure::Unexpected {
                command,
                reply: frame.command,
            });
        }
        Ok(())
    }

    /// The next frame from the milter.
    async fn read_frame(&mut self) -> Result<Frame, Failure> {
        loop {
            match Frame::parse(&mut self.buf, MAX_REPLY) {
                Ok(Some(frame)) => return Ok(frame),
                Ok(None) => {}
                Err(error) => return Err(Failure::BadFrame(error)),
            }
            match self.stream.read_buf(&mut self.buf).await {
                Ok(0) => return Err(Failure::Closed),
                Ok(_) => {}
                Err(error) => return Err(Failure::Io(error)),
            }
        }
    }
}

/// NUL-terminated strings, one after another.
fn strings(texts: &[&str]) -> Bytes {
    let mut data = BytesMut::new();
    for text in texts {
        data.put_slice(text.as_bytes());
        data.put_u8(0);
    }
    data.freeze()
}

/// `SMFIC_CONNECT` data for an IPv4 client in the documentation range.
fn connect_data() -> Bytes {
    let mut data = BytesMut::new();
    data.put_slice(b"[192.0.2.1]\0");
    data.put_u8(SMFIA_INET);
    data.put_u16(4711);
    data.put_slice(b"192.0.2.1\0");
    data.freeze()
}
