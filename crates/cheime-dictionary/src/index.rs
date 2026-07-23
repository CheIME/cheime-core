#![forbid(unsafe_code)]

use crate::body::DictEntry;
use cheime_model::{Candidate, CandidateId, DeploymentGeneration};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::tiered::TieredIndex;

// ---------------------------------------------------------------------------
// MemoryIndex — the original full-in-memory index
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryIndex {
    pub generation: DeploymentGeneration,
    pub source_hash: String,
    pub total_entries: usize,
    pub(crate) entries: BTreeMap<String, Vec<(String, Option<i64>)>>,
}

impl MemoryIndex {
    pub fn build(entries: Vec<DictEntry>, generation: DeploymentGeneration) -> Self {
        let mut grouped: BTreeMap<String, Vec<(String, Option<i64>)>> = BTreeMap::new();
        let mut hash_state = String::new();

        for entry in &entries {
            hash_state.push_str(&entry.text);
            hash_state.push('\t');
            hash_state.push_str(&entry.code);
            hash_state.push('\t');
            if let Some(w) = entry.weight {
                hash_state.push_str(&w.to_string());
            }
            hash_state.push('\n');

            grouped
                .entry(entry.code.clone())
                .or_default()
                .push((entry.text.clone(), entry.weight));
        }

        for group in grouped.values_mut() {
            group.sort_by(|a, b| {
                b.1.unwrap_or(0)
                    .cmp(&a.1.unwrap_or(0))
                    .then_with(|| a.0.cmp(&b.0))
            });
        }

        let mut hasher = Sha256::new();
        hasher.update(hash_state.as_bytes());
        let source_hash = format!("{:x}", hasher.finalize());

        Self {
            generation,
            source_hash,
            total_entries: entries.len(),
            entries: grouped,
        }
    }

    /// Construct from a cache fragment (avoids re-sorting).
    pub(crate) fn from_fragment(
        generation: DeploymentGeneration,
        source_hash: String,
        total_entries: usize,
        entries: BTreeMap<String, Vec<(String, Option<i64>)>>,
    ) -> Self {
        Self {
            generation,
            source_hash,
            total_entries,
            entries,
        }
    }

    /// Exact code lookup (single key).
    pub fn query(&self, code: &str) -> Vec<Candidate> {
        let hash8 = self.source_hash.chars().take(8).collect::<String>();
        self.entries
            .get(code)
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(idx, (text, _))| Candidate {
                id: CandidateId::new(idx as u64 + 1),
                text: text.clone(),
                annotation: Some(code.to_owned()),
                source: format!("dict:{hash8}"),
                is_emoji: false,
            })
            .collect()
    }

    /// Prefix search: all entries whose code starts with `prefix`.
    /// Returns up to `limit` candidates, sorted by weight descending.
    pub fn query_prefix(&self, prefix: &str, limit: usize) -> Vec<Candidate> {
        use std::collections::BinaryHeap;
        let end = format!("{prefix}\u{10FFFF}");
        let range = self.entries.range(prefix.to_string()..=end);

        let mut heap: BinaryHeap<(i64, String, String)> = BinaryHeap::new();

        for (code, entries) in range {
            for (text, weight) in entries {
                let w = weight.unwrap_or(1);
                heap.push((w, text.clone(), code.clone()));
                if heap.len() > limit * 2 {
                    let mut drained: Vec<_> = heap.drain().collect();
                    drained.sort_by(|left, right| {
                        right
                            .0
                            .cmp(&left.0)
                            .then_with(|| left.1.cmp(&right.1))
                            .then_with(|| left.2.cmp(&right.2))
                    });
                    drained.truncate(limit);
                    heap = drained.into_iter().collect();
                }
            }
        }

        let mut results: Vec<_> = heap.into_iter().collect();
        results.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| left.1.cmp(&right.1))
                .then_with(|| left.2.cmp(&right.2))
        });
        results.truncate(limit);

        let hash8 = self.source_hash.chars().take(8).collect::<String>();
        results
            .into_iter()
            .enumerate()
            .map(|(idx, (_w, text, code))| Candidate {
                id: CandidateId::new(idx as u64 + 1),
                text,
                annotation: (!code.is_empty()).then_some(code),
                source: format!("dict:{hash8}"),
                is_emoji: false,
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// CompiledIndex — enum over memory / tiered modes
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum CompiledIndex {
    Memory(Box<MemoryIndex>),
    Tiered(Arc<TieredIndex>),
}

// Manual impls because TieredIndex contains Mmap (no PartialEq).
impl PartialEq for CompiledIndex {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (CompiledIndex::Memory(a), CompiledIndex::Memory(b)) => a == b,
            (CompiledIndex::Tiered(a), CompiledIndex::Tiered(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl Eq for CompiledIndex {}

impl CompiledIndex {
    /// Build a full memory resident index (default mode).
    pub fn build(entries: Vec<DictEntry>, generation: DeploymentGeneration) -> Self {
        CompiledIndex::Memory(Box::new(MemoryIndex::build(entries, generation)))
    }

    /// Build a tiered index from pre-sorted code entries + cold .tidx file.
    pub fn build_tiered(
        code_entries: Vec<(String, Vec<(String, i32)>)>,
        tidx_path: &std::path::Path,
        hot_entries_per_code: usize,
        source_hash: String,
        generation: DeploymentGeneration,
    ) -> Result<Self, crate::tiered::TidexBuildError> {
        let tiered = TieredIndex::new(
            code_entries,
            tidx_path,
            hot_entries_per_code,
            source_hash,
            generation,
        )?;
        Ok(CompiledIndex::Tiered(Arc::new(tiered)))
    }

    pub fn generation(&self) -> Option<&DeploymentGeneration> {
        match self {
            CompiledIndex::Memory(m) => Some(&m.generation),
            CompiledIndex::Tiered(t) => Some(&t.generation),
        }
    }

    pub fn source_hash(&self) -> &str {
        match self {
            CompiledIndex::Memory(m) => &m.source_hash,
            CompiledIndex::Tiered(t) => &t.source_hash,
        }
    }

    pub fn total_entries(&self) -> usize {
        match self {
            CompiledIndex::Memory(m) => m.total_entries,
            CompiledIndex::Tiered(t) => t.total_entries,
        }
    }

    /// Exact code lookup.
    pub fn query(&self, code: &str) -> Vec<Candidate> {
        match self {
            CompiledIndex::Memory(m) => m.query(code),
            CompiledIndex::Tiered(t) => t.query(code),
        }
    }

    /// Prefix search.
    pub fn query_prefix(&self, prefix: &str, limit: usize) -> Vec<Candidate> {
        match self {
            CompiledIndex::Memory(m) => m.query_prefix(prefix, limit),
            CompiledIndex::Tiered(t) => t.query_prefix(prefix, limit),
        }
    }
}

// ---------------------------------------------------------------------------
// From-fragment for cache layer — used only for memory mode
// ---------------------------------------------------------------------------

impl MemoryIndex {
    pub(crate) fn into_compiled(self) -> CompiledIndex {
        CompiledIndex::Memory(Box::new(self))
    }
}

#[cfg(test)]
#[path = "index_tests.rs"]
mod tests;
