# PROJECT KNOWLEDGE BASE

**Generated:** 2026-07-22
**Commit:** 0ad7ad6
**Branch:** main

## OVERVIEW

CheIME Core is a platform-independent Rust input engine. Frontends live in separate repositories; this repository owns protocol values, session state, input pipelines, dictionaries, user learning, configuration, diagnostics, extensions, and the CLI core harness.

Rust 1.85.0, edition 2024, Cargo resolver 3, MPL-2.0. Source centrality is unmeasured: this workspace has no codegraph index and rust-analyzer did not respond during generation.

## STRUCTURE

```text
apps/cheime-cli/          # Executable core harness: interactive and JSON-lines modes
crates/cheime-model/      # Platform-neutral values and IDs; foundation crate
crates/cheime-protocol/   # FrontendMessage / EngineMessage contracts
crates/cheime-pipeline/   # Component traits, implementations, and factory assembly
crates/cheime-session/    # Versioned single-session state machine
crates/cheime-config/     # Typed YAML, layered config, merge, atomic deploy
crates/cheime-dictionary/ # Rime dictionary parsing, indexes, cache, deployments
crates/cheime-user-data/  # SQLite-backed learning and user candidates
crates/cheime-tidx/       # mmap-backed binary cold index; designated unsafe boundary
crates/cheime-extension/  # Extension traits and host
crates/cheime-lua/        # Lua runtime; not fully wired into PipelineFactory
crates/cheime-diagnostics/ # Structured E-{DOMAIN}-{KIND} diagnostics
crates/cheime-wire/       # MessagePack framing/handshake; outside the workspace
config/schemas/           # base, quanpin, and flypy schema fixtures
data/                     # Large dictionaries and OpenCC conversion data
docs/                     # Architecture and subsystem references; some milestone text is stale
```

## WHERE TO LOOK

| Task | Location | Notes |
| --- | --- | --- |
| Trace a key end-to-end | `crates/cheime-session/src/state.rs` | `Session::handle` validates and dispatches messages |
| Change pipeline order or contracts | `crates/cheime-pipeline/src/lib.rs` | `InputPipeline`, component traits, `apply_internal` |
| Change component assembly | `crates/cheime-pipeline/src/factory.rs` | Config-to-runtime boundary |
| Change shared wire values | `crates/cheime-model/src/lib.rs`, `crates/cheime-protocol/src/lib.rs` | Keep platform-neutral |
| Change YAML shape | `crates/cheime-config/src/schema.rs` | Strict serde schemas |
| Change config inheritance/deploy | `crates/cheime-config/src/merge.rs`, `deploy.rs` | Extends merge and atomic promotion |
| Change lookup/cache behavior | `crates/cheime-dictionary/src/index.rs`, `cache.rs`, `tiered.rs` | Memory and mmap tiers differ |
| Change learning persistence | `crates/cheime-user-data/src/event.rs` | Event/cache/SQLite logic share one file |
| Exercise the assembled core | `apps/cheime-cli/src/main.rs` | No platform frontend required |

## CODE MAP

| Symbol | Type | Location | Refs | Role |
| --- | --- | --- | --- | --- |
| `Session::handle` | method | `crates/cheime-session/src/state.rs:62` | unmeasured | Core message/state transition boundary |
| `InputPipeline::apply` | trait method | `crates/cheime-pipeline/src/lib.rs:44` | unmeasured | Session-to-pipeline contract |
| `ComposablePipeline::apply_internal` | method | `crates/cheime-pipeline/src/lib.rs:158` | unmeasured | Processor-to-ranker execution chain |
| `PipelineFactory::build` | method | `crates/cheime-pipeline/src/factory.rs:26` | unmeasured | Typed config to component graph |
| `DiagnosticError` | struct | `crates/cheime-diagnostics/src/lib.rs:64` | unmeasured | Structured diagnostics value |
| `main` | function | `apps/cheime-cli/src/main.rs:22` | entry | Core-only executable harness |

## PURE CORE DEBUGGING

