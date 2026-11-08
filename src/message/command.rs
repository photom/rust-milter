//! Decoding the commands the MTA sends.

use bytes::{Buf, Bytes};

use super::MilterText;
use crate::protocol::*;
use crate::{Frame, ProtocolError, Result};

/// Length of the three 32-bit fields of an option negotiation.
const OPTNEG_LENGTH: usize = 12;

/// A command from the MTA (`SMFIC_*`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Command {
    /// `SMFIC_OPTNEG`: the MTA's protocol version, actions, and protocol
    /// flags, as sent.
    OptNeg {
        /// Protocol version.
        version: u32,
        /// `SMFIF_*` action flags.
        actions: u32,
        /// `SMFIP_*` protocol flags.
        protocol: u32,
    },
    /// `SMFIC_MACRO`: macro definitions for the command that follows.
    Macro {
        /// The raw stage byte: the command code the macros belong to,
        /// including `L`, `B`, and `U` from Postfix.
        stage: u8,
        /// Name and value pairs, in the order the MTA sent them.
        macros: Vec<(MilterText, MilterText)>,
    },
    /// `SMFIC_CONNECT`: the SMTP client's hostname and address.
    Connect {
        /// The client's hostname; `[<address>]` when it did not resolve.
        hostname: MilterText,
        /// The address family (`SMFIA_*`).
        family: AddressFamily,
        /// The client's port; `None` for [`AddressFamily::Unknown`].
        port: Option<u16>,
        /// The client's address as text; `None` for
        /// [`AddressFamily::Unknown`].
        address: Option<MilterText>,
    },
    /// `SMFIC_HELO`: the HELO or EHLO name.
    Helo(MilterText),
    /// `SMFIC_MAIL`: the sender, then its ESMTP arguments.
    Mail(Vec<MilterText>),
    /// `SMFIC_RCPT`: the recipient, then its ESMTP arguments.
    Rcpt(Vec<MilterText>),
    /// `SMFIC_DATA`: the DATA command.
    Data,
    /// `SMFIC_HEADER`: one header field. A folded value keeps LF between its
    /// lines.
    Header {
        /// The header name.
        name: MilterText,
        /// The header value.
        value: MilterText,
    },
    /// `SMFIC_EOH`: end of headers.
    Eoh,
    /// `SMFIC_BODY`: one body chunk, unchanged.
    Body(Bytes),
    /// `SMFIC_BODYEOB`: end of body, with optional trailing body data.
    BodyEob(Bytes),
    /// `SMFIC_ABORT`: abort the current message.
    Abort,
    /// `SMFIC_UNKNOWN`: an SMTP command the MTA does not know.
    Unknown(MilterText),
    /// `SMFIC_QUIT`: the SMTP session ended.
    Quit,
    /// `SMFIC_QUIT_NC`: the SMTP session ended; a new one follows on the same
    /// connection.
    QuitNewConnection,
}

/// The address family of the SMTP client (`SMFIA_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AddressFamily {
    /// `SMFIA_UNKNOWN`: the MTA sent no port or address.
    Unknown,
    /// `SMFIA_UNIX`: a Unix or local socket.
    Unix,
    /// `SMFIA_INET`: IPv4.
    Inet,
    /// `SMFIA_INET6`: IPv6.
    Inet6,
}

impl Command {
    /// Decodes a frame from the MTA.
    ///
    /// Fails with [`Error::Protocol`](crate::Error::Protocol) on an unknown
    /// command code or data that does not match the command's format,
    /// including bytes after the last field. An option negotiation may be
    /// longer than its 12 bytes, as libmilter accepts.
    pub fn from_frame(frame: Frame) -> Result<Command> {
        let Frame { command, mut data } = frame;
        match command {
            SMFIC_OPTNEG => {
                if data.len() < OPTNEG_LENGTH {
                    return Err(ProtocolError::Malformed { command }.into());
                }
                Ok(Command::OptNeg {
                    version: data.get_u32(),
                    actions: data.get_u32(),
                    protocol: data.get_u32(),
                })
            }
            SMFIC_MACRO => required(decode_macro(data), command),
            SMFIC_CONNECT => required(decode_connect(data), command),
            SMFIC_HELO => Ok(Command::Helo(required(only_one(data), command)?)),
            SMFIC_MAIL => Ok(Command::Mail(required(strings(data), command)?)),
            SMFIC_RCPT => Ok(Command::Rcpt(required(strings(data), command)?)),
            SMFIC_DATA => Ok(Command::Data),
            SMFIC_HEADER => {
                let name = required(MilterText::take(&mut data), command)?;
                let value = required(only_one(data), command)?;
                Ok(Command::Header { name, value })
            }
            SMFIC_EOH => Ok(Command::Eoh),
            SMFIC_BODY => Ok(Command::Body(data)),
            SMFIC_BODYEOB => Ok(Command::BodyEob(data)),
            SMFIC_ABORT => Ok(Command::Abort),
            SMFIC_UNKNOWN => Ok(Command::Unknown(required(only_one(data), command)?)),
            SMFIC_QUIT => Ok(Command::Quit),
            SMFIC_QUIT_NC => Ok(Command::QuitNewConnection),
            other => Err(ProtocolError::UnknownCommand(other).into()),
        }
    }
}

/// The value decoded from the data of `command`, or a
/// [`ProtocolError::Malformed`] error when the data did not decode.
fn required<T>(decoded: Option<T>, command: u8) -> Result<T> {
    match decoded {
        Some(value) => Ok(value),
        None => Err(ProtocolError::Malformed { command }.into()),
    }
}

/// Decodes `SMFIC_MACRO` data: a stage byte, then name and value pairs.
fn decode_macro(mut data: Bytes) -> Option<Command> {
    if data.is_empty() {
        return None;
    }
    let stage = data.get_u8();
    let mut macros = Vec::new();
    while !data.is_empty() {
        let name = MilterText::take(&mut data)?;
        let value = MilterText::take(&mut data)?;
        macros.push((name, value));
    }
    Some(Command::Macro { stage, macros })
}

/// Decodes `SMFIC_CONNECT` data: hostname, family, and for a known family
/// the port and address.
fn decode_connect(mut data: Bytes) -> Option<Command> {
    let hostname = MilterText::take(&mut data)?;
    if data.is_empty() {
        return None;
    }
    let family = match data.get_u8() {
        SMFIA_UNKNOWN => {
            if !data.is_empty() {
                return None;
            }
            return Some(Command::Connect {
                hostname,
                family: AddressFamily::Unknown,
                port: None,
                address: None,
            });
        }
        SMFIA_UNIX => AddressFamily::Unix,
        SMFIA_INET => AddressFamily::Inet,
        SMFIA_INET6 => AddressFamily::Inet6,
        _ => return None,
    };
    if data.len() < 2 {
        return None;
    }
    let port = data.get_u16();
    let address = only_one(data)?;
    Some(Command::Connect {
        hostname,
        family,
        port: Some(port),
        address: Some(address),
    })
}

/// Reads `data` as exactly one NUL-terminated string, with nothing after it.
fn only_one(mut data: Bytes) -> Option<MilterText> {
    let text = MilterText::take(&mut data)?;
    data.is_empty().then_some(text)
}

/// Splits `data` into one or more NUL-terminated strings.
///
/// Returns `None` when `data` is empty or does not end with a NUL.
fn strings(mut data: Bytes) -> Option<Vec<MilterText>> {
    let mut texts = Vec::new();
    while !data.is_empty() {
        texts.push(MilterText::take(&mut data)?);
    }
    (!texts.is_empty()).then_some(texts)
}
