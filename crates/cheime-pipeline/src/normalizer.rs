//! Code normalizer: expands a segment code into fuzzy/spelling variants.
//!
//! Each normalizer returns `Vec<Vec<CodeSegment>>` — a list of alternative
//! segment sequences. Each alternative is translated independently by
//! downstream translators. The original input is always included as the
//! first alternative.

use crate::CodeSegment;
use std::collections::HashMap;

/// Expands code segments into alternative segment sequences.
/// Each alternative is translated independently by downstream translators.
pub trait CodeNormalizer: Send + Sync {
    fn name(&self) -> &str;

    /// Normalize a single segment into alternatives (per-segment expansion).
    fn normalize(&self, segment: &CodeSegment) -> Vec<CodeSegment>;

    /// Normalize all segments together, producing alternative segment sequences.
    /// Each inner Vec is an independent alternative to be translated separately.
    /// Default: cross-product of per-segment normalize results.
    fn normalize_all(&self, segments: &[CodeSegment]) -> Vec<Vec<CodeSegment>> {
        if segments.is_empty() {
            return vec![vec![]];
        }
        // Start with the original as the only alternative
        let mut alternatives: Vec<Vec<CodeSegment>> = vec![segments.to_vec()];
        for (i, seg) in segments.iter().enumerate() {
            let variants = self.normalize(seg);
            if variants.len() <= 1 {
                continue; // no expansion for this segment
            }
            let mut new_alts = Vec::new();
            for alt in &alternatives {
                for variant in &variants {
                    let mut new_alt = alt.clone();
                    new_alt[i] = variant.clone();
                    new_alts.push(new_alt);
                }
            }
            alternatives = new_alts;
        }
        alternatives
    }
}

// ── Fuzzy pinyin ───────────────────────────────────────────────────

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
    /// Standard fuzzy rules covering both prefix (consonant) and suffix (vowel/final) patterns.
    pub fn standard() -> Self {
        Self {
            rules: vec![
                // Consonant fuzzy (prefix match)
                FuzzyRule { from: "zh", to: "z" },
                FuzzyRule { from: "z", to: "zh" },
                FuzzyRule { from: "ch", to: "c" },
                FuzzyRule { from: "c", to: "ch" },
                FuzzyRule { from: "sh", to: "s" },
                FuzzyRule { from: "s", to: "sh" },
                FuzzyRule { from: "n", to: "l" },
                FuzzyRule { from: "l", to: "n" },
                FuzzyRule { from: "f", to: "h" },
                FuzzyRule { from: "h", to: "f" },
                // Vowel/final fuzzy (suffix match)
                FuzzyRule { from: "ang", to: "an" },
                FuzzyRule { from: "an", to: "ang" },
                FuzzyRule { from: "eng", to: "en" },
                FuzzyRule { from: "en", to: "eng" },
                FuzzyRule { from: "ing", to: "in" },
                FuzzyRule { from: "in", to: "ing" },
            ],
        }
    }

    /// Apply a substitution rule. Tries prefix match first, then suffix match.
    fn apply_rule(code: &str, from: &str, to: &str) -> Option<String> {
        if code == from {
            return Some(to.to_string());
        }
        // Prefix match: "zhong" with zh→z → "zong"
        // Skip if code already starts with target (avoids z→zh on "zhong")
        if code.starts_with(from) && code.len() > from.len() && !(code.starts_with(to) && to.starts_with(from)) {
            return Some(format!("{to}{}", &code[from.len()..]));
        }
        // Suffix match: "bang" with ang→an → "ban"
        if code.ends_with(from) && code.len() > from.len() {
            let prefix = &code[..code.len() - from.len()];
            return Some(format!("{prefix}{to}"));
        }
        None
    }

    pub fn from_rules(rule_names: &[String]) -> Self {
        let all = Self::standard();
        let rules: Vec<FuzzyRule> = all.rules.into_iter()
            .filter(|r| {
                let key = format!("{}_{}", r.from, r.to);
                rule_names.iter().any(|name| name == &key)
            })
            .collect();
        Self { rules }
    }
}

