use serde::{Deserialize, Serialize};

/// Sent by the engine immediately after a named-pipe connection is established.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ServerHello {
    /// Core protocol version the engine speaks.
    pub protocol_version: u16,
    /// Human-readable engine version (semver, e.g. "0.1.0").
    pub engine_version: String,
    /// Capabilities the engine advertises (MVP: empty vec).
    pub supported_caps: Vec<String>,
}

/// Sent by the TIP in response to [`ServerHello`].
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClientHello {
    /// Protocol version the TIP expects. Mismatch → engine sends [`HelloRejected`].
    pub protocol_version: u16,
    /// Unique client instance identity for this connection.
    pub client_instance_id: u64,
    /// Capabilities the TIP supports (MVP: empty vec).
    pub client_caps: Vec<String>,
}

/// Sent by the engine after a successful handshake.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HelloAck {
    /// Starting value for session IDs allocated on this connection.
    pub session_id_base: u64,
}

/// Sent by the engine when the protocol version is incompatible, then closes the pipe.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HelloRejected {
    /// Human-readable reason for rejection.
    pub reason: String,
    /// Engine version for diagnostics.
    pub engine_version: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::MessageCodec;

    fn codec() -> MessageCodec {
        MessageCodec::new(MessageCodec::DEFAULT_MAX)
    }

    #[test]
    fn server_hello_roundtrip() {
        let msg = ServerHello {
            protocol_version: 1,
            engine_version: String::from("0.1.0"),
            supported_caps: vec![],
        };
        let c = codec();
        let data = c.encode_handshake(&msg).unwrap();
        let decoded: ServerHello = c.decode_handshake(&data).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn client_hello_roundtrip() {
        let msg = ClientHello {
            protocol_version: 1,
            client_instance_id: 42,
            client_caps: vec![String::from("test_cap")],
        };
        let c = codec();
        let data = c.encode_handshake(&msg).unwrap();
        let decoded: ClientHello = c.decode_handshake(&data).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn hello_ack_roundtrip() {
        let msg = HelloAck {
            session_id_base: 1000,
        };
        let c = codec();
        let data = c.encode_handshake(&msg).unwrap();
        let decoded: HelloAck = c.decode_handshake(&data).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn hello_rejected_roundtrip() {
        let msg = HelloRejected {
            reason: String::from("unsupported protocol"),
            engine_version: String::from("0.1.0"),
        };
        let c = codec();
        let data = c.encode_handshake(&msg).unwrap();
        let decoded: HelloRejected = c.decode_handshake(&data).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn decode_wrong_type_returns_error() {
        let msg = ServerHello {
            protocol_version: 1,
            engine_version: String::from("0.1.0"),
            supported_caps: vec![],
        };
        let c = codec();
        let data = c.encode_handshake(&msg).unwrap();
        // Try to decode as ClientHello — should fail
        let result: Result<ClientHello, _> = c.decode_handshake(&data);
        assert!(result.is_err());
    }
}
