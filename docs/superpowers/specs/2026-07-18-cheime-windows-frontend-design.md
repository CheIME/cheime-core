# CheIME Windows Frontend Design

> 日期：2026-07-18
> 状态：已确认
> 适用范围：Windows TSF TIP、Engine Host、安装注册、候选窗口渲染

## 1. 定位

Windows 前端仓库 `cheime-win` 包含 TSF TIP、engine host/launcher、HWND 候选窗渲染、安装注册和平台集成测试。它通过固定 commit 的 Git submodule 引入 `cheime-core`，并只通过 `cheime-wire` / `cheime-protocol` 调用核心。

`cheime-core` 不包含本文的任何代码（没有 TSF、COM、Win32、HWND 依赖）。

## 2. 仓库结构

```
cheime-win/
├── .gitmodules                  ← submodule 指向 cheime-core 固定 commit
├── cheime-core/                 ← Git submodule
├── Cargo.toml                   ← 独立 workspace
├── crates/
│   ├── cheime-tip/              ← TSF TIP DLL (x64 + x86)
│   ├── cheime-engine-host/      ← engine-host.exe (x64)
│   ├── cheime-installer/        ← setup/register/unregister tool
│   └── cheime-tip-core/         ← 候选窗渲染、通道调度、平台动作应用
└── tests/
    └── integration/             ← 平台验收矩阵测试
```

依赖方向：
```
cheime-core (submodule)
    ↑
cheime-tip-core  ←  通道、渲染、平台动作
    ↑
cheime-tip       ←  COM/TSF 适配、DllRegisterServer
cheime-engine-host ← Named pipe、Session Actor
cheime-installer ←  注册/卸载
```

## 3. 技术栈

| 组件 | 语言/工具 |
|------|-----------|
| TSF TIP DLL | Rust + `windows` crate，x64 + x86 |
| Engie host | Rust，x64 |
| 候选窗渲染 | GDI (`ExtTextOutW`, `FillRect`) |
| IPC | Named Pipes + `cheime-wire` (长度前缀 + MessagePack) |
| 安装/注册 | `cheime-installer.exe` 调用 `DllRegisterServer` |
| 构建 | Cargo workspace + `.cargo/config.toml` 多目标 |

## 4. TSF TIP DLL (`cheime-tip`)

### 4.1 COM 注册

TIP DLL 实现标准 COM 自注册：

```rust
// 导出自 DllRegisterServer / DllUnregisterServer
pub unsafe extern "stdcall" fn DllRegisterServer() -> HRESULT { ... }
pub unsafe extern "stdcall" fn DllUnregisterServer() -> HRESULT { ... }
pub unsafe extern "stdcall" fn DllGetClassObject(rclsid: REFCLSID, riid: REFIID, ppv: *mut *mut c_void) -> HRESULT { ... }
pub unsafe extern "stdcall" fn DllCanUnloadNow() -> HRESULT { ... }
```

`DllRegisterServer` 写入：
- `HKEY_CURRENT_USER\Software\Classes\CLSID\{CLSID}`
- `InprocServer32` → TIP DLL 完整路径
- `ThreadingModel` → `"Apartment"`

`DllUnregisterServer` 删除上述键。

TIP 不使用 `InprocServer32` 全局注册（避免影响所有用户），只注册到 `HKCU`。

### 4.2 实现的 COM 接口

```rust
// TSF 核心
ITfTextInputProcessorEx   // ActivateEx, Deactivate
ITfKeyEventSink           // OnTestKeyDown, OnKeyDown(预留), OnKeyUp(预留)
ITfCompositionSink        // OnCompositionTerminated
ITfEditSession            // 平台动作应用编辑会话
ITfDisplayAttributeProvider // preedit 下划线等展示属性
ITfThreadMgrEventSink     // 焦点/线程变化通知

// COM 基础
IClassFactory              // CreateInstance, LockServer
```

### 4.3 线程模型

