#![forbid(unsafe_code)]

use crate::body::DictEntry;
use cheime_model::{Candidate, CandidateId, DeploymentGeneration};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
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
    pub fn lookup_exact(&self, code: &str) -> Vec<LexiconEntry> {
        let hash8 = self.source_hash.chars().take(8).collect::<String>();
        self.entries
            .get(code)
            .into_iter()
            .flatten()
            .map(|(text, weight)| LexiconEntry {
                text: text.clone(),
                code: code.to_owned(),
                weight: weight.unwrap_or(1),
                source: format!("dict:{hash8}"),
                completion: false,
            })
            .collect()
    }

    /// Prefix search: all entries whose code starts with `prefix`.
    /// Returns up to `limit` candidates, sorted by weight descending.
    pub fn lookup_prefix(&self, prefix: &str, limit: usize) -> Vec<LexiconEntry> {
        use std::cmp::Reverse;
        use std::collections::BinaryHeap;

        if limit == 0 {
            return Vec::new();
        }
        let end = format!("{prefix}\u{10FFFF}");
        let range = self.entries.range(prefix.to_string()..=end);
        let mut heap = BinaryHeap::new();

        for (code, entries) in range {
            for (text, weight) in entries {
                heap.push((Reverse(weight.unwrap_or(1)), text.clone(), code.clone()));
                if heap.len() > limit {
                    heap.pop();
                }
            }
        }

        let source = format!(
            "dict:{}",
            self.source_hash.chars().take(8).collect::<String>()
        );
        let mut results: Vec<_> = heap
            .into_iter()
            .map(|(Reverse(weight), text, code)| LexiconEntry {
                completion: code != prefix,
                text,
                code,
                weight,
                source: source.clone(),
            })
            .collect();
        results.sort_by(|left, right| {
            right.weight.cmp(&left.weight).then_with(|| {
                left.text
                    .cmp(&right.text)
                    .then_with(|| left.code.cmp(&right.code))
            })
        });
        results
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(text: &str, code: &str, weight: i64) -> DictEntry {
        DictEntry {
            text: text.into(),
            code: code.into(),
            weight: Some(weight),
            stem: None,
        }
    }

    #[test]
    fn weighted_lookup_preserves_code_weight_and_completion() {
        let idx = MemoryIndex::build(
            vec![entry("你好", "ni hao", 200)],
            DeploymentGeneration::new(1),
        );
        let exact = idx.lookup_exact("ni hao");
        assert_eq!(exact[0].code, "ni hao");
        assert_eq!(exact[0].weight, 200);
        assert!(!exact[0].completion);

        let prefix = idx.lookup_prefix("ni h", 10);
        assert_eq!(prefix[0].code, "ni hao");
        assert!(prefix[0].completion);
    }

    #[test]
    fn sorts_by_weight_desc_then_text_asc() {
        let entries = vec![
            entry("你", "ni", 100),
            entry("呢", "ni", 90),
            entry("拟", "ni", 80),
        ];
        let idx = CompiledIndex::build(entries, DeploymentGeneration::new(1));
        let candidates = idx.query("ni");
        assert_eq!(candidates[0].text, "你"); // weight 100 highest
        assert_eq!(candidates[1].text, "呢"); // weight 90
        assert_eq!(candidates[2].text, "拟"); // weight 80
    }

    #[test]
    fn prefix_search_ni_matches_ni_and_ni_hao() {
        let entries = vec![
            entry("你", "ni", 100),
            entry("你好", "ni hao", 200),
            entry("那里", "na li", 50),
        ];
        let idx = CompiledIndex::build(entries, DeploymentGeneration::new(1));
        let cs = idx.query_prefix("ni", 10);
        assert_eq!(cs.len(), 2); // "ni" + "ni hao" = 2
        assert!(cs.iter().any(|c| c.text == "你"));
        assert!(cs.iter().any(|c| c.text == "你好"));
        // Should NOT contain "na li" entries
        assert!(!cs.iter().any(|c| c.text == "那里"));
    }

    #[test]
    fn prefix_search_n_matches_multiple_initials() {
        let entries = vec![
            entry("那", "na", 100),
            entry("你", "ni", 90),
            entry("女", "nv", 80),
            entry("年", "nian", 70),
        ];
        let idx = CompiledIndex::build(entries, DeploymentGeneration::new(1));
        let cs = idx.query_prefix("n", 10);
        assert_eq!(cs.len(), 4);
        assert_eq!(cs[0].text, "那"); // highest weight
    }

    #[test]
    fn assigns_stable_candidate_ids() {
        let entries = vec![entry("你", "ni", 100), entry("好", "hao", 100)];
        let idx1 = CompiledIndex::build(entries.clone(), DeploymentGeneration::new(1));
        let idx2 = CompiledIndex::build(entries, DeploymentGeneration::new(1));
        assert_eq!(idx1.query("ni")[0].id, idx2.query("ni")[0].id);
    }

    #[test]
    fn empty_query_returns_empty() {
        let idx = CompiledIndex::build(vec![], DeploymentGeneration::new(1));
        assert!(idx.query("nonexistent").is_empty());
        assert!(idx.query_prefix("x", 10).is_empty());
    }
}
