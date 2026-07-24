//! Code normalizer: expands a segment code into fuzzy/spelling variants.
//!
//! CheIME advantage: typed Normalizer component replaces Rime's
//! speller algebra regex chains (derive/xform/fuzz/abbrev/erase).
//! Each rule is a typed struct, not an opaque regex.

use crate::CodeSegment;
use crate::segmentation::{SegmentationGraph, SyllableEdge};
use std::collections::HashMap;

/// Expands a code segment into variant spellings.
/// Each variant is then translated independently by downstream translators.
pub trait CodeNormalizer: Send + Sync {
    fn name(&self) -> &str;
    fn normalize(&self, segment: &CodeSegment) -> Vec<CodeSegment>;

    /// Normalize all segments together (cross-segment logic like abbreviation).
    /// Default: per-segment normalize.
    fn normalize_all(&self, segments: &[CodeSegment]) -> Vec<CodeSegment> {
        segments.iter().flat_map(|s| self.normalize(s)).collect()
    }

    fn normalize_graph(&self, graph: &SegmentationGraph) -> SegmentationGraph {
        let mut normalized = SegmentationGraph::new(graph.input_len());
        for edge in graph.edges() {
            let segment = CodeSegment {
                code: edge.canonical.clone(),
                tag: String::from("pinyin"),
            };
            for variant in self.normalize(&segment) {
                normalized.add_edge(SyllableEdge {
                    span: edge.span,
                    raw: edge.raw.clone(),
                    canonical: variant.code,
                    kind: edge.kind,
                });
            }
        }
        normalized.finish();
        normalized
    }
}

// ── Fuzzy pinyin ───────────────────────────────────────────────────

/// Common fuzzy pinyin rules. Each rule maps a consonant/vowel pair.
#[derive(Clone, Debug, Default)]
pub struct FuzzyNormalizer {
    rules: Vec<FuzzyRule>,
}

#[derive(Clone, Debug)]
struct FuzzyRule {
    from: &'static str,
    to: &'static str,
}

impl FuzzyNormalizer {
    /// Standard fuzzy rules (Chinese southern accent common patterns).
    pub fn standard() -> Self {
        Self {
            rules: vec![
                FuzzyRule {
                    from: "zh",
                    to: "z",
                },
                FuzzyRule {
                    from: "z",
                    to: "zh",
                },
                FuzzyRule {
                    from: "ch",
                    to: "c",
                },
                FuzzyRule {
                    from: "c",
                    to: "ch",
                },
                FuzzyRule {
                    from: "sh",
                    to: "s",
                },
                FuzzyRule {
                    from: "s",
                    to: "sh",
                },
                FuzzyRule { from: "n", to: "l" },
                FuzzyRule { from: "l", to: "n" },
                FuzzyRule { from: "f", to: "h" },
                FuzzyRule { from: "h", to: "f" },
                FuzzyRule {
                    from: "ang",
                    to: "an",
                },
                FuzzyRule {
                    from: "eng",
                    to: "en",
                },
                FuzzyRule {
                    from: "ing",
                    to: "in",
                },
                FuzzyRule {
                    from: "an",
                    to: "ang",
                },
                FuzzyRule {
                    from: "en",
                    to: "eng",
                },
                FuzzyRule {
                    from: "in",
                    to: "ing",
                },
            ],
        }
    }

    /// Apply a single substitution to a code string.
    fn apply_rule(code: &str, from: &str, to: &str) -> Option<String> {
        code.strip_prefix(from)
            .map(|suffix| format!("{to}{suffix}"))
    }

    /// Create from rule names like "zh_z", "n_l". Empty = all standard rules.
    pub fn from_rules(rule_names: &[String]) -> Self {
        let all = Self::standard();
        let rules: Vec<FuzzyRule> = all
            .rules
            .into_iter()
            .filter(|r| {
                let key = format!("{}_{}", r.from, r.to);
                rule_names.iter().any(|name| name == &key)
            })
            .collect();
        Self { rules }
    }
}

impl CodeNormalizer for FuzzyNormalizer {
    fn name(&self) -> &str {
        "fuzzy"
    }

    fn normalize(&self, segment: &CodeSegment) -> Vec<CodeSegment> {
        let mut variants = Vec::new();
        variants.push(segment.clone());

        for rule in &self.rules {
            if rule.from.len() == 1
                && rule.to.starts_with(rule.from)
                && segment.code.starts_with(rule.to)
            {
                continue;
            }
            if let Some(variant_code) = Self::apply_rule(&segment.code, rule.from, rule.to) {
                variants.push(CodeSegment {
                    code: variant_code,
                    tag: format!("{}-fuzzy", segment.tag),
                });
            }
        }

        variants
    }
}

