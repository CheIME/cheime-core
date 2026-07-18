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
        if fields.len() != columns.len() {
            return Err(BodyError::ColumnCount {
                line: line_num + 1,
                expected: columns.len(),
                got: fields.len(),
            });
        }
        let mut text = String::new();
        let mut code = String::new();
        let mut weight = None;
        let mut stem = None;

        for (idx, col) in columns.iter().enumerate() {
            match col {
                DictColumn::Text => text = fields[idx].to_owned(),
                DictColumn::Code => code = fields[idx].to_owned(),
                DictColumn::Weight => {
                    weight =
                        Some(
                            fields[idx]
                                .parse::<i64>()
                                .map_err(|_| BodyError::InvalidWeight {
                                    line: line_num + 1,
                                    value: fields[idx].to_owned(),
                                })?,
                        );
                }
                DictColumn::Stem => stem = Some(fields[idx].to_owned()),
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
    fn rejects_wrong_column_count() {
        let columns = [DictColumn::Text, DictColumn::Code];
        let err = parse_body("你\tni\textra\n", &columns).unwrap_err();
        assert!(matches!(
            err,
            BodyError::ColumnCount {
                line: 1,
                expected: 2,
                got: 3
            }
        ));
    }

    #[test]
    fn skips_empty_lines_and_comments() {
        let columns = [DictColumn::Text, DictColumn::Code];
        let body = "# this is a comment\n你好\tni\n  \n# another comment\n世界\tshi\n";
        let entries = parse_body(body, &columns).unwrap();
        assert_eq!(entries.len(), 2);
    }
}
