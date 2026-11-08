//! Protocol state machine and abort scenarios (ENG-050 to ENG-070) from
//! `docs/testing/protocol-engine.md`.

mod support;

use rust_milter::protocol::*;
use rust_milter::{
    Bytes, ConnectDecision, ConnectInfo, Decision, EomContext, Events, Handler, MessageContext,
    MilterText, ReplyCode, Result, SessionContext, SessionEnd,
};
use support::logs::Logs;
use support::mta::{Mta, Reach, connect_data, strings};
use support::{Log, TestServer};

/// Logs every callback, answers with the configured decisions, and logs
/// `stale` when a session or message value is not fresh.
#[derive(Clone)]
struct Scripted {
    log: Log,
    events: Events,
    connect: ConnectDecision,
    helo: ConnectDecision,
    mail: Decision,
    eom: Decision,
}

impl Default for Scripted {
    fn default() -> Self {
        Scripted {
            log: Log::default(),
            events: Events::ALL,
            connect: ConnectDecision::Continue,
            helo: ConnectDecision::Continue,
            mail: Decision::Continue,
            eom: Decision::Continue,
        }
    }
}

impl Scripted {
    async fn start(self) -> (TestServer, Log) {
        let log = self.log.clone();
        (TestServer::start(self).await, log)
    }

    fn session_seen(&self, ctx: &mut SessionContext<Self>) {
        if std::mem::replace(ctx.session(), true) {
            self.log.push("stale session");
        }
    }
}

impl Handler for Scripted {
    type Session = bool;
    type Message = bool;

    fn events(&self) -> Events {
        self.events
    }

    async fn connect(
        &self,
        ctx: &mut SessionContext<Self>,
        _info: &ConnectInfo,
    ) -> Result<ConnectDecision> {
        self.log.push("connect");
        self.session_seen(ctx);
        Ok(self.connect.clone())
    }

    async fn helo(
        &self,
        ctx: &mut SessionContext<Self>,
        _name: &MilterText,
    ) -> Result<ConnectDecision> {
        self.log.push("helo");
        if !self.events.contains(Events::CONNECT) {
            self.session_seen(ctx);
        }
        Ok(self.helo.clone())
    }

    async fn mail(
        &self,
        ctx: &mut MessageContext<Self>,
        _sender: &MilterText,
        _args: &[MilterText],
    ) -> Result<Decision> {
        self.log.push("mail");
        if std::mem::replace(ctx.message(), true) {
            self.log.push("stale message");
        }
        Ok(self.mail.clone())
    }

    async fn rcpt(
        &self,
        _ctx: &mut MessageContext<Self>,
        _recipient: &MilterText,
        _args: &[MilterText],
    ) -> Result<Decision> {
        self.log.push("rcpt");
        Ok(Decision::Continue)
    }

    async fn end_of_message(&self, _ctx: &mut EomContext<Self>) -> Result<Decision> {
        self.log.push("end_of_message");
        Ok(self.eom.clone())
    }

    async fn abort(&self, _ctx: &mut MessageContext<Self>) -> Result<()> {
        self.log.push("abort");
        Ok(())
    }

    async fn unknown(
        &self,
        _ctx: &mut SessionContext<Self>,
        _command: &MilterText,
    ) -> Result<Decision> {
        self.log.push("unknown");
        Ok(Decision::Continue)
    }

    async fn session_end(&self, _ctx: &mut SessionContext<Self>, reason: SessionEnd) -> Result<()> {
        self.log.push(format!("session_end:{reason:?}"));
        Ok(())
    }
}

/// The commands that close the connection after HELO in ENG-050.
fn out_of_order() -> [(u8, Bytes); 8] {
    [
        (SMFIC_RCPT, strings(&["<rcpt@void.invalid>"])),
        (SMFIC_DATA, Bytes::new()),
        (SMFIC_HEADER, strings(&["Subject", "test"])),
        (SMFIC_EOH, Bytes::new()),
        (SMFIC_BODY, Bytes::from_static(b"body\r\n")),
        (SMFIC_BODYEOB, Bytes::new()),
        (
            SMFIC_CONNECT,
            connect_data("[192.0.2.1]", "192.0.2.1", 4711),
        ),
        (
            SMFIC_OPTNEG,
            support::mta::optneg_data(6, SMFI_CURR_ACTS, SMFI_CURR_PROT),
        ),
    ]
}

