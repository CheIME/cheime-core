# CheIME 配置系统设计

> 目标：自由度超越 Rime，同时保证类型安全、可验证、可迁移。

---

## 一、Rime 配置的局限（我们要超越的）

| 局限 | Rime 现状 | CheIME 方案 |
|------|----------|------------|
| **无类型校验** | 字段名拼错 → 静默忽略 | 编译期类型检查，未知字段报错 |
| **无按应用/窗口条件配置** | 只能靠 Lua 脚本实现 | 原生 `when:` 条件表达式 |
| **继承靠手写 __include** | 每个文件显式 `__include: default:/xxx` | 自动分层合并 + 显式 `extends` |
| **Pipeline 组件是字符串** | `- ascii_composer` → 运行时查表 | 类型化组件引用，参数强校验 |
| **无能力检查** | 主题请求 mica → Windows 7 上静默退化 | 部署前校验能力清单，不支持 = 拒绝部署 |
| **Patch 局限** | 仅支持 append/prepend/set | 增加 `insert_before`, `insert_after`, `remove`, `replace`, `merge` |
| **无表达式语言** | 条件逻辑必须 Lua | 内置轻量表达式（when 子句、speller algebra） |
| **无版本迁移** | 手动改配置 | 声明式 schema_version + 迁移规则 |
| **switch 平铺** | 所有开关同级，无依赖关系 | 支持开关分组、级联启用 |
| **无 profile 概念** | 只能切换 schema | schema + profile 双层：方案管输入逻辑，profile 管外观/行为偏好 |
| **Pipeline 创建后不可变** | `InitializeComponents()` 只调用一次 | 运行时动态启用/禁用/重排组件 |
| **条件系统极度受限** | 仅 `key_binder` 和 `schema_list` 支持 | 统一条件表达式引擎，可用于任何配置节点 |
| **无热重载** | 改配置需重新部署 + 重启引擎 | 文件监听 → 编译 → 原子切换，不丢 session |
| **无列表删除操作** | 只能整列表覆盖 | `remove`、`delete` 指令 |
| **switch 仅支持布尔** | radio group = 多个布尔值映射 | 类型化选项: bool/int/string/enum/list |
| **无组件参数默认值文档** | 每个组件自行解析 config subtree | 每个组件声明参数 schema，引擎校验 + 生成文档 |
---

## 二、配置分层模型（6 层，优先级从低到高）

```
Layer 6: Session      ← 会话级临时覆盖（无痕模式、单次切换）
Layer 5: App          ← 按应用覆盖（terminal: 关闭学习, IDE: 英文标点）
Layer 4: Profile      ← 用户偏好（外观、快捷键、候选数）
Layer 3: Schema       ← 方案定义（引擎管线、词典、拼写规则）
Layer 2: Platform     ← 平台默认（快捷键约定、字体回退）
Layer 1: System       ← 引擎内置默认值
```

合并规则：
- 深层 key 合并（不是整层替换）
- 列表操作：`append`, `prepend`, `replace`, `remove`, `insert_before`, `insert_after`
- 标量操作：`set`

### 示例

```yaml
# profiles/coding.yaml
extends: base
patch:
  menu.page_size: { set: 5 }
  ascii_punct: { set: true }
  learning.enabled: { set: false }

# profiles/writing.yaml  
extends: base
patch:
  menu.page_size: { set: 9 }
```

```yaml
# app_overrides/terminal.yaml
when:
  app: "com.terminal.*"
patch:
  learning.enabled: { set: false }
  network.enabled: { set: false }
  surrounding_text.read: { set: false }
```

---

## 三、管线组件配置（类型化）

### Rime 的方式（字符串 + 约定）

```yaml
engine:
  processors:
    - ascii_composer           # ← 字符串，参数从同文件其他节读取
    - recognizer
    - lua_processor@*my_proc   # ← @name 约定
  translators:
    - table_translator@custom_phrase
```

问题：
1. `ascii_composer` 的参数在哪？散落在文件的 `ascii_composer:` 节
2. 组件名和配置节名必须匹配（约定）
3. 无法在列表内直接写参数
4. 同一类型多个实例靠 `@name` 区分（字符串约定）

### CheIME 的方式（类型化）