```
TSF UI 线程 (宿主 App)
├── TIP 实例
├── HWND 候选窗口 (WS_POPUP + WS_EX_NOACTIVATE)
├── OnTestKeyDown / OnKeyDown → push KeyCommand 到 mpsc Sender
├── WindowProc → 处理 WM_USER_SNAPSHOT（渲染候选）
├── WindowProc → 处理 WM_USER_ACTION（应用平台动作）
└── WindowProc → 处理 WM_USER_STATUS（连接状态）

专用 I/O 线程
├── 从 mpsc 接收 FrontendMessage → 写 pipe
├── 读 pipe → EngineMessage 分发
│   ├── CandidateSnapshot → PostMessage(WM_USER_SNAPSHOT)
│   ├── PlatformAction   → PostMessage(WM_USER_ACTION)
│   └── SessionStatus    → PostMessage(WM_USER_STATUS)
└── 握手管理（ServerHello/ClientHello）
```

### 4.4 本地按键准入

`OnTestKeyDown` 无副作用的判定规则：
- CheIME 未激活 → 不处理
- 英文模式 → 只处理快捷键（Shift/Ctrl+Space 切换到中文）
- 中文模式 → 处理 a-z, Backspace, Enter, Escape, Space, 数字 1-9, +/-/PgUp/PgDn/Up/Down

`OnKeyDown` 复用同一判定 token。按键命令入队后立即返回，不等待引擎。

### 4.5 编辑会话（PlatformAction 应用）

`WM_USER_ACTION` 处理：
1. 读取 `PlatformAction`（action_id, epoch, revision, kind）
2. 请求 TSF 异步编辑会话（`RequestEditSession`）
3. 在编辑会话中：
   - `SetPreedit` → `ITfComposition` + `ITfRange` 设置文本和光标
   - `Commit` → `ITfComposition` 结束+提交文本，释放 composition
   - `CancelComposition` → 结束 composition 不提交
4. 完成后通过 channel 发送 `PlatformActionResult { action_id, Applied }`
5. 失败发送 `PlatformActionResult { action_id, Rejected { reason } }`

## 5. Engine Host (`cheime-engine-host`)

### 5.1 进程模型

`cheime-engine.exe` 是单一用户级进程：
- 首次 TIP 连接时由 `cheime-tip` 通过 `CreateProcess` 启动
- 通过命名管道互斥实现单例（`CreateNamedPipe("cheime-engine")` 失败 = 已有实例运行）
- 进程退出时所有客户端连接断开

### 5.2 架构

```
Engine Host Process
├── 连接监听器 — 监听管道 \\.\pipe\cheime-engine
├── 每客户端:
│   ├── 专用管道 \\.\pipe\cheime-engine.{client_id}
│   ├── 握手 → HelloAck / HelloRejected
│   ├── FramedReader / FramedWriter
│   ├── Session Actor (cheime-session)
│   ├── Pipeline (cheime-pipeline + BuiltinPipeline)
│   ├── Dictionary (cheime-dictionary)
│   ├── User Data (cheime-user-data)
│   └── Extension Host (cheime-extension + cheime-lua)
├── Deployment Manager (词典/配置部署)
└── 诊断日志（最小披露原则）
```

### 5.3 连接生命周期

```
TIP 连接 → ServerHello → ClientHello 等待(5s超时)
  ├── version匹配 → HelloAck + 专用管道创建 → 消息流
  └── version不匹配 → HelloRejected + 断开
```

- 专用管道断开 → 该 client 所有 session 清理
- 引擎退出 → 所有连接断开
- 重连 → 新握手、新 client_instance_id，不恢复旧 session

## 6. 候选窗口渲染

### 6.1 窗口属性

```rust
// 窗口样式
WS_POPUP                    // 无边框弹出窗口
WS_EX_NOACTIVATE            // 不抢夺输入焦点
WS_EX_TOPMOST               // 显示在最上层
WS_EX_LAYERED               // 支持透明度（可选圆角）
WS_EX_TOOLWINDOW            // 不显示在任务栏
```

### 6.2 渲染

纯 GDI 渲染，不使用 Direct2D/DirectWrite：

