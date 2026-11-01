//! Reading frames from a receive buffer.

use bytes::{Buf, BytesMut};

use super::Frame;
use crate::{Error, ProtocolError, Result};

/// Size of the length field that starts every frame.
const LENGTH_FIELD: usize = 4;

impl Frame {
    /// Checks whether `buf` starts with a complete frame.
    ///
    /// Returns the length of the whole packet, length field included, or
    /// `Ok(None)` while the frame is incomplete. Fails with
    /// [`Error::FrameTooLarge`] as soon as the length field announces a packet
    /// longer than `max`, before the data arrives.
    pub fn check(buf: &[u8], max: usize) -> Result<Option<usize>> {
        let Some(length_field) = buf.first_chunk::<LENGTH_FIELD>() else {
            return Ok(None);
        };
        let length = u32::from_be_bytes(*length_field) as usize;
        if length == 0 {
            return Err(ProtocolError::ZeroLength.into());
        }
        // Saturates instead of wrapping where usize is 32 bits wide.
        let packet = LENGTH_FIELD.saturating_add(length);
        if packet > max {
            return Err(Error::FrameTooLarge {
                length: packet,
                max,
            });
        }
        if buf.len() < packet {
            return Ok(None);
        }
        Ok(Some(packet))
    }

    /// Removes one frame from the front of `buf` and returns it.
    ///
    /// Returns `Ok(None)` and leaves `buf` unchanged while the frame is
    /// incomplete.
    pub fn parse(buf: &mut BytesMut, max: usize) -> Result<Option<Frame>> {
        let Some(packet) = Frame::check(buf, max)? else {
            return Ok(None);
        };
        let mut packet = buf.split_to(packet);
        packet.advance(LENGTH_FIELD);
        let command = packet.get_u8();
        Ok(Some(Frame {
            command,
            data: packet.freeze(),
        }))
    }
}
