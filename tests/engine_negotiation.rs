//! Option negotiation scenarios (ENG-001 to ENG-016) from
//! `docs/testing/protocol-engine.md`.

mod support;

use rust_milter::protocol::*;
use rust_milter::{
    Actions, Bytes, ConnectDecision, ConnectInfo, Decision, EomContext, Error, Events, Handler,
    MessageContext, MilterText, ProtocolError, ProtocolOptions, Result, SessionContext,
};
use support::logs::Logs;
use support::mta::{Reach, connect_data, strings};
use support::{Log, TestServer};

/// A handler with chosen events and actions and no callbacks.
struct Declares {
    events: Events,
    actions: Actions,
}

impl Declares {
    fn server(events: Events, actions: Actions) -> Declares {
        Declares { events, actions }
    }
}

impl Handler for Declares {
    type Session = ();
    type Message = ();

    fn events(&self) -> Events {
        self.events
    }

    fn actions(&self) -> Actions {
        self.actions
    }
}

#[tokio::test]
async fn eng_001_answers_with_the_handlers_actions_and_no_stage_flags() {
    let server = TestServer::start(Declares::server(Events::ALL, Actions::ADD_HEADERS)).await;
    let mut mta = server.mta().await;

    let reply = mta.negotiate().await;

    assert_eq!(reply.version, 6);
    assert_eq!(reply.actions, SMFIF_ADDHDRS);
    assert_eq!(reply.protocol, 0);
    server.stop().await;
}

#[tokio::test]
async fn eng_002_asks_the_mta_not_to_send_the_stages_left_out() {
    let events = Events::ALL.difference(Events::HEADER | Events::BODY);
    let server = TestServer::start(Declares::server(events, Actions::NONE)).await;
    let mut mta = server.mta().await;

    let reply = mta.negotiate().await;

    assert_eq!(reply.protocol, SMFIP_NOHDRS | SMFIP_NOBODY);
    server.stop().await;
}

#[tokio::test]
async fn eng_003_answers_version_2_with_version_2_flags_only() {
    let events = Events::ALL.difference(Events::DATA | Events::UNKNOWN | Events::HEADER);
    let server = TestServer::start(Declares::server(events, Actions::NONE)).await;
    let mut mta = server.mta().await;

    let reply = mta.optneg(2, SMFI_V2_ACTS, SMFI_V2_PROT).await.unwrap();

    assert_eq!(reply.version, 2);
    assert_eq!(reply.protocol, SMFIP_NOHDRS);
    assert_eq!(reply.protocol & !SMFI_V2_PROT, 0);
    server.stop().await;
}

#[tokio::test]
async fn eng_004_closes_without_a_reply_below_version_2() {
    let server = TestServer::start(support::ContinueAll).await;
    for version in [1, 0] {
        let mut mta = server.mta().await;

        let reply = mta.optneg(version, SMFI_CURR_ACTS, SMFI_CURR_PROT).await;

        assert_eq!(reply, None, "version {version}");
    }
    server.stop().await;
}

#[tokio::test]
async fn eng_005_reads_zero_actions_as_the_version_1_actions() {
    let server = TestServer::start(Declares::server(Events::ALL, Actions::CHANGE_FROM)).await;
    let mut mta = server.mta().await;

    let reply = mta.optneg(6, 0, SMFI_CURR_PROT).await;

    assert_eq!(reply, None);
    server.stop().await;
}

#[tokio::test]
async fn eng_006_closes_when_the_mta_does_not_offer_a_requested_action() {
    let server = TestServer::start(Declares::server(Events::ALL, Actions::QUARANTINE)).await;
    let mut mta = server.mta().await;

    let reply = mta
        .optneg(6, SMFI_CURR_ACTS & !SMFIF_QUARANTINE, SMFI_CURR_PROT)
        .await;

    assert_eq!(reply, None);
    server.stop().await;
}

#[tokio::test]
async fn eng_007_requests_only_flags_the_mta_offered() {
    let events = Events::ALL.difference(Events::DATA);
    let server = TestServer::start(Declares::server(events, Actions::NONE)).await;
    let mut mta = server.mta().await;
    let offer = SMFI_CURR_PROT & !SMFIP_NODATA;

    let reply = mta.optneg(6, SMFI_CURR_ACTS, offer).await.unwrap();

    assert_eq!(reply.protocol & SMFIP_NODATA, 0);
    assert_eq!(reply.protocol & !offer, 0);
    server.stop().await;
}

#[tokio::test]
async fn eng_008_closes_on_a_short_option_negotiation() {
    let server = TestServer::start(support::ContinueAll).await;
    let mut mta = server.mta().await;

    mta.send(SMFIC_OPTNEG, vec![0; 11]).await;

    assert!(mta.closes().await);
    server.stop().await;
}

