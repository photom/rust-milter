//! The trace and log scenarios of `docs/testing/telemetry.md`.
//!
//! One global subscriber sends every span and log record of this test
//! binary to in-memory exporters. Tests run in parallel, so each one picks
//! out its own traces: the connection spans on its leased port that started
//! after the test did, and the spans and log records of the same traces.

mod support;

use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

use opentelemetry::logs::AnyValue;
use opentelemetry::trace::{SpanId, SpanKind, Status, TraceId, TracerProvider as _};
use opentelemetry::{KeyValue, Value};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_sdk::logs::{InMemoryLogExporter, SdkLoggerProvider};
use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider, SpanData};
use rust_milter::protocol::*;
use rust_milter::{
    Actions, Bytes, ConnectDecision, ConnectInfo, Decision, EomContext, Handler, MessageContext,
    MilterText, ReplyCode, Result, SessionContext, TraceCommands,
};
use support::mta::{Mta, Reach, connect_data, macro_data, strings};
use support::{ContinueAll, TestServer};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::layer::SubscriberExt;

struct Exporters {
    spans: InMemorySpanExporter,
    logs: InMemoryLogExporter,
    _tracer: SdkTracerProvider,
    _logger: SdkLoggerProvider,
}

static EXPORTERS: OnceLock<Exporters> = OnceLock::new();

/// Installs the global subscriber once, with simple exporters, so a span is
/// exported as soon as it ends.
fn exporters() -> &'static Exporters {
    EXPORTERS.get_or_init(|| {
        let spans = InMemorySpanExporter::default();
        let tracer = SdkTracerProvider::builder()
            .with_simple_exporter(spans.clone())
            .build();
        let logs = InMemoryLogExporter::default();
        let logger = SdkLoggerProvider::builder()
            .with_simple_exporter(logs.clone())
            .build();
        let subscriber = tracing_subscriber::registry()
            .with(tracing_opentelemetry::layer().with_tracer(tracer.tracer("tests")))
            .with(OpenTelemetryTracingBridge::new(&logger));
        tracing::subscriber::set_global_default(subscriber)
            .expect("no other global subscriber in this test binary");
        Exporters {
            spans,
            logs,
            _tracer: tracer,
            _logger: logger,
        }
    })
}

/// The time a test holds its port. It opens once the server has its port
/// lease and closes before the lease is released, so no other test's
/// connection on the same port falls inside it.
struct Window {
    port: u16,
    since: SystemTime,
}

impl Window {
    fn open(server: &TestServer) -> Window {
        exporters();
        Window {
            port: server.address().port(),
            since: SystemTime::now(),
        }
    }

    /// Stops the server and returns the traces of the window.
    async fn close(self, server: TestServer) -> Traces {
        let until = SystemTime::now();
        server.stop().await;
        traces(self.port, self.since, until)
    }
}

/// The spans of the connections a test made on `port` in its window.
struct Traces {
    spans: Vec<SpanData>,
}

fn traces(port: u16, since: SystemTime, until: SystemTime) -> Traces {
    let all = exporters()
        .spans
        .get_finished_spans()
        .expect("the span exporter can be read");
    let ids: Vec<TraceId> = all
        .iter()
        .filter(|span| {
            span.name == "milter.connection"
                && span.start_time >= since
                && span.start_time <= until
                && attr(span, "server.port") == Some(Value::I64(i64::from(port)))
        })
        .map(|span| span.span_context.trace_id())
        .collect();
    let mut spans: Vec<SpanData> = all
        .into_iter()
        .filter(|span| ids.contains(&span.span_context.trace_id()))
        .collect();
    spans.sort_by_key(|span| span.start_time);
    Traces { spans }
}

impl Traces {
    fn named(&self, name: &str) -> Vec<&SpanData> {
        self.spans.iter().filter(|span| span.name == name).collect()
    }

    fn children(&self, parent: &SpanData) -> Vec<&SpanData> {
        self.spans
            .iter()
            .filter(|span| span.parent_span_id == parent.span_context.span_id())
            .collect()
    }

    /// The `milter.command` events of every span, in time order, with the
    /// span they are on.
    fn commands(&self) -> Vec<(&SpanData, &opentelemetry::trace::Event)> {
        let mut events: Vec<_> = self
            .spans
            .iter()
            .flat_map(|span| span.events.iter().map(move |event| (span, event)))
            .filter(|(_, event)| event.name == "milter.command")
            .collect();
        events.sort_by_key(|(_, event)| event.timestamp);
        events
    }

