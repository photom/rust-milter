//! Decisions and replies: INT-010 to INT-016.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use rust_milter::{
    Actions, ConnectDecision, ConnectInfo, Decision, EomContext, Handler, MessageContext,
    MilterText, ReplyCode, Result, SessionContext, SessionEnd,
};
use rust_milter_interop::{Logs, Mail, Milter, Mta, Smtp, queue, sink};
use tracing::Level;

/// How long a test waits to be sure a message is not delivered.
const NOT_DELIVERED: Duration = Duration::from_secs(5);

/// Rejects the recipient `bad@void.invalid`.
struct RejectsBadRecipient;

impl Handler for RejectsBadRecipient {
    type Session = ();
    type Message = ();

    async fn rcpt(
        &self,
        _ctx: &mut MessageContext<Self>,
        recipient: &MilterText,
        _args: &[MilterText],
    ) -> Result<Decision> {
        let bad = recipient.to_string_lossy().contains("bad@void.invalid");
        Ok(if bad {
            Decision::Reject
        } else {
            Decision::Continue
        })
    }
}

#[tokio::test]
#[ignore = "runs in the interoperability stack"]
async fn int_010_a_rejected_recipient_does_not_get_the_message() {
    for mta in Mta::BOTH {
        let milter = Milter::start(RejectsBadRecipient).await;
        let mail = Mail::new().to(&["bad@void.invalid", "good@void.invalid"]);

        let sent = Smtp::send_one(mta, &mail).await;

        assert_eq!(sent.rcpts[0].code / 100, 5, "{mta}: {sent:?}");
        assert_eq!(sent.rcpts[1].code, 250, "{mta}: {sent:?}");
        assert!(sent.accepted(), "{mta}: {sent:?}");
        let delivered = sink::wait_for(&mail.token).await;
        assert_eq!(delivered.rcpts, ["good@void.invalid"], "{mta}");
        milter.wait_closed(1).await;
        milter.stop().await;
    }
}

/// Ends each message with a fixed decision.
struct DecidesAtEnd(Decision);

impl Handler for DecidesAtEnd {
    type Session = ();
    type Message = ();

    async fn end_of_message(&self, _ctx: &mut EomContext<Self>) -> Result<Decision> {
        Ok(self.0.clone())
    }
}

#[tokio::test]
#[ignore = "runs in the interoperability stack"]
async fn int_011_a_custom_reply_reaches_the_client_unchanged() {
    let code = ReplyCode::new(550, Some("5.7.1"), "Rejected: 100% spam").unwrap();
    for mta in Mta::BOTH {
        let milter = Milter::start(DecidesAtEnd(Decision::Reply(code.clone()))).await;
        let mail = Mail::new();

        let sent = Smtp::send_one(mta, &mail).await;

        let reply = sent.data.expect("DATA was sent");
        assert_eq!(reply.lines, ["550 5.7.1 Rejected: 100% spam"], "{mta}");
        milter.wait_closed(1).await;
        milter.stop().await;
    }
}

#[tokio::test]
#[ignore = "runs in the interoperability stack"]
async fn int_012_a_multi_line_reply_reaches_the_client_in_order() {
    let lines = ["First line", "Second line", "Third line"];
    let code = ReplyCode::multi_line(550, Some("5.7.1"), &lines).unwrap();
    for mta in Mta::BOTH {
        let milter = Milter::start(DecidesAtEnd(Decision::Reply(code.clone()))).await;
        let mail = Mail::new();

        let sent = Smtp::send_one(mta, &mail).await;

        let reply = sent.data.expect("DATA was sent");
        assert_eq!(
            reply.lines,
            [
                "550-5.7.1 First line",
                "550-5.7.1 Second line",
                "550 5.7.1 Third line"
            ],
            "{mta}"
        );
        milter.wait_closed(1).await;
        milter.stop().await;
    }
}

/// Rejects every connection, and records why each session ended.
#[derive(Clone, Default)]
struct RejectsConnection(Arc<Mutex<Vec<SessionEnd>>>);

impl Handler for RejectsConnection {
    type Session = ();
    type Message = ();

    async fn connect(
        &self,
        _ctx: &mut SessionContext<Self>,
        _info: &ConnectInfo,
    ) -> Result<ConnectDecision> {
        Ok(ConnectDecision::Reject)
    }

    async fn session_end(&self, _ctx: &mut SessionContext<Self>, reason: SessionEnd) -> Result<()> {
        self.0.lock().unwrap().push(reason);
        Ok(())
    }
}

