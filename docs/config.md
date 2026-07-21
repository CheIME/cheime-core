# 配置系统

CheIME 配置系统特点: 类型化 schema, 显式继承, 原子部署, 严格校验。

## 配置目录 (DRAFT §8.1)

```
config/
├── schemas/           ← 方案配置 (输入流水线)
│   ├── base.yaml      ← 公共基础
│   ├── quanpin.yaml   ← 全拼方案
│   └── flypy.yaml     ← 小鹤双拼
├── dictionaries/      ← 词库定义 (待实现)
└── opencc/            ← OpenCC 转换表
    └── s2t.tsv
```

## Schema 文件格式

### 最小可工作配置

```yaml
schema_version: 1
engine:
  segmentors:
    - type: pinyin_syllable
```

### 完整示例 (quanpin.yaml)

```yaml
schema_version: 1
extends:
  - "../schemas/base.yaml"    # 继承基础流水线

engine:
  translators:
    - type: emoji              # 在 base 的基础上增加 emoji

  filters:
    - type: simplifier         # 可选: 简繁转换
      option_name: s2t
      opencc_config: "../../data/opencc/s2t.tsv"

speller:
  alphabet: "zyxwvutsrqponmlkjihgfedcba"
  initials: "bpmfdtnlgkhjqxzcsryw"
  delimiter: "'"

menu:
  page_size: 9
```

### 完整示例 (flypy.yaml)

```yaml
schema_version: 1
extends:
  - "../schemas/base.yaml"    # 公用基础 (processor + segmentor + dict + filter)

# flypy 仅覆盖 speller.max_code_length
# 词库、segmentor、ranker 全部复用 base

speller:
  max_code_length: 2          # 双拼: 恰好 2 键/音节
  delimiter: "'"

menu:
  page_size: 9
```

## 继承系统

### extends 链

```yaml
extends:
  - "../schemas/base.yaml"
  - "../schemas/extra.yaml"
```

解析过程:
1. 递归加载父配置 (深度优先, 最底层优先合并)
2. 子配置覆盖父配置 (递归 overlay)
3. Engine 列表: 子列表 prepend 到父列表前面
4. Switches: 子完全替换父
5. Speller/Menu: 子字段覆盖父字段

### 循环依赖检测

```yaml
# a.yaml extends b, b.yaml extends a → 部署时返回错误:
E-CONFIG-CIRCULAR: a.yaml
```

### 合并规则总结

| 字段 | 合并策略 |
|---|---|
| `engine.processors` | 子 prepend 父 |
| `engine.segmentors` | 子 prepend 父 |
| `engine.translators` | 子 prepend 父 |
| `engine.filters` | 子 prepend 父 |
| `switches` | 子完全替换父 |
| `speller.*` | 子字段覆盖父字段 (None 不覆盖) |
| `menu.*` | 子字段覆盖父字段 |
| `schema` | 子完全替换父 |
| `schema_version` | 子胜出 |

### 代码使用

```rust
use cheime_config::{ConfigLoader, SchemaConfig};

// 从 YAML 字符串加载 (解析 extends 链)
let config: SchemaConfig = ConfigLoader::new()
    .with_base_dir("config/schemas")
    .load(yaml_str)?;

// 从文件加载 (extends 路径相对于文件所在目录)
let config: SchemaConfig = ConfigLoader::new()
    .load_file(Path::new("config/schemas/quanpin.yaml"))?;
```

## 原子部署 (DeploymentManager)

```rust
use cheime_config::DeploymentManager;

let mgr = DeploymentManager::new(PathBuf::from("runtime"));

// 部署 (解析 → 验证 → 写版本目录 → 原子切换 current.txt)
let handle = mgr.deploy(yaml_str)?;

// 读取当前部署
let current = mgr.current()?;
println!("schema version: {}", current.schema.schema_version);

// 列出所有部署版本
for d in mgr.list_deployments()? {
    println!("  {d}");
}

// 失败时: current.txt 不变, 旧版本继续运行
```

### 部署目录结构

```
runtime/
├── deployments/
│   ├── EPOCH-21345-08-30-00-a1b2c3d4/
│   │   ├── schema.yaml          ← 部署的配置
│   │   └── diagnostics.json     ← 验证报告
│   └── EPOCH-21346-12-15-00-e5f6g7h8/
└── current.txt                  ← 指向活跃版本, e.g. "deployments/EPOCH-21346-..."
```

### 原子性保证

1. 新配置写入 `deployments/<ts>-<hash>/schema.yaml`
2. 诊断报告写入同目录 `diagnostics.json`
3. `current.tmp` 写入新路径
4. `rename(current.tmp, current.txt)` — 文件系统原子操作
5. 步骤 1-2 失败: 不创建 `current.tmp`, 旧版本保持
6. 步骤 4 失败: `current.tmp` 残留, 下次部署覆盖

## 严格校验 (deny_unknown_fields)

所有 SchemaConfig 使用 `#[serde(deny_unknown_fields)]`:

```yaml
# ❌ 部署报错: unknown field `page_siz`
menu:
  page_siz: 9

# ❌ 部署报错: unknown variant `dictionary`
engine:
  translators:
    - type: dictionary

# ✅ 正确配置
menu:
  page_size: 9

engine:
  translators:
    - type: dict
      dictionary: rime_ice_base
```

错误格式:

```
E-CONFIG-VALIDATION
file: config/schemas/quanpin.yaml
path: engine.translators[0].type
message: unknown variant `dictionary`, expected one of `dict`, `table`, `script`, ...
```

## 配置校验流程 (DRAFT §8.4)

```
读取源文件
  ↓
解析 YAML                       ← serde + deny_unknown_fields
  ↓
处理 extends/import             ← ConfigLoader 递归加载 + 深度 merge
  ↓
类型检查                        ← 所有字段 Rust 类型, 编译时保证
  ↓
引用检查                        ← (未实现)
  ↓
能力与平台检查                   ← (未实现)
  ↓
权限检查                        ← (未实现)
  ↓
生成部署包                      ← schema.yaml + diagnostics.json
  ↓
原子切换                        ← DeploymentManager.deploy()
```
