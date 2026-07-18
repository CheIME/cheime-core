#![forbid(unsafe_code)]

use cheime_model::{
    CORE_PROTOCOL_VERSION, CandidateSnapshot, ClientInstanceId, DeploymentGeneration, KeyEvent,
    PlatformAction, PlatformActionResult, Revision, Sequence, SessionEpoch, SessionId, UiCommand,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MessageHeader {
    pub protocol_version: u16,
    pub client: ClientInstanceId,
    pub session: SessionId,
    pub epoch: SessionEpoch,
    pub sequence: Sequence,
    pub revision: Revision,
    pub deployment: DeploymentGeneration,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum FrontendMessage {
    OpenSession {
        header: MessageHeader,
    },
    CloseSession {
        header: MessageHeader,
    },
    KeyCommand {
        header: MessageHeader,
        event: KeyEvent,
    },
    UiCommand {
        header: MessageHeader,
        command: UiCommand,
    },
    PlatformActionResult {
        header: MessageHeader,
        result: PlatformActionResult,
    },
}

impl FrontendMessage {
    #[must_use]
    pub const fn header(&self) -> &MessageHeader {
        match self {
            Self::OpenSession { header }
            | Self::CloseSession { header }
            | Self::KeyCommand { header, .. }
            | Self::UiCommand { header, .. }
            | Self::PlatformActionResult { header, .. } => header,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum EngineMessage {
    SessionOpened {
        header: MessageHeader,
    },
    CandidateSnapshot {
        header: MessageHeader,
        snapshot: CandidateSnapshot,
    },
    PlatformAction {
        header: MessageHeader,
        action: PlatformAction,
    },
    SessionClosed {
        header: MessageHeader,
    },
    ProtocolRejected {
        received: u16,
        supported: u16,
    },
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ProtocolError {
    #[error("unsupported protocol version {received}; supported version is {supported}")]
    UnsupportedVersion { received: u16, supported: u16 },
}

pub fn validate_protocol_version(received: u16) -> Result<(), ProtocolError> {
    if received == CORE_PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(ProtocolError::UnsupportedVersion {
            received,
            supported: CORE_PROTOCOL_VERSION,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cheime_model::{Key, KeyEvent, KeyState};

    fn header() -> MessageHeader {
        MessageHeader {
            protocol_version: CORE_PROTOCOL_VERSION,
            client: ClientInstanceId::new(1),
            session: SessionId::new(2),
            epoch: SessionEpoch::new(3),
            sequence: Sequence::new(4),
            revision: Revision::new(5),
            deployment: DeploymentGeneration::new(6),
        }
    }

    #[test]
    fn current_protocol_version_is_accepted() {
        assert_eq!(validate_protocol_version(CORE_PROTOCOL_VERSION), Ok(()));
    }

    #[test]
    fn other_protocol_version_is_rejected() {
        assert_eq!(
            validate_protocol_version(CORE_PROTOCOL_VERSION + 1),
            Err(ProtocolError::UnsupportedVersion {
                received: CORE_PROTOCOL_VERSION + 1,
                supported: CORE_PROTOCOL_VERSION,
            })
        );
    }

    #[test]
    fn key_message_keeps_complete_session_identity() {
        let message = FrontendMessage::KeyCommand {
            header: header(),
            event: KeyEvent {
                key: Key::Character('n'),
                state: KeyState::default(),
            },
        };
        assert_eq!(message.header(), &header());
    }
}
