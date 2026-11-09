//! Text from the MTA, kept as the bytes it sent.

use std::borrow::Cow;
use std::fmt;

use bytes::{Buf, Bytes, BytesMut};

/// Text exactly as the MTA sent it.
///
/// Envelope addresses, ESMTP arguments, HELO names, headers, and macro values
/// can hold bytes that are not valid UTF-8, so the library never converts
/// them. A `MilterText` shares the frame's buffer; creating one copies
/// nothing. The macros the library stores are copies, so they do not keep
/// the frame's buffer.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct MilterText(Bytes);

impl MilterText {
    /// The raw bytes, without the terminating NUL.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// The text, when it is valid UTF-8.
    pub fn as_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.0).ok()
    }

    /// The text with invalid bytes replaced by U+FFFD, for logging and
    /// display.
    pub fn to_string_lossy(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.0)
    }

    /// Splits one NUL-terminated string off the front of `data`.
    ///
    /// Returns `None`, and leaves `data` unchanged, when no NUL follows.
    pub(crate) fn take(data: &mut Bytes) -> Option<MilterText> {
        let mut end = None;
        for (index, byte) in data.iter().enumerate() {
            if *byte == 0 {
                end = Some(index);
                break;
            }
        }
        let end = end?;
        let text = data.split_to(end);
        data.advance(1);
        Some(MilterText(text))
    }

    /// Copies name and value pairs into one buffer of their exact size, so
    /// that keeping the copies does not keep the frame's buffer.
    pub(crate) fn copy_pairs(pairs: &[(MilterText, MilterText)]) -> Vec<(MilterText, MilterText)> {
        let mut length = 0;
        for (name, value) in pairs {
            length += name.0.len() + value.0.len();
        }
        let mut buffer = BytesMut::with_capacity(length);
        for (name, value) in pairs {
            buffer.extend_from_slice(&name.0);
            buffer.extend_from_slice(&value.0);
        }
        let mut text = buffer.freeze();
        let mut copies = Vec::with_capacity(pairs.len());
        for (name, value) in pairs {
            let name = MilterText(text.split_to(name.0.len()));
            let value = MilterText(text.split_to(value.0.len()));
            copies.push((name, value));
        }
        copies
    }
}

impl fmt::Debug for MilterText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.to_string_lossy(), f)
    }
}
