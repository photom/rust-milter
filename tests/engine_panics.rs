//! Panics in the handler (ENG-110 to ENG-115) from
//! `docs/testing/protocol-engine.md`.

mod support;

use std::time::Instant;

use rust_milter::protocol::*;
use rust_milter::{
    Bytes, ConnectDecision, ConnectInfo, Decision, EomContext, Events, Handler, MessageContext,
    MilterText, Result, ServerHandle, SessionContext, SessionEnd,
};
use support::mta::{Reach, connect_data, strings};
use support::{Log, TestServer};

/// The handler method that panics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Panics {
    Connect,
    Header,
    EndOfMessage,
    Abort,
    SessionEnd,
    Events,
}

/// Records the callbacks it sees, and panics in one of them.
struct Panicking {
    panics: Panics,
    log: Log,
}

impl Panicking {
    /// Records `name`, and panics if `here` is the method that panics.
    fn enter(&self, here: Panics, name: &str) {
        self.log.push(name);
        if self.panics == here {
            panic!("the test handler panics in {name}");
        }
    }
}

impl Handler for Panicking {
    type Session = ();
    type Message = ();

    fn events(&self) -> Events {
        if self.panics == Panics::Events {
            panic!("the test handler panics in events");
        }
        Events::ALL
    }

    async fn connect(
        &self,
        _ctx: &mut SessionContext<Self>,
        _info: &ConnectInfo,
    ) -> Result<ConnectDecision> {
        self.enter(Panics::Connect, "connect");
        Ok(ConnectDecision::Continue)
    }

    async fn header(
        &self,
        _ctx: &mut MessageContext<Self>,
        _name: &MilterText,
        _value: &MilterText,
    ) -> Result<Decision> {
        self.enter(Panics::Header, "header");
        Ok(Decision::Continue)
    }

    async fn end_of_message(&self, _ctx: &mut EomContext<Self>) -> Result<Decision> {
        self.enter(Panics::EndOfMessage, "end_of_message");
        Ok(Decision::Continue)
    }

    async fn abort(&self, _ctx: &mut MessageContext<Self>) -> Result<()> {
        self.enter(Panics::Abort, "abort");
        Ok(())
    }

    async fn session_end(
        &self,
        _ctx: &mut SessionContext<Self>,
        _reason: SessionEnd,
    ) -> Result<()> {
        self.enter(Panics::SessionEnd, "session_end");
        Ok(())
    }
}

/// Starts a server whose handler panics in `panics`, and returns it with
/// the handler's log.
async fn start(panics: Panics) -> (TestServer, Log) {
    let log = Log::default();
    let handler = Panicking {
        panics,
        log: log.clone(),
    };
    (TestServer::start(handler).await, log)
}

/// Whether the server counts one connection closed with the reason `error`
/// and `error.type` `HandlerPanic`, waiting for the count until the fake
/// MTA's timeout.
async fn closed_by_panic(handle: &ServerHandle) -> bool {
    let started = Instant::now();
    while started.elapsed() < support::mta::WAIT {
        for (reason, error, count) in handle.status().connections_closed {
            if reason == "error" && error == Some("HandlerPanic") && count == 1 {
                return true;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    false
}

/// Whether a new connection to `server` negotiates and gets a reply to
/// connect, so the server serves on after a panic.
async fn still_serves(server: &TestServer) -> bool {
    let mut mta = server.mta().await;
    mta.negotiate().await;
    let reply = mta
        .ask(
            SMFIC_CONNECT,
            connect_data("[192.0.2.2]", "192.0.2.2", 4712),
        )
        .await;
    reply.command == SMFIR_CONTINUE
}

#[tokio::test]
async fn eng_110_a_panic_in_connect_closes_the_connection_with_no_further_callback() {
    let (server, log) = start(Panics::Connect).await;
    let mut mta = server.mta().await;
    mta.negotiate().await;

    mta.send(
        SMFIC_CONNECT,
        connect_data("[192.0.2.1]", "192.0.2.1", 4711),
    )
    .await;

    assert!(mta.closes().await);
    assert!(closed_by_panic(&server.handle).await);
    assert_eq!(log.events(), ["connect"]);
    let mut next = server.mta().await;
    next.negotiate().await;
    server.stop().await;
}

#[tokio::test]
async fn eng_111_a_panic_in_header_calls_neither_abort_nor_session_end() {
    let (server, log) = start(Panics::Header).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Data).await;

    mta.send(SMFIC_HEADER, strings(&["Subject", "test"])).await;

    assert!(mta.closes().await);
    assert!(closed_by_panic(&server.handle).await);
    assert_eq!(log.events(), ["connect", "header"]);
    assert!(still_serves(&server).await);
    server.stop().await;
}

#[tokio::test]
async fn eng_112_a_panic_in_end_of_message_sends_no_final_reply() {
    let (server, log) = start(Panics::EndOfMessage).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Body).await;

    mta.send(SMFIC_BODYEOB, Bytes::new()).await;

    assert!(mta.closes().await);
    assert!(closed_by_panic(&server.handle).await);
    assert_eq!(
        log.events(),
        ["connect", "header", "end_of_message"],
        "no abort or session_end after the panic"
    );
    server.stop().await;
}

#[tokio::test]
async fn eng_113_a_panic_in_abort_skips_session_end() {
    let (server, log) = start(Panics::Abort).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Rcpt).await;

    mta.send(SMFIC_QUIT, Bytes::new()).await;

    assert!(mta.closes().await);
    assert!(closed_by_panic(&server.handle).await);
    assert_eq!(log.events(), ["connect", "abort"]);
    server.stop().await;
}

#[tokio::test]
async fn eng_114_a_panic_in_session_end_closes_with_handler_panic() {
    let (server, log) = start(Panics::SessionEnd).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Helo).await;

    mta.send(SMFIC_QUIT, Bytes::new()).await;

    assert!(mta.closes().await);
    assert!(closed_by_panic(&server.handle).await);
    assert_eq!(log.events(), ["connect", "session_end"]);
    server.stop().await;
}

#[tokio::test]
async fn eng_115_a_panic_in_events_closes_the_connection_without_a_reply() {
    let (server, log) = start(Panics::Events).await;
    let mut mta = server.mta().await;

    assert!(
        mta.optneg(6, SMFI_CURR_ACTS, SMFI_CURR_PROT)
            .await
            .is_none()
    );

    assert!(closed_by_panic(&server.handle).await);
    assert!(log.events().is_empty());
    assert!(server.handle.is_running());
    server.stop().await;
}