```yaml
engine:
  processors:
    - type: ascii_composer
      switch_key:
        Caps_Lock: clear
        Shift_L: commit_code
    - type: recognizer
      patterns:
        email: "^[A-Za-z][-_.0-9A-Za-z]*@.*$"
        url: "^(www[.]|https?:|ftp[.:]).*$"
    - type: lua
      ref: my_proc           # ← 引用注册的 Lua 组件
  segmentors:
    - type: pinyin_syllable
    - type: ascii
    - type: fallback
  translators:
    - type: dict
      dictionary: rime_ice
      ref: main_dict          # ← 可选：命名此实例，供 patch 定位
      enable_completion: true
      initial_quality: 1.2
    - type: table
      dictionary: custom_phrase
      initial_quality: 99
    - type: lua
      ref: date_translator
  filters:
    - type: uniquifier
    - type: simplifier
      opencc_config: emoji.json
    - type: lua
      ref: autocap_filter
  ranker:
    type: unified
    weights:
      frequency: 1.0
      user_history: 0.8
      source_priority: 0.6
      code_length: 0.3
```

**优势**：
1. 每个组件的参数在其 `{ }` 块内，不分散
2. 编译期校验：`type: dict` 只接受 dict 组件定义的字段
3. `ref` 用于定位，可被 patch 精确索引：`engine.translators[ref=main_dict].initial_quality`
4. 列表操作语义明确

---

## 四、条件表达式（when 子句）

不用 Lua 实现简单的条件逻辑。使用受限的表达式语言：

### 语法

```yaml
when:
  # 应用匹配（支持 glob）
  app: "com.terminal.*"
  
  # 窗口标题匹配
  window_title: "*password*"
  
  # 组合条件
  all_of:
    - app: "com.vscode.*"
    - window_title: "*.ts"
  
  # 或条件
  any_of:
    - app: "com.terminal.*"
    - app: "com.ssh.*"
  
  # 否定
  not:
    app: "com.browser.*"
  
  # 平台
  platform: "windows"
  
  # 输入类型
  input_type: "password"   # 密码框自动匹配
```

### 使用场景

```yaml
# 1. App 级覆盖
app_overrides:
  - when: { app: "com.terminal.*" }
    patch:
      learning: { set: false }
      network: { set: false }
  
  - when: { any_of: [{app: "com.vscode.*"}, {app: "com.jetbrains.*"}] }
    patch:
      ascii_punct: { set: true }
      menu.page_size: { set: 5 }

# 2. Key binding 条件
key_binder:
  - when: { composing: true, has_menu: false }
    accept: Tab
    send: Shift+Right
  - when: { has_menu: true }
    accept: Tab
    send: Page_Down

# 3. 隐私控制
privacy:
  - when: { input_type: "password" }
    enforce:
      learning: false
      network: false
      clipboard: false
```

---

## 五、拼写规则（Speller Algebra）

Rime 用 `xform`/`derive`/`abbrev`/`erase`/`fuzz` 规则链处理拼写。我们保持这个模型但增强：

```yaml
speller:
  alphabet: "zyxwvutsrqponmlkjihgfedcba"
  initials: "zyxwvutsrqponmlkjihgfedcba"
  delimiter: " '"
  
  # 代数规则（兼容 Rime 语法，增强为结构化）
  algebra:
    # 删除规则
    - erase: "^xx$"
    
    # 派生规则（生成额外拼写变体）
    - derive: "^([jqxy])u$"
      to: "$1v"
      label: "ju→jv"   # 可选：诊断标签
    
    # 变换规则（就地替换）
    - xform: "iu$"
      to: "Ⓠ"
    
    # 模糊音规则
    - fuzz: "zh"
      to: "z"
    - fuzz: "ch" 
      to: "c"
    - fuzz: "sh"
      to: "s"
    
    # 缩写规则
    - abbrev: "^([a-z]).*$"
      to: "$1"
      min_length: 3    # 至少 3 个字符才缩写
```

---

## 六、开关系统（Switches）

Rime 的 switches 是平铺列表。我们支持分组和级联：