#[tokio::test]
async fn eng_050_a_command_that_does_not_fit_after_helo_closes_the_connection() {
    for (command, data) in out_of_order() {
        let (server, log) = Scripted::default().start().await;
        let mut mta = server.mta().await;
        mta.reach(Reach::Helo).await;

        mta.send(command, data).await;

        assert!(mta.closes().await, "command {}", char::from(command));
        log.wait_for("session_end:Error").await;
        assert_eq!(log.events(), ["connect", "helo", "session_end:Error"]);
        server.stop().await;
    }
}

#[tokio::test]
async fn eng_051_quit_new_connection_after_end_of_message_starts_a_new_session() {
    let (server, log) = Scripted::default().start().await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Endm).await;

    mta.send(SMFIC_QUIT_NC, Bytes::new()).await;
    mta.ask_continue(
        SMFIC_CONNECT,
        connect_data("[192.0.2.1]", "192.0.2.1", 4711),
    )
    .await;

    let events = log.events();
    assert_eq!(
        &events[events.len() - 2..],
        ["session_end:QuitNewConnection", "connect"]
    );
    assert!(!events.contains(&"stale session".to_owned()), "{events:?}");
    server.stop().await;
}

#[tokio::test]
async fn eng_052_quit_new_connection_after_an_accepted_message_closes() {
    let scripted = Scripted {
        eom: Decision::Accept,
        ..Scripted::default()
    };
    let (server, log) = scripted.start().await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Body).await;
    assert_eq!(
        mta.ask(SMFIC_BODYEOB, Bytes::new()).await.command,
        SMFIR_ACCEPT
    );

    mta.send(SMFIC_QUIT_NC, Bytes::new()).await;

    assert!(mta.closes().await);
    let events = log.wait_for("session_end:Error").await;
    assert_eq!(events.last().unwrap(), "session_end:Error");
    server.stop().await;
}

#[tokio::test]
async fn eng_053_quit_new_connection_after_an_unknown_command_ends_the_session() {
    let (server, log) = Scripted::default().start().await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Unkn).await;

    mta.send(SMFIC_QUIT_NC, Bytes::new()).await;

    let events = log.wait_for("session_end:QuitNewConnection").await;
    assert_eq!(
        events,
        [
            "connect",
            "helo",
            "unknown",
            "session_end:QuitNewConnection"
        ]
    );
    server.stop().await;
}

#[tokio::test]
async fn eng_054_quit_new_connection_in_any_other_state_closes() {
    let states = [
        Reach::Conn,
        Reach::Helo,
        Reach::Mail,
        Reach::Rcpt,
        Reach::Data,
        Reach::Hdrs,
        Reach::Eohs,
        Reach::Body,
    ];
    let server = TestServer::start(support::ContinueAll).await;
    for state in states {
        let mut mta = server.mta().await;
        mta.reach(state).await;

        mta.send(SMFIC_QUIT_NC, Bytes::new()).await;

        assert!(mta.closes().await, "in {state:?}");
    }
    server.stop().await;
}

#[tokio::test]
async fn eng_055_a_reply_that_resets_the_state_makes_quit_new_connection_close() {
    let rejects_mail = Scripted {
        mail: Decision::Reject,
        ..Scripted::default()
    };
    let tempfails_eom = Scripted {
        eom: Decision::TempFail,
        ..Scripted::default()
    };
    let replies_at_helo = Scripted {
        helo: ConnectDecision::Reply(ReplyCode::new(550, Some("5.7.1"), "No").unwrap()),
        ..Scripted::default()
    };

    let (server, _) = rejects_mail.start().await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Helo).await;
    assert_eq!(
        mta.ask(SMFIC_MAIL, strings(&["<s@void.invalid>"]))
            .await
            .command,
        SMFIR_REJECT
    );
    mta.send(SMFIC_QUIT_NC, Bytes::new()).await;
    assert!(mta.closes().await, "after a rejected MAIL");
    server.stop().await;

    let (server, _) = tempfails_eom.start().await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Body).await;
    assert_eq!(
        mta.ask(SMFIC_BODYEOB, Bytes::new()).await.command,
        SMFIR_TEMPFAIL
    );
    mta.send(SMFIC_QUIT_NC, Bytes::new()).await;
    assert!(
        mta.closes().await,
        "after a temporary failure at end of message"
    );
    server.stop().await;

    let (server, _) = replies_at_helo.start().await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Conn).await;
    let helo = mta.ask(SMFIC_HELO, strings(&["client.void.invalid"])).await;
    assert_eq!(helo.command, SMFIR_REPLYCODE);
    mta.send(SMFIC_QUIT_NC, Bytes::new()).await;
    assert!(mta.closes().await, "after a reply code at HELO");
    server.stop().await;
}

