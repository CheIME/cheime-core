#![forbid(unsafe_code)]

use cheime_dictionary::{CompiledIndex, DictColumn, parse_body};
use cheime_model::DeploymentGeneration;
use cheime_tidx::write_tidex;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn rime_ice_files() -> Vec<PathBuf> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let base = manifest.join("../../data/dicts/rime_ice_base.dict.yaml");
    if base.exists() {
        return vec![base];
    }
    let alt = manifest.join("../../../../data/dicts/rime_ice_base.dict.yaml");
    if alt.exists() {
        return vec![alt];
    }
    vec![]
}

fn extract_body(raw: &str) -> &str {
    let sep_crlf = "\r\n---\r\n";
    let sep = "\n---\n";
    let body = if let Some(p) = raw.find(sep_crlf) {
        &raw[p + sep_crlf.len()..]
    } else if let Some(p) = raw.find(sep) {
        &raw[p + sep.len()..]
    } else {
        return raw;
    };
    for marker in &["\n...\r\n", "\n...\n"] {
        if let Some(q) = body.find(marker) {
            return &body[q + marker.len()..];
        }
    }
    body
}

fn load_all_entries(files: &[PathBuf]) -> Vec<cheime_dictionary::DictEntry> {
    let columns = [DictColumn::Text, DictColumn::Code, DictColumn::Weight];
    let mut all = Vec::new();
    for path in files {
        let raw = std::fs::read_to_string(path).expect("read dict");
        let body = extract_body(&raw);
        match parse_body(body, &columns) {
            Ok(entries) => all.extend(entries),
            Err(e) => eprintln!("warning: skipping {}: {e}", path.display()),
        }
    }
    all
}

fn group_entries(entries: Vec<cheime_dictionary::DictEntry>) -> Vec<(String, Vec<(String, i32)>)> {
    let mut groups: BTreeMap<String, Vec<(String, i32)>> = BTreeMap::new();
    for e in entries {
        groups
            .entry(e.code.clone())
            .or_default()
            .push((e.text.clone(), e.weight.unwrap_or(1) as i32));
    }
    let mut result: Vec<_> = groups.into_iter().collect();
    result.sort_by(|(a, _), (b, _)| a.cmp(b));
    for (_, ents) in &mut result {
        ents.sort_by_key(|(_, w)| std::cmp::Reverse(*w));
    }
    result
}

#[test]
fn tiered_index_matches_memory_mode() {
    let files = rime_ice_files();
    if files.is_empty() {
        panic!("SKIP: no rime_ice dict files found — cannot run integration test");
    }

    eprintln!("Loading entries from {} files...", files.len());
    let entries = load_all_entries(&files);
    eprintln!("Loaded {} entries", entries.len());
    assert!(!entries.is_empty(), "should have entries");

    let mem_idx = CompiledIndex::build(entries.clone(), DeploymentGeneration::new(1));
    eprintln!(
        "Memory index: {} entries, {} hash",
        mem_idx.total_entries(),
        mem_idx.source_hash()
    );

    let hot_per_code = 5;
    let grouped = group_entries(entries);
    eprintln!("Unique codes: {}", grouped.len());

    let code_refs: Vec<(&str, &[(String, i32)])> = grouped
        .iter()
        .map(|(c, e)| (c.as_str(), e.as_slice()))
        .collect();

    let entry_count: usize = code_refs.iter().map(|(_, e)| e.len()).sum();
    eprintln!(
        "Building .tidx: {} codes, {} entries",
        code_refs.len(),
        entry_count
    );

    let tmp = tempfile::TempDir::new().unwrap();
    let tidx_path = tmp.path().join("rime_ice.tidx");
    write_tidex(&tidx_path, &code_refs).expect("write_tidex");

    let file_size = std::fs::metadata(&tidx_path).unwrap().len();
    eprintln!(
        "Wrote .tidx: {} bytes ({:.1} MB)",
        file_size,
        file_size as f64 / 1_048_576.0
    );

    let tiered_idx = CompiledIndex::build_tiered(
        grouped,
        &tidx_path,
        hot_per_code,
        mem_idx.source_hash().to_string(),
        DeploymentGeneration::new(1),
    )
    .expect("build_tiered");

    eprintln!(
        "Total entries: mem={}, tiered={}",
        mem_idx.total_entries(),
        tiered_idx.total_entries()
    );
    assert_eq!(mem_idx.total_entries(), tiered_idx.total_entries());

    let sample = [
        "ni",
        "wo",
        "ta",
        "hao",
        "zhong",
        "guo",
        "shi",
        "ren",
        "da",
        "zhong guo",
        "xue xi",
        "shi jie",
    ];

    for &code in &sample {
        let mem = mem_idx.query(code);
        let tiered = tiered_idx.query(code);
        if mem.is_empty() && tiered.is_empty() {
            continue;
        }
        assert_eq!(mem.len(), tiered.len(), "code '{}' count mismatch", code);
        for (i, (m, t)) in mem.iter().zip(tiered.iter()).enumerate() {
            assert_eq!(m.text, t.text, "code '{}'[{}] text mismatch", code, i);
            assert_eq!(
                m.annotation, t.annotation,
                "code '{}'[{}] annotation mismatch",
                code, i
            );
        }
    }

    let prefixes = ["n", "w", "t", "h", "ni", "wo", "ha"];
    for &p in &prefixes {
        let mem = mem_idx.query_prefix(p, 20);
        let tiered = tiered_idx.query_prefix(p, 20);
        let mem_texts: Vec<&str> = mem.iter().map(|c| c.text.as_str()).collect();
        let tiered_texts: Vec<&str> = tiered.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(mem_texts, tiered_texts, "prefix '{}' mismatch", p);
    }

    if let CompiledIndex::Tiered(ref t) = tiered_idx {
        eprintln!(
            "Hot codes: {}, entries: ~{}",
            t.hot_code_count(),
            t.hot_code_count() * hot_per_code
        );
        eprintln!("Cold file: {} MB", file_size as f64 / 1_048_576.0);
    }

    eprintln!("PASS: tiered index matches memory mode");
}
