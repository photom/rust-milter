//! Resource limit and concurrency scenarios (SRV-050 to SRV-052, SRV-056, SRV-058,
//! SRV-060, SRV-061, SRV-073, SRV-080 to SRV-083, SRV-085, SRV-086) from
//! `docs/testing/server.md`. SRV-053 to SRV-055 are unit tests of the accept
//! loop, and SRV-084 a unit test of the macro store.

mod support;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rust_milter::protocol::*;
use rust_milter::{
    Bytes, ConnectDecision, ConnectInfo, Decision, Handler, MessageContext, MilterText, Result,
    ServerConfig, SessionContext,
};
use support::logs::Logs;
use support::mta::{Mta, Reach, connect_data, macro_data, strings};
use support::{ContinueAll, Recorder, TestServer};

fn max_connections(max: usize) -> ServerConfig {
    let mut config = support::config();
    config.max_connections = max;
    config
}

fn tight_memory() -> ServerConfig {
    let mut config = support::config();
    config.resource_limits.per_connection_buffer = 65_540;
    config.resource_limits.max_memory = Some(65_540);
    config
}

/// The first part of a header frame of `length` bytes: the length field,
/// the command, and a few bytes of the data.
fn frame_start(length: u32) -> Vec<u8> {
    let mut start = length.to_be_bytes().to_vec();
    start.push(SMFIC_HEADER);
    start.extend_from_slice(b"X-Big\0");
    start
}

#[tokio::test]
async fn srv_050_closes_a_connection_over_max_connections_at_once() {
    let (logs, _guard) = Logs::capture();
    let server = TestServer::start_with(max_connections(2), ContinueAll).await;
    let mut first = server.mta().await;
    let mut second = server.mta().await;
    first.reach(Reach::Helo).await;
    second.reach(Reach::Helo).await;

    let mut third = server.mta().await;

    assert!(third.closes().await, "the third connection gets no data");
    first
        .ask_continue(SMFIC_MAIL, strings(&["<a@void.invalid>"]))
        .await;
    second
        .ask_continue(SMFIC_MAIL, strings(&["<b@void.invalid>"]))
        .await;
    let rejected = logs.one("Connection rejected (limit)");
    assert_eq!(rejected.level, tracing::Level::WARN);
    assert_eq!(rejected.field("max_connections"), Some("2"));
    assert_eq!(server.handle.status().connections_rejected, 1);
    server.stop().await;
}

#[tokio::test]
async fn srv_051_a_closed_connection_frees_its_place() {
    let server = TestServer::start_with(max_connections(2), ContinueAll).await;
    let mut first = server.mta().await;
    let second = server.mta().await;
    first.reach(Reach::Helo).await;
    second.close().await;
    wait_for_connections(&server, 1).await;

    let mut third = server.mta().await;

    third.reach(Reach::Helo).await;
    server.stop().await;
}

#[tokio::test]
async fn srv_052_closes_a_connection_whose_frame_would_pass_max_memory() {
    let (logs, _guard) = Logs::capture();
    let server = TestServer::start_with(tight_memory(), ContinueAll).await;
    let mut a = server.mta().await;
    a.reach(Reach::Data).await;
    a.write_raw(&frame_start(60_000)).await;
    // Let the server read the length field and reserve the frame.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let mut b = server.mta().await;
    b.reach(Reach::Data).await;

    b.write_raw(&frame_start(10_000)).await;

    assert!(b.closes().await);
    // A completes its frame and keeps working.
    a.write_raw(&frame_rest(60_000)).await;
    assert_eq!(a.recv().await.unwrap().command, SMFIR_CONTINUE);
    a.ask_continue(SMFIC_EOH, Bytes::new()).await;
    let mut found = None;
    for event in logs.named("Connection closed") {
        if event.field("reason") == Some("limit_reached") {
            found = Some(event);
            break;
        }
    }
    let closed = found.expect("B's close log");
    assert!(closed.field("error").is_some());
    server.stop().await;
}

#[tokio::test]
async fn srv_056_closes_a_new_connection_when_max_memory_is_full() {
    let (logs, _guard) = Logs::capture();
    let server = TestServer::start_with(tight_memory(), ContinueAll).await;
    let mut a = server.mta().await;
    a.reach(Reach::Data).await;
    a.write_raw(&frame_start(62_000)).await;
    // Let the server read the length field and reserve the frame.
    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut b = server.mta().await;

    assert!(b.closes().await, "B gets no data");
    let limit = logs.one("Resource limit reached");
    assert_eq!(limit.level, tracing::Level::WARN);
    assert_eq!(limit.field("limit_type"), Some("max_memory"));
    a.write_raw(&frame_rest(62_000)).await;
    assert_eq!(a.recv().await.unwrap().command, SMFIR_CONTINUE);
    server.stop().await;
}

/// The levels of the events with this message, in order.
fn levels(logs: &Logs, message: &str) -> Vec<tracing::Level> {
    let mut levels = Vec::new();
    for event in logs.named(message) {
        levels.push(event.level);
    }
    levels
}

