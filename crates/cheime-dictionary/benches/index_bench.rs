//! Benchmarks for 检字 (dictionary lookup) and 拼音切分 (pinyin segmentation).
//!
//! # Performance targets (from DRAFT.md §17.4)
//! - Per-key P50:     < 1ms
//! - Per-key P95/P99: < 10ms
//! - First-screen candidate latency: < 1ms
//! - Large dictionary query: stable, predictable
//!
//! Uses the real Rime luna_pinyin dictionary (~55K entries, 962KB) for realistic
//! benchmarking. Data loaded once via `OnceLock` to amortize I/O.
//!
//! # Bench groups
//! - **dict/build_***: CompiledIndex construction at deploy time
//! - **dict/query_***: single code → candidates (the hot path)
//! - **dict/typing_***: multi-keystroke typing simulation (real-world pattern)
//! - **seg/***: pinyin syllable segmentation (prefix-tree based)

use cheime_dictionary::{CompiledIndex, DictColumn, DictEntry, parse_body};
use cheime_model::DeploymentGeneration;
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use std::sync::OnceLock;

// ── Dict body extraction (handles CRLF/LF) ───────────────────────────

/// Extract the tab-separated body from a Rime .dict.yaml file, skipping
/// the YAML frontmatter (--- ... ---). Handles both LF and CRLF.
fn dict_body(raw: &str) -> &str {
    for line in raw.lines() {
        if line.trim() == "..." {
            let byte_offset = line.as_ptr() as usize - raw.as_ptr() as usize;
            let line_end = byte_offset + line.len();
            let remaining = &raw[line_end..];
            let skip = remaining
                .chars()
                .take_while(|c| *c == '\r' || *c == '\n')
                .map(|c| c.len_utf8())
                .sum::<usize>();
            return &raw[line_end + skip..];
        }
    }
    raw
}

// ── Real dictionary loading (once, amortized) ────────────────────────

static REAL_INDEX: OnceLock<(CompiledIndex, Vec<String>)> = OnceLock::new();

fn real_index() -> &'static (CompiledIndex, Vec<String>) {
    REAL_INDEX.get_or_init(|| {
        let raw = include_str!("../../../data/dicts/luna_pinyin.dict.yaml");
        let body = dict_body(raw);
        // Luna pinyin has mixed 2-column and 3-column (percentage weight) lines.
        // Filter to 2-column only — clean entries without percentage weights.
        let body_2col: String = body
            .lines()
            .filter(|l| {
                let t = l.trim();
                if t.is_empty() || t.starts_with('#') {
                    return false;
                }
                t.split('\t').count() == 2
            })
            .collect::<Vec<_>>()
            .join("\n");
        let columns = &[DictColumn::Text, DictColumn::Code];

        let entries = parse_body(&body_2col, columns).expect("failed to parse luna_pinyin body");
        let count = entries.len();
        let index = CompiledIndex::build(entries, DeploymentGeneration::new(1));

        let mut codes: Vec<String> = Vec::with_capacity(count / 4);
        let mut seen = String::new();
        for line in body.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Some(code) = trimmed.split('\t').nth(1) {
                if seen != code {
                    codes.push(code.to_owned());
                    seen = code.to_owned();
                }
            }
        }

        eprintln!(
            "loaded luna_pinyin: {} entries, {} unique codes, {} entries in index",
            count,
            codes.len(),
            index.total_entries(),
        );
        (index, codes)
    })
}

// ── Rime Ice dictionary loading (539K entries, stress test) ──────────

static RIME_ICE_INDEX: OnceLock<(CompiledIndex, Vec<String>)> = OnceLock::new();

