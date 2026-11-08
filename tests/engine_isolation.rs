//! Session isolation and connection detail scenarios (ENG-080 to ENG-083,
//! ENG-090, ENG-091) from `docs/testing/protocol-engine.md`.

mod support;

use std::net::SocketAddr;

use bytes::{BufMut, BytesMut};
use rust_milter::protocol::*;
use rust_milter::{
    Bytes, ConnectDecision, ConnectInfo, Decision, Handler, MessageContext, MilterText, Result,
    SessionContext,
};
use support::mta::{Mta, Reach, connect_data, macro_data, strings};
use support::{Log, TestServer};

/// Keeps a marker in its session and message values, and logs what it
/// finds there and in `{auth_authen}`.
struct Marks(Log);

impl Handler for Marks {
    type Session = Option<String>;
    type Message = Option<String>;

    async fn connect(
        &self,
        ctx: &mut SessionContext<Self>,
        info: &ConnectInfo,
    ) -> Result<ConnectDecision> {
        let auth = match ctx.macro_value("{auth_authen}") {
            Some(value) => Some(value.to_string_lossy().into_owned()),
            None => None,
        };
        let marker = info.hostname.to_string_lossy().into_owned();
        let old = ctx.session().replace(marker.clone());
        self.0
            .push(format!("connect {marker}: session {old:?}, auth {auth:?}"));
        Ok(ConnectDecision::Continue)
    }

    async fn mail(
        &self,
        ctx: &mut MessageContext<Self>,
        sender: &MilterText,
        _args: &[MilterText],
    ) -> Result<Decision> {
        let sender = sender.to_string_lossy().into_owned();
        let old = ctx.message().replace(sender.clone());
        let session = ctx.session().clone();
        self.0.push(format!(
            "mail {sender}: message {old:?}, session {session:?}"
        ));
        Ok(Decision::Continue)
    }
}

async fn start() -> (TestServer, Log) {
    let log = Log::default();
    let handler = Marks(log.clone());
    (TestServer::start(handler).await, log)
}

#[tokio::test]
async fn eng_080_a_new_session_on_the_same_connection_starts_fresh() {
    let (server, log) = start().await;
    let mut mta = server.mta().await;
    mta.negotiate().await;
    mta.ask_continue(SMFIC_CONNECT, connect_data("one", "192.0.2.1", 4711))
        .await;
    mta.ask_continue(SMFIC_UNKNOWN, strings(&["NOOP"])).await;

    mta.send(SMFIC_QUIT_NC, Bytes::new()).await;
    mta.ask_continue(SMFIC_CONNECT, connect_data("two", "192.0.2.1", 4711))
        .await;

    assert_eq!(
        log.events(),
        [
            "connect one: session None, auth None",
            "connect two: session None, auth None"
        ]
    );
    server.stop().await;
}

#[tokio::test]
async fn eng_081_a_second_message_starts_fresh() {
    let (server, log) = start().await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Helo).await;

    for sender in ["<a@void.invalid>", "<b@void.invalid>"] {
        mta.ask_continue(SMFIC_MAIL, strings(&[sender])).await;
        mta.ask_continue(SMFIC_RCPT, strings(&["<r@void.invalid>"]))
            .await;
        mta.ask_continue(SMFIC_BODYEOB, Bytes::new()).await;
    }

    assert_eq!(
        &log.events()[1..],
        [
            "mail <a@void.invalid>: message None, session Some(\"[192.0.2.1]\")",
            "mail <b@void.invalid>: message None, session Some(\"[192.0.2.1]\")"
        ]
    );
    server.stop().await;
}

#[tokio::test]
async fn eng_082_a_new_session_does_not_see_the_last_sessions_macros() {
    let (server, log) = start().await;
    let mut mta = server.mta().await;
    mta.negotiate().await;
    mta.send(
        SMFIC_MACRO,
        macro_data(SMFIC_CONNECT, &[("{auth_authen}", "alice")]),
    )
    .await;
    mta.ask_continue(SMFIC_CONNECT, connect_data("one", "192.0.2.1", 4711))
        .await;
    mta.ask_continue(SMFIC_UNKNOWN, strings(&["NOOP"])).await;

    mta.send(SMFIC_QUIT_NC, Bytes::new()).await;
    mta.send(
        SMFIC_MACRO,
        macro_data(SMFIC_CONNECT, &[("{client_addr}", "192.0.2.2")]),
    )
    .await;
    mta.ask_continue(SMFIC_CONNECT, connect_data("two", "192.0.2.2", 4711))
        .await;

    assert_eq!(
        log.events(),
        [
            "connect one: session None, auth Some(\"alice\")",
            "connect two: session None, auth None"
        ]
    );
    server.stop().await;
}

/// Runs one session on its own connection, with its own markers.
async fn session(mut mta: Mta, name: &str) {
    mta.negotiate().await;
    mta.ask_continue(SMFIC_CONNECT, connect_data(name, "192.0.2.1", 4711))
        .await;
    mta.ask_continue(SMFIC_HELO, strings(&["client.void.invalid"]))
        .await;
    for i in 0..20 {
        let sender = format!("<{name}-{i}@void.invalid>");
        mta.ask_continue(SMFIC_MAIL, strings(&[&sender])).await;
        mta.ask_continue(SMFIC_RCPT, strings(&["<r@void.invalid>"]))
            .await;
        mta.ask_continue(SMFIC_BODYEOB, Bytes::new()).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn eng_083_concurrent_connections_see_only_their_own_values() {
    let (server, log) = start().await;
    let first = server.mta().await;
    let second = server.mta().await;

    tokio::join!(session(first, "one"), session(second, "two"));

    let events = log.events();
    let mut mails = Vec::new();
    for event in &events {
        if event.starts_with("mail") {
            mails.push(event);
        }
    }
    assert_eq!(mails.len(), 40);
    for event in mails {
        let name = if event.starts_with("mail <one") {
            "one"
        } else {
            "two"
        };
        assert!(event.contains("message None"), "{event}");
        assert!(
            event.ends_with(&format!("session Some(\"{name}\")")),
            "{event}"
        );
    }
    server.stop().await;
}

/// Logs the `ConnectInfo` it receives.
struct RecordsInfo(Log);

impl Handler for RecordsInfo {
    type Session = ();
    type Message = ();

    async fn connect(
        &self,
        _ctx: &mut SessionContext<Self>,
        info: &ConnectInfo,
    ) -> Result<ConnectDecision> {
        self.0.push(format!(
            "{} {:?}",
            info.hostname.to_string_lossy(),
            info.address
        ));
        Ok(ConnectDecision::Continue)
    }
}

async fn connect_info(data: Bytes) -> Vec<String> {
    let log = Log::default();
    let handler = RecordsInfo(log.clone());
    let server = TestServer::start(handler).await;
    let mut mta = server.mta().await;
    mta.negotiate().await;
    mta.ask_continue(SMFIC_CONNECT, data).await;
    server.stop().await;
    log.events()
}

#[tokio::test]
async fn eng_090_connect_info_holds_the_hostname_and_address() {
    let events = connect_info(connect_data("[192.0.2.1]", "192.0.2.1", 4711)).await;

    let address: SocketAddr = "192.0.2.1:4711".parse().unwrap();
    assert_eq!(events, [format!("[192.0.2.1] {:?}", Some(address))]);
}

#[tokio::test]
async fn eng_091_an_unknown_family_has_no_address() {
    let mut data = BytesMut::new();
    data.put_slice(b"unknown\0");
    data.put_u8(SMFIA_UNKNOWN);

    let events = connect_info(data.freeze()).await;

    assert_eq!(events, ["unknown None"]);
}