/// Waits until the server counts `open` connections.
async fn wait_for_connections(server: &TestServer, open: usize) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while server.handle.status().connections != open {
        assert!(
            Instant::now() < deadline,
            "the server did not see the change"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn srv_085_only_the_first_rejection_of_a_run_logs_at_warn() {
    use tracing::Level;
    let (logs, _guard) = Logs::capture();
    let server = TestServer::start_with(max_connections(1), ContinueAll).await;
    let mut open = server.mta().await;
    open.reach(Reach::Helo).await;

    let mut second = server.mta().await;
    assert!(second.closes().await);
    let mut third = server.mta().await;
    assert!(third.closes().await);
    open.close().await;
    wait_for_connections(&server, 0).await;
    let mut served = server.mta().await;
    served.reach(Reach::Helo).await;
    let mut last = server.mta().await;
    assert!(last.closes().await);

    assert_eq!(
        levels(&logs, "Connection rejected (limit)"),
        [Level::WARN, Level::DEBUG, Level::WARN]
    );
    assert_eq!(server.handle.status().connections_rejected, 3);
    server.stop().await;
}

#[tokio::test]
async fn srv_086_only_the_first_memory_rejection_of_a_run_logs_at_warn() {
    use tracing::Level;
    let (logs, _guard) = Logs::capture();
    let server = TestServer::start_with(tight_memory(), ContinueAll).await;
    let mut a = server.mta().await;
    a.reach(Reach::Data).await;
    a.write_raw(&frame_start(62_000)).await;
    // Let the server read the length field and reserve the frame.
    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut b = server.mta().await;
    assert!(b.closes().await);
    let mut c = server.mta().await;
    assert!(c.closes().await);
    a.write_raw(&frame_rest(62_000)).await;
    assert_eq!(a.recv().await.unwrap().command, SMFIR_CONTINUE);
    let mut d = server.mta().await;
    d.reach(Reach::Helo).await;
    a.write_raw(&frame_start(60_000)).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let mut e = server.mta().await;
    assert!(e.closes().await);

    assert_eq!(
        levels(&logs, "Resource limit reached"),
        [Level::WARN, Level::DEBUG, Level::WARN]
    );
    // A connection finishes the frame it is receiving before it drains.
    a.write_raw(&frame_rest(60_000)).await;
    assert_eq!(a.recv().await.unwrap().command, SMFIR_CONTINUE);
    server.stop().await;
}

#[tokio::test]
async fn srv_060_serves_100_sessions_at_once() {
    let recorder = Recorder::default();
    let server = TestServer::start(recorder).await;
    let mut sessions = tokio::task::JoinSet::new();
    for _ in 0..100 {
        let mut mta = server.mta().await;
        sessions.spawn(async move {
            mta.reach(Reach::Endm).await;
            mta.send(SMFIC_QUIT, Bytes::new()).await;
            assert!(mta.closes().await);
        });
    }

    while let Some(done) = sessions.join_next().await {
        done.expect("a session failed");
    }
    server.stop().await;
}

/// Sleeps 2 s in `mail` on the connection whose hostname is `slow`.
#[derive(Default)]
struct SlowForOne;

impl Handler for SlowForOne {
    type Session = bool;
    type Message = ();

    async fn connect(
        &self,
        ctx: &mut SessionContext<Self>,
        info: &ConnectInfo,
    ) -> Result<ConnectDecision> {
        *ctx.session() = info.hostname.as_bytes() == b"slow";
        Ok(ConnectDecision::Continue)
    }

    async fn mail(
        &self,
        ctx: &mut MessageContext<Self>,
        _sender: &MilterText,
        _args: &[MilterText],
    ) -> Result<Decision> {
        if *ctx.session() {
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        Ok(Decision::Continue)
    }
}

#[tokio::test]
async fn srv_061_a_slow_callback_does_not_hold_up_other_connections() {
    let server = TestServer::start(SlowForOne).await;
    let mut slow = server.mta().await;
    slow.negotiate().await;
    slow.ask_continue(
        SMFIC_CONNECT,
        support::mta::connect_data("slow", "192.0.2.1", 4711),
    )
    .await;
    slow.send(SMFIC_MAIL, strings(&["<s@void.invalid>"])).await;
    let mut fast = server.mta().await;

    let times = Arc::new(Mutex::new(Vec::new()));
    fast.negotiate().await;
    for (command, data) in [
        (
            SMFIC_CONNECT,
            support::mta::connect_data("fast", "192.0.2.2", 4712),
        ),
        (SMFIC_HELO, strings(&["fast.void.invalid"])),
        (SMFIC_MAIL, strings(&["<f@void.invalid>"])),
        (SMFIC_RCPT, strings(&["<r@void.invalid>"])),
    ] {
        let sent = Instant::now();
        fast.ask_continue(command, data).await;
        times.lock().unwrap().push(sent.elapsed());
    }

    for took in times.lock().unwrap().iter() {
        assert!(*took < Duration::from_millis(100), "{took:?}");
    }
    assert_eq!(slow.recv().await.unwrap().command, SMFIR_CONTINUE);
    server.stop().await;
}

#[tokio::test]
async fn srv_073_status_counts_accepted_and_rejected_connections() {
    let server = TestServer::start_with(max_connections(1), ContinueAll).await;
    let mut first = server.mta().await;
    first.reach(Reach::Helo).await;

    for _ in 0..2 {
        let mut extra: Mta = server.mta().await;
        assert!(extra.closes().await);
    }

    let status = server.handle.status();
    assert_eq!(status.connections_accepted, 3);
    assert_eq!(status.connections_rejected, 2);
    server.stop().await;
}

#[tokio::test]
async fn srv_058_a_first_packet_reserves_no_room_before_negotiation() {
    let server = TestServer::start_with(tight_memory(), ContinueAll).await;
    let mut a = server.mta().await;
    a.write_raw(&62_000u32.to_be_bytes()).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut b = server.mta().await;
    b.negotiate().await;

    assert!(a.closes().await);
    server.stop().await;
}

/// A `per_connection_buffer` of 65,540 bytes and no `max_memory`.
fn small_buffer() -> ServerConfig {
    let mut config = support::config();
    config.resource_limits.per_connection_buffer = 65_540;
    config
}

/// The rest of a header frame that [`frame_start`] began.
fn frame_rest(length: u32) -> Vec<u8> {
    let start = frame_start(length).len();
    let mut rest = vec![b'v'; length as usize + 4 - start - 1];
    rest.push(0);
    rest
}

/// Whether a "Connection closed" log has the reason `limit_reached` and an
/// error that names `limit`.
fn closed_at_limit(logs: &Logs, limit: &str) -> bool {
    for event in logs.named("Connection closed") {
        if event.field("reason") != Some("limit_reached") {
            continue;
        }
        if let Some(error) = event.field("error") {
            if error.contains(limit) {
                return true;
            }
        }
    }
    false
}

#[tokio::test]
async fn srv_080_stored_macros_count_against_max_memory() {
    let (logs, _guard) = Logs::capture();
    let server = TestServer::start_with(tight_memory(), ContinueAll).await;
    let mut a = server.mta().await;
    a.negotiate().await;
    let value = "v".repeat(40_000);
    a.send(SMFIC_MACRO, macro_data(SMFIC_CONNECT, &[("{x}", &value)]))
        .await;
    a.ask_continue(
        SMFIC_CONNECT,
        connect_data("[192.0.2.1]", "192.0.2.1", 4711),
    )
    .await;
    let mut b = server.mta().await;
    b.negotiate().await;

    b.write_raw(&frame_start(30_000)).await;

    assert!(b.closes().await);
    a.ask_continue(SMFIC_HELO, strings(&["client.void.invalid"]))
        .await;
    assert!(closed_at_limit(&logs, "max_memory"));
    server.stop().await;
}

#[tokio::test]
async fn srv_081_stored_macros_may_not_pass_per_connection_buffer() {
    let (logs, _guard) = Logs::capture();
    let server = TestServer::start_with(small_buffer(), ContinueAll).await;
    let mut mta = server.mta().await;
    mta.negotiate().await;
    let value = "v".repeat(30_000);

    for stage in [SMFIC_CONNECT, SMFIC_HELO, SMFIC_MAIL] {
        mta.send(SMFIC_MACRO, macro_data(stage, &[("{x}", &value)]))
            .await;
    }

    assert!(mta.closes().await);
    assert!(closed_at_limit(&logs, "per_connection_buffer"));
    server.stop().await;
}

#[tokio::test]
async fn srv_082_the_bookkeeping_of_empty_macros_counts() {
    let (logs, _guard) = Logs::capture();
    let server = TestServer::start_with(small_buffer(), ContinueAll).await;
    let mut mta = server.mta().await;
    mta.negotiate().await;
    let pairs = vec![("", ""); 30_000];

    mta.send(SMFIC_MACRO, macro_data(SMFIC_CONNECT, &pairs))
        .await;

    assert!(mta.closes().await);
    assert!(closed_at_limit(&logs, "per_connection_buffer"));
    server.stop().await;
}

#[tokio::test]
async fn srv_083_dropped_macros_return_their_memory() {
    let server = TestServer::start_with(tight_memory(), ContinueAll).await;
    let mut a = server.mta().await;
    a.reach(Reach::Body).await;
    let value = "v".repeat(40_000);
    a.send(SMFIC_MACRO, macro_data(SMFIC_BODYEOB, &[("i", &value)]))
        .await;
    a.ask_continue(SMFIC_BODYEOB, Bytes::new()).await;
    a.ask_continue(SMFIC_MAIL, strings(&["<sender@void.invalid>"]))
        .await;
    let mut b = server.mta().await;
    b.reach(Reach::Data).await;

    b.write_raw(&frame_start(30_000)).await;
    b.write_raw(&frame_rest(30_000)).await;

    assert_eq!(b.recv().await.unwrap().command, SMFIR_CONTINUE);
    server.stop().await;
}
