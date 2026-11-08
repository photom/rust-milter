//! Connection reset and end of file scenarios (ENG-100 to ENG-107) from
//! `docs/testing/protocol-engine.md`. A keepalive failure reaches the engine
//! as a failed read, like these resets.

mod support;

use std::time::Duration;

use rust_milter::protocol::*;
use rust_milter::{
    Bytes, Decision, EomContext, Handler, MessageContext, Result, SessionContext, SessionEnd,
};
use support::mta::{Reach, strings};
use support::{Log, Recorder, TestServer};

async fn recording() -> (TestServer, Log) {
    let recorder = Recorder::default();
    let log = recorder.log.clone();
    (TestServer::start(recorder).await, log)
}

#[tokio::test]
async fn eng_100_a_reset_before_a_session_calls_nothing() {
    let (server, log) = recording().await;

    server.mta().await.reset();
    let mut mta = server.mta().await;
    mta.negotiate().await;
    mta.reset();

    // The server still serves new connections.
    let mut mta = server.mta().await;
    mta.reach(Reach::Conn).await;
    assert_eq!(log.events(), ["connect"]);
    server.stop().await;
}

#[tokio::test]
async fn eng_101_a_reset_outside_a_message_ends_the_session_with_an_error() {
    for state in [Reach::Conn, Reach::Helo, Reach::Endm, Reach::Unkn] {
        let (server, log) = recording().await;
        let mut mta = server.mta().await;
        mta.reach(state).await;

        mta.reset();

        let events = log.wait_for("session_end:Error").await;
        assert!(
            !events.contains(&"abort".to_owned()),
            "in {state:?}: {events:?}"
        );
        assert_eq!(events.last().unwrap(), "session_end:Error", "in {state:?}");
        server.stop().await;
    }
}

#[tokio::test]
async fn eng_102_a_reset_inside_a_message_aborts_it_first() {
    for state in Reach::IN_MESSAGE {
        let (server, log) = recording().await;
        let mut mta = server.mta().await;
        mta.reach(state).await;

        mta.reset();

        let events = log.wait_for("session_end:Error").await;
        assert_eq!(
            &events[events.len() - 2..],
            ["abort", "session_end:Error"],
            "in {state:?}"
        );
        server.stop().await;
    }
}

#[tokio::test]
async fn eng_103_a_reset_after_quit_new_connection_calls_nothing() {
    let (server, log) = recording().await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Endm).await;
    mta.send(SMFIC_QUIT_NC, Bytes::new()).await;
    let ended = log.wait_for("session_end:QuitNewConnection").await;

    mta.reset();

    let mut events = Vec::new();
    for event in &ended {
        events.push(event.as_str());
    }
    log.settles_at(&events).await;
    server.stop().await;
}

#[tokio::test]
async fn eng_104_a_reset_in_the_middle_of_a_frame_ends_the_session_with_an_error() {
    for (state, expected) in [
        (Reach::Rcpt, &["abort", "session_end:Error"][..]),
        (Reach::Helo, &["helo", "session_end:Error"][..]),
    ] {
        let (server, log) = recording().await;
        let mut mta = server.mta().await;
        mta.reach(state).await;

        // A length of 20 and the command byte, then nothing more.
        mta.write_raw(&[0, 0, 0, 20, SMFIC_HEADER]).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        mta.reset();

        let events = log.wait_for("session_end:Error").await;
        assert_eq!(&events[events.len() - 2..], expected, "in {state:?}");
        server.stop().await;
    }
}

/// Takes 200 ms at end of message, and logs the callbacks around it.
struct SlowEom(Log);

impl Handler for SlowEom {
    type Session = ();
    type Message = ();

    async fn end_of_message(&self, _ctx: &mut EomContext<Self>) -> Result<Decision> {
        tokio::time::sleep(Duration::from_millis(200)).await;
        self.0.push("end_of_message done");
        Ok(Decision::Continue)
    }

    async fn abort(&self, _ctx: &mut MessageContext<Self>) -> Result<()> {
        self.0.push("abort");
        Ok(())
    }

    async fn session_end(&self, _ctx: &mut SessionContext<Self>, reason: SessionEnd) -> Result<()> {
        self.0.push(format!("session_end:{reason:?}"));
        Ok(())
    }
}

#[tokio::test]
async fn eng_105_a_reset_while_a_callback_runs_aborts_after_it() {
    let log = Log::default();
    let handler = SlowEom(log.clone());
    let server = TestServer::start(handler).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Body).await;

    mta.send(SMFIC_BODYEOB, Bytes::new()).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    mta.reset();

    let events = log.wait_for("session_end:Error").await;
    assert_eq!(
        events,
        ["end_of_message done", "abort", "session_end:Error"]
    );
    server.stop().await;
}

#[tokio::test]
async fn eng_106_end_of_file_in_the_middle_of_a_frame_is_an_error() {
    let (server, log) = recording().await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Rcpt).await;

    mta.write_raw(&[0, 0, 0, 20, SMFIC_HEADER]).await;
    mta.close().await;

    let events = log.wait_for("session_end:Error").await;
    assert_eq!(&events[events.len() - 2..], ["abort", "session_end:Error"]);
    server.stop().await;
}

#[tokio::test]
async fn eng_107_end_of_file_between_frames_inside_a_message_aborts_it() {
    let (server, log) = recording().await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Rcpt).await;
    mta.ask_continue(SMFIC_HEADER, strings(&["Subject", "test"]))
        .await;

    mta.close().await;

    let events = log.wait_for("session_end:ConnectionClosed").await;
    assert_eq!(
        &events[events.len() - 2..],
        ["abort", "session_end:ConnectionClosed"]
    );
    server.stop().await;
}
