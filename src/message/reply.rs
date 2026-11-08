//! Encoding the replies the milter sends.

use bytes::{BufMut, Bytes, BytesMut};

use crate::protocol::*;
use crate::{Frame, ReplyError, Result};

/// The most body data one `SMFIR_REPLBODY` frame carries.
const MAX_BODY_CHUNK: usize = 65_535;

/// A reply from the milter (`SMFIR_*`), and the milter's answer to option
/// negotiation.
///
/// This is the wire-level type. Handlers return decisions and
/// [`ReplyCode`](crate::ReplyCode) values instead.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Reply {
    /// The answer to `SMFIC_OPTNEG`.
    OptNeg {
        /// Protocol version.
        version: u32,
        /// `SMFIF_*` action flags.
        actions: u32,
        /// `SMFIP_*` protocol flags.
        protocol: u32,
        /// The macros requested per `SMFIM_*` stage, as space-separated
        /// names.
        macros: Vec<(u32, String)>,
    },
    /// `SMFIR_CONTINUE`.
    Continue,
    /// `SMFIR_ACCEPT`.
    Accept,
    /// `SMFIR_REJECT`.
    Reject,
    /// `SMFIR_TEMPFAIL`.
    TempFail,
    /// `SMFIR_DISCARD`.
    Discard,
    /// `SMFIR_REPLYCODE`: the SMTP reply as it goes on the wire.
    ReplyCode(String),
    /// `SMFIR_SKIP`.
    Skip,
    /// `SMFIR_PROGRESS`.
    Progress,
    /// `SMFIR_QUARANTINE`, with the reason.
    Quarantine(String),
    /// `SMFIR_ADDHEADER`.
    AddHeader {
        /// The header name.
        name: String,
        /// The header value.
        value: String,
    },
    /// `SMFIR_INSHEADER`.
    InsertHeader {
        /// Position: 0 inserts before the first header.
        index: u32,
        /// The header name.
        name: String,
        /// The header value.
        value: String,
    },
    /// `SMFIR_CHGHEADER`; an empty value deletes the header.
    ChangeHeader {
        /// Which occurrence of the header, counting from 1.
        index: u32,
        /// The header name.
        name: String,
        /// The new value.
        value: String,
    },
    /// `SMFIR_ADDRCPT`.
    AddRecipient(String),
    /// `SMFIR_ADDRCPT_PAR`.
    AddRecipientWithArgs {
        /// The recipient.
        rcpt: String,
        /// ESMTP arguments, if any.
        args: Option<String>,
    },
    /// `SMFIR_DELRCPT`.
    DeleteRecipient(String),
    /// `SMFIR_CHGFROM`.
    ChangeFrom {
        /// The new sender.
        from: String,
        /// ESMTP arguments, if any.
        args: Option<String>,
    },
    /// `SMFIR_REPLBODY`: one chunk of the new body.
    ReplaceBody(Bytes),
}

impl Reply {
    /// Encodes the reply as a frame.
    ///
    /// Fails with [`Error::InvalidReply`](crate::Error::InvalidReply) when
    /// the reply breaks a rule of the protocol or of either MTA. The
    /// [`ReplyError`] variants list the rules.
    pub fn to_frame(&self) -> Result<Frame> {
        self.validate()?;
        let mut data = BytesMut::new();
        let command = match self {
            Reply::OptNeg {
                version,
                actions,
                protocol,
                macros,
            } => {
                data.put_u32(*version);
                data.put_u32(*actions);
                data.put_u32(*protocol);
                for (stage, names) in macros {
                    data.put_u32(*stage);
                    put_str(&mut data, names);
                }
                SMFIC_OPTNEG
            }
            Reply::Continue => SMFIR_CONTINUE,
            Reply::Accept => SMFIR_ACCEPT,
            Reply::Reject => SMFIR_REJECT,
            Reply::TempFail => SMFIR_TEMPFAIL,
            Reply::Discard => SMFIR_DISCARD,
            Reply::ReplyCode(text) => {
                put_str(&mut data, text);
                SMFIR_REPLYCODE
            }
            Reply::Skip => SMFIR_SKIP,
            Reply::Progress => SMFIR_PROGRESS,
            Reply::Quarantine(reason) => {
                put_str(&mut data, reason);
                SMFIR_QUARANTINE
            }
            Reply::AddHeader { name, value } => {
                put_str(&mut data, name);
                put_str(&mut data, value);
                SMFIR_ADDHEADER
            }
            Reply::InsertHeader { index, name, value } => {
                data.put_u32(*index);
                put_str(&mut data, name);
                put_str(&mut data, value);
                SMFIR_INSHEADER
            }
            Reply::ChangeHeader { index, name, value } => {
                data.put_u32(*index);
                put_str(&mut data, name);
                put_str(&mut data, value);
                SMFIR_CHGHEADER
            }
            Reply::AddRecipient(rcpt) => {
                put_str(&mut data, rcpt);
                SMFIR_ADDRCPT
            }
            Reply::AddRecipientWithArgs { rcpt, args } => {
                put_str(&mut data, rcpt);
                if let Some(args) = args {
                    put_str(&mut data, args);
                }
                SMFIR_ADDRCPT_PAR
            }
            Reply::DeleteRecipient(rcpt) => {
                put_str(&mut data, rcpt);
                SMFIR_DELRCPT
            }
            Reply::ChangeFrom { from, args } => {
                put_str(&mut data, from);
                if let Some(args) = args {
                    put_str(&mut data, args);
                }
                SMFIR_CHGFROM
            }
            Reply::ReplaceBody(chunk) => {
                data.put_slice(chunk);
                SMFIR_REPLBODY
            }
        };
        Ok(Frame::new(command, data.freeze()))
    }
}

