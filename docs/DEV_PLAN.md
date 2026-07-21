# CheIME Core 开发计划

> 基准: DRAFT.md §20 阶段划分 + 当前代码审计
> 更新: 2026-07-21

---

## 当前状态总览

| 模块 | 状态 | 说明 |
|------|------|------|
| cheime-model | ✅ 完成 | 所有 ID 类型、KeyEvent、Candidate、Snapshot、UiCommand |
| cheime-protocol | ✅ 完成 | 消息枚举、Header、版本校验 |
| cheime-wire | ✅ 完成 | MessagePack 编解码、帧协议、握手 |
| cheime-session | ⚠️ 基础 | 支持 KeyCommand/Commit，**忽略 UiCommand** |
| cheime-pipeline | ⚠️ 基础 | 单体 BuiltinPipeline，**无组件组合** |
| cheime-extension | ✅ 完成 | Processor/Segmentor/Translator/Filter trait 齐全 |
| cheime-lua | ✅ 完成 | 四个 trait 均实现，异步 Lua 未做 |
| cheime-dictionary | ⚠️ 基础 | YAML 导入 + BTreeMap 索引，**无切分/排序** |
| cheime-user-data | ⚠️ 基础 | 内存事件模型，**无持久化/管线接入** |
| cheime-config | ❌ 未实现 | 无此 crate |
| cheime-orthography | ❌ 未实现 | 无此 crate |
| cheime-emoji | ❌ 未实现 | 无此 crate |
| cheime-services | ❌ 未实现 | 无此 crate (同步/热词/诊断) |
| 基准测试 | ✅ 完成 | 68K Rime 词典检字 19µs，切分 84ns |

---

## 阶段 1：补齐基础设施 — 使引擎可"完整输入"

**目标**: 在命令行（stdin → stdout）完成完整输入流程：打字 → 出候选 → 选字 → 上屏 → 用户词学习。覆盖 DRAFT §22 验收标准的第 1,2,6,8,14 条。

### 1.1 Session 补全 UiCommand

**当前**: Session 收到 `UiCommand` 后直接忽略（`handle` 方法中未处理）。

**需要实现**:
- `SelectCandidate`: 将选中候选上屏（触发 Commit action），清除 pending 状态
- `NextPage` / `PreviousPage`: 候选翻页（需扩充 CandidateSnapshot 支持分页元数据）
- `MoveHighlight` / `Dismiss`: 高亮移动与取消

**涉及 crate**: `cheime-session`, `cheime-model`

### 1.2 Pipeline 组件化重构

**当前**: `InputPipeline` trait 只有一个 `apply()` 方法，BuiltinPipeline 是单体实现，没有 Processor→Segmentor→Translator→Filter→Ranker 的组件组合。

**需要实现**:

```rust
// 新的 Pipeline trait — 组件可组合
pub trait PipelineComponent: Send + Sync {
    fn name(&self) -> &str;
}

// 每个组件返回 PipelineUpdate 的累积结果
pub struct ComposedPipeline {
    processors: Vec<Box<dyn Processor>>,
    segmentor: Box<dyn Segmentor>,
    translators: Vec<Box<dyn Translator>>,
    filters: Vec<Box<dyn Filter>>,
    ranker: Box<dyn Ranker>,
}
```

**设计要点**:
- 每个组件有 `enabled: bool` 开关
- Translator 分同步/异步两类
- 组件从配置中构建，不硬编码顺序
- 保持现有 `apply()` 签名兼容，内部走组件链

**涉及 crate**: `cheime-pipeline`, `cheime-extension`

### 1.3 拼音音节切分器 (PinyinSegmentor)

**当前**: 基准测试中有贪心 Trie 原型（84ns），未集成到管线。

**需要实现**:
- 提取 `SyllableTrie` 为正式模块，放入 `cheime-dictionary` 或独立 `cheime-segmentor` crate
- 实现 `Segmentor` trait
- 支持功能:
  - 标准拼音音节切分
  - 零声母处理（"an" → "a"+"n"? → "an"）
  - 歧义标注（"xian" → ["xian"] 或 ["xi","an"]，两种都保留）
  - 可扩展: 后续加入 fuzzy/abbr/correction

**算法选择**: 升级为基于音节图的 BFS（参考 librime Syllabifier），不停留在贪心匹配。歧义时保留多条路径，由 Ranker 决策。