#[tokio::test]
async fn eng_056_an_unknown_command_code_closes_the_connection() {
    let server = TestServer::start(support::ContinueAll).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Helo).await;

    mta.send(b'Z', Bytes::new()).await;

    assert!(mta.closes().await);
    server.stop().await;
}

/// Brings a recording server to `state` and sends one more command; returns
/// the log after `reach` and the reply.
async fn recover(state: Reach, command: u8, data: Bytes) -> (Vec<String>, u8) {
    let (server, log) = Scripted::default().start().await;
    let mut mta = server.mta().await;
    mta.reach(state).await;
    let before = log.events().len();

    let reply = mta.ask(command, data).await.command;

    let events = log.events()[before..].to_vec();
    server.stop().await;
    (events, reply)
}

#[tokio::test]
async fn eng_057_mail_inside_a_message_aborts_it_and_starts_a_new_one() {
    let (events, reply) = recover(Reach::Body, SMFIC_MAIL, strings(&["<new@void.invalid>"])).await;

    assert_eq!(events, ["abort", "mail"]);
    assert_eq!(reply, SMFIR_CONTINUE);
}

#[tokio::test]
async fn eng_058_helo_inside_a_message_aborts_it() {
    let (events, reply) = recover(Reach::Rcpt, SMFIC_HELO, strings(&["again.void.invalid"])).await;

    assert_eq!(events, ["abort", "helo"]);
    assert_eq!(reply, SMFIR_CONTINUE);
}

#[tokio::test]
async fn eng_059_an_unknown_command_after_data_aborts_the_message() {
    let (events, reply) = recover(Reach::Data, SMFIC_UNKNOWN, strings(&["NOOP"])).await;

    assert_eq!(events, ["abort", "unknown"]);
    assert_eq!(reply, SMFIR_CONTINUE);
}

#[tokio::test]
async fn eng_065_a_switched_off_stage_is_skipped() {
    let scripted = Scripted {
        events: Events::ALL.difference(Events::MAIL),
        ..Scripted::default()
    };
    let (server, log) = scripted.start().await;
    let mut mta = server.mta().await;
    assert_eq!(mta.negotiate().await.protocol, SMFIP_NOMAIL);
    mta.ask_continue(
        SMFIC_CONNECT,
        connect_data("[192.0.2.1]", "192.0.2.1", 4711),
    )
    .await;
    mta.ask_continue(SMFIC_HELO, strings(&["client.void.invalid"]))
        .await;

    mta.ask_continue(SMFIC_RCPT, strings(&["<rcpt@void.invalid>"]))
        .await;

    assert_eq!(log.events(), ["connect", "helo", "rcpt"]);
    server.stop().await;
}

#[tokio::test]
async fn eng_066_quit_outside_a_message_is_ignored_until_the_connection_closes() {
    let (server, log) = Scripted::default().start().await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Helo).await;

    mta.send(SMFIC_QUIT, Bytes::new()).await;
    log.settles_at(&["connect", "helo"]).await;
    mta.close().await;

    let events = log.wait_for("session_end:ConnectionClosed").await;
    assert_eq!(events, ["connect", "helo", "session_end:ConnectionClosed"]);
    server.stop().await;
}

#[tokio::test]
async fn eng_067_abort_after_helo_is_ignored() {
    let (server, log) = Scripted::default().start().await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Helo).await;

    mta.send(SMFIC_ABORT, Bytes::new()).await;
    mta.ask_continue(SMFIC_MAIL, strings(&["<s@void.invalid>"]))
        .await;

    assert_eq!(log.events(), ["connect", "helo", "mail"]);
    server.stop().await;
}

#[tokio::test]
async fn eng_068_without_connect_the_session_starts_with_helo() {
    let scripted = Scripted {
        events: Events::ALL.difference(Events::CONNECT),
        ..Scripted::default()
    };
    let (server, log) = scripted.start().await;
    let mut mta = server.mta().await;
    assert_eq!(mta.negotiate().await.protocol, SMFIP_NOCONNECT);

    mta.ask_continue(SMFIC_HELO, strings(&["client.void.invalid"]))
        .await;
    mta.ask_continue(SMFIC_MAIL, strings(&["<s@void.invalid>"]))
        .await;
    mta.ask_continue(SMFIC_RCPT, strings(&["<r@void.invalid>"]))
        .await;
    mta.ask_continue(SMFIC_BODYEOB, Bytes::new()).await;
    mta.send(SMFIC_QUIT, Bytes::new()).await;

    assert!(mta.closes().await);
    let events = log.wait_for("session_end:Quit").await;
    assert_eq!(
        events,
        ["helo", "mail", "rcpt", "end_of_message", "session_end:Quit"]
    );
    server.stop().await;
}