#[tokio::test]
async fn eng_009_answers_version_6_to_a_later_version() {
    let server = TestServer::start(support::ContinueAll).await;
    let mut mta = server.mta().await;

    let reply = mta.optneg(7, SMFI_CURR_ACTS, SMFI_CURR_PROT).await.unwrap();

    assert_eq!(reply.version, 6);
    server.stop().await;
}

/// Keeps the default protocol options, and logs the options in force at
/// connect.
struct ReportsOptions(Log);

impl Handler for ReportsOptions {
    type Session = ();
    type Message = ();

    async fn connect(
        &self,
        ctx: &mut SessionContext<Self>,
        _info: &ConnectInfo,
    ) -> Result<ConnectDecision> {
        self.0.push(format!("{:?}", ctx.negotiated().options()));
        Ok(ConnectDecision::Continue)
    }
}

/// Negotiates with the full version 6 offer and sends connect, so the
/// handler's `connect` has run when it returns.
async fn negotiate_and_connect(mta: &mut support::mta::Mta) -> support::mta::OptNeg {
    let reply = mta.negotiate().await;
    mta.ask_continue(
        SMFIC_CONNECT,
        connect_data("[192.0.2.1]", "192.0.2.1", 4711),
    )
    .await;
    reply
}

#[tokio::test]
async fn eng_013_a_default_handler_requests_no_protocol_option() {
    let log = Log::default();
    let server = TestServer::start(ReportsOptions(log.clone())).await;
    let mut mta = server.mta().await;

    let reply = negotiate_and_connect(&mut mta).await;

    assert_eq!(reply.protocol & SMFIP_HDR_LEADSPC, 0);
    assert_eq!(log.events(), [format!("{:?}", ProtocolOptions::NONE)]);
    server.stop().await;
}

/// Receives every stage, requests `HEADER_LEADING_SPACE` and the chosen
/// actions, and logs the options in force at connect and each header value
/// as `header:` and the value's text.
struct LeadingSpace {
    actions: Actions,
    log: Log,
}

impl LeadingSpace {
    fn server(actions: Actions, log: &Log) -> LeadingSpace {
        LeadingSpace {
            actions,
            log: log.clone(),
        }
    }
}

impl Handler for LeadingSpace {
    type Session = ();
    type Message = ();

    fn actions(&self) -> Actions {
        self.actions
    }

    fn protocol_options(&self) -> ProtocolOptions {
        ProtocolOptions::HEADER_LEADING_SPACE
    }

    async fn connect(
        &self,
        ctx: &mut SessionContext<Self>,
        _info: &ConnectInfo,
    ) -> Result<ConnectDecision> {
        self.log.push(format!("{:?}", ctx.negotiated().options()));
        Ok(ConnectDecision::Continue)
    }

    async fn header(
        &self,
        _ctx: &mut MessageContext<Self>,
        _name: &MilterText,
        value: &MilterText,
    ) -> Result<Decision> {
        self.log.push(format!("header:{}", value.to_string_lossy()));
        Ok(Decision::Continue)
    }
}

#[tokio::test]
async fn eng_010_requests_the_leading_space_when_the_mta_offers_it() {
    let log = Log::default();
    let server = TestServer::start(LeadingSpace::server(Actions::NONE, &log)).await;
    let mut mta = server.mta().await;

    let reply = negotiate_and_connect(&mut mta).await;

    assert_eq!(reply.protocol, SMFIP_HDR_LEADSPC);
    assert_eq!(
        log.events(),
        [format!("{:?}", ProtocolOptions::HEADER_LEADING_SPACE)]
    );
    server.stop().await;
}

/// The `error` field of the "Connection closed" log for `error`: the
/// `Error::Protocol` text, then the `ProtocolError` text.
fn close_error(error: ProtocolError) -> String {
    let detail = error.to_string();
    format!("{}: {detail}", Error::from(error))
}

/// Checks that the server logged one connection closed at WARN with
/// `error`, after `server.stop()` has waited for the connection.
fn assert_closed_with(logs: &Logs, error: ProtocolError) {
    let closed = logs.one("Connection closed");
    assert_eq!(closed.level, tracing::Level::WARN);
    assert_eq!(closed.field("error"), Some(close_error(error).as_str()));
}

#[tokio::test]
async fn eng_011_closes_when_the_mta_does_not_offer_a_requested_option() {
    let (logs, _guard) = Logs::capture();
    let log = Log::default();
    let server = TestServer::start(LeadingSpace::server(Actions::NONE, &log)).await;
    let mut mta = server.mta().await;

    let reply = mta
        .optneg(6, SMFI_CURR_ACTS, SMFI_CURR_PROT & !SMFIP_HDR_LEADSPC)
        .await;

    assert_eq!(reply, None);
    server.stop().await;
    assert_closed_with(
        &logs,
        ProtocolError::OptionsNotOffered(ProtocolOptions::HEADER_LEADING_SPACE),
    );
    assert!(log.events().is_empty(), "{:?}", log.events());
}

