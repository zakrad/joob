use thiserror::Error;

/// Maximum payload size (u16::MAX = 65535 bytes).
pub const MAX_PAYLOAD_SIZE: usize = u16::MAX as usize;

/// Header size: 4 (stream_id) + 1 (type) + 2 (length) = 7 bytes.
pub const HEADER_SIZE: usize = 7;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FrameError {
    #[error("buffer too short to decode frame")]
    BufferTooShort,
    #[error("invalid frame type: {0}")]
    InvalidFrameType(u8),
    #[error("payload too large (max 65535 bytes)")]
    PayloadTooLarge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameType {
    Open = 1,
    Data = 2,
    Fin = 3,
    Rst = 4,
    Ping = 5,
    Pong = 6,
}

impl FrameType {
    pub fn from_u8(v: u8) -> Option<FrameType> {
        match v {
            1 => Some(FrameType::Open),
            2 => Some(FrameType::Data),
            3 => Some(FrameType::Fin),
            4 => Some(FrameType::Rst),
            5 => Some(FrameType::Ping),
            6 => Some(FrameType::Pong),
            _ => None,
        }
    }

    pub fn as_u8(&self) -> u8 {
        *self as u8
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub stream_id: u32,
    pub frame_type: FrameType,
    pub payload: Vec<u8>,
}

impl Frame {
    /// Returns the header size (7 bytes).
    pub fn header_size() -> usize {
        HEADER_SIZE
    }

    /// Encode this frame into wire format:
    /// [stream_id: 4 bytes BE][type: 1 byte][length: 2 bytes BE][payload: N bytes]
    pub fn encode(&self) -> Vec<u8> {
        let len = self.payload.len() as u16;
        let mut buf = Vec::with_capacity(HEADER_SIZE + self.payload.len());
        buf.extend_from_slice(&self.stream_id.to_be_bytes());
        buf.push(self.frame_type.as_u8());
        buf.extend_from_slice(&len.to_be_bytes());
        buf.extend_from_slice(&self.payload);
        buf
    }

    /// Decode a frame from the buffer. Returns the frame and the number of bytes consumed.
    pub fn decode(buf: &[u8]) -> Result<(Frame, usize), FrameError> {
        if buf.len() < HEADER_SIZE {
            return Err(FrameError::BufferTooShort);
        }

        let stream_id = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let type_byte = buf[4];
        let length = u16::from_be_bytes([buf[5], buf[6]]) as usize;

        let frame_type = FrameType::from_u8(type_byte)
            .ok_or(FrameError::InvalidFrameType(type_byte))?;

        let total = HEADER_SIZE + length;
        if buf.len() < total {
            return Err(FrameError::BufferTooShort);
        }

        let payload = buf[HEADER_SIZE..total].to_vec();

        Ok((
            Frame {
                stream_id,
                frame_type,
                payload,
            },
            total,
        ))
    }
}

/// Utility for encoding/decoding multiple frames as a contiguous batch.
pub struct FrameBatch;

impl FrameBatch {
    /// Encode multiple frames into a single byte buffer by concatenating their wire representations.
    pub fn encode(frames: &[Frame]) -> Vec<u8> {
        let mut buf = Vec::new();
        for frame in frames {
            buf.extend_from_slice(&frame.encode());
        }
        buf
    }

    /// Decode all frames from a contiguous buffer.
    pub fn decode(buf: &[u8]) -> Result<Vec<Frame>, FrameError> {
        let mut frames = Vec::new();
        let mut offset = 0;
        while offset < buf.len() {
            let (frame, consumed) = Frame::decode(&buf[offset..])?;
            frames.push(frame);
            offset += consumed;
        }
        Ok(frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_type_from_u8() {
        assert_eq!(FrameType::from_u8(1), Some(FrameType::Open));
        assert_eq!(FrameType::from_u8(2), Some(FrameType::Data));
        assert_eq!(FrameType::from_u8(3), Some(FrameType::Fin));
        assert_eq!(FrameType::from_u8(4), Some(FrameType::Rst));
        assert_eq!(FrameType::from_u8(5), Some(FrameType::Ping));
        assert_eq!(FrameType::from_u8(6), Some(FrameType::Pong));
        assert_eq!(FrameType::from_u8(0), None);
        assert_eq!(FrameType::from_u8(7), None);
        assert_eq!(FrameType::from_u8(255), None);
    }

    #[test]
    fn test_frame_type_as_u8() {
        assert_eq!(FrameType::Open.as_u8(), 1);
        assert_eq!(FrameType::Data.as_u8(), 2);
        assert_eq!(FrameType::Fin.as_u8(), 3);
        assert_eq!(FrameType::Rst.as_u8(), 4);
        assert_eq!(FrameType::Ping.as_u8(), 5);
        assert_eq!(FrameType::Pong.as_u8(), 6);
    }

    #[test]
    fn test_header_size() {
        assert_eq!(Frame::header_size(), 7);
    }

    #[test]
    fn test_encode_decode_roundtrip_open() {
        let frame = Frame {
            stream_id: 42,
            frame_type: FrameType::Open,
            payload: vec![1, 2, 3, 4],
        };
        let encoded = frame.encode();
        let (decoded, consumed) = Frame::decode(&encoded).unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(decoded, frame);
    }

    #[test]
    fn test_encode_decode_roundtrip_data() {
        let frame = Frame {
            stream_id: 0xDEADBEEF,
            frame_type: FrameType::Data,
            payload: b"hello world".to_vec(),
        };
        let encoded = frame.encode();
        let (decoded, consumed) = Frame::decode(&encoded).unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(decoded, frame);
    }

    #[test]
    fn test_encode_decode_roundtrip_fin() {
        let frame = Frame {
            stream_id: 1,
            frame_type: FrameType::Fin,
            payload: vec![],
        };
        let encoded = frame.encode();
        let (decoded, consumed) = Frame::decode(&encoded).unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(decoded, frame);
    }

    #[test]
    fn test_encode_decode_roundtrip_rst() {
        let frame = Frame {
            stream_id: 999,
            frame_type: FrameType::Rst,
            payload: vec![0xFF],
        };
        let encoded = frame.encode();
        let (decoded, consumed) = Frame::decode(&encoded).unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(decoded, frame);
    }

    #[test]
    fn test_encode_decode_roundtrip_ping() {
        let frame = Frame {
            stream_id: 0,
            frame_type: FrameType::Ping,
            payload: vec![0xAA, 0xBB],
        };
        let encoded = frame.encode();
        let (decoded, consumed) = Frame::decode(&encoded).unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(decoded, frame);
    }

    #[test]
    fn test_encode_decode_roundtrip_pong() {
        let frame = Frame {
            stream_id: 0,
            frame_type: FrameType::Pong,
            payload: vec![0xCC, 0xDD],
        };
        let encoded = frame.encode();
        let (decoded, consumed) = Frame::decode(&encoded).unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(decoded, frame);
    }

    #[test]
    fn test_empty_payload_frame() {
        let frame = Frame {
            stream_id: 100,
            frame_type: FrameType::Data,
            payload: vec![],
        };
        let encoded = frame.encode();
        assert_eq!(encoded.len(), HEADER_SIZE);
        let (decoded, consumed) = Frame::decode(&encoded).unwrap();
        assert_eq!(consumed, HEADER_SIZE);
        assert_eq!(decoded, frame);
    }

    #[test]
    fn test_max_size_payload_frame() {
        let payload = vec![0x42; MAX_PAYLOAD_SIZE];
        let frame = Frame {
            stream_id: 7,
            frame_type: FrameType::Data,
            payload: payload.clone(),
        };
        let encoded = frame.encode();
        assert_eq!(encoded.len(), HEADER_SIZE + MAX_PAYLOAD_SIZE);
        let (decoded, consumed) = Frame::decode(&encoded).unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(decoded.payload.len(), MAX_PAYLOAD_SIZE);
        assert_eq!(decoded, frame);
    }

    #[test]
    fn test_buffer_too_short_header() {
        let buf = [0u8; 5]; // less than HEADER_SIZE
        let result = Frame::decode(&buf);
        assert_eq!(result, Err(FrameError::BufferTooShort));
    }

    #[test]
    fn test_buffer_too_short_payload() {
        // Valid header claiming 10 bytes of payload, but only 3 available
        let frame = Frame {
            stream_id: 1,
            frame_type: FrameType::Data,
            payload: vec![0; 10],
        };
        let encoded = frame.encode();
        // Truncate to header + 3 bytes
        let truncated = &encoded[..HEADER_SIZE + 3];
        let result = Frame::decode(truncated);
        assert_eq!(result, Err(FrameError::BufferTooShort));
    }

    #[test]
    fn test_invalid_frame_type() {
        let mut buf = [0u8; HEADER_SIZE];
        // stream_id = 1
        buf[0..4].copy_from_slice(&1u32.to_be_bytes());
        // invalid type = 99
        buf[4] = 99;
        // length = 0
        buf[5..7].copy_from_slice(&0u16.to_be_bytes());

        let result = Frame::decode(&buf);
        assert_eq!(result, Err(FrameError::InvalidFrameType(99)));
    }

    #[test]
    fn test_batch_encode_decode() {
        let frames = vec![
            Frame {
                stream_id: 1,
                frame_type: FrameType::Open,
                payload: vec![],
            },
            Frame {
                stream_id: 1,
                frame_type: FrameType::Data,
                payload: b"hello".to_vec(),
            },
            Frame {
                stream_id: 2,
                frame_type: FrameType::Data,
                payload: b"world".to_vec(),
            },
            Frame {
                stream_id: 1,
                frame_type: FrameType::Fin,
                payload: vec![],
            },
        ];

        let encoded = FrameBatch::encode(&frames);
        let decoded = FrameBatch::decode(&encoded).unwrap();
        assert_eq!(decoded.len(), frames.len());
        for (a, b) in decoded.iter().zip(frames.iter()) {
            assert_eq!(a, b);
        }
    }

    #[test]
    fn test_batch_decode_empty() {
        let decoded = FrameBatch::decode(&[]).unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn test_batch_decode_partial_buffer() {
        let frame = Frame {
            stream_id: 1,
            frame_type: FrameType::Data,
            payload: b"test".to_vec(),
        };
        let mut encoded = FrameBatch::encode(&[frame.clone(), frame.clone()]);
        // Truncate the second frame in the middle
        encoded.truncate(encoded.len() - 2);
        let result = FrameBatch::decode(&encoded);
        assert_eq!(result, Err(FrameError::BufferTooShort));
    }

    #[test]
    fn test_wire_format_correctness() {
        let frame = Frame {
            stream_id: 0x00000001,
            frame_type: FrameType::Data,
            payload: vec![0xAA, 0xBB],
        };
        let encoded = frame.encode();
        // stream_id: 00 00 00 01
        assert_eq!(&encoded[0..4], &[0x00, 0x00, 0x00, 0x01]);
        // type: 02 (Data)
        assert_eq!(encoded[4], 0x02);
        // length: 00 02
        assert_eq!(&encoded[5..7], &[0x00, 0x02]);
        // payload: AA BB
        assert_eq!(&encoded[7..9], &[0xAA, 0xBB]);
    }
}
