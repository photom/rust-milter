//! Basic flow: INT-001 to INT-004.

use std::sync::{Arc, Mutex};

use rust_milter::{
    Actions, ConnectDecision, ConnectInfo, Decision, EomContext, Handler, MessageContext,
    MilterText, Result, SessionContext, SessionEnd,
};
use rust_milter_interop::{Logs, Mail, Milter, Mta, Smtp, sink};

/// Adds `X-Filtered-By: rust-milter` at the end of each message.
struct AddsHeader;

impl Handler for AddsHeader {
    type Session = ();
    type Message = ();

    fn actions(&self) -> Actions {
        Actions::ADD_HEADERS
    }

    async fn end_of_message(&self, ctx: &mut EomContext<Self>) -> Result<Decision> {
        ctx.add_header("X-Filtered-By", "rust-milter").await?;
        Ok(Decision::Continue)
    }
}

#[tokio::test]
#[ignore = "runs in the interoperability stack"]
async fn int_001_a_header_added_at_end_of_message_is_delivered() {
    for mta in Mta::BOTH {
        let logs = Logs::capture();
        let milter = Milter::start(AddsHeader).await;
        let mail = Mail::new();

        let sent = Smtp::send_one(mta, &mail).await;

        assert!(sent.accepted(), "{mta}: {sent:?}");
        let delivered = sink::wait_for(&mail.token).await;
        assert_eq!(
            delivered.header("X-Filtered-By").as_deref(),
            Some("rust-milter"),
            "{mta}: {delivered:?}"
        );
        milter.wait_closed(1).await;
        milter.stop().await;
        let warnings = logs.warnings();
        assert!(warnings.is_empty(), "{mta}: {warnings:?}");
    }
}

/// What the milter saw in one session.
#[derive(Debug, Clone, Default)]
struct Seen {
    versions: Vec<u32>,
    unknown: usize,
    data: usize,
}

/// Records the negotiated version and the `unknown` and `data` calls.
#[derive(Clone, Default)]
struct RecordsVersion(Arc<Mutex<Seen>>);

impl Handler for RecordsVersion {
    type Session = ();
    type Message = ();

    async fn connect(
        &self,
        ctx: &mut SessionContext<Self>,
        _info: &ConnectInfo,
    ) -> Result<ConnectDecision> {
        let version = ctx.negotiated().version();
        self.0.lock().unwrap().versions.push(version);
        Ok(ConnectDecision::Continue)
    }

    async fn unknown(
        &self,
        _ctx: &mut SessionContext<Self>,
        _command: &MilterText,
    ) -> Result<Decision> {
        self.0.lock().unwrap().unknown += 1;
        Ok(Decision::Continue)
    }

    async fn data(&self, _ctx: &mut MessageContext<Self>) -> Result<Decision> {
        self.0.lock().unwrap().data += 1;
        Ok(Decision::Continue)
    }
}

#[tokio::test]
#[ignore = "runs in the interoperability stack"]
async fn int_002_postfix_negotiates_each_protocol_version() {
    for (port, version) in [(2602, 2), (2603, 3), (2604, 4), (25, 6)] {
        let handler = RecordsVersion::default();
        let seen = Arc::clone(&handler.0);
        let milter = Milter::start(handler).await;
        let mail = Mail::new();

        let mut smtp = Smtp::connect_port(Mta::Postfix, port).await;
        smtp.ehlo().await;
        let unknown = smtp.command("XUNKNOWN").await;
        let sent = smtp.send(&mail).await;
        smtp.quit().await;

        assert_eq!(unknown.code / 100, 5, "port {port}: {unknown}");
        assert!(sent.accepted(), "port {port}: {sent:?}");
        sink::wait_for(&mail.token).await;
        milter.wait_closed(1).await;
        milter.stop().await;
        let seen = seen.lock().unwrap().clone();
        assert_eq!(seen.versions, [version], "port {port}: {seen:?}");
        assert_eq!(
            seen.unknown,
            usize::from(version >= 3),
            "port {port}: {seen:?}"
        );
        assert_eq!(
            seen.data,
            usize::from(version >= 4),
            "port {port}: {seen:?}"
        );
    }
}

/// Records why each session ended.
#[derive(Clone, Default)]
struct RecordsEnd(Arc<Mutex<Vec<SessionEnd>>>);

impl Handler for RecordsEnd {
    type Session = ();
    type Message = ();

    async fn session_end(&self, _ctx: &mut SessionContext<Self>, reason: SessionEnd) -> Result<()> {
        self.0.lock().unwrap().push(reason);
        Ok(())
    }
}

#[tokio::test]
#[ignore = "runs in the interoperability stack"]
async fn int_003_a_session_without_mail_ends_cleanly() {
    for mta in Mta::BOTH {
        let logs = Logs::capture();
        let handler = RecordsEnd::default();
        let ends = Arc::clone(&handler.0);
        let milter = Milter::start(handler).await;

        let mut smtp = Smtp::connect(mta).await;
        smtp.ehlo().await;
        let reply = smtp.quit().await;

        assert_eq!(reply.code, 221, "{mta}: {reply}");
        milter.wait_closed(1).await;
        milter.stop().await;
        let ends = ends.lock().unwrap().clone();
        assert!(
            matches!(ends[..], [SessionEnd::Quit | SessionEnd::ConnectionClosed]),
            "{mta}: {ends:?}"
        );
        let warnings = logs.warnings();
        assert!(warnings.is_empty(), "{mta}: {warnings:?}");
    }
}

/// Per-message state that starts out unmarked.
#[derive(Default)]
struct Marked(bool);

/// Records, for each MAIL, the connection and whether the message state was
/// fresh, then marks it.
#[derive(Clone, Default)]
struct ChecksFreshMessage(Arc<Mutex<Vec<(u64, bool)>>>);

impl Handler for ChecksFreshMessage {
    type Session = ();
    type Message = Marked;

    async fn mail(
        &self,
        ctx: &mut MessageContext<Self>,
        _sender: &MilterText,
        _args: &[MilterText],
    ) -> Result<Decision> {
        let connection = ctx.connection().id;
        let message = ctx.message();
        let fresh = !message.0;
        message.0 = true;
        self.0.lock().unwrap().push((connection, fresh));
        Ok(Decision::Continue)
    }
}

#[tokio::test]
#[ignore = "runs in the interoperability stack"]
async fn int_004_two_messages_in_one_session_share_the_connection() {
    for mta in Mta::BOTH {
        let handler = ChecksFreshMessage::default();
        let seen = Arc::clone(&handler.0);
        let milter = Milter::start(handler).await;
        let first = Mail::new();
        let second = Mail::new();

        let mut smtp = Smtp::connect(mta).await;
        smtp.ehlo().await;
        let sent_first = smtp.send(&first).await;
        let sent_second = smtp.send(&second).await;
        smtp.quit().await;

        assert!(sent_first.accepted(), "{mta}: {sent_first:?}");
        assert!(sent_second.accepted(), "{mta}: {sent_second:?}");
        sink::wait_for(&first.token).await;
        sink::wait_for(&second.token).await;
        milter.wait_closed(1).await;
        milter.stop().await;
        let seen = seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 2, "{mta}: {seen:?}");
        assert_eq!(seen[0].0, seen[1].0, "{mta}: one connection: {seen:?}");
        for (_, fresh) in &seen {
            assert!(*fresh, "{mta}: {seen:?}");
        }
    }
}
