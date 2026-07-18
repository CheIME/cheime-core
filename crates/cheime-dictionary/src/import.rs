#![forbid(unsafe_code)]

use crate::body::DictEntry;
use std::collections::{HashMap, HashSet};
use thiserror::Error;

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ImportError {
    #[error("import not found: {0}")]
    NotFound(String),
    #[error("circular import involving: {0}")]
    Cycle(String),
}

pub fn resolve_imports(
    root_name: &str,
    sources: &HashMap<String, (super::header::DictHeader, Vec<DictEntry>)>,
) -> Result<Vec<DictEntry>, ImportError> {
    let mut resolved = Vec::new();
    let mut visiting = HashSet::new();
    let mut resolved_set = HashSet::new();
    visit(
        root_name,
        sources,
        &mut resolved,
        &mut visiting,
        &mut resolved_set,
    )?;
    Ok(resolved)
}

fn visit(
    name: &str,
    sources: &HashMap<String, (super::header::DictHeader, Vec<DictEntry>)>,
    resolved: &mut Vec<DictEntry>,
    visiting: &mut HashSet<String>,
    resolved_set: &mut HashSet<String>,
) -> Result<(), ImportError> {
    if resolved_set.contains(name) {
        return Ok(());
    }
    if !visiting.insert(name.to_owned()) {
        return Err(ImportError::Cycle(name.to_owned()));
    }
    let (header, entries) = sources
        .get(name)
        .ok_or_else(|| ImportError::NotFound(name.to_owned()))?;
    for import_name in &header.import_tables {
        visit(import_name, sources, resolved, visiting, resolved_set)?;
    }
    resolved.extend(entries.iter().cloned());
    visiting.remove(name);
    resolved_set.insert(name.to_owned());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::DictHeader;

    fn entry(text: &str, code: &str) -> DictEntry {
        DictEntry {
            text: text.into(),
            code: code.into(),
            weight: None,
            stem: None,
        }
    }

    fn header(imports: Vec<&str>) -> DictHeader {
        DictHeader {
            import_tables: imports.into_iter().map(String::from).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn imports_are_prepended() {
        let mut sources = HashMap::new();
        sources.insert(
            "root".into(),
            (header(vec!["base"]), vec![entry("你", "ni")]),
        );
        sources.insert("base".into(), (header(vec![]), vec![entry("好", "hao")]));
        let result = resolve_imports("root", &sources).unwrap();
        assert_eq!(result[0].text, "好");
        assert_eq!(result[1].text, "你");
    }

    #[test]
    fn missing_import_is_error() {
        let mut sources = HashMap::new();
        sources.insert("root".into(), (header(vec!["missing"]), vec![]));
        let err = resolve_imports("root", &sources).unwrap_err();
        assert!(matches!(err, ImportError::NotFound(n) if n == "missing"));
    }

    #[test]
    fn cycle_is_detected() {
        let mut sources = HashMap::new();
        sources.insert("a".into(), (header(vec!["b"]), vec![]));
        sources.insert("b".into(), (header(vec!["a"]), vec![]));
        let err = resolve_imports("a", &sources).unwrap_err();
        assert!(matches!(err, ImportError::Cycle(_)));
    }
}
