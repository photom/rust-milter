//! Macro scenarios (ENG-040 to ENG-046) from
//! `docs/testing/protocol-engine.md`.

mod support;

use rust_milter::protocol::*;
use rust_milter::{
    Bytes, ConnectDecision, ConnectInfo, Decision, EomContext, Events, Handler, MessageContext,
    MilterText, Result, SessionContext,
};
use support::mta::{Reach, connect_data, macro_data, strings};
use support::{Log, TestServer};

/// Logs the value of one macro in the callbacks that read it.
struct Reads {
    name: &'static str,
    log: Log,
}

impl Reads {
    fn record(&self, callback: &str, value: Option<&MilterText>) {
        let value = match value {
            Some(value) => value.to_string_lossy().into_owned(),
            None => "none".to_owned(),
        };
        self.log.push(format!("{callback}={value}"));
    }
}

impl Handler for Reads {
    type Session = ();
    type Message = ();

    async fn connect(
        &self,
        ctx: &mut SessionContext<Self>,
        _info: &ConnectInfo,
    ) -> Result<ConnectDecision> {
        self.record("connect", ctx.macro_value(self.name));
        Ok(ConnectDecision::Continue)
    }

    async fn mail(
        &self,
        ctx: &mut MessageContext<Self>,
        _sender: &MilterText,
        _args: &[MilterText],
    ) -> Result<Decision> {
        self.record("mail", ctx.macro_value(self.name));
        Ok(Decision::Continue)
    }

    async fn rcpt(
        &self,
        ctx: &mut MessageContext<Self>,
        _recipient: &MilterText,
        _args: &[MilterText],
    ) -> Result<Decision> {
        self.record("rcpt", ctx.macro_value(self.name));
        Ok(Decision::Continue)
    }

    async fn end_of_message(&self, ctx: &mut EomContext<Self>) -> Result<Decision> {
        self.record("end_of_message", ctx.macro_value(self.name));
        Ok(Decision::Continue)
    }
}

async fn start(name: &'static str) -> (TestServer, Log) {
    let log = Log::default();
    let handler = Reads {
        name,
        log: log.clone(),
    };
    (TestServer::start(handler).await, log)
}

#[tokio::test]
async fn eng_040_connect_sees_the_connect_macros() {
    let (server, log) = start("{client_addr}").await;
    let mut mta = server.mta().await;
    mta.negotiate().await;

    mta.send(
        SMFIC_MACRO,
        macro_data(SMFIC_CONNECT, &[("{client_addr}", "192.0.2.1")]),
    )
    .await;
    mta.ask_continue(
        SMFIC_CONNECT,
        connect_data("[192.0.2.1]", "192.0.2.1", 4711),
    )
    .await;

    assert_eq!(log.events(), ["connect=192.0.2.1"]);
    server.stop().await;
}

#[tokio::test]
async fn eng_041_end_of_message_sees_the_queue_id() {
    let (server, log) = start("{i}").await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Body).await;

    // Sendmail sends one-letter macro names without braces.
    mta.send(SMFIC_MACRO, macro_data(SMFIC_BODYEOB, &[("i", "4XYZ123")]))
        .await;
    mta.ask_continue(SMFIC_BODYEOB, Bytes::new()).await;

    let events = log.events();
    assert_eq!(events.last().unwrap(), "end_of_message=4XYZ123");
    server.stop().await;
}

#[tokio::test]
async fn eng_042_later_stages_see_the_mail_macros() {
    let (server, log) = start("{mail_addr}").await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Helo).await;

    mta.send(
        SMFIC_MACRO,
        macro_data(SMFIC_MAIL, &[("{mail_addr}", "sender@void.invalid")]),
    )
    .await;
    mta.ask_continue(SMFIC_MAIL, strings(&["<sender@void.invalid>"]))
        .await;
    mta.ask_continue(SMFIC_RCPT, strings(&["<rcpt@void.invalid>"]))
        .await;
    mta.ask_continue(SMFIC_BODYEOB, Bytes::new()).await;

    assert_eq!(
        &log.events()[1..],
        [
            "mail=sender@void.invalid",
            "rcpt=sender@void.invalid",
            "end_of_message=sender@void.invalid"
        ]
    );
    server.stop().await;
}

