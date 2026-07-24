//! Candidate filters: deduplication, etc.

use crate::Filter;
use crate::decoder::ResolvedCandidate;
use std::collections::HashMap;

/// Removes duplicate candidates by text, keeping the first occurrence
/// (which typically comes from the highest-priority translator).
#[derive(Clone, Debug, Default)]
pub struct DedupFilter;

impl DedupFilter {
    pub fn new() -> Self {
        Self
    }
}

impl Filter for DedupFilter {
    fn name(&self) -> &str {
        "dedup"
    }

    fn filter(&self, candidates: Vec<ResolvedCandidate>) -> Vec<ResolvedCandidate> {
        let mut positions: HashMap<String, usize> = HashMap::with_capacity(candidates.len());
        let mut deduped: Vec<ResolvedCandidate> = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            if let Some(index) = positions.get(&candidate.text).copied() {
                if candidate.score > deduped[index].score {
                    deduped[index] = candidate;
                }
            } else {
                positions.insert(candidate.text.clone(), deduped.len());
                deduped.push(candidate);
            }
        }
        deduped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segmentation::InputSpan;
    use cheime_model::{Candidate, CandidateId};

    fn c(text: &str, source: &str, id: u64) -> ResolvedCandidate {
        ResolvedCandidate::from_display(
            Candidate {
                id: CandidateId::new(id),
                text: text.into(),
                annotation: None,
                source: source.into(),
                is_emoji: false,
            },
            InputSpan::new(0, 1),
            String::from("x"),
            true,
            0,
        )
    }

    #[test]
    fn removes_duplicates_by_text() {
        let filter = DedupFilter::new();
        let input = vec![c("你", "dict", 1), c("你", "user", 2), c("好", "dict", 3)];
        let result = filter.filter(input);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].source, "dict"); // keeps first
        assert_eq!(result[1].text, "好");
    }

    #[test]
    fn dedup_removes_exact_duplicate_by_text() {
        let filter = DedupFilter::new();
        let input = vec![c("测试", "dict", 1), c("测试", "user", 2)];
        let result = filter.filter(input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].source, "dict"); // keeps first
    }

    #[test]
    fn dedup_keeps_first_occurrence() {
        let filter = DedupFilter::new();
        // Three entries with "好": first from dict, second from user, third from history
        let input = vec![
            c("好", "dict", 1),
            c("好", "user", 2),
            c("好", "history", 3),
        ];
        let result = filter.filter(input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].source, "dict");
    }

    #[test]
    fn dedup_emoji_with_same_text_as_word() {
        let filter = DedupFilter::new();
        // The text differs: "👍" vs "赞" — should keep both
        let emoji = ResolvedCandidate::from_display(
            Candidate::emoji(CandidateId::new(1), "👍"),
            InputSpan::new(0, 1),
            String::from("x"),
            true,
            0,
        );
        let word = c("赞", "dict", 2);
        let input = vec![emoji.clone(), word.clone()];
        let result = filter.filter(input);
        assert_eq!(
            result.len(),
            2,
            "emoji and word have different text strings, both should be kept"
        );
    }

    #[test]
    fn empty_input_returns_empty() {
        let filter = DedupFilter::new();
        let result = filter.filter(Vec::new());
        assert!(result.is_empty());
    }
}
