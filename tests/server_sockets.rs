//! Socket option scenarios (SRV-020 to SRV-025) from
//! `docs/testing/server.md`. They read the server's accepted socket through
//! the `test-support` feature.
#![cfg(feature = "test-support")]

mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use rust_milter::protocol::*;
use rust_milter::{
    Bytes, ConnectDecision, ConnectInfo, Decision, EomContext, Handler, KeepaliveConfig,
    MessageContext, MilterText, Result, ServerConfig, SessionContext, SessionEnd, SocketReadback,
};
use support::TestServer;
use support::mta::{Reach, connect_data, strings};

/// The callback names and the options each one read.
type Reads = Arc<Mutex<Vec<(String, SocketReadback)>>>;

/// Reads its accepted socket's options in every callback.
#[derive(Clone, Default)]
struct ReadsOptions(Reads);

impl ReadsOptions {
    fn read<H: Handler>(&self, callback: &str, ctx: &SessionContext<H>) {
        let options = ctx
            .connection()
            .read_socket_options()
            .expect("the socket options can be read");
        self.0.lock().unwrap().push((callback.to_owned(), options));
    }
}

impl Handler for ReadsOptions {
    type Session = ();
    type Message = ();

    async fn connect(
        &self,
        ctx: &mut SessionContext<Self>,
        _info: &ConnectInfo,
    ) -> Result<ConnectDecision> {
        self.read("connect", ctx);
        Ok(ConnectDecision::Continue)
    }

    async fn helo(
        &self,
        ctx: &mut SessionContext<Self>,
        _name: &MilterText,
    ) -> Result<ConnectDecision> {
        self.read("helo", ctx);
        Ok(ConnectDecision::Continue)
    }

    async fn mail(
        &self,
        ctx: &mut MessageContext<Self>,
        _sender: &MilterText,
        _args: &[MilterText],
    ) -> Result<Decision> {
        self.read("mail", ctx);
        Ok(Decision::Continue)
    }

    async fn rcpt(
        &self,
        ctx: &mut MessageContext<Self>,
        _recipient: &MilterText,
        _args: &[MilterText],
    ) -> Result<Decision> {
        self.read("rcpt", ctx);
        Ok(Decision::Continue)
    }

    async fn data(&self, ctx: &mut MessageContext<Self>) -> Result<Decision> {
        self.read("data", ctx);
        Ok(Decision::Continue)
    }

    async fn header(
        &self,
        ctx: &mut MessageContext<Self>,
        _name: &MilterText,
        _value: &MilterText,
    ) -> Result<Decision> {
        self.read("header", ctx);
        Ok(Decision::Continue)
    }

    async fn end_of_headers(&self, ctx: &mut MessageContext<Self>) -> Result<Decision> {
        self.read("end_of_headers", ctx);
        Ok(Decision::Continue)
    }

    async fn body(
        &self,
        ctx: &mut MessageContext<Self>,
        _chunk: Bytes,
    ) -> Result<rust_milter::BodyDecision> {
        self.read("body", ctx);
        Ok(rust_milter::BodyDecision::Continue)
    }

    async fn end_of_message(&self, ctx: &mut EomContext<Self>) -> Result<Decision> {
        self.read("end_of_message", ctx);
        Ok(Decision::Continue)
    }

    async fn abort(&self, ctx: &mut MessageContext<Self>) -> Result<()> {
        self.read("abort", ctx);
        Ok(())
    }

    async fn unknown(
        &self,
        ctx: &mut SessionContext<Self>,
        _command: &MilterText,
    ) -> Result<Decision> {
        self.read("unknown", ctx);
        Ok(Decision::Continue)
    }

    async fn session_end(&self, ctx: &mut SessionContext<Self>, reason: SessionEnd) -> Result<()> {
        self.read(&format!("session_end:{reason:?}"), ctx);
        Ok(())
    }
}

/// Starts a server with `config`, connects, and reads the options in the
/// first callback.
async fn first_read(config: ServerConfig) -> SocketReadback {
    let handler = ReadsOptions::default();
    let reads = Arc::clone(&handler.0);
    let server = TestServer::start_with(config, handler).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Conn).await;
    server.stop().await;
    let reads = reads.lock().unwrap();
    reads[0].1.clone()
}

