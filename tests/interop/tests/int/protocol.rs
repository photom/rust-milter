//! Protocol details: INT-030 to INT-036. INT-033 and INT-034 are withdrawn.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use rust_milter::{
    ConnectDecision, ConnectInfo, Decision, EomContext, Handler, MessageContext, MilterText,
    Result, SessionContext,
};
use rust_milter_interop::{Logs, Mail, Milter, Mta, Smtp, sink};

/// Records the value of the `{i}` macro at the end of each message.
#[derive(Clone, Default)]
struct RecordsQueueId(Arc<Mutex<Vec<Option<Vec<u8>>>>>);

impl Handler for RecordsQueueId {
    type Session = ();
    type Message = ();

    async fn end_of_message(&self, ctx: &mut EomContext<Self>) -> Result<Decision> {
        let value = match ctx.macro_value("i") {
            Some(value) => Some(value.as_bytes().to_vec()),
            None => None,
        };
        self.0.lock().unwrap().push(value);
        Ok(Decision::Continue)
    }
}

#[tokio::test]
#[ignore = "runs in the interoperability stack"]
async fn int_030_the_queue_id_macro_is_set_at_end_of_message() {
    for mta in Mta::BOTH {
        let logs = Logs::capture();
        let handler = RecordsQueueId::default();
        let seen = Arc::clone(&handler.0);
        let milter = Milter::start(handler).await;
        // The unknown command, the headers, and the body make Postfix send
        // its `U`, `L`, and `B` stage macros as well.
        let mail = Mail::new()
            .header("X-Stage", "headers")
            .body("Line one.\r\nLine two.\r\n");

        let mut smtp = Smtp::connect(mta).await;
        smtp.ehlo().await;
        smtp.command("XUNKNOWN").await;
        let sent = smtp.send(&mail).await;
        smtp.quit().await;

        assert!(sent.accepted(), "{mta}: {sent:?}");
        sink::wait_for(&mail.token).await;
        milter.wait_closed(1).await;
        milter.stop().await;
        let seen = seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 1, "{mta}: {seen:?}");
        match &seen[0] {
            Some(queue_id) => assert!(!queue_id.is_empty(), "{mta}: an empty {{i}}"),
            None => panic!("{mta}: no {{i}} at end of message"),
        }
        let warnings = logs.warnings();
        assert!(warnings.is_empty(), "{mta}: {warnings:?}");
    }
}

/// The client as `connect` saw it.
#[derive(Debug, Clone)]
struct Client {
    hostname: Vec<u8>,
    address: Option<SocketAddr>,
}

/// Records the client from each `connect`.
#[derive(Clone, Default)]
struct RecordsClient(Arc<Mutex<Vec<Client>>>);

impl Handler for RecordsClient {
    type Session = ();
    type Message = ();

    async fn connect(
        &self,
        _ctx: &mut SessionContext<Self>,
        info: &ConnectInfo,
    ) -> Result<ConnectDecision> {
        let client = Client {
            hostname: info.hostname.as_bytes().to_vec(),
            address: info.address,
        };
        self.0.lock().unwrap().push(client);
        Ok(ConnectDecision::Continue)
    }
}

#[tokio::test]
#[ignore = "runs in the interoperability stack"]
async fn int_031_an_unresolved_client_arrives_as_its_bracketed_address() {
    let handler = RecordsClient::default();
    let seen = Arc::clone(&handler.0);
    let milter = Milter::start(handler).await;

    let mut smtp = Smtp::connect_port(Mta::Postfix, 2626).await;
    smtp.ehlo().await;
    smtp.quit().await;

    milter.wait_closed(1).await;
    milter.stop().await;
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1, "{seen:?}");
    let client = &seen[0];
    let Some(address) = client.address else {
        panic!("no client address: {seen:?}");
    };
    let expected = format!("[{}]", address.ip());
    assert_eq!(
        String::from_utf8_lossy(&client.hostname),
        expected,
        "{seen:?}"
    );
}

/// Per-message state that starts out unmarked.
#[derive(Default)]
struct Marked(bool);

