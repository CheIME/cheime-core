use crate::language_model::{LanguageModel, LanguageModelContext, NullLanguageModel};
use crate::segmentation::{InputSpan, SegmentationGraph, SyllableEdge, SyllableKind};
use cheime_dictionary::{CompiledIndex, LexiconEntry};
use cheime_model::{Candidate, CandidateId};
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

const BEAM_WIDTH: usize = 32;
const MAX_HOMOGRAPHS: usize = 8;
const MAX_SINGLE_SYLLABLE_HOMOGRAPHS: usize = 32;
const MAX_CANDIDATES: usize = 100;
const MAX_SYLLABLES_PER_LEXEME: usize = 8;
const LONG_INPUT_BYTES: usize = 12;
const MAX_SEQUENCES_PER_START: usize = 1024;
const MAX_COSTED_SEQUENCES_PER_START: usize = 64;
const PHRASE_BONUS: i64 = 1_000_000;
const USER_PINNED_BONUS: i64 = 100_000_000;

pub trait Lexicon: Send + Sync {
    fn exact(&self, code: &str) -> Vec<LexiconEntry>;
    fn prefix(&self, code: &str, limit: usize) -> Vec<LexiconEntry>;

    fn exact_limited(&self, code: &str, limit: usize) -> Vec<LexiconEntry> {
        self.exact(code).into_iter().take(limit).collect()
    }

    /// Returns whether a code may exist below this prefix.
    ///
    /// The conservative default preserves compatibility with custom lexicons:
    /// returning true only disables pruning, never hides candidates.
    fn has_prefix(&self, _prefix: &str) -> bool {
        true
    }

    fn has_longer(&self, code: &str) -> bool {
        let mut prefix = code.to_owned();
        prefix.push(' ');
        self.has_prefix(&prefix)
    }
}

impl Lexicon for CompiledIndex {
    fn exact(&self, code: &str) -> Vec<LexiconEntry> {
        self.lookup_exact(code)
    }

    fn prefix(&self, code: &str, limit: usize) -> Vec<LexiconEntry> {
        self.lookup_prefix(code, limit)
    }

    fn exact_limited(&self, code: &str, limit: usize) -> Vec<LexiconEntry> {
        self.lookup_exact_limited(code, limit)
    }

    fn has_prefix(&self, prefix: &str) -> bool {
        self.has_code_prefix(prefix)
    }

