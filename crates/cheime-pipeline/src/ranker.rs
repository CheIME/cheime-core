//! Unified ranker — multi-signal candidate scoring (DRAFT §5.5).
//!
//! CheIME advantage: single re-ranker across all translators.
//! Rime sorts within each translator independently; no unified re-rank.

use crate::Ranker;
use crate::decoder::ResolvedCandidate;
use std::cmp::Ordering;

#[derive(Clone, Debug)]
pub struct RankWeights {
    pub source: f64,
    pub code_length: f64,
}

impl Default for RankWeights {
    fn default() -> Self {
        Self {
            source: 1.0,
            code_length: 0.3,
        }
    }
}

#[derive(Clone, Debug)]
pub struct UnifiedRanker {
    weights: RankWeights,
}

impl UnifiedRanker {
    pub fn new(weights: RankWeights) -> Self {
        Self { weights }
    }

    fn score(&self, c: &ResolvedCandidate) -> f64 {
        let mut s = c.score as f64;
        s += source_priority(&c.source) * self.weights.source * 10_000_000.0;
        s += self.weights.code_length * (1.0 / (c.text.chars().count() as f64).max(1.0));
        if c.is_emoji {
            s += 0.05;
        }
        s
    }
}

fn source_priority(src: &str) -> f64 {
    if src.starts_with("user") {
        1.0
    } else if src.starts_with("dict:exact:") {
        0.9
    } else if src.starts_with("dict") {
        0.8
    } else if src == "builtin" {
        0.7
    } else if src == "emoji" {
        0.5
    } else {
        0.3
    }
}

fn candidate_tier(src: &str) -> u8 {
    if src.starts_with("user") {
        5
    } else if src.starts_with("dict:exact:") {
        4
    } else if src.starts_with("dict") {
        3
    } else if src == "builtin" {
        2
    } else if src == "emoji" {
        1
    } else {
        0
    }
}

impl Ranker for UnifiedRanker {
    fn name(&self) -> &str {
        "unified"
    }
    fn rank(&self, mut candidates: Vec<ResolvedCandidate>) -> Vec<ResolvedCandidate> {
        candidates.sort_by(|a, b| {
            candidate_tier(&b.source)
                .cmp(&candidate_tier(&a.source))
                .then_with(|| {
                    self.score(b)
                        .partial_cmp(&self.score(a))
                        .unwrap_or(Ordering::Equal)
                })
        });
        candidates
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segmentation::InputSpan;
    use cheime_model::{Candidate, CandidateId};

    fn candidate(id: u64, text: &str, source: &str) -> ResolvedCandidate {
        ResolvedCandidate::from_display(
            Candidate::text(CandidateId::new(id), text, source),
            InputSpan::new(0, 1),
            String::from("x"),
            true,
            0,
        )
    }

    #[test]
    fn user_source_ranks_higher() {
        let r = UnifiedRanker::new(RankWeights::default());
        let input = vec![
            candidate(1, "中国", "dict:abc"),
            candidate(2, "中国", "user_dict"),
        ];
        let result = r.rank(input);
        assert_eq!(result[0].source, "user_dict");
    }

    #[test]
    fn emoji_ranks_below_dict() {
        let r = UnifiedRanker::new(RankWeights::default());
        let input = vec![
            ResolvedCandidate::from_display(
                Candidate::emoji(CandidateId::new(1), "😄"),
                InputSpan::new(0, 1),
                String::from("x"),
                true,
                0,
            ),
            candidate(2, "笑", "dict:abc"),
        ];
        let result = r.rank(input);
        assert_eq!(result[0].text, "笑");
    }

    #[test]
    fn shorter_code_preferred() {
        let r = UnifiedRanker::new(RankWeights {
            code_length: 10.0,
            ..Default::default()
        });
        let input = vec![
            candidate(1, "中华人民共和国", "dict"),
            candidate(2, "中国", "dict"),
        ];
        let result = r.rank(input);
        assert_eq!(result[0].text, "中国");
    }

    #[test]
    fn simplifier_annotated_source_retains_dict_priority() {
        // Use equal-length texts to isolate source_priority effect
        let r = UnifiedRanker::new(RankWeights {
            source: 1.0,
            code_length: 0.0,
        }); // disable code_length
        let input = vec![
            candidate(1, "中A", "builtin"),             // 0.7
            candidate(2, "中B", "dict:abc→simplified"), // annotated, should be 0.8
        ];
        let result = r.rank(input);
        assert_eq!(
            result[0].text, "中B",
            "simplifier-annotated dict (0.8) should rank above builtin (0.7)"
        );
    }
    #[test]
    fn annotated_dict_source_ranks_above_builtin() {
        let r = UnifiedRanker::new(RankWeights::default());
        let input = vec![
            candidate(1, "中国", "builtin"),
            candidate(2, "中国", "dict:abc→simplified"),
        ];
        let result = r.rank(input);
        assert_eq!(
            result[0].source, "dict:abc→simplified",
            "annotated dict source should rank above builtin"
        );
    }

    #[test]
    fn exact_dictionary_candidate_ranks_above_completion() {
        let r = UnifiedRanker::new(RankWeights {
            source: 1.0,
            code_length: 0.0,
        });
        let input = vec![
            candidate(1, "精确", "dict:exact:fixture"),
            candidate(2, "补全", "dict:fixture"),
        ];

        let result = r.rank(input);

        assert_eq!(result[0].text, "精确");
    }

    #[test]
    fn exact_dictionary_candidate_precedes_shorter_completion_by_default() {
        let r = UnifiedRanker::new(RankWeights::default());
        let input = vec![
            candidate(1, "中华人民共和国", "dict:exact:fixture"),
            candidate(2, "吗", "dict:fixture"),
        ];

        let result = r.rank(input);

        assert_eq!(result[0].text, "中华人民共和国");
    }

    #[test]
    fn annotated_user_source_still_top() {
        let r = UnifiedRanker::new(RankWeights::default());
        let input = vec![
            candidate(1, "中国", "dict:abc→simplified"),
            candidate(2, "中国", "user:abc→simplified"),
        ];
        let result = r.rank(input);
        assert_eq!(
            result[0].source, "user:abc→simplified",
            "annotated user source should still rank highest"
        );
    }

    #[test]
    fn multiple_annotated_sources_rank_correctly() {
        let r = UnifiedRanker::new(RankWeights::default());
        let input = vec![
            candidate(1, "中国", "unknown:x"),
            candidate(2, "中国", "emoji"),
            candidate(3, "中国", "dict:s2t→traditional"),
            candidate(4, "中国", "user_dict→simplified"),
        ];
        let result = r.rank(input);
        let sources: Vec<&str> = result.iter().map(|c| c.source.as_str()).collect();
        assert_eq!(
            sources[0], "user_dict→simplified",
            "user-annotated should be first"
        );
        assert_eq!(
            sources[1], "dict:s2t→traditional",
            "dict-annotated should be second"
        );
        assert_eq!(sources[2], "emoji", "emoji should be third");
        assert_eq!(sources[3], "unknown:x", "unknown should be last");
    }
}
