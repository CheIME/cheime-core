#![forbid(unsafe_code)]

use std::path::Path;
use std::sync::Arc;

use crate::index::LexiconEntry;
use cheime_model::{Candidate, CandidateId, DeploymentGeneration};
use cheime_tidx::TidexReader;

// ---------------------------------------------------------------------------
// HotEntry
// ---------------------------------------------------------------------------

/// In-memory text + weight pair for hot tier.
#[derive(Clone, Debug)]
pub struct HotEntry {
    pub text: String,
    pub weight: i32,
}

// ---------------------------------------------------------------------------
// TieredIndex
// ---------------------------------------------------------------------------

/// A tiered index: top-N entries per code kept in memory (hot),
/// remainder on disk via mmap-backed `.tidx` file (cold).
pub struct TieredIndex {
    pub(crate) hot: Vec<(String, Vec<HotEntry>)>,
    pub(crate) cold: Arc<TidexReader>,
    pub(crate) hot_entries_per_code: usize,
    pub(crate) total_entries: usize,
    pub(crate) source_hash: String,
    pub(crate) generation: DeploymentGeneration,
}

// TidexReader contains Mmap (no Debug), so manual impl.
impl std::fmt::Debug for TieredIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TieredIndex")
            .field("codes", &self.hot.len())
            .field("total_entries", &self.total_entries)
            .field("hot_entries_per_code", &self.hot_entries_per_code)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// impl
// ---------------------------------------------------------------------------

impl TieredIndex {
    /// Build from pre-sorted, pre-grouped entries.
    pub fn new(
        code_entries: Vec<(String, Vec<(String, i32)>)>,
        tidx_path: &Path,
        hot_entries_per_code: usize,
        source_hash: String,
        generation: DeploymentGeneration,
    ) -> Result<Self, TidexBuildError> {
        let total_entries: usize = code_entries.iter().map(|(_, e)| e.len()).sum();
        let cold = TidexReader::open(tidx_path)
            .map_err(|e| TidexBuildError::ColdOpen(format!("{}", e)))?;

        let mut hot = Vec::with_capacity(code_entries.len());
        for (code, entries) in code_entries {
            let hot_entries: Vec<HotEntry> = entries
                .iter()
                .take(hot_entries_per_code)
                .map(|(text, weight)| HotEntry {
                    text: text.clone(),
                    weight: *weight,
                })
                .collect();
            hot.push((code, hot_entries));
        }

        Ok(Self {
            hot,
            cold: Arc::new(cold),
            hot_entries_per_code,
            total_entries,
            source_hash,
            generation,
        })
    }

    /// Exact code lookup — merge hot + cold entries.
    pub fn lookup_exact(&self, code: &str) -> Vec<LexiconEntry> {
        let mut seen = std::collections::HashSet::new();
        let mut all = Vec::new();

        // Hot entries
        if let Ok(idx) = self.hot.binary_search_by(|(c, _)| c.as_str().cmp(code)) {
            for e in &self.hot[idx].1 {
                if seen.insert(e.text.clone()) {
                    all.push((e.weight, e.text.clone()));
                }
            }
        }

        // Cold entries — skip duplicates
        for (text, weight) in self.cold.query(code) {
            if seen.insert(text.clone()) {
                all.push((weight, text));
            }
        }

        all.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
        let hash8 = self.source_hash.chars().take(8).collect::<String>();
        all.into_iter()
            .map(|(weight, text)| LexiconEntry {
                text,
                code: code.to_owned(),
                weight: i64::from(weight),
                source: format!("dict:{hash8}"),
                completion: false,
            })
            .collect()
    }

    /// Prefix search: top `limit` entries across all codes matching `prefix`.
    pub fn lookup_prefix(&self, prefix: &str, limit: usize) -> Vec<LexiconEntry> {
        let start = self.hot.partition_point(|(c, _)| c.as_str() < prefix);

        let mut all = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let hash8 = self.source_hash.chars().take(8).collect::<String>();

        // `hot` retains every code, so it can also provide the code metadata
        // for cold entries without changing the on-disk format.
        for (code, entries) in &self.hot[start..] {
            if !code.starts_with(prefix) {
                break;
            }
            for e in entries {
                if seen.insert((code.clone(), e.text.clone())) {
                    all.push(LexiconEntry {
                        text: e.text.clone(),
                        code: code.clone(),
                        weight: i64::from(e.weight),
                        source: format!("dict:{hash8}"),
                        completion: code != prefix,
                    });
                }
            }
            for (text, weight) in self.cold.query(code) {
                if seen.insert((code.clone(), text.clone())) {
                    all.push(LexiconEntry {
                        text,
                        code: code.clone(),
                        weight: i64::from(weight),
                        source: format!("dict:{hash8}"),
                        completion: code != prefix,
                    });
                }
            }
        }

        all.sort_by(|left, right| {
            right
                .weight
                .cmp(&left.weight)
                .then_with(|| left.text.cmp(&right.text))
                .then_with(|| left.code.cmp(&right.code))
        });
        all.truncate(limit);
        all
    }

    pub fn query(&self, code: &str) -> Vec<Candidate> {
        self.lookup_exact(code)
            .into_iter()
            .enumerate()
            .map(|(idx, entry)| Candidate {
                id: CandidateId::new(idx as u64 + 1),
                text: entry.text,
                annotation: Some(entry.code),
                source: entry.source,
                is_emoji: false,
            })
            .collect()
    }

    pub fn query_prefix(&self, prefix: &str, limit: usize) -> Vec<Candidate> {
        self.lookup_prefix(prefix, limit)
            .into_iter()
            .enumerate()
            .map(|(idx, entry)| Candidate {
                id: CandidateId::new(idx as u64 + 1),
                text: entry.text,
                annotation: Some(entry.code),
                source: entry.source,
                is_emoji: false,
            })
            .collect()
    }

    /// Number of unique codes in the hot tier.
    pub fn hot_code_count(&self) -> usize {
        self.hot.len()
    }
}

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum TidexBuildError {
    #[error("failed to open cold index: {0}")]
    ColdOpen(String),
}