    fn command_names(&self) -> Vec<String> {
        self.commands()
            .iter()
            .map(|(_, event)| text(&event.attributes, "milter.command").unwrap_or_default())
            .collect()
    }
}

fn attr(span: &SpanData, key: &str) -> Option<Value> {
    find(&span.attributes, key)
}

fn find(attributes: &[KeyValue], key: &str) -> Option<Value> {
    attributes
        .iter()
        .find(|kv| kv.key.as_str() == key)
        .map(|kv| kv.value.clone())
}

fn text(attributes: &[KeyValue], key: &str) -> Option<String> {
    find(attributes, key).map(|value| value.as_str().into_owned())
}

fn span_text(span: &SpanData, key: &str) -> Option<String> {
    text(&span.attributes, key)
}

/// A server whose `trace_commands` is `commands`.
async fn server_with(commands: TraceCommands) -> TestServer {
    TestServer::start_with(
        |config| config.telemetry.trace_commands = commands,
        ContinueAll::new,
    )
    .await
}

async fn message(mta: &mut Mta) {
    mta.ask_continue(SMFIC_MAIL, strings(&["<sender@void.invalid>"]))
        .await;
    mta.ask_continue(SMFIC_RCPT, strings(&["<rcpt@void.invalid>"]))
        .await;
    mta.ask_continue(SMFIC_BODYEOB, Bytes::new()).await;
}

#[tokio::test]
async fn tel_020_spans_follow_connections_sessions_and_messages() {
    let server = TestServer::start(ContinueAll::new).await;
    let window = Window::open(&server);
    let mut mta = server.mta().await;
    mta.reach(Reach::Helo).await;
    message(&mut mta).await;
    message(&mut mta).await;
    mta.send(SMFIC_QUIT_NC, Bytes::new()).await;
    mta.ask_continue(
        SMFIC_CONNECT,
        connect_data("[192.0.2.2]", "192.0.2.2", 4712),
    )
    .await;
    mta.close().await;
    let traces = window.close(server).await;
    let connections = traces.named("milter.connection");
    assert_eq!(connections.len(), 1);
    let connection = connections[0];
    assert_eq!(connection.span_kind, SpanKind::Server);
    let sessions = traces.children(connection);
    let names: Vec<&str> = sessions.iter().map(|s| s.name.as_ref()).collect();
    assert_eq!(names, ["milter.session", "milter.session"]);
    let first = traces.children(sessions[0]);
    let names: Vec<&str> = first.iter().map(|s| s.name.as_ref()).collect();
    assert_eq!(names, ["milter.message", "milter.message"]);
    assert!(traces.children(sessions[1]).is_empty());
    assert_eq!(sessions[1].span_kind, SpanKind::Internal);
    let ids: Vec<Option<Value>> = sessions
        .iter()
        .map(|s| attr(s, "milter.session.id"))
        .collect();
    assert!(ids.iter().all(Option::is_some), "{ids:?}");
    assert_ne!(ids[0], ids[1]);
}

#[tokio::test]
async fn tel_021_a_message_span_carries_the_queue_id() {
    let server = TestServer::start(ContinueAll::new).await;
    let window = Window::open(&server);
    let mut with_id = server.mta().await;
    with_id.reach(Reach::Body).await;
    with_id
        .send(SMFIC_MACRO, macro_data(SMFIC_BODYEOB, &[("i", "4XyZ12")]))
        .await;
    with_id.ask_continue(SMFIC_BODYEOB, Bytes::new()).await;
    with_id.close().await;
    let mut without = server.mta().await;
    without.reach(Reach::Endm).await;
    without.close().await;
    let traces = window.close(server).await;
    let messages = traces.named("milter.message");
    let ids: Vec<Option<String>> = messages
        .iter()
        .map(|m| span_text(m, "milter.queue_id"))
        .collect();
    assert_eq!(ids, [Some("4XyZ12".to_owned()), None]);
}

