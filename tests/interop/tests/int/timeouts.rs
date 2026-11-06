//! Timeouts: INT-040 to INT-042.

use std::time::{Duration, Instant};

use rust_milter::{Decision, EomContext, Handler, Result};
use rust_milter_interop::{Logs, Mail, Milter, Mta, Smtp, sink};

/// How long [`SlowWithProgress`] takes at the end of each message.
const SLOW: Duration = Duration::from_secs(5);

/// Takes [`SLOW`] at the end of each message, and calls `progress()` every
/// second meanwhile.
struct SlowWithProgress;

impl Handler for SlowWithProgress {
    type Session = ();
    type Message = ();

    async fn end_of_message(&self, ctx: &mut EomContext<Self>) -> Result<Decision> {
        let mut waited = Duration::ZERO;
        while waited < SLOW {
            tokio::time::sleep(Duration::from_secs(1)).await;
            waited += Duration::from_secs(1);
            ctx.progress().await?;
        }
        Ok(Decision::Continue)
    }
}

#[tokio::test]
#[ignore = "runs in the interoperability stack"]
async fn int_040_progress_restarts_postfix_content_timeout() {
    let milter = Milter::start(SlowWithProgress).await;
    let mail = Mail::new();

    let mut smtp = Smtp::connect_port(Mta::Postfix, 2625).await;
    smtp.ehlo().await;
    let started = Instant::now();
    let sent = smtp.send(&mail).await;
    let took = started.elapsed();
    smtp.quit().await;

    assert!(sent.accepted(), "{sent:?}");
    assert!(took >= SLOW, "the handler's 5 s were waited out: {took:?}");
    sink::wait_for(&mail.token).await;
    milter.wait_closed(1).await;
    milter.stop().await;
}

#[tokio::test]
#[ignore = "runs in the interoperability stack"]
async fn int_041_progress_does_not_extend_sendmail_end_of_message_timeout() {
    let milter = Milter::start(SlowWithProgress).await;
    let mail = Mail::new();

    let mut smtp = Smtp::connect(Mta::SendmailShort).await;
    smtp.ehlo().await;
    let started = Instant::now();
    let sent = smtp.send(&mail).await;
    let took = started.elapsed();
    smtp.quit().await;

    let Some(reply) = &sent.data else {
        panic!("DATA was not sent: {sent:?}");
    };
    // The filter flag F=T turns the timeout into a temporary failure.
    assert_eq!(reply.code / 100, 4, "{sent:?}");
    assert!(
        took >= Duration::from_millis(2_500) && took < SLOW,
        "Sendmail timed out after about 3 s: {took:?}"
    );
    sink::assert_none_within(&mail.token, Duration::from_secs(1)).await;
    milter.wait_closed(1).await;
    milter.stop().await;
}

/// Implements no callback, so every stage continues.
struct Continues;

impl Handler for Continues {
    type Session = ();
    type Message = ();
}

#[tokio::test]
#[ignore = "runs in the interoperability stack"]
async fn int_042_an_idle_connection_is_closed_and_the_mta_tempfails() {
    for mta in Mta::BOTH {
        let logs = Logs::capture();
        let mut config = Milter::config();
        config.timeouts.idle = Duration::from_secs(3);
        let milter = Milter::start_with(config, Continues).await;
        let mail = Mail::new();

        let mut smtp = Smtp::connect(mta).await;
        smtp.ehlo().await;
        let from = smtp.command("MAIL FROM:<sender@void.invalid>").await;
        let rcpt = smtp.command("RCPT TO:<rcpt@void.invalid>").await;
        tokio::time::sleep(Duration::from_secs(5)).await;
        let data = smtp.data(&mail.content()).await;
        smtp.quit().await;

        assert_eq!(from.code, 250, "{mta}: {from}");
        assert_eq!(rcpt.code, 250, "{mta}: {rcpt}");
        assert_eq!(data.code / 100, 4, "{mta}: {data}");
        milter.wait_closed(1).await;
        let closed = logs.named("Connection closed");
        milter.stop().await;
        let mut timed_out = false;
        for event in &closed {
            if event.field("reason") == Some("timeout") {
                timed_out = true;
            }
        }
        assert!(timed_out, "{mta}: {closed:?}");
    }
}
