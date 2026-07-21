#![forbid(unsafe_code)]

use crate::body::DictEntry;
use cheime_model::{Candidate, CandidateId, DeploymentGeneration};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledIndex {
    pub generation: DeploymentGeneration,
    pub source_hash: String,
    pub total_entries: usize,
    pub(crate) entries: BTreeMap<String, Vec<(String, Option<i64>)>>,
}

impl CompiledIndex {
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

            grouped.entry(entry.code.clone()).or_default().push((entry.text.clone(), entry.weight));
        }

        for group in grouped.values_mut() {
            group.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        }

        let mut hasher = Sha256::new();
        hasher.update(hash_state.as_bytes());
        let source_hash = format!("{:x}", hasher.finalize());

        Self { generation, source_hash, total_entries: entries.len(), entries: grouped }
    }

    /// Exact code lookup (single key).
    pub fn query(&self, code: &str) -> Vec<Candidate> {
        let hash8 = self.source_hash.chars().take(8).collect::<String>();
        self.entries.get(code).into_iter().flatten().enumerate().map(|(idx, (text, _))| Candidate {
            id: CandidateId::new(idx as u64 + 1),
            text: text.clone(),
            annotation: Some(code.to_owned()),
            source: format!("dict:{hash8}"),
            is_emoji: false,
        }).collect()
    }

    /// Prefix search: all entries whose code starts with `prefix`.
    /// Returns up to `limit` candidates, sorted by weight descending.
    pub fn query_prefix(&self, prefix: &str, limit: usize) -> Vec<Candidate> {
        use std::collections::BinaryHeap;
        let end = format!("{prefix}\u{10FFFF}");
        let range = self.entries.range(prefix.to_string()..=end);

        // Min-heap: (weight, text, code) — keep top `limit`
        let mut heap: BinaryHeap<(i64, String, String)> = BinaryHeap::new();

        for (code, entries) in range {
            for (text, weight) in entries {
                let w = weight.unwrap_or(1);
                if heap.len() < limit {
                    heap.push((w, text.clone(), code.clone()));
                } else if let Some(peek) = heap.peek() {
                    // Rust BinaryHeap is max-heap, so smallest = last after drain
                    // Simpler: just push and drain if over limit*2
                    heap.push((w, text.clone(), code.clone()));
                    if heap.len() > limit * 2 {
                        let mut drained: Vec<_> = heap.drain().collect();
                        drained.sort_by_key(|(w, _, _)| std::cmp::Reverse(*w));
                        drained.truncate(limit);
                        heap = drained.into_iter().collect();
                    }
                }
            }
        }

        let mut results: Vec<_> = heap.into_iter().collect();
        results.sort_by_key(|(w, _, _)| std::cmp::Reverse(*w));
        results.truncate(limit);

        let hash8 = self.source_hash.chars().take(8).collect::<String>();
        results.into_iter().enumerate().map(|(idx, (_w, text, code))| Candidate {
            id: CandidateId::new(idx as u64 + 1),
            text,
            annotation: (!code.is_empty()).then_some(code),
            source: format!("dict:{hash8}"),
            is_emoji: false,
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(text: &str, code: &str, weight: i64) -> DictEntry {
        DictEntry { text: text.into(), code: code.into(), weight: Some(weight), stem: None }
    }

    #[test]
    fn sorts_by_weight_desc_then_text_asc() {
        let entries = vec![
            entry("你", "ni", 80), entry("呢", "ni", 100), entry("拟", "ni", 90),
        ];
        let idx = CompiledIndex::build(entries, DeploymentGeneration::new(1));
        let candidates = idx.query("ni");
        assert_eq!(candidates[0].text, "呢");
        assert_eq!(candidates[1].text, "拟");
        assert_eq!(candidates[2].text, "你");
        assert_eq!(candidates[0].annotation.as_deref(), Some("ni"));
    }

    #[test]
    fn prefix_search_ni_matches_ni_and_ni_hao() {
        let entries = vec![
            entry("你", "ni", 100),
            entry("你好", "ni hao", 90),
            entry("你们", "ni men", 80),
            entry("呢", "ne", 100),
        ];
        let idx = CompiledIndex::build(entries, DeploymentGeneration::new(1));
        let cs = idx.query_prefix("ni", 10);
        assert_eq!(cs.len(), 3);
        assert!(cs.iter().any(|c| c.text == "你"));
        assert!(cs.iter().any(|c| c.text == "你好"));
        assert!(cs.iter().any(|c| c.text == "你们"));
    }

    #[test]
    fn prefix_search_n_matches_multiple_initials() {
        let entries = vec![
            entry("那", "na", 100),
            entry("你", "ni", 90),
            entry("牛", "niu", 80),
            entry("闹", "nao", 70),
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
