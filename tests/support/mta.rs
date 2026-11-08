//! A scripted fake MTA: it sends commands as frames and reads the replies.

use std::net::SocketAddr;
use std::time::Duration;

use bytes::{BufMut, Bytes, BytesMut};
use rust_milter::Frame;
use rust_milter::protocol::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// How long a test waits for a reply or for the connection to close.
pub const WAIT: Duration = Duration::from_secs(5);

/// The largest reply frame the fake MTA reads.
const MAX_REPLY: usize = 1 << 20;

/// The states of `docs/reference/milter-protocol.md` the fake MTA can bring
/// a connection to with a default handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    Conn,
    Helo,
    Mail,
    Rcpt,
    Data,
    Hdrs,
    Eohs,
    Body,
    Endm,
    Unkn,
}

impl Reach {
    /// The states with a message open.
    pub const IN_MESSAGE: [Reach; 6] = [
        Reach::Mail,
        Reach::Rcpt,
        Reach::Data,
        Reach::Hdrs,
        Reach::Eohs,
        Reach::Body,
    ];
}

/// One connection of the fake MTA.
pub struct Mta {
    stream: TcpStream,
    buf: BytesMut,
}

impl Mta {
    /// Connects, retrying while the server under test starts listening.
    pub async fn connect(addr: SocketAddr) -> Mta {
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            match TcpStream::connect(addr).await {
                Ok(stream) => {
                    return Mta {
                        stream,
                        buf: BytesMut::new(),
                    };
                }
                Err(_) if tokio::time::Instant::now() < deadline => {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                Err(error) => panic!("cannot connect to {addr}: {error}"),
            }
        }
    }

    /// Connects with a 4 KB receive buffer, so the milter's writes block
    /// soon after the fake MTA stops reading.
    pub async fn connect_with_small_receive_buffer(addr: SocketAddr) -> Mta {
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            let socket = tokio::net::TcpSocket::new_v4().expect("cannot create a socket");
            socket
                .set_recv_buffer_size(4 * 1024)
                .expect("cannot set SO_RCVBUF");
            match socket.connect(addr).await {
                Ok(stream) => {
                    return Mta {
                        stream,
                        buf: BytesMut::new(),
                    };
                }
                Err(_) if tokio::time::Instant::now() < deadline => {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                Err(error) => panic!("cannot connect to {addr}: {error}"),
            }
        }
    }

    /// Sends one frame.
    pub async fn send(&mut self, command: u8, data: impl Into<Bytes>) {
        let mut out = BytesMut::new();
        Frame::new(command, data.into()).write_to(&mut out);
        self.write_raw(&out).await;
    }

    /// Writes raw bytes, such as half of a frame.
    pub async fn write_raw(&mut self, bytes: &[u8]) {
        self.stream
            .write_all(bytes)
            .await
            .expect("the write to the milter failed");
    }

    /// The next frame from the milter, or `None` when the connection closed.
    pub async fn recv(&mut self) -> Option<Frame> {
        tokio::time::timeout(WAIT, self.read_frame())
            .await
            .expect("no reply from the milter in time")
    }

    async fn read_frame(&mut self) -> Option<Frame> {
        loop {
            if let Some(frame) = Frame::parse(&mut self.buf, MAX_REPLY).expect("bad reply frame") {
                return Some(frame);
            }
            match self.stream.read_buf(&mut self.buf).await {
                Ok(0) | Err(_) => return None,
                Ok(_) => {}
            }
        }
    }

    /// Sends a command and returns the milter's reply.
    pub async fn ask(&mut self, command: u8, data: impl Into<Bytes>) -> Frame {
        self.send(command, data).await;
        self.recv().await.expect("the milter closed the connection")
    }

    /// Sends a command and checks that the reply is `SMFIR_CONTINUE`.
    pub async fn ask_continue(&mut self, command: u8, data: impl Into<Bytes>) {
        let reply = self.ask(command, data).await;
        assert_eq!(
            char::from(reply.command),
            char::from(SMFIR_CONTINUE),
            "reply to {}",
            char::from(command)
        );
    }

    /// Writes raw bytes and ignores a failure, for a milter that may have
    /// closed the connection already.
    pub async fn try_write_raw(&mut self, bytes: &[u8]) -> bool {
        self.stream.write_all(bytes).await.is_ok()
    }

    /// Whether the milter closes the connection without sending anything
    /// more.
    pub async fn closes(&mut self) -> bool {
        self.recv().await.is_none()
    }

    /// Option negotiation with the given offer; `None` when the milter closes
    /// the connection instead of replying.
    pub async fn optneg(&mut self, version: u32, actions: u32, protocol: u32) -> Option<OptNeg> {
        self.send(SMFIC_OPTNEG, optneg_data(version, actions, protocol))
            .await;
        let reply = self.recv().await?;
        assert_eq!(reply.command, SMFIC_OPTNEG);
        Some(OptNeg::decode(&reply.data))
    }

