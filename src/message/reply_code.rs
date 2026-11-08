//! Validated custom SMTP replies.

use super::Reply;
use crate::ReplyError;

/// The most lines a reply may have, as in libmilter's `smfi_setmlreply`.
const MAX_LINES: usize = 32;

/// The longest text a line may have, in bytes, as in libmilter.
const MAX_LINE_LENGTH: usize = 980;

/// A validated custom SMTP reply.
///
/// The constructors check the reply as libmilter's `smfi_setreply` and
/// `smfi_setmlreply` do. The text is taken literally: each `%` is written as
/// `%%` on the wire, so both MTAs show the reply as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplyCode {
    /// The reply as it goes on the wire, without the terminating NUL.
    wire: String,
}

impl ReplyCode {
    /// Builds a one-line reply, such as `550 5.7.1 Rejected by policy`.
    ///
    /// Fails if the code is not 4xx or 5xx, if the enhanced status code is not
    /// of the form `d.ddd.ddd` with the code's first digit, or if the text
    /// holds CR, LF, or NUL, or is longer than 980 bytes with each `%` written
    /// as `%%`. Without an enhanced code, the text must not start with a digit
    /// other than the code's first digit, because Postfix reads it as a
    /// mismatched enhanced code.
    pub fn new(
        code: u16,
        enhanced_code: Option<&str>,
        text: &str,
    ) -> Result<ReplyCode, ReplyError> {
        ReplyCode::multi_line(code, enhanced_code, &[text])
    }

    /// Builds a reply of 1 to 32 lines, each with the same code and enhanced
    /// status code.
    ///
    /// Each line is checked as [`ReplyCode::new`] checks its text.
    pub fn multi_line<S: AsRef<str>>(
        code: u16,
        enhanced_code: Option<&str>,
        lines: &[S],
    ) -> Result<ReplyCode, ReplyError> {
        if !(400..600).contains(&code) {
            return Err(ReplyError::InvalidCode(code));
        }
        if let Some(enhanced) = enhanced_code {
            check_enhanced_code(code, enhanced)?;
        }
        if lines.is_empty() || lines.len() > MAX_LINES {
            return Err(ReplyError::LineCount(lines.len()));
        }

        // Postfix rejects a digit after the code unless it is the code's first
        // digit, whether or not an enhanced code follows. The code is 4xx or
        // 5xx here, so its first digit fits in a byte.
        let class = b'0' + (code / 100) as u8;
        let mut wire = String::new();
        for (index, line) in lines.iter().enumerate() {
            let line = line.as_ref();
            let escaped = line.replace('%', "%%");
            check_line(&escaped)?;
            if enhanced_code.is_none() {
                if let Some(first) = line.as_bytes().first() {
                    if first.is_ascii_digit() && *first != class {
                        return Err(ReplyError::EnhancedCodeMismatch);
                    }
                }
            }
            if index > 0 {
                wire.push_str("\r\n");
            }
            let separator = if index + 1 == lines.len() { ' ' } else { '-' };
            wire.push_str(&code.to_string());
            wire.push(separator);
            if let Some(enhanced) = enhanced_code {
                wire.push_str(enhanced);
                wire.push(' ');
            }
            wire.push_str(&escaped);
        }
        Ok(ReplyCode { wire })
    }
}

impl From<ReplyCode> for Reply {
    fn from(code: ReplyCode) -> Reply {
        Reply::ReplyCode(code.wire)
    }
}

/// Whether `part` is one to `max` ASCII digits.
fn is_digits(part: &str, max: usize) -> bool {
    if !(1..=max).contains(&part.len()) {
        return false;
    }
    for byte in part.bytes() {
        if !byte.is_ascii_digit() {
            return false;
        }
    }
    true
}

/// Requires the form `d.ddd.ddd`, one class digit and one to three digits in
/// the other parts, with the same class as the reply code.
fn check_enhanced_code(code: u16, enhanced: &str) -> Result<(), ReplyError> {
    let parts: Vec<&str> = enhanced.split('.').collect();
    let well_formed = parts.len() == 3
        && is_digits(parts[0], 1)
        && is_digits(parts[1], 3)
        && is_digits(parts[2], 3);
    if !well_formed {
        return Err(ReplyError::InvalidEnhancedCode);
    }
    if parts[0] != (code / 100).to_string() {
        return Err(ReplyError::EnhancedCodeMismatch);
    }
    Ok(())
}

/// Rejects NUL, CR, and LF, and text longer than 980 bytes. It takes the
/// line with each `%` already written as `%%`, the text libmilter's limit
/// applies to; escaping changes no NUL, CR, or LF.
fn check_line(escaped: &str) -> Result<(), ReplyError> {
    if escaped.contains('\0') {
        return Err(ReplyError::NulByte);
    }
    if escaped.contains(['\r', '\n']) {
        return Err(ReplyError::LineBreak);
    }
    if escaped.len() > MAX_LINE_LENGTH {
        return Err(ReplyError::LineTooLong {
            length: escaped.len(),
        });
    }
    Ok(())
}