#[tokio::test]
async fn eng_012_counts_an_option_as_offered_only_at_version_6() {
    let (logs, _guard) = Logs::capture();
    let log = Log::default();
    let server = TestServer::start(LeadingSpace::server(Actions::NONE, &log)).await;
    let offers = [(4, SMFI_CURR_PROT), (2, SMFI_CURR_PROT), (6, 0)];

    for (version, protocol) in offers {
        let mut mta = server.mta().await;
        let reply = mta.optneg(version, SMFI_CURR_ACTS, protocol).await;
        assert_eq!(reply, None, "version {version}, protocol {protocol:#x}");
    }

    server.stop().await;
    let closed = logs.named("Connection closed");
    assert_eq!(closed.len(), offers.len());
    let expected = close_error(ProtocolError::OptionsNotOffered(
        ProtocolOptions::HEADER_LEADING_SPACE,
    ));
    for event in closed {
        assert_eq!(event.level, tracing::Level::WARN);
        assert_eq!(event.field("error"), Some(expected.as_str()));
    }
    assert!(log.events().is_empty(), "{:?}", log.events());
}

#[tokio::test]
async fn eng_014_checks_the_actions_before_the_options() {
    let (logs, _guard) = Logs::capture();
    let log = Log::default();
    let server = TestServer::start(LeadingSpace::server(Actions::QUARANTINE, &log)).await;
    let mut mta = server.mta().await;

    let reply = mta
        .optneg(
            6,
            SMFI_CURR_ACTS & !SMFIF_QUARANTINE,
            SMFI_CURR_PROT & !SMFIP_HDR_LEADSPC,
        )
        .await;

    assert_eq!(reply, None);
    server.stop().await;
    assert_closed_with(&logs, ProtocolError::ActionsNotOffered(Actions::QUARANTINE));
}

#[tokio::test]
async fn eng_015_header_receives_values_unchanged_with_the_leading_space() {
    let log = Log::default();
    let server = TestServer::start(LeadingSpace::server(Actions::NONE, &log)).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Data).await;

    for (name, value) in [
        ("X-One", " one"),
        ("X-None", "none"),
        ("X-Two", "  two"),
        ("X-Tab", "\ttab"),
    ] {
        mta.ask_continue(SMFIC_HEADER, strings(&[name, value]))
            .await;
    }

    assert_eq!(
        log.events(),
        [
            "ProtocolOptions(HEADER_LEADING_SPACE)",
            "header: one",
            "header:none",
            "header:  two",
            "header:\ttab",
        ]
    );
    server.stop().await;
}

/// Requests `HEADER_LEADING_SPACE` and the header actions, and adds,
/// inserts, and changes one header each at end of message.
struct ModifiesHeaders;

impl Handler for ModifiesHeaders {
    type Session = ();
    type Message = ();

    fn actions(&self) -> Actions {
        Actions::ADD_HEADERS | Actions::CHANGE_HEADERS
    }

    fn protocol_options(&self) -> ProtocolOptions {
        ProtocolOptions::HEADER_LEADING_SPACE
    }

    async fn end_of_message(&self, ctx: &mut EomContext<Self>) -> Result<Decision> {
        ctx.add_header("X-A", " a").await?;
        ctx.insert_header(0, "X-B", "b").await?;
        ctx.change_header("X-C", 1, " c").await?;
        Ok(Decision::Continue)
    }
}

#[tokio::test]
async fn eng_016_header_modifications_send_values_as_given() {
    let server = TestServer::start(ModifiesHeaders).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Body).await;

    mta.send(SMFIC_BODYEOB, Bytes::new()).await;
    let replies = [
        mta.recv().await.unwrap(),
        mta.recv().await.unwrap(),
        mta.recv().await.unwrap(),
        mta.recv().await.unwrap(),
    ];

    assert_eq!(replies[0].command, SMFIR_ADDHEADER);
    assert_eq!(replies[0].data, &b"X-A\0 a\0"[..]);
    assert_eq!(replies[1].command, SMFIR_INSHEADER);
    assert_eq!(replies[1].data, &b"\0\0\0\0X-B\0b\0"[..]);
    assert_eq!(replies[2].command, SMFIR_CHGHEADER);
    assert_eq!(replies[2].data, &b"\0\0\0\x01X-C\0 c\0"[..]);
    assert_eq!(replies[3].command, SMFIR_CONTINUE);
    server.stop().await;
}