    fn has_longer(&self, code: &str) -> bool {
        self.has_longer_code(code)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectedLexeme {
    pub text: String,
    pub canonical_code: String,
    pub weight: i64,
    pub source: String,
}

impl SelectedLexeme {
    pub fn test(text: &str, canonical_code: &str) -> Self {
        Self {
            text: text.to_owned(),
            canonical_code: canonical_code.to_owned(),
            weight: 1,
            source: String::from("test"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedCandidate {
    pub display: Candidate,
    pub consumed: InputSpan,
    pub canonical_code: String,
    pub lexemes: Vec<SelectedLexeme>,
    pub complete: bool,
    pub exact_phrase: bool,
    pub completion: bool,
    pub score: i64,
}

impl ResolvedCandidate {
    pub fn from_display(
        display: Candidate,
        consumed: InputSpan,
        canonical_code: String,
        complete: bool,
        score: i64,
    ) -> Self {
        let lexeme = SelectedLexeme {
            text: display.text.clone(),
            canonical_code: canonical_code.clone(),
            weight: score,
            source: display.source.clone(),
        };
        Self {
            display,
            consumed,
            canonical_code,
            lexemes: vec![lexeme],
            complete,
            exact_phrase: true,
            completion: false,
            score,
        }
    }
}

impl Deref for ResolvedCandidate {
    type Target = Candidate;

    fn deref(&self) -> &Self::Target {
        &self.display
    }
}

impl DerefMut for ResolvedCandidate {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.display
    }
}

pub struct Decoder {
    lexicons: Vec<Arc<dyn Lexicon>>,
    options: DecoderOptions,
    language_model: Arc<dyn LanguageModel>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecoderOptions {
    pub enable_completion: bool,
    pub enable_sentence: bool,
}

impl Default for DecoderOptions {
    fn default() -> Self {
        Self {
            enable_completion: true,
            enable_sentence: true,
        }
    }
}

#[derive(Clone, Debug)]
struct LexicalOption {
    end: usize,
    entry: LexiconEntry,
    typing_cost: i64,
}

struct SyllableSequence<'a> {
    edges: Vec<&'a SyllableEdge>,
    code: String,
    typing_cost: i64,
}

#[derive(Clone, Debug)]
struct DecodePath {
    end: usize,
    text: String,
    lexemes: Vec<SelectedLexeme>,
    syllables: usize,
    completion: bool,
    score: ScoreBreakdown,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ScoreBreakdown {
    total: i64,
    dictionary: i64,
    user_lexicon: i64,
    phrase: i64,
    language_model: i64,
    typing_penalty: i64,
}

impl ScoreBreakdown {
    fn total(self) -> i64 {
        self.total
    }

    fn append(
        &mut self,
        dictionary: i64,
        user_lexicon: i64,
        phrase: i64,
        language_model: i64,
        typing_penalty: i64,
    ) {
        self.total = self
            .total
            .saturating_add(dictionary)
            .saturating_add(user_lexicon)
            .saturating_add(phrase)
            .saturating_add(language_model)
            .saturating_sub(typing_penalty.max(0));
        self.dictionary = self.dictionary.saturating_add(dictionary);
        self.user_lexicon = self.user_lexicon.saturating_add(user_lexicon);
        self.phrase = self.phrase.saturating_add(phrase);
        self.language_model = self.language_model.saturating_add(language_model);
        self.typing_penalty = self.typing_penalty.saturating_add(typing_penalty.max(0));
    }
}

impl Decoder {
    pub fn new(lexicons: Vec<Arc<dyn Lexicon>>) -> Self {
        Self::with_options(lexicons, DecoderOptions::default())
    }

    pub fn with_options(lexicons: Vec<Arc<dyn Lexicon>>, options: DecoderOptions) -> Self {
        Self {
            lexicons,
            options,
            language_model: Arc::new(NullLanguageModel),
        }
    }

    /// Attach a deterministic language model to this decoder.
    ///
    /// The default is [`NullLanguageModel`], which keeps historical ordering.
    pub fn with_language_model(mut self, language_model: Arc<dyn LanguageModel>) -> Self {
        self.language_model = language_model;
        self
    }

    pub fn decode(&self, _input: &str, graph: &SegmentationGraph) -> Vec<ResolvedCandidate> {
        let mut beams = vec![Vec::<DecodePath>::new(); graph.input_len() + 1];
        beams[0].push(DecodePath {
            end: 0,
            text: String::new(),
            lexemes: Vec::new(),
            syllables: 0,
            completion: false,
            score: ScoreBreakdown::default(),
        });

        let mut resolved = Vec::new();
        let mut longer_prefix_cache = HashMap::<String, bool>::new();
        // Short ambiguous inputs benefit from an explicit segmentation prior.
        // Long inputs already prefer fewer lexical units in the beam and must
        // avoid paying for another ordering dimension on every comparison.
        let prefer_fewer_syllables =
            graph.input_len() < LONG_INPUT_BYTES && graph.has_complete_path();
        for start in 0..graph.input_len() {
            Self::prune_beam(
                &mut beams,
                start,
                &mut resolved,
                graph.input_len(),
                prefer_fewer_syllables,
            );
            if beams[start].is_empty() {
                continue;
            }
            let options = self.lexical_options(graph, start, &mut longer_prefix_cache);
            for path in beams[start].clone() {
                if !self.options.enable_sentence && !path.lexemes.is_empty() {
                    continue;
                }
                for option in &options {
                    let lexeme = SelectedLexeme {
                        text: option.entry.text.clone(),
                        canonical_code: option.entry.code.clone(),
                        weight: option.entry.weight,
                        source: option.entry.source.clone(),
                    };
                    let mut next = path.clone();
                    next.end = option.end;
                    next.text.push_str(&lexeme.text);
                    next.syllables = next
                        .syllables
                        .saturating_add(lexeme.canonical_code.split_ascii_whitespace().count());
                    next.completion |= option.entry.completion;
                    let context = LanguageModelContext {
                        previous_previous: path
                            .lexemes
                            .len()
                            .checked_sub(2)
                            .and_then(|index| path.lexemes.get(index))
                            .map(|item| item.text.as_str()),
                        previous: path.lexemes.last().map(|item| item.text.as_str()),
                    };
                    let language_model_score = self.language_model.score(context, &lexeme.text);
                    next.score.append(
                        option.entry.weight,
                        if option.entry.source.starts_with("user:pinned") {
                            USER_PINNED_BONUS
                        } else {
                            0
                        },
                        if option.entry.code.contains(' ') {
                            PHRASE_BONUS
                        } else {
                            0
                        },
                        language_model_score,
                        option.typing_cost,
                    );
                    next.lexemes.push(lexeme);
                    beams[option.end].push(next);
                }
            }
        }
        Self::prune_beam(
            &mut beams,
            graph.input_len(),
            &mut resolved,
            graph.input_len(),
            prefer_fewer_syllables,
        );

        resolved.sort_by(|left, right| {
            right
                .complete
                .cmp(&left.complete)
                .then_with(|| left.completion.cmp(&right.completion))
                .then_with(|| right.exact_phrase.cmp(&left.exact_phrase))
                .then_with(|| {
                    left.canonical_code
                        .split_ascii_whitespace()
                        .count()
                        .cmp(&right.canonical_code.split_ascii_whitespace().count())
                })
                .then_with(|| right.score.cmp(&left.score))
                .then_with(|| left.lexemes.len().cmp(&right.lexemes.len()))
                .then_with(|| left.display.text.cmp(&right.display.text))
                .then_with(|| left.canonical_code.cmp(&right.canonical_code))
        });

        let mut by_text = HashMap::<String, usize>::new();
        let mut deduped = Vec::new();
        for candidate in resolved {
            if by_text.contains_key(&candidate.display.text) {
                continue;
            }
            by_text.insert(candidate.display.text.clone(), deduped.len());
            deduped.push(candidate);
            if deduped.len() == MAX_CANDIDATES {
                break;
            }
        }
        for (index, candidate) in deduped.iter_mut().enumerate() {
            candidate.display.id = CandidateId::new(index as u64 + 1);
        }
        deduped
    }

    fn prune_beam(
        beams: &mut [Vec<DecodePath>],
        offset: usize,
        resolved: &mut Vec<ResolvedCandidate>,
        input_len: usize,
        prefer_fewer_syllables: bool,
    ) {
        let beam = &mut beams[offset];
        beam.sort_by(|left, right| {
            left.completion
                .cmp(&right.completion)
                .then_with(|| {
                    if prefer_fewer_syllables {
                        left.syllables.cmp(&right.syllables)
                    } else {
                        std::cmp::Ordering::Equal
                    }
                })
                .then_with(|| {
                    if offset == input_len || input_len >= LONG_INPUT_BYTES {
                        left.lexemes.len().cmp(&right.lexemes.len())
                    } else {
                        std::cmp::Ordering::Equal
                    }
                })
                .then_with(|| right.score.total().cmp(&left.score.total()))
                .then_with(|| left.lexemes.len().cmp(&right.lexemes.len()))
                .then_with(|| left.text.cmp(&right.text))
        });
        beam.dedup_by(|left, right| {
            left.text == right.text
                && left
                    .lexemes
                    .iter()
                    .map(|lexeme| lexeme.canonical_code.as_str())
                    .eq(right
                        .lexemes
                        .iter()
                        .map(|lexeme| lexeme.canonical_code.as_str()))
        });
        beam.truncate(BEAM_WIDTH);

        if offset == 0 {
            return;
        }
        resolved.extend(beam.iter().map(|path| {
            let canonical_code = path
                .lexemes
                .iter()
                .map(|lexeme| lexeme.canonical_code.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let source = path
                .lexemes
                .first()
                .map(|lexeme| lexeme.source.clone())
                .unwrap_or_else(|| String::from("decoder"));
            ResolvedCandidate {
                display: Candidate {
                    id: CandidateId::new(0),
                    text: path.text.clone(),
                    annotation: Some(canonical_code.clone()),
                    source,
                    is_emoji: false,
                },
                consumed: InputSpan::new(0, path.end),
                canonical_code,
                lexemes: path.lexemes.clone(),
                complete: path.end == input_len,
                exact_phrase: path.lexemes.len() == 1,
                completion: path.completion,
                score: path.score.total(),
            }
        }));
    }

    fn lexical_options(
        &self,
        graph: &SegmentationGraph,
        start: usize,
        longer_prefix_cache: &mut HashMap<String, bool>,
    ) -> Vec<LexicalOption> {
        let mut sequences = Vec::new();
        let sequence_limit = if graph.has_costs() {
            MAX_COSTED_SEQUENCES_PER_START
        } else {
            MAX_SEQUENCES_PER_START
        };
        self.collect_sequences(
            graph,
            start,
            sequence_limit,
            longer_prefix_cache,
            &mut sequences,
        );
        let mut options = Vec::new();

        for sequence in sequences {
            let Some(last) = sequence.edges.last() else {
                continue;
            };
            let is_completion = last.kind == SyllableKind::Incomplete
                && sequence.edges[..sequence.edges.len() - 1]
                    .iter()
                    .all(|edge| edge.kind == SyllableKind::Complete);
            let is_exact = sequence
                .edges
                .iter()
                .all(|edge| edge.kind == SyllableKind::Complete);
            if !is_exact && !is_completion {
                continue;
            }
            if is_completion && !self.options.enable_completion {
                continue;
            }
            // A composition containing only one syllable is also the common
            // state after the user confirms the first segment of a phrase.
            // Keep more than one page of homographs in that narrow case so
            // page navigation remains useful, without multiplying the branch
            // factor of normal sentence decoding.
            let homograph_limit =
                if start == 0 && last.span.end == graph.input_len() && sequence.edges.len() == 1 {
                    MAX_SINGLE_SYLLABLE_HOMOGRAPHS
                } else {
                    MAX_HOMOGRAPHS
                };
            for lexicon in &self.lexicons {
                let entries = if is_completion {
                    lexicon.prefix(&sequence.code, homograph_limit)
                } else {
                    lexicon.exact_limited(&sequence.code, homograph_limit)
                };
                options.extend(entries.into_iter().map(|entry| LexicalOption {
                    end: last.span.end,
                    entry,
                    typing_cost: sequence.typing_cost,
                }));
            }
        }

        options.sort_by(|left, right| {
            right
                .entry
                .weight
                .saturating_sub(right.typing_cost)
                .cmp(&left.entry.weight.saturating_sub(left.typing_cost))
                .then_with(|| left.entry.text.cmp(&right.entry.text))
                .then_with(|| left.entry.code.cmp(&right.entry.code))
                .then_with(|| left.end.cmp(&right.end))
                .then_with(|| left.typing_cost.cmp(&right.typing_cost))
        });
        options.dedup_by(|left, right| {
            left.end == right.end
                && left.entry.text == right.entry.text
                && left.entry.code == right.entry.code
        });
        options
    }

    fn collect_sequences<'a>(
        &self,
        graph: &'a SegmentationGraph,
        start: usize,
        sequence_limit: usize,
        longer_prefix_cache: &mut HashMap<String, bool>,
        sequences: &mut Vec<SyllableSequence<'a>>,
    ) {
        struct Pending<'a> {
            offset: usize,
            edges: Vec<&'a SyllableEdge>,
            code: String,
            typing_cost: i64,
            emit: bool,
        }

        let mut pending = vec![Some(Pending {
            offset: start,
            edges: Vec::new(),
            code: String::new(),
            typing_cost: 0,
            emit: false,
        })];
        let mut frontier = BinaryHeap::from([Reverse((0i64, 0usize, 0usize))]);
        let mut serial = 1usize;

        let pending_limit = sequence_limit.saturating_mul(4);
        while sequences.len() < sequence_limit {
            let Some(Reverse((_cost, _order, pending_index))) = frontier.pop() else {
                break;
            };
            let Some(state) = pending[pending_index].take() else {
                continue;
            };
            if state.emit {
                sequences.push(SyllableSequence {
                    edges: state.edges.clone(),
                    code: state.code.clone(),
                    typing_cost: state.typing_cost,
                });
            }
            if state.edges.len() == MAX_SYLLABLES_PER_LEXEME
                || state
                    .edges
                    .last()
                    .is_some_and(|edge| edge.kind != SyllableKind::Complete)
            {
                continue;
            }

            // A longer lexeme must contain the current complete code followed
            // by another syllable. Once that prefix is absent from every
            // lexicon, continuing this sequence can never produce a match.
            if state.emit {
                let has_longer = if let Some(cached) = longer_prefix_cache.get(&state.code) {
                    *cached
                } else {
                    let found = self
                        .lexicons
                        .iter()
                        .any(|lexicon| lexicon.has_longer(&state.code));
                    longer_prefix_cache.insert(state.code.clone(), found);
                    found
                };
                if !has_longer {
                    continue;
                }
            }

            let mut edges = graph.edges_from(state.offset).iter().collect::<Vec<_>>();
            edges.sort_by(|left, right| {
                graph
                    .edge_cost(left)
                    .cmp(&graph.edge_cost(right))
                    .then_with(|| right.span.end.cmp(&left.span.end))
                    .then_with(|| left.kind.cmp(&right.kind))
                    .then_with(|| left.canonical.cmp(&right.canonical))
            });
            for edge in edges {
                if frontier.len() >= pending_limit {
                    break;
                }
                if edge.kind == SyllableKind::Raw && edge.raw != "'" {
                    continue;
                }
                let mut next_edges = state.edges.clone();
                let emit = edge.kind != SyllableKind::Raw;
                let mut code = state.code.clone();
                let mut typing_cost = state.typing_cost;
                if emit {
                    next_edges.push(edge);
                    if !code.is_empty() {
                        code.push(' ');
                    }
                    code.push_str(&edge.canonical);
                    typing_cost = typing_cost.saturating_add(graph.edge_cost(edge));
                }
                let pending_index = pending.len();
                pending.push(Some(Pending {
                    offset: edge.span.end,
                    edges: next_edges,
                    code,
                    typing_cost,
                    emit,
                }));
                frontier.push(Reverse((typing_cost, serial, pending_index)));
                serial = serial.saturating_add(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Segmentor;
    use crate::segmentor::PinyinSegmentor;
    use cheime_dictionary::{CompiledIndex, DictEntry};
    use cheime_model::DeploymentGeneration;
    use std::sync::Arc;

    fn decoder(entries: &[(&str, &str, i64)]) -> Decoder {
        let entries = entries
            .iter()
            .map(|(text, code, weight)| DictEntry {
                text: (*text).to_owned(),
                code: (*code).to_owned(),
                weight: Some(*weight),
                stem: None,
            })
            .collect();
        Decoder::new(vec![Arc::new(CompiledIndex::build(
            entries,
            DeploymentGeneration::new(1),
        ))])
    }

    #[test]
    fn incomplete_nih_decodes_to_nihao() {
        let decoder = decoder(&[
            ("你好", "ni hao", 200),
            ("你", "ni", 100),
            ("好", "hao", 100),
        ]);
        let graph = PinyinSegmentor::new().segment("nih");
        let results = decoder.decode("nih", &graph);
        let candidate = results
            .iter()
            .find(|candidate| candidate.display.text == "你好")
            .unwrap();
        assert!(candidate.complete);
        assert_eq!(candidate.canonical_code, "ni hao");
        assert!(candidate.completion);
    }

    #[test]
    fn missing_phrase_is_composed_from_lexemes() {
        let decoder = decoder(&[("旎", "ni", 90), ("皓", "hao", 80)]);
        let graph = PinyinSegmentor::new().segment("nihao");
        let candidate = decoder
            .decode("nihao", &graph)
            .into_iter()
            .find(|candidate| candidate.display.text == "旎皓")
            .unwrap();
        assert!(!candidate.exact_phrase);
        assert_eq!(candidate.lexemes.len(), 2);
    }

    #[test]
    fn single_remaining_syllable_keeps_multiple_pages_of_homographs() {
        let entries = (1..=20)
            .map(|index| DictEntry {
                text: format!("hao-{index}"),
                code: String::from("hao"),
                weight: Some(1_000 - index),
                stem: None,
            })
            .collect();
        let decoder = Decoder::new(vec![Arc::new(CompiledIndex::build(
            entries,
            DeploymentGeneration::new(1),
        ))]);
        let graph = PinyinSegmentor::new().segment("hao");

        let results = decoder.decode("hao", &graph);

        assert_eq!(results.len(), 20);
        assert!(results.iter().all(|candidate| candidate.complete));
    }

    #[test]
    fn long_sentence_ranking_surfaces_real_decoder_prefix_candidates() {
        use crate::Ranker;
        use crate::ranker::{RankWeights, UnifiedRanker};

        let decoder = decoder(&[
            ("full-a", "ni hao shi jie zhong guo", 900),
            ("full-b", "ni hao shi jie zhong guo", 800),
            ("full-c", "ni hao shi jie zhong guo", 700),
            ("ni-a", "ni", 600),
            ("ni-b", "ni", 590),
            ("ni-c", "ni", 580),
            ("ni-d", "ni", 570),
            ("ni-e", "ni", 560),
            ("ni-f", "ni", 550),
            ("ni-g", "ni", 540),
            ("ni-h", "ni", 530),
            ("hao", "hao", 500),
            ("shi", "shi", 500),
            ("jie", "jie", 500),
            ("zhong", "zhong", 500),
            ("guo", "guo", 500),
        ]);
        let graph = PinyinSegmentor::new().segment("nihaoshijiezhongguo");
        let ranked = UnifiedRanker::new(RankWeights::default())
            .rank(decoder.decode("nihaoshijiezhongguo", &graph));

        assert!(ranked[0].complete);
        assert_eq!(
            ranked[..9]
                .iter()
                .filter(|candidate| candidate.complete)
                .count(),
            1,
            "only the best complete sentence should occupy the first page"
        );
        assert!(
            ranked[1..9]
                .iter()
                .all(|candidate| candidate.consumed.end < graph.input_len()),
            "later first-page candidates should consume only a prefix"
        );
    }

    #[test]
    fn complete_syllable_path_beats_frequency_inflated_split() {
        let decoder = decoder(&[
            ("选字", "xuan zi", 2_325),
            ("框", "kuang", 27_076),
            ("许", "xu", 1_299_512),
            ("按", "an", 3_826_950),
            ("字", "zi", 906_615),
            ("况", "kuang", 655_809),
        ]);
        let graph = PinyinSegmentor::new().segment("xuanzikuang");
        let results = decoder.decode("xuanzikuang", &graph);

        assert_eq!(results[0].canonical_code, "xuan zi kuang");
        assert!(!results[0].canonical_code.starts_with("xu an"));
    }

    #[test]
    fn completion_can_be_disabled() {
        let index = Arc::new(CompiledIndex::build(
            vec![DictEntry {
                text: "你好".into(),
                code: "ni hao".into(),
                weight: Some(200),
                stem: None,
            }],
            DeploymentGeneration::new(1),
        ));
        let decoder = Decoder::with_options(
            vec![index],
            DecoderOptions {
                enable_completion: false,
                enable_sentence: true,
            },
        );
        let graph = PinyinSegmentor::new().segment("nih");
        assert!(
            decoder
                .decode("nih", &graph)
                .iter()
                .all(|candidate| candidate.display.text != "你好")
        );
    }

    #[test]
    fn sentence_composition_can_be_disabled_without_hiding_exact_phrases() {
        let decoder = decoder(&[("你好", "ni hao", 200), ("旎", "ni", 90), ("皓", "hao", 80)]);
        let decoder = Decoder::with_options(
            decoder.lexicons,
            DecoderOptions {
                enable_completion: true,
                enable_sentence: false,
            },
        );
        let graph = PinyinSegmentor::new().segment("nihao");
        let results = decoder.decode("nihao", &graph);
        assert!(
            results
                .iter()
                .any(|candidate| candidate.display.text == "你好")
        );
        assert!(
            results
                .iter()
                .all(|candidate| candidate.display.text != "旎皓")
        );
    }

    #[test]
    fn apostrophe_is_a_traversable_hard_syllable_boundary() {
        let decoder = decoder(&[("西安", "xi an", 200)]);
        let graph = PinyinSegmentor::new().segment("xi'an");
        let candidate = decoder
            .decode("xi'an", &graph)
            .into_iter()
            .find(|candidate| candidate.display.text == "西安")
            .unwrap();
        assert!(candidate.complete);
        assert_eq!(candidate.canonical_code, "xi an");
    }

    #[test]
    fn language_model_reranks_composed_lexemes() {
        use crate::language_model::BackoffNgramModel;

        let decoder = decoder(&[
            ("甲", "ni", 100),
            ("乙", "ni", 100),
            ("丙", "hao", 100),
            ("丁", "hao", 100),
        ])
        .with_language_model(Arc::new(
            BackoffNgramModel::new(0).with_bigram("甲", "丁", 1_000),
        ));
        let graph = PinyinSegmentor::new().segment("nihao");
        let results = decoder.decode("nihao", &graph);

        assert_eq!(results[0].display.text, "甲丁");
        assert_eq!(results[0].score, 1_200);
    }

    #[test]
    fn extreme_language_model_score_saturates_instead_of_overflowing() {
        use crate::language_model::BackoffNgramModel;

        let decoder = decoder(&[("甲", "ni", 100)])
            .with_language_model(Arc::new(BackoffNgramModel::new(i64::MAX)));
        let graph = PinyinSegmentor::new().segment("ni");
        let results = decoder.decode("ni", &graph);

        assert_eq!(results[0].score, i64::MAX);
    }

    #[test]
    fn typo_edge_competes_inside_the_same_word_graph() {
        use crate::segmentor::PinyinCorrectionOptions;

        let decoder = decoder(&[("什么", "shen me", 200)]);
        let graph = PinyinSegmentor::new()
            .with_correction(PinyinCorrectionOptions {
                enabled: true,
                edit_penalty: 10_000,
                max_candidates_per_start: 64,
                ..Default::default()
            })
            .segment("shenem");
        let candidate = decoder
            .decode("shenem", &graph)
            .into_iter()
            .find(|candidate| candidate.display.text == "什么")
            .expect("joint graph should decode transposed em as me");

        assert!(candidate.complete);
        assert_eq!(candidate.canonical_code, "shen me");
        assert_eq!(candidate.score, PHRASE_BONUS + 200 - 10_000);
    }
}
