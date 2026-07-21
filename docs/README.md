# CheIME 文档

澈输入法引擎。Rust 原生、离线完整、类型化流水线、原子配置部署。

## 快速导航

| 文档 | 内容 |
|---|---|
| [快速开始](getting-started.md) | 安装、运行 CLI、JSON 模式、数据目录 |
| [Schema 配置参考](schema.md) | 所有 YAML 字段的类型、默认值、枚举变体 |
| [流水线组件](pipeline.md) | 7 阶段流水线、trait 定义、内置实现 |
| [词库系统](dictionary.md) | .dict.yaml 格式、缓存机制、分文件策略 |
| [配置系统](config.md) | 继承链、原子部署、目录结构 |
| [架构全景](architecture.md) | 5 层架构、10 crates、CheIME vs Rime 对比 |

## 项目结构

```
cheime-core/
├── docs/                      ← 文档 (你在看这里)
├── config/schemas/            ← 方案配置文件 (base/quanpin/flypy)
├── data/
│   ├── dicts/                 ← Rime 词库 (.dict.yaml)
│   └── opencc/                ← OpenCC 转换表 (TSV)
├── crates/                    ← 引擎核心库
│   ├── cheime-model           ← 数据模型 (Candidate, KeyEvent, Snapshot...)
│   ├── cheime-protocol        ← 消息协议 (FrontendMessage/EngineMessage)
│   ├── cheime-config          ← 配置系统 (schema, merge, deploy)
│   ├── cheime-dictionary      ← 词库 (解析, 索引, 缓存)
│   ├── cheime-pipeline        ← 流水线 (Processor→Segmentor→...→Ranker)
│   ├── cheime-session         ← 会话状态机 (revision, pagination)
│   ├── cheime-user-data       ← 用户数据 (SQLite, 事件模型)
│   ├── cheime-diagnostics     ← 结构化错误码
│   ├── cheime-wire            ← 二进制帧协议
│   ├── cheime-lua             ← Lua 脚本运行时
│   └── cheime-extension       ← 原生扩展接口
└── apps/
    └── cheime-cli             ← 命令行工具 (交互 + JSON I/O)
```

## 快速开始

```bash
# 构建
cargo build -p cheime-cli

# 交互模式
cargo run -p cheime-cli
# 输入 nihao → 显示 1.你好 2.拟好 3.👍
# Space 提交

# JSON 模式 (脚本集成)
echo '{"key":{"Character":"n"},"state":{"shift":false,"control":false,"alt":false}}' \
  | cargo run -p cheime-cli -- --json

# 运行所有测试
cargo test --workspace          # 179 tests, 24 suites

# 运行基准 (需雾凇词库 ~16MB)
cargo bench --workspace
```

## 核心概念

### 流水线

```
KeyEvent → KeyMapper → Processor → Segmentor → Normalizer → Translators → Filters → Ranker → CandidateSnapshot
           (Flypy)     (按键)       (音节切分)   (模糊音)      (词库+用户+emoji)  (去重)    (多信号排序)
```

### 配置驱动

```yaml
# flypy.yaml — 小鹤双拼仅需替换 KeyMapper,词库完全共用
extends: ["../schemas/base.yaml"]
speller:
  max_code_length: 2
```

### 原子部署

```text
runtime/deployments/
  2026-07-21T08-00-00Z-a1b2c3/   ← 新部署
  2026-07-20T12-00-00Z-d4e5f6/   ← 旧版本保留
current.txt → 原子切换, 失败不替换
```

### CheIME vs Rime

| 特性 | Rime | CheIME |
|---|---|---|
| 组件模型 | 字符串引用, 运行时查找 | **Rust trait, 编译时类型安全** |
| 排序 | 每个 translator 独立排序 | **UnifiedRanker 统一重排** |
| Emoji | OpenCC filter 取巧 | **Translator trait 一等公民** |
| 配置合并 | YAML `__include` 字符串拼接 | **类型化 struct overlay** |
| 错误处理 | 静默忽略未知字段 | **deny_unknown_fields + 结构化错误码** |
| 部署 | YAML 原地编辑 | **版本化目录 + 原子切换 + 失败回滚** |
| I/O 契约 | 文件约定 | **JSON KeyEvent→EngineMessage 管道** |
| 正字法 | OpenCC 核心路径 | **OpenCC = compat filter; 原生 = lexeme profile** |