#[tokio::test]
async fn tel_022_the_connection_span_names_both_ends() {
    let server = TestServer::start(ContinueAll::new).await;
    let window = Window::open(&server);
    let port = window.port;
    let mut mta = server.mta().await;
    mta.reach(Reach::Helo).await;
    mta.close().await;
    let traces = window.close(server).await;
    let connection = traces.named("milter.connection")[0];
    assert_eq!(
        span_text(connection, "network.transport").as_deref(),
        Some("tcp")
    );
    assert_eq!(
        span_text(connection, "network.type").as_deref(),
        Some("ipv4")
    );
    assert_eq!(
        span_text(connection, "network.peer.address").as_deref(),
        Some("127.0.0.1")
    );
    assert!(matches!(
        attr(connection, "network.peer.port"),
        Some(Value::I64(peer)) if peer > 0
    ));
    assert_eq!(
        span_text(connection, "server.address").as_deref(),
        Some("127.0.0.1")
    );
    assert_eq!(
        attr(connection, "server.port"),
        Some(Value::I64(i64::from(port)))
    );
    assert!(matches!(
        attr(connection, "milter.connection.id"),
        Some(Value::I64(1..))
    ));
    assert_eq!(
        attr(connection, "milter.protocol.version"),
        Some(Value::I64(6))
    );
}

#[tokio::test]
async fn tel_023_a_protocol_error_marks_the_connection_span() {
    let server = TestServer::start(ContinueAll::new).await;
    let window = Window::open(&server);
    let mut mta = server.mta().await;
    mta.negotiate().await;
    mta.send(b'Z', Bytes::new()).await;
    assert!(mta.closes().await);
    let traces = window.close(server).await;
    let connection = traces.named("milter.connection")[0];
    assert!(
        matches!(connection.status, Status::Error { .. }),
        "{:?}",
        connection.status
    );
    assert_eq!(
        span_text(connection, "error.type").as_deref(),
        Some("Protocol")
    );
}

/// The session of TEL-024: three headers, two body chunks, and a macro.
async fn full_session(mta: &mut Mta) {
    mta.negotiate().await;
    mta.ask_continue(
        SMFIC_CONNECT,
        connect_data("[192.0.2.1]", "192.0.2.1", 4711),
    )
    .await;
    mta.ask_continue(SMFIC_HELO, strings(&["client.void.invalid"]))
        .await;
    mta.send(SMFIC_MACRO, macro_data(SMFIC_MAIL, &[("{mail_addr}", "x")]))
        .await;
    mta.ask_continue(SMFIC_MAIL, strings(&["<sender@void.invalid>"]))
        .await;
    mta.ask_continue(SMFIC_RCPT, strings(&["<rcpt@void.invalid>"]))
        .await;
    mta.ask_continue(SMFIC_DATA, Bytes::new()).await;
    for name in ["Subject", "From", "To"] {
        mta.ask_continue(SMFIC_HEADER, strings(&[name, "value"]))
            .await;
    }
    mta.ask_continue(SMFIC_EOH, Bytes::new()).await;
    for chunk in ["one\r\n", "two\r\n"] {
        mta.ask_continue(SMFIC_BODY, Bytes::from(chunk)).await;
    }
    mta.ask_continue(SMFIC_BODYEOB, Bytes::new()).await;
    mta.send(SMFIC_QUIT, Bytes::new()).await;
    assert!(mta.closes().await);
}

#[tokio::test]
async fn tel_024_trace_commands_sets_which_commands_become_events() {
    let mut seen = Vec::new();
    for commands in [
        TraceCommands::None,
        TraceCommands::Envelope,
        TraceCommands::All,
    ] {
        let server = server_with(commands).await;
        let window = Window::open(&server);
        let mut mta = server.mta().await;
        full_session(&mut mta).await;
        seen.push(window.close(server).await.command_names());
    }

    let envelope = ["connect", "helo", "mail", "rcpt", "data", "eoh", "eom"];
    let all = [
        "optneg", "connect", "helo", "macro", "mail", "rcpt", "data", "header", "header", "header",
        "eoh", "body", "body", "eom", "quit",
    ];
    assert_eq!(seen[0], [] as [&str; 0]);
    assert_eq!(seen[1], envelope);
    assert_eq!(seen[2], all);
}

#[tokio::test]
async fn tel_025_span_names_are_fixed() {
    let server = TestServer::start(ContinueAll::new).await;
    let window = Window::open(&server);
    let mut mta = server.mta().await;
    mta.reach(Reach::Endm).await;
    mta.send(SMFIC_QUIT_NC, Bytes::new()).await;
    mta.reach_after_negotiation(Reach::Endm).await;
    mta.close().await;
    let traces = window.close(server).await;
    assert!(!traces.spans.is_empty());
    for span in &traces.spans {
        assert!(
            ["milter.connection", "milter.session", "milter.message"].contains(&span.name.as_ref()),
            "{}",
            span.name
        );
    }
}

/// Adds an attribute to the current span in `connect`.
struct Annotating;

