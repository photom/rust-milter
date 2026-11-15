//! Header values: INT-060 to INT-063.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use rust_milter::{
    Actions, Decision, EomContext, Error, Handler, MessageContext, MilterText, ProtocolError,
    ProtocolOptions, Result,
};
use rust_milter_interop::{Logs, Mail, Milter, Mta, Reply, Smtp, sink};

/// The message of INT-060: four `X-` fields with different whitespace
/// after the colon.
fn spaced_mail() -> Mail {
    Mail::new()
        .field("X-None", "none")
        .field("X-One", " one")
        .field("X-Two", "  two")
        .field("X-Tab", "\ttab")
}

/// The names of the fields in [`spaced_mail`], in order.
const SPACED: [&str; 4] = ["X-None", "X-One", "X-Two", "X-Tab"];

/// The values the handler saw for the fields of [`spaced_mail`], in order.
type Seen = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

/// Requests `options` and `actions`, records the value of each field of
/// [`spaced_mail`] as `header` receives it, and at end of message adds the
/// headers in `adds` and changes the first header of each name in
/// `changes`.
struct RecordsValues {
    options: ProtocolOptions,
    actions: Actions,
    adds: Vec<(&'static str, &'static str)>,
    changes: Vec<(&'static str, &'static str)>,
    seen: Seen,
}

impl RecordsValues {
    /// Records values and modifies nothing.
    fn new(options: ProtocolOptions) -> RecordsValues {
        RecordsValues {
            options,
            actions: Actions::NONE,
            adds: Vec::new(),
            changes: Vec::new(),
            seen: Seen::default(),
        }
    }
}

impl Handler for RecordsValues {
    type Session = ();
    type Message = ();

    fn actions(&self) -> Actions {
        self.actions
    }

    fn protocol_options(&self) -> ProtocolOptions {
        self.options
    }

    async fn header(
        &self,
        _ctx: &mut MessageContext<Self>,
        name: &MilterText,
        value: &MilterText,
    ) -> Result<Decision> {
        for spaced in SPACED {
            if name.as_bytes() == spaced.as_bytes() {
                let seen = (spaced.to_owned(), value.as_bytes().to_vec());
                self.seen.lock().unwrap().push(seen);
            }
        }
        Ok(Decision::Continue)
    }

    async fn end_of_message(&self, ctx: &mut EomContext<Self>) -> Result<Decision> {
        for (name, value) in &self.adds {
            ctx.add_header(name, value).await?;
        }
        for (name, value) in &self.changes {
            ctx.change_header(name, 1, value).await?;
        }
        Ok(Decision::Continue)
    }
}

/// `(name, value)` pairs for [`SPACED`] and `values`.
fn pairs(values: [&[u8]; 4]) -> Vec<(String, Vec<u8>)> {
    let mut pairs = Vec::new();
    for (name, value) in SPACED.iter().zip(values) {
        pairs.push(((*name).to_owned(), value.to_vec()));
    }
    pairs
}

#[tokio::test]
#[ignore = "runs in the interoperability stack"]
async fn int_060_the_leading_space_option_delivers_values_unchanged() {
    for mta in Mta::BOTH {
        let handler = RecordsValues::new(ProtocolOptions::HEADER_LEADING_SPACE);
        let seen = Arc::clone(&handler.seen);
        let milter = Milter::start(handler).await;
        let mail = spaced_mail();

        let sent = Smtp::send_one(mta, &mail).await;

        assert!(sent.accepted(), "{mta}: {sent:?}");
        sink::wait_for(&mail.token).await;
        milter.wait_closed(1).await;
        milter.stop().await;
        let seen = seen.lock().unwrap().clone();
        assert_eq!(seen, pairs([b"none", b" one", b"  two", b"\ttab"]), "{mta}");
    }
}

