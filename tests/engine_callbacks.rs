//! Callback and decision scenarios (ENG-020 to ENG-033) from
//! `docs/testing/protocol-engine.md`.

mod support;

use std::time::Duration;

use rust_milter::protocol::*;
use rust_milter::{
    Actions, BodyDecision, Bytes, ConnectDecision, ConnectInfo, Decision, EomContext, Error,
    Handler, MessageContext, MilterText, ReplyCode, Result, SessionContext, SessionEnd,
};
use support::mta::{Reach, connect_data, macro_data, strings};
use support::{Log, Recorder, TestServer};

#[tokio::test]
async fn eng_020_continues_every_command_of_a_session_by_default() {
    let server = TestServer::start(support::ContinueAll).await;
    let mut mta = server.mta().await;
    mta.negotiate().await;

    mta.send(
        SMFIC_MACRO,
        macro_data(SMFIC_CONNECT, &[("j", "mta.void.invalid")]),
    )
    .await;
    let commands: [(u8, Bytes); 13] = [
        (
            SMFIC_CONNECT,
            connect_data("[192.0.2.1]", "192.0.2.1", 4711),
        ),
        (SMFIC_HELO, strings(&["client.void.invalid"])),
        (SMFIC_MAIL, strings(&["<sender@void.invalid>"])),
        (SMFIC_RCPT, strings(&["<one@void.invalid>"])),
        (SMFIC_RCPT, strings(&["<two@void.invalid>"])),
        (SMFIC_DATA, Bytes::new()),
        (SMFIC_HEADER, strings(&["From", "sender@void.invalid"])),
        (SMFIC_HEADER, strings(&["To", "one@void.invalid"])),
        (SMFIC_HEADER, strings(&["Subject", "test"])),
        (SMFIC_EOH, Bytes::new()),
        (SMFIC_BODY, Bytes::from_static(b"first\r\n")),
        (SMFIC_BODY, Bytes::from_static(b"second\r\n")),
        (SMFIC_BODYEOB, Bytes::new()),
    ];
    for (command, data) in commands {
        mta.ask_continue(command, data).await;
    }
    mta.send(SMFIC_QUIT, Bytes::new()).await;

    assert!(mta.closes().await, "QUIT gets no reply");
    server.stop().await;
}

/// Accepts the connection at `connect`, and logs the callbacks.
struct AcceptsAtConnect(Log);

impl Handler for AcceptsAtConnect {
    type Session = ();
    type Message = ();

    async fn connect(
        &self,
        _ctx: &mut SessionContext<Self>,
        _info: &ConnectInfo,
    ) -> Result<ConnectDecision> {
        self.0.push("connect");
        Ok(ConnectDecision::Accept)
    }

    async fn helo(
        &self,
        _ctx: &mut SessionContext<Self>,
        _name: &MilterText,
    ) -> Result<ConnectDecision> {
        self.0.push("helo");
        Ok(ConnectDecision::Continue)
    }
}

#[tokio::test]
async fn eng_021_returns_to_helo_after_a_connect_level_accept() {
    let log = Log::default();
    let handler = AcceptsAtConnect(log.clone());
    let server = TestServer::start(handler).await;
    let mut mta = server.mta().await;
    mta.negotiate().await;

    let connect = mta
        .ask(
            SMFIC_CONNECT,
            connect_data("[192.0.2.1]", "192.0.2.1", 4711),
        )
        .await;
    let helo = mta.ask(SMFIC_HELO, strings(&["client.void.invalid"])).await;

    assert_eq!(connect.command, SMFIR_ACCEPT);
    assert_eq!(helo.command, SMFIR_CONTINUE);
    assert_eq!(log.events(), ["connect", "helo"]);
    server.stop().await;
}

/// Rejects the recipient `<bad@void.invalid>`.
struct RejectsBadRecipient(Log);

impl Handler for RejectsBadRecipient {
    type Session = ();
    type Message = ();

