# 架构全景

## 5 层架构 (DRAFT §4)

```text
┌──────────────────────────────────────────┐
│ 平台接入层                                │
│ Windows TSF / macOS IMK / Fcitx5 / IBus  │
│ (cheime-win 子模块, Stage 2)             │
├──────────────────────────────────────────┤
│ UI 与渲染层                               │
│ (Stage 4)                                │
├──────────────────────────────────────────┤
│ 输入会话与流水线层                        │
│ Session / Pipeline / Candidate Snapshot   │
│ ← cheime-session, cheime-pipeline        │
├──────────────────────────────────────────┤
│ 数据与扩展层                              │
│ Dictionary / User Data / Lua / Emoji      │
│ ← cheime-dictionary, cheime-user-data,    │
│   cheime-lua, cheime-extension            │
├──────────────────────────────────────────┤
│ 服务层                                    │
│ Config Deploy / Sync / Diagnostics        │
│ ← cheime-config, cheime-diagnostics       │
└──────────────────────────────────────────┘
```

## Crate 依赖图

```text
cheime-model     ← 零依赖, 所有 crate 共享的类型定义
  ↑
cheime-protocol  ← 消息协议, 依赖 model
  ↑
cheime-config    ← 配置系统, 依赖 model
cheime-dictionary← 词库编译, 依赖 model
cheime-user-data ← 用户数据, 依赖 model
cheime-diagnostics← 结构化错误, 零依赖
  ↑
cheime-pipeline  ← 流水线, 依赖 config + dict + user-data + diagnostics
  ↑
cheime-session   ← 会话状态机, 依赖 pipeline + protocol
  ↑
cheime-cli       ← 命令行工具, 集成全部
```

## 数据流: 一次按键的全路径

```
用户按键 'n'
  ↓
[CLI / 平台层]
  FrontendMessage::KeyCommand { key: Character('n'), ... }
  ↓
[cheime-session]
  Session::handle() → validate_header() → handle_key()
  revision += 1
  ↓
[cheime-pipeline]
  ComposablePipeline::apply()
    ├─ KeyMapper::map()        → 全拼透传 / Flypy 两键→拼音
    ├─ Processor::process()    → composition = "n"
    ├─ Segmentor::segment()    → [{code: "n", tag: "partial"}]
    ├─ Normalizer::normalize() → (fuzzy: z→zh 等, 可选)
    ├─ Translator::translate() → [UserDictTranslator, DictTranslator, EmojiTranslator]
    │    DictTranslator: index.query("n") → BTreeMap::get → Vec<Candidate>
    ├─ Filter::filter()        → DedupFilter
    └─ Ranker::rank()          → UnifiedRanker: source优先级 + 码长 + emoji bonus
  ↓
  PipelineUpdate { composition: "n", candidates: [...], intent: None }
  ↓
[cheime-session]
  publish: PlatformAction(SetPreedit) + CandidateSnapshot
  ↓
[CLI / 平台层]
  显示 preedit + 候选列表
```

## 核心类型

### cheime-model (共享词汇表)

```rust
// 会话标识
Revision(u64)           // 单调递增, 防异步回写
Sequence(u64)           // 消息序号
SessionId / SessionEpoch / ClientInstanceId / ActionId

// 候选
Candidate { id, text, annotation, source, is_emoji }
CandidateSnapshot { preedit, cursor, candidates, highlighted, page_size, page }

// 输入
Key { Character(char), Backspace, Enter, Escape, Space, ... }
KeyEvent { key, state: KeyState }
UiCommand { SelectCandidate, MoveHighlight, NextPage, PreviousPage, Dismiss }
PlatformActionKind { SetPreedit, Commit, ClearPreedit, CancelComposition }
```

### cheime-pipeline (7 个 trait)

```rust
trait KeyMapper      { fn map(&mut self, event: &KeyEvent) -> MappedKeys; }
trait Processor      { fn process(&self, composition, event) -> ProcessorOutput; }
trait Segmentor      { fn segment(&self, composition) -> Vec<CodeSegment>; }
trait CodeNormalizer { fn normalize(&self, segment) -> Vec<CodeSegment>; }
trait Translator     { fn translate(&self, segments: &[CodeSegment]) -> Vec<Candidate>; }
trait Filter         { fn filter(&self, candidates) -> Vec<Candidate>; }
trait Ranker         { fn rank(&self, candidates) -> Vec<Candidate>; }
```

### cheime-config (配置编译链)

```
源 YAML → serde parse → extends 链解析 → 深度 merge → validate → deploy → current.txt
                                                      ↓ 失败
                                               保留 current, 返回结构化错误
```

## 性能特征 (rime_ice 539K 词库, Intel Ultra 9 285H)

| 指标 | 实测 | DRAFT 目标 | 裕度 |
|---|---|---|---|
| 单键延迟 (P50) | 4.75 µs | < 1ms | 210× |
| 长句延迟 (23 键) | 18.7 µs | < 10ms | 535× |
| 首屏候选 | 1.18 µs | < 1ms | 847× |
| 词库查询 (单码) | 427 ns | 稳定可预测 | — |
| 词库构建 (539K) | 475 ms | — | — |
| 缓存命中构建 | ~50 ms | — | — |

## CheIME vs Rime 架构差异

| 维度 | Rime | CheIME |
|---|---|---|
| **组件引用** | `"translator@name"` 字符串 | `Box<dyn Translator>` trait object |
| **类型安全** | 运行时 YAML 解析 | 编译时类型检查 + deny_unknown_fields |
| **配置继承** | `__include` + `__patch` 字符串 | 类型化 struct merge (递归 overlay) |
| **部署** | YAML 原地编辑, 无回滚 | 版本化目录 + 原子 current.txt 切换 |
| **错误** | ad-hoc 字符串 | 结构化 DiagnosticError (code + severity + fix) |
| **Emoji** | `simplifier@emoji` filter | `EmojiTranslator` (Translator trait) |
| **排序** | 每个 translator 独立 | `UnifiedRanker` 跨全部 translator 统一排序 |
| **正字法** | OpenCC 核心路径 | OpenCC = compat filter; 原生 = lexeme profile (Stage 3) |
| **用户在途管理** | 二元 blob | SQLite 事件日志 (可审计, 可撤销, 可同步) |
| **I/O 契约** | 文件约定 | JSON 管道 (KeyEvent→EngineMessage), 可脚本化测试 |
