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

## Crate boundaries

| Crate | Responsibility |
| --- | --- |
| `cheime-model` | Platform-neutral identities, commands, candidates, snapshots, and platform-action values |
| `cheime-protocol` | Versioned frontend/engine message families |
| `cheime-pipeline` | Language-neutral processing interface and deterministic built-in test pipeline |
| `cheime-session` | Single-writer session, revision checks, snapshots, and confirmed platform actions |

The dependency direction is `model <- protocol/pipeline <- session`. Platform frontends consume the public protocol; they never access session internals.
