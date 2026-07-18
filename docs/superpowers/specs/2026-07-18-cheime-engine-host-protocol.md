# CheIME Engine Host Protocol Design

> 日期：2026-07-18
> 状态：已确认
> 适用范围：`cheime-core` 传输层格式定义与 Windows TIP↔Engine IPC 协议

## 1. 定位

本文定义 CheIME 引擎与平台前端的通信格式。传输层格式（分帧、编解码、握手消息）定义在 `cheime-core` 的新 crate `cheime-wire` 中。具体 I/O 实现（Named Pipe 创建、连接、ACL、读写循环）由 Windows 前端仓库负责。

`cheime-wire` 是纯格式定义 crate：只操作字节切片，不接触任何平台 I/O 句柄（pipe handle、socket、文件描述符）。

## 2. Crate 结构

```
cheime-core/
├── crates/
│   ├── cheime-wire/          ← 新增
│   │   ├── Cargo.toml        ← 依赖 cheime-protocol + rmp-serde
│   │   └── src/
│   │       ├── lib.rs        ← pub mod frame, pub mod codec, pub mod handshake
│   │       ├── frame.rs      ← FramedReader / FramedWriter (长度前缀分帧)
│   │       ├── codec.rs      ← MessageCodec (MessagePack + 64KB上限)
│   │       └── handshake.rs  ← ServerHello, ClientHello, HelloAck, HelloRejected
│   ├── cheime-protocol/      ← 不变
│   └── ...
```

依赖方向：`cheime-model` ← `cheime-protocol` ← `cheime-wire` ← Windows 前端

## 3. 分帧格式

### 3.1 帧结构

```
┌──────────────┬────────────────────────────────┐
│ length: u32  │ payload: [u8; length]          │
│ (big-endian) │ (MessagePack-encoded message)  │
└──────────────┴────────────────────────────────┘
```

- `length` 包含 payload 字节数，不含自身 4 字节
- `length == 0` 允许但无操作（保留给未来 keepalive）
- `length > 65536`（64 KiB）→ 非法帧

### 3.2 FramedWriter

```rust
pub struct FramedWriter;

impl FramedWriter {
    /// 将消息序列化为带长度前缀的帧
    /// 返回写入 buf 的字节数
    pub fn write_frame<M: Serialize>(
        buf: &mut [u8],
        codec: &MessageCodec,
        msg: &M,
    ) -> Result<usize, WireError>;
}
```

### 3.3 FramedReader

```rust
pub struct FramedReader;

impl FramedReader {
    /// 从 buf 中读取第一帧 payload 的位置和长度
    /// 返回 (payload_start_in_buf, payload_len)
    /// 如果 buf 中还没有完整帧，返回 Ok(None)
    /// 如果帧头损坏（长度 = 0 或 > max），返回 Err
    pub fn read_frame(
        buf: &[u8],
        max_size: usize,
    ) -> Result<Option<(usize, usize)>, WireError>;
}
```

设计要点：
- **零拷贝**：`read_frame` 返回 buffer 内偏移，不分配
- **不阻塞**：reader/writer 是纯函数，I/O 由前端控制
- **无 unsafe**：长度前缀使用 `u32::from_be_bytes`

## 4. 编解码

### 4.1 MessageCodec

```rust
pub struct MessageCodec {
    max_size: usize,  // 默认 65536
}

impl MessageCodec {
    pub const DEFAULT_MAX: usize = 65536;

    pub fn new(max_message_size: usize) -> Self;

    // Frontend message encode/decode
    pub fn encode_frontend(&self, msg: &FrontendMessage) -> Result<Vec<u8>, WireError>;
    pub fn decode_frontend(&self, data: &[u8]) -> Result<FrontendMessage, WireError>;

    // Engine message encode/decode
    pub fn encode_engine(&self, msg: &EngineMessage) -> Result<Vec<u8>, WireError>;
    pub fn decode_engine(&self, data: &[u8]) -> Result<EngineMessage, WireError>;
}
```

### 4.2 WireError

```rust
pub enum WireError {
    /// 帧或消息超过大小上限
    SizeExceeded { actual: usize, max: usize },
    /// MessagePack 序列化失败
    Encode(String),
    /// MessagePack 反序列化失败（含未知 variant）
    Decode(String),
    /// 缓冲区不完整：长度前缀指示的 payload 超出实际可用数据
    IncompleteFrame { expected: usize, available: usize },
    /// 长度前缀为零或占用字节数多于实际帧
    InvalidFrameLength,
    /// 握手阶段收到非握手消息
    ProtocolViolation(String),
}
```

