//! The SMTP sink on port 2525 of the runner. Both MTAs relay mail for
//! `void.invalid` to it, and it keeps every message it receives.
//!
//! The sink runs on its own thread for the whole test binary, so that an
//! MTA's delivery never finds it stopped between two tests. A test finds its
//! own messages by the [`Mail::token`](crate::Mail::token) in them.

use std::io;
use std::sync::{Mutex, Once};
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

use crate::{WAIT, text};

/// The port both MTAs deliver to.
pub const PORT: u16 = 2525;

static DELIVERED: Mutex<Vec<Delivered>> = Mutex::new(Vec::new());
static START: Once = Once::new();

/// A message the sink received.
#[derive(Debug, Clone)]
pub struct Delivered {
    /// The envelope sender, without the angle brackets.
    pub from: String,
    /// The envelope recipients, without the angle brackets.
    pub rcpts: Vec<String>,
    /// The message after DATA, with dot-stuffing removed.
    pub data: Vec<u8>,
}

impl Delivered {
    /// The header fields in order. The line breaks of a folded field are
    /// removed, so a value is one line with its original spacing.
    pub fn headers(&self) -> Vec<(String, Vec<u8>)> {
        let mut fields: Vec<(String, Vec<u8>)> = Vec::new();
        for line in text::lines(self.header_section()) {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            let folded = matches!(line.first(), Some(b' ' | b'\t'));
            if folded {
                if let Some((_, value)) = fields.last_mut() {
                    value.extend_from_slice(line);
                }
                continue;
            }
            let Some(colon) = text::find_byte(line, b':') else {
                continue;
            };
            let name = String::from_utf8_lossy(&line[..colon]).into_owned();
            let after_colon = &line[colon + 1..];
            let value = after_colon.strip_prefix(b" ").unwrap_or(after_colon);
            fields.push((name, value.to_vec()));
        }
        fields
    }

    /// The values of the header fields named `name`, in order, compared
    /// without case.
    pub fn header_values(&self, name: &str) -> Vec<Vec<u8>> {
        let mut values = Vec::new();
        for (field_name, value) in self.headers() {
            if field_name.eq_ignore_ascii_case(name) {
                values.push(value);
            }
        }
        values
    }

    /// The first value of the header field `name`, as text.
    pub fn header(&self, name: &str) -> Option<String> {
        let values = self.header_values(name);
        let first = values.first()?;
        Some(String::from_utf8_lossy(first).into_owned())
    }

    /// The body, after the empty line that ends the headers.
    pub fn body(&self) -> &[u8] {
        match text::find(&self.data, b"\r\n\r\n") {
            Some(at) => &self.data[at + 4..],
            None => &[],
        }
    }

    fn header_section(&self) -> &[u8] {
        match text::find(&self.data, b"\r\n\r\n") {
            Some(at) => &self.data[..at],
            None => &self.data,
        }
    }

    fn holds(&self, token: &str) -> bool {
        text::contains(&self.data, token.as_bytes())
    }
}

/// Starts the sink on first use.
pub fn start() {
    START.call_once(|| {
        let (ready, started) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("the sink's runtime starts");
            runtime.block_on(serve(ready));
        });
        started.recv().expect("the sink binds its port");
    });
}

/// Every copy of the message with `token` received so far.
pub fn received(token: &str) -> Vec<Delivered> {
    let delivered = DELIVERED.lock().unwrap();
    let mut copies = Vec::new();
    for copy in delivered.iter() {
        if copy.holds(token) {
            copies.push(copy.clone());
        }
    }
    copies
}