fn rime_ice_index() -> &'static (CompiledIndex, Vec<String>) {
    RIME_ICE_INDEX.get_or_init(|| {
        let raw = include_str!("../../../data/dicts/rime_ice_base.dict.yaml");
        let body = dict_body(raw);
        let columns = &[DictColumn::Text, DictColumn::Code, DictColumn::Weight];

        let entries = parse_body(body, columns).expect("failed to parse rime_ice body");
        let count = entries.len();
        let index = CompiledIndex::build(entries, DeploymentGeneration::new(1));

        let mut codes: Vec<String> = Vec::with_capacity(count / 4);
        let mut seen = String::new();
        for line in body.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Some(code) = trimmed.split('\t').nth(1) {
                if seen != code {
                    codes.push(code.to_owned());
                    seen = code.to_owned();
                }
            }
        }

        eprintln!(
            "loaded rime_ice: {} entries, {} unique codes, {} entries in index",
            count,
            codes.len(),
            index.total_entries(),
        );
        (index, codes)
    })
}

// ── Dict: build benchmarks ──────────────────────────────────────────

fn bench_build_real_dict(c: &mut Criterion) {
    let raw = include_str!("../../../data/dicts/luna_pinyin.dict.yaml");
    let body = dict_body(raw);
    let body_2col: String = body
        .lines()
        .filter(|l| {
            let t = l.trim();
            if t.is_empty() || t.starts_with('#') {
                return false;
            }
            t.split('\t').count() == 2
        })
        .collect::<Vec<_>>()
        .join("\n");
    let columns = &[DictColumn::Text, DictColumn::Code];

    c.bench_function("dict/build_luna_pinyin_55k", |b| {
        b.iter(|| {
            let entries = parse_body(black_box(&body_2col), black_box(columns)).unwrap();
            black_box(CompiledIndex::build(
                entries,
                black_box(DeploymentGeneration::new(1)),
            ))
        })
    });
}

fn bench_build_synthetic_100k(c: &mut Criterion) {
    let entries: Vec<DictEntry> = (0..100_000)
        .map(|i| {
            let syllable = SYNTHETIC_CODES[i % SYNTHETIC_CODES.len()];
            DictEntry {
                text: format!("词{}", i),
                code: syllable.to_owned(),
                weight: Some((100_000 - i) as i64),
                stem: None,
            }
        })
        .collect();

    c.bench_function("dict/build_synthetic_100k", |b| {
        b.iter(|| {
            black_box(CompiledIndex::build(
                black_box(entries.clone()),
                black_box(DeploymentGeneration::new(1)),
            ))
        })
    });
}

// ── Dict: query benchmarks ──────────────────────────────────────────

fn bench_query_short_code(c: &mut Criterion) {
    let (index, _codes) = real_index();
    c.bench_function("dict/query_short_code", |b| {
        b.iter(|| black_box(index.query(black_box("zhong"))))
    });
}

fn bench_query_long_code(c: &mut Criterion) {
    let (index, _codes) = real_index();
    c.bench_function("dict/query_long_code", |b| {
        b.iter(|| black_box(index.query(black_box("zhuang"))))
    });
}

fn bench_query_miss(c: &mut Criterion) {
    let (index, _codes) = real_index();
    c.bench_function("dict/query_miss", |b| {
        b.iter(|| black_box(index.query(black_box("zzz"))))
    });
}

/// Query every unique code in the real dict — measures throughput across
/// the full code space (hot, warm, cold cache patterns mixed).
fn bench_query_all_codes(c: &mut Criterion) {
    let (index, codes) = real_index();
    c.bench_function("dict/query_all_codes_batch", |b| {
        b.iter(|| {
            for code in codes {
                black_box(index.query(black_box(code.as_str())));
            }
        })
    });
}

// ── Dict: rime_ice stress benchmarks (539K entries) ──────────────────

fn bench_build_rime_ice_539k(c: &mut Criterion) {
    let raw = include_str!("../../../data/dicts/rime_ice_base.dict.yaml");
    let body = dict_body(raw);
    let columns = &[DictColumn::Text, DictColumn::Code, DictColumn::Weight];

    c.bench_function("dict/build_rime_ice_539k", |b| {
        b.iter(|| {
            let entries = parse_body(black_box(body), black_box(columns)).unwrap();
            black_box(CompiledIndex::build(
                entries,
                black_box(DeploymentGeneration::new(1)),
            ))
        })
    });
}