**涉及 crate**: 新建 `cheime-segmentor`

### 1.4 统一排序器 (Ranker)

**当前**: BuiltinPipeline 只在构建时按 weight 排序，运行时无排序。

**需要实现**:
- 根据 DRAFT §5.5 实现多维度排序:
  - 词频 (来自词典 weight)
  - 用户历史频率 (来自 UserStore)
  - 来源权重 (词典 > 用户词 > 在线热词 > emoji)
  - 码长偏好 (短码优先)
  - 编辑距离 (fuzzy 匹配惩罚)
  - 用户固定候选 (pinned 强制置顶)
- 排序过程输出可解释信息（诊断用）

**涉及 crate**: `cheime-pipeline` (新增 `ranker` 模块)

### 1.5 用户词 Translator

**当前**: UserStore 存在但未接入管线。

**需要实现**:
- 实现 `Translator` trait，查询 UserStore
- 合并用户词到候选列表（与词典 Translator 并行产出候选）
- 用户词上屏后触发 `UserEvent::LearnWord` / `UpdateFrequency`

**涉及 crate**: `cheime-user-data`, `cheime-pipeline`

### 1.6 命令行测试界面

**当前**: 无 CLI。

**需要实现**:
- 新建 `apps/cheime-cli` binary
- stdin 接收按键（a-z, Backspace, Enter, Space, Escape, 数字选字, -/= 翻页）
- stdout 输出: preedit + 候选列表（每页 5/9 个）+ 状态
- 用于日常开发验证和集成测试

**涉及 crate**: 新建 `apps/cheime-cli`

### 1.7 阶段 1 验收标准

```
$ echo "ni hao" | cargo run --bin cheime-cli
preedit: ni
1. 你  2. 尼  3. 泥  4. 逆  5. 拟
→ 回车选第一个: 你
commit: 你

preedit: hao
1. 好  2. 号  3. 毫  4. 耗  5. 浩
→ 输入'1': 好
commit: 好

最终输出: 你好
用户词: "你好" 的频率已更新
```

---

## 阶段 2：输入方案与配置系统

**目标**: 支持全拼和至少一种双拼方案，通过 YAML 配置文件定义方案行为。覆盖 DRAFT §22 第 3,4,9,10 条。

### 2.1 配置系统 (cheime-config)

**当前**: 无。

**需要实现**:
- 按 DRAFT §8 实现:
  - YAML 解析 + 类型校验（serde + 自定义 validator）
  - `extends` / `import` 继承链
  - `patch` 操作（set/append/prepend/remove/replace/insert_before/insert_after）
  - 引用解析（`translator@dict_name`）
  - 循环依赖检查
  - 能力声明 + 平台兼容检查
  - 生成 Schema IR
  - 原子部署（版本目录 + `current` 符号链接）
- 配置驱动 Pipeline 构建: 从配置中读取 processor/segmentor/translator/filter/ranker 列表并实例化

**涉及 crate**: 新建 `cheime-config`

### 2.2 方案抽象 (cheime-schema)

**当前**: 无。

**需要实现**:
- Schema 数据结构: 包含引擎组件列表、词典引用、拼写规则、快捷键、正字法配置
- Schema IR: 编译后的内部表示，供引擎直接消费
- 方案注册表: 管理已安装方案及其依赖

**涉及 crate**: 新建 `cheime-schema`

### 2.3 Key Mapper — 双拼支持

**当前**: KeyEvent 只接收纯字符。

**需要实现**:
- `KeyMapper` trait: `map(KeyEvent) → (char, bool)` — 将物理按键映射为逻辑字符
- 全拼 Mapper: 直接透传字母键
- 双拼 Mapper: 支持多种键位（小鹤、自然码、微软双拼、搜狗双拼、紫光双拼）
- 零声母处理: 双拼中 'o' 代表零声母
- 用户自定义键位: 从配置加载映射表

**涉及 crate**: `cheime-pipeline` (新增 `key_mapper` 模块)

### 2.4 Code Normalizer

**当前**: 拼音规范化和模糊音未实现。

**需要实现**:
- `CodeNormalizer` trait: `normalize(code: &str) → Vec<String>` — 一个编码展开为多个可能的规范形式
- 模糊音规则: zh→z, ch→c, sh→s, n→l, f→h, ang→an, eng→en, ing→in 等
- 用户自定义模糊音规则
- 展开后去重

