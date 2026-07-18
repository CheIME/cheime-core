use crate::codec::MessageCodec;
use crate::error::WireError;
use serde::Serialize;

/// Writes a length-delimited frame into a byte buffer.
///
/// Frame format: 4-byte big-endian u32 length prefix followed by the
/// MessagePack-encoded payload bytes.
pub struct FramedWriter;

impl FramedWriter {
    /// Encode `msg` via the `codec`, then write the length-prefixed frame into `buf`.
    ///
    /// Returns the total number of bytes written into `buf` (4 header bytes + payload).
    ///
    /// # Errors
    ///
    /// Returns [`WireError::Encode`] if serialization fails, or
    /// [`WireError::SizeExceeded`] if the encoded payload exceeds `codec.max_size`.
    pub fn write_frame<M: Serialize>(
        buf: &mut [u8],
        codec: &MessageCodec,
        msg: &M,
    ) -> Result<usize, WireError> {
        // Encode via the generic path — handshake messages use encode_handshake,
        // but we accept any Serialize to avoid coupling.
        let payload = codec.encode_handshake(msg)?;
        let total = 4 + payload.len();
        if total > buf.len() {
            return Err(WireError::Encode(format!(
                "buffer too small: need {total} bytes, have {} bytes",
                buf.len()
            )));
        }

        // Write big-endian u32 length prefix
        let len_bytes = (payload.len() as u32).to_be_bytes();
        buf[..4].copy_from_slice(&len_bytes);
        buf[4..total].copy_from_slice(&payload);
        Ok(total)
    }
}

/// Reads a length-delimited frame from a byte buffer.
///
/// Purely functional — operates on byte slices and never blocks.
pub struct FramedReader;

impl FramedReader {
    /// Attempt to parse the first frame from `buf`.
    ///
    /// Returns:
    /// - `Ok(Some((payload_start, payload_len)))` when a complete frame is available.
    ///   `payload_start` is the byte offset within `buf` where the MessagePack payload begins
    ///   (always 4 for a valid frame). `payload_len` is the number of payload bytes.
    /// - `Ok(None)` when `buf` does not yet contain a complete frame (more data needed).
    /// - `Err(WireError::InvalidFrameLength)` if the length prefix is zero.
    /// - `Err(WireError::SizeExceeded)` if the declared frame length exceeds `max_size`.
    /// - `Err(WireError::IncompleteFrame)` if we can parse the header but the buffer
    ///   is too short for the declared payload (shouldn't happen — caller should pass more data).
    ///
    /// The caller is responsible for buffering: if `Ok(None)` is returned, read more data
    /// from the pipe, append it to the buffer, and call again.
    pub fn read_frame(buf: &[u8], max_size: usize) -> Result<Option<(usize, usize)>, WireError> {
        // Need at least 4 bytes for the length prefix
        if buf.len() < 4 {
            return Ok(None);
        }

        let len_bytes: [u8; 4] = buf[..4].try_into().unwrap();
        let length = u32::from_be_bytes(len_bytes) as usize;

        if length == 0 {
            return Err(WireError::InvalidFrameLength);
        }

        if length > max_size {
            return Err(WireError::SizeExceeded {
                actual: length,
                max: max_size,
            });
        }

        if buf.len() < 4 + length {
            return Ok(None);
        }

        Ok(Some((4, length)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::MessageCodec;
    use cheime_model::{
        CORE_PROTOCOL_VERSION, ClientInstanceId, DeploymentGeneration, Revision, Sequence,
        SessionEpoch, SessionId,
    };
    use cheime_model::{Key, KeyEvent, KeyState};
    use cheime_protocol::{FrontendMessage, MessageHeader};

    fn codec() -> MessageCodec {
        MessageCodec::new(MessageCodec::DEFAULT_MAX)
    }

    fn key_message(c: char) -> FrontendMessage {
        FrontendMessage::KeyCommand {
            header: MessageHeader {
                protocol_version: CORE_PROTOCOL_VERSION,
                client: ClientInstanceId::new(1),
                session: SessionId::new(2),
                epoch: SessionEpoch::new(3),
                sequence: Sequence::new(4),
                revision: Revision::new(5),
                deployment: DeploymentGeneration::new(6),
            },
            event: KeyEvent {
                key: Key::Character(c),
                state: KeyState::default(),
            },
        }
    }

    #[test]
    fn write_then_read_frame_roundtrip() {
        let msg = key_message('n');
        let c = codec();

        // Use a generous buffer
        let mut buf = vec![0u8; 1024];
        let written = FramedWriter::write_frame(&mut buf, &c, &msg).unwrap();
        assert!(written >= 5); // 4-byte prefix + at least 1 byte payload

        // Read the frame back
        let (payload_start, payload_len) = FramedReader::read_frame(&buf[..written], c.max_size())
            .unwrap()
            .expect("should parse complete frame");
        assert_eq!(payload_start, 4);
        assert_eq!(payload_len, written - 4);

        // Decode and compare
        let decoded: FrontendMessage = c
            .decode_handshake(&buf[payload_start..payload_start + payload_len])
            .unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn empty_buffer_returns_none() {
        let result = FramedReader::read_frame(&[], MessageCodec::DEFAULT_MAX).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn partial_header_returns_none() {
        // Only 2 bytes — can't parse a u32
        let result = FramedReader::read_frame(&[0x00, 0x01], MessageCodec::DEFAULT_MAX).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn full_header_but_partial_payload_returns_none() {
        // Declare 10-byte payload but only provide 2 bytes after header
        let mut buf = vec![0u8; 6]; // 4 header + 2 payload (but header says 10)
        let len_bytes = 10u32.to_be_bytes();
        buf[..4].copy_from_slice(&len_bytes);

        let result = FramedReader::read_frame(&buf, MessageCodec::DEFAULT_MAX).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn zero_length_frame_returns_error() {
        // Declare 0-byte payload
        let buf = [0u8; 4]; // all zeros → length_prefix = 0
        let result = FramedReader::read_frame(&buf, MessageCodec::DEFAULT_MAX);
        assert!(matches!(result, Err(WireError::InvalidFrameLength)));
    }

    #[test]
    fn length_exceeding_max_returns_size_exceeded() {
        let max = 1024;
        // Declare a payload larger than max
        let oversized = (max + 1) as u32;
        let mut buf = vec![0u8; 4 + max + 1];
        let len_bytes = oversized.to_be_bytes();
        buf[..4].copy_from_slice(&len_bytes);

        let result = FramedReader::read_frame(&buf, max);
        assert!(matches!(result, Err(WireError::SizeExceeded { .. })));
    }

    #[test]
    fn end_to_end_frontend_message_via_frame() {
        let msg = key_message('i');
        let c = codec();

        let mut buf = vec![0u8; 4096];
        let written = FramedWriter::write_frame(&mut buf, &c, &msg).unwrap();

        let (off, len) = FramedReader::read_frame(&buf[..written], c.max_size())
            .unwrap()
            .unwrap();

        let decoded: FrontendMessage = c.decode_handshake(&buf[off..off + len]).unwrap();
        assert_eq!(msg, decoded);
    }
}