impl CodeNormalizer for FuzzyNormalizer {
    fn name(&self) -> &str { "fuzzy" }

    fn normalize(&self, segment: &CodeSegment) -> Vec<CodeSegment> {
        let mut variants = vec![segment.clone()];
        for rule in &self.rules {
            if let Some(variant_code) = Self::apply_rule(&segment.code, rule.from, rule.to) {
                if !variants.iter().any(|v| v.code == variant_code) {
                    variants.push(CodeSegment {
                        code: variant_code,
                        tag: format!("{}-fuzzy", segment.tag),
                    });
                }
            }
        }
        variants
    }
}

// ── Passthrough ────────────────────────────────────────────────────

#[derive(Clone, Debug, Default)]
pub struct PassthroughNormalizer;

impl CodeNormalizer for PassthroughNormalizer {
    fn name(&self) -> &str { "passthrough" }
    fn normalize(&self, segment: &CodeSegment) -> Vec<CodeSegment> {
        vec![segment.clone()]
    }
}

// ── Abbreviation (简拼) ────────────────────────────────────────────

/// When ALL segments are single letters (pure abbreviation like "nh", "zg"),
/// expands the first segment to all possible pinyin syllables starting with
/// that letter. Each expansion produces a separate alternative sequence.
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
    fn default() -> Self { Self::new() }
}

impl CodeNormalizer for AbbreviationNormalizer {
    fn name(&self) -> &str { "abbreviation" }

    fn normalize(&self, segment: &CodeSegment) -> Vec<CodeSegment> {
        vec![segment.clone()]
    }

    fn normalize_all(&self, segments: &[CodeSegment]) -> Vec<Vec<CodeSegment>> {
        // Only activate for pure abbreviation: all segments single letters, >= 2 segments
        if segments.len() < 2 || !segments.iter().all(|s| s.code.len() == 1) {
            return vec![segments.to_vec()];
        }

        let first_letter = match segments[0].code.chars().next() {
            Some(c) => c,
            None => return vec![segments.to_vec()],
        };
        let expansions = match self.by_initial.get(&first_letter) {
            Some(v) => v,
            None => return vec![segments.to_vec()],
        };

        let mut alternatives = Vec::with_capacity(expansions.len());
        for expanded in expansions {
            let mut alt = Vec::with_capacity(segments.len());
            alt.push(CodeSegment {
                code: expanded.clone(),
                tag: format!("{}-abbrev", segments[0].tag),
            });
            for s in &segments[1..] {
                alt.push(s.clone());
            }
            alternatives.push(alt);
        }
        alternatives
    }
}

// ── Composite (组合) ────────────────────────────────────────────────

/// Chains multiple normalizers sequentially.
pub struct CompositeNormalizer {
    normalizers: Vec<Box<dyn CodeNormalizer>>,
}

impl CompositeNormalizer {
    pub fn new(normalizers: Vec<Box<dyn CodeNormalizer>>) -> Self {
        Self { normalizers }
    }
}

impl CodeNormalizer for CompositeNormalizer {
    fn name(&self) -> &str { "composite" }

    fn normalize(&self, segment: &CodeSegment) -> Vec<CodeSegment> {
        let mut current = vec![segment.clone()];
        for norm in &self.normalizers {
            current = current.iter().flat_map(|s| norm.normalize(s)).collect();
        }
        current
    }