impl Handler for Annotating {
    type Session = ();
    type Message = ();

    async fn connect(
        &self,
        _ctx: &mut SessionContext<Self>,
        _info: &ConnectInfo,
    ) -> Result<ConnectDecision> {
        tracing::Span::current().set_attribute("app.verdict", "clean");
        Ok(ConnectDecision::Continue)
    }
}

#[tokio::test]
async fn tel_026_a_handler_attribute_lands_on_the_session_span() {
    let server = TestServer::start(|_| Annotating).await;
    let window = Window::open(&server);
    let mut mta = server.mta().await;
    mta.reach(Reach::Helo).await;
    mta.close().await;
    let traces = window.close(server).await;
    let session = traces.named("milter.session")[0];
    assert_eq!(span_text(session, "app.verdict").as_deref(), Some("clean"));
    let connection = traces.named("milter.connection")[0];
    assert_eq!(span_text(connection, "app.verdict"), None);
}

/// Rejects the client `reject.void.invalid`, and the recipient
/// `<bad@void.invalid>`; accepts every message at its end.
struct Deciding;

impl Handler for Deciding {
    type Session = ();
    type Message = ();

    async fn connect(
        &self,
        _ctx: &mut SessionContext<Self>,
        info: &ConnectInfo,
    ) -> Result<ConnectDecision> {
        if info.hostname.as_bytes() == b"reject.void.invalid" {
            return Ok(ConnectDecision::Reject);
        }
        Ok(ConnectDecision::Continue)
    }

    async fn rcpt(
        &self,
        _ctx: &mut MessageContext<Self>,
        recipient: &MilterText,
        _args: &[MilterText],
    ) -> Result<Decision> {
        if recipient.as_bytes() == b"<bad@void.invalid>" {
            return Ok(Decision::Reject);
        }
        Ok(Decision::Continue)
    }

    async fn end_of_message(&self, _ctx: &mut EomContext<Self>) -> Result<Decision> {
        Ok(Decision::Accept)
    }
}

#[tokio::test]
async fn tel_027_session_and_message_spans_record_the_final_decision() {
    let server = TestServer::start(|_| Deciding).await;
    let window = Window::open(&server);
    let mut rejected = server.mta().await;
    rejected.negotiate().await;
    let reply = rejected
        .ask(
            SMFIC_CONNECT,
            connect_data("reject.void.invalid", "192.0.2.9", 4713),
        )
        .await;
    assert_eq!(reply.command, SMFIR_REJECT);
    rejected.close().await;
    let mut mta = server.mta().await;
    mta.reach(Reach::Rcpt).await;
    assert_eq!(
        mta.ask(SMFIC_BODYEOB, Bytes::new()).await.command,
        SMFIR_ACCEPT
    );
    mta.ask_continue(SMFIC_MAIL, strings(&["<sender@void.invalid>"]))
        .await;
    assert_eq!(
        mta.ask(SMFIC_RCPT, strings(&["<bad@void.invalid>"]))
            .await
            .command,
        SMFIR_REJECT
    );
    mta.ask_continue(SMFIC_RCPT, strings(&["<rcpt@void.invalid>"]))
        .await;
    assert_eq!(
        mta.ask(SMFIC_BODYEOB, Bytes::new()).await.command,
        SMFIR_ACCEPT
    );
    mta.ask_continue(SMFIC_MAIL, strings(&["<sender@void.invalid>"]))
        .await;
    mta.send(SMFIC_ABORT, Bytes::new()).await;
    mta.close().await;
    let traces = window.close(server).await;
    let sessions: Vec<Option<String>> = traces
        .named("milter.session")
        .iter()
        .map(|s| span_text(s, "milter.reply"))
        .collect();
    assert_eq!(sessions, [Some("reject".to_owned()), None]);
    let messages: Vec<Option<String>> = traces
        .named("milter.message")
        .iter()
        .map(|m| span_text(m, "milter.reply"))
        .collect();
    assert_eq!(
        messages,
        [Some("accept".to_owned()), Some("accept".to_owned()), None]
    );
}