#[tokio::test]
async fn srv_020_sets_nodelay_and_keepalive_by_default() {
    let options = first_read(support::config()).await;

    assert!(options.tcp_nodelay);
    assert_eq!(
        options.keepalive,
        Some(KeepaliveConfig {
            time: Duration::from_secs(60),
            interval: Duration::from_secs(10),
            retries: 6,
        })
    );
}

#[tokio::test]
async fn srv_021_turns_keepalive_off() {
    let mut config = support::config();
    config.socket_options.keepalive = None;
    let options = first_read(config).await;

    assert_eq!(options.keepalive, None);
}

#[tokio::test]
async fn srv_022_sets_the_buffer_sizes() {
    let mut config = support::config();
    config.socket_options.recv_buffer_size = Some(64 * 1024);
    config.socket_options.send_buffer_size = Some(64 * 1024);
    let options = first_read(config).await;

    assert!(options.recv_buffer_size >= 64 * 1024, "{options:?}");
    assert!(options.send_buffer_size >= 64 * 1024, "{options:?}");
}

/// Runs a session through every stage, a second message that is aborted,
/// `SMFIC_QUIT_NC`, a second session, and a shutdown; returns what each
/// callback read.
async fn every_state(config: ServerConfig) -> Vec<(String, SocketReadback)> {
    let handler = ReadsOptions::default();
    let reads = Arc::clone(&handler.0);
    let server = TestServer::start_with(config, handler).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Endm).await;
    mta.ask_continue(SMFIC_UNKNOWN, strings(&["NOOP"])).await;
    mta.ask_continue(SMFIC_MAIL, strings(&["<second@void.invalid>"]))
        .await;
    mta.send(SMFIC_ABORT, Bytes::new()).await;
    mta.ask_continue(SMFIC_UNKNOWN, strings(&["NOOP"])).await;
    mta.send(SMFIC_QUIT_NC, Bytes::new()).await;
    mta.ask_continue(
        SMFIC_CONNECT,
        connect_data("[192.0.2.2]", "192.0.2.2", 4712),
    )
    .await;
    server.stop().await;
    let reads = reads.lock().unwrap();
    reads.clone()
}

/// Checks that every state was read, and that each read matches.
fn assert_every_state(
    reads: &[(String, SocketReadback)],
    nodelay: bool,
    keepalive: Option<KeepaliveConfig>,
) {
    let mut callbacks = Vec::new();
    for (name, _) in reads {
        callbacks.push(name.as_str());
    }
    assert_eq!(
        callbacks,
        [
            "connect",
            "helo",
            "mail",
            "rcpt",
            "data",
            "header",
            "end_of_headers",
            "body",
            "end_of_message",
            "unknown",
            "mail",
            "abort",
            "unknown",
            "session_end:QuitNewConnection",
            "connect",
            "session_end:ServerShutdown",
        ]
    );
    for (callback, options) in reads {
        assert_eq!(options.tcp_nodelay, nodelay, "in {callback}");
        assert_eq!(options.keepalive, keepalive, "in {callback}");
    }
}

#[tokio::test]
async fn srv_023_default_options_hold_in_every_state() {
    let reads = every_state(support::config()).await;

    assert_every_state(&reads, true, Some(KeepaliveConfig::default()));
}

#[tokio::test]
async fn srv_024_custom_keepalive_holds_in_every_state() {
    let custom = KeepaliveConfig {
        time: Duration::from_secs(30),
        interval: Duration::from_secs(5),
        retries: 3,
    };
    let mut config = support::config();
    config.socket_options.keepalive = Some(custom.clone());

    let reads = every_state(config).await;

    assert_every_state(&reads, true, Some(custom));
}

#[tokio::test]
async fn srv_025_options_turned_off_stay_off_in_every_state() {
    let mut config = support::config();
    config.socket_options.keepalive = None;
    config.socket_options.tcp_nodelay = false;

    let reads = every_state(config).await;

    assert_every_state(&reads, false, None);
}
