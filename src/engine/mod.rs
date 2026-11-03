//! The protocol engine: runs the Milter conversation on one connection and
//! calls the handler, as libmilter's `mi_engine` does.
//!
//! The rules it follows are in "Protocol engine behavior" in
//! `docs/reference/api-specification.md` and in ADR 0001.

mod io;
mod macros;
mod state;

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::sync::CancellationToken;

pub(crate) use io::Outbound;
pub(crate) use macros::Macros;

use io::Inbound;
use state::{State, Transitions};

use crate::handler::{
    Actions, BodyDecision, ConnectDecision, ConnectInfo, Decision, EomContext, Events, Handler,
    MessageContext, Negotiated, SessionContext, SessionEnd,
};
use crate::protocol::*;
use crate::server::ConnectionInfo;
use crate::{
    AddressFamily, Command, Frame, MilterText, ProtocolError, Reply, Result, TimeoutConfig,
};

/// Runs the conversation on one connection until it ends.
///
/// Returns the error that closed the connection, if any. The handler has
/// already seen `abort` and `session_end` for it.
pub(crate) async fn run<H, R, W>(
    handler: &H,
    reader: R,
    writer: W,
    connection: Arc<ConnectionInfo>,
    max_packet: usize,
    timeouts: &TimeoutConfig,
    shutdown: CancellationToken,
) -> Result<()>
where
    H: Handler,
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Send + Unpin + 'static,
{
    let accepted_at = tokio::time::Instant::from_std(connection.accepted_at);
    let mut engine = Engine {
        handler,
        inbound: Inbound::new(reader, max_packet, timeouts.clone(), accepted_at),
        ctx: EomContext {
            message: MessageContext {
                session: SessionContext::new(connection, shutdown.clone()),
                message: None,
            },
            outbound: Outbound::new(writer, timeouts.write),
        },
        state: State::Init,
        rules: Transitions::new(),
        events: Events::ALL,
        in_session: false,
        body_skipped: false,
    };
    engine.serve(&shutdown).await
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
}

impl<H: Handler, R: AsyncRead + Unpin + Send> Engine<'_, H, R> {
    async fn serve(&mut self, shutdown: &CancellationToken) -> Result<()> {
        loop {
            let frame = tokio::select! {
                biased;
                () = shutdown.cancelled() => {
                    self.end(SessionEnd::ServerShutdown).await;
                    return Ok(());
                }
                frame = self.inbound.next_frame() => frame,
            };
            let outcome = match frame {
                Ok(Some(frame)) => self.process(frame).await,
                Ok(None) => {
                    self.end(SessionEnd::ConnectionClosed).await;
                    return Ok(());
                }
                Err(error) => Err(error),
            };
            match outcome {
                Ok(Flow::Open) => {}
                Ok(Flow::Close) => return Ok(()),
                Err(error) => {
                    self.end(SessionEnd::Error).await;
                    return Err(error);
                }
            }
        }
    }

    async fn process(&mut self, frame: Frame) -> Result<Flow> {
        let code = frame.command;
        let command = Command::from_frame(frame)?;
        let wrong_state = || ProtocolError::WrongState { command: code };

        if self.state == State::Init {
            let Command::OptNeg {
                version,
                actions,
                protocol,
            } = command
            else {
                return Err(wrong_state().into());
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
                return Err(wrong_state().into());
            }
            self.abort().await?;
            self.state = State::Helo;
        }
        if !self.in_session && !matches!(command, Command::Abort | Command::Quit) {
            self.in_session = true;
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
                    let ctx = self.session_ctx(Some(SMFIC_CONNECT));
                    handler.connect(ctx, &info).await?
                } else {
                    ConnectDecision::Continue
                };
                self.decide(decision.into(), Origin::Session).await?;
            }
            Command::Helo(name) => {
                let decision = if self.events.contains(Events::HELO) {
                    let ctx = self.session_ctx(Some(SMFIC_HELO));
                    handler.helo(ctx, &name).await?
                } else {
                    ConnectDecision::Continue
                };
                self.decide(decision.into(), Origin::Session).await?;
            }
            Command::Mail(args) => {
                self.abort().await?;
                self.open_message();
                let decision = if self.events.contains(Events::MAIL) {
                    let ctx = self.message_ctx(SMFIC_MAIL);
                    handler.mail(ctx, &args[0], &args[1..]).await?
                } else {
                    Decision::Continue
                };
                self.decide(decision, Origin::Message).await?;
            }
            Command::Rcpt(args) => {
                let decision = if self.events.contains(Events::RCPT) {
                    let ctx = self.message_ctx(SMFIC_RCPT);
                    handler.rcpt(ctx, &args[0], &args[1..]).await?
                } else {
                    Decision::Continue
                };
                self.decide(decision, Origin::Rcpt).await?;
            }
            Command::Data => {
                let decision = if self.events.contains(Events::DATA) {
                    handler.data(self.message_ctx(SMFIC_DATA)).await?
                } else {
                    Decision::Continue
                };
                self.decide(decision, Origin::Message).await?;
            }
            Command::Header { name, value } => {
                let decision = if self.events.contains(Events::HEADER) {
                    let ctx = self.message_ctx(SMFIC_HEADER);
                    handler.header(ctx, &name, &value).await?
                } else {
                    Decision::Continue
                };
                self.decide(decision, Origin::Message).await?;
            }
            Command::Eoh => {
                let decision = if self.events.contains(Events::END_OF_HEADERS) {
                    handler.end_of_headers(self.message_ctx(SMFIC_EOH)).await?
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
                    handler.unknown(self.session_ctx(None), &text).await?
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
        Ok(())
    }

    async fn body(&mut self, chunk: bytes::Bytes) -> Result<BodyDecision> {
        if !self.events.contains(Events::BODY) || self.body_skipped {
            return Ok(BodyDecision::Continue);
        }
        let handler = self.handler;
        handler.body(self.message_ctx(SMFIC_BODY), chunk).await
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
        let decision = self.handler.end_of_message(&mut self.ctx).await?;
        let continues = decision == Decision::Continue;
        self.ctx.outbound.send(&reply_for(decision)).await?;
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
        self.ctx.outbound.send(&reply_for(decision)).await?;
        if ends {
            if origin == Origin::Unknown {
                self.abort().await?;
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
        let result = self.handler.abort(&mut self.ctx.message).await;
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
        let _ = self
            .handler
            .session_end(&mut self.ctx.message.session, reason)
            .await;
        self.ctx.message.session.reset();
        self.in_session = false;
        self.state = State::QuitNc;
    }

    fn open_message(&mut self) {
        self.ctx.message.message = Some(H::Message::default());
        self.body_skipped = false;
    }

    fn ensure_message(&mut self) {
        if self.ctx.message.message.is_none() {
            self.open_message();
        }
    }

    fn close_message(&mut self) {
        self.ctx.message.message = None;
        self.body_skipped = false;
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
            ip_address(&text).map(|ip| SocketAddr::new(ip, port))
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