    /// Option negotiation offering version 6, every action, and every flag.
    pub async fn negotiate(&mut self) -> OptNeg {
        self.optneg(6, SMFI_CURR_ACTS, SMFI_CURR_PROT)
            .await
            .expect("the milter closed the connection during option negotiation")
    }

    /// Option negotiation, then commands that each get `SMFIR_CONTINUE`,
    /// until the connection is in `state`.
    pub async fn reach(&mut self, state: Reach) {
        self.negotiate().await;
        self.reach_after_negotiation(state).await;
    }

    /// Like [`Mta::reach`], on a connection that has negotiated.
    pub async fn reach_after_negotiation(&mut self, state: Reach) {
        self.ask_continue(
            SMFIC_CONNECT,
            connect_data("[192.0.2.1]", "192.0.2.1", 4711),
        )
        .await;
        if state == Reach::Conn {
            return;
        }
        self.ask_continue(SMFIC_HELO, strings(&["client.void.invalid"]))
            .await;
        match state {
            Reach::Helo => return,
            Reach::Unkn => {
                self.ask_continue(SMFIC_UNKNOWN, strings(&["NOOP"])).await;
                return;
            }
            _ => {}
        }
        let steps: [(Reach, u8, Bytes); 7] = [
            (Reach::Mail, SMFIC_MAIL, strings(&["<sender@void.invalid>"])),
            (Reach::Rcpt, SMFIC_RCPT, strings(&["<rcpt@void.invalid>"])),
            (Reach::Data, SMFIC_DATA, Bytes::new()),
            (Reach::Hdrs, SMFIC_HEADER, strings(&["Subject", "test"])),
            (Reach::Eohs, SMFIC_EOH, Bytes::new()),
            (Reach::Body, SMFIC_BODY, Bytes::from_static(b"body\r\n")),
            (Reach::Endm, SMFIC_BODYEOB, Bytes::new()),
        ];
        for (reached, command, data) in steps {
            self.ask_continue(command, data).await;
            if reached == state {
                return;
            }
        }
    }

    /// Resets the connection: closes the socket with a linger time of 0, so
    /// the milter's next read fails with a reset.
    pub fn reset(self) {
        let socket = socket2::SockRef::from(&self.stream);
        socket
            .set_linger(Some(Duration::ZERO))
            .expect("cannot set SO_LINGER");
        drop(self);
    }

    /// Closes the connection normally.
    pub async fn close(mut self) {
        let _ = self.stream.shutdown().await;
    }
}

/// The milter's answer to option negotiation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptNeg {
    pub version: u32,
    pub actions: u32,
    pub protocol: u32,
}

impl OptNeg {
    fn decode(data: &[u8]) -> OptNeg {
        OptNeg {
            version: field(data, 0),
            actions: field(data, 1),
            protocol: field(data, 2),
        }
    }
}

/// The `index`th 32-bit field of option negotiation data.
fn field(data: &[u8], index: usize) -> u32 {
    let start = index * 4;
    u32::from_be_bytes(data[start..start + 4].try_into().unwrap())
}

/// `SMFIC_OPTNEG` data.
pub fn optneg_data(version: u32, actions: u32, protocol: u32) -> Bytes {
    let mut data = BytesMut::new();
    data.put_u32(version);
    data.put_u32(actions);
    data.put_u32(protocol);
    data.freeze()
}

/// NUL-terminated strings, one after another.
pub fn strings(texts: &[&str]) -> Bytes {
    let mut data = BytesMut::new();
    for text in texts {
        data.put_slice(text.as_bytes());
        data.put_u8(0);
    }
    data.freeze()
}

/// `SMFIC_CONNECT` data for an IPv4 client.
pub fn connect_data(hostname: &str, address: &str, port: u16) -> Bytes {
    let mut data = BytesMut::new();
    data.put_slice(hostname.as_bytes());
    data.put_u8(0);
    data.put_u8(SMFIA_INET);
    data.put_u16(port);
    data.put_slice(address.as_bytes());
    data.put_u8(0);
    data.freeze()
}

/// `SMFIC_MACRO` data: the stage byte, then name and value pairs.
pub fn macro_data(stage: u8, pairs: &[(&str, &str)]) -> Bytes {
    let mut data = BytesMut::new();
    data.put_u8(stage);
    for (name, value) in pairs {
        data.put_slice(name.as_bytes());
        data.put_u8(0);
        data.put_slice(value.as_bytes());
        data.put_u8(0);
    }
    data.freeze()
}