fn bench_query_rime_ice_short(c: &mut Criterion) {
    let (index, _codes) = rime_ice_index();
    c.bench_function("dict/query_rime_ice_short", |b| {
        b.iter(|| black_box(index.query(black_box("ni hao"))))
    });
}

fn bench_query_rime_ice_long(c: &mut Criterion) {
    let (index, _codes) = rime_ice_index();
    c.bench_function("dict/query_rime_ice_long", |b| {
        b.iter(|| black_box(index.query(black_box("zhong hua ren min gong he guo"))))
    });
}

fn bench_query_rime_ice_miss(c: &mut Criterion) {
    let (index, _codes) = rime_ice_index();
    c.bench_function("dict/query_rime_ice_miss", |b| {
        b.iter(|| black_box(index.query(black_box("zzz zzz"))))
    });
}

fn bench_query_rime_ice_all_codes(c: &mut Criterion) {
    let (index, codes) = rime_ice_index();
    c.bench_function("dict/query_rime_ice_all_codes", |b| {
        b.iter(|| {
            for code in codes {
                black_box(index.query(black_box(code.as_str())));
            }
        })
    });
}

// ── Dict: typing simulation ─────────────────────────────────────────

/// Simulate typing "zhongguo" — 8 keystrokes, each queries the current
/// composition prefix. This is the real-world user-facing latency path.
fn bench_typing_zhongguo(c: &mut Criterion) {
    let (index, _codes) = real_index();
    let prefixes = [
        "z", "zh", "zho", "zhon", "zhong", "zhongg", "zhonggu", "zhongguo",
    ];
    c.bench_function("dict/typing_zhongguo_8keys", |b| {
        b.iter(|| {
            for prefix in &prefixes {
                black_box(index.query(black_box(prefix)));
            }
        })
    });
}

fn bench_typing_zhonghuarenmin(c: &mut Criterion) {
    let (index, _codes) = real_index();
    let s = "zhonghuarenmin";
    let prefixes: Vec<&str> = (1..=s.len()).map(|i| &s[..i]).collect();
    c.bench_function("dict/typing_zhonghuarenmin_14keys", |b| {
        b.iter(|| {
            for prefix in &prefixes {
                black_box(index.query(black_box(prefix)));
            }
        })
    });
}

// ── Pinyin segmentation benchmarks ──────────────────────────────────