    async fn rcpt(
        &self,
        _ctx: &mut MessageContext<Self>,
        recipient: &MilterText,
        _args: &[MilterText],
    ) -> Result<Decision> {
        self.0.push(recipient.to_string_lossy());
        if recipient.as_bytes() == b"<bad@void.invalid>" {
            Ok(Decision::Reject)
        } else {
            Ok(Decision::Continue)
        }
    }
}

#[tokio::test]
async fn eng_022_a_rejected_recipient_does_not_end_the_message() {
    let log = Log::default();
    let handler = RejectsBadRecipient(log.clone());
    let server = TestServer::start(handler).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Mail).await;

    let first = mta.ask(SMFIC_RCPT, strings(&["<bad@void.invalid>"])).await;
    let second = mta.ask(SMFIC_RCPT, strings(&["<good@void.invalid>"])).await;

    assert_eq!(first.command, SMFIR_REJECT);
    assert_eq!(second.command, SMFIR_CONTINUE);
    assert_eq!(log.events(), ["<bad@void.invalid>", "<good@void.invalid>"]);
    server.stop().await;
}

/// Rejects every sender, and logs whether the message value was fresh.
struct RejectsSender(Log);

impl Handler for RejectsSender {
    type Session = ();
    type Message = bool;

    async fn mail(
        &self,
        ctx: &mut MessageContext<Self>,
        _sender: &MilterText,
        _args: &[MilterText],
    ) -> Result<Decision> {
        self.0.push(if *ctx.message() { "used" } else { "fresh" });
        *ctx.message() = true;
        Ok(Decision::Reject)
    }

    async fn abort(&self, _ctx: &mut MessageContext<Self>) -> Result<()> {
        self.0.push("abort");
        Ok(())
    }
}

#[tokio::test]
async fn eng_023_a_new_mail_after_a_reject_gets_a_fresh_message() {
    let log = Log::default();
    let handler = RejectsSender(log.clone());
    let server = TestServer::start(handler).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Helo).await;

    let first = mta.ask(SMFIC_MAIL, strings(&["<one@void.invalid>"])).await;
    let second = mta.ask(SMFIC_MAIL, strings(&["<two@void.invalid>"])).await;

    assert_eq!(first.command, SMFIR_REJECT);
    assert_eq!(second.command, SMFIR_REJECT);
    assert_eq!(log.events(), ["fresh", "fresh"]);
    server.stop().await;
}

/// Answers every recipient with a custom reply.
struct RepliesToRecipient(ReplyCode);

impl Handler for RepliesToRecipient {
    type Session = ();
    type Message = ();

    async fn rcpt(
        &self,
        _ctx: &mut MessageContext<Self>,
        _recipient: &MilterText,
        _args: &[MilterText],
    ) -> Result<Decision> {
        Ok(Decision::Reply(self.0.clone()))
    }
}

#[tokio::test]
async fn eng_024_sends_a_custom_reply_with_percent_doubled() {
    for (code, enhanced, wire) in [
        (451, "4.7.1", "451 4.7.1 Try 100% later"),
        (550, "5.7.1", "550 5.7.1 Try 100% later"),
    ] {
        let reply = ReplyCode::new(code, Some(enhanced), "Try 100% later").unwrap();
        let server = TestServer::start(RepliesToRecipient(reply)).await;
        let mut mta = server.mta().await;
        mta.reach(Reach::Mail).await;

        let answer = mta.ask(SMFIC_RCPT, strings(&["<rcpt@void.invalid>"])).await;

        assert_eq!(answer.command, SMFIR_REPLYCODE);
        let expected = format!("{}\0", wire.replace('%', "%%"));
        assert_eq!(answer.data, expected.as_bytes());
        server.stop().await;
    }
}

/// Skips the body after the first chunk.
struct SkipsBody(Log);

impl Handler for SkipsBody {
    type Session = ();
    type Message = ();