**涉及 crate**: `cheime-pipeline` (新增 `normalizer` 模块)

### 2.5 阶段 2 验收标准

```yaml
# schema/double_pinyin_flypy.yaml
schema:
  name: 小鹤双拼
  key_mapper: flypy
  normalizer:
    fuzzy: [zh_z, ch_c, sh_s]
  engine:
    segmentor: pinyin_syllable
    translators:
      - dict_translator@luna_pinyin
      - user_dict_translator
    filters:
      - dedup_filter
      - charset_filter@simplified
    ranker: unified_ranker
```

---

## 阶段 3：用户数据持久化与学习策略

**目标**: 用户词频历史、自造词、固定候选在重启后保持。

### 3.1 SQLite 持久化

**当前**: UserStore 纯内存，重启丢失。

**需要实现**:
- 按 DRAFT §10.2 建议使用 SQLite + WAL
- 表设计:
  - `events`: event_id, timestamp, operation, schema, text, code, delta
  - `frequency`: schema, code, text, count (物化视图，加速查询)
  - `pinned`: schema, text
  - `blocked`: schema, text
- UserStore 改为 trait，提供内存实现和 SQLite 实现
- 启动时从 SQLite 恢复状态，运行时写入事件日志
- 定期 compact 事件为快照

**涉及 crate**: `cheime-user-data`

### 3.2 学习策略

- 全局学习: 所有应用共享
- 方案级学习: 每个 schema 独立计数
- 应用级学习: 按前台应用隔离（后续实现）
- 无痕模式: Session 标记 transparent → 不产生 UserEvent
- 撤销学习: 记录操作历史，支持回退
- 候选来源溯源: Ranker 输出中携带"候选 X 排第 3 是因为用户词频 +2"

**涉及 crate**: `cheime-user-data`, `cheime-session`

### 3.3 自造词

- 用户连续上屏相邻单字 → 触发词频合并 → 产生自造词
- 手动添加: 用户可以显式添加自定义词条
- 自造词参与正常排序

---

## 阶段 4：扩展功能

### 4.1 Emoji 原生支持 (cheime-emoji)

**当前**: Candidate.text 是 plain String，无 Emoji 类型。

**需要实现**:
- 按 DRAFT §14 实现 `CandidateContent` 枚举
- Emoji 数据文件: Unicode CLDR emoji 列表 + 中英文关键词
- Emoji Translator: 按关键词/拼音/英文匹配 emoji
- 输入方式: `:rocket` / 拼音输入 / 中文语义
- 最近使用 emoji 列表
- Emoji 与文本混合排序

**涉及 crate**: 新建 `cheime-emoji`

### 4.2 候选过滤器 (Filters)

**当前**: 无 filter 链。

**需要实现**:
- `DedupFilter`: 按 text 去重，保留最高分的来源
- `CharsetFilter`: 限制候选字符集（仅简体/仅繁体/仅 CJK 基本区等）
- `AnnotationFilter`: 为候选添加注音/编码注释
- `BlockFilter`: 按用户屏蔽列表过滤
- 过滤器链按配置顺序执行，每个 filter 可增删改候选

**涉及 crate**: `cheime-pipeline`

### 4.3 特殊 Translator

- `DateTranslator`: "date" / "rq" / "日期" → 2026-07-21
- `TimeTranslator`: "time" / "sj" / "时间" → 14:30:00
- `CalculatorTranslator`: "1+2*3" → 7
- `SymbolTranslator`: 特殊符号候选

**涉及 crate**: `cheime-pipeline`

### 4.4 Lua 管线接入

**当前**: LuaRuntime 实现了全部 4 个 trait，但 BuiltinPipeline 未使用 ExtensionHost。

**需要实现**:
- Pipeline 构建时从配置读取 Lua 脚本引用
- 通过 ExtensionHost 注册 Lua 组件到对应管线位置
- Lua translator/filter/processor 在 native 组件间按配置顺序执行
- 异步 Lua: 实现 `async.request` 合约，结果通过 revision 回写
- 执行保护: 超时/指令数/内存限制

**涉及 crate**: `cheime-pipeline`, `cheime-lua`

### 4.5 OpenCC 兼容过滤器

