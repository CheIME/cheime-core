use crate::error::WireError;
use cheime_protocol::{EngineMessage, FrontendMessage};
use serde::Serialize;
use serde::de::DeserializeOwned;

#[derive(Clone, Debug)]
pub struct MessageCodec {
    max_size: usize,
}

impl MessageCodec {
    pub const DEFAULT_MAX: usize = 65536;

    pub fn new(max_message_size: usize) -> Self {
        Self {
            max_size: max_message_size,
        }
    }

    #[must_use]
    pub fn max_size(&self) -> usize {
        self.max_size
    }

    pub fn encode_frontend(&self, msg: &FrontendMessage) -> Result<Vec<u8>, WireError> {
        let data = rmp_serde::to_vec(msg).map_err(|e| WireError::Encode(e.to_string()))?;
        if data.len() > self.max_size {
            return Err(WireError::SizeExceeded {
                actual: data.len(),
                max: self.max_size,
            });
        }
        Ok(data)
    }

    pub fn decode_frontend(&self, data: &[u8]) -> Result<FrontendMessage, WireError> {
        let msg: FrontendMessage =
            rmp_serde::from_slice(data).map_err(|e| WireError::Decode(e.to_string()))?;
        Ok(msg)
    }

    pub fn encode_engine(&self, msg: &EngineMessage) -> Result<Vec<u8>, WireError> {
        let data = rmp_serde::to_vec(msg).map_err(|e| WireError::Encode(e.to_string()))?;
        if data.len() > self.max_size {
            return Err(WireError::SizeExceeded {
                actual: data.len(),
                max: self.max_size,
            });
        }
        Ok(data)
    }

    pub fn decode_engine(&self, data: &[u8]) -> Result<EngineMessage, WireError> {
        let msg: EngineMessage =
            rmp_serde::from_slice(data).map_err(|e| WireError::Decode(e.to_string()))?;
        Ok(msg)
    }

    /// Generic encode for handshake messages.
    pub fn encode_handshake<T: Serialize>(&self, msg: &T) -> Result<Vec<u8>, WireError> {
        let data = rmp_serde::to_vec(msg).map_err(|e| WireError::Encode(e.to_string()))?;
        if data.len() > self.max_size {
            return Err(WireError::SizeExceeded {
                actual: data.len(),
                max: self.max_size,
            });
        }
        Ok(data)
    }

    /// Generic decode for handshake messages.
    pub fn decode_handshake<T: DeserializeOwned>(&self, data: &[u8]) -> Result<T, WireError> {
        let msg: T = rmp_serde::from_slice(data).map_err(|e| WireError::Decode(e.to_string()))?;
        Ok(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cheime_model::{
        CORE_PROTOCOL_VERSION, Candidate, CandidateId, CandidateSnapshot, DeploymentGeneration,
        Key, KeyEvent, KeyState, Revision, SessionEpoch, SessionStatus,
    };
    use cheime_protocol::MessageHeader;

    fn codec() -> MessageCodec {
        MessageCodec::new(MessageCodec::DEFAULT_MAX)
    }

    fn quick_header() -> MessageHeader {
        MessageHeader {
            protocol_version: CORE_PROTOCOL_VERSION,
            client: cheime_model::ClientInstanceId::new(1),
            session: cheime_model::SessionId::new(2),
            epoch: SessionEpoch::new(3),
            sequence: cheime_model::Sequence::new(4),
            revision: Revision::new(5),
            deployment: DeploymentGeneration::new(6),
        }
    }

    #[test]
    fn roundtrip_frontend_key_command() {
        let msg = FrontendMessage::KeyCommand {
            header: quick_header(),
            event: KeyEvent {
                key: Key::Character('n'),
                state: KeyState::default(),
            },
        };
        let c = codec();
        let encoded = c.encode_frontend(&msg).unwrap();
        let decoded = c.decode_frontend(&encoded).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_engine_snapshot() {
        let snapshot = CandidateSnapshot {
            epoch: SessionEpoch::new(3),
            revision: Revision::new(5),
            deployment: DeploymentGeneration::new(6),
            preedit: String::from("ni"),
            cursor: 2,
            candidates: vec![Candidate {
                id: CandidateId::new(1),
                text: String::from("你"),
                annotation: Some(String::from("ni")),
                source: String::from("builtin"),
            }],
            highlighted: Some(CandidateId::new(1)),
            status: SessionStatus::Composing,
            page_size: 9,
            page: 0,
        };
        let msg = EngineMessage::CandidateSnapshot {
            header: quick_header(),
            snapshot,
        };
        let c = codec();
        let encoded = c.encode_engine(&msg).unwrap();
        let decoded = c.decode_engine(&encoded).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn size_exceeded_on_oversized_encoding() {
        // Use a very small max to force the error
        let c = MessageCodec::new(4); // any real message will be larger
        let msg = FrontendMessage::KeyCommand {
            header: quick_header(),
            event: KeyEvent {
                key: Key::Character('x'),
                state: KeyState::default(),
            },
        };
        let result = c.encode_frontend(&msg);
        assert!(matches!(result, Err(WireError::SizeExceeded { .. })));
    }

    #[test]
    fn decode_garbage_returns_error() {
        let c = codec();
        let result = c.decode_frontend(&[0xFF, 0xFF, 0xFF]);
        assert!(matches!(result, Err(WireError::Decode(_))));
    }
}