/// All valid Hanyu Pinyin syllables (without tones).
const PINYIN_SYLLABLES: &[&str] = &[
    "a", "ai", "an", "ang", "ao", "ba", "bai", "ban", "bang", "bao", "bei", "ben", "beng", "bi",
    "bian", "biao", "bie", "bin", "bing", "bo", "bu", "ca", "cai", "can", "cang", "cao", "ce",
    "cen", "ceng", "cha", "chai", "chan", "chang", "chao", "che", "chen", "cheng", "chi", "chong",
    "chou", "chu", "chua", "chuai", "chuan", "chuang", "chui", "chun", "chuo", "ci", "cong", "cou",
    "cu", "cuan", "cui", "cun", "cuo", "da", "dai", "dan", "dang", "dao", "de", "dei", "den",
    "deng", "di", "dian", "diao", "die", "ding", "diu", "dong", "dou", "du", "duan", "dui", "dun",
    "duo", "e", "ei", "en", "eng", "er", "fa", "fan", "fang", "fei", "fen", "feng", "fo", "fou",
    "fu", "ga", "gai", "gan", "gang", "gao", "ge", "gei", "gen", "geng", "gong", "gou", "gu",
    "gua", "guai", "guan", "guang", "gui", "gun", "guo", "ha", "hai", "han", "hang", "hao", "he",
    "hei", "hen", "heng", "hong", "hou", "hu", "hua", "huai", "huan", "huang", "hui", "hun", "huo",
    "ji", "jia", "jian", "jiang", "jiao", "jie", "jin", "jing", "jiong", "jiu", "ju", "juan",
    "jue", "jun", "ka", "kai", "kan", "kang", "kao", "ke", "ken", "keng", "kong", "kou", "ku",
    "kua", "kuai", "kuan", "kuang", "kui", "kun", "kuo", "la", "lai", "lan", "lang", "lao", "le",
    "lei", "leng", "li", "lia", "lian", "liang", "liao", "lie", "lin", "ling", "liu", "long",
    "lou", "lu", "luan", "lun", "luo", "lv", "lve", "ma", "mai", "man", "mang", "mao", "me", "mei",
    "men", "meng", "mi", "mian", "miao", "mie", "min", "ming", "miu", "mo", "mou", "mu", "na",
    "nai", "nan", "nang", "nao", "ne", "nei", "nen", "neng", "ni", "nian", "niang", "niao", "nie",
    "nin", "ning", "niu", "nong", "nou", "nu", "nuan", "nuo", "nv", "nve", "o", "ou", "pa", "pai",
    "pan", "pang", "pao", "pei", "pen", "peng", "pi", "pian", "piao", "pie", "pin", "ping", "po",
    "pou", "pu", "qi", "qia", "qian", "qiang", "qiao", "qie", "qin", "qing", "qiong", "qiu", "qu",
    "quan", "que", "qun", "ran", "rang", "rao", "re", "ren", "reng", "ri", "rong", "rou", "ru",
    "ruan", "rui", "run", "ruo", "sa", "sai", "san", "sang", "sao", "se", "sen", "seng", "sha",
    "shai", "shan", "shang", "shao", "she", "shei", "shen", "sheng", "shi", "shou", "shu", "shua",
    "shuai", "shuan", "shuang", "shui", "shun", "shuo", "si", "song", "sou", "su", "suan", "sui",
    "sun", "suo", "ta", "tai", "tan", "tang", "tao", "te", "tei", "teng", "ti", "tian", "tiao",
    "tie", "ting", "tong", "tou", "tu", "tuan", "tui", "tun", "tuo", "wa", "wai", "wan", "wang",
    "wei", "wen", "weng", "wo", "wu", "xi", "xia", "xian", "xiang", "xiao", "xie", "xin", "xing",
    "xiong", "xiu", "xu", "xuan", "xue", "xun", "ya", "yan", "yang", "yao", "ye", "yi", "yin",
    "ying", "yo", "yong", "you", "yu", "yuan", "yue", "yun", "za", "zai", "zan", "zang", "zao",
    "ze", "zei", "zen", "zeng", "zha", "zhai", "zhan", "zhang", "zhao", "zhe", "zhei", "zhen",
    "zheng", "zhi", "zhong", "zhou", "zhu", "zhua", "zhuai", "zhuan", "zhuang", "zhui", "zhun",
    "zhuo", "zi", "zong", "zou", "zu", "zuan", "zui", "zun", "zuo",
];

const SYNTHETIC_CODES: &[&str] = &[
    "ni", "hao", "zhong", "guo", "bei", "jing", "shang", "hai", "da", "xue", "dian", "nao", "shu",
    "ru", "fa", "pin", "yin", "ci", "ku", "jian", "pan", "xian", "shi", "qi", "wo", "men",
];

static SYLLABLE_TRIE: OnceLock<SyllableTrie> = OnceLock::new();

#[derive(Clone, Debug, Default)]
struct SyllableTrie {
    children: [Option<Box<SyllableTrie>>; 26],
    is_end: bool,
}

impl SyllableTrie {
    fn insert(&mut self, s: &str) {
        let mut node = self;
        for b in s.bytes() {
            let idx = (b - b'a') as usize;
            node = node.children[idx].get_or_insert_with(|| Box::new(SyllableTrie::default()));
        }
        node.is_end = true;
    }

    fn build(syllables: &[&str]) -> Self {
        let mut trie = SyllableTrie::default();
        for s in syllables {
            trie.insert(s);
        }
        trie
    }

