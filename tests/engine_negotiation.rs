//! Option negotiation scenarios (ENG-001 to ENG-009) from
//! `docs/testing/protocol-engine.md`.

mod support;

use rust_milter::protocol::*;
use rust_milter::{Actions, Events, Handler};
use support::TestServer;

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
