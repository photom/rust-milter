//! Scenarios from `docs/testing/frame.md`.

use proptest::prelude::*;
use rust_milter::{Bytes, BytesMut, Error, Frame, ProtocolError};

/// The default `per_connection_buffer`, 128 KB.
const DEFAULT_MAX: usize = 131_072;

/// Encodes one frame by hand, so the parser tests do not depend on the writer.
fn raw_frame(command: u8, data: &[u8]) -> Vec<u8> {
    let length = u32::try_from(data.len() + 1).expect("test data fits in a u32 length");
    let mut out = length.to_be_bytes().to_vec();
    out.push(command);
    out.extend_from_slice(data);
    out
}

#[test]
fn frm_001_parses_one_complete_frame() {
    let mut buf = BytesMut::from(&raw_frame(b'H', b"mail\0")[..]);

    let frame = Frame::parse(&mut buf, DEFAULT_MAX)
        .expect("a valid frame parses")
        .expect("the frame is complete");

    assert_eq!(frame.command, b'H');
    assert_eq!(&frame.data[..], b"mail\0");
    assert!(buf.is_empty());
}

#[test]
fn frm_002_parses_two_frames_back_to_back() {
    let mut bytes = raw_frame(b'H', b"mail\0");
    bytes.extend(raw_frame(b'R', b"<rcpt@void.invalid>\0"));
    let mut buf = BytesMut::from(&bytes[..]);

    let first = Frame::parse(&mut buf, DEFAULT_MAX).unwrap().unwrap();
    let second = Frame::parse(&mut buf, DEFAULT_MAX).unwrap().unwrap();

    assert_eq!((first.command, &first.data[..]), (b'H', &b"mail\0"[..]));
    assert_eq!(
        (second.command, &second.data[..]),
        (b'R', &b"<rcpt@void.invalid>\0"[..])
    );
    assert!(buf.is_empty());
}

#[test]
fn frm_003_waits_for_a_complete_length_field() {
    let mut buf = BytesMut::from(&[0u8, 0, 0][..]);

    let result = Frame::parse(&mut buf, DEFAULT_MAX).expect("no error is raised");

    assert_eq!(result, None);
    assert_eq!(&buf[..], &[0, 0, 0]);
}

#[test]
fn frm_004_waits_for_the_rest_of_the_data() {
    let whole = raw_frame(b'H', b"mail\0");
    let partial = &whole[..whole.len() - 2];
    let mut buf = BytesMut::from(partial);

    let result = Frame::parse(&mut buf, DEFAULT_MAX).expect("no error is raised");

    assert_eq!(result, None);
    assert_eq!(&buf[..], partial);
}

#[test]
fn frm_005_returns_a_frame_only_when_the_last_byte_arrives() {
    let whole = raw_frame(b'H', b"mail\0");
    let mut buf = BytesMut::new();
    let mut frames = Vec::new();

    for (index, byte) in whole.iter().enumerate() {
        buf.extend_from_slice(&[*byte]);
        let result = Frame::parse(&mut buf, DEFAULT_MAX).expect("no error is raised");
        if index + 1 < whole.len() {
            assert_eq!(result, None, "a frame came out after {} bytes", index + 1);
        }
        frames.extend(result);
    }

    assert_eq!(
        frames,
        vec![Frame::new(b'H', Bytes::from_static(b"mail\0"))]
    );
}

#[test]
fn frm_006_parses_a_frame_with_no_data() {
    let mut buf = BytesMut::from(&raw_frame(b'C', b"")[..]);

    let frame = Frame::parse(&mut buf, DEFAULT_MAX).unwrap().unwrap();

    assert_eq!(frame.command, b'C');
    assert!(frame.data.is_empty());
}

#[test]
fn frm_007_rejects_a_zero_length() {
    let mut buf = BytesMut::from(&[0u8, 0, 0, 0][..]);

    let result = Frame::parse(&mut buf, DEFAULT_MAX);

    assert!(
        matches!(result, Err(Error::Protocol(ProtocolError::ZeroLength))),
        "got {result:?}"
    );
}

/// A frame with `data_len` bytes of data, all `x`.
fn raw_frame_of_size(command: u8, data_len: usize) -> Vec<u8> {
    raw_frame(command, &vec![b'x'; data_len])
}

#[test]
fn frm_020_rejects_an_oversized_length_before_the_data_arrives() {
    let length_field = 200_001u32.to_be_bytes();

    let result = Frame::check(&length_field, DEFAULT_MAX);

    assert!(
        matches!(
            result,
            Err(Error::FrameTooLarge {
                length: 200_005,
                max: DEFAULT_MAX
            })
        ),
        "got {result:?}"
    );
}

#[test]
fn frm_021_accepts_a_packet_of_exactly_the_maximum() {
    let mut buf = BytesMut::from(&raw_frame_of_size(b'B', 65_535)[..]);

    let frame = Frame::parse(&mut buf, 65_540).unwrap().unwrap();

    assert_eq!(frame.data.len(), 65_535);
}