    fn normalize_all(&self, segments: &[CodeSegment]) -> Vec<Vec<CodeSegment>> {
        let mut current: Vec<Vec<CodeSegment>> = vec![segments.to_vec()];
        for norm in &self.normalizers {
            let mut next = Vec::new();
            for alt in &current {
                next.extend(norm.normalize_all(alt));
            }
            current = next;
        }
        current
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_zh_to_z_prefix() {
        let n = FuzzyNormalizer::standard();
        let seg = CodeSegment { code: "zhong".into(), tag: "pinyin".into() };
        let vars = n.normalize(&seg);
        assert!(vars.iter().any(|v| v.code == "zhong"));
        assert!(vars.iter().any(|v| v.code == "zong"));
    }

    #[test]
    fn fuzzy_z_to_zh_reverse() {
        let n = FuzzyNormalizer::standard();
        let seg = CodeSegment { code: "zong".into(), tag: "pinyin".into() };
        let vars = n.normalize(&seg);
        assert!(vars.iter().any(|v| v.code == "zong"));
        assert!(vars.iter().any(|v| v.code == "zhong"));
    }

    #[test]
    fn fuzzy_ang_suffix() {
        let n = FuzzyNormalizer::standard();
        let seg = CodeSegment { code: "bang".into(), tag: "pinyin".into() };
        let vars = n.normalize(&seg);
        assert!(vars.iter().any(|v| v.code == "bang"), "should keep original");
        assert!(vars.iter().any(|v| v.code == "ban"), "ang→an suffix should fire");
    }

    #[test]
    fn fuzzy_an_to_ang_reverse() {
        let n = FuzzyNormalizer::standard();
        let seg = CodeSegment { code: "ban".into(), tag: "pinyin".into() };
        let vars = n.normalize(&seg);
        assert!(vars.iter().any(|v| v.code == "ban"));
        assert!(vars.iter().any(|v| v.code == "bang"));
    }

    #[test]
    fn fuzzy_normalize_all_produces_independent_alternatives() {
        let n = FuzzyNormalizer::standard();
        let segments = vec![CodeSegment { code: "zhong".into(), tag: "pinyin".into() }];
        let alts = n.normalize_all(&segments);
        // Should produce [["zhong"], ["zong"]] — two independent alternatives
        assert_eq!(alts.len(), 2);
        assert!(alts.iter().any(|a| a.len() == 1 && a[0].code == "zhong"));
        assert!(alts.iter().any(|a| a.len() == 1 && a[0].code == "zong"));
    }

    #[test]
    fn abbreviation_expands_to_independent_alternatives() {
        let norm = AbbreviationNormalizer::new();
        let segments = vec![
            CodeSegment { code: "n".into(), tag: "pinyin".into() },
            CodeSegment { code: "h".into(), tag: "pinyin".into() },
        ];
        let alts = norm.normalize_all(&segments);
        assert!(alts.len() > 20, "expected many alternatives, got {}", alts.len());
        // Each alternative should be [expanded_syllable, "h"]
        for alt in &alts {
            assert_eq!(alt.len(), 2);
            assert_eq!(alt[1].code, "h");
            assert!(alt[0].code.len() > 1);
        }
        // Should contain "ni" + "h"
        assert!(alts.iter().any(|a| a[0].code == "ni"), "should contain ni expansion");
    }

    #[test]
    fn abbreviation_mixed_input_passthrough() {
        let norm = AbbreviationNormalizer::new();
        let segments = vec![
            CodeSegment { code: "n".into(), tag: "pinyin".into() },
            CodeSegment { code: "hao".into(), tag: "pinyin".into() },
        ];
        let alts = norm.normalize_all(&segments);
        assert_eq!(alts.len(), 1, "mixed input should passthrough");
        assert_eq!(alts[0][0].code, "n");
        assert_eq!(alts[0][1].code, "hao");
    }

    #[test]
    fn composite_chains_correctly() {
        let composite = CompositeNormalizer::new(vec![
            Box::new(AbbreviationNormalizer::new()),
            Box::new(FuzzyNormalizer::standard()),
        ]);
        // "zg" = pure abbreviation
        let segments = vec![
            CodeSegment { code: "z".into(), tag: "pinyin".into() },
            CodeSegment { code: "g".into(), tag: "pinyin".into() },
        ];
        let alts = composite.normalize_all(&segments);
        // Should contain both z→zh fuzzy expansions and abbreviation expansions
        assert!(alts.iter().any(|a| a[0].code == "zong"), "should contain zong");
        assert!(alts.iter().any(|a| a[0].code == "zhong"), "fuzzy z→zh should add zhong");
    }
}
