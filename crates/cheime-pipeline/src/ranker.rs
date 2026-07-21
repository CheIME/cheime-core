//! Candidate ranker: sorts by frequency/weight.
//!
//! For now a simple sort by (weight desc, text asc). The unified
//! multi-signal ranker (DRAFT §5.5) is planned for a later phase.

use crate::Ranker;
use cheime_model::Candidate;

/// Sorts candidates by frequency (higher = earlier) then text (shorter = earlier).
#[derive(Clone, Debug, Default)]
pub struct FrequencyRanker;

impl FrequencyRanker {
    pub fn new() -> Self {
        Self
    }
}

impl Ranker for FrequencyRanker {
    fn name(&self) -> &str {
        "frequency"
    }

    fn rank(&self, mut candidates: Vec<Candidate>) -> Vec<Candidate> {
        // Sort by id as a proxy for weight — candidates from translators
        // should already be in weight order. This is a placeholder.
        candidates.sort_by(|a, b| {
            a.id
                .get()
                .cmp(&b.id.get())
                .then_with(|| a.text.len().cmp(&b.text.len()))
                .then_with(|| a.text.cmp(&b.text))
        });
        candidates
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cheime_model::CandidateId;

    #[test]
    fn sorts_by_id_then_text() {
        let ranker = FrequencyRanker::new();
        let input = vec![
            Candidate {
                id: CandidateId::new(3),
                text: "重".into(),
                annotation: None,
                source: "dict".into(),
            },
            Candidate {
                id: CandidateId::new(1),
                text: "中".into(),
                annotation: None,
                source: "dict".into(),
            },
            Candidate {
                id: CandidateId::new(2),
                text: "种".into(),
                annotation: None,
                source: "dict".into(),
            },
        ];
        let result = ranker.rank(input);
        assert_eq!(result[0].text, "中");
        assert_eq!(result[1].text, "种");
        assert_eq!(result[2].text, "重");
    }
}
