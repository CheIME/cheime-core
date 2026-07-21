# 词库系统

## 格式: Rime `.dict.yaml`

CheIME 兼容 Rime 词库格式。文件结构:

```yaml
# YAML 头 (可选)
---
name: rime_ice_base
version: "1.0"
sort: by_weight
use_preset_vocabulary: false
import_tables:
  - cn_dicts/base
columns:
  - text
  - code
  - weight
...

# TSV 正文 (Tab 分隔)
你好	ni hao	300000
中国	zhong guo	500000
世界	shi jie	200000
```

### 列格式

| 列 | 含义 | 必需 |
|---|---|---|
| `text` | 候选文本 | ✅ |
| `code` | 拼音编码 (空格分隔多音节) | ✅ |
| `weight` | 权重 (整数, 越大越靠前) | 可选, 默认 0 |
| `stem` | 词干 (未实现) | 可选 |

### 雾凇词库 (rime_ice)

- 使用文件: `data/dicts/rime_ice_base.dict.yaml`
- 条目数: 539,071
- 编码数量: ~455,000 unique codes
- 格式: 3 列 TSV (text + code + weight)
- YAML 头: ~2,960 行注释 + 元数据
- 数据起始标记: 单独的 `...` 行

### 明月词库 (luna_pinyin)

- 使用文件: `data/dicts/luna_pinyin.dict.yaml`
- 条目数: ~68,000
- 格式: 2 列 / 3 列混合 (部分行无 weight)

## 编译为 CompiledIndex

```
.dict.yaml 文件
  ↓ parse_header()    → DictHeader { columns, import_tables, ... }
  ↓ parse_body()      → Vec<DictEntry>
  ↓ CompiledIndex::build() → BTreeMap<code, Vec<(text, weight)>>
  ↓ 排序: 每个 code 组内按 weight desc, text asc 排序
  ↓ 计算 source_hash (SHA256 of content)
  ↓ CompiledIndex { entries, total_entries, source_hash, generation }
```

### 数据结构

```rust
pub struct CompiledIndex {
    pub generation: DeploymentGeneration,
    pub source_hash: String,         // SHA256 of all entries (deterministic)
    pub total_entries: usize,        // 总条目数
    entries: BTreeMap<String, Vec<(String, Option<i64>)>>,  // code → candidates
}
```

### 查询性能

`index.query("ni hao")` → `BTreeMap::get` → **427ns** (539K 条目)

BTreeMap 查找复杂度: O(log n), n = 唯一编码数 (~455K), log₂(455K) ≈ 19 次比较。

## 词典缓存 (DictCache)

避免每次启动重新解析 16MB 文件。

### 缓存布局

```
cache/dicts/
  rime_ice_base/
    a1b2c3d4e5f6.bin       ← SHA256 of source file content
    7890abcdef01.bin       ← 另一个版本
  luna_pinyin/
    ...
```

### 工作流

```rust
use cheime_dictionary::{DictCache, DictColumn, DeploymentGeneration};

let cache = DictCache::new(PathBuf::from("cache"));
let files = vec![PathBuf::from("data/dicts/rime_ice_base.dict.yaml")];
let cols = &[DictColumn::Text, DictColumn::Code, DictColumn::Weight];

// 首次: 解析 + 编译 + 缓存到 .bin → 475ms
// 再次: 从 .bin 加载 + 合并 → ~50ms
let index = cache.load_or_build(&files, "rime_ice", cols, DeploymentGeneration::new(1))?;
```

### API

| 方法 | 说明 |
|---|---|
| `new(cache_dir)` | 创建缓存管理器 |
| `load_or_build(files, name, cols, gen)` | 加载或构建。hash 不变则命中缓存 |
| `invalidate(name)` | 强制清除某词库的全部缓存 |
| `cached_hashes(name)` | 列出当前缓存的 hash 值 (诊断用) |

### 分文件策略

如果词库拆分为多个文件, DictCache 会分别缓存:

```rust
let files = vec![
    "data/dicts/rime_ice_base.dict.yaml",
    "data/dicts/my_custom.dict.yaml",
];

// 两个文件各有独立缓存
// 只有变更的文件会重新编译
// 所有片段在最后合并为一个 CompiledIndex
let index = cache.load_or_build(&files, "combined", cols, gen)?;
```

### 合并策略

```
文件 A 缓存 (hash 不变) → 加载 CacheFragment A
文件 B 缓存 (hash 变了) → 解析 B + 编译 B + 存缓存
                           ↓
                      merge: A.entries.extend(B.entries)
                           ↓
                      按组重排序 (weight desc, text asc)
                           ↓
                      CompiledIndex
```

## 导入链 (import_tables)

Rime `.dict.yaml` 的 `import_tables` 字段指定依赖的其他词库:

```yaml
import_tables:
  - cn_dicts/base
  - cn_dicts/ext
```

CheIME 的 `resolve_imports()` 解析 DAG 依赖并合并, 检测循环依赖:

```rust
let resolved: Vec<DictEntry> = resolve_imports(&dicts, &header)?;
```

## 自定义词库

### 格式要求

- TSV 格式, Tab 分隔
- 第一行: `text<TAB>code` (2 列) 或 `text<TAB>code<TAB>weight` (3 列)
- 编码规范: 拼音用空格分隔音节, 如 `zhong guo`
- 权重: 整数, 越大越靠前

### 示例

```
自定义短语	zi ding yi duan yu	100
测试词	ce shi ci	50
```

### 加载

```rust
let body = std::fs::read_to_string("my_dict.tsv")?;
let entries = parse_body(&body, &[DictColumn::Text, DictColumn::Code, DictColumn::Weight])?;
let index = CompiledIndex::build(entries, gen);
```