    async fn body(&self, _ctx: &mut MessageContext<Self>, _chunk: Bytes) -> Result<BodyDecision> {
        self.0.push("body");
        Ok(BodyDecision::Skip)
    }

    async fn end_of_message(&self, _ctx: &mut EomContext<Self>) -> Result<Decision> {
        self.0.push("end_of_message");
        Ok(Decision::Continue)
    }
}

/// Sends three body chunks and end of message; returns the reply codes.
async fn three_chunks(protocol: u32, log: &Log) -> Vec<u8> {
    let handler = SkipsBody(log.clone());
    let server = TestServer::start(handler).await;
    let mut mta = server.mta().await;
    mta.optneg(6, SMFI_CURR_ACTS, protocol).await.unwrap();
    mta.reach_after_negotiation(Reach::Eohs).await;

    let mut replies = Vec::new();
    for _ in 0..3 {
        replies.push(
            mta.ask(SMFIC_BODY, Bytes::from_static(b"chunk\r\n"))
                .await
                .command,
        );
    }
    replies.push(mta.ask(SMFIC_BODYEOB, Bytes::new()).await.command);
    server.stop().await;
    replies
}

#[tokio::test]
async fn eng_025_skip_sends_smfir_skip_when_the_mta_offered_it() {
    let log = Log::default();

    let replies = three_chunks(SMFI_CURR_PROT, &log).await;

    assert_eq!(replies[0], SMFIR_SKIP);
    assert_eq!(replies[3], SMFIR_CONTINUE);
    assert_eq!(log.events(), ["body", "end_of_message"]);
}

#[tokio::test]
async fn eng_026_skip_continues_when_the_mta_did_not_offer_it() {
    let log = Log::default();

    let replies = three_chunks(SMFI_CURR_PROT & !SMFIP_SKIP, &log).await;

    assert_eq!(replies, [SMFIR_CONTINUE; 4]);
    assert_eq!(log.events(), ["body", "end_of_message"]);
}

#[tokio::test]
async fn eng_027_version_2_never_calls_data_or_unknown() {
    let recorder = Recorder::default();
    let log = recorder.log.clone();
    let server = TestServer::start(recorder).await;
    let mut mta = server.mta().await;
    mta.optneg(2, SMFI_V2_ACTS, SMFI_V2_PROT).await.unwrap();

    mta.reach_after_negotiation(Reach::Endm).await;
    mta.ask_continue(SMFIC_UNKNOWN, strings(&["NOOP"])).await;

    let events = log.events();
    for event in &events {
        assert!(event != "data" && event != "unknown", "{events:?}");
    }
    server.stop().await;
}

/// Adds two headers at end of message.
struct AddsTwoHeaders;

impl Handler for AddsTwoHeaders {
    type Session = ();
    type Message = ();

    fn actions(&self) -> Actions {
        Actions::ADD_HEADERS
    }

    async fn end_of_message(&self, ctx: &mut EomContext<Self>) -> Result<Decision> {
        ctx.add_header("X-First", "1").await?;
        ctx.add_header("X-Second", "2").await?;
        Ok(Decision::Continue)
    }
}

#[tokio::test]
async fn eng_028_sends_modifications_in_call_order_before_the_final_reply() {
    let server = TestServer::start(AddsTwoHeaders).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Body).await;

    mta.send(SMFIC_BODYEOB, Bytes::new()).await;
    let replies = [
        mta.recv().await.unwrap(),
        mta.recv().await.unwrap(),
        mta.recv().await.unwrap(),
    ];

    assert_eq!(replies[0].command, SMFIR_ADDHEADER);
    assert_eq!(replies[0].data, &b"X-First\x001\0"[..]);
    assert_eq!(replies[1].command, SMFIR_ADDHEADER);
    assert_eq!(replies[1].data, &b"X-Second\x002\0"[..]);
    assert_eq!(replies[2].command, SMFIR_CONTINUE);
    server.stop().await;
}