#[tokio::test]
async fn tel_028_command_events_go_to_the_innermost_span() {
    let server = server_with(TraceCommands::Envelope).await;
    let window = Window::open(&server);
    let mut mta = server.mta().await;
    mta.reach(Reach::Mail).await;
    mta.send(SMFIC_ABORT, Bytes::new()).await;
    mta.close().await;
    let traces = window.close(server).await;
    let message = traces.named("milter.message")[0];
    let session = traces.named("milter.session")[0];
    let connection = traces.named("milter.connection")[0];
    let events = traces.commands();
    let on = |name: &str| {
        events
            .iter()
            .find(|(_, event)| text(&event.attributes, "milter.command").as_deref() == Some(name))
            .copied()
            .unwrap_or_else(|| panic!("no {name} event"))
    };
    let (span, mail) = on("mail");
    assert_eq!(span.span_context.span_id(), message.span_context.span_id());
    assert_eq!(
        text(&mail.attributes, "milter.reply").as_deref(),
        Some("continue")
    );
    let (span, abort) = on("abort");
    assert_eq!(span.span_context.span_id(), message.span_context.span_id());
    assert_eq!(
        text(&abort.attributes, "milter.reply").as_deref(),
        Some("noreply")
    );
    let (span, _) = on("connect");
    assert_eq!(span.span_context.span_id(), session.span_context.span_id());
    for (_, event) in &events {
        assert!(
            matches!(
                find(&event.attributes, "milter.command.duration"),
                Some(Value::F64(seconds)) if seconds >= 0.0
            ),
            "{event:?}"
        );
    }
    let negotiated: Vec<_> = connection
        .events
        .iter()
        .filter(|event| event.name == "milter.negotiated")
        .collect();
    assert_eq!(negotiated.len(), 1);
    assert_eq!(
        find(&negotiated[0].attributes, "milter.protocol.version"),
        Some(Value::I64(6))
    );
}

#[tokio::test]
async fn tel_029_a_timeout_or_a_reset_is_no_span_error() {
    let server = TestServer::start_with(
        |config| config.timeouts.idle = Duration::from_secs(1),
        ContinueAll::new,
    )
    .await;
    let window = Window::open(&server);
    let mut idle = server.mta().await;
    idle.negotiate().await;
    assert!(idle.closes().await);
    let mut reset = server.mta().await;
    reset.reach(Reach::Helo).await;
    reset.reset();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let traces = window.close(server).await;
    let connections = traces.named("milter.connection");
    assert_eq!(connections.len(), 2);
    for connection in connections {
        assert!(
            !matches!(connection.status, Status::Error { .. }),
            "{:?}",
            connection.status
        );
        assert_eq!(span_text(connection, "error.type"), None);
    }
}

/// Values that must not appear in any span, event, or log record.
const MARKERS: [&str; 14] = [
    "marker-host.void.invalid",
    "192.0.2.77",
    "marker-helo.void.invalid",
    "marker-sender@void.invalid",
    "marker-rcpt@void.invalid",
    "MARKER-ESMTP",
    "X-Marker-Name",
    "marker-header-value",
    "marker-body",
    "marker-macro-value",
    "marker-reply-text",
    "marker-quarantine",
    "marker-auth",
    "MARKERQID",
];

/// Replies with a custom text to every recipient but the first, and
/// quarantines every message.
struct Marking;

impl Handler for Marking {
    type Session = ();
    type Message = u32;

    fn actions(&self) -> Actions {
        Actions::QUARANTINE
    }

    async fn rcpt(
        &self,
        ctx: &mut MessageContext<Self>,
        _recipient: &MilterText,
        _args: &[MilterText],
    ) -> Result<Decision> {
        *ctx.message() += 1;
        if *ctx.message() == 1 {
            return Ok(Decision::Continue);
        }
        let reply = ReplyCode::new(550, Some("5.7.1"), "marker-reply-text").expect("a valid reply");
        Ok(Decision::Reply(reply))
    }

    async fn end_of_message(&self, ctx: &mut EomContext<Self>) -> Result<Decision> {
        ctx.quarantine("marker-quarantine").await?;
        Ok(Decision::Continue)
    }
}

