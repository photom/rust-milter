//! Serializing frames for the send buffer.

use bytes::{BufMut, BytesMut};

use super::Frame;

impl Frame {
    /// Appends the frame to `out` in wire format.
    ///
    /// # Panics
    ///
    /// Panics if the data is 4 GiB or longer, because the 4-byte length field
    /// cannot hold it. The library never builds such a frame: the largest
    /// packet it accepts is 16 MB.
    pub fn write_to(&self, out: &mut BytesMut) {
        // The length field counts the command byte as well as the data.
        let length =
            u32::try_from(self.data.len() + 1).expect("frame data must be shorter than 4 GiB");
        out.reserve(4 + 1 + self.data.len());
        out.put_u32(length);
        out.put_u8(self.command);
        out.put_slice(&self.data);
    }
}