**当前**: 无。

**需要实现**:
- 作为兼容层 filter 实现（非原生正字法路径，按 DRAFT §9.7）
- 加载 OpenCC 词典（`.ocd` / `.txt`）
- 在 filter 阶段执行简繁转换
- 标记来源: "经由 OpenCC 转换"

---

## 阶段 5：异步与在线服务

**目标**: 支持异步 Translator（在线热词），实现 revision-based 结果取消。

### 5.1 异步 Translator 基础

**当前**: InputPipeline::apply 是同步的。

**需要实现**:
- `AsyncTranslator` trait: `translate_async(ctx) → JoinHandle<Result>`
- 管线执行: 先收集所有同步 translator 结果 → 生成首屏快照 → 同时启动所有异步 translator → 异步结果按 revision 校验后增量合并
- 取消机制: 新 revision 到达时，取消旧 revision 的 inflight 任务

**涉及 crate**: `cheime-pipeline`, `cheime-session`

### 5.2 在线热词 Translator

- `HotwordsClient` trait: 可替换 provider
- 首次本地候选列表先出（<1ms），热词结果异步追加
- 请求仅发送 composition 编码，不带文本上下文
- 自托管 endpoint 支持

**涉及 crate**: 新建 `cheime-services`

---

## 阶段 6：诊断与工具

### 6.1 结构化错误 (cheime-diagnostics)

**当前**: 错误类型分散在各 crate 的 thiserror 枚举中。

**需要实现**:
- 统一错误码规范: `E-{MODULE}-{CODE}` 格式
- 用户说明 + 技术原因 + 修复建议
- 错误链追踪（source chain）
- 诊断导出器: 收集版本信息、配置状态、引擎状态、性能指标

**涉及 crate**: 新建 `cheime-diagnostics`

### 6.2 集成测试 + 差分测试

**当前**: 仅有单元测试。

**需要实现**:
- 集成测试: stdin → pipeline → stdout 完整链路
- Rime 差分测试: 同一 schema + 同一词典 + 同一按键序列 → 对比 preedit/候选/排序/上屏 与 librime 的差异
- 回归测试: 捕获已知行为，防止退化

**涉及 crate**: 新建 `tests/`

---

## 总路线图

```
阶段 1 (2-3 周): 补齐基础设施
  ├─ 1.1 Session UiCommand 补全
  ├─ 1.2 Pipeline 组件化
  ├─ 1.3 拼音切分器
  ├─ 1.4 统一排序器
  ├─ 1.5 用户词 Translator
  └─ 1.6 CLI 测试界面

阶段 2 (3-4 周): 输入方案与配置
  ├─ 2.1 配置系统
  ├─ 2.2 方案抽象
  ├─ 2.3 双拼 KeyMapper
  └─ 2.4 模糊音 Normalizer

阶段 3 (2 周): 用户数据持久化
  ├─ 3.1 SQLite + trait 抽象
  ├─ 3.2 学习策略
  └─ 3.3 自造词

阶段 4 (2-3 周): 扩展功能
  ├─ 4.1 Emoji
  ├─ 4.2 过滤器链
  ├─ 4.3 特殊 Translator
  ├─ 4.4 Lua 管线接入
  └─ 4.5 OpenCC 兼容

阶段 5 (2 周): 异步与在线
  ├─ 5.1 异步 Translator
  └─ 5.2 热词服务

阶段 6 (1-2 周): 诊断与测试
  ├─ 6.1 结构化错误
  └─ 6.2 集成/差分测试
```

**优先级规则**:
- 阶段 1 是硬依赖 — 不做完后续无法推进
- 阶段 1 内的子项可以并行开发
- 阶段 2 可与阶段 1 后期重叠
- 阶段 3-5 按需调整优先级（根据 Windows TSF 接入反馈）

---

## 本期（下次对话开始）推荐切入点

**阶段 1.1 + 1.2 + 1.3**: 
1. Session 补全 UiCommand（候选选择、翻页）
2. Pipeline 从单体重构为组件链（Processor → Segmentor → Translator → Filter → Ranker）
3. 从 bench 中提取 `SyllableTrie`，实现正式的 `PinyinSegmentor`

这三个可以并行推进，完成后即可在 CLI 中完成"打字→选字→上屏"的完整流程，成为第一个可用的输入引擎。