#[test]
fn frm_022_rejects_a_packet_one_byte_over_the_maximum() {
    let bytes = raw_frame_of_size(b'B', 65_536);

    let result = Frame::check(&bytes, 65_540);

    assert!(
        matches!(
            result,
            Err(Error::FrameTooLarge {
                length: 65_541,
                max: 65_540
            })
        ),
        "got {result:?}"
    );
}

#[test]
fn frm_023_accepts_the_largest_body_chunk_at_the_default_maximum() {
    let mut buf = BytesMut::from(&raw_frame_of_size(b'B', 65_535)[..]);

    let frame = Frame::parse(&mut buf, DEFAULT_MAX).unwrap().unwrap();

    assert_eq!(frame.data.len(), 65_535);
}

#[test]
fn frm_024_accepts_a_100_kb_header_frame_at_the_default_maximum() {
    let mut buf = BytesMut::from(&raw_frame_of_size(b'L', 100_000)[..]);

    let frame = Frame::parse(&mut buf, DEFAULT_MAX).unwrap().unwrap();

    assert_eq!(frame.data.len(), 100_000);
}

#[test]
fn frm_040_writes_a_frame_with_no_data() {
    let mut out = BytesMut::new();

    Frame::new(b'c', Bytes::new()).write_to(&mut out);

    assert_eq!(&out[..], &[0, 0, 0, 1, b'c']);
}

#[test]
fn frm_041_writes_the_length_in_network_byte_order() {
    let mut out = BytesMut::new();

    Frame::new(b'h', Bytes::from_static(b"0123456789")).write_to(&mut out);

    assert_eq!(&out[..4], &11u32.to_be_bytes());
    assert_eq!(out[4], b'h');
    assert_eq!(&out[5..], b"0123456789");
}

proptest! {
    #[test]
    fn frm_042_a_written_frame_parses_back_unchanged(
        command in any::<u8>(),
        data in proptest::collection::vec(any::<u8>(), 0..=65_535),
    ) {
        let original = Frame::new(command, Bytes::from(data));
        let mut buf = BytesMut::new();
        original.write_to(&mut buf);

        let parsed = Frame::parse(&mut buf, DEFAULT_MAX).unwrap();

        prop_assert_eq!(parsed, Some(original));
        prop_assert!(buf.is_empty());
    }

    #[test]
    fn frm_061_frames_split_into_random_chunks_come_out_complete_and_in_order(
        frames in proptest::collection::vec(
            (any::<u8>(), proptest::collection::vec(any::<u8>(), 0..512)),
            0..16,
        ),
        chunk_sizes in proptest::collection::vec(1usize..64, 1..32),
    ) {
        let mut built = Vec::new();
        for (command, data) in frames {
            built.push(Frame::new(command, Bytes::from(data)));
        }
        let frames = built;
        let mut wire = BytesMut::new();
        for frame in &frames {
            frame.write_to(&mut wire);
        }

        let mut buf = BytesMut::new();
        let mut parsed = Vec::new();
        let mut rest = &wire[..];
        for size in chunk_sizes.iter().cycle() {
            if rest.is_empty() {
                break;
            }
            let (chunk, tail) = rest.split_at((*size).min(rest.len()));
            rest = tail;
            buf.extend_from_slice(chunk);
            while let Some(frame) = Frame::parse(&mut buf, DEFAULT_MAX).unwrap() {
                parsed.push(frame);
            }
        }

        prop_assert_eq!(parsed, frames);
        prop_assert!(buf.is_empty());
    }
}

/// Input for FRM-060: random bytes, mixed with length fields small enough
/// that the parser reads past them instead of rejecting the first frame.
/// Each piece is its random bytes, or the length field when it has one.
fn frm_060_input(pieces: &[(Vec<u8>, Option<u32>)]) -> Vec<u8> {
    let mut input = Vec::new();
    for (random, plausible_length) in pieces {
        match plausible_length {
            Some(length) => input.extend_from_slice(&length.to_be_bytes()),
            None => input.extend_from_slice(random),
        }
    }
    input
}

proptest! {
    #[test]
    fn frm_060_arbitrary_input_never_panics_or_exceeds_the_maximum(
        pieces in proptest::collection::vec(
            (proptest::collection::vec(any::<u8>(), 0..64), proptest::option::of(0u32..1_100)),
            0..32,
        ),
        max in 5usize..=1_024,
    ) {
        let input = frm_060_input(&pieces);
        let mut buf = BytesMut::from(&input[..]);

        loop {
            match Frame::parse(&mut buf, max) {
                Ok(Some(frame)) => prop_assert!(4 + 1 + frame.data.len() <= max),
                Ok(None) => {
                    prop_assert!(buf.len() < max, "{} bytes left over", buf.len());
                    break;
                }
                Err(_) => break,
            }
        }
    }
}
