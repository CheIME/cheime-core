# SESSION KNOWLEDGE BASE

## OVERVIEW

Single-writer protocol state machine. It validates message identity/order, invokes `InputPipeline`, owns pagination and pending platform actions, and publishes immutable snapshots.

## WHERE TO LOOK

| Task | Location | Notes |
| --- | --- | --- |
| Message dispatch and validation | `src/state.rs:62` | `handle`, then `validate_header` |
| Key-to-pipeline transition | `src/state.rs:116` | Applies the injected pipeline |
| Candidate UI commands | `src/state.rs:151` | Selection, highlight, paging, dismiss |
| Commit/cancel lifecycle | `src/state.rs:236` | Proposes platform actions; does not apply them |
| Applied/rejected result | `src/state.rs:291` | Finalizes pending effects |
| Snapshot/header creation | `src/state.rs:326` | Output is cloned immutable state |
| Narrow state tests | `src/state.rs` test module | Header, pagination, action edge cases |
| Minimal end-to-end test | `tests/vertical_slice.rs` | Session + inline `BuiltinPipeline` |

## STATE INVARIANTS

- `epoch` identifies the session incarnation. Reject stale epochs.
- Non-result command sequences must increase monotonically. Platform action results bypass sequence/revision validation because the frontend assigns their sequence.
- `revision` changes when composition state changes, including confirmed clear; guard overflow.
- `candidates` stores the full set; snapshots expose only the current page. Highlight indexes the full set.
- Commit/cancel methods create a pending `PlatformAction`. Composition is cleared only when an `Applied` result resolves a `ClearComposition` effect.
- A rejected action removes its pending effect but preserves composition.

## PURE CORE DEBUGGING

```sh
cargo test -p cheime-session --test vertical_slice frontend_commands_produce_confirmed_commit -- --exact --nocapture
cargo test -p cheime-session TEST_NAME -- --exact --nocapture
```

The vertical slice is the canonical frontend-free reproducer: an inline `BuiltinPipeline`, `n`, `i`, Enter, and explicit platform confirmation. Extend this pattern for protocol/session defects before involving the CLI or real dictionary.

Debug in this order: `handle` input/header, `validate_header`, `handle_key` pipeline update, `propose_commit` pending action, then `handle_action_result`. Inspect both emitted messages and retained internal composition after each step.

## CONVENTIONS

- Keep `Session<P: InputPipeline>` generic so tests can inject deterministic pipelines.
- Build test headers explicitly; sequence and revision values are part of the behavior under test.
- Assert message variants plus snapshot revision/status, not only candidate text.
- Use candidate IDs for selection; never infer identity from display text or page position.
- Page size defaults to 9 and pagination uses the full candidate list.

## ANTI-PATTERNS

- Do not clear composition when proposing a commit. Wait for `PlatformActionOutcome::Applied`.
- Do not collapse epoch, sequence, and revision into one freshness check; they protect different contracts.
- Do not expose mutable candidate/session internals to a frontend. Publish `EngineMessage` snapshots and actions.
- Do not debug session semantics through platform UI first; reproduce with `vertical_slice.rs` or a focused state test.
- Do not change a `PipelineIntent` without updating the exhaustive session match and lifecycle tests.