/// Records `mail`, `abort`, and `end_of_message` in order, and whether each
/// message state was fresh at MAIL.
#[derive(Clone, Default)]
struct RecordsAbort(Arc<Mutex<Vec<&'static str>>>);

impl Handler for RecordsAbort {
    type Session = ();
    type Message = Marked;

    async fn mail(
        &self,
        ctx: &mut MessageContext<Self>,
        _sender: &MilterText,
        _args: &[MilterText],
    ) -> Result<Decision> {
        let message = ctx.message();
        let event = if message.0 {
            "mail reused"
        } else {
            "mail fresh"
        };
        message.0 = true;
        self.0.lock().unwrap().push(event);
        Ok(Decision::Continue)
    }

    async fn abort(&self, _ctx: &mut MessageContext<Self>) -> Result<()> {
        self.0.lock().unwrap().push("abort");
        Ok(())
    }

    async fn end_of_message(&self, _ctx: &mut EomContext<Self>) -> Result<Decision> {
        self.0.lock().unwrap().push("eom");
        Ok(Decision::Continue)
    }
}

#[tokio::test]
#[ignore = "runs in the interoperability stack"]
async fn int_032_rset_aborts_the_open_message() {
    for mta in Mta::BOTH {
        let handler = RecordsAbort::default();
        let events = Arc::clone(&handler.0);
        let milter = Milter::start(handler).await;
        let second = Mail::new();

        let mut smtp = Smtp::connect(mta).await;
        smtp.ehlo().await;
        smtp.command("MAIL FROM:<sender@void.invalid>").await;
        smtp.command("RCPT TO:<rcpt@void.invalid>").await;
        let reset = smtp.command("RSET").await;
        let sent = smtp.send(&second).await;
        smtp.quit().await;

        assert_eq!(reset.code, 250, "{mta}: {reset}");
        assert!(sent.accepted(), "{mta}: {sent:?}");
        sink::wait_for(&second.token).await;
        milter.wait_closed(1).await;
        milter.stop().await;
        let events = events.lock().unwrap().clone();
        assert!(events.len() >= 4, "{mta}: {events:?}");
        assert_eq!(
            events[..4],
            ["mail fresh", "abort", "mail fresh", "eom"],
            "{mta}"
        );
        for event in &events {
            assert_ne!(*event, "mail reused", "{mta}: {events:?}");
        }
    }
}

/// A header value of about `size` bytes, folded into lines of 97 `x`s.
/// Returns the folded value as sent and the value with its line breaks
/// removed.
fn long_header(size: usize) -> (String, String) {
    let part = "x".repeat(97);
    let mut folded = part.clone();
    let mut unfolded = part.clone();
    while folded.len() + 3 + part.len() <= size {
        folded.push_str("\r\n ");
        folded.push_str(&part);
        unfolded.push(' ');
        unfolded.push_str(&part);
    }
    (folded, unfolded)
}

/// Removes every CR and LF.
fn without_line_breaks(bytes: &[u8]) -> Vec<u8> {
    let mut kept = Vec::with_capacity(bytes.len());
    for &byte in bytes {
        if byte != b'\r' && byte != b'\n' {
            kept.push(byte);
        }
    }
    kept
}

/// Records the value of each `X-Long` header.
#[derive(Clone, Default)]
struct RecordsLongHeader(Arc<Mutex<Vec<Vec<u8>>>>);

impl Handler for RecordsLongHeader {
    type Session = ();
    type Message = ();

    async fn header(
        &self,
        _ctx: &mut MessageContext<Self>,
        name: &MilterText,
        value: &MilterText,
    ) -> Result<Decision> {
        if name.as_bytes() == b"X-Long" {
            self.0.lock().unwrap().push(value.as_bytes().to_vec());
        }
        Ok(Decision::Continue)
    }
}

/// An `X-Raw` header value as the handler saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RawHeader {
    bytes: Vec<u8>,
    is_text: bool,
}

/// Records each `X-Raw` header value, and whether `as_str` accepted it.
#[derive(Clone, Default)]
struct RecordsRawHeader(Arc<Mutex<Vec<RawHeader>>>);

impl Handler for RecordsRawHeader {
    type Session = ();
    type Message = ();

    async fn header(
        &self,
        _ctx: &mut MessageContext<Self>,
        name: &MilterText,
        value: &MilterText,
    ) -> Result<Decision> {
        if name.as_bytes() == b"X-Raw" {
            let header = RawHeader {
                bytes: value.as_bytes().to_vec(),
                is_text: value.as_str().is_some(),
            };
            self.0.lock().unwrap().push(header);
        }
        Ok(Decision::Continue)
    }
}

#[tokio::test]
#[ignore = "runs in the interoperability stack"]
async fn int_035_a_header_that_is_not_utf8_arrives_as_raw_bytes() {
    // "café" in Latin-1: the 0xE9 byte is not valid UTF-8.
    let raw = b"caf\xe9".to_vec();
    for mta in Mta::BOTH {
        let handler = RecordsRawHeader::default();
        let seen = Arc::clone(&handler.0);
        let milter = Milter::start(handler).await;
        let mail = Mail::new().header("X-Raw", raw.clone());

        let sent = Smtp::send_one(mta, &mail).await;

        assert!(sent.accepted(), "{mta}: {sent:?}");
        sink::wait_for(&mail.token).await;
        milter.wait_closed(1).await;
        milter.stop().await;
        let seen = seen.lock().unwrap().clone();
        let expected = RawHeader {
            bytes: raw.clone(),
            is_text: false,
        };
        assert_eq!(seen, [expected], "{mta}");
    }
}

/// The most bytes of one header Postfix sends to a milter
/// (`KLUDGE_HEADER_LIMIT` in Postfix's `cleanup_message.c`).
const POSTFIX_HEADER_CUT: usize = 60_000;

#[tokio::test]
#[ignore = "runs in the interoperability stack"]
async fn int_036_postfix_cuts_a_long_header_to_60000_bytes() {
    let handler = RecordsLongHeader::default();
    let seen = Arc::clone(&handler.0);
    let milter = Milter::start(handler).await;
    let (folded, unfolded) = long_header(100_000);
    let mail = Mail::new().header("X-Long", folded);

    let sent = Smtp::send_one(Mta::Postfix, &mail).await;

    assert!(sent.accepted(), "{sent:?}");
    sink::wait_for(&mail.token).await;
    milter.wait_closed(1).await;
    milter.stop().await;
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1, "one X-Long header");
    let value = &seen[0];
    assert!(
        value.len() <= POSTFIX_HEADER_CUT,
        "{} bytes, more than the cut",
        value.len()
    );
    let received = without_line_breaks(value);
    assert!(
        received.len() < unfolded.len(),
        "the header was cut: {} of {} bytes",
        received.len(),
        unfolded.len()
    );
    assert!(
        unfolded.as_bytes().starts_with(&received),
        "the handler received the start of the header"
    );
}