### 4.3 序列化格式

使用 MessagePack（`rmp-serde` crate）：
- 自描述：enum variant 以 field name 编码，新增 variant 向后兼容
- 紧凑：候选人列表等重复结构压缩良好
- 有界：序列化后的字节数组长度已知，配合长度前缀分帧天然支持

`cheime-protocol` 的 `FrontendMessage` 和 `EngineMessage` 已有 `#[derive(Serialize, Deserialize)]`，直接通过 `rmp-serde` 编码。

## 5. 握手协议

### 5.1 流程

```
TIP (client)                          Engine (server)
     │                                      │
     │── ConnectPipe ──────────────────────>│  命名管道连接建立
     │                                      │
     │<── ServerHello ──────────────────────│  protocol_version + engine_version
     │                                      │
     │── ClientHello ──────────────────────>│  protocol_version + client_instance_id
     │                                      │
     │<── HelloAck ─────────────────────────│  握手成功
     │   OR HelloRejected ─────────────────>│  版本不兼容，关闭连接
     │                                      │
     │── OpenSession ──────────────────────>│  后续所有消息都走分帧格式
     │<── SessionOpened ────────────────────│
     │   ...                                │
```

### 5.2 握手消息类型

也走 MessagePack，但属于独立消息类型，不经 `FrontendMessage`/`EngineMessage` 枚举：

```rust
/// 引擎在连接建立后立即发送
pub struct ServerHello {
    pub protocol_version: u16,
    pub engine_version: String,       // semver, e.g. "0.1.0"
    pub supported_caps: Vec<String>,  // MVP 保留为空
}

/// TIP 收到 ServerHello 后发送
pub struct ClientHello {
    pub protocol_version: u16,
    pub client_instance_id: ClientInstanceId,
    pub client_caps: Vec<String>,     // MVP 保留为空
}

/// 引擎验证通过后发送
pub struct HelloAck {
    pub session_id_base: u64,
}

/// 版本不兼容时引擎发送此消息并关闭连接
pub struct HelloRejected {
    pub reason: String,
    pub engine_version: String,
}
```

### 5.3 握手约束

- 握手有超时：引擎在 5 秒内未收到 `ClientHello` 即关闭连接
- `protocol_version` 不匹配 → 引擎发送 `HelloRejected` 后立即关闭管道
- 握手成功后，此后所有消息使用 `FrontendMessage`/`EngineMessage` 分帧格式
- 握手消息本身不使用长度前缀分帧（它们由 pipe 消息模式读取，具体在前端仓库实现）
- `cheime-wire` 只定义握手消息类型和 encode/decode，不做超时逻辑

## 6. 连接与会话生命周期

### 6.1 连接拓扑

每 TIP 客户端使用独立命名管道：
- 引擎监听 `cheime-engine` 管道接受新连接
- 协商完成后引擎创建 `cheime-engine.{client_id}` 作为专用通信管道
- 专用管道断开 = 对应 `client_instance_id` 的所有 session 立即清理

### 6.2 会话边界

- 一根连接上可有多个 session（对应该宿主的不同编辑 context）
- 管道断开 = 该 client 所有 session 清理
- 引擎退出 = 所有已连接客户端断开，TIP 各自进入透明输入
- 重连 = 新管道连接 + 新握手 + 新 `client_instance_id`，不恢复旧 session

### 6.3 错误恢复

| 场景 | 行为 |
|------|------|
| 管道断开，无 composition | TIP 不吞普通按键 |
| 管道断开，有 composition | 隐藏候选窗，结束旧 composition，进入透明输入 |
| 管道断开，未决 commit | 旧 epoch 失效，不重发 |
| 重连成功 | 新 epoch，只处理新按键 |
| 旧消息到达 | 按 epoch/sequence/action_id 丢弃 |

## 7. 不在本设计的范围

以下内容推迟到后续 ADR：

- 退避重连算法（指数退避参数、最大重试次数）
- 引擎心跳/keepalive 机制
- 引擎版本兼容性矩阵规则
- 加密/签名
- 握手 caps 协商具体内容
- Named Pipe 具体创建、ACL 配置和 I/O 循环（属于 Windows 前端仓库）