/// Waits for the first copy of the message with `token`.
pub async fn wait_for(token: &str) -> Delivered {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(first) = received(token).into_iter().next() {
            return first;
        }
        assert!(
            Instant::now() < deadline,
            "message {token} delivered in time"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Waits until the copies of the message with `token` together reach every
/// recipient in `recipients`, and returns the recipients they reached,
/// sorted. An MTA may deliver one message in several transactions.
pub async fn wait_for_recipients(token: &str, recipients: &[&str]) -> Vec<String> {
    let deadline = Instant::now() + WAIT;
    loop {
        let mut reached: Vec<String> = Vec::new();
        for copy in received(token) {
            for recipient in copy.rcpts {
                if !reached.contains(&recipient) {
                    reached.push(recipient);
                }
            }
        }
        reached.sort();
        let mut all_reached = true;
        for recipient in recipients {
            if !reached.contains(&(*recipient).to_owned()) {
                all_reached = false;
            }
        }
        if all_reached {
            return reached;
        }
        assert!(
            Instant::now() < deadline,
            "message {token} reached {recipients:?} in time; reached {reached:?}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Checks that no copy of the message with `token` arrives within `period`.
pub async fn assert_none_within(token: &str, period: Duration) {
    tokio::time::sleep(period).await;
    let copies = received(token);
    assert!(
        copies.is_empty(),
        "message {token} was delivered: {copies:?}"
    );
}

async fn serve(ready: std::sync::mpsc::Sender<()>) {
    let listener = TcpListener::bind(("0.0.0.0", PORT))
        .await
        .expect("the sink binds port 2525");
    ready.send(()).expect("the test waits for the sink");
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(async move {
                    if let Err(error) = session(stream).await {
                        println!("sink: session failed: {error}");
                    }
                });
            }
            Err(error) => println!("sink: accept failed: {error}"),
        }
    }
}

async fn session(stream: TcpStream) -> io::Result<()> {
    let (read, mut write) = stream.into_split();
    let mut read = BufReader::new(read);
    write.write_all(b"220 sink.void.invalid ESMTP\r\n").await?;
    let mut from = String::new();
    let mut rcpts = Vec::new();
    loop {
        let mut line = Vec::new();
        if read.read_until(b'\n', &mut line).await? == 0 {
            return Ok(());
        }
        let line = String::from_utf8_lossy(&line).trim_end().to_owned();
        let upper = line.to_ascii_uppercase();
        let reply: &[u8] = if upper.starts_with("EHLO") {
            b"250-sink.void.invalid\r\n250 8BITMIME\r\n"
        } else if upper.starts_with("HELO") {
            b"250 sink.void.invalid\r\n"
        } else if upper.starts_with("MAIL FROM:") {
            from = path(&line["MAIL FROM:".len()..]);
            rcpts.clear();
            b"250 2.1.0 OK\r\n"
        } else if upper.starts_with("RCPT TO:") {
            rcpts.push(path(&line["RCPT TO:".len()..]));
            b"250 2.1.5 OK\r\n"
        } else if upper == "DATA" {
            write
                .write_all(b"354 End data with <CR><LF>.<CR><LF>\r\n")
                .await?;
            let data = read_data(&mut read).await?;
            DELIVERED.lock().unwrap().push(Delivered {
                from: std::mem::take(&mut from),
                rcpts: std::mem::take(&mut rcpts),
                data,
            });
            b"250 2.0.0 OK\r\n"
        } else if upper == "RSET" {
            from.clear();
            rcpts.clear();
            b"250 2.0.0 OK\r\n"
        } else if upper == "NOOP" {
            b"250 2.0.0 OK\r\n"
        } else if upper == "QUIT" {
            write.write_all(b"221 2.0.0 Bye\r\n").await?;
            return Ok(());
        } else {
            b"502 5.5.1 Command not implemented\r\n"
        };
        write.write_all(reply).await?;
    }
}

/// Reads the message after DATA up to the line with a single dot, and
/// removes the dot-stuffing.
async fn read_data(read: &mut BufReader<tokio::net::tcp::OwnedReadHalf>) -> io::Result<Vec<u8>> {
    let mut data = Vec::new();
    loop {
        let mut line = Vec::new();
        if read.read_until(b'\n', &mut line).await? == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        if line == b".\r\n" || line == b".\n" {
            return Ok(data);
        }
        let line = line.strip_prefix(b".").unwrap_or(&line);
        data.extend_from_slice(line);
    }
}

/// The address in a MAIL or RCPT argument, such as `<a@void.invalid> SIZE=10`.
fn path(argument: &str) -> String {
    let argument = argument.trim_start();
    if let Some(rest) = argument.strip_prefix('<') {
        return match rest.find('>') {
            Some(end) => rest[..end].to_owned(),
            None => rest.to_owned(),
        };
    }
    match argument.find(' ') {
        Some(end) => argument[..end].to_owned(),
        None => argument.to_owned(),
    }
}
