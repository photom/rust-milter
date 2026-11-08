//! The protocol engine: runs the Milter conversation on one connection and
//! calls the handler, as libmilter's `mi_engine` does.
//!
//! The rules it follows are in "Protocol engine behavior" in
//! `docs/reference/api-specification.md` and in ADR 0001.

mod io;
mod macros;
mod state;
mod trace;

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::sync::CancellationToken;
use tracing::field::Empty;
use tracing::{Instrument, Span};

pub(crate) use io::Outbound;
pub(crate) use macros::Macros;
pub(crate) use trace::{Trace, Traffic};

use io::Inbound;
use state::{State, Transitions};

use crate::handler::{
    Actions, BodyDecision, ConnectDecision, ConnectInfo, Decision, EomContext, Events, Handler,
    MessageContext, Negotiated, SessionContext, SessionEnd,
};
use crate::protocol::*;
use crate::server::{ConnectionInfo, Reservation};
use crate::{
    AddressFamily, Command, Frame, MilterText, ProtocolError, Reply, Result, TimeoutConfig,
};

/// How a connection ended, when no error ended it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Closed {
    /// The MTA closed it, or ended it with `SMFIC_QUIT`.
    ByClient,
    /// The server shut down.
    ByShutdown,
}

/// What happened on a connection, for the close log.
#[derive(Debug)]
pub(crate) struct Report {
    /// How the connection ended; the handler has already seen `abort` and
    /// `session_end` for it.
    pub(crate) outcome: Result<Closed>,
    pub(crate) bytes_read: u64,
    pub(crate) bytes_written: u64,
    pub(crate) commands: u64,
}

/// The limits of one connection.
pub(crate) struct Limits<'a> {
    /// The longest packet accepted, `per_connection_buffer`.
    pub(crate) max_packet: usize,
    pub(crate) timeouts: &'a TimeoutConfig,
    /// The connection's share of `max_memory`.
    pub(crate) memory: Reservation,
}

/// The tokens that end a connection.
#[derive(Clone)]
pub(crate) struct Stop {
    /// Cancelled when the shutdown grace period ends: the connection closes
    /// when it waits for the next command.
    pub(crate) shutdown: CancellationToken,
    /// Cancelled to force-close: the conversation is dropped wherever it is,
    /// and no more callbacks run.
    pub(crate) force: CancellationToken,
}

/// Runs the conversation on one connection until it ends. It runs inside
/// the connection's span, which becomes the parent of the session spans.
pub(crate) async fn run<H, R, W>(
    handler: &H,
    reader: R,
    writer: W,
    connection: Arc<ConnectionInfo>,
    limits: Limits<'_>,
    stop: Stop,
    trace: Trace,
) -> Report
where
    H: Handler,
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Send + Unpin + 'static,
{
    let accepted_at = tokio::time::Instant::from_std(connection.accepted_at);
    let timeouts = limits.timeouts;
    let mut engine = Engine {
        handler,
        inbound: Inbound::new(
            reader,
            limits.max_packet,
            timeouts.clone(),
            accepted_at,
            limits.memory,
            Arc::clone(&trace.traffic),
        ),
        ctx: EomContext {
            message: MessageContext {
                session: SessionContext::new(connection, stop.shutdown.clone()),
                message: None,
            },
            outbound: Outbound::new(writer, timeouts.write, Arc::clone(&trace.traffic)),
        },
        state: State::Init,
        rules: Transitions::new(),
        events: Events::ALL,
        in_session: false,
        body_skipped: false,
        connection_span: Span::current(),
        session_span: None,
        message_span: None,
        command_span: Span::none(),
        message_started: None,
        trace,
    };
    let outcome = tokio::select! {
        biased;
        () = stop.force.cancelled() => Ok(Closed::ByShutdown),
        outcome = engine.serve(&stop.shutdown) => outcome,
    };
    Report {
        outcome,
        bytes_read: engine.inbound.bytes_read,
        bytes_written: engine.ctx.outbound.bytes_written,
        commands: engine.inbound.frames,
    }
}

/// Where a decision came from; it decides how the state moves on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Origin {
    /// `connect` or `helo`.
    Session,
    /// `mail`, `data`, `header`, `end_of_headers`, or `body`.
    Message,
    /// `rcpt`: a reject applies to one recipient, so the state stays.
    Rcpt,
    /// `unknown`: a reject keeps the state too.
    Unknown,
}

/// Whether the connection stays open after a command.
enum Flow {
    Open,
    Close,
}

