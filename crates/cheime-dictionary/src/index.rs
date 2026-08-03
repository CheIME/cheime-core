#![forbid(unsafe_code)]

use crate::body::DictEntry;
use cheime_model::{Candidate, CandidateId, DeploymentGeneration};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BinaryHeap, HashMap};
use std::ops::Bound::{Excluded, Included, Unbounded};
use std::sync::Arc;

use crate::tiered::TieredIndex;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LexiconEntry {
    pub text: String,
    pub code: String,
    pub weight: i64,
    pub source: String,
    pub completion: bool,
}

impl LexiconEntry {
    fn into_candidate(self, id: u64) -> Candidate {
        Candidate {
            id: CandidateId::new(id),
            text: self.text,
            annotation: Some(self.code),
            source: self.source,
            is_emoji: false,
        }
    }
}

fn normalize_groups(entries: &mut BTreeMap<String, Vec<(String, Option<i64>)>>) {
    for group in entries.values_mut() {
        let mut unique = HashMap::<String, Option<i64>>::with_capacity(group.len());
        for (text, weight) in std::mem::take(group) {
            unique
                .entry(text)
                .and_modify(|existing| {
                    if weight.unwrap_or(0) > existing.unwrap_or(0) {
                        *existing = weight;
                    }
                })
                .or_insert(weight);
        }
        *group = unique.into_iter().collect();
        group.sort_by(|a, b| {
            b.1.unwrap_or(0)
                .cmp(&a.1.unwrap_or(0))
                .then_with(|| a.0.cmp(&b.0))
        });
    }
}

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

        normalize_groups(&mut grouped);

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
        mut entries: BTreeMap<String, Vec<(String, Option<i64>)>>,
    ) -> Self {
        normalize_groups(&mut entries);
        Self {
            generation,
            source_hash,
            total_entries,
            entries,
        }
    }

    /// Exact code lookup (single key).
    pub fn lookup_exact(&self, code: &str) -> Vec<LexiconEntry> {
        self.lookup_exact_limited(code, usize::MAX)
    }

    /// Exact code lookup capped before candidate allocation.
    pub fn lookup_exact_limited(&self, code: &str, limit: usize) -> Vec<LexiconEntry> {
        if limit == 0 {
            return Vec::new();
        }
        let hash8 = self.source_hash.chars().take(8).collect::<String>();
        let source = format!("dict:{hash8}");
        let Some(entries) = self.entries.get(code) else {
            return Vec::new();
        };
        entries
            .iter()
            .take(limit)
            .map(|(text, weight)| LexiconEntry {
                text: text.clone(),
                code: code.to_owned(),
                weight: weight.unwrap_or(1),
                source: source.clone(),
                completion: false,
            })
            .collect()
    }

    /// Whether at least one dictionary code starts with `prefix`.
    pub fn has_code_prefix(&self, prefix: &str) -> bool {
        self.entries
            .range::<str, _>((Included(prefix), Unbounded))
            .next()
            .is_some_and(|(code, _)| code.starts_with(prefix))
    }

    /// Whether a longer, space-delimited code begins with `code`.
    pub fn has_longer_code(&self, code: &str) -> bool {
        self.entries
            .range::<str, _>((Excluded(code), Unbounded))
            .next()
            .is_some_and(|(candidate, _)| {
                candidate.starts_with(code) && candidate.as_bytes().get(code.len()) == Some(&b' ')
            })
    }

    /// Prefix search: all entries whose code starts with `prefix`.
    /// Returns up to `limit` candidates, sorted by weight descending.
    pub fn lookup_prefix(&self, prefix: &str, limit: usize) -> Vec<LexiconEntry> {
        if limit == 0 {
            return Vec::new();
        }

        #[derive(Eq, PartialEq)]
        struct RankedRef<'a> {
            weight: i64,
            text: &'a str,
            code: &'a str,
        }

        impl Ord for RankedRef<'_> {
            fn cmp(&self, other: &Self) -> Ordering {
                // BinaryHeap keeps the greatest item at the top. Reverse the
                // weight comparison so the worst retained candidate is the
                // one replaced first; larger text/code values lose ties.
                other
                    .weight
                    .cmp(&self.weight)
                    .then_with(|| self.text.cmp(other.text))
                    .then_with(|| self.code.cmp(other.code))
            }
        }

        impl PartialOrd for RankedRef<'_> {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }

        let range = self.entries.range::<str, _>((Included(prefix), Unbounded));
        let mut heap = BinaryHeap::<RankedRef<'_>>::with_capacity(limit);

        for (code, entries) in range {
            if !code.starts_with(prefix) {
                break;
            }
            for (text, weight) in entries {
                let candidate = RankedRef {
                    weight: weight.unwrap_or(1),
                    text,
                    code,
                };
                if heap.len() < limit {
                    heap.push(candidate);
                } else if heap
                    .peek()
                    .is_some_and(|worst| candidate.cmp(worst) == Ordering::Less)
                {
                    heap.pop();
                    heap.push(candidate);
                }
            }
        }

        let mut results: Vec<_> = heap.into_iter().collect();
        results.sort_by(|left, right| {
            right
                .weight
                .cmp(&left.weight)
                .then_with(|| left.text.cmp(right.text))
                .then_with(|| left.code.cmp(right.code))
        });

        let hash8 = self.source_hash.chars().take(8).collect::<String>();
        let source = format!("dict:{hash8}");
        results
            .into_iter()
            .map(|entry| LexiconEntry {
                weight: entry.weight,
                text: entry.text.to_owned(),
                code: entry.code.to_owned(),
                completion: entry.code != prefix,
                source: source.clone(),
            })
            .collect()
    }

    pub fn query(&self, code: &str) -> Vec<Candidate> {
        self.lookup_exact(code)
            .into_iter()
            .enumerate()
            .map(|(index, entry)| entry.into_candidate(index as u64 + 1))
            .collect()
    }

    pub fn query_prefix(&self, prefix: &str, limit: usize) -> Vec<Candidate> {
        self.lookup_prefix(prefix, limit)
            .into_iter()
            .enumerate()
            .map(|(index, entry)| entry.into_candidate(index as u64 + 1))
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

    pub fn lookup_exact(&self, code: &str) -> Vec<LexiconEntry> {
        match self {
            CompiledIndex::Memory(m) => m.lookup_exact(code),
            CompiledIndex::Tiered(t) => t.lookup_exact(code),
        }
    }

    pub fn lookup_exact_limited(&self, code: &str, limit: usize) -> Vec<LexiconEntry> {
        match self {
            CompiledIndex::Memory(m) => m.lookup_exact_limited(code, limit),
            CompiledIndex::Tiered(t) => t.lookup_exact(code).into_iter().take(limit).collect(),
        }
    }

    pub fn has_code_prefix(&self, prefix: &str) -> bool {
        match self {
            CompiledIndex::Memory(m) => m.has_code_prefix(prefix),
            CompiledIndex::Tiered(t) => t.has_code_prefix(prefix),
        }
    }

    pub fn has_longer_code(&self, code: &str) -> bool {
        match self {
            CompiledIndex::Memory(m) => m.has_longer_code(code),
            CompiledIndex::Tiered(t) => t.has_longer_code(code),
        }
    }

    pub fn lookup_prefix(&self, prefix: &str, limit: usize) -> Vec<LexiconEntry> {
        match self {
            CompiledIndex::Memory(m) => m.lookup_prefix(prefix, limit),
            CompiledIndex::Tiered(t) => t.lookup_prefix(prefix, limit),
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