/// Calls `add_header` without having asked for `ADD_HEADERS`.
struct AddsWithoutAction(Log);

impl Handler for AddsWithoutAction {
    type Session = ();
    type Message = ();

    async fn end_of_message(&self, ctx: &mut EomContext<Self>) -> Result<Decision> {
        let result = ctx.add_header("X-Test", "1").await;
        let not_negotiated =
            matches!(result, Err(Error::ActionNotNegotiated(a)) if a == Actions::ADD_HEADERS);
        self.0.push(if not_negotiated {
            "not negotiated"
        } else {
            "other"
        });
        Ok(Decision::Continue)
    }
}

#[tokio::test]
async fn eng_029_a_modification_without_its_action_fails_and_sends_nothing() {
    let log = Log::default();
    let handler = AddsWithoutAction(log.clone());
    let server = TestServer::start(handler).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Body).await;

    let reply = mta.ask(SMFIC_BODYEOB, Bytes::new()).await;
    mta.ask_continue(SMFIC_MAIL, strings(&["<next@void.invalid>"]))
        .await;

    assert_eq!(reply.command, SMFIR_CONTINUE);
    assert_eq!(log.events(), ["not negotiated"]);
    server.stop().await;
}

/// Replaces the body with 200,000 bytes.
struct ReplacesBody;

const NEW_BODY_LENGTH: usize = 200_000;

/// The body `ReplacesBody` writes: `NEW_BODY_LENGTH` bytes counting up
/// modulo 251.
fn new_body() -> Vec<u8> {
    let mut body = Vec::with_capacity(NEW_BODY_LENGTH);
    for i in 0..NEW_BODY_LENGTH {
        body.push((i % 251) as u8);
    }
    body
}

impl Handler for ReplacesBody {
    type Session = ();
    type Message = ();

    fn actions(&self) -> Actions {
        Actions::CHANGE_BODY
    }

    async fn end_of_message(&self, ctx: &mut EomContext<Self>) -> Result<Decision> {
        ctx.replace_body(&new_body()).await?;
        Ok(Decision::Continue)
    }
}

#[tokio::test]
async fn eng_030_replaces_a_large_body_in_chunks() {
    let server = TestServer::start(ReplacesBody).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Body).await;

    mta.send(SMFIC_BODYEOB, Bytes::new()).await;
    let mut chunks = Vec::new();
    let final_reply = loop {
        let frame = mta.recv().await.unwrap();
        if frame.command != SMFIR_REPLBODY {
            break frame;
        }
        chunks.push(frame.data);
    };

    assert_eq!(chunks.len(), 4);
    for chunk in &chunks {
        assert!(chunk.len() <= 65_535);
    }
    let joined: Vec<u8> = chunks.concat();
    assert_eq!(joined, new_body());
    assert_eq!(final_reply.command, SMFIR_CONTINUE);
    server.stop().await;
}

/// Reports progress twice before deciding.
struct ReportsProgress;

impl Handler for ReportsProgress {
    type Session = ();
    type Message = ();

    async fn end_of_message(&self, ctx: &mut EomContext<Self>) -> Result<Decision> {
        ctx.progress().await?;
        ctx.progress().await?;
        Ok(Decision::Accept)
    }
}

#[tokio::test]
async fn eng_031_sends_progress_before_the_final_reply() {
    let server = TestServer::start(ReportsProgress).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Body).await;

    mta.send(SMFIC_BODYEOB, Bytes::new()).await;
    let replies = [
        mta.recv().await.unwrap().command,
        mta.recv().await.unwrap().command,
        mta.recv().await.unwrap().command,
    ];

    assert_eq!(replies, [SMFIR_PROGRESS, SMFIR_PROGRESS, SMFIR_ACCEPT]);
    server.stop().await;
}

/// Fails in `header`.
struct FailsInHeader(Log);

#[derive(Debug)]
struct Broken;