struct Engine<'h, H: Handler, R> {
    handler: &'h H,
    inbound: Inbound<R>,
    ctx: EomContext<H>,
    state: State,
    rules: Transitions,
    /// The stages the handler receives at the negotiated version.
    events: Events,
    in_session: bool,
    /// `body` returned `Skip` for the open message.
    body_skipped: bool,
    connection_span: Span,
    session_span: Option<Span>,
    message_span: Option<Span>,
    /// The innermost span open while the current command runs, for its
    /// `milter.command` event.
    command_span: Span,
    /// When the open message started, for `milter.server.message.duration`.
    message_started: Option<std::time::Instant>,
    trace: Trace,
}

impl<H: Handler, R: AsyncRead + Unpin + Send> Engine<'_, H, R> {
    async fn serve(&mut self, shutdown: &CancellationToken) -> Result<Closed> {
        loop {
            let frame = if self.inbound.in_frame() {
                // A connection finishes the command it is receiving, even
                // after shutdown.
                self.inbound.next_frame().await
            } else {
                tokio::select! {
                    biased;
                    () = shutdown.cancelled() => {
                        if self.inbound.in_frame() {
                            continue;
                        }
                        self.end(SessionEnd::ServerShutdown).await;
                        return Ok(Closed::ByShutdown);
                    }
                    frame = self.inbound.next_frame() => frame,
                }
            };
            let outcome = match frame {
                Ok(Some(frame)) => self.process(frame).await,
                Ok(None) => {
                    self.end(SessionEnd::ConnectionClosed).await;
                    return Ok(Closed::ByClient);
                }
                Err(error) => Err(error),
            };
            match outcome {
                Ok(Flow::Open) => {}
                Ok(Flow::Close) => return Ok(Closed::ByClient),
                Err(error) => {
                    self.end(SessionEnd::Error).await;
                    return Err(error);
                }
            }
        }
    }

    /// Processes one command, then records its `milter.command` event.
    async fn process(&mut self, frame: Frame) -> Result<Flow> {
        let code = frame.command;
        let started = std::time::Instant::now();
        self.ctx.outbound.last_reply = None;
        self.ctx.outbound.command = trace::command_index(code);
        if let Some(index) = self.ctx.outbound.command {
            self.trace.traffic.command(index);
        }
        self.command_span = self.span();
        let flow = self.handle(frame).await?;
        if trace::traced(self.trace.commands, code) {
            let reply = self.ctx.outbound.last_reply.unwrap_or("noreply");
            // Entered, not only named as the parent, so that a log bridge
            // gives the record this span's ID.
            let _entered = self.command_span.enter();
            tracing::debug!(
                milter.command = trace::command_name(code),
                milter.reply = reply,
                milter.command.duration = started.elapsed().as_secs_f64(),
                "milter.command"
            );
        }
        self.command_span = Span::none();
        Ok(flow)
    }

    async fn handle(&mut self, frame: Frame) -> Result<Flow> {
        let code = frame.command;
        let command = Command::from_frame(frame)?;

        if self.state == State::Init {
            let Command::OptNeg {
                version,
                actions,
                protocol,
            } = command
            else {
                return Err(ProtocolError::WrongState { command: code }.into());
            };
            self.negotiate(version, actions, protocol).await?;
            return Ok(Flow::Open);
        }
        if let Command::Macro { stage, macros } = command {
            self.ctx.message.session.macros.set(stage, macros);
            return Ok(Flow::Open);
        }

        let target = target_state(&command);
        if !self.rules.allows(self.state, target) {
            if matches!(command, Command::Abort | Command::Quit) {
                return Ok(Flow::Open);
            }
            if !self.rules.allows(State::Helo, target) {
                return Err(ProtocolError::WrongState { command: code }.into());
            }
            self.abort().await?;
            self.state = State::Helo;
        }
        if !self.in_session && !matches!(command, Command::Abort | Command::Quit) {
            self.open_session();
        }
        self.state = target;
        self.dispatch(command).await
    }

    async fn dispatch(&mut self, command: Command) -> Result<Flow> {
        let handler = self.handler;
        match command {
            Command::Connect {
                hostname,
                family,
                port,
                address,
            } => {
                let info = connect_info(hostname, family, port, address);
                let decision = if self.events.contains(Events::CONNECT) {
                    let span = self.span();
                    let ctx = self.session_ctx(Some(SMFIC_CONNECT));
                    handler.connect(ctx, &info).instrument(span).await?
                } else {
                    ConnectDecision::Continue
                };
                self.decide(decision.into(), Origin::Session).await?;
            }
            Command::Helo(name) => {
                let decision = if self.events.contains(Events::HELO) {
                    let span = self.span();
                    let ctx = self.session_ctx(Some(SMFIC_HELO));
                    handler.helo(ctx, &name).instrument(span).await?
                } else {
                    ConnectDecision::Continue
                };
                self.decide(decision.into(), Origin::Session).await?;
            }
            Command::Mail(args) => {
                self.abort().await?;
                self.open_message();
                let decision = if self.events.contains(Events::MAIL) {
                    let span = self.span();
                    let ctx = self.message_ctx(SMFIC_MAIL);
                    handler
                        .mail(ctx, &args[0], &args[1..])
                        .instrument(span)
                        .await?
                } else {
                    Decision::Continue
                };
                self.decide(decision, Origin::Message).await?;
            }
            Command::Rcpt(args) => {
                let decision = if self.events.contains(Events::RCPT) {
                    self.ensure_message();
                    let span = self.span();
                    let ctx = self.message_ctx(SMFIC_RCPT);
                    handler
                        .rcpt(ctx, &args[0], &args[1..])
                        .instrument(span)
                        .await?
                } else {
                    Decision::Continue
                };
                self.decide(decision, Origin::Rcpt).await?;
            }
            Command::Data => {
                let decision = if self.events.contains(Events::DATA) {
                    self.ensure_message();
                    let span = self.span();
                    handler
                        .data(self.message_ctx(SMFIC_DATA))
                        .instrument(span)
                        .await?
                } else {
                    Decision::Continue
                };
                self.decide(decision, Origin::Message).await?;
            }
            Command::Header { name, value } => {
                let decision = if self.events.contains(Events::HEADER) {
                    self.ensure_message();
                    let span = self.span();
                    let ctx = self.message_ctx(SMFIC_HEADER);
                    handler.header(ctx, &name, &value).instrument(span).await?
                } else {
                    Decision::Continue
                };
                self.decide(decision, Origin::Message).await?;
            }
            Command::Eoh => {
                let decision = if self.events.contains(Events::END_OF_HEADERS) {
                    self.ensure_message();
                    let span = self.span();
                    handler
                        .end_of_headers(self.message_ctx(SMFIC_EOH))
                        .instrument(span)
                        .await?
                } else {
                    Decision::Continue
                };
                self.decide(decision, Origin::Message).await?;
            }
            Command::Body(chunk) => {
                let decision = self.body(chunk).await?;
                self.decide_body(decision).await?;
            }
            Command::BodyEob(chunk) => self.end_of_message(chunk).await?,
            Command::Abort => {
                self.abort().await?;
                self.state = State::Helo;
            }
            Command::Unknown(text) => {
                let decision = if self.events.contains(Events::UNKNOWN) {
                    let span = self.span();
                    handler
                        .unknown(self.session_ctx(None), &text)
                        .instrument(span)
                        .await?
                } else {
                    Decision::Continue
                };
                self.decide(decision, Origin::Unknown).await?;
            }
            Command::Quit => {
                self.end(SessionEnd::Quit).await;
                return Ok(Flow::Close);
            }
            Command::QuitNewConnection => self.end(SessionEnd::QuitNewConnection).await,
            Command::OptNeg { .. } | Command::Macro { .. } => {
                unreachable!("handled before the state check")
            }
        }
        Ok(Flow::Open)
    }

    async fn negotiate(&mut self, version: u32, actions: u32, protocol: u32) -> Result<()> {
        if version < SMFI_PROT_VERSION_MIN {
            return Err(ProtocolError::VersionTooOld(version).into());
        }
        let version = version.min(SMFI_PROT_VERSION);
        let mta_actions = if actions == 0 { SMFI_V1_ACTS } else { actions };
        let mta_protocol = if protocol == 0 {
            SMFI_V1_PROT
        } else {
            protocol
        };

        let wanted = self.handler.actions();
        let missing = wanted.difference(Actions::from_bits_truncate(mta_actions));
        if !missing.is_empty() {
            return Err(ProtocolError::ActionsNotOffered(missing).into());
        }
        let events = self.handler.events();
        let switched_off = Events::ALL.difference(events).no_flags() & mta_protocol;

        self.ctx
            .outbound
            .send(&Reply::OptNeg {
                version,
                actions: wanted.bits(),
                protocol: switched_off,
                macros: Vec::new(),
            })
            .await?;

        let mut delivered = events;
        if version < 3 {
            delivered = delivered.difference(Events::UNKNOWN);
        }
        if version < 4 {
            delivered = delivered.difference(Events::DATA);
        }
        self.events = delivered;
        self.rules = Transitions::for_protocol(switched_off);
        self.ctx.message.session.negotiated = Negotiated {
            version,
            actions: wanted,
            protocol: switched_off,
            mta_protocol,
        };
        self.state = State::Opts;
        self.trace.traffic.negotiated(version);
        self.connection_span
            .record("milter.protocol.version", i64::from(version));
        tracing::debug!(
            parent: &self.connection_span,
            milter.protocol.version = i64::from(version),
            "milter.negotiated"
        );
        Ok(())
    }

    async fn body(&mut self, chunk: bytes::Bytes) -> Result<BodyDecision> {
        if !self.events.contains(Events::BODY) || self.body_skipped {
            return Ok(BodyDecision::Continue);
        }
        let handler = self.handler;
        self.ensure_message();
        let span = self.span();
        handler
            .body(self.message_ctx(SMFIC_BODY), chunk)
            .instrument(span)
            .await
    }

    async fn decide_body(&mut self, decision: BodyDecision) -> Result<()> {
        let decision = match decision {
            BodyDecision::Continue => Decision::Continue,
            BodyDecision::Accept => Decision::Accept,
            BodyDecision::Reject => Decision::Reject,
            BodyDecision::TempFail => Decision::TempFail,
            BodyDecision::Reply(code) => Decision::Reply(code),
            BodyDecision::Discard => Decision::Discard,
            BodyDecision::Skip => {
                self.body_skipped = true;
                let negotiated = self.ctx.message.session.negotiated;
                let reply = if negotiated.version >= 6 && negotiated.mta_protocol & SMFIP_SKIP != 0
                {
                    Reply::Skip
                } else {
                    Reply::Continue
                };
                return self.ctx.outbound.send(&reply).await;
            }
        };
        self.decide(decision, Origin::Message).await
    }

    async fn end_of_message(&mut self, chunk: bytes::Bytes) -> Result<()> {
        if !chunk.is_empty() {
            match self.body(chunk).await? {
                BodyDecision::Continue | BodyDecision::Skip => {}
                other => return self.decide_body(other).await,
            }
        }
        self.ensure_message();
        self.ctx.message.session.stage = Some(SMFIC_BODYEOB);
        #[cfg(feature = "otel")]
        let eom_started = std::time::Instant::now();
        let span = self.span();
        let decision = self
            .handler
            .end_of_message(&mut self.ctx)
            .instrument(span)
            .await?;
        let continues = decision == Decision::Continue;
        let reply = reply_for(decision);
        self.ctx.outbound.send(&reply).await?;
        self.record_reply(trace::reply_name(&reply));
        #[cfg(feature = "otel")]
        {
            let histograms = &self.trace.histograms;
            let name = trace::reply_name(&reply);
            histograms.eom(eom_started.elapsed(), name);
            if let Some(started) = self.message_started {
                histograms.message(started.elapsed(), name);
            }
        }
        self.close_message();
        self.state = if continues { State::Endm } else { State::Helo };
        Ok(())
    }

    /// Sends the reply for a decision and moves the state on, as
    /// `mi_engine` does after a callback.
    async fn decide(&mut self, decision: Decision, origin: Origin) -> Result<()> {
        let ends = match decision {
            Decision::Continue => false,
            Decision::Accept => true,
            _ => !matches!(origin, Origin::Rcpt | Origin::Unknown),
        };
        let ends_session = origin == Origin::Session && decision != Decision::Continue;
        let reply = reply_for(decision);
        self.ctx.outbound.send(&reply).await?;
        if ends_session {
            if let Some(session) = &self.session_span {
                session.record("milter.reply", trace::reply_name(&reply));
            }
        }
        if ends {
            if origin == Origin::Unknown {
                self.abort().await?;
            } else {
                self.record_reply(trace::reply_name(&reply));
            }
            self.close_message();
            self.state = State::Helo;
        }
        Ok(())
    }

    /// Calls `abort` for the open message, if any, and drops it.
    async fn abort(&mut self) -> Result<()> {
        if self.ctx.message.message.is_none() {
            return Ok(());
        }
        self.ctx.message.session.stage = None;
        let span = self.span();
        let result = self
            .handler
            .abort(&mut self.ctx.message)
            .instrument(span)
            .await;
        self.close_message();
        result
    }

    /// Ends the session, if one is open: `abort` for an open message, then
    /// `session_end`. Errors from these callbacks are dropped, because the
    /// session ends either way.
    async fn end(&mut self, reason: SessionEnd) {
        if !self.in_session {
            return;
        }
        let _ = self.abort().await;
        self.ctx.message.session.stage = None;
        let span = self.span();
        let _ = self
            .handler
            .session_end(&mut self.ctx.message.session, reason)
            .instrument(span)
            .await;
        self.ctx.message.session.reset();
        self.in_session = false;
        self.session_span = None;
        self.state = State::QuitNc;
    }

    /// Starts a session and its span, a child of the connection span.
    fn open_session(&mut self) {
        self.in_session = true;
        let id = self
            .trace
            .sessions
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let span = tracing::info_span!(
            parent: &self.connection_span,
            "milter.session",
            milter.session.id = attribute(id),
            milter.reply = Empty,
        );
        self.command_span = span.clone();
        self.session_span = Some(span);
    }

    fn open_message(&mut self) {
        self.ctx.message.message = Some(H::Message::default());
        self.body_skipped = false;
        let parent = self.span();
        let span = tracing::info_span!(
            parent: &parent,
            "milter.message",
            milter.queue_id = Empty,
            milter.reply = Empty,
        );
        self.command_span = span.clone();
        self.message_span = Some(span);
        self.message_started = Some(std::time::Instant::now());
    }

    fn ensure_message(&mut self) {
        if self.ctx.message.message.is_none() {
            self.open_message();
        }
    }

    /// Drops the open message and ends its span, with the queue ID if the
    /// MTA sent one.
    fn close_message(&mut self) {
        self.ctx.message.message = None;
        self.body_skipped = false;
        self.message_started = None;
        if let Some(span) = self.message_span.take() {
            let macros = &self.ctx.message.session.macros;
            if let Some(id) = macros.get("i", Some(SMFIC_BODYEOB)) {
                span.record("milter.queue_id", id.to_string_lossy().as_ref());
            }
        }
    }

    /// Sets `milter.reply` on the open message's span.
    fn record_reply(&self, reply: &'static str) {
        if let Some(span) = &self.message_span {
            span.record("milter.reply", reply);
        }
    }

    /// The innermost open span: the message's, the session's, or the
    /// connection's.
    fn span(&self) -> Span {
        self.message_span
            .as_ref()
            .or(self.session_span.as_ref())
            .unwrap_or(&self.connection_span)
            .clone()
    }

    fn session_ctx(&mut self, stage: Option<u8>) -> &mut SessionContext<H> {
        self.ctx.message.session.stage = stage;
        &mut self.ctx.message.session
    }

    fn message_ctx(&mut self, stage: u8) -> &mut MessageContext<H> {
        self.ensure_message();
        self.ctx.message.session.stage = Some(stage);
        &mut self.ctx.message
    }
}

