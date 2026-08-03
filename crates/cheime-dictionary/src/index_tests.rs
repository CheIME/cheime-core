use super::*;

fn entry(text: &str, code: &str, weight: i64) -> DictEntry {
    DictEntry {
        text: text.into(),
        code: code.into(),
        weight: Some(weight),
        stem: None,
    }
}

#[test]
fn sorts_by_weight_desc_then_text_asc() {
    let entries = vec![
        entry("你", "ni", 100),
        entry("呢", "ni", 90),
        entry("拟", "ni", 80),
    ];
    let idx = CompiledIndex::build(entries, DeploymentGeneration::new(1));
    let candidates = idx.query("ni");
    assert_eq!(candidates[0].text, "你");
    assert_eq!(candidates[1].text, "呢");
    assert_eq!(candidates[2].text, "拟");
}

#[test]
fn prefix_search_ni_matches_ni_and_ni_hao() {
    let entries = vec![
        entry("你", "ni", 100),
        entry("你好", "ni hao", 200),
        entry("那里", "na li", 50),
    ];
    let idx = CompiledIndex::build(entries, DeploymentGeneration::new(1));
    let candidates = idx.query_prefix("ni", 10);
    assert_eq!(candidates.len(), 2);
    assert!(candidates.iter().any(|candidate| candidate.text == "你"));
    assert!(candidates.iter().any(|candidate| candidate.text == "你好"));
    assert!(!candidates.iter().any(|candidate| candidate.text == "那里"));
}

#[test]
fn prefix_search_n_matches_multiple_initials() {
    let entries = vec![
        entry("那", "na", 100),
        entry("你", "ni", 90),
        entry("女", "nv", 80),
        entry("年", "nian", 70),
    ];
    let idx = CompiledIndex::build(entries, DeploymentGeneration::new(1));
    let candidates = idx.query_prefix("n", 10);
    assert_eq!(candidates.len(), 4);
    assert_eq!(candidates[0].text, "那");
}

#[test]
fn prefix_search_breaks_equal_weight_ties_by_text() {
    let entries = vec![entry("b", "ni hao", 100), entry("a", "ni", 100)];
    let idx = CompiledIndex::build(entries, DeploymentGeneration::new(1));

    let candidates = idx.query_prefix("ni", 10);
    let texts: Vec<&str> = candidates
        .iter()
        .map(|candidate| candidate.text.as_str())
        .collect();

    assert_eq!(texts, ["a", "b"]);
}

#[test]
fn assigns_stable_candidate_ids() {
    let entries = vec![entry("你", "ni", 100), entry("好", "hao", 100)];
    let idx1 = CompiledIndex::build(entries.clone(), DeploymentGeneration::new(1));
    let idx2 = CompiledIndex::build(entries, DeploymentGeneration::new(1));
    assert_eq!(idx1.query("ni")[0].id, idx2.query("ni")[0].id);
}

#[test]
fn empty_query_returns_empty() {
    let idx = CompiledIndex::build(vec![], DeploymentGeneration::new(1));
    assert!(idx.query("nonexistent").is_empty());
    assert!(idx.query_prefix("x", 10).is_empty());
}

#[test]
fn exact_limit_counts_unique_texts_across_merged_sources() {
    let entries = vec![
        entry("真", "zhen", 300),
        entry("真", "zhen", 300),
        entry("阵", "zhen", 200),
        entry("阵", "zhen", 200),
        entry("震", "zhen", 100),
    ];
    let idx = CompiledIndex::build(entries, DeploymentGeneration::new(1));

    let candidates = idx.lookup_exact_limited("zhen", 3);
    let texts: Vec<_> = candidates.iter().map(|entry| entry.text.as_str()).collect();
    assert_eq!(texts, ["真", "阵", "震"]);
}

#[test]
fn longer_code_requires_a_syllable_boundary() {
    let entries = vec![
        entry("你", "ni", 100),
        entry("你好", "ni hao", 200),
        entry("泥泞", "ning", 50),
    ];
    let idx = CompiledIndex::build(entries, DeploymentGeneration::new(1));

    assert!(idx.has_longer_code("ni"));
    assert!(!idx.has_longer_code("nin"));
    assert!(!idx.has_longer_code("ni hao"));
}

#[test]
fn bounded_prefix_search_retains_the_global_top_k() {
    let entries = vec![
        entry("低一", "na", 1),
        entry("最高", "nan", 100),
        entry("低二", "nang", 2),
        entry("次高", "nao", 90),
    ];
    let idx = CompiledIndex::build(entries, DeploymentGeneration::new(1));

    let candidates = idx.lookup_prefix("n", 2);
    let texts: Vec<_> = candidates.iter().map(|entry| entry.text.as_str()).collect();
    assert_eq!(texts, ["最高", "次高"]);
}
