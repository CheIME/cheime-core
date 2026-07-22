# PIPELINE KNOWLEDGE BASE

## OVERVIEW

Highest-coupling subsystem: converts a `KeyEvent` plus composition into candidates and an intent, then exposes that deterministic result to `cheime-session`.

## WHERE TO LOOK

| Task | Location | Notes |
| --- | --- | --- |
| Change top-level contracts/order | `src/lib.rs` | `InputPipeline`, traits, `ComposablePipeline` |
| Change config assembly | `src/factory.rs` | Optional stores/indexes/mappers become trait objects |
| Key-to-composition behavior | `src/processor.rs`, `src/punctuator.rs` | Punctuator wraps the default processor |
| Pinyin segmentation | `src/segmentor.rs` | Greedy syllable segmentation |
| Fuzzy/abbreviation expansion | `src/normalizer.rs` | May multiply segment variants |
| Static/user/emoji candidates | `src/translator.rs`, `src/emoji.rs` | Translators append into one candidate list |
| Dedup and ordering | `src/filter.rs`, `src/ranker.rs` | Filters run before ranker |
| Double-pinyin input | `src/key_mapper.rs` | Mapper can emit multiple synthetic characters |
| Fast test double | `src/builtin.rs` | Inline entries, no full component chain |
| Real dictionary behavior | `tests/stress_tests.rs` | Embedded rime_ice fixture |

## EXECUTION ORDER

`ComposablePipeline::apply` first runs the optional stateful `KeyMapper`. Every emitted character is passed through `apply_internal` in order. The internal order is fixed:

```text
Processor -> Segmentor -> optional CodeNormalizer -> Translators
          -> Filters -> Ranker -> PipelineUpdate
```

`ProcessorOutput::consumed` returns early with no candidates. Processor-injected candidates are placed before translator output but still pass through filters and ranking. `PipelineIntent` is consumed by the session; changes to intent semantics require session tests.

## DEBUGGING

```sh
# Fast component/factory unit test
cargo test -p cheime-pipeline TEST_NAME -- --exact --nocapture

# Heavy integration path with real dictionary
cargo test -p cheime-pipeline --test stress_tests TEST_NAME -- --exact --nocapture
```

For candidate defects, inspect intermediate values at `apply_internal`: processor output, segments, normalized variants, each translator's output, post-filter candidates, then ranked candidates. For assembly defects, stop at `PipelineFactory::build` and check which config variants actually produce components.

## CONVENTIONS

- Component traits declare thread-safety explicitly. Stateful processor/key mapper objects are behind `parking_lot::Mutex`; read-only stages are `Send + Sync` trait objects.
- Keep candidate ordering deterministic. Weight/order changes need assertions covering ties and multiple sources.
- Use `BuiltinPipeline` for session semantics; use `ComposablePipeline` when stage ordering or factory behavior matters.
- Factory errors convert to structured diagnostics through `BuildError::to_diagnostic`.
- Add narrow unit coverage beside a component; use `stress_tests.rs` only for cross-stage real-data behavior.

## ANTI-PATTERNS

- Do not reorder stages casually; translators, deduplication, and ranking depend on the current sequence.
- Do not bypass `PipelineFactory` in CLI/runtime assembly while adding config-driven behavior.
- Do not assume every accepted config variant is implemented. Script/Lua translators are currently skipped; unsupported simplifier directions return errors; unknown filters are skipped.
- Do not use the embedded 539K-entry dictionary for a reproducer that `BuiltinPipeline` or an inline `CompiledIndex` can cover.
- Do not add blanket fallbacks for impossible internal states; errors belong at config, file, and extension boundaries.
