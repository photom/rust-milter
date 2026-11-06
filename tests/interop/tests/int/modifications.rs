//! Message modifications: INT-020 to INT-022.

use rust_milter::{Actions, Decision, EomContext, Handler, Result};
use rust_milter_interop::{Mail, Milter, Mta, Smtp, sink};

/// Makes every kind of change at the end of each message.
struct ChangesEverything;

impl Handler for ChangesEverything {
    type Session = ();
    type Message = ();

    fn actions(&self) -> Actions {
        Actions::ADD_HEADERS
            | Actions::CHANGE_HEADERS
            | Actions::ADD_RECIPIENTS
            | Actions::DELETE_RECIPIENTS
            | Actions::CHANGE_FROM
            | Actions::CHANGE_BODY
    }

    async fn end_of_message(&self, ctx: &mut EomContext<Self>) -> Result<Decision> {
        ctx.change_header("X-Change", 1, "new").await?;
        ctx.delete_header("X-Delete", 1).await?;
        ctx.add_recipient("<added@void.invalid>").await?;
        ctx.delete_recipient("<removed@void.invalid>").await?;
        ctx.change_from("<changed@void.invalid>", None).await?;
        ctx.replace_body(b"Replaced body.\r\n").await?;
        Ok(Decision::Continue)
    }
}

#[tokio::test]
#[ignore = "runs in the interoperability stack"]
async fn int_020_every_modification_reaches_the_delivered_message() {
    for mta in Mta::BOTH {
        let milter = Milter::start(ChangesEverything).await;
        let mail = Mail::new()
            .to(&["kept@void.invalid", "removed@void.invalid"])
            .header("X-Change", "old")
            .header("X-Delete", "gone");

        let sent = Smtp::send_one(mta, &mail).await;

        assert!(sent.accepted(), "{mta}: {sent:?}");
        let reached =
            sink::wait_for_recipients(&mail.token, &["added@void.invalid", "kept@void.invalid"])
                .await;
        assert_eq!(
            reached,
            ["added@void.invalid", "kept@void.invalid"],
            "{mta}"
        );
        let copies = sink::received(&mail.token);
        for copy in &copies {
            assert_eq!(copy.from, "changed@void.invalid", "{mta}: {copy:?}");
            assert_eq!(copy.header("X-Change").as_deref(), Some("new"), "{mta}");
            assert_eq!(copy.header("X-Delete"), None, "{mta}");
            assert_eq!(copy.body(), b"Replaced body.\r\n", "{mta}");
        }
        milter.wait_closed(1).await;
        milter.stop().await;
    }
}

/// Inserts `X-First: yes` before every other header.
struct InsertsFirstHeader;

impl Handler for InsertsFirstHeader {
    type Session = ();
    type Message = ();

    fn actions(&self) -> Actions {
        Actions::ADD_HEADERS
    }

    async fn end_of_message(&self, ctx: &mut EomContext<Self>) -> Result<Decision> {
        ctx.insert_header(0, "X-First", "yes").await?;
        Ok(Decision::Continue)
    }
}

#[tokio::test]
#[ignore = "runs in the interoperability stack"]
async fn int_021_postfix_inserts_header_zero_above_its_received_header() {
    let milter = Milter::start(InsertsFirstHeader).await;
    let mail = Mail::new();

    let sent = Smtp::send_one(Mta::Postfix, &mail).await;

    assert!(sent.accepted(), "{sent:?}");
    let delivered = sink::wait_for(&mail.token).await;
    let mut names: Vec<String> = Vec::new();
    for (name, _) in delivered.headers() {
        names.push(name);
    }
    assert!(!names.is_empty(), "the message has headers");
    assert_eq!(names[0], "X-First", "{names:?}");
    let mut received_below = false;
    for name in &names[1..] {
        if name.eq_ignore_ascii_case("Received") {
            received_below = true;
        }
    }
    assert!(received_below, "{names:?}");
    milter.wait_closed(1).await;
    milter.stop().await;
}

/// The 200,000-byte body of INT-022: 2,000 lines of 98 characters and CRLF.
fn large_body() -> Vec<u8> {
    let mut body = Vec::with_capacity(200_000);
    for line in 0..2_000 {
        let text = format!("line {line:05} ");
        body.extend_from_slice(text.as_bytes());
        body.resize(body.len() + 98 - text.len(), b'x');
        body.extend_from_slice(b"\r\n");
    }
    body
}

/// Replaces each body with [`large_body`].
struct ReplacesWithLargeBody;

impl Handler for ReplacesWithLargeBody {
    type Session = ();
    type Message = ();

    fn actions(&self) -> Actions {
        Actions::CHANGE_BODY
    }

    async fn end_of_message(&self, ctx: &mut EomContext<Self>) -> Result<Decision> {
        ctx.replace_body(&large_body()).await?;
        Ok(Decision::Continue)
    }
}

#[tokio::test]
#[ignore = "runs in the interoperability stack"]
async fn int_022_a_large_replacement_body_is_delivered_whole() {
    let expected = large_body();
    assert_eq!(expected.len(), 200_000);
    for mta in Mta::BOTH {
        let milter = Milter::start(ReplacesWithLargeBody).await;
        let mail = Mail::new();

        let sent = Smtp::send_one(mta, &mail).await;

        assert!(sent.accepted(), "{mta}: {sent:?}");
        let delivered = sink::wait_for(&mail.token).await;
        assert!(
            delivered.body() == expected,
            "{mta}: the body differs; {} bytes delivered",
            delivered.body().len()
        );
        milter.wait_closed(1).await;
        milter.stop().await;
    }
}
