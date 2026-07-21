## CRITICAL

### C1: `generation` silently discarded in `build_tiered` — index.rs:150

```rust
let tiered = TieredIndex::new(code_entries, tidx_path, hot_entries_per_code, source_hash)?;
// Store generation in some way if needed...
let _ = generation;   // ← silently dropped
```

`TieredIndex` has no `generation` field, so `CompiledIndex::generation()` returns `None` for tiered indices. `DeploymentHandle::generation()` (deploy.rs:18) then falls back to `DeploymentGeneration::new(0)`, meaning **all tiered deployments report generation 0** regardless of the actual generation passed in. This silently breaks deployment tracking / versioning — any code that compares generations or uses them for cache invalidation will be incorrect.

**Fix:** Add `generation: DeploymentGeneration` to `TieredIndex` and wire it through.

---

### C2: `query_prefix` under-fetches from cold tier — tiered.rs:144

```rust
let cold = self.cold.query_prefix(prefix, limit.saturating_sub(all.len()));
```

When hot entries count toward `all.len()`, the cold tier is asked for fewer entries. After dedup (the same text can exist in both hot and cold), the final result can have **fewer entries than `limit`** even though more qualifying entries exist in the data.

Scenario: limit=5, hot has 5 entries {A,B,C,D,E}. Cold is asked for `limit - 5 = 0` entries. But cold may also have entries {F,G,H} with lower weights that should appear in the result — they're never fetched.

**Fix:** Always request the full `limit` from cold:

```rust
let cold = self.cold.query_prefix(prefix, limit);
```

---

## IMPORTANT

### I1: `annotation` behavioral difference — Memory vs Tiered — tiered.rs:107 vs index.rs:68

`MemoryIndex::query()` sets `annotation: Some(code.to_owned())` (the pinyin code as annotation), while `TieredIndex::query()` sets `annotation: None`. Downstream code (pipeline, wire codec, UI) reads this field for pinyin hints. The integration test only compares `.text`, so this mismatch is invisible to tests but observable at runtime.

For `query_prefix` this is harder to solve (entries come from multiple codes). But for exact `query()` the code is known:

```rust
// tiered.rs query(), line ~107
annotation: None,  // ← should be Some(code.to_owned()) for exact matches
```

---

### I2: Type narrowing `i64` → `i32` for weights — tiered.rs:17 vs body.rs:10

`DictEntry.weight` is `Option<i64>`, `HotEntry.weight` is `i32`, and `TidexReader` stores `i32`. The integration test's `group_entries` does `e.weight.unwrap_or(1) as i32` — a silent truncation. If any dictionary weight exceeds `i32::MAX` (~2.1 billion), data corruption occurs silently. The `.tidx` format likely uses `i32` for compactness, but there's no assertion or validation at the boundary.

---

### I3: O(hot × cold) dedup in `TieredIndex::query()` — tiered.rs:100

```rust
if !hot_entries.iter().any(|he| he.text == text) {
```

For each cold entry, linearly scans all hot entries comparing strings. With typical small hot sizes (5–20), this is fine. But the pattern doesn't scale, and `query_prefix` already uses a `HashSet` for the same purpose — inconsistency in approach.

---

## MINOR

### M1: `PartialEq` for `CompiledIndex::Tiered` is pointer identity — index.rs:126

```rust
(CompiledIndex::Tiered(a), CompiledIndex::Tiered(b)) => Arc::ptr_eq(a, b),
```

Two independently-built `TieredIndex` instances with identical content compare as unequal. Documented in comment and unavoidable given `Mmap` lacks `PartialEq`, but callers should be aware.

### M2: No unit tests for tiered variant in index.rs — index.rs:207–273

All tiered tests live in the integration test file, gated behind a real `.tidx` file. The `index.rs` tests only cover `MemoryIndex`. Missing coverage:
- `CompiledIndex::generation()` returning `None` for tiered
- `build_tiered` parameter handling (e.g., generation discarding)

### M3: `hot_entries_per_code` stored but unused at query time — tiered.rs:42

The field is stored in the struct but only used during construction (`.take(hot_entries_per_code)`). Dead weight at runtime.
