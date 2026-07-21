# 流水线组件

7 阶段输入流水线。每个阶段是 Rust trait，通过 `ComposablePipeline` 组装。

## 架构

```
Key Event
  ↓
KeyMapper      ← 可选的编码映射 (Flypy 双拼状态机 / 全拼透传)
  ↓
Processor      ← 按键处理: 字母追加 / Backspace / Enter 提交 / Escape 取消
  ↓
Segmentor      ← 音节切分: 贪心前缀 Trie, O(n) 84ns 典型延迟
  ↓
Normalizer     ← 编码规范化: Fuzzy 模糊音 (zh→z, ch→c, sh→s, ang→an...)
  ↓
Translator(s)  ← 候选生成: UserDictTranslator, DictTranslator, EmojiTranslator
  ↓
Filter(s)      ← 候选后处理: DedupFilter 去重
  ↓
Ranker         ← 统一排序: UnifiedRanker (来源优先级 + 码长 + emoji bonus)
  ↓
CandidateSnapshot
```

## 1. KeyMapper (code: `key_mapper.rs`)

```rust
pub trait KeyMapper: Send + Sync {
    fn map(&mut self, event: &KeyEvent) -> MappedKeys;
}
```

有状态组件, 通过 `Mutex` 实现内部可变性。

### 内置实现

| 实现 | 说明 |
|---|---|
| `QuanPinMapper` | 全拼透传: 字母键直接映射到自身 |
| `FlypyMapper` | 小鹤双拼状态机: 2 键→拼音 (v→zh/ui, i→ch/i, u→sh/u...) |

### 配置

```yaml
# KeyMapper 是运行时组件, 通过 PipelineFactory::build() 传入
# 不在 YAML 中配置
let pipeline = PipelineFactory::build(config, store, dict, Some(Box::new(FlypyMapper::new())));
```

## 2. Processor (code: `processor.rs`)

```rust
pub trait Processor: Send + Sync {
    fn process(&self, composition: &str, event: &KeyEvent) -> Result<ProcessorOutput, PipelineError>;
}
```

### 内置实现

**DefaultProcessor** — 覆盖基础按键场景:

| 按键 | 行为 |
|---|---|
| 小写字母 a–z | 追加到 composition |
| Backspace | 删除最后一个字符 |
| Enter / Space | 返回 `PipelineIntent::CommitHighlighted` |
| Escape | 返回 `PipelineIntent::Cancel` |
| 大写/其他 | 忽略 |

## 3. Segmentor (code: `segmentor.rs`)

```rust
pub trait Segmentor: Send + Sync {
    fn segment(&self, composition: &str) -> Vec<CodeSegment>;
}
```

### 内置实现

**PinyinSegmentor** — 前缀 Trie + 贪心最长匹配:

- 包含全部 400+ 有效汉语拼音音节
- 从输入字符串左端开始, 每次取最长可匹配音节
- 无法匹配时取剩余全部 (保留未完成输入)
- 性能: ~93ns for "zhongguo"

示例:
```
"nihao"    → [{code: "ni"}, {code: "hao"}]
"xianshiqi"→ [{code: "xian"}, {code: "shi"}, {code: "qi"}]
"zhongguo" → [{code: "zhong"}, {code: "guo"}]
"n"        → [{code: "n", tag: "partial"}]
```

**PassthroughSegmentor** — 整个 composition 作为一个 segment (回退)。

## 4. Normalizer (code: `normalizer.rs`)

```rust
pub trait CodeNormalizer: Send + Sync {
    fn normalize(&self, segment: &CodeSegment) -> Vec<CodeSegment>;
}
```

将一个 segment 展开为零个或多个变体 (用于模糊音)。

### 内置实现

**FuzzyNormalizer** — 13 条规则, 每条规则匹配 segment.code 的前缀:

| 规则 | 替换 | 示例 |
|---|---|---|
| zh→z | "zha"→["zha","za"] | 知道/资道 |
| ch→c | "cha"→["cha","ca"] | 茶/擦 |
| sh→s | "sha"→["sha","sa"] | 沙/撒 |
| n→l | "na"→["na","la"] | 那/拉 |
| l→n | "la"→["la","na"] | 拉/那 |
| f→h | "fa"→["fa","ha"] | 发/哈 |
| h→f | "ha"→["ha","fa"] | 哈/发 |
| ang→an | "fang"→["fang","fan"] | 方/翻 |
| eng→en | "feng"→["feng","fen"] | 风/分 |
| ing→in | "xing"→["xing","xin"] | 星/新 |

**PassthroughNormalizer** — 不做任何变换。

## 5. Translator (code: `translator.rs`, `emoji.rs`)

```rust
pub trait Translator: Send + Sync {
    fn name(&self) -> &str;
    fn translate(&self, segments: &[CodeSegment]) -> Vec<Candidate>;
}
```

**所有 segment 一次传入** — Translator 自行拼接 code (如 `["ni","hao"]` → `"ni hao"`)。

### 内置实现

| 实现 | 说明 |
|---|---|
| `DictTranslator` | BTreeMap 词典查询: 拼接 code → `index.query("ni hao")` |
| `UserDictTranslator` | SQLite 用户词库查询, 标注频率 |
| `EmojiTranslator` | 拼音 + 关键词双索引, 内置 50+ emoji |
| `PassthroughTranslator` | 回退: 直接输出 code 文本 |

### Emoji 查询示例

```
"zan" → EmojiTranslator.by_pinyin.get("zan") → ["👍"]
"hao" → EmojiTranslator.by_pinyin.get("hao") → ["👍"]
"xiao" → EmojiTranslator.by_pinyin.get("xiao") → ["😀", "😂"]
```

## 6. Filter (code: `filter.rs`, `simplifier.rs`)

```rust
pub trait Filter: Send + Sync {
    fn name(&self) -> &str;
    fn filter(&self, candidates: Vec<Candidate>) -> Vec<Candidate>;
}
```

### 内置实现

| 实现 | 说明 |
|---|---|
| `DedupFilter` | 按 text 去重, 保留首次出现 |
| `SimplifierFilter` | OpenCC 兼容简繁转换 (s2t/t2s), 标注来源为 compat |

SimplifierFilter 核心行为:
```
输入: [Candidate { text: "中国", source: "dict:abc" }]
输出 (s2t + annotate): 
  [Candidate { text: "中國", source: "dict:abc→simplified" }]
```

## 7. Ranker (code: `ranker.rs`)

```rust
pub trait Ranker: Send + Sync {
    fn name(&self) -> &str;
    fn rank(&self, candidates: Vec<Candidate>) -> Vec<Candidate>;
}
```

### 内置实现

**UnifiedRanker** — 多信号统一排序 (CheIME 核心优势):

```
score = source_priority × w_source + (1 / text_length) × w_length + emoji_bonus

source_priority:
  user_*  → 1.0   (用户词最高)
  dict_*  → 0.8   (词典词)
  builtin → 0.7   (内置)
  emoji   → 0.5   (emoji 低于文本)
  other   → 0.3
```

Rime 对比: Rime 每个 translator 独立排序, 无法统一调整来源权重。
