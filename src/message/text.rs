//! Text from the MTA, kept as the bytes it sent.

use std::borrow::Cow;
use std::fmt;

use bytes::{Buf, Bytes};

/// Text exactly as the MTA sent it.
///
/// Envelope addresses, ESMTP arguments, HELO names, headers, and macro values
/// can hold bytes that are not valid UTF-8, so the library never converts
/// them. A `MilterText` shares the frame's buffer; creating one copies
/// nothing.
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
}

impl fmt::Debug for MilterText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.to_string_lossy(), f)
    }
}
