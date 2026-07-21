//! OpenCC-compatible simplifier filter (DRAFT §9.7, Rime §7 L1 compat).
//!
//! CheIME advantage: the OpenCC simplifier is explicitly tagged as a
//! Rime-compat filter. Candidate source annotations mark which entries
//! went through conversion, so the user can always see the provenance.
//!
//! The native orthography path (DRAFT §9.2) generates target script forms
//! directly from lexemes — it does NOT pass through this filter.
//!
//! All conversion data is loaded from external files. Nothing is hard-coded.

use crate::Filter;
use cheime_model::Candidate;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Conversion direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conversion {
    /// Simplified → Traditional
    S2T,
    /// Traditional → Simplified
    T2S,
}

/// Error loading a conversion table.
#[derive(Clone, Debug)]
pub enum SimplifierError {
    Io(String),
    Parse(String, usize),
    Empty,
}

impl std::fmt::Display for SimplifierError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Parse(e, line) => write!(f, "parse error at line {line}: {e}"),
            Self::Empty => write!(f, "conversion table is empty"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SimplifierFilter {
    direction: Conversion,
    char_map: HashMap<char, String>,
    annotate: bool,
}

impl SimplifierFilter {
    /// Load from a TSV file: each line is `source_char<TAB>target_text`.
    /// Lines starting with `#` are comments. Blank lines are skipped.
    pub fn from_file(path: &Path, direction: Conversion, annotate: bool) -> Result<Self, SimplifierError> {
        let content = fs::read_to_string(path).map_err(|e| SimplifierError::Io(e.to_string()))?;
        Self::parse(&content, direction, annotate)
    }

    /// Load from an in-memory TSV string (for embedded configs, test fixtures, etc.).
    pub fn parse(source: &str, direction: Conversion, annotate: bool) -> Result<Self, SimplifierError> {
        let mut map = HashMap::new();
        for (idx, line) in source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = trimmed.splitn(2, '\t').collect();
            if parts.len() < 2 {
                return Err(SimplifierError::Parse(
                    "expected <char><TAB><text>".into(), idx + 1));
            }
            let source_char = parts[0].chars().next().ok_or_else(|| {
                SimplifierError::Parse("empty source char".into(), idx + 1)
            })?;
            let target = parts[1].to_owned();
            map.insert(source_char, target);
        }
        if map.is_empty() {
            return Err(SimplifierError::Empty);
        }
        Ok(Self { direction, char_map: map, annotate })
    }

    /// Build from a pre-populated HashMap (for programmatic construction / tests).
    pub fn from_table(table: HashMap<char, String>, direction: Conversion, annotate: bool) -> Self {
        Self { direction, char_map: table, annotate }
    }

    fn convert(&self, text: &str) -> String {
        let mut result = String::with_capacity(text.len() * 3);
        for ch in text.chars() {
            if let Some(replacement) = self.char_map.get(&ch) {
                result.push_str(replacement);
            } else {
                result.push(ch);
            }
        }
        result
    }
}

impl Filter for SimplifierFilter {
    fn name(&self) -> &str {
        match self.direction {
            Conversion::S2T => "simplifier@s2t",
            Conversion::T2S => "simplifier@t2s",
        }
    }

    fn filter(&self, candidates: Vec<Candidate>) -> Vec<Candidate> {
        let mut seen: HashMap<String, bool> = HashMap::new();
        let mut result = Vec::with_capacity(candidates.len());

        for mut c in candidates {
            let converted = self.convert(&c.text);
            if converted == c.text {
                result.push(c);
                continue;
            }
            if self.annotate {
                let tag = match self.direction {
                    Conversion::S2T => "simplified",
                    Conversion::T2S => "traditional",
                };
                c.source = format!("{}→{}", c.source, tag);
            }
            c.text = converted;
            if seen.contains_key(&c.text) {
                continue;
            }
            seen.insert(c.text.clone(), true);
            result.push(c);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cheime_model::CandidateId;

    fn sample_s2t() -> SimplifierFilter {
        let mut map = HashMap::new();
        map.insert('爱', "愛".into());
        map.insert('国', "國".into());
        map.insert('发', "發".into());
        map.insert('门', "門".into());
        SimplifierFilter::from_table(map, Conversion::S2T, false)
    }

    fn sample_with_annotate() -> SimplifierFilter {
        let mut map = HashMap::new();
        map.insert('爱', "愛".into());
        map.insert('国', "國".into());
        SimplifierFilter::from_table(map, Conversion::S2T, true)
    }

    #[test]
    fn s2t_converts_text() {
        let f = sample_s2t();
        let cands = vec![Candidate::text(CandidateId::new(1), "爱国", "dict")];
        let result = f.filter(cands);
        assert_eq!(result[0].text, "愛國");
        assert_eq!(result[0].source, "dict");
    }

    #[test]
    fn s2t_annotates_source() {
        let f = sample_with_annotate();
        let cands = vec![Candidate::text(CandidateId::new(1), "爱国", "dict:abc")];
        let result = f.filter(cands);
        assert_eq!(result[0].text, "愛國");
        assert!(result[0].source.contains("simplified"), "source: {}", result[0].source);
    }

    #[test]
    fn t2s_converts_traditional() {
        let mut map = HashMap::new();
        map.insert('愛', "爱".into());
        map.insert('國', "国".into());
        let f = SimplifierFilter::from_table(map, Conversion::T2S, false);
        let cands = vec![Candidate::text(CandidateId::new(1), "愛國", "dict")];
        let result = f.filter(cands);
        assert_eq!(result[0].text, "爱国");
    }

    #[test]
    fn empty_candidates_passthrough() {
        let f = sample_s2t();
        assert!(f.filter(vec![]).is_empty());
    }

    #[test]
    fn no_conversion_needed() {
        let f = sample_with_annotate();
        let cands = vec![Candidate::text(CandidateId::new(1), "hello", "dict")];
        let result = f.filter(cands);
        assert_eq!(result[0].text, "hello");
        assert_eq!(result[0].source, "dict");
    }

    #[test]
    fn parse_tsv_table() {
        let tsv = "# comment\n爱\t愛\n国\t國\n";
        let f = SimplifierFilter::parse(tsv, Conversion::S2T, false).unwrap();
        let cands = vec![Candidate::text(CandidateId::new(1), "爱国", "dict")];
        assert_eq!(f.filter(cands)[0].text, "愛國");
    }

    #[test]
    fn parse_rejects_malformed() {
        assert!(SimplifierFilter::parse("a\n", Conversion::S2T, false).is_err());
    }

    #[test]
    fn parse_rejects_empty() {
        assert!(matches!(
            SimplifierFilter::parse("# only comments\n", Conversion::S2T, false),
            Err(SimplifierError::Empty)
        ));
    }
}
