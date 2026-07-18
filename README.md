# CheIME Core

`cheime-core` is CheIME's platform-independent input engine. Platform frontend repositories pin this repository as a Git submodule.

The core owns protocol models, input sessions, pipelines, dictionaries, user learning, deployment state, and extension runtimes. It does not depend on TSF, COM, Win32, HWND, platform launchers, or concrete UI toolkits.

## Current milestone

The first milestone builds a deterministic in-memory vertical slice:

1. accept a versioned frontend command;
2. update one logical input session;
3. publish an immutable candidate snapshot;
4. propose a platform commit action;
5. finalize the transition only after platform confirmation.

Dictionary compilation, Lua, durable user data, transport framing, and platform frontends are delivered in later milestones.

## Quality gate

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
