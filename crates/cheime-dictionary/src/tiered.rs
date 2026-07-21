#![forbid(unsafe_code)]

use std::path::Path;
use std::sync::Arc;

use cheime_model::{Candidate, CandidateId};
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
    /// Sorted by code string ascending. Each code has top-N entries by weight.
    pub(crate) hot: Vec<(String, Vec<HotEntry>)>,
    /// Cold disk index.
    pub(crate) cold: Arc<TidexReader>,
    pub(crate) hot_entries_per_code: usize,
    pub(crate) total_entries: usize,
    pub(crate) source_hash: String,
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
        })
    }

    /// Exact code lookup — merge hot + cold entries.
    pub fn query(&self, code: &str) -> Vec<Candidate> {
        // Collect hot entries
        let hot_entries: Vec<_> = if let Ok(idx) = self.hot.binary_search_by(|(c, _)| c.as_str().cmp(code)) {
            self.hot[idx].1.clone()
        } else {
            Vec::new()
        };
        let cold_entries = self.cold.query(code);
        // Merge: hot first (already top-N by weight), then cold
        let mut all = Vec::new();
        for e in &hot_entries {
            all.push((e.weight, e.text.clone()));
        }
        for (text, weight) in cold_entries {
            // Avoid duplicates between hot and cold
            if !hot_entries.iter().any(|he| he.text == text) {
                all.push((weight, text));
            }
        }
        all.sort_by_key(|(w, _)| std::cmp::Reverse(*w));
        let hash8 = self.source_hash.chars().take(8).collect::<String>();
        all.into_iter().enumerate().map(|(idx, (_w, text))| Candidate {
            id: CandidateId::new(idx as u64 + 1),
            text,
            annotation: None,
            source: format!("dict:{hash8}"),
            is_emoji: false,
        }).collect()
    }

    /// Prefix search: top `limit` entries across all codes matching `prefix`.
    pub fn query_prefix(&self, prefix: &str, limit: usize) -> Vec<Candidate> {
        let start = self
            .hot
            .partition_point(|(c, _)| c.as_str() < prefix);

        let mut all = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // Hot entries within prefix range
        for (code, entries) in &self.hot[start..] {
            if !code.starts_with(prefix) {
                break;
            }
            for e in entries {
                if seen.insert(e.text.clone()) {
                    all.push((e.weight, e.text.clone()));
                }
            }
        }

        // Cold entries — skip texts already seen in hot
        let cold = self.cold.query_prefix(prefix, limit.saturating_sub(all.len()));
        for (text, weight) in cold {
            if seen.insert(text.clone()) {
                all.push((weight, text));
            }
        }

        all.sort_by_key(|(w, _)| std::cmp::Reverse(*w));
        all.truncate(limit);

        let hash8 = self.source_hash.chars().take(8).collect::<String>();
        all.into_iter()
            .enumerate()
            .map(|(idx, (_w, text))| Candidate {
                id: CandidateId::new(idx as u64 + 1),
                text,
                annotation: None,
                source: format!("dict:{hash8}"),
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