- **背景**: `FillRect` + `COLOR_WINDOW` 画刷
- **Preedit 行**: `ExtTextOutW` 画 preedit 文字 + 下划线（display attribute）
- **候选列表**: 每行 `TextOutW` 画 `序号. 候选文本 [注释]`
- **高亮**: `FillRect` + `COLOR_HIGHLIGHT` 画刷覆盖当前选中候选
- **分页指示**: 底部 `TextOutW` 画 `当前页/总页数`

### 6.3 布局

- 字号: `SystemParametersInfo(SPI_GETNONCLIENTMETRICS)` 获取 `lfMessageFont`
- 行高: `TEXTMETRIC.tmHeight + tmExternalLeading` + 行间距(2px)
- 宽度: 候选文本最大宽度 + 序号宽度(24px) + 左右 padding(8px)
- 位置: 跟随 TSF composition 的 anchor（`ITfContextView::GetTextExt` 或光标位置）
- DPI: 响应 `WM_DPICHANGED` 重新计算布局

### 6.4 消息处理

```rust
WM_PAINT        → 重绘候选窗口
WM_USER_SNAPSHOT → 新 CandidateSnapshot → 更新候选列表 → InvalidateRect
WM_USER_STATUS  → 连接/引擎状态改变 → 更新状态指示
WM_DPICHANGED   → 重新计算字号和布局
WM_DESTROY      → 清理 GDI 资源
```

### 6.5 交互

- 鼠标左键点击候选 → 生成 `UiCommand::SelectCandidate { epoch, snapshot_revision, candidate_id }`
- 键盘数字 1-9 → 同候选选择
- 点击窗口外部 → Dismiss（由 `WM_ACTIVATE` 检测失活处理）

## 7. 安装与注册

### 7.1 cheime-installer.exe

独立的命令行工具：

```text
cheime-installer.exe install    ← DllRegisterServer + TSF profile 注册
cheime-installer.exe uninstall  ← TSF profile 注销 + DllUnregisterServer
cheime-installer.exe status     ← 检查注册状态
```

### 7.2 TSF Profile 注册

通过 COM `ITfInputProcessorProfileMgr` 接口注册：

- Profile GUID
- 语言: `0x0804` (zh-CN)
- 显示名称: "CheIME" (中文) / "CheIME Chinese Input" (英文)
- 图标: TIP DLL 内嵌资源
- 类别: `GUID_TFCAT_TIP_KEYBOARD`

### 7.3 文件布局

```text
%LOCALAPPDATA%\CheIME\
├── bin\
│   ├── cheime-engine.exe       ← x64 engine host
│   ├── cheime-tip-x64.dll      ← 64-bit TIP
│   └── cheime-tip-x86.dll      ← 32-bit TIP
├── data\
│   ├── dicts\                  ← 词典源文件和编译索引
│   ├── lua\                    ← Lua 脚本
│   └── user\                   ← 用户数据
└── config\
    └── cheime.yaml             ← 配置文件
```

安装时注册 64-bit TIP 到 `HKCU\Software\Classes\CLSID\...` 和 32-bit TIP 到 `HKCU\Software\Classes\WOW6432Node\CLSID\...`。

## 8. 构建

### 8.1 目标矩阵

| 目标 | 组件 |
|------|------|
| `x86_64-pc-windows-msvc` | engine-host.exe, cheime-tip-x64.dll, cheime-installer.exe |
| `i686-pc-windows-msvc` | cheime-tip-x86.dll |

### 8.2 构建脚本

```powershell
# 完整构建
cargo build --release --target x86_64-pc-windows-msvc
cargo build --release --target i686-pc-windows-msvc -p cheime-tip
```

`cheime-tip` 导出 `.def` 文件确保 `DllRegisterServer` 等符号正确导出。

## 9. 不在本设计的范围

以下推迟到后续 ADR：
- ARM64 TIP
- 外部 UI 进程
- DPI 缩放动画和过渡效果
- Direct2D/DirectWrite 渲染升级
- MSI/商店分发
- 在线更新
- 崩溃报告收集
- 用户设置 GUI
