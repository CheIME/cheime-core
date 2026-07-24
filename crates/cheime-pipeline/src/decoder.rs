use crate::segmentation::{InputSpan, SegmentationGraph, SyllableEdge, SyllableKind};
use cheime_dictionary::{CompiledIndex, LexiconEntry};
use cheime_model::{Candidate, CandidateId};
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

const BEAM_WIDTH: usize = 32;
const MAX_HOMOGRAPHS: usize = 8;
const MAX_CANDIDATES: usize = 100;
const MAX_SYLLABLES_PER_LEXEME: usize = 8;
const MAX_SEQUENCES_PER_START: usize = 1024;
const PHRASE_BONUS: i64 = 1_000_000;

pub trait Lexicon: Send + Sync {
    fn exact(&self, code: &str) -> Vec<LexiconEntry>;
    fn prefix(&self, code: &str, limit: usize) -> Vec<LexiconEntry>;
}

impl Lexicon for CompiledIndex {
    fn exact(&self, code: &str) -> Vec<LexiconEntry> {
        self.lookup_exact(code)
    }

    fn prefix(&self, code: &str, limit: usize) -> Vec<LexiconEntry> {
        self.lookup_prefix(code, limit)
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
}

#[derive(Clone, Debug)]
struct LexicalOption {
    end: usize,
    entry: LexiconEntry,
}

#[derive(Clone, Debug)]
struct DecodePath {
    end: usize,
    text: String,
    lexemes: Vec<SelectedLexeme>,
    completion: bool,
    score: i64,
}

impl Decoder {
    pub fn new(lexicons: Vec<Arc<dyn Lexicon>>) -> Self {
        Self { lexicons }
    }

    pub fn decode(&self, _input: &str, graph: &SegmentationGraph) -> Vec<ResolvedCandidate> {
        let mut beams = vec![Vec::<DecodePath>::new(); graph.input_len() + 1];
        beams[0].push(DecodePath {
            end: 0,
            text: String::new(),
            lexemes: Vec::new(),
            completion: false,
            score: 0,
        });

        let mut resolved = Vec::new();
        for start in 0..graph.input_len() {
            Self::prune_beam(&mut beams, start, &mut resolved, graph.input_len());
            if beams[start].is_empty() {
                continue;
            }
            let options = self.lexical_options(graph, start);
            for path in beams[start].clone() {
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
                    next.completion |= option.entry.completion;
                    next.score = next.score.saturating_add(option.entry.weight);
                    if option.entry.code.contains(' ') {
                        next.score = next.score.saturating_add(PHRASE_BONUS);
                    }
                    next.lexemes.push(lexeme);
                    beams[option.end].push(next);
                }
            }
        }
        Self::prune_beam(&mut beams, graph.input_len(), &mut resolved, graph.input_len());

        resolved.sort_by(|left, right| {
            right
                .complete
                .cmp(&left.complete)
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
    ) {
        let beam = &mut beams[offset];
        beam.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
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
                score: path.score,
            }
        }));
    }

    fn lexical_options(&self, graph: &SegmentationGraph, start: usize) -> Vec<LexicalOption> {
        let mut sequences = Vec::new();
        Self::collect_sequences(graph, start, &mut Vec::new(), &mut sequences);
        let mut options = Vec::new();

        for sequence in sequences {
            let Some(last) = sequence.last() else {
                continue;
            };
            let code = sequence
                .iter()
                .map(|edge| edge.canonical.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let is_completion = last.kind == SyllableKind::Incomplete
                && sequence[..sequence.len() - 1]
                    .iter()
                    .all(|edge| edge.kind == SyllableKind::Complete);
            let is_exact = sequence
                .iter()
                .all(|edge| edge.kind == SyllableKind::Complete);
            if !is_exact && !is_completion {
                continue;
            }
            for lexicon in &self.lexicons {
                let entries = if is_completion {
                    lexicon.prefix(&code, MAX_HOMOGRAPHS)
                } else {
                    lexicon
                        .exact(&code)
                        .into_iter()
                        .take(MAX_HOMOGRAPHS)
                        .collect()
                };
                options.extend(entries.into_iter().map(|entry| LexicalOption {
                    end: last.span.end,
                    entry,
                }));
            }
        }

        options.sort_by(|left, right| {
            right
                .entry
                .weight
                .cmp(&left.entry.weight)
                .then_with(|| left.entry.text.cmp(&right.entry.text))
                .then_with(|| left.entry.code.cmp(&right.entry.code))
                .then_with(|| left.end.cmp(&right.end))
        });
        options.dedup_by(|left, right| {
            left.end == right.end
                && left.entry.text == right.entry.text
                && left.entry.code == right.entry.code
        });
        options
    }

    fn collect_sequences<'a>(
        graph: &'a SegmentationGraph,
        offset: usize,
        current: &mut Vec<&'a SyllableEdge>,
        sequences: &mut Vec<Vec<&'a SyllableEdge>>,
    ) {
        if current.len() == MAX_SYLLABLES_PER_LEXEME {
            return;
        }
        if sequences.len() >= MAX_SEQUENCES_PER_START {
            return;
        }
        let mut edges: Vec<_> = graph.edges_from(offset).iter().collect();
        edges.sort_by(|left, right| {
            right
                .span
                .end
                .cmp(&left.span.end)
                .then_with(|| left.kind.cmp(&right.kind))
                .then_with(|| left.canonical.cmp(&right.canonical))
        });
        for edge in edges {
            if sequences.len() >= MAX_SEQUENCES_PER_START {
                break;
            }
            if edge.kind == SyllableKind::Raw {
                continue;
            }
            current.push(edge);
            sequences.push(current.clone());
            if edge.kind == SyllableKind::Complete {
                Self::collect_sequences(graph, edge.span.end, current, sequences);
            }
            current.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segmentor::PinyinSegmentor;
    use crate::Segmentor;
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
}
