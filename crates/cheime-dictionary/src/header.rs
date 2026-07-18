#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DictColumn {
    Text,
    Code,
    Weight,
    Stem,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DictHeader {
    pub name: Option<String>,
    pub version: Option<String>,
    pub sort: Option<String>,
    pub columns: Vec<DictColumn>,
    pub import_tables: Vec<String>,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum HeaderError {
    #[error("unknown header field: {0}")]
    UnknownField(String),
    #[error("unknown column name: {0}")]
    UnknownColumn(String),
    #[error("YAML parse error: {0}")]
    ParseError(String),
}

pub fn parse_header(raw_yaml: &str) -> Result<DictHeader, HeaderError> {
    let raw: HashMap<String, serde_yaml::Value> =
        serde_yaml::from_str(raw_yaml).map_err(|e| HeaderError::ParseError(e.to_string()))?;

    let known_keys = ["name", "version", "sort", "columns", "import_tables"];
    for key in raw.keys() {
        if !known_keys.contains(&key.as_str()) {
            return Err(HeaderError::UnknownField(key.clone()));
        }
    }

    let columns: Vec<String> = raw
        .get("columns")
        .map(|v| serde_yaml::from_value(v.clone()))
        .transpose()
        .map_err(|_| HeaderError::ParseError("columns: expected list of strings".into()))?
        .unwrap_or_default();

    let mut parsed_columns = Vec::new();
    for col in &columns {
        match col.as_str() {
            "text" => parsed_columns.push(DictColumn::Text),
            "code" => parsed_columns.push(DictColumn::Code),
            "weight" => parsed_columns.push(DictColumn::Weight),
            "stem" => parsed_columns.push(DictColumn::Stem),
            other => return Err(HeaderError::UnknownColumn(other.to_owned())),
        }
    }

    let import_tables: Vec<String> = raw
        .get("import_tables")
        .map(|v| serde_yaml::from_value(v.clone()))
        .transpose()
        .map_err(|_| HeaderError::ParseError("import_tables: expected list of strings".into()))?
        .unwrap_or_default();

    Ok(DictHeader {
        name: raw.get("name").and_then(|v| v.as_str()).map(String::from),
        version: raw
            .get("version")
            .and_then(|v| v.as_str())
            .map(String::from),
        sort: raw.get("sort").and_then(|v| v.as_str()).map(String::from),
        columns: parsed_columns,
        import_tables,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_header() {
        let header = parse_header("name: test\ncolumns: [text, code]\n").unwrap();
        assert_eq!(header.name.as_deref(), Some("test"));
        assert_eq!(header.columns, vec![DictColumn::Text, DictColumn::Code]);
    }

    #[test]
    fn rejects_unknown_column() {
        let err = parse_header("columns: [text, unknown_col]\n").unwrap_err();
        assert!(matches!(err, HeaderError::UnknownColumn(c) if c == "unknown_col"));
    }

    #[test]
    fn parses_import_tables() {
        let h = parse_header("columns: [text]\nimport_tables: [base.dict.yaml, ext.dict.yaml]\n")
            .unwrap();
        assert_eq!(h.import_tables, vec!["base.dict.yaml", "ext.dict.yaml"]);
    }
}