/// A counter as a span attribute. `tracing-opentelemetry` records a `u64`
/// as text, and an `i64` as an integer.
pub(crate) fn attribute(count: u64) -> i64 {
    i64::try_from(count).unwrap_or(i64::MAX)
}

/// The state a command moves to.
fn target_state(command: &Command) -> State {
    match command {
        Command::OptNeg { .. } | Command::Macro { .. } => State::Opts,
        Command::Connect { .. } => State::Conn,
        Command::Helo(_) => State::Helo,
        Command::Mail(_) => State::Mail,
        Command::Rcpt(_) => State::Rcpt,
        Command::Data => State::Data,
        Command::Header { .. } => State::Hdrs,
        Command::Eoh => State::Eohs,
        Command::Body(_) => State::Body,
        Command::BodyEob(_) => State::Endm,
        Command::Abort => State::Abrt,
        Command::Unknown(_) => State::Unkn,
        Command::Quit => State::Quit,
        Command::QuitNewConnection => State::QuitNc,
    }
}

fn reply_for(decision: Decision) -> Reply {
    match decision {
        Decision::Continue => Reply::Continue,
        Decision::Accept => Reply::Accept,
        Decision::Reject => Reply::Reject,
        Decision::TempFail => Reply::TempFail,
        Decision::Discard => Reply::Discard,
        Decision::Reply(code) => Reply::from(code),
    }
}

fn connect_info(
    hostname: MilterText,
    family: AddressFamily,
    port: Option<u16>,
    address: Option<MilterText>,
) -> ConnectInfo {
    let address = match (family, port, address) {
        (AddressFamily::Inet | AddressFamily::Inet6, Some(port), Some(text)) => {
            match ip_address(&text) {
                Some(ip) => Some(SocketAddr::new(ip, port)),
                None => None,
            }
        }
        _ => None,
    };
    ConnectInfo { hostname, address }
}

/// Reads an IP address as the MTA writes it. Sendmail can put `IPv6:` in
/// front of an IPv6 address.
fn ip_address(text: &MilterText) -> Option<IpAddr> {
    let text = text.as_str()?;
    let text = match text.get(..5) {
        Some(prefix) if prefix.eq_ignore_ascii_case("IPv6:") => &text[5..],
        _ => text,
    };
    text.parse().ok()
}
