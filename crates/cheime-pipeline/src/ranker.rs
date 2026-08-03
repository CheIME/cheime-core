//! Unified ranker — multi-signal candidate scoring (DRAFT §5.5).
//!
//! CheIME advantage: single re-ranker across all translators.
//! Rime sorts within each translator independently; no unified re-rank.

use crate::Ranker;
use crate::decoder::ResolvedCandidate;
use std::cmp::Ordering;

const LONG_COMPOSITION_SYLLABLES: usize = 5;
const PRIMARY_CANDIDATE_WINDOW: usize = 9;
const MAX_CHUNK_SYLLABLES: usize = 4;
const EMOJI_PRIMARY_POSITION: usize = 4;

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

    /// Keep the best sentence candidate, then spend the rest of the first page
    /// on independently confirmable prefixes. Applying this as a list-level
    /// penalty preserves decoder score semantics while preventing a long input
    /// from producing a page full of speculative sentence alternatives.
    fn diversify_long_composition(
        &self,
        candidates: Vec<ResolvedCandidate>,
    ) -> Vec<ResolvedCandidate> {
        let full_span = candidates
            .iter()
            .filter(|candidate| candidate.complete)
            .map(|candidate| candidate.consumed.end)
            .max()
            .unwrap_or(0);
        let full_syllables = candidates
            .iter()
            .filter(|candidate| candidate.complete && candidate.consumed.end == full_span)
            .map(candidate_syllables)
            .max()
            .unwrap_or(0);
        if candidates.len() <= 1 || full_syllables < LONG_COMPOSITION_SYLLABLES {
            return candidates;
        }

        let mut remaining: Vec<Option<ResolvedCandidate>> =
            candidates.into_iter().map(Some).collect();
        let mut diversified = Vec::with_capacity(remaining.len());

        // Preserve the already-ranked top result, including pinned user words.
        if let Some(first) = remaining[0].take() {
            diversified.push(first);
        }

        let slots = PRIMARY_CANDIDATE_WINDOW
            .saturating_sub(diversified.len())
            .min(remaining.len().saturating_sub(diversified.len()));
        for _ in 0..slots {
            let Some(index) = remaining.iter().position(|candidate| {
                candidate.as_ref().is_some_and(|candidate| {
                    is_confirmable_chunk(candidate, full_span)
                        && candidate_syllables(candidate) <= MAX_CHUNK_SYLLABLES
                })
            }) else {
                break;
            };
            if let Some(candidate) = remaining[index].take() {
                diversified.push(candidate);
            }
        }

        // Keep every other candidate accessible in its original relative order.
        diversified.extend(remaining.into_iter().flatten());
        diversified
    }

    /// Put the highest-ranked matching emoji in a predictable first-page slot.
    ///
    /// For a normal candidate page this is the fifth item. If the whole list
    /// contains five or fewer items, place it second-to-last so the final text
    /// candidate remains easy to reach. Other emoji candidates retain their
    /// relative order.
    fn position_primary_emoji(
        &self,
        mut candidates: Vec<ResolvedCandidate>,
    ) -> Vec<ResolvedCandidate> {
        let Some(source_index) = candidates.iter().position(|candidate| candidate.is_emoji) else {
            return candidates;
        };
        let emoji = candidates.remove(source_index);
        let total_len = candidates.len() + 1;
        let target_index = if total_len <= EMOJI_PRIMARY_POSITION + 1 {
            total_len.saturating_sub(2)
        } else {
            EMOJI_PRIMARY_POSITION
        }
        .min(candidates.len());
        candidates.insert(target_index, emoji);
        candidates
    }
}

fn candidate_syllables(candidate: &ResolvedCandidate) -> usize {
    candidate.canonical_code.split_ascii_whitespace().count()
}

fn is_confirmable_chunk(candidate: &ResolvedCandidate, full_span: usize) -> bool {
    !candidate.complete
        && !candidate.completion
        && candidate.consumed.start == 0
        && candidate.consumed.end > 0
        && candidate.consumed.end < full_span
}