```yaml
switches:
  # 分组显示
  - group: "输入模式"
    switches:
      - id: ascii_mode
        label: "中/英"
        states: ["中", "Ａ"]
        hotkey: "Shift"
      - id: full_shape
        label: "半/全角"
        states: ["半角", "全角"]
        hotkey: "Control+."
  
  - group: "显示偏好"
    switches:
      - id: traditional
        label: "简/繁"
        states: ["简", "繁"]
        hotkey: "Control+/"
      - id: emoji
        label: "emoji"
        states: ["💀", "😄"]
        default: true   # 默认开启
  
  # 级联：开启 traditional 时才显示 traditional_variant
  - id: traditional_variant
    depends_on: { switch: traditional, state: 1 }
    label: "繁体变体"
    states: ["TW", "HK"]
```

---

## 七、版本化与迁移

```yaml
# 配置文件头部
schema_version: 2

# 引擎内置迁移规则（不写在用户配置中）
# 当 engine 加载 config 时，检测 schema_version，按迁移链升级：
#   v1 → v2: ascii_composer 字段从 default.yaml 移到 schema.yaml
#   v2 → v3: translator/preedit_format 从 regex 改为结构化
```

引擎内置迁移表：

```rust
// cheime-config/src/migrations.rs
fn migrations() -> Vec<Migration> {
    vec![
        Migration::new(1, 2, |config| {
            // migrate v1 to v2
        }),
        Migration::new(2, 3, |config| {
            // migrate v2 to v3
        }),
    ]
}
```

---

## 八、原子部署与回滚

```
runtime/
├── deployments/
│   ├── 2026-07-21T170000Z-a1b2c3/   ← 完整部署包
│   │   ├── compiled/
│   │   │   ├── rime_ice.index
│   │   │   ├── rime_ice.prism
│   │   │   └── luna_pinyin.index
│   │   ├── schemas/
│   │   ├── themes/
│   │   └── lua/
│   └── 2026-07-21T180000Z-d4e5f6/
└── current → deployments/2026-07-21T170000Z-a1b2c3/
```

- 部署前执行全部校验
- 校验失败 → 不替换 `current`，保留错误报告
- `current` 是原子 symlink 替换
- 保留最近 N 个部署用于回滚
- 诊断产物保存在部署目录内

---

## 九、Profile 系统（超越 Rime 的关键）

Profile 是方案之上的偏好层。一个方案可以被多个 profile 使用：

```yaml
# profiles/coding.yaml
extends: base
name: "编程"
description: "英文标点、小候选窗、关学习"
patch:
  menu.page_size: { set: 5 }
  ascii_punct: { set: true }
  learning.enabled: { set: false }

# profiles/writing.yaml
extends: base
name: "写作" 
description: "大候选窗、开学习、繁简转换"
patch:
  menu.page_size: { set: 9 }
  learning.enabled: { set: true }
  switches.traditional: { set: false }
```

用户可以 **方案 + Profile 组合**：
```
方案: 小鹤双拼 × Profile: 编程
方案: 小鹤双拼 × Profile: 写作
方案: 全拼      × Profile: 编程
```

---

## 十、与 Rime 的迁移兼容

1. **导入**: 工具自动将 Rime 配置转为 CheIME 格式
   - `default.yaml` → `platform.yaml` + `system.yaml`
   - `.schema.yaml` → `schemas/<name>.yaml`
   - `.custom.yaml` → `profiles/<name>.yaml`
2. **导出**: 反向转换用于差分测试
3. **共存**: 同一安装目录下 Rime 格式和 CheIME 格式可共存，通过 `format: rime | native` 声明

---

## 十一、实现路线图

| 优先级 | 功能 | 依赖 |
|--------|------|------|
| P0 | 类型化 Schema 定义（Rust struct） | 无 |
| P0 | YAML 加载 + 严格校验（未知字段报错） | Schema 定义 |
| P0 | `extends` + 分层合并 | YAML 加载 |
| P1 | `patch` 操作（set/append/remove/replace） | 分层合并 |
| P1 | Pipeline 组件从配置构建 | Schema + Patch |
| P1 | 原子部署 + 回滚 | 文件系统 |
| P2 | `when` 条件表达式引擎 | 无 |
| P2 | App-level overrides | when 引擎 |
| P2 | Switch 系统（分组 + 级联） | Schema |
| P3 | Profile 系统 | Switch + Override |
| P3 | 版本迁移框架 | Schema |
| P3 | Rime 配置导入工具 | 全部 |
