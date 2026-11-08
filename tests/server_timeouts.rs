//! Connection lifecycle and timeout scenarios (SRV-030 to SRV-040) from
//! `docs/testing/server.md`.

mod support;

use std::time::{Duration, Instant};

use rust_milter::protocol::*;
use rust_milter::{
    Actions, Bytes, Decision, EomContext, Error, Handler, MessageContext, MilterText, Result,
    ServerConfig, TimeoutKind,
};
use support::logs::Logs;
use support::mta::{Mta, Reach, strings};
use support::{Log, Recorder, TestServer};

fn timeouts(connect: u64, read: u64, write: u64, idle: u64) -> ServerConfig {
    let mut config = support::config();
    let t = &mut config.timeouts;
    t.connect = Duration::from_millis(connect);
    t.read = Duration::from_millis(read);
    t.write = Duration::from_millis(write);
    t.idle = Duration::from_millis(idle);
    config
}

/// How long until the milter closes the connection.
async fn closes_after(mta: &mut Mta, since: Instant) -> Duration {
    assert!(mta.closes().await, "the milter sent something");
    since.elapsed()
}

async fn recording(config: ServerConfig) -> (TestServer, Log) {
    let recorder = Recorder::default();
    let log = recorder.log.clone();
    (TestServer::start_with(config, recorder).await, log)
}

#[tokio::test]
async fn srv_030_closes_a_connection_that_sends_no_command() {
    let (logs, _guard) = Logs::capture();
    let (server, _) = recording(timeouts(200, 30_000, 30_000, 60_000)).await;
    let mut mta = server.mta().await;
    let accepted = Instant::now();

    let after = closes_after(&mut mta, accepted).await;

    assert!(
        after >= Duration::from_millis(150) && after < Duration::from_secs(1),
        "{after:?}"
    );
    server.stop().await;
    let timeout = logs.one("Connection timeout");
    assert_eq!(timeout.level, tracing::Level::DEBUG);
    assert_eq!(timeout.field("timeout_type"), Some("connect"));
    let closed = logs.one("Connection closed");
    assert_eq!(closed.level, tracing::Level::DEBUG);
    assert_eq!(closed.field("reason"), Some("timeout"));
    assert_eq!(
        closed.field("error"),
        Some(Error::Timeout(TimeoutKind::Connect).to_string().as_str())
    );
}

#[tokio::test]
async fn srv_031_the_first_command_ends_the_connect_timeout() {
    let (server, _) = recording(timeouts(200, 30_000, 30_000, 60_000)).await;
    let mut mta = server.mta().await;

    tokio::time::sleep(Duration::from_millis(100)).await;
    mta.negotiate().await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    mta.reach_after_negotiation(Reach::Conn).await;
    server.stop().await;
}

#[tokio::test]
async fn srv_032_closes_an_idle_session() {
    let (server, log) = recording(timeouts(10_000, 30_000, 30_000, 1_000)).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Helo).await;
    let replied = Instant::now();

    let after = closes_after(&mut mta, replied).await;

    assert!(
        after >= Duration::from_millis(900) && after < Duration::from_secs(2),
        "{after:?}"
    );
    log.wait_for("session_end:Error").await;
    server.stop().await;
}

#[tokio::test]
async fn srv_033_each_command_restarts_the_idle_wait() {
    let (server, _) = recording(timeouts(10_000, 30_000, 30_000, 1_000)).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Helo).await;

    for _ in 0..6 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        mta.ask_continue(SMFIC_UNKNOWN, strings(&["NOOP"])).await;
    }
    server.stop().await;
}

#[tokio::test]
async fn srv_034_closes_a_frame_that_stops_halfway() {
    let (server, _) = recording(timeouts(10_000, 200, 30_000, 60_000)).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Helo).await;

    // A 20-byte frame: the length field, the command, and 9 of 19 bytes.
    let mut half = vec![0, 0, 0, 20, SMFIC_HEADER];
    half.extend_from_slice(b"Subject\0x");
    mta.write_raw(&half).await;
    let sent = Instant::now();

    let after = closes_after(&mut mta, sent).await;

    assert!(
        after >= Duration::from_millis(150) && after < Duration::from_secs(1),
        "{after:?}"
    );
    server.stop().await;
}