#[tokio::test]
async fn tel_030_no_smtp_data_reaches_spans_events_or_logs() {
    let server = TestServer::start_with(
        |config| config.telemetry.trace_commands = TraceCommands::All,
        |_| Marking,
    )
    .await;
    let window = Window::open(&server);
    let mut mta = server.mta().await;
    mta.negotiate().await;
    mta.send(
        SMFIC_MACRO,
        macro_data(SMFIC_CONNECT, &[("{client_name}", "marker-macro-value")]),
    )
    .await;
    mta.ask_continue(
        SMFIC_CONNECT,
        connect_data("marker-host.void.invalid", "192.0.2.77", 4714),
    )
    .await;
    mta.ask_continue(SMFIC_HELO, strings(&["marker-helo.void.invalid"]))
        .await;
    mta.send(
        SMFIC_MACRO,
        macro_data(
            SMFIC_MAIL,
            &[("{auth_authen}", "marker-auth"), ("i", "MARKERQID")],
        ),
    )
    .await;
    mta.ask_continue(
        SMFIC_MAIL,
        strings(&["<marker-sender@void.invalid>", "MARKER-ESMTP"]),
    )
    .await;
    mta.ask_continue(SMFIC_RCPT, strings(&["<marker-rcpt@void.invalid>"]))
        .await;
    let reply = mta
        .ask(SMFIC_RCPT, strings(&["<marker-rcpt@void.invalid>"]))
        .await;
    assert_eq!(reply.command, SMFIR_REPLYCODE);
    mta.ask_continue(
        SMFIC_HEADER,
        strings(&["X-Marker-Name", "marker-header-value"]),
    )
    .await;
    mta.ask_continue(SMFIC_EOH, Bytes::new()).await;
    mta.ask_continue(SMFIC_BODY, Bytes::from("marker-body\r\n"))
        .await;
    mta.send(SMFIC_BODYEOB, Bytes::new()).await;
    let quarantine = mta.recv().await.expect("the quarantine reply");
    assert_eq!(quarantine.command, SMFIR_QUARANTINE);
    assert_eq!(
        mta.recv().await.expect("the final reply").command,
        SMFIR_CONTINUE
    );
    mta.send(SMFIC_QUIT, Bytes::new()).await;
    assert!(mta.closes().await);
    let traces = window.close(server).await;
    assert!(!traces.spans.is_empty());
    let mut found = Vec::new();
    for span in &traces.spans {
        for kv in &span.attributes {
            let allowed = span.name == "milter.message" && kv.key.as_str() == "milter.queue_id";
            if !allowed {
                found.extend(markers_in(&format!("{}={}", kv.key, kv.value), &span.name));
            }
        }
        found.extend(markers_in(&span.name, "span name"));
        for event in span.events.iter() {
            found.extend(markers_in(&format!("{event:?}"), &event.name));
        }
    }
    let ids: Vec<TraceId> = traces
        .spans
        .iter()
        .map(|s| s.span_context.trace_id())
        .collect();
    for log in logs_of(&ids) {
        found.extend(markers_in(&format!("{:?}", log.record), "log record"));
    }
    assert!(found.is_empty(), "{found:?}");
    let message = traces.named("milter.message")[0];
    assert_eq!(
        span_text(message, "milter.queue_id").as_deref(),
        Some("MARKERQID")
    );
}

fn markers_in(text: &str, place: &str) -> Vec<String> {
    MARKERS
        .iter()
        .filter(|marker| text.contains(*marker))
        .map(|marker| format!("{marker} in {place}: {text}"))
        .collect()
}

fn logs_of(
    traces: &[TraceId],
) -> Vec<opentelemetry_sdk::logs::in_memory_exporter::LogDataWithResource> {
    exporters()
        .logs
        .get_emitted_logs()
        .expect("the log exporter can be read")
        .into_iter()
        .filter(|log| {
            log.record
                .trace_context()
                .is_some_and(|context| traces.contains(&context.trace_id))
        })
        .collect()
}

#[tokio::test]
async fn tel_031_log_records_carry_the_span_they_were_written_in() {
    let server = server_with(TraceCommands::Envelope).await;
    let window = Window::open(&server);
    let mut mta = server.mta().await;
    mta.reach(Reach::Mail).await;
    mta.close().await;
    let traces = window.close(server).await;
    let message = traces.named("milter.message")[0];
    let connection = traces.named("milter.connection")[0];
    let ids = [connection.span_context.trace_id()];
    let logs = logs_of(&ids);
    let span_of = |body: &str| -> Option<SpanId> {
        logs.iter()
            .find(|log| {
                matches!(log.record.body(), Some(AnyValue::String(text)) if text.as_str() == body)
            })
            .and_then(|log| log.record.trace_context().map(|context| context.span_id))
    };
    let mail = logs
        .iter()
        .find(|log| {
            log.record.attributes_iter().any(|(key, value)| {
                key.as_str() == "milter.command" && format!("{value:?}").contains("mail")
            })
        })
        .expect("the mail command event as a log record");
    assert_eq!(
        mail.record.trace_context().map(|context| context.span_id),
        Some(message.span_context.span_id())
    );
    assert_eq!(
        span_of("Connection closed"),
        Some(connection.span_context.span_id())
    );
}
