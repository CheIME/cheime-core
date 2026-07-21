# CheIME Schema 配置参考

> CheIME 的类型化 schema 配置系统。每个字段映射到 Rust 类型，`deny_unknown_fields` 确保拼写错误和不支持的选项在解析时即被捕获。

---

## 目录

- [快速入门](#快速入门)
- [SchemaConfig — 顶层配置](#schemaconfig--顶层配置)
- [EngineConfig — 引擎流水线](#engineconfig--引擎流水线)
  - [ProcessorConfig — 处理器](#processorconfig--处理器)
  - [SegmentorConfig — 分词器](#segmentorconfig--分词器)
  - [TranslatorConfig — 翻译器](#translatorconfig--翻译器)
  - [FilterConfig — 过滤器](#filterconfig--过滤器)
- [SpellerConfig — 拼写引擎](#spellerconfig--拼写引擎)
- [MenuConfig — 菜单](#menuconfig--菜单)
- [SwitchGroup / SwitchConfig — 开关](#switchgroup--switchconfig--开关)
- [真实配置示例](#真实配置示例)

---

## 快速入门

一个最小可用的全拼 schema：

```yaml
# quanpin.yaml — 全拼输入方案
schema_version: 1

extends:
  - "../schemas/base.yaml"

engine:
  processors:
    - type: ascii_composer

  segmentors:
    - type: pinyin_syllable

  translators:
    - type: dict
      dictionary: rime_ice_base

  filters:
    - type: uniquifier

speller:
  alphabet: "zyxwvutsrqponmlkjihgfedcba"
  initials: "bpmfdtnlgkhjqxzcsryw"
  delimiter: "'"
  max_code_length: 0          # 0 = 不限长

menu:
  page_size: 9
  page_down_cycle: false
```

---

## SchemaConfig — 顶层配置

**Rust 类型:** `SchemaConfig`  
**序列化标记:** `deny_unknown_fields` — 未知字段会触发解析错误

| YAML 键 | Rust 类型 | 默认值 | 必填 | 说明 |
|---|---|---|---|---|
| `schema_version` | `u32` | `1` | 否 | Schema 格式版本。当前固定为 `1`。 |
| `extends` | `Vec<String>` | `[]` | 否 | 继承的父 schema 路径列表（相对路径）。部署时递归合并。 |
| `schema` | `Option<SchemaMeta>` | `None` | 否 | Schema 元信息（ID、名称、版本、描述）。 |
| `engine` | `EngineConfig` | `Default::default()` | 否 | 引擎流水线配置。 |
| `switches` | `Vec<SwitchGroup>` | `[]` | 否 | 状态开关组（如中英切换、繁简切换）。 |
| `speller` | `Option<SpellerConfig>` | `None` | 否 | 拼写引擎配置（字母表、模糊音、代数规则）。 |
| `menu` | `MenuConfig` | `page_size=9, cycle=false` | 否 | 候选菜单配置。 |

### SchemaMeta

| YAML 键 | Rust 类型 | 默认值 | 必填 | 说明 |
|---|---|---|---|---|
| `schema_id` | `Option<String>` | `None` | 否 | Schema 唯一标识符。 |
| `name` | `Option<String>` | `None` | 否 | Schema 显示名称。 |
| `version` | `Option<String>` | `None` | 否 | Schema 版本号（语义化版本）。 |
| `description` | `Option<String>` | `None` | 否 | Schema 描述文本。 |

```yaml
schema:
  schema_id: "quanpin"
  name: "全拼"
  version: "1.0.0"
  description: "标准汉语全拼输入方案"
```

---

## EngineConfig — 引擎流水线

**Rust 类型:** `EngineConfig`

```yaml
engine:
  processors:   # 按键处理器列表
  segmentors:   # 分词器列表
  translators:  # 翻译器列表
  filters:      # 过滤器列表
```

| YAML 键 | Rust 类型 | 默认值 | 必填 | 说明 |
|---|---|---|---|---|
| `processors` | `Vec<ProcessorConfig>` | `[]` | 否 | 按键处理器（按键 → 动作映射）。按列表顺序执行。 |
| `segmentors` | `Vec<SegmentorConfig>` | `[]` | 否 | 分词器（原始输入 → 编码序列）。按列表顺序尝试。 |
| `translators` | `Vec<TranslatorConfig>` | `[]` | 否 | 翻译器（编码 → 候选词）。按列表顺序收集结果。 |
| `filters` | `Vec<FilterConfig>` | `[]` | 否 | 过滤器（候选词 → 过滤/排序）。按列表顺序处理。 |

**处理顺序:** `processors → segmentors → translators → filters`

CheIME 流水线以输入事件（按键）为起点：

1. **Processors** 层拦截按键，决定是直接上屏、切换状态还是转发给分词层。
2. **Segmentors** 将原始编码串切分为音节/编码片段。
3. **Translators** 将编码片段翻译为候选词列表。
4. **Filters** 对候选词去重、简繁转换、字符集过滤等。

每个组件通过 `type` 字段标识具体实现。类型名称使用 snake_case。

---

### ProcessorConfig — 处理器

**Rust 类型:** `ProcessorConfig`（tagged enum，`tag = "type"`）

处理器的 `type` 字段唯一标识变体，其余字段由变体决定。

#### 变体一览

| `type` 值 | 配置结构体 | 说明 |
|---|---|---|
| `ascii_composer` | `AsciiComposerConfig` | ASCII 模式处理：中英文输入状态切换。 |
| `recognizer` | `RecognizerConfig` | 模式识别：匹配特定输入模式并执行动作。 |
| `key_binder` | `KeyBinderConfig` | 按键绑定：自定义按键映射和动作。 |
| `speller` | （无配置） | 拼写处理器：驱动编码输入与候选查找。 |
| `punctuator` | `PunctuatorConfig` | 标点处理器：符号键全角/半角映射。 |
| `selector` | （无配置） | 候选选择：翻页、上屏候选词。 |
| `navigator` | （无配置） | 光标导航：在编码串中移动编辑位置。 |
| `express_editor` | （无配置） | 表达式编辑器：直接编辑预编辑区文本。 |
| `lua` | `LuaComponentRef` | Lua 自定义处理器。 |

#### `ascii_composer` — ASCII 模式

```yaml
processors:
  - type: ascii_composer
    switch_key:
      Shift_L: commit_code      # 左 Shift 提交已输入编码并进入英文模式
      Shift_R: commit_text      # 右 Shift 提交候选文字并进入英文模式
```

**AsciiComposerConfig:**

| YAML 键 | Rust 类型 | 默认值 | 必填 | 说明 |
|---|---|---|---|---|
| `switch_key` | `BTreeMap<String, String>` | `{}` | 否 | 切换键映射：键名 → 切换模式（`commit_code`、`commit_text`、`inline_ascii`、`noop`、`clear`）。 |

#### `recognizer` — 模式识别

```yaml
processors:
  - type: recognizer
    patterns:
      url: "^https?://"
      email: "^[a-z]+@[a-z]+\\.[a-z]+"
      reverse_lookup: "^`[a-z]*$"
```

**RecognizerConfig:**

| YAML 键 | Rust 类型 | 默认值 | 必填 | 说明 |
|---|---|---|---|---|
| `patterns` | `BTreeMap<String, String>` | `{}` | 否 | 模式名 → 正则表达式。匹配成功触发对应动作。 |

#### `key_binder` — 按键绑定

```yaml
processors:
  - type: key_binder
    bindings:
      - when: always        # 条件：始终生效
        accept: "Control+j" # 接受的按键
        toggle: ascii_mode  # 动作：切换 ASCII 模式
      - when: composing     # 条件：编码中
        accept: "Escape"
        send: Escape        # 动作：转发 Escape
```

**KeyBinderConfig:**

| YAML 键 | Rust 类型 | 默认值 | 必填 | 说明 |
|---|---|---|---|---|
| `bindings` | `Vec<KeyBinding>` | `[]` | 否 | 按键绑定列表。 |

**KeyBinding:**

| YAML 键 | Rust 类型 | 默认值 | 必填 | 说明 |
|---|---|---|---|---|
| `when` | `String` | — | **是** | 触发条件（`always`、`composing`、`has_menu` 等）。 |
| `accept` | `String` | — | **是** | 接受的按键（如 `Control+j`、`Escape`、`Return`）。 |
| `send` | `Option<String>` | `None` | 否 | 转发按键序列（替代原按键）。与 `toggle` 互斥。 |
| `toggle` | `Option<String>` | `None` | 否 | 切换开关名。与 `send` 互斥。 |

> **注意:** 每条绑定中 `send` 和 `toggle` 互斥——同时指定两者是配置错误。

#### `speller` — 拼写处理器

无额外配置字段，仅类型标记：

```yaml
processors:
  - type: speller
```

#### `punctuator` — 标点处理器

```yaml
processors:
  - type: punctuator
    full_shape:
      ",": "，"      # 半角逗号 → 全角逗号
      ".": "。"
    half_shape:
      "[" : "【"     # 半角括号 → 全角括号
      "]" : "】"
```

**PunctuatorConfig:**

| YAML 键 | Rust 类型 | 默认值 | 必填 | 说明 |
|---|---|---|---|---|
| `full_shape` | `BTreeMap<String, Value>` | `{}` | 否 | 全角模式映射：按键 → 输出符号（支持字符串或字符串列表）。 |
| `half_shape` | `BTreeMap<String, Value>` | `{}` | 否 | 半角模式映射：同上。 |

#### `selector` — 候选选择

无额外配置字段：

```yaml
processors:
  - type: selector
```

#### `navigator` — 光标导航

无额外配置字段：

```yaml
processors:
  - type: navigator
```

#### `express_editor` — 表达式编辑器

无额外配置字段：

```yaml
processors:
  - type: express_editor
```

#### `lua` — Lua 处理器

```yaml
processors:
  - type: lua
    ref: "my_custom_processor"   # 引用 Lua 组件名
```

**LuaComponentRef（所有 Lua 变体共用）:**

| YAML 键 | Rust 类型 | 默认值 | 必填 | 说明 |
|---|---|---|---|---|
| `ref` | `String` | — | **是** | Lua 脚本中注册的组件名。 |

---

### SegmentorConfig — 分词器

**Rust 类型:** `SegmentorConfig`（tagged enum，`tag = "type"`）

#### 变体一览

| `type` 值 | 配置结构体 | 说明 |
|---|---|---|
| `pinyin_syllable` | （无配置） | 拼音音节分词：识别标准拼音音节边界。 |
| `ascii` | （无配置） | ASCII 分词：按单词边界切分英文输入。 |
| `abc` | （无配置） | ABC 分词：按固定编码长度切分（形码用）。 |
| `affix` | `AffixSegmentorConfig` | 词缀分词：识别前缀/后缀标记。 |
| `punct` | （无配置） | 标点分词：将标点符号作为独立 token。 |
| `fallback` | （无配置） | 回退分词：兜底切分策略。 |
| `lua` | `LuaComponentRef` | Lua 自定义分词器。 |

#### `pinyin_syllable` — 拼音音节

```yaml
segmentors:
  - type: pinyin_syllable
```

无额外字段。根据 speller 中定义的 `alphabet` 和 `initials` 自动处理声母/韵母切分。

#### `ascii` — ASCII 分词

```yaml
segmentors:
  - type: ascii
```

#### `abc` — ABC 分词

```yaml
segmentors:
  - type: abc
```

#### `affix` — 词缀分词

```yaml
segmentors:
  - type: affix
    tag: abc                # 用于标记分词结果的标签名
    prefix: "`"             # 前缀字符（如反引号开启词缀模式）
    suffix: "'"             # 后缀字符（如单引号结束词缀模式）
```

**AffixSegmentorConfig:**

| YAML 键 | Rust 类型 | 默认值 | 必填 | 说明 |
|---|---|---|---|---|
| `tag` | `Option<String>` | `None` | 否 | 附加到识别结果的标签名。 |
| `prefix` | `Option<String>` | `None` | 否 | 前缀标识符字符。 |
| `suffix` | `Option<String>` | `None` | 否 | 后缀标识符字符。 |

#### `punct` — 标点分词

```yaml
segmentors:
  - type: punct
```

#### `fallback` — 回退分词

```yaml
segmentors:
  - type: fallback
```

#### `lua` — Lua 分词

```yaml
segmentors:
  - type: lua
    ref: "my_segmentor"
```

配置同 [LuaComponentRef](#luacomponentref)。

---

### TranslatorConfig — 翻译器

**Rust 类型:** `TranslatorConfig`（tagged enum，`tag = "type"`）

#### 变体一览

| `type` 值 | 配置结构体 | 说明 |
|---|---|---|
| `dict` | `DictTranslatorConfig` | 词典翻译：基于词库的词组/整句翻译。 |
| `table` | `TableTranslatorConfig` | 码表翻译：基于结构化码表的单字翻译。 |
| `script` | `ScriptTranslatorConfig` | 脚本翻译：词库增强翻译（支持 prism 加速）。 |
| `punct` | （无配置） | 标点翻译：将标点编码转为标点候选。 |
| `echo` | （无配置） | 回显翻译：将原始输入显示为候选（调试用）。 |
| `lua` | `LuaComponentRef` | Lua 自定义翻译器。 |
| `emoji` | （无配置） | Emoji 翻译：表情符号候选。 |
| `history` | （无配置） | 历史翻译：记忆用户历史输入。 |

#### `dict` — 词典翻译

```yaml
translators:
  - type: dict
    dictionary: rime_ice_base          # 词典名（必填）
    enable_completion: true            # 启用逐字补全
    initial_quality: 1.0               # 初始权重（影响候选排序）
```

**DictTranslatorConfig:**

| YAML 键 | Rust 类型 | 默认值 | 必填 | 说明 |
|---|---|---|---|---|
| `dictionary` | `String` | — | **是** | 词典名称（对应 data 目录下的词典文件）。 |
| `ref` | `Option<String>` | `None` | 否 | 翻译器引用名（多翻译器协同时的标识）。 |
| `enable_completion` | `bool` | `true` | 否 | 是否启用逐字补全（输入不完整拼音时补全半选字）。 |
| `initial_quality` | `f64` | `0.0` | 否 | 翻译器初始权重。多个翻译器时，权重影响候选排序优先级。 |

#### `table` — 码表翻译

```yaml
translators:
  - type: table
    dictionary: wubi86                 # 码表名（必填）
    enable_sentence: false             # 是否启用组句
    initial_quality: 1.0
```

**TableTranslatorConfig:**

| YAML 键 | Rust 类型 | 默认值 | 必填 | 说明 |
|---|---|---|---|---|
| `dictionary` | `String` | — | **是** | 码表名称。 |
| `ref` | `Option<String>` | `None` | 否 | 翻译器引用名。 |
| `enable_sentence` | `bool` | `true` | 否 | 是否启用组句模式（多个编码连续输入自动组词）。 |
| `initial_quality` | `f64` | `0.0` | 否 | 翻译器初始权重。 |

#### `script` — 脚本翻译

```yaml
translators:
  - type: script
    dictionary: rime_ice               # 词库名（必填）
    enable_completion: true
    enable_sentence: true
    enable_correction: false           # 是否启用纠错
    initial_quality: 1.0
    prism: rime_ice                    # Prism 加速文件（可选）
```

**ScriptTranslatorConfig:**

| YAML 键 | Rust 类型 | 默认值 | 必填 | 说明 |
|---|---|---|---|---|
| `dictionary` | `String` | — | **是** | 词库名称。 |
| `ref` | `Option<String>` | `None` | 否 | 翻译器引用名。 |
| `enable_completion` | `bool` | `true` | 否 | 逐字补全。 |
| `enable_sentence` | `bool` | `true` | 否 | 组句模式。 |
| `enable_correction` | `bool` | `false` | 否 | 是否启用拼写纠错。 |
| `initial_quality` | `f64` | `0.0` | 否 | 翻译器初始权重。 |
| `prism` | `Option<String>` | `None` | 否 | Prism 加速文件名（预编译的 trie 索引）。有则显著提升加载速度。 |

#### `punct` — 标点翻译

```yaml
translators:
  - type: punct
```

#### `echo` — 回显翻译

```yaml
translators:
  - type: echo
```

#### `emoji` — Emoji 翻译

```yaml
translators:
  - type: emoji
```

#### `history` — 历史翻译

```yaml
translators:
  - type: history
```

#### `lua` — Lua 翻译

```yaml
translators:
  - type: lua
    ref: "my_translator"
```

配置同 [LuaComponentRef](#luacomponentref)。

---

### FilterConfig — 过滤器

**Rust 类型:** `FilterConfig`（tagged enum，`tag = "type"`）

#### 变体一览

| `type` 值 | 配置结构体 | 说明 |
|---|---|---|
| `uniquifier` | （无配置） | 去重过滤器：移除重复候选词。 |
| `simplifier` | `SimplifierConfig` | 简繁转换：候选词简繁转换。 |
| `charset_filter` | `CharsetFilterConfig` | 字符集过滤：按字符集过滤候选。 |
| `single_char` | （无配置） | 单字过滤器：过滤多字词，仅保留单字候选。 |
| `lua` | `LuaComponentRef` | Lua 自定义过滤器。 |

#### `uniquifier` — 去重

```yaml
filters:
  - type: uniquifier
```

#### `simplifier` — 简繁转换

```yaml
filters:
  - type: simplifier
    option_name: s2t                   # 转换选项名（对应 switch 中的开关 id）
    opencc_config: "../../data/opencc/s2t.tsv"   # OpenCC 配置文件路径
    tips: all                          # 显示提示（all/char 级别）
```

**SimplifierConfig:**

| YAML 键 | Rust 类型 | 默认值 | 必填 | 说明 |
|---|---|---|---|---|
| `option_name` | `Option<String>` | `None` | 否 | 关联的开关名。`s2t` 简化→繁体，`t2s` 繁体→简化。 |
| `opencc_config` | `Option<String>` | `None` | 否 | OpenCC TSV 配置文件路径（相对 schema 文件）。 |
| `tips` | `Option<String>` | `None` | 否 | 提示级别：`all`（所有词）、`char`（仅单字）、`none`。 |

#### `charset_filter` — 字符集过滤

```yaml
filters:
  - type: charset_filter
    charset: "gb2312"                  # 目标字符集
```

**CharsetFilterConfig:**

| YAML 键 | Rust 类型 | 默认值 | 必填 | 说明 |
|---|---|---|---|---|
| `charset` | `Option<String>` | `None` | 否 | 目标字符集名（如 `gb2312`、`utf8`）。不在字符集内的候选被剔除。 |

#### `single_char` — 单字过滤

```yaml
filters:
  - type: single_char
```

#### `lua` — Lua 过滤

```yaml
filters:
  - type: lua
    ref: "my_filter"
```

配置同 [LuaComponentRef](#luacomponentref)。

---

## SpellerConfig — 拼写引擎

**Rust 类型:** `SpellerConfig`

```yaml
speller:
  alphabet: "zyxwvutsrqponmlkjihgfedcba"    # 可用字母表
  initials: "bpmfdtnlgkhjqxzcsryw"          # 声母列表
  delimiter: "'"                             # 音节分隔符
  max_code_length: 0                         # 0 = 不限制最大编码长度
  auto_select: false                         # 自动上屏唯一候选
  use_space: false                           # 空格参与编码
  algebra:                                   # 拼写代数规则
    - rule: "xform"             # 规则类型
      pattern: "^([zcs])h"      # 匹配模式（正则）
      to: "$1"                  # 替换模板
      min_length: 2             # 最小编码长度阈值
```

| YAML 键 | Rust 类型 | 默认值 | 必填 | 说明 |
|---|---|---|---|---|
| `alphabet` | `Option<String>` | `None` | 否 | 可用字母集合。不在表中的按键不会进入编码。 |
| `initials` | `Option<String>` | `None` | 否 | 声母列表（用于拼音音节切分时识别声母边界）。 |
| `delimiter` | `Option<String>` | `None` | 否 | 音节分隔符（拼音中通常为 `'`）。 |
| `max_code_length` | `usize` | `0` | 否 | 最大编码长度。`0` 表示不限制。形码方案通常设为固定值（如 4）。 |
| `auto_select` | `bool` | `false` | 否 | 候选唯一时是否自动上屏。 |
| `use_space` | `bool` | `false` | 否 | 是否将空格作为编码的一部分（形码空格选重时通常为 `false`）。 |
| `algebra` | `Vec<SpellerAlgebra>` | `[]` | 否 | 拼写代数规则列表（按顺序应用）。 |

### SpellerAlgebra — 拼写代数规则

每条规则在编码输入时按顺序应用于原始输入串。

| YAML 键 | Rust 类型 | 默认值 | 必填 | 说明 |
|---|---|---|---|---|
| `rule` | `String` | — | **是** | 规则类型：`xform`（原地替换）、`erase`（删除匹配）、`derive`（派生新串）。 |
| `pattern` | `String` | — | **是** | 正则匹配模式。 |
| `to` | `Option<String>` | `None` | 否 | 替换模板（`$1` 引用捕获组）。仅 `xform`/`derive` 需要。 |
| `min_length` | `Option<usize>` | `None` | 否 | 最小编码长度阈值。编码长度不足时跳过此规则。 |

**常见用例:**

```yaml
# 模糊音：zh/ch/sh → z/c/s
algebra:
  - rule: xform
    pattern: "^zh"
    to: "z"
  - rule: xform
    pattern: "^ch"
    to: "c"
  - rule: xform
    pattern: "^sh"
    to: "s"

# 双拼展开：v → zh/ui（Flypy 映射由 KeyMapper 完成，非 algebra）
# algebra 更常用于模糊音和容错规则
```

---

## MenuConfig — 菜单

**Rust 类型:** `MenuConfig`

```yaml
menu:
  page_size: 9                          # 每页候选数
  page_down_cycle: false                # 翻页到底后是否循环回首页
  alternative_select_keys: "asdfghjkl"  # 替代选择键（不写则用默认 1-9）
```

| YAML 键 | Rust 类型 | 默认值 | 必填 | 说明 |
|---|---|---|---|---|
| `page_size` | `usize` | `9` | 否 | 每页显示的候选词数量。 |
| `page_down_cycle` | `bool` | `false` | 否 | 翻页到底后是否循环回首页（`true`）或停在末页（`false`）。 |
| `alternative_select_keys` | `Option<String>` | `None` | 否 | 替代选择键序列。不指定则使用 `1` 到 `page_size` 的数字键。字符串长度应与 `page_size` 一致。 |

---

## PunctuatorConfig — 标点符号映射

**Rust 类型:** `PunctuatorConfig`

**父字段:** `SchemaConfig.punctuator` (可选)

标点符号按键到输出的映射。支持 full_shape (全角模式) 和 half_shape (半角模式) 两套映射。

```yaml
punctuator:
  full_shape:
    ".": {commit: "。"}           # 单提交: 按 . 直接输出 。
    ",": {commit: "，"}
    "|": ["·", "｜", "§", "¦"]    # 候选列表: 按 | 显示多个符号供选择
    "$": ["￥", "$", "€", "£"]
    "\"": {pair: [""", """]}     # 配对符号: 按 " 输出 ""
  half_shape: {}                  # 半角模式 (通常为空, 直接输出原字符)
```

### 值类型

| 值格式 | 含义 | 行为 |
|---|---|---|
| `"字符串"` | 字面量提交 | 直接 commit 该字符串 |
| `{commit: "字符串"}` | 单提交 | 直接 commit |
| `["A", "B", "C"]` | 候选列表 | 显示候选, 用户选择 |
| `{pair: ["开", "闭"]}` | 配对符号 | 一次提交两个字符 |

### 数字透传

数字后 `.` 和 `:` 自动保持半角 (不需要配置):
- `3.14` — 小数点透传
- `12:30` — 冒号透传
- 非数字后恢复全角行为

| YAML 键 | Rust 类型 | 默认值 | 必填 | 说明 |
|---|---|---|---|---|
| `full_shape` | `BTreeMap<String, Value>` | `{}` | 否 | 全角模式映射 |
| `half_shape` | `BTreeMap<String, Value>` | `{}` | 否 | 半角模式映射 |

---

## SwitchGroup / SwitchConfig — 开关

**Rust 类型:** `SwitchGroup` + `SwitchConfig`

开关用于运行时切换输入状态（如中/英文模式、简/繁体等）。

```yaml
switches:
  - group: "基本模式"                   # 组名（可选，用于 UI 分组）
    switches:
      - id: ascii_mode                 # 开关唯一 ID（必填）
        label: "中"                     # 显示标签（必填）
        states: ["中", "英"]            # 状态列表（必填，至少 2 个）
        default: false                 # 默认状态：false = 第 0 个状态（"中"）
        hotkey: "Control+space"        # 快捷键（可选）

      - id: s2t
        label: "繁"
        states: ["简", "繁"]
        default: false
        depends_on:                    # 级联依赖（可选）
          switch: ascii_mode           # 依赖的开关 ID
          state: 0                     # 仅在 ascii_mode 为第 0 个状态时启用
```

### SwitchGroup

| YAML 键 | Rust 类型 | 默认值 | 必填 | 说明 |
|---|---|---|---|---|
| `group` | `Option<String>` | `None` | 否 | 开关组名称（UI 分组显示用）。为 `None` 时归入默认组。 |
| `switches` | `Vec<SwitchConfig>` | `[]` | 否 | 该组内的开关列表。 |

### SwitchConfig

| YAML 键 | Rust 类型 | 默认值 | 必填 | 说明 |
|---|---|---|---|---|
| `id` | `String` | — | **是** | 开关唯一标识符。被 `key_binder` 的 `toggle` 和 `simplifier` 的 `option_name` 引用。 |
| `label` | `String` | — | **是** | 开关的 UI 显示标签。 |
| `states` | `Vec<String>` | — | **是** | 状态名称列表。至少包含 2 个状态（开/关）。 |
| `default` | `bool` | `false` | 否 | 默认状态：`false` = 第 0 个状态，`true` = 第 1 个状态。 |
| `hotkey` | `Option<String>` | `None` | 否 | 快捷键（如 `Control+space`、`F4`）。 |
| `depends_on` | `Option<SwitchDependency>` | `None` | 否 | 级联依赖：当前开关仅在依赖开关处于指定状态时生效。 |

### SwitchDependency

| YAML 键 | Rust 类型 | 默认值 | 必填 | 说明 |
|---|---|---|---|---|
| `switch` | `String` | — | **是** | 依赖的开关 `id`。 |
| `state` | `u8` | — | **是** | 依赖开关的目标状态索引（从 0 开始）。 |

---

## 真实配置示例

### base.yaml — 基础流水线

```yaml
# CheIME base schema — 所有具体输入方案共享的流水线基础
schema_version: 1

extends: []

engine:
  processors:
    - type: ascii_composer

  segmentors:
    - type: pinyin_syllable

  translators:
    - type: dict
      dictionary: rime_ice_base

  filters:
    - type: uniquifier

menu:
  page_size: 9
  page_down_cycle: false
```

### quanpin.yaml — 全拼

```yaml
# CheIME QuanPin (全拼) schema
# 在 base 流水线基础上添加 emoji 翻译和简繁转换
schema_version: 1

extends:
  - "../schemas/base.yaml"

engine:
  translators:
    - type: emoji

  filters:
    - type: simplifier
      option_name: s2t
      opencc_config: "../../data/opencc/s2t.tsv"

speller:
  alphabet: "zyxwvutsrqponmlkjihgfedcba"
  initials: "bpmfdtnlgkhjqxzcsryw"
  delimiter: "'"
  max_code_length: 0

menu:
  page_size: 9
```

### flypy.yaml — 小鹤双拼

```yaml
# CheIME Flypy (小鹤双拼) schema
# 双拼通过 KeyMapper 实现，schema 仅需调整 max_code_length
schema_version: 1

extends:
  - "../schemas/base.yaml"

engine:
  translators:
    - type: emoji

# Flypy key mapper 是运行时组件，此处的注释仅为文档说明：
#   type: flypy
#   mapping:
#     a: [a, a]  v: [zh, ui]  i: [ch, i]  u: [sh, u]
#     ... (完整 26 键映射)

speller:
  alphabet: "zyxwvutsrqponmlkjihgfedcba"
  max_code_length: 2    # 双拼：每个音节恰好 2 个键
  delimiter: "'"

menu:
  page_size: 9
```

### 完整配置（含开关）

```yaml
# 完整示例：展示全部可配置项
schema_version: 1

schema:
  schema_id: "my_schema"
  name: "我的输入方案"
  version: "1.0.0"
  description: "自定义全拼方案，支持模糊音和简繁转换"

extends:
  - "../schemas/base.yaml"

switches:
  - group: "模式"
    switches:
      - id: ascii_mode
        label: "中"
        states: ["中", "英"]
        default: false
        hotkey: "Shift_L"

      - id: s2t
        label: "繁"
        states: ["简", "繁"]
        default: false
        hotkey: "Control+Shift+F"

engine:
  processors:
    - type: ascii_composer
      switch_key:
        Shift_L: commit_code
        Shift_R: commit_text

    - type: recognizer
      patterns:
        url: "^https?://"
        email: "^[a-z]+@[a-z]+\\.[a-z]+"

    - type: key_binder
      bindings:
        - when: always
          accept: "Control+j"
          toggle: ascii_mode
        - when: composing
          accept: "Escape"
          send: Escape

    - type: speller

    - type: punctuator
      full_shape:
        ",": "，"
        ".": "。"

    - type: selector

    - type: navigator

  segmentors:
    - type: pinyin_syllable
    - type: ascii
    - type: punct
    - type: fallback

  translators:
    - type: dict
      dictionary: rime_ice_base
      enable_completion: true
      initial_quality: 1.0

    - type: emoji

    - type: punct

    - type: history

  filters:
    - type: uniquifier

    - type: simplifier
      option_name: s2t
      opencc_config: "../../data/opencc/s2t.tsv"
      tips: all

    - type: charset_filter
      charset: "gb2312"

speller:
  alphabet: "zyxwvutsrqponmlkjihgfedcba"
  initials: "bpmfdtnlgkhjqxzcsryw"
  delimiter: "'"
  max_code_length: 0
  auto_select: false
  use_space: false
  algebra:
    - rule: xform
      pattern: "^zh"
      to: "z"
    - rule: xform
      pattern: "^ch"
      to: "c"
    - rule: xform
      pattern: "^sh"
      to: "s"

menu:
  page_size: 9
  page_down_cycle: false
  alternative_select_keys: "asdfghjkl;"
```

---

## 类型系统约定

| Rust 类型 | YAML 表示 | 示例 |
|---|---|---|
| `bool` | `true` / `false` | `enable_completion: true` |
| `u32` | 无符号整数 | `schema_version: 1` |
| `usize` | 非负整数 | `page_size: 9` |
| `f64` | 浮点数 | `initial_quality: 1.0` |
| `String` | 字符串（单/双引号可选） | `dictionary: rime_ice_base` |
| `Option<T>` | 可缺省 | 不写字段 = `None` |
| `Vec<T>` | YAML 列表 | `extends: [a, b]` 或块列表 |
| `BTreeMap<String, T>` | YAML 映射 | `switch_key: {Shift_L: commit_code}` |
| `LuaComponentRef` | 带 `ref` 键的映射 | `{type: lua, ref: "my_comp"}` |

所有 tagged enum（`ProcessorConfig`、`SegmentorConfig`、`TranslatorConfig`、`FilterConfig`）通过 `type` 字段区分变体。未指定 `type` 或不认识的 `type` 值会在反序列化时报错。

`deny_unknown_fields` 确保任何拼写错误的字段名都会触发解析错误——不会静默忽略。