fn source_priority(src: &str) -> f64 {
    if src.starts_with("user:pinned") {
        1.0
    } else if src.starts_with("dict:exact:") {
        0.9
    } else if src.starts_with("dict") || src.starts_with("user") {
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
    if src.starts_with("user:pinned") {
        5
    } else if src.starts_with("dict:exact:") {
        4
    } else if src.starts_with("dict") || src.starts_with("user") {
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
                .then_with(|| b.complete.cmp(&a.complete))
                .then_with(|| a.completion.cmp(&b.completion))
                .then_with(|| b.exact_phrase.cmp(&a.exact_phrase))
                .then_with(|| {
                    a.canonical_code
                        .split_ascii_whitespace()
                        .count()
                        .cmp(&b.canonical_code.split_ascii_whitespace().count())
                })
                .then_with(|| {
                    self.score(b)
                        .partial_cmp(&self.score(a))
                        .unwrap_or(Ordering::Equal)
                })
        });
        let candidates = self.diversify_long_composition(candidates);
        self.position_primary_emoji(candidates)
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

    fn spanned_candidate(
        id: u64,
        text: &str,
        code: &str,
        consumed: usize,
        complete: bool,
        score: i64,
    ) -> ResolvedCandidate {
        ResolvedCandidate::from_display(
            Candidate::text(CandidateId::new(id), text, "dict"),
            InputSpan::new(0, consumed),
            code.to_owned(),
            complete,
            score,
        )
    }

    #[test]
    fn long_composition_keeps_best_sentence_then_exposes_prefix_chunks() {
        let ranker = UnifiedRanker::new(RankWeights::default());
        let mut candidates = vec![
            spanned_candidate(
                1,
                "best-full-sentence",
                "ni hao shi jie zhong guo",
                19,
                true,
                900,
            ),
            spanned_candidate(
                2,
                "second-full-sentence",
                "ni hao shi jie zhong guo",
                19,
                true,
                800,
            ),
            spanned_candidate(
                3,
                "third-full-sentence",
                "ni hao shi jie zhong guo",
                19,
                true,
                700,
            ),
        ];
        for (id, text, code, consumed, score) in [
            (10, "chunk-1a", "ni", 2, 600),
            (11, "chunk-2a", "ni hao", 5, 590),
            (12, "chunk-3a", "ni hao shi", 8, 580),
            (13, "chunk-4a", "ni hao shi jie", 11, 570),
            (14, "chunk-1b", "ni", 2, 560),
            (15, "chunk-1c", "ni", 2, 550),
            (16, "chunk-2b", "ni hao", 5, 540),
            (17, "chunk-2c", "ni hao", 5, 530),
        ] {
            candidates.push(spanned_candidate(id, text, code, consumed, false, score));
        }

        let ranked = ranker.rank(candidates);

        assert_eq!(ranked[0].text, "best-full-sentence");
        assert!(
            ranked[1..PRIMARY_CANDIDATE_WINDOW]
                .iter()
                .all(|candidate| !candidate.complete),
            "the remainder of the first page should contain confirmable chunks"
        );
        assert_eq!(
            ranked[PRIMARY_CANDIDATE_WINDOW].text,
            "second-full-sentence"
        );
    }

    #[test]
    fn short_composition_keeps_normal_complete_candidate_ordering() {
        let ranker = UnifiedRanker::new(RankWeights::default());
        let ranked = ranker.rank(vec![
            spanned_candidate(1, "best-full", "ni hao", 5, true, 900),
            spanned_candidate(2, "second-full", "ni hao", 5, true, 800),
            spanned_candidate(3, "prefix", "ni", 2, false, 700),
        ]);

        assert_eq!(
            ranked
                .iter()
                .map(|candidate| candidate.text.as_str())
                .collect::<Vec<_>>(),
            vec!["best-full", "second-full", "prefix"]
        );
    }

    #[test]
    fn incomplete_completion_is_not_promoted_as_a_confirmable_chunk() {
        let ranker = UnifiedRanker::new(RankWeights::default());
        let best = spanned_candidate(1, "best-full", "ni hao shi jie zhong guo", 19, true, 900);
        let alternative =
            spanned_candidate(2, "second-full", "ni hao shi jie zhong guo", 19, true, 800);
        let mut completion = spanned_candidate(3, "completion", "ni hao ma", 4, false, 10_000);
        completion.completion = true;
        let chunk = spanned_candidate(4, "chunk", "ni", 2, false, 100);

        let ranked = ranker.rank(vec![completion, alternative, chunk, best]);

        assert_eq!(ranked[0].text, "best-full");
        assert_eq!(ranked[1].text, "chunk");
        assert_eq!(ranked[2].text, "second-full");
        assert_eq!(ranked[3].text, "completion");
    }

    #[test]
    fn matching_emoji_is_the_fifth_candidate_on_a_normal_page() {
        let ranker = UnifiedRanker::new(RankWeights::default());
        let mut candidates = (1..=8)
            .map(|id| candidate(id, &format!("word-{id}"), "dict"))
            .collect::<Vec<_>>();
        candidates.push(ResolvedCandidate::from_display(
            Candidate::emoji(CandidateId::new(20), "😀"),
            InputSpan::new(0, 1),
            String::from("xiao"),
            true,
            0,
        ));

        let ranked = ranker.rank(candidates);

        assert_eq!(ranked[EMOJI_PRIMARY_POSITION].text, "😀");
        assert!(ranked[EMOJI_PRIMARY_POSITION].is_emoji);
    }

    #[test]
    fn emoji_is_second_to_last_when_five_or_fewer_candidates_exist() {
        let ranker = UnifiedRanker::new(RankWeights::default());
        for text_candidate_count in 1..=4 {
            let mut candidates = (1..=text_candidate_count)
                .map(|id| candidate(id, &format!("word-{id}"), "dict"))
                .collect::<Vec<_>>();
            candidates.push(ResolvedCandidate::from_display(
                Candidate::emoji(CandidateId::new(20), "😀"),
                InputSpan::new(0, 1),
                String::from("xiao"),
                true,
                0,
            ));

            let ranked = ranker.rank(candidates);
            let expected = ranked.len().saturating_sub(2);

            assert_eq!(ranked[expected].text, "😀");
        }
    }

    #[test]
    fn a_single_emoji_candidate_remains_selectable() {
        let ranker = UnifiedRanker::new(RankWeights::default());
        let emoji = ResolvedCandidate::from_display(
            Candidate::emoji(CandidateId::new(20), "😀"),
            InputSpan::new(0, 1),
            String::from("xiao"),
            true,
            0,
        );

        let ranked = ranker.rank(vec![emoji]);

        assert_eq!(ranked.len(), 1);
        assert!(ranked[0].is_emoji);
    }

    #[test]
    fn pinned_user_source_ranks_higher() {
        let r = UnifiedRanker::new(RankWeights::default());
        let input = vec![
            candidate(1, "中国", "dict:abc"),
            candidate(2, "中国", "user:pinned"),
        ];
        let result = r.rank(input);
        assert_eq!(result[0].source, "user:pinned");
    }

    #[test]
    fn ordinary_learned_source_competes_by_score() {
        let r = UnifiedRanker::new(RankWeights::default());
        let dictionary = ResolvedCandidate {
            score: 500_000,
            ..candidate(1, "自己", "dict")
        };
        let learned_once = ResolvedCandidate {
            score: 200_000,
            ..candidate(2, "字级", "user:learned")
        };
        let result = r.rank(vec![learned_once, dictionary]);
        assert_eq!(result[0].text, "自己");
    }

    #[test]
    fn emoji_uses_second_to_last_slot_even_when_source_priority_is_lower() {
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
        assert_eq!(result[0].text, "😄");
        assert_eq!(result[1].text, "笑");
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
    fn fully_spelled_candidate_precedes_higher_scored_completion_in_same_tier() {
        let r = UnifiedRanker::new(RankWeights::default());
        let exact = candidate(1, "完整", "dict");
        let mut completion = candidate(2, "补全", "dict");
        completion.completion = true;
        completion.score = i64::MAX;

        let result = r.rank(vec![completion, exact]);

        assert_eq!(result[0].text, "完整");
        assert!(!result[0].completion);
    }

    #[test]
    fn exact_lexeme_precedes_higher_scored_composed_abbreviation() {
        let ranker = UnifiedRanker::new(RankWeights::default());
        let exact = ResolvedCandidate {
            exact_phrase: true,
            score: 3_314_815,
            ..candidate(1, "真", "dict:abc")
        };
        let composed = ResolvedCandidate {
            exact_phrase: false,
            score: 20_000_000,
            ..candidate(2, "这年", "dict:abc")
        };

        let ranked = ranker.rank(vec![composed, exact]);
        assert_eq!(ranked[0].text, "真");
    }

    #[test]
    fn annotated_user_source_still_top() {
        let r = UnifiedRanker::new(RankWeights::default());
        let input = vec![
            candidate(1, "中国", "dict:abc→simplified"),
            candidate(2, "中国", "user:pinned→simplified"),
        ];
        let result = r.rank(input);
        assert_eq!(
            result[0].source, "user:pinned→simplified",
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
            candidate(4, "中国", "user:pinned→simplified"),
        ];
        let result = r.rank(input);
        let sources: Vec<&str> = result.iter().map(|c| c.source.as_str()).collect();
        assert_eq!(
            sources[0], "user:pinned→simplified",
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