#[tokio::test]
async fn eng_069_an_unknown_command_after_rcpt_keeps_the_message_until_mail() {
    let (server, log) = Scripted::default().start().await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Rcpt).await;

    mta.ask_continue(SMFIC_UNKNOWN, strings(&["NOOP"])).await;
    mta.ask_continue(SMFIC_MAIL, strings(&["<new@void.invalid>"]))
        .await;

    assert_eq!(
        log.events(),
        [
            "connect", "helo", "mail", "rcpt", "unknown", "abort", "mail"
        ]
    );
    server.stop().await;
}

#[tokio::test]
async fn eng_070_a_reject_at_mail_resets_the_state_to_helo() {
    let scripted = Scripted {
        mail: Decision::Reject,
        ..Scripted::default()
    };
    let (server, log) = scripted.start().await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Helo).await;
    assert_eq!(
        mta.ask(SMFIC_MAIL, strings(&["<s@void.invalid>"]))
            .await
            .command,
        SMFIR_REJECT
    );

    mta.send(SMFIC_RCPT, strings(&["<r@void.invalid>"])).await;

    assert!(mta.closes().await);
    let events = log.wait_for("session_end:Error").await;
    assert_eq!(events, ["connect", "helo", "mail", "session_end:Error"]);
    server.stop().await;
}

#[tokio::test]
async fn eng_060_abort_ends_the_message_without_a_reply() {
    let (server, log) = Scripted::default().start().await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Rcpt).await;

    mta.send(SMFIC_ABORT, Bytes::new()).await;
    // The next reply belongs to MAIL, so the abort got none.
    mta.ask_continue(SMFIC_MAIL, strings(&["<new@void.invalid>"]))
        .await;

    assert_eq!(
        log.events(),
        ["connect", "helo", "mail", "rcpt", "abort", "mail"]
    );
    server.stop().await;
}

#[tokio::test]
async fn eng_061_abort_outside_a_message_does_nothing() {
    let (server, log) = Scripted::default().start().await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Helo).await;

    mta.send(SMFIC_ABORT, Bytes::new()).await;

    log.settles_at(&["connect", "helo"]).await;
    mta.ask_continue(SMFIC_HELO, strings(&["again.void.invalid"]))
        .await;
    server.stop().await;
}

#[tokio::test]
async fn eng_062_quit_after_a_message_ends_the_session() {
    let (server, log) = Scripted::default().start().await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Endm).await;

    mta.send(SMFIC_QUIT, Bytes::new()).await;

    assert!(mta.closes().await, "QUIT gets no reply");
    let events = log.wait_for("session_end:Quit").await;
    assert_eq!(events.last().unwrap(), "session_end:Quit");
    server.stop().await;
}

#[tokio::test]
async fn eng_063_a_close_after_a_connect_level_reject_is_a_normal_end() {
    let scripted = Scripted {
        connect: ConnectDecision::Reject,
        ..Scripted::default()
    };
    let (logs, _guard) = Logs::capture();
    let (server, log) = scripted.start().await;
    let mut mta = server.mta().await;
    mta.negotiate().await;
    let reply = mta
        .ask(
            SMFIC_CONNECT,
            connect_data("[192.0.2.1]", "192.0.2.1", 4711),
        )
        .await;
    assert_eq!(reply.command, SMFIR_REJECT);

    mta.close().await;

    let events = log.wait_for("session_end:ConnectionClosed").await;
    assert_eq!(events, ["connect", "session_end:ConnectionClosed"]);
    server.stop().await;
    assert_eq!(
        logs.one("Connection closed").field("reason"),
        Some("client_closed")
    );
    assert!(logs.warnings().is_empty(), "{:?}", logs.warnings());
}

#[tokio::test]
async fn eng_064_shutdown_ends_a_waiting_session() {
    let (server, log) = Scripted::default().start().await;
    let mut mta: Mta = server.mta().await;
    mta.reach(Reach::Helo).await;

    server.stop().await;

    assert_eq!(
        log.events(),
        ["connect", "helo", "session_end:ServerShutdown"]
    );
    assert!(mta.closes().await);
}