Start with the smallest layer that reproduces the issue; add the real dictionary or CLI only when needed.

```sh
# Session + BuiltinPipeline; no UI, filesystem, user DB, or real dictionary
cargo test -p cheime-session --test vertical_slice frontend_commands_produce_confirmed_commit -- --exact --nocapture

# One pipeline unit test; substitute the exact test name
cargo test -p cheime-pipeline TEST_NAME -- --exact --nocapture

# Full component graph with embedded rime_ice dictionary
cargo test -p cheime-pipeline --test stress_tests TEST_NAME -- --exact --nocapture

# Black-box core process: one KeyEvent JSON object per stdin line
cargo run -p cheime-cli -- --json
```

The vertical slice sends `n`, `i`, Enter, then `PlatformActionResult::Applied`; it verifies that composition is retained while commit is pending and cleared only after confirmation. Prefer JSON mode over the interactive CLI for deterministic replay. JSON key state includes all of `shift`, `control`, and `alt`. Set `CHEIME_DATA_DIR` to a temporary directory when isolating CLI cache/user-data effects.

Useful breakpoints: `Session::handle`, `Session::handle_key`, `ComposablePipeline::apply_internal`, `PipelineFactory::build`, `PinyinSegmentor::segment`, `DictTranslator::translate`, `MemoryIndex::query`, `Session::propose_commit`, and `Session::handle_action_result`. There is no tracing framework or checked-in debugger launch configuration; `DiagnosticError` is error reporting, not runtime tracing.

## CONVENTIONS

- Crate manifests inherit version, edition, MSRV, license, and repository from the workspace.
- Public surface is re-exported from each crate's `lib.rs`; keep internal modules private unless consumers need them.
- Config structs reject unknown keys with `serde(deny_unknown_fields)`; do not emulate Rime's silent acceptance.
- Config engine lists prepend child entries; switches replace; speller/menu fields overlay. Preserve these distinct merge semantics.
- Diagnostic codes use `E-{DOMAIN}-{KIND}` and typed errors use `thiserror`.
- Most crates use `#![forbid(unsafe_code)]`. Only `cheime-tidx` owns mmap/format unsafe code.
- Tests are both inline unit tests and `tests/` integration tests. Use behavior-oriented names and inline fixtures for narrow cases.
- Local crate-level Clippy allowances are targeted exceptions, not workspace policy.

## ANTI-PATTERNS

- Do not add TSF, COM, Win32, HWND, launcher, socket, file-handle, or UI toolkit types to core APIs.
- Do not mutate commit state before an `Applied` platform result; commit is a two-phase transition.
- Do not write engine data into `user/`; user/sync/settings tools own it. Engine-maintained persistence belongs under `state/` or cache paths.
- Do not edit deployment `current.txt` in place; deployment writes `current.tmp` then renames atomically.
- Do not add unsafe code outside `cheime-tidx`.
- Do not treat plans or milestone docs as proof of implementation; confirm current manifests and source.
- Do not assume `--workspace` covers `crates/cheime-wire`; it is not listed in root workspace members.
- Do not manually edit `Cargo.lock`.

## COMMANDS

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p cheime-cli
cargo run -p cheime-cli -- --json
cargo test --manifest-path crates/cheime-wire/Cargo.toml
```

Target `cheime-wire` through its manifest when needed because package selection from the root workspace does not include it.

## NOTES

- Root `Cargo.toml` explicitly lists 11 member paths. Cargo metadata resolves 12 workspace packages because path-dependent `cheime-session` is auto-enrolled; `cheime-wire` is neither a member nor used by the CLI.
- Workspace-wide gates cover `cheime-session` according to Cargo metadata, but not `cheime-wire`; wire needs manifest-path invocation.
- Large dictionary tests embed `data/dicts/rime_ice_base.dict.yaml`; expect slower compile/test cycles than `BuiltinPipeline` tests.
- Current dirty source did not compile during generation: `cheime-config/src/lib.rs` re-exports missing `schema::AbbreviationConfig`. Treat this as an existing worktree blocker, not a canonical project state.
