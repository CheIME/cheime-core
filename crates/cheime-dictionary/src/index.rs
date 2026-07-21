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
    entries: BTreeMap<String, Vec<(String, Option<i64>)>>,
}

impl CompiledIndex {
    pub fn build(entries: Vec<DictEntry>, generation: DeploymentGeneration) -> Self {
        let mut grouped: BTreeMap<String, Vec<(String, Option<i64>)>> = BTreeMap::new();
        let mut hash_state = String::new();

        for entry in &entries {
            hash_state.push_str(&format!(
                "{}\t{}\t{:?}\t{:?}\n",
                entry.text, entry.code, entry.weight, entry.stem
            ));
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

    pub fn query(&self, code: &str) -> Vec<Candidate> {
        self.entries
            .get(code)
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(idx, (text, _weight))| Candidate {
                id: CandidateId::new(idx as u64 + 1),
                text: text.clone(),
                annotation: Some(code.to_owned()),
                source: format!("dict:{}", self.source_hash.chars().take(8).collect::<String>()),
                is_emoji: false,
            })
            .collect()
    }
}

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
    fn sorts_by_weight_desc_then_text_asc() {
        let entries = vec![
            entry("呢", "ni", 10),
            entry("你", "ni", 100),
            entry("拟", "ni", 100),
        ];
        let idx = CompiledIndex::build(entries, DeploymentGeneration::new(1));
        let candidates = idx.query("ni");
        assert_eq!(candidates[0].text, "你");
        assert_eq!(candidates[1].text, "拟");
        assert_eq!(candidates[2].text, "呢");
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
    }
}