// ── Passthrough ────────────────────────────────────────────────────

#[derive(Clone, Debug, Default)]
pub struct PassthroughNormalizer;

impl CodeNormalizer for PassthroughNormalizer {
    fn name(&self) -> &str {
        "passthrough"
    }
    fn normalize(&self, segment: &CodeSegment) -> Vec<CodeSegment> {
        vec![segment.clone()]
    }
}

// ── Abbreviation (简拼) ────────────────────────────────────────────

/// Expands the first single-letter segment to all possible pinyin syllables
/// starting with that letter. Only activates when ALL segments are single letters
/// (pure abbreviation input like "nh", "nhm").
pub struct AbbreviationNormalizer {
    by_initial: HashMap<char, Vec<String>>,
}

impl AbbreviationNormalizer {
    pub fn new() -> Self {
        let mut by_initial: HashMap<char, Vec<String>> = HashMap::new();
        for &syl in crate::segmentor::PINYIN_SYLLABLES {
            if let Some(first) = syl.chars().next() {
                by_initial.entry(first).or_default().push(syl.to_string());
            }
        }
        Self { by_initial }
    }
}

impl Default for AbbreviationNormalizer {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeNormalizer for AbbreviationNormalizer {
    fn name(&self) -> &str {
        "abbreviation"
    }

    fn normalize(&self, segment: &CodeSegment) -> Vec<CodeSegment> {
        vec![segment.clone()]
    }

    fn normalize_all(&self, segments: &[CodeSegment]) -> Vec<CodeSegment> {
        // Only activate for pure abbreviation: all segments are single letters, >= 2 segments
        if segments.len() < 2 || !segments.iter().all(|s| s.code.len() == 1) {
            return segments.to_vec();
        }

        let first_letter = match segments[0].code.chars().next() {
            Some(c) => c,
            None => return segments.to_vec(),
        };
        let expansions = match self.by_initial.get(&first_letter) {
            Some(v) => v,
            None => return segments.to_vec(),
        };

        let mut variants = Vec::with_capacity(expansions.len() * segments.len());
        for expanded in expansions {
            variants.push(CodeSegment {
                code: expanded.clone(),
                tag: format!("{}-abbrev", segments[0].tag),
            });
            for s in &segments[1..] {
                variants.push(s.clone());
            }
        }
        variants
    }

    fn normalize_graph(&self, graph: &SegmentationGraph) -> SegmentationGraph {
        let mut normalized = graph.clone();
        for edge in graph.edges() {
            if edge.raw.len() != 1 || !edge.raw.as_bytes()[0].is_ascii_lowercase() {
                continue;
            }
            let initial = edge.raw.as_bytes()[0] as char;
            let Some(expansions) = self.by_initial.get(&initial) else {
                continue;
            };
            for canonical in expansions {
                normalized.add_edge(SyllableEdge {
                    span: edge.span,
                    raw: edge.raw.clone(),
                    canonical: canonical.clone(),
                    kind: crate::segmentation::SyllableKind::Complete,
                });
            }
        }
        normalized.finish();
        normalized
    }
}

// ── Composite (组合) ────────────────────────────────────────────────

/// Chains multiple normalizers: e.g. abbreviation expansion then fuzzy variants.
pub struct CompositeNormalizer {
    normalizers: Vec<Box<dyn CodeNormalizer>>,
}

impl CompositeNormalizer {
    pub fn new(normalizers: Vec<Box<dyn CodeNormalizer>>) -> Self {
        Self { normalizers }
    }
}

impl CodeNormalizer for CompositeNormalizer {
    fn name(&self) -> &str {
        "composite"
    }

    fn normalize(&self, segment: &CodeSegment) -> Vec<CodeSegment> {
        let mut current = vec![segment.clone()];
        for norm in &self.normalizers {
            current = current.iter().flat_map(|s| norm.normalize(s)).collect();
        }
        current
    }

    fn normalize_all(&self, segments: &[CodeSegment]) -> Vec<CodeSegment> {
        let mut current: Vec<CodeSegment> = segments.to_vec();
        for norm in &self.normalizers {
            current = norm.normalize_all(&current);
        }
        current
    }

