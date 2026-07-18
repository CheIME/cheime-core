# CheIME Engine Host Protocol Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add `cheime-wire` crate with MessagePack serialization, length-prefix framing, and handshake message types. This crate defines the on-wire format for TIP↔Engine IPC without touching platform I/O.

**Architecture:** `cheime-wire` depends on `cheime-protocol` + `rmp-serde`. It provides pure byte-slice functions: `FramedReader`/`FramedWriter` for length-delimited framing, `MessageCodec` for MessagePack encode/decode, and handshake structs (`ServerHello`, `ClientHello`, `HelloAck`, `HelloRejected`).

## Global Constraints

- Work only in `D:/coding/cheime/cheime-core`.
- `cheime-wire` must not depend on platform I/O types (no Named Pipe, socket, file handles).
- All functions operate on `&[u8]` / `&mut [u8]` slices — no blocking calls.
- `#![forbid(unsafe_code)]`.
- Max message size: 65536 bytes (64 KiB).
- Frame format: 4-byte big-endian u32 length prefix + MessagePack payload.
- TDD: red, green, refactor, commit.
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` as quality gate.

---

### Task 1: Create `cheime-wire` crate skeleton

**Files:**
- Modify: `Cargo.toml` (add `rmp-serde` dep, add crate to members)
- Create: `crates/cheime-wire/Cargo.toml`
- Create: `crates/cheime-wire/src/lib.rs`

Add `rmp-serde = "1.3"` to workspace dependencies.

`cheime-wire/Cargo.toml`:
```toml
[package]
name = "cheime-wire"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
cheime-protocol = { path = "../cheime-protocol" }
rmp-serde.workspace = true
serde.workspace = true
thiserror.workspace = true
```

**Commit:** `chore: add cheime-wire crate skeleton`

---

### Task 2: Define WireError and MessageCodec

**Files:**
- Create: `crates/cheime-wire/src/error.rs`
- Create: `crates/cheime-wire/src/codec.rs`

**WireError:**
```rust
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum WireError {
    #[error("message size {actual} exceeds limit {max}")]
    SizeExceeded { actual: usize, max: usize },
    #[error("encode error: {0}")]
    Encode(String),
    #[error("decode error: {0}")]
    Decode(String),
    #[error("incomplete frame: expected {expected}, available {available}")]
    IncompleteFrame { expected: usize, available: usize },
    #[error("invalid frame length")]
    InvalidFrameLength,
    #[error("protocol violation: {0}")]
    ProtocolViolation(String),
}
```

**MessageCodec:**
```rust
pub struct MessageCodec { max_size: usize }

impl MessageCodec {
    pub const DEFAULT_MAX: usize = 65536;
    pub fn new(max_message_size: usize) -> Self;

    pub fn encode_frontend(&self, msg: &FrontendMessage) -> Result<Vec<u8>, WireError>;
    pub fn decode_frontend(&self, data: &[u8]) -> Result<FrontendMessage, WireError>;
    pub fn encode_engine(&self, msg: &EngineMessage) -> Result<Vec<u8>, WireError>;
    pub fn decode_engine(&self, data: &[u8]) -> Result<EngineMessage, WireError>;
}
```

Internal: after encoding, check `len() <= max_size` — reject if oversized. After decoding, same check (protect against decompression bombs). MessagePack serialization uses `rmp_serde::from_slice` / `rmp_serde::to_vec`.

Tests:
1. Round-trip: encode then decode `FrontendMessage::KeyCommand` → same value
2. Round-trip: encode then decode `EngineMessage::CandidateSnapshot` → same value
3. Oversized message returns `SizeExceeded`

**Commit:** `feat: add WireError and MessageCodec`

---

### Task 3: Implement FramedReader and FramedWriter

**Files:**
- Create: `crates/cheime-wire/src/frame.rs`

```rust
pub struct FramedWriter;

impl FramedWriter {
    pub fn write_frame<M: Serialize>(
        buf: &mut [u8],
        codec: &MessageCodec,
        msg: &M,
    ) -> Result<usize, WireError>;
}

pub struct FramedReader;

impl FramedReader {
    pub fn read_frame(
        buf: &[u8],
        max_size: usize,
    ) -> Result<Option<(usize, usize)>, WireError>;
}
```

`write_frame`:
1. Serialize `msg` via `codec.encode_*` → `Vec<u8>` payload
2. Encode `payload.len()` as big-endian u32 into first 4 bytes
3. Copy payload after the length prefix
4. Return total written bytes

`read_frame`:
1. If `buf.len() < 4` → `Ok(None)` (need more data)
2. Parse first 4 bytes as u32 big-endian → `length`
3. If `length == 0` → `Err(WireError::InvalidFrameLength)`
4. If `length > max_size` → `Err(WireError::SizeExceeded)`
5. If `buf.len() < 4 + length` → `Ok(None)` (need more data)
6. Return `Ok(Some((4, length)))` — payload starts at offset 4

Tests:
1. Write then read a frame → round-trip succeeds
2. Empty buffer returns None
3. Buffer with only 2 bytes returns None (partial header)
4. Buffer with full header but partial payload returns None
5. Zero-length frame returns InvalidFrameLength
6. Length exceeding max_size returns SizeExceeded
7. End-to-end: write FrontendMessage → read frame → decode → same value

**Commit:** `feat: add length-prefix frame reader and writer`

---

### Task 4: Define handshake message types

**Files:**
- Create: `crates/cheime-wire/src/handshake.rs`

```rust
use cheime_protocol::ClientInstanceId;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ServerHello {
    pub protocol_version: u16,
    pub engine_version: String,
    pub supported_caps: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClientHello {
    pub protocol_version: u16,
    pub client_instance_id: ClientInstanceId,
    pub client_caps: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HelloAck {
    pub session_id_base: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HelloRejected {
    pub reason: String,
    pub engine_version: String,
}
```

Add `encode_handshake<T: Serialize>(msg: &T) -> Result<Vec<u8>, WireError>` and `decode_handshake<T: DeserializeOwned>(data: &[u8]) -> Result<T, WireError>` to MessageCodec (or standalone free functions in handshake module — these are simple wrappers around `rmp_serde`).

Tests:
1. ServerHello round-trip
2. ClientHello round-trip
3. HelloAck round-trip
4. HelloRejected round-trip
5. Version mismatch: decode_handshake with wrong type returns Decode error

**Commit:** `feat: add handshake message types`

---

### Task 5: Wire everything together and verify workspace quality gate

- Update `crates/cheime-wire/src/lib.rs` with all pub mod and re-exports
- Run `cargo fmt --all -- --check`
- Run `cargo clippy --workspace --all-targets -- -D warnings`
- Run `cargo test --workspace`
- Run `cargo tree --workspace` to verify no platform deps leak

**Commit:** `chore: finalize cheime-wire public API and quality gate`
