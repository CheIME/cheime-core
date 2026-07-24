# CheIME CLI 快速上手

`cheime-cli` 是用于人工体验 core 输入流程的交互式终端 demo。JSON
协议解析与输出由 engine host 负责，不属于 CLI。

## 编译

需要 Rust 1.85 或更高版本：

```bash
cargo build -p cheime-cli
```

## 启动

CLI 不再内嵌词典。启动时必须通过 `--dict` 指定一个词典目录：

```bash
cargo run -p cheime-cli -- \
  --dict data/dicts
```

Windows PowerShell：

```powershell
cargo run -p cheime-cli -- `
  --dict .\data\dicts
```

CLI 会合并目录第一层所有以 `.dict.yaml` 结尾的文件，忽略其他文件和
子目录。目录不存在、不可读取、没有匹配文件，或任一字典解析失败时，
CLI 会在进入 raw terminal 前退出并显示错误。

## 日志

默认日志路径：

```text
%LOCALAPPDATA%\cheime\logs\cheime-cli.log
```

设置 `CHEIME_DATA_DIR` 后，默认路径变为：

```text
<CHEIME_DATA_DIR>/logs/cheime-cli.log
```

可用 `--log` 指定其他路径：

```powershell
cargo run -p cheime-cli -- `
  --dict .\data\dicts `
  --log .\tmp\cheime-demo.log
```

日志以普通文本追加写入，记录 demo 会话、发送给 core 的消息、core
输出及错误；CLI 不建立第二套 JSON 协议格式。

## 按键

| 按键 | 功能 |
| --- | --- |
| `a`–`z` | 输入拼音 |
| `1`–`9` | 选择当前页候选 |
| `Up` / `Down` | 移动高亮候选 |
| `PageUp` / `PageDown` | 翻页 |
| `Space` / `Enter` | 提交候选 |
| `Backspace` | 删除组合区字符 |
| `Escape` | 取消当前组合 |
| `Ctrl+C` | 退出 demo |

未处于组合状态时，可以在 demo 文档中移动光标并使用
`Backspace`、`Delete`、`Home`、`End`。

## 用户数据

用户学习数据库仍写入：

```text
<CHEIME_DATA_DIR>/cheime_cli_user.db
```

未设置 `CHEIME_DATA_DIR` 时，Windows 默认使用
`%LOCALAPPDATA%\cheime`，其他平台使用当前目录下的 `cheime`。