#[tokio::test]
#[ignore = "runs in the interoperability stack"]
async fn int_061_without_the_option_the_mtas_remove_and_add_one_space() {
    for mta in Mta::BOTH {
        let mut handler = RecordsValues::new(ProtocolOptions::NONE);
        handler.actions = Actions::ADD_HEADERS;
        handler.adds = vec![("X-Added", "added")];
        let seen = Arc::clone(&handler.seen);
        let milter = Milter::start(handler).await;
        let mail = spaced_mail();

        let sent = Smtp::send_one(mta, &mail).await;

        assert!(sent.accepted(), "{mta}: {sent:?}");
        let delivered = sink::wait_for(&mail.token).await;
        milter.wait_closed(1).await;
        milter.stop().await;
        let seen = seen.lock().unwrap().clone();
        assert_eq!(seen, pairs([b"none", b"one", b" two", b"\ttab"]), "{mta}");
        assert_eq!(
            delivered.fields_named("X-Added"),
            [b"X-Added: added".to_vec()],
            "{mta}"
        );
    }
}

#[tokio::test]
#[ignore = "runs in the interoperability stack"]
async fn int_062_with_the_option_the_mtas_store_values_as_sent() {
    for mta in Mta::BOTH {
        let mut handler = RecordsValues::new(ProtocolOptions::HEADER_LEADING_SPACE);
        handler.actions = Actions::ADD_HEADERS | Actions::CHANGE_HEADERS;
        handler.adds = vec![("X-Added", " added"), ("X-Bare", "bare")];
        handler.changes = vec![("X-None", " changed")];
        let milter = Milter::start(handler).await;
        let mail = spaced_mail();

        let sent = Smtp::send_one(mta, &mail).await;

        assert!(sent.accepted(), "{mta}: {sent:?}");
        let delivered = sink::wait_for(&mail.token).await;
        milter.wait_closed(1).await;
        milter.stop().await;
        let expected: [(&str, &[u8]); 3] = [
            ("X-Added", b"X-Added: added"),
            ("X-Bare", b"X-Bare:bare"),
            ("X-None", b"X-None: changed"),
        ];
        for (name, line) in expected {
            assert_eq!(delivered.fields_named(name), [line.to_vec()], "{mta}");
        }
    }
}

/// How long INT-063 waits to see that nothing is delivered.
const NOT_DELIVERED: Duration = Duration::from_secs(3);

/// The Postfix port whose `smtpd` uses `milter_protocol = 4`.
const POSTFIX_VERSION_4: u16 = 2604;

#[tokio::test]
#[ignore = "runs in the interoperability stack"]
async fn int_063_postfix_below_version_6_tempfails_a_handler_that_needs_the_option() {
    let milter = Milter::start(RecordsValues::new(ProtocolOptions::HEADER_LEADING_SPACE)).await;
    let logs = Logs::capture();
    let mail = Mail::new();

    // Postfix answers the tempfail of the failed milter at a later command,
    // so the test collects every reply of the session.
    let mut smtp = Smtp::connect_port(Mta::Postfix, POSTFIX_VERSION_4).await;
    let mut replies: Vec<Reply> = vec![smtp.greeting.clone()];
    replies.push(smtp.command("EHLO client.void.invalid").await);
    let sent = smtp.send(&mail).await;
    smtp.quit().await;
    replies.push(sent.mail.clone());
    replies.extend(sent.rcpts.clone());
    if let Some(data) = &sent.data {
        replies.push(data.clone());
    }

    assert!(!sent.accepted(), "{sent:?}");
    let mut tempfailed = false;
    for reply in &replies {
        if reply.code / 100 == 4 {
            tempfailed = true;
        }
    }
    assert!(tempfailed, "{replies:?}");
    sink::assert_none_within(&mail.token, NOT_DELIVERED).await;
    milter.wait_closed(1).await;
    let closed = logs.named("Connection closed");
    milter.stop().await;
    let error = ProtocolError::OptionsNotOffered(ProtocolOptions::HEADER_LEADING_SPACE);
    let detail = error.to_string();
    let expected = format!("{}: {detail}", Error::from(error));
    assert!(!closed.is_empty(), "a connection closed");
    for event in &closed {
        assert_eq!(event.level, tracing::Level::WARN, "{event:?}");
        assert_eq!(event.field("error"), Some(expected.as_str()), "{event:?}");
    }
}