#[tokio::test]
#[ignore = "runs in the interoperability stack"]
async fn int_013_postfix_closes_the_connection_after_a_connect_reject() {
    let handler = RejectsConnection::default();
    let ends = Arc::clone(&handler.0);
    let milter = Milter::start(handler).await;
    // Captured from here, so the server's own start and stop lines at INFO
    // are not counted.
    let logs = Logs::capture();

    // Postfix may answer the reject in its greeting or at a later command,
    // so the test collects the replies up to MAIL.
    let mut smtp = Smtp::connect(Mta::Postfix).await;
    let mut replies = vec![smtp.greeting.clone()];
    replies.push(smtp.command("EHLO client.void.invalid").await);
    replies.push(smtp.command("MAIL FROM:<sender@void.invalid>").await);
    smtp.quit().await;

    let mut rejected = false;
    for reply in &replies {
        if reply.code / 100 == 5 {
            rejected = true;
        }
    }
    assert!(rejected, "{replies:?}");
    milter.wait_closed(1).await;
    let logged = logs.at_least(Level::INFO);
    milter.stop().await;
    let ends = ends.lock().unwrap().clone();
    assert_eq!(ends, [SessionEnd::ConnectionClosed]);
    assert!(logged.is_empty(), "{logged:?}");
}

#[tokio::test]
#[ignore = "runs in the interoperability stack"]
async fn int_014_a_discarded_message_is_accepted_and_not_delivered() {
    for mta in Mta::BOTH {
        let milter = Milter::start(DecidesAtEnd(Decision::Discard)).await;
        let mail = Mail::new();

        let sent = Smtp::send_one(mta, &mail).await;

        assert!(sent.accepted(), "{mta}: {sent:?}");
        sink::assert_none_within(&mail.token, NOT_DELIVERED).await;
        milter.wait_closed(1).await;
        milter.stop().await;
    }
}

/// Quarantines each message with the reason `test`.
struct Quarantines;

impl Handler for Quarantines {
    type Session = ();
    type Message = ();

    fn actions(&self) -> Actions {
        Actions::QUARANTINE
    }

    async fn end_of_message(&self, ctx: &mut EomContext<Self>) -> Result<Decision> {
        ctx.quarantine("test").await?;
        Ok(Decision::Continue)
    }
}

#[tokio::test]
#[ignore = "runs in the interoperability stack"]
async fn int_015_a_quarantined_message_is_held() {
    for mta in Mta::BOTH {
        let milter = Milter::start(Quarantines).await;
        let mail = Mail::new();

        let sent = Smtp::send_one(mta, &mail).await;

        assert!(sent.accepted(), "{mta}: {sent:?}");
        match mta {
            Mta::Postfix => {
                queue::wait_for_postfix_hold(&mail.token).await;
            }
            _ => {
                let reason = queue::wait_for_sendmail_quarantine(&mail.token).await;
                assert_eq!(reason, "test");
            }
        }
        assert!(sink::received(&mail.token).is_empty(), "{mta}");
        milter.wait_closed(1).await;
        milter.stop().await;
    }
}

/// Tempfails MAIL from `tempfail@void.invalid`.
struct TempfailsSender;

impl Handler for TempfailsSender {
    type Session = ();
    type Message = ();

    async fn mail(
        &self,
        _ctx: &mut MessageContext<Self>,
        sender: &MilterText,
        _args: &[MilterText],
    ) -> Result<Decision> {
        let tempfail = sender.to_string_lossy().contains("tempfail@void.invalid");
        Ok(if tempfail {
            Decision::TempFail
        } else {
            Decision::Continue
        })
    }
}

#[tokio::test]
#[ignore = "runs in the interoperability stack"]
async fn int_016_a_tempfail_at_mail_leaves_the_session_usable() {
    for mta in Mta::BOTH {
        let milter = Milter::start(TempfailsSender).await;
        let mut refused = Mail::new();
        refused.from = "tempfail@void.invalid".to_owned();
        let second = Mail::new();

        let mut smtp = Smtp::connect(mta).await;
        smtp.ehlo().await;
        let sent_refused = smtp.send(&refused).await;
        let sent_second = smtp.send(&second).await;
        smtp.quit().await;

        assert_eq!(sent_refused.mail.code / 100, 4, "{mta}: {sent_refused:?}");
        assert!(sent_second.accepted(), "{mta}: {sent_second:?}");
        sink::wait_for(&second.token).await;
        milter.wait_closed(1).await;
        milter.stop().await;
    }
}
