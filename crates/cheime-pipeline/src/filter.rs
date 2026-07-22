//! Candidate filters: deduplication, etc.

use crate::Filter;
use cheime_model::Candidate;
use std::collections::HashSet;

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

    fn filter(&self, candidates: Vec<Candidate>) -> Vec<Candidate> {
        let mut seen = HashSet::with_capacity(candidates.len());
        candidates
            .into_iter()
            .filter(|c| seen.insert(c.text.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cheime_model::CandidateId;

    fn c(text: &str, source: &str, id: u64) -> Candidate {
        Candidate {
            id: CandidateId::new(id),
            text: text.into(),
            annotation: None,
            source: source.into(),
            is_emoji: false,
        }
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
        let emoji = Candidate::emoji(CandidateId::new(1), "👍");
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
