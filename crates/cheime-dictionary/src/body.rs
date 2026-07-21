#![forbid(unsafe_code)]

use crate::header::DictColumn;
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DictEntry {
    pub text: String,
    pub code: String,
    pub weight: Option<i64>,
    pub stem: Option<String>,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum BodyError {
    #[error("line {line}: expected {expected} columns, got {got}")]
    ColumnCount {
        line: usize,
        expected: usize,
        got: usize,
    },
    #[error("line {line}: invalid weight: {value}")]
    InvalidWeight { line: usize, value: String },
}

pub fn parse_body(lines: &str, columns: &[DictColumn]) -> Result<Vec<DictEntry>, BodyError> {
    let mut entries = Vec::new();
    for (line_num, raw) in lines.lines().enumerate() {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = trimmed.split('\t').collect();
        if fields.len() < 2 {
            return Err(BodyError::ColumnCount { line: line_num + 1, expected: 2, got: fields.len() });
        }
        let mut text = String::new();
        let mut code = String::new();
        let mut weight = Some(1i64);
        let mut stem = None;

        for (idx, col) in columns.iter().enumerate() {
            let val = fields.get(idx).copied().unwrap_or("");
            match col {
                DictColumn::Text => text = val.to_owned(),
                DictColumn::Code => code = val.to_owned(),
                DictColumn::Weight => {
                    if idx < fields.len() && !val.is_empty() {
                        weight = val.parse::<i64>().ok();
                    }
                }
                DictColumn::Stem => {
                    if !val.is_empty() { stem = Some(val.to_owned()); }
                }
            }
        }
        entries.push(DictEntry {
            text,
            code,
            weight,
            stem,
        });
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_text_code_lines() {
        let columns = [DictColumn::Text, DictColumn::Code];
        let body = "你好\tni hao\n世界\tshi jie\n";
        let entries = parse_body(body, &columns).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].text, "你好");
        assert_eq!(entries[0].code, "ni hao");
    }

    #[test]
    fn parses_weight_column() {
        let columns = [DictColumn::Text, DictColumn::Code, DictColumn::Weight];
        let entries = parse_body("你\tni\t100\n", &columns).unwrap();
        assert_eq!(entries[0].weight, Some(100));
    }

    #[test]
    fn accepts_extra_columns() {
        let columns = [DictColumn::Text, DictColumn::Code];
        let entries = parse_body("你\tni\textra\n", &columns).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].text, "你");
    }

    #[test]
    fn skips_empty_lines_and_comments() {
        let columns = [DictColumn::Text, DictColumn::Code];
        let body = "# this is a comment\n你好\tni\n  \n# another comment\n世界\tshi\n";
        let entries = parse_body(body, &columns).unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn empty_body_returns_empty() {
        let columns = [DictColumn::Text, DictColumn::Code];
        let entries = parse_body("", &columns).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn malformed_line_is_reported() {
        let columns = [DictColumn::Text, DictColumn::Code];
        // Only one field: rejected (minimum 2 columns)
        let err = parse_body("你好\n", &columns).unwrap_err();
        assert!(matches!(err, BodyError::ColumnCount { expected: 2, got: 1, .. }));
    }

    #[test]
    fn zero_weight_entry_is_ok() {
        let columns = [DictColumn::Text, DictColumn::Code, DictColumn::Weight];
        let entries = parse_body("你\tni\t0\n", &columns).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].weight, Some(0));
    }

    #[test]
    fn duplicate_code_preserves_all_entries() {
        let columns = [DictColumn::Text, DictColumn::Code];
        let entries = parse_body("你\tni\n您\tni\n", &columns).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].text, "你");
        assert_eq!(entries[0].code, "ni");
        assert_eq!(entries[1].text, "您");
        assert_eq!(entries[1].code, "ni");
    }
}
