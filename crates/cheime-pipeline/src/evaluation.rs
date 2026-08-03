//! Frontend-free evaluation of candidate ranking and selection effort.
//!
//! The evaluator reuses [`InputPipeline::refresh`] and never mutates a live
//! session. It can therefore run in CI without loading a platform frontend or
//! registering an input-method DLL.

use crate::{InputPipeline, PipelineError};
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvaluationCase<'a> {
    pub pinyin: &'a str,
    pub target: &'a str,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EvaluationSummary {
    pub total_cases: usize,
    pub top1_hits: usize,
    pub topk_hits: usize,
    pub completed_cases: usize,
    pub failed_cases: usize,
    pub total_selection_cost: usize,
}

impl EvaluationSummary {
    pub fn top1_accuracy(&self) -> f64 {
        ratio(self.top1_hits, self.total_cases)
    }

    pub fn topk_accuracy(&self) -> f64 {
        ratio(self.topk_hits, self.total_cases)
    }

    /// KySS-style ideal-to-actual selection-cost ratio.
    ///
    /// A result is only reported when every case can be completed. This avoids
    /// silently inflating the score by dropping unrepresentable inputs.
    pub fn kyss(&self) -> Option<f64> {
        if self.total_cases == 0 || self.failed_cases != 0 || self.total_selection_cost == 0 {
            return None;
        }
        Some(self.total_cases as f64 / self.total_selection_cost as f64)
    }
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum EvaluationError {
    #[error("candidate page size must be greater than zero")]
    InvalidPageSize,
    #[error(transparent)]
    Pipeline(#[from] PipelineError),
}

pub fn evaluate_cases<P: InputPipeline + ?Sized>(
    pipeline: &P,
    cases: &[EvaluationCase<'_>],
    page_size: usize,
    top_k: usize,
) -> Result<EvaluationSummary, EvaluationError> {
    if page_size == 0 {
        return Err(EvaluationError::InvalidPageSize);
    }

    let mut summary = EvaluationSummary::default();
    for case in cases {
        summary.total_cases = summary.total_cases.saturating_add(1);
        let initial = pipeline.refresh(case.pinyin)?;
        let target_rank = initial
            .iter()
            .position(|candidate| candidate.display.text == case.target);
        summary.top1_hits = summary
            .top1_hits
            .saturating_add(usize::from(target_rank == Some(0)));
        summary.topk_hits = summary
            .topk_hits
            .saturating_add(usize::from(target_rank.is_some_and(|rank| rank < top_k)));

        match minimum_selection_cost(pipeline, case.pinyin, case.target, page_size)? {
            Some(cost) => {
                summary.completed_cases = summary.completed_cases.saturating_add(1);
                summary.total_selection_cost = summary.total_selection_cost.saturating_add(cost);
            }
            None => {
                summary.failed_cases = summary.failed_cases.saturating_add(1);
            }
        }
    }
    Ok(summary)
}

/// Find the cheapest sequence of candidate selections that produces `target`.
///
/// Candidate rank `r` has cost `floor(r / page_size) + 1`, matching the
/// page-turn plus numeric-selection model used by KySS. Dynamic programming is
/// used because selecting a lower-ranked long phrase can be cheaper than
/// repeatedly selecting top-ranked single characters.
pub fn minimum_selection_cost<P: InputPipeline + ?Sized>(
    pipeline: &P,
    pinyin: &str,
    target: &str,
    page_size: usize,
) -> Result<Option<usize>, EvaluationError> {
    if page_size == 0 {
        return Err(EvaluationError::InvalidPageSize);
    }
    let mut frontier = BinaryHeap::from([Reverse((0usize, 0usize, 0usize))]);
    let mut distances = HashMap::from([((0usize, 0usize), 0usize)]);
    let mut candidates_by_input_offset = HashMap::new();

    while let Some(Reverse((cost, input_offset, target_offset))) = frontier.pop() {
        if distances.get(&(input_offset, target_offset)) != Some(&cost) {
            continue;
        }
        if input_offset == pinyin.len() && target_offset == target.len() {
            return Ok(Some(cost));
        }
        if input_offset >= pinyin.len() || target_offset >= target.len() {
            continue;
        }

        if let std::collections::hash_map::Entry::Vacant(entry) =
            candidates_by_input_offset.entry(input_offset)
        {
            entry.insert(pipeline.refresh(&pinyin[input_offset..])?);
        }
        let remaining_input = &pinyin[input_offset..];
        let remaining_target = &target[target_offset..];
        let Some(candidates) = candidates_by_input_offset.get(&input_offset) else {
            continue;
        };
        for (rank, candidate) in candidates.iter().enumerate() {
            if candidate.consumed.start != 0
                || candidate.consumed.end == 0
                || candidate.consumed.end > remaining_input.len()
                || candidate.display.text.is_empty()
                || !remaining_target.starts_with(&candidate.display.text)
            {
                continue;
            }
            let next = (
                input_offset + candidate.consumed.end,
                target_offset + candidate.display.text.len(),
            );
            let next_cost = cost.saturating_add(rank / page_size + 1);
            let should_update = distances
                .get(&next)
                .is_none_or(|current| next_cost < *current);
            if should_update {
                distances.insert(next, next_cost);
                frontier.push(Reverse((next_cost, next.0, next.1)));
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoder::ResolvedCandidate;
    use crate::segmentation::InputSpan;
    use crate::{PipelineIntent, PipelineUpdate};
    use cheime_model::{Candidate, CandidateId, KeyEvent};

    struct FixturePipeline {
        by_input: HashMap<String, Vec<ResolvedCandidate>>,
    }

    impl InputPipeline for FixturePipeline {
        fn apply(
            &self,
            composition: &str,
            _event: &KeyEvent,
        ) -> Result<PipelineUpdate, PipelineError> {
            Ok(PipelineUpdate {
                composition: composition.to_owned(),
                candidates: self.refresh(composition)?,
                intent: PipelineIntent::None,
            })
        }

        fn refresh(&self, composition: &str) -> Result<Vec<ResolvedCandidate>, PipelineError> {
            Ok(self.by_input.get(composition).cloned().unwrap_or_default())
        }
    }

    fn candidate(text: &str, consumed: usize, id: u64) -> ResolvedCandidate {
        ResolvedCandidate::from_display(
            Candidate::text(CandidateId::new(id), text, "fixture"),
            InputSpan::new(0, consumed),
            String::from("fixture"),
            true,
            0,
        )
    }

    #[test]
    fn direct_top_candidate_has_ideal_kyss() {
        let pipeline = FixturePipeline {
            by_input: HashMap::from([(String::from("nihao"), vec![candidate("你好", 5, 1)])]),
        };
        let summary = evaluate_cases(
            &pipeline,
            &[EvaluationCase {
                pinyin: "nihao",
                target: "你好",
            }],
            5,
            5,
        )
        .unwrap();

        assert_eq!(summary.top1_accuracy(), 1.0);
        assert_eq!(summary.topk_accuracy(), 1.0);
        assert_eq!(summary.kyss(), Some(1.0));
    }

    #[test]
    fn dynamic_programming_finds_cheapest_partial_selection_path() {
        let pipeline = FixturePipeline {
            by_input: HashMap::from([
                (
                    String::from("nihao"),
                    vec![
                        candidate("你", 2, 1),
                        candidate("拟", 2, 2),
                        candidate("你好", 5, 3),
                    ],
                ),
                (String::from("hao"), vec![candidate("好", 3, 1)]),
            ]),
        };

        assert_eq!(
            minimum_selection_cost(&pipeline, "nihao", "你好", 5).unwrap(),
            Some(1),
            "one rank-2 phrase selection is cheaper than two rank-0 selections"
        );
    }

    #[test]
    fn page_turns_are_counted_in_selection_cost() {
        let mut candidates = (0..6)
            .map(|index| candidate(&format!("候选{index}"), 2, index + 1))
            .collect::<Vec<_>>();
        candidates.push(candidate("目标", 2, 7));
        let pipeline = FixturePipeline {
            by_input: HashMap::from([(String::from("mb"), candidates)]),
        };

        assert_eq!(
            minimum_selection_cost(&pipeline, "mb", "目标", 5).unwrap(),
            Some(2)
        );
    }

    #[test]
    fn failed_cases_do_not_report_a_misleading_kyss() {
        let pipeline = FixturePipeline {
            by_input: HashMap::new(),
        };
        let summary = evaluate_cases(
            &pipeline,
            &[EvaluationCase {
                pinyin: "nihao",
                target: "你好",
            }],
            5,
            5,
        )
        .unwrap();

        assert_eq!(summary.failed_cases, 1);
        assert_eq!(summary.kyss(), None);
    }
}
