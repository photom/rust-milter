//! A test message with a unique token, so that the sink can tell which test
//! sent what it receives.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A message to send through an MTA.
#[derive(Debug, Clone)]
pub struct Mail {
    /// Unique to this message; it is in the `Message-ID` and the `Subject`.
    pub token: String,
    /// The envelope sender.
    pub from: String,
    /// The envelope recipients.
    pub to: Vec<String>,
    headers: Vec<(String, Vec<u8>)>,
    body: Vec<u8>,
}

impl Mail {
    /// A short message from `sender@void.invalid` to `rcpt@void.invalid`.
    pub fn new() -> Mail {
        let since_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("the clock is after 1970");
        let token = format!(
            "int-{}-{}-{}",
            std::process::id(),
            since_epoch.as_millis(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        Mail {
            token,
            from: "sender@void.invalid".to_owned(),
            to: vec!["rcpt@void.invalid".to_owned()],
            headers: Vec::new(),
            body: b"A test message.\r\n".to_vec(),
        }
    }

    /// Replaces the envelope recipients.
    pub fn to(mut self, recipients: &[&str]) -> Mail {
        self.to = Vec::new();
        for recipient in recipients {
            self.to.push((*recipient).to_owned());
        }
        self
    }

    /// Adds a header after the standard ones. The value may hold any bytes.
    pub fn header(mut self, name: &str, value: impl Into<Vec<u8>>) -> Mail {
        self.headers.push((name.to_owned(), value.into()));
        self
    }

    /// Replaces the body. Lines end with CRLF.
    pub fn body(mut self, body: impl Into<Vec<u8>>) -> Mail {
        self.body = body.into();
        self
    }

    /// The message as sent after DATA, before dot-stuffing.
    pub fn content(&self) -> Vec<u8> {
        let mut content = format!(
            "From: <{}>\r\nTo: <{}>\r\nSubject: {}\r\nMessage-ID: <{}@void.invalid>\r\n",
            self.from,
            self.to.join(">, <"),
            self.token,
            self.token
        )
        .into_bytes();
        for (name, value) in &self.headers {
            content.extend_from_slice(name.as_bytes());
            content.extend_from_slice(b": ");
            content.extend_from_slice(value);
            content.extend_from_slice(b"\r\n");
        }
        content.extend_from_slice(b"\r\n");
        content.extend_from_slice(&self.body);
        content
    }
}

impl Default for Mail {
    fn default() -> Mail {
        Mail::new()
    }
}
