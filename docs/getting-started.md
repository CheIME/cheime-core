# CheIME CLI 快速上手

> 雾凇词库 + 智能学习，命令行版 CheIME 引擎。

---

## 一、环境准备

### 安装 Rust 工具链

需要 **Rust 1.85+**。推荐使用 [rustup](https://rustup.rs/) 安装：

```bash
rustup update stable
```

### 克隆仓库

```bash
git clone <repo-url> cheime
cd cheime
```

### 编译

```bash
cargo build -p cheime-cli
```

首次构建会下载并编译所有依赖，耗时较长。后续增量编译很快，通常在数秒内完成。

---

## 二、交互模式

交互模式是默认运行方式，直接在终端内输入拼音、查看候选并选词上屏。

```bash
cargo run -p cheime-cli
```

### 按键说明

| 按键 | 功能 |
|------|------|
| `a`–`z` | 输入拼音字母 |
| `Space` / `Enter` | 提交当前高亮候选上屏 |
| `Backspace` | 删除最后一个拼音字母 |
| `Escape` | 退出程序 |

### 交互示例

```
$ cargo run -p cheime-cli
CheIME CLI — 雾凇词库 + 智能学习
DB: C:\Users\me\AppData\Local\cheime\cheime_cli_user.db

> nihao
ni hao >1.你好 2.拟好 3.你 4.尼 5.泥
→ 你好
```

输入 `nihao` 后，终端显示：

- **preedit**：拼音区 `ni hao`
- **候选列表**：编号 + 候选词，`>` 标记当前高亮项
- 按 `Space` 提交 `你好`，绿色 `→ 你好` 表示上屏

### 聚焦与翻页

当前交互模式支持单页候选列表。翻页（`Page_Down`/`Page_Up`）和数字键直接选词的完整交互将在后续迭代中提供。

---

## 三、JSON 模式

JSON 模式面向自动化集成和工具链场景：引擎通过 stdin/stdout 以结构化 JSON 进行通信。

```bash
cargo run -p cheime-cli -- --json
```

### 输入格式：KeyEvent

stdin 每行一个 JSON 对象，描述一次按键事件：

```json
{"key":{"Character":"n"},"state":{"shift":false,"control":false,"alt":false}}
```

**KeyEvent 字段**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `key` | `"Character" \| "n"`、`"Backspace"`、`"Escape"`、`"Enter"`、`"Space"` | 按键类型 |
| `key.Character` | `char` | 单个字母 `a`–`z` |
| `state.shift` | `bool` | Shift 是否按下 |
| `state.control` | `bool` | Ctrl 是否按下 |
| `state.alt` | `bool` | Alt 是否按下 |

> **注意**：`state` 中的 `shift`、`control`、`alt` 三个字段缺一不可，否则反序列化会失败。

### 输出格式：EngineMessage

stdout 每行一个 JSON 对象，引擎的输出消息：

| 变体 | 含义 |
|------|------|
| `SessionOpened` | 会话已建立 |
| `CandidateSnapshot` | 候选快照（含 preedit + 候选列表） |
| `PlatformAction` | 平台操作（上屏 Commit、设置 Preedit 等） |
| `SessionClosed` | 会话已关闭 |
| `ProtocolRejected` | 协议版本不匹配 |

**CandidateSnapshot 示例**：

```json
{"CandidateSnapshot":{"header":{"protocol_version":1,"client":1,"session":1,"epoch":1,"sequence":3,"revision":0,"deployment":1},"snapshot":{"epoch":1,"revision":4,"deployment":1,"preedit":"ni hao","cursor":6,"candidates":[{"id":12,"text":"你好","annotation":null,"source":"rime_ice","is_emoji":false},{"id":13,"text":"拟好","annotation":null,"source":"rime_ice","is_emoji":false}],"highlighted":12,"status":"Composing","page_size":9,"page":0}}}
```

**PlatformAction (Commit) 示例**：

```json
{"PlatformAction":{"header":{"protocol_version":1,"client":1,"session":1,"epoch":2,"sequence":4,"revision":0,"deployment":1},"action":{"id":1,"epoch":2,"revision":3,"kind":{"Commit":{"text":"你好"}}}}}
```

### 管道示例

从 shell 直接向引擎发送 JSON 按键序列：

```bash
printf '{"key":{"Character":"n"},"state":{"shift":false,"control":false,"alt":false}}\n' \
       '{"key":{"Character":"i"},"state":{"shift":false,"control":false,"alt":false}}\n' \
       '{"key":{"Character":"h"},"state":{"shift":false,"control":false,"alt":false}}\n' \
       '{"key":{"Character":"a"},"state":{"shift":false,"control":false,"alt":false}}\n' \
       '{"key":{"Character":"o"},"state":{"shift":false,"control":false,"alt":false}}\n' \
       '{"key":"Space","state":{"shift":false,"control":false,"alt":false}}\n' \
  | cargo run -p cheime-cli -- --json
```

与前端的集成模式：

```
 Frontend                     stdin/stdout                     Engine
 ┌─────────┐   KeyEvent JSON ──────────────>    ┌─────────┐
 │   UI    │                                    │ cheime  │
 │ process │   <────────────── EngineMessage    │   CLI   │
 └─────────┘               JSON                └─────────┘
```

---

## 四、数据目录

### 默认路径

| 操作系统 | 默认数据目录 |
|----------|-------------|
| Windows | `%LOCALAPPDATA%\cheime\` |
| 其他 | 当前目录 |

可通过环境变量 `CHEIME_DATA_DIR` 覆盖：

```bash
# Windows PowerShell
$env:CHEIME_DATA_DIR = "D:\cheime-data"

# Linux / macOS
export CHEIME_DATA_DIR=/home/me/.cheime
```

### 目录结构

```
%LOCALAPPDATA%\cheime\
├── cheime_cli_user.db      # 用户词库 (SQLite WAL)
└── cache\
    └── dicts\              # 词典编译缓存
        └── rime_ice_base\
            └── <sha256>.bin
```

### 用户词库

`cheime_cli_user.db` 使用 SQLite WAL 模式存储用户提交的词条和学习数据。运行时自动创建，无需手动初始化。

### 词典缓存

词典缓存位于 `cache/dicts/<name>/<hash>.bin`，其中 `<hash>` 是词典源文件内容的 SHA256 值。引擎首次加载词典时编译并写入缓存；后续启动直接反序列化缓存，大幅缩短冷启动时间。

> CLI 默认将雾凇词库 (`rime_ice_base.dict.yaml`) 编译进二进制内（通过 `include_str!`），因此交互模式不需要外部词典文件即可运行。

---

## 五、常见问题

### 编译太慢

首次 `cargo build -p cheime-cli` 需要下载并编译所有依赖（约数百个 crate），在普通机器上可能耗时数分钟。这是 Rust 项目的正常现象。后续增量编译（修改源码后重新 build）通常只需几秒。

**加速技巧**：

- 使用 `sccache`：`cargo install sccache`，然后设置 `RUSTC_WRAPPER=sccache`
- 切换到国内 crates.io 镜像（如清华 tuna）

### 候选列表为空

可能原因与排查步骤：

1. **词典未加载**：确认 `data/dicts/` 目录下存在 `.dict.yaml` 文件。CLI 内置了雾凇词库，但如果通过 Pipeline 配置了额外词典，需要确保文件存在。
2. **拼音切分失败**：检查输入是否为合法拼音音节组合。非拼音字符（数字、大写字母、标点）在交互模式下会被忽略。
3. **运行日志**：观察 stderr 输出，引擎在启动时会打印加载的词典条目数。

### JSON 输入报错

如果出现 `bad input` 错误：

1. **确认 KeyState 字段完整**：`state` 对象必须包含 `shift`、`control`、`alt` 三个布尔字段，缺一不可。例如 `{"state":{}}` 会解析失败。
2. **检查 Key 枚举值**：`Character` 必须包含字母值；其他变体 (`Backspace`、`Escape`、`Enter`、`Space`) 不需要额外数据。
3. **每行一个 JSON**：JSON 模式按行解析，确保每个 KeyEvent 独占一行，无多余空白或换行符混入 JSON 内部。

### 交互模式按键无反应

交互模式仅在终端支持 raw input 时正常工作。如果按键被终端拦截（如 `Ctrl+C` 被 shell 捕获），尝试：

- 使用 Windows Terminal 而非传统 cmd.exe
- 确认终端未开启 IME 拦截模式
- 按 `Escape` 正常退出，若卡住可用 `Ctrl+C` 强制退出