    fn normalize_graph(&self, graph: &SegmentationGraph) -> SegmentationGraph {
        let mut current = graph.clone();
        for normalizer in &self.normalizers {
            current = normalizer.normalize_graph(&current);
        }
        current
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segmentation::{InputSpan, SegmentationGraph, SyllableEdge, SyllableKind};

    #[test]
    fn graph_normalization_preserves_span_and_kind() {
        let mut graph = SegmentationGraph::new(5);
        graph.add_edge(SyllableEdge {
            span: InputSpan::new(0, 5),
            raw: String::from("zhong"),
            canonical: String::from("zhong"),
            kind: SyllableKind::Complete,
        });

        let normalized = FuzzyNormalizer::standard().normalize_graph(&graph);
        assert!(normalized.edges_from(0).iter().any(|edge| {
            edge.span == InputSpan::new(0, 5)
                && edge.canonical == "zong"
                && edge.kind == SyllableKind::Complete
        }));
    }

    #[test]
    fn fuzzy_zh_to_z() {
        let n = FuzzyNormalizer::standard();
        let seg = CodeSegment {
            code: "zhong".into(),
            tag: "pinyin".into(),
        };
        let vars = n.normalize(&seg);
        assert_eq!(vars.len(), 2);
        assert_eq!(vars[0].code, "zhong");
        assert_eq!(vars[1].code, "zong");
    }

    #[test]
    fn fuzzy_multiple() {
        let n = FuzzyNormalizer::standard();
        let seg = CodeSegment {
            code: "zhang".into(),
            tag: "pinyin".into(),
        };
        let vars = n.normalize(&seg);
        // zh→z, ang→an — three variants: original, z-ang, zh-an, z-an
        assert!(vars.iter().any(|v| v.code == "zhang"));
        assert!(vars.iter().any(|v| v.code == "zang"));
    }
    #[test]
    fn abbreviation_expands_first_letter() {
        let norm = AbbreviationNormalizer::new();
        let segments = vec![
            CodeSegment {
                code: "n".into(),
                tag: "pinyin".into(),
            },
            CodeSegment {
                code: "h".into(),
                tag: "pinyin".into(),
            },
        ];
        let variants = norm.normalize_all(&segments);
        // "n" expands to ni, na, ne, nai, nan, nang, nao, nei, nen, neng, ni, nian, etc.
        assert!(
            variants.len() > 20,
            "expected many variants, got {}",
            variants.len()
        );
        // Each variant pair: [expanded_syllable, "h"]
        assert!(
            variants[0].code.len() > 1,
            "first variant should be expanded syllable"
        );
    }

    #[test]
    fn abbreviation_mixed_input_passthrough() {
        let norm = AbbreviationNormalizer::new();
        let segments = vec![
            CodeSegment {
                code: "n".into(),
                tag: "pinyin".into(),
            },
            CodeSegment {
                code: "hao".into(),
                tag: "pinyin".into(),
            },
        ];
        let variants = norm.normalize_all(&segments);
        // Not all single letters → passthrough
        assert_eq!(variants.len(), 2);
        assert_eq!(variants[0].code, "n");
        assert_eq!(variants[1].code, "hao");
    }

    #[test]
    fn abbreviation_single_segment_passthrough() {
        let norm = AbbreviationNormalizer::new();
        let segments = vec![CodeSegment {
            code: "n".into(),
            tag: "pinyin".into(),
        }];
        let variants = norm.normalize_all(&segments);
        assert_eq!(variants.len(), 1);
    }

    #[test]
    fn abbreviation_three_letter_expands_first() {
        let norm = AbbreviationNormalizer::new();
        let segments = vec![
            CodeSegment {
                code: "n".into(),
                tag: "pinyin".into(),
            },
            CodeSegment {
                code: "h".into(),
                tag: "pinyin".into(),
            },
            CodeSegment {
                code: "m".into(),
                tag: "pinyin".into(),
            },
        ];
        let variants = norm.normalize_all(&segments);
        // Should expand "n" to syllables, keep "h" and "m"
        // Variants come in groups of 3: [expanded, "h", "m"]
        assert_eq!(variants.len() % 3, 0);
        assert!(variants.len() > 30);
    }

    #[test]
    fn composite_fuzzy_then_abbreviation() {
        use super::{AbbreviationNormalizer, CompositeNormalizer, FuzzyNormalizer};
        let composite = CompositeNormalizer::new(vec![
            Box::new(AbbreviationNormalizer::new()),
            Box::new(FuzzyNormalizer::standard()),
        ]);
        // "zg" = pure abbreviation
        let segments = vec![
            CodeSegment {
                code: "z".into(),
                tag: "pinyin".into(),
            },
            CodeSegment {
                code: "g".into(),
                tag: "pinyin".into(),
            },
        ];
        let variants = composite.normalize_all(&segments);
        // Should expand "z" to zong, zuo, etc., then fuzzy should add zh variants
        assert!(
            variants
                .iter()
                .any(|v| v.code == "zhong" || v.code.starts_with("zh")),
            "composite should produce zh variants for fuzzy z/zh"
        );
    }
}