    /// Greedy leftmost-longest segmentation.
    fn segment(&self, input: &str) -> Vec<String> {
        let mut result = Vec::new();
        let mut pos = 0;
        let bytes = input.as_bytes();
        while pos < bytes.len() {
            let mut node = self;
            let mut longest = pos;
            for i in pos..bytes.len() {
                let idx = (bytes[i] - b'a') as usize;
                match &node.children[idx] {
                    Some(child) => {
                        node = child;
                        if node.is_end {
                            longest = i + 1;
                        }
                    }
                    None => break,
                }
            }
            if longest == pos {
                longest = bytes.len();
            }
            result.push(input[pos..longest].to_owned());
            pos = longest;
        }
        result
    }
}

fn syllable_trie() -> &'static SyllableTrie {
    SYLLABLE_TRIE.get_or_init(|| SyllableTrie::build(PINYIN_SYLLABLES))
}

fn bench_segment_zhongguo(c: &mut Criterion) {
    let trie = syllable_trie();
    c.bench_function("seg/segment_zhongguo", |b| {
        b.iter(|| black_box(trie.segment(black_box("zhongguo"))))
    });
}

fn bench_segment_zhonghuarenmin(c: &mut Criterion) {
    let trie = syllable_trie();
    c.bench_function("seg/segment_zhonghuarenmin", |b| {
        b.iter(|| black_box(trie.segment(black_box("zhonghuarenmin"))))
    });
}

fn bench_segment_nihaoma(c: &mut Criterion) {
    let trie = syllable_trie();
    c.bench_function("seg/segment_nihaoma", |b| {
        b.iter(|| black_box(trie.segment(black_box("nihaoma"))))
    });
}

fn bench_segment_xianshiqi(c: &mut Criterion) {
    let trie = syllable_trie();
    c.bench_function("seg/segment_xianshiqi", |b| {
        b.iter(|| black_box(trie.segment(black_box("xianshiqi"))))
    });
}

// ── Combined: segment + query ───────────────────────────────────────

fn bench_combined_segment_and_query(c: &mut Criterion) {
    let trie = syllable_trie();
    let (index, _codes) = real_index();
    c.bench_function("combined/segment_and_query_zhongguo", |b| {
        b.iter(|| {
            let segments = trie.segment(black_box("zhongguo"));
            let mut total = 0usize;
            for seg in &segments {
                total = total.wrapping_add(index.query(black_box(seg.as_str())).len());
            }
            black_box(total);
        })
    });
}

fn bench_combined_segment_and_query_long(c: &mut Criterion) {
    let trie = syllable_trie();
    let (index, _codes) = real_index();
    c.bench_function("combined/segment_and_query_zhonghuarenmin", |b| {
        b.iter(|| {
            let segments = trie.segment(black_box("zhonghuarenmin"));
            let mut total = 0usize;
            for seg in &segments {
                total = total.wrapping_add(index.query(black_box(seg.as_str())).len());
            }
            black_box(total);
        })
    });
}

// ── Criterion groups ────────────────────────────────────────────────

criterion_group!(
    dict_build,
    bench_build_real_dict,
    bench_build_synthetic_100k,
    bench_build_rime_ice_539k,
);

criterion_group!(
    dict_query,
    bench_query_short_code,
    bench_query_long_code,
    bench_query_miss,
    bench_query_all_codes,
);

criterion_group!(
    dict_typing,
    bench_typing_zhongguo,
    bench_typing_zhonghuarenmin,
);

criterion_group!(
    segmentation,
    bench_segment_zhongguo,
    bench_segment_zhonghuarenmin,
    bench_segment_nihaoma,
    bench_segment_xianshiqi,
);

criterion_group!(
    combined,
    bench_combined_segment_and_query,
    bench_combined_segment_and_query_long,
);
criterion_group!(
    rime_ice,
    bench_query_rime_ice_short,
    bench_query_rime_ice_long,
    bench_query_rime_ice_miss,
    bench_query_rime_ice_all_codes,
);

criterion_main!(
    dict_build,
    dict_query,
    dict_typing,
    segmentation,
    combined,
    rime_ice
);