impl std::fmt::Display for Broken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("broken")
    }
}

impl std::error::Error for Broken {}

impl Handler for FailsInHeader {
    type Session = ();
    type Message = ();

    async fn header(
        &self,
        _ctx: &mut MessageContext<Self>,
        _name: &MilterText,
        _value: &MilterText,
    ) -> Result<Decision> {
        Err(Error::handler(Broken))
    }

    async fn session_end(&self, _ctx: &mut SessionContext<Self>, reason: SessionEnd) -> Result<()> {
        self.0.push(format!("session_end:{reason:?}"));
        Ok(())
    }
}

#[tokio::test]
async fn eng_032_a_failing_callback_ends_the_session_with_an_error() {
    let log = Log::default();
    let handler = FailsInHeader(log.clone());
    let server = TestServer::start(handler).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Data).await;

    mta.send(SMFIC_HEADER, strings(&["Subject", "test"])).await;

    assert!(mta.closes().await);
    assert_eq!(log.events(), ["session_end:Error"]);
    server.stop().await;
}

/// Logs when each callback starts and ends, with a pause in between.
struct Overlaps(Log);

impl Overlaps {
    async fn run(&self, name: &str) {
        self.0.push(format!("{name} start"));
        tokio::time::sleep(Duration::from_millis(5)).await;
        self.0.push(format!("{name} end"));
    }
}

impl Handler for Overlaps {
    type Session = ();
    type Message = ();

    async fn connect(
        &self,
        _ctx: &mut SessionContext<Self>,
        _info: &ConnectInfo,
    ) -> Result<ConnectDecision> {
        self.run("connect").await;
        Ok(ConnectDecision::Continue)
    }

    async fn helo(
        &self,
        _ctx: &mut SessionContext<Self>,
        _name: &MilterText,
    ) -> Result<ConnectDecision> {
        self.run("helo").await;
        Ok(ConnectDecision::Continue)
    }

    async fn mail(
        &self,
        _ctx: &mut MessageContext<Self>,
        _sender: &MilterText,
        _args: &[MilterText],
    ) -> Result<Decision> {
        self.run("mail").await;
        Ok(Decision::Continue)
    }

    async fn rcpt(
        &self,
        _ctx: &mut MessageContext<Self>,
        _recipient: &MilterText,
        _args: &[MilterText],
    ) -> Result<Decision> {
        self.run("rcpt").await;
        Ok(Decision::Continue)
    }

    async fn end_of_message(&self, _ctx: &mut EomContext<Self>) -> Result<Decision> {
        self.run("end_of_message").await;
        Ok(Decision::Continue)
    }
}

#[tokio::test]
async fn eng_033_callbacks_of_one_connection_run_one_at_a_time_in_order() {
    let log = Log::default();
    let handler = Overlaps(log.clone());
    let server = TestServer::start(handler).await;
    let mut mta = server.mta().await;
    mta.negotiate().await;

    // The commands go out at once, without waiting for each reply.
    let commands: [(u8, Bytes); 5] = [
        (
            SMFIC_CONNECT,
            connect_data("[192.0.2.1]", "192.0.2.1", 4711),
        ),
        (SMFIC_HELO, strings(&["client.void.invalid"])),
        (SMFIC_MAIL, strings(&["<sender@void.invalid>"])),
        (SMFIC_RCPT, strings(&["<rcpt@void.invalid>"])),
        (SMFIC_BODYEOB, Bytes::new()),
    ];
    for (command, data) in commands {
        mta.send(command, data).await;
    }
    for _ in 0..5 {
        assert_eq!(mta.recv().await.unwrap().command, SMFIR_CONTINUE);
    }

    let mut expected = Vec::new();
    for name in ["connect", "helo", "mail", "rcpt", "end_of_message"] {
        expected.push(format!("{name} start"));
        expected.push(format!("{name} end"));
    }
    assert_eq!(log.events(), expected);
    server.stop().await;
}