#[tokio::test]
async fn srv_035_the_read_timeout_covers_the_whole_frame() {
    let (server, _) = recording(timeouts(10_000, 200, 30_000, 5_000)).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Helo).await;
    let mut frame = vec![0, 0, 0, 20, SMFIC_HEADER];
    frame.extend_from_slice(b"Subject\0value-12345\0");
    let started = Instant::now();

    // One byte every 50 ms: the whole frame would take 1.25 s.
    for byte in frame {
        if !mta.try_write_raw(&[byte]).await {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let after = closes_after(&mut mta, started).await;
    assert!(
        after < Duration::from_millis(1_250),
        "closed after {after:?}"
    );
    server.stop().await;
}

/// Sleeps 2 s in `mail`.
struct SlowMail;

impl Handler for SlowMail {
    type Session = ();
    type Message = ();

    async fn mail(
        &self,
        _ctx: &mut MessageContext<Self>,
        _sender: &MilterText,
        _args: &[MilterText],
    ) -> Result<Decision> {
        tokio::time::sleep(Duration::from_secs(2)).await;
        Ok(Decision::Continue)
    }
}

#[tokio::test]
async fn srv_036_time_in_a_callback_counts_against_no_timeout() {
    let server = TestServer::start_with(timeouts(10_000, 200, 200, 1_000), SlowMail).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Helo).await;
    let sent = Instant::now();

    mta.ask_continue(SMFIC_MAIL, strings(&["<s@void.invalid>"]))
        .await;

    assert!(sent.elapsed() >= Duration::from_millis(1_900));
    mta.ask_continue(SMFIC_RCPT, strings(&["<r@void.invalid>"]))
        .await;
    server.stop().await;
}

/// Replaces the body with 8 MB, and logs how `replace_body` ended.
struct HugeBody(Log);

impl Handler for HugeBody {
    type Session = ();
    type Message = ();

    fn actions(&self) -> Actions {
        Actions::CHANGE_BODY
    }

    async fn end_of_message(&self, ctx: &mut EomContext<Self>) -> Result<Decision> {
        let result = ctx.replace_body(&vec![b'x'; 8 * 1024 * 1024]).await;
        let outcome = match &result {
            Ok(()) => "sent".to_owned(),
            Err(Error::Timeout(kind)) => format!("timeout {kind:?}"),
            Err(other) => format!("{other:?}"),
        };
        self.0.push(outcome);
        result?;
        Ok(Decision::Continue)
    }
}

#[tokio::test]
async fn srv_037_a_write_to_an_mta_that_stops_reading_times_out() {
    let log = Log::default();
    let handler = HugeBody(log.clone());
    let mut config = timeouts(10_000, 30_000, 200, 60_000);
    config.socket_options.send_buffer_size = Some(4 * 1024);
    let server = TestServer::start_with(config, handler).await;
    let addr = server.address();
    let mut mta = Mta::connect_with_small_receive_buffer(addr).await;
    mta.reach(Reach::Body).await;
    let sent = Instant::now();

    // The fake MTA sends end of message and reads nothing more.
    mta.send(SMFIC_BODYEOB, Bytes::new()).await;

    log.wait_for(&format!("timeout {:?}", TimeoutKind::Write))
        .await;
    assert!(sent.elapsed() < Duration::from_secs(2));
    drop(mta);
    server.stop().await;
}

#[tokio::test]
async fn srv_038_closes_at_once_on_a_frame_longer_than_the_buffer() {
    let (server, _) = recording(support::config()).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Helo).await;

    mta.write_raw(&200_000u32.to_be_bytes()).await;
    let sent = Instant::now();

    let after = closes_after(&mut mta, sent).await;
    assert!(after < Duration::from_millis(500), "{after:?}");
    server.stop().await;
}

#[tokio::test]
async fn srv_039_end_of_file_halfway_through_a_frame_ends_the_session_with_an_error() {
    let (server, log) = recording(support::config()).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Helo).await;

    mta.write_raw(&[0, 0, 0, 20, SMFIC_HEADER]).await;
    mta.close().await;

    let events = log.wait_for("session_end:Error").await;
    assert_eq!(events.last().unwrap(), "session_end:Error");
    server.stop().await;
}

#[tokio::test]
async fn srv_040_a_reset_ends_the_session_with_an_error() {
    let (logs, _guard) = Logs::capture();
    let (server, log) = recording(support::config()).await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Helo).await;

    mta.reset();

    let events = log.wait_for("session_end:Error").await;
    assert_eq!(events.last().unwrap(), "session_end:Error");
    server.stop().await;
    let closed = logs.one("Connection closed");
    assert_eq!(closed.level, tracing::Level::DEBUG);
    assert_eq!(closed.field("reason"), Some("reset"));
    assert!(logs.warnings().is_empty(), "{:?}", logs.warnings());
}