impl Reply {
    fn validate(&self) -> std::result::Result<(), ReplyError> {
        match self {
            Reply::OptNeg { macros, .. } => {
                for (_, names) in macros {
                    check_text(names)?;
                }
            }
            Reply::ReplyCode(text) => {
                check_text(text)?;
                check_reply_lines(text)?;
                check_percent(text)?;
            }
            Reply::Quarantine(text) | Reply::AddRecipient(text) | Reply::DeleteRecipient(text) => {
                check_required(text)?;
            }
            Reply::AddHeader { name, value } | Reply::InsertHeader { name, value, .. } => {
                check_new_header_name(name)?;
                check_header_value(value)?;
            }
            Reply::ChangeHeader { name, value, .. } => {
                check_delivered_header_name(name)?;
                check_header_value(value)?;
            }
            Reply::AddRecipientWithArgs {
                rcpt: address,
                args,
            }
            | Reply::ChangeFrom {
                from: address,
                args,
            } => {
                check_required(address)?;
                if let Some(args) = args {
                    check_single_line(args)?;
                }
            }
            Reply::ReplaceBody(chunk) => {
                if chunk.len() > MAX_BODY_CHUNK {
                    return Err(ReplyError::BodyChunkTooLarge {
                        length: chunk.len(),
                    });
                }
            }
            Reply::Continue
            | Reply::Accept
            | Reply::Reject
            | Reply::TempFail
            | Reply::Discard
            | Reply::Skip
            | Reply::Progress => {}
        }
        Ok(())
    }
}

/// Rejects a NUL byte, which would end the field early on the wire.
fn check_text(text: &str) -> std::result::Result<(), ReplyError> {
    if text.contains('\0') {
        return Err(ReplyError::NulByte);
    }
    Ok(())
}

/// Rejects a NUL byte, CR, and LF, so the text stays one field on one line.
fn check_single_line(text: &str) -> std::result::Result<(), ReplyError> {
    check_text(text)?;
    if text.contains(['\r', '\n']) {
        return Err(ReplyError::LineBreak);
    }
    Ok(())
}

/// Rejects an empty address or reason, as libmilter does, and anything
/// [`check_single_line`] rejects.
fn check_required(text: &str) -> std::result::Result<(), ReplyError> {
    if text.is_empty() {
        return Err(ReplyError::EmptyField);
    }
    check_single_line(text)
}

/// Requires the name of a new header to be printable ASCII without a colon,
/// as RFC 5322 defines a field name: the characters 33 to 126 except 58.
fn check_new_header_name(name: &str) -> std::result::Result<(), ReplyError> {
    if name.is_empty() {
        return Err(ReplyError::InvalidHeaderName);
    }
    for byte in name.bytes() {
        if !(33..=126).contains(&byte) || byte == b':' {
            return Err(ReplyError::InvalidHeaderName);
        }
    }
    Ok(())
}

/// Checks the name of a header the MTA delivered. Such a name can hold
/// non-ASCII bytes, so only an empty name, a colon, whitespace, and control
/// characters are refused.
fn check_delivered_header_name(name: &str) -> std::result::Result<(), ReplyError> {
    if name.is_empty() {
        return Err(ReplyError::InvalidHeaderName);
    }
    for c in name.chars() {
        if c == ':' || c.is_whitespace() || c.is_control() {
            return Err(ReplyError::InvalidHeaderName);
        }
    }
    Ok(())
}

/// Allows LF only where it folds the value: each continuation line starts
/// with a space or tab and holds more than whitespace. CR is never allowed.
/// Anything else would start another header, or end the header block for
/// parsers that read a blank line there.
fn check_header_value(value: &str) -> std::result::Result<(), ReplyError> {
    check_text(value)?;
    if value.contains('\r') {
        return Err(ReplyError::HeaderLineBreak);
    }
    for line in value.split('\n').skip(1) {
        if !line.starts_with([' ', '\t']) || line.trim().is_empty() {
            return Err(ReplyError::HeaderLineBreak);
        }
    }
    Ok(())
}

/// Applies Postfix's rules (`milter8_valid_smtp_reply`) to each line of a
/// reply: a 4xx or 5xx code, `-` after the code on a line that is not the last
/// and a space on the last, and no digit after that other than the code's
/// first digit. Postfix snapshots also require the same code on every line.
fn check_reply_lines(text: &str) -> std::result::Result<(), ReplyError> {
    let mut lines: Vec<&[u8]> = Vec::new();
    for line in text.split("\r\n") {
        lines.push(line.as_bytes());
    }
    let code = lines[0].get(..3);
    for (index, line) in lines.iter().enumerate() {
        let separator = if index + 1 == lines.len() { b' ' } else { b'-' };
        let well_formed = line.len() >= 4
            && matches!(line[0], b'4' | b'5')
            && line[1].is_ascii_digit()
            && line[2].is_ascii_digit()
            && line.get(..3) == code
            && line[3] == separator
            && !line.contains(&b'\r')
            && !line.contains(&b'\n');
        if !well_formed {
            return Err(ReplyError::MalformedReplyText);
        }
        if let Some(next) = line.get(4) {
            if next.is_ascii_digit() && *next != line[0] {
                return Err(ReplyError::EnhancedCodeMismatch);
            }
        }
    }
    Ok(())
}

/// Requires every `%` to be part of a `%%` pair, as Sendmail does.
fn check_percent(text: &str) -> std::result::Result<(), ReplyError> {
    let mut bytes = text.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' && bytes.next() != Some(b'%') {
            return Err(ReplyError::UnescapedPercent);
        }
    }
    Ok(())
}

/// Appends `text` and its terminating NUL.
fn put_str(data: &mut BytesMut, text: &str) {
    data.put_slice(text.as_bytes());
    data.put_u8(0);
}
