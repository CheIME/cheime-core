//! Unified ranker — multi-signal candidate scoring (DRAFT §5.5).
//!
//! CheIME advantage: single re-ranker across all translators.
//! Rime sorts within each translator independently; no unified re-rank.

use crate::Ranker;
use cheime_model::Candidate;
use std::cmp::Ordering;

#[derive(Clone, Debug)]
pub struct RankWeights {
    pub source: f64,
    pub code_length: f64,
}

impl Default for RankWeights {
    fn default() -> Self { Self { source: 1.0, code_length: 0.3 } }
}

#[derive(Clone, Debug)]
pub struct UnifiedRanker {
    weights: RankWeights,
}

impl UnifiedRanker {
    pub fn new(weights: RankWeights) -> Self { Self { weights } }

    fn score(&self, c: &Candidate) -> f64 {
        let mut s = source_priority(&c.source) * self.weights.source;
        s += self.weights.code_length * (1.0 / (c.text.chars().count() as f64).max(1.0));
        if c.is_emoji { s += 0.05; }
        s
    }
}

fn source_priority(src: &str) -> f64 {
    if src.starts_with("user") { 1.0 }
    else if src.starts_with("dict") { 0.8 }
    else if src == "builtin" { 0.7 }
    else if src == "emoji" { 0.5 }
    else { 0.3 }
}

impl Ranker for UnifiedRanker {
    fn name(&self) -> &str { "unified" }
    fn rank(&self, mut candidates: Vec<Candidate>) -> Vec<Candidate> {
        candidates.sort_by(|a, b| {
            self.score(b).partial_cmp(&self.score(a)).unwrap_or(Ordering::Equal)
        });
        candidates
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cheime_model::CandidateId;

    #[test]
    fn user_source_ranks_higher() {
        let r = UnifiedRanker::new(RankWeights::default());
        let input = vec![
            Candidate::text(CandidateId::new(1), "中国", "dict:abc"),
            Candidate::text(CandidateId::new(2), "中国", "user_dict"),
        ];
        let result = r.rank(input);
        assert_eq!(result[0].source, "user_dict");
    }

    #[test]
    fn emoji_ranks_below_dict() {
        let r = UnifiedRanker::new(RankWeights::default());
        let input = vec![
            Candidate::emoji(CandidateId::new(1), "😄"),
            Candidate::text(CandidateId::new(2), "笑", "dict:abc"),
        ];
        let result = r.rank(input);
        assert_eq!(result[0].text, "笑");
    }

    #[test]
    fn shorter_code_preferred() {
        let r = UnifiedRanker::new(RankWeights { code_length: 10.0, ..Default::default() });
        let input = vec![
            Candidate::text(CandidateId::new(1), "中华人民共和国", "dict"),
            Candidate::text(CandidateId::new(2), "中国", "dict"),
        ];
        let result = r.rank(input);
        assert_eq!(result[0].text, "中国");
    }
}
