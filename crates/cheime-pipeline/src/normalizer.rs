//! Code normalizer: expands a segment code into fuzzy/spelling variants.
//!
//! CheIME advantage: typed Normalizer component replaces Rime's
//! speller algebra regex chains (derive/xform/fuzz/abbrev/erase).
//! Each rule is a typed struct, not an opaque regex.

use crate::CodeSegment;

/// Expands a code segment into variant spellings.
/// Each variant is then translated independently by downstream translators.
pub trait CodeNormalizer: Send + Sync {
    fn name(&self) -> &str;
    fn normalize(&self, segment: &CodeSegment) -> Vec<CodeSegment>;
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
                FuzzyRule { from: "zh", to: "z" },
                FuzzyRule { from: "ch", to: "c" },
                FuzzyRule { from: "sh", to: "s" },
                FuzzyRule { from: "n", to: "l" },
                FuzzyRule { from: "l", to: "n" },
                FuzzyRule { from: "f", to: "h" },
                FuzzyRule { from: "h", to: "f" },
                FuzzyRule { from: "ang", to: "an" },
                FuzzyRule { from: "eng", to: "en" },
                FuzzyRule { from: "ing", to: "in" },
                FuzzyRule { from: "an", to: "ang" },
                FuzzyRule { from: "en", to: "eng" },
                FuzzyRule { from: "in", to: "ing" },
            ],
        }
    }

    /// Apply a single substitution to a code string.
    fn apply_rule(code: &str, from: &str, to: &str) -> Option<String> {
        if code.starts_with(from) {
            Some(format!("{to}{}", &code[from.len()..]))
        } else {
            None
        }
    }
}

impl CodeNormalizer for FuzzyNormalizer {
    fn name(&self) -> &str { "fuzzy" }

    fn normalize(&self, segment: &CodeSegment) -> Vec<CodeSegment> {
        let mut variants = Vec::new();
        variants.push(segment.clone());

        for rule in &self.rules {
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
    fn name(&self) -> &str { "passthrough" }
    fn normalize(&self, segment: &CodeSegment) -> Vec<CodeSegment> {
        vec![segment.clone()]
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_zh_to_z() {
        let n = FuzzyNormalizer::standard();
        let seg = CodeSegment { code: "zhong".into(), tag: "pinyin".into() };
        let vars = n.normalize(&seg);
        assert_eq!(vars.len(), 2);
        assert_eq!(vars[0].code, "zhong");
        assert_eq!(vars[1].code, "zong");
    }

    #[test]
    fn fuzzy_multiple() {
        let n = FuzzyNormalizer::standard();
        let seg = CodeSegment { code: "zhang".into(), tag: "pinyin".into() };
        let vars = n.normalize(&seg);
        // zh→z, ang→an — three variants: original, z-ang, zh-an, z-an
        assert!(vars.iter().any(|v| v.code == "zhang"));
        assert!(vars.iter().any(|v| v.code == "zang"));
    }
}