#[tokio::test]
async fn eng_043_macros_of_postfix_stages_get_no_reply() {
    let server = TestServer::start(support::ContinueAll).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Rcpt).await;

    for stage in [b'L', b'B', b'U'] {
        mta.send(SMFIC_MACRO, macro_data(stage, &[("{x}", "1")]))
            .await;
    }
    // The first reply after the macros belongs to the header.
    mta.ask_continue(SMFIC_HEADER, strings(&["Subject", "test"]))
        .await;
    server.stop().await;
}

/// Leaves out the headers.
struct NoHeaders;

impl Handler for NoHeaders {
    type Session = ();
    type Message = ();

    fn events(&self) -> Events {
        Events::ALL.difference(Events::HEADER)
    }
}

#[tokio::test]
async fn eng_044_a_header_stage_macro_without_headers_is_harmless() {
    let server = TestServer::start(NoHeaders).await;
    let mut mta = server.mta().await;
    let reply = mta.negotiate().await;
    assert_eq!(reply.protocol, SMFIP_NOHDRS);
    mta.reach_after_negotiation(Reach::Rcpt).await;

    mta.send(SMFIC_MACRO, macro_data(b'L', &[("{x}", "1")]))
        .await;
    mta.ask_continue(SMFIC_EOH, Bytes::new()).await;
    mta.ask_continue(SMFIC_BODYEOB, Bytes::new()).await;
    server.stop().await;
}

/// Sends one message whose MAIL macros set `{mail_addr}`.
async fn message_with_sender(mta: &mut support::mta::Mta, sender: &str) {
    mta.send(
        SMFIC_MACRO,
        macro_data(SMFIC_MAIL, &[("{mail_addr}", sender)]),
    )
    .await;
    mta.ask_continue(SMFIC_MAIL, strings(&[sender])).await;
    mta.ask_continue(SMFIC_RCPT, strings(&["<rcpt@void.invalid>"]))
        .await;
    mta.ask_continue(SMFIC_BODYEOB, Bytes::new()).await;
}

#[tokio::test]
async fn eng_045_macros_sent_again_replace_the_earlier_ones() {
    let (server, log) = start("{mail_addr}").await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Helo).await;

    message_with_sender(&mut mta, "a@void.invalid").await;
    message_with_sender(&mut mta, "b@void.invalid").await;

    assert_eq!(
        &log.events()[4..],
        [
            "mail=b@void.invalid",
            "rcpt=b@void.invalid",
            "end_of_message=b@void.invalid"
        ]
    );
    server.stop().await;
}

#[tokio::test]
async fn eng_046_a_later_stages_macros_are_not_searched() {
    let (server, log) = start("{x}").await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Helo).await;

    mta.send(SMFIC_MACRO, macro_data(SMFIC_MAIL, &[("{x}", "m1")]))
        .await;
    mta.ask_continue(SMFIC_MAIL, strings(&["<one@void.invalid>"]))
        .await;
    mta.send(SMFIC_MACRO, macro_data(SMFIC_RCPT, &[("{x}", "r1")]))
        .await;
    mta.ask_continue(SMFIC_RCPT, strings(&["<rcpt@void.invalid>"]))
        .await;
    mta.ask_continue(SMFIC_BODYEOB, Bytes::new()).await;
    mta.send(SMFIC_MACRO, macro_data(SMFIC_MAIL, &[("{x}", "m2")]))
        .await;
    mta.ask_continue(SMFIC_MAIL, strings(&["<two@void.invalid>"]))
        .await;

    assert_eq!(
        &log.events()[1..],
        ["mail=m1", "rcpt=r1", "end_of_message=r1", "mail=m2"]
    );
    server.stop().await;
}
