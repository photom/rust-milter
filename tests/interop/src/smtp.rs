//! The tests' SMTP client. It prints the conversation, so the output of a
//! failing test shows what the MTA answered.

use std::fmt;
use std::time::Instant;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

use crate::{Mail, Mta, STARTUP, WAIT, text};

/// An SMTP reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    /// The three-digit code.
    pub code: u16,
    /// Every line, with its code and without the line break.
    pub lines: Vec<String>,
}

impl Reply {
    /// The code is 2xx or 3xx.
    pub fn is_positive(&self) -> bool {
        self.code < 400
    }
}

impl fmt::Display for Reply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.lines.join(" | "))
    }
}

/// The replies to one mail transaction.
#[derive(Debug, Clone)]
pub struct Sent {
    /// The reply to MAIL.
    pub mail: Reply,
    /// The replies to RCPT, one per recipient. Empty if MAIL failed.
    pub rcpts: Vec<Reply>,
    /// The final reply to DATA: to the end of the message, or to the DATA
    /// command if it was not answered with 354. `None` if no recipient was
    /// accepted, so that DATA was not sent.
    pub data: Option<Reply>,
}

impl Sent {
    /// The MTA accepted the message with 250.
    pub fn accepted(&self) -> bool {
        match &self.data {
            Some(reply) => reply.code == 250,
            None => false,
        }
    }
}

/// An SMTP session with an MTA.
pub struct Smtp {
    mta: Mta,
    read: BufReader<OwnedReadHalf>,
    write: OwnedWriteHalf,
    /// The MTA's greeting.
    pub greeting: Reply,
}

impl Smtp {
    /// Connects to `mta` on port 25 and reads its greeting.
    pub async fn connect(mta: Mta) -> Smtp {
        Smtp::connect_port(mta, 25).await
    }

    /// Connects to `mta` on `port` and reads its greeting. Retries for up to
    /// [`STARTUP`], because the MTA may still be starting when the first test
    /// runs.
    pub async fn connect_port(mta: Mta, port: u16) -> Smtp {
        let deadline = Instant::now() + STARTUP;
        let stream = loop {
            match TcpStream::connect((mta.host(), port)).await {
                Ok(stream) => break stream,
                Err(error) => {
                    assert!(
                        Instant::now() < deadline,
                        "{mta} port {port} accepts connections in time: {error}"
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            }
        };
        let (read, write) = stream.into_split();
        let mut smtp = Smtp {
            mta,
            read: BufReader::new(read),
            write,
            greeting: Reply {
                code: 0,
                lines: Vec::new(),
            },
        };
        smtp.greeting = smtp.read_reply().await;
        smtp
    }

    /// Sends `mail` in a session of its own on port 25: EHLO, the
    /// transaction, and QUIT.
    pub async fn send_one(mta: Mta, mail: &Mail) -> Sent {
        let mut smtp = Smtp::connect(mta).await;
        smtp.ehlo().await;
        let sent = smtp.send(mail).await;
        smtp.quit().await;
        sent
    }

    /// Sends one command line and reads the reply.
    pub async fn command(&mut self, line: &str) -> Reply {
        println!("smtp {} > {line}", self.mta);
        self.write_all(format!("{line}\r\n").as_bytes()).await;
        self.read_reply().await
    }

    /// Sends EHLO and checks that the MTA accepts it.
    pub async fn ehlo(&mut self) {
        let reply = self.command("EHLO client.void.invalid").await;
        assert_eq!(reply.code, 250, "{} accepts EHLO: {reply}", self.mta);
    }

    /// Sends one mail transaction: MAIL, RCPT for each recipient, and DATA
    /// with the message if a recipient was accepted.
    pub async fn send(&mut self, mail: &Mail) -> Sent {
        let mail_reply = self.command(&format!("MAIL FROM:<{}>", mail.from)).await;
        if !mail_reply.is_positive() {
            return Sent {
                mail: mail_reply,
                rcpts: Vec::new(),
                data: None,
            };
        }
        let mut rcpts = Vec::new();
        for recipient in &mail.to {
            rcpts.push(self.command(&format!("RCPT TO:<{recipient}>")).await);
        }
        let mut any_accepted = false;
        for reply in &rcpts {
            if reply.is_positive() {
                any_accepted = true;
            }
        }
        let data = if any_accepted {
            Some(self.data(&mail.content()).await)
        } else {
            None
        };
        Sent {
            mail: mail_reply,
            rcpts,
            data,
        }
    }

    /// Sends DATA and, after 354, the message with dot-stuffing.
    pub async fn data(&mut self, content: &[u8]) -> Reply {
        let reply = self.command("DATA").await;
        if reply.code != 354 {
            return reply;
        }
        let mut stuffed = Vec::with_capacity(content.len() + 5);
        for line in text::lines_with_ends(content) {
            if line.starts_with(b".") {
                stuffed.push(b'.');
            }
            stuffed.extend_from_slice(line);
        }
        if !stuffed.ends_with(b"\r\n") {
            stuffed.extend_from_slice(b"\r\n");
        }
        stuffed.extend_from_slice(b".\r\n");
        println!("smtp {} > ({} bytes of message)", self.mta, content.len());
        self.write_all(&stuffed).await;
        self.read_reply().await
    }

    /// Sends QUIT and reads the reply.
    pub async fn quit(mut self) -> Reply {
        self.command("QUIT").await
    }

    async fn write_all(&mut self, bytes: &[u8]) {
        let mta = self.mta;
        let written = match tokio::time::timeout(WAIT, self.write.write_all(bytes)).await {
            Ok(written) => written,
            Err(_) => panic!("{mta} did not read the command in time"),
        };
        if let Err(error) = written {
            panic!("writing to {mta} failed: {error}");
        }
    }

    async fn read_reply(&mut self) -> Reply {
        let mta = self.mta;
        let mut lines = Vec::new();
        loop {
            let mut line = Vec::new();
            let read =
                match tokio::time::timeout(WAIT, self.read.read_until(b'\n', &mut line)).await {
                    Ok(Ok(read)) => read,
                    Ok(Err(error)) => panic!("reading from {mta} failed: {error}"),
                    Err(_) => panic!("{mta} did not reply in time; lines so far: {lines:?}"),
                };
            assert!(
                read > 0,
                "{mta} closed the connection; lines so far: {lines:?}"
            );
            let reply_line = String::from_utf8_lossy(&line).trim_end().to_owned();
            println!("smtp {mta} < {reply_line}");
            let code: u16 = match reply_line.get(..3) {
                Some(digits) => match digits.parse() {
                    Ok(code) => code,
                    Err(_) => panic!("{mta} sent no reply code: {reply_line:?}"),
                },
                None => panic!("{mta} sent no reply code: {reply_line:?}"),
            };
            let last = reply_line.as_bytes().get(3) != Some(&b'-');
            lines.push(reply_line);
            if last {
                return Reply { code, lines };
            }
        }
    }
}
