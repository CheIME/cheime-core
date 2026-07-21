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
}
