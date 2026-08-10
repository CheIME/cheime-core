//! Native double-pinyin input: raw key codes → canonical syllable graph.
//!
//! The scheme (key → initial/finals/standalone) is compiled once against the
//! valid-pinyin syllable set into a 26×26 pair table. The runtime hot path
//! never concatenates `initial + final` strings — it only indexes the
//! compiled table. Each pair maps to 0..N canonical syllables so multi-final
//! keys (Flypy `k` → ing/uai, `x` → ia/ua, …) and ü-spellings (`ue`/`ve`,
//! `un`/`vn`) work without special cases.

use crate::segmentor::PINYIN_SYLLABLES;
use cheime_config::schema::{DoublePinyinPreset, DoublePinyinSchemeConfig};

/// One key of a double-pinyin scheme.
///
/// `initial == None` marks a zero-initial key. `single == true` (zero-initial
/// only) means the key alone emits its finals as complete syllables
/// (Flypy `a` → "a"); `false` zero-initial keys would defer to the second
/// key, but no shipped preset uses that mode.
#[derive(Clone, Debug)]
pub struct DoublePinyinKey {
    pub key: char,
    pub initial: Option<String>,
    pub finals: Vec<String>,
    pub single: bool,
}

/// Compiled double-pinyin scheme: raw key codes → canonical syllables.
#[derive(Clone, Debug)]
pub struct CompiledDoublePinyinTable {
    /// 26×26 entries; index = (k1 - b'a') * 26 + (k2 - b'a').
    pair: Box<[Vec<String>]>,
    /// 26 entries; index = k - b'a'. Single-key complete syllables.
    single: Box<[Vec<String>]>,
    /// 26 entries; index = k - b'a'. Single-key incomplete initials (e.g. `v` → "zh").
    initials: Box<[Option<String>]>,
}

/// Preset tables as `(key, initial, finals, single)` tuples.
type KeyTuple = (&'static str, Option<&'static str>, &'static [&'static str], bool);

/// Flypy (小鹤双拼), including multi-final keys and ü-spellings:
/// `k` → ing/uai, `l` → uang/iang, `o` → o/uo, `r` → uan/er, `s` → ong/iong,
/// `t` → ue/ve, `v` → ui/v, `x` → ia/ua, `y` → un/vn.
const FLYPY_KEYS: &[KeyTuple] = &[
    ("a", None, &["a"], true),
    ("b", Some("b"), &["in"], false),
    ("c", Some("c"), &["ao"], false),
    ("d", Some("d"), &["ai"], false),
    ("e", None, &["e"], true),
    ("f", Some("f"), &["en"], false),
    ("g", Some("g"), &["eng"], false),
    ("h", Some("h"), &["ang"], false),
    ("i", Some("ch"), &["i"], false),
    ("j", Some("j"), &["an"], false),
    ("k", Some("k"), &["ing", "uai"], false),
    ("l", Some("l"), &["uang", "iang"], false),
    ("m", Some("m"), &["ian"], false),
    ("n", Some("n"), &["iao"], false),
    ("o", None, &["o", "uo"], true),
    ("p", Some("p"), &["ie"], false),
    ("q", Some("q"), &["iu"], false),
    ("r", Some("r"), &["uan", "er"], false),
    ("s", Some("s"), &["ong", "iong"], false),
    ("t", Some("t"), &["ue", "ve"], false),
    ("u", Some("sh"), &["u"], false),
    ("v", Some("zh"), &["ui", "v"], false),
    ("w", Some("w"), &["ei"], false),
    ("x", Some("x"), &["ia", "ua"], false),
    ("y", Some("y"), &["un", "vn"], false),
    ("z", Some("z"), &["ou"], false),
];

/// MS Double Pinyin (微软双拼), ported from `key_mapper.rs` with the real
/// scheme's finals: `o → o/uo` (so `guo = g+o`) and `l → ai` (verbatim, so
/// `shuang = u+d` via uang on `d`, and `u+l` is `shai`).
/// Single-final per key; full-coverage validation is deferred (the codebase
/// `j → ian` value differs from the official `j → an`).
const MS_DOUBLE_KEYS: &[KeyTuple] = &[
    ("a", None, &["a"], true),
    ("b", Some("b"), &["ou"], false),
    ("c", Some("c"), &["iao"], false),
    ("d", Some("d"), &["uang"], false),
    ("e", None, &["e"], true),
    ("f", Some("f"), &["en"], false),
    ("g", Some("g"), &["eng"], false),
    ("h", Some("h"), &["ang"], false),
    ("i", Some("ch"), &["i"], false),
    ("j", Some("j"), &["ian"], false),
    ("k", Some("k"), &["ao"], false),
    ("l", Some("l"), &["ai"], false),
    ("m", Some("m"), &["ian"], false),
    ("n", Some("n"), &["in"], false),
    ("o", None, &["o", "uo"], true),
    ("p", Some("p"), &["un"], false),
    ("q", Some("q"), &["iu"], false),
    ("r", Some("r"), &["uan"], false),
    ("s", Some("s"), &["ong"], false),
    ("t", Some("t"), &["ue"], false),
    ("u", Some("sh"), &["u"], false),
    ("v", Some("zh"), &["ue"], false),
    ("w", Some("w"), &["ia"], false),
    ("x", Some("x"), &["ie"], false),
    ("y", Some("y"), &["uai"], false),
    ("z", Some("z"), &["ei"], false),
];

/// Ziranma (自然码双拼), ported from `key_mapper.rs` with the real scheme's
/// finals: `t → ue/ve` so both `xue = x+t` and `lve = l+t` resolve.
const ZIRANMA_KEYS: &[KeyTuple] = &[
    ("a", None, &["a"], true),
    ("b", Some("b"), &["ou"], false),
    ("c", Some("c"), &["iao"], false),
    ("d", Some("d"), &["ua"], false),
    ("e", None, &["e"], true),
    ("f", Some("f"), &["en"], false),
    ("g", Some("g"), &["eng"], false),
    ("h", Some("h"), &["ang"], false),
    ("i", Some("ch"), &["i"], false),
    ("j", Some("j"), &["an"], false),
    ("k", Some("k"), &["ao"], false),
    ("l", Some("l"), &["ai"], false),
    ("m", Some("m"), &["ian"], false),
    ("n", Some("n"), &["in"], false),
    ("o", None, &["o"], true),
    ("p", Some("p"), &["un"], false),
    ("q", Some("q"), &["iu"], false),
    ("r", Some("r"), &["uan"], false),
    ("s", Some("s"), &["ong"], false),
    ("t", Some("t"), &["ue", "ve"], false),
    ("u", Some("sh"), &["u"], false),
    ("v", Some("zh"), &["ui"], false),
    ("w", Some("w"), &["ia"], false),
    ("x", Some("x"), &["ie"], false),
    ("y", Some("y"), &["ing"], false),
    ("z", Some("z"), &["ei"], false),
];

fn keys_from_tuples(entries: &[KeyTuple]) -> Vec<DoublePinyinKey> {
    entries
        .iter()
        .map(|(key, initial, finals, single)| DoublePinyinKey {
            key: key.chars().next().expect("preset keys are single chars"),
            initial: initial.map(str::to_owned),
            finals: finals.iter().map(|final_| (*final_).to_owned()).collect(),
            single: *single,
        })
        .collect()
}

fn pair_index(k1: u8, k2: u8) -> usize {
    ((k1 - b'a') as usize) * 26 + (k2 - b'a') as usize
}

impl CompiledDoublePinyinTable {
    pub fn flypy() -> Self {
        Self::compile(&keys_from_tuples(FLYPY_KEYS)).expect("flypy preset is valid")
    }

    pub fn ms_double() -> Self {
        Self::compile(&keys_from_tuples(MS_DOUBLE_KEYS)).expect("ms preset is valid")
    }

    pub fn ziranma() -> Self {
        Self::compile(&keys_from_tuples(ZIRANMA_KEYS)).expect("ziranma preset is valid")
    }

    /// Resolve a schema scheme config (preset name or inline keys) into a table.
    pub fn from_scheme_config(scheme: &DoublePinyinSchemeConfig) -> Result<Self, String> {
        match (scheme.preset, scheme.keys.is_empty()) {
            (Some(DoublePinyinPreset::Flypy), true) => Ok(Self::flypy()),
            (Some(DoublePinyinPreset::MsDouble), true) => Ok(Self::ms_double()),
            (Some(DoublePinyinPreset::Ziranma), true) => Ok(Self::ziranma()),
            (Some(_), false) => {
                Err(String::from("scheme: preset and keys are mutually exclusive"))
            }
            (None, false) => {
                let mut keys = Vec::with_capacity(scheme.keys.len());
                for configured in &scheme.keys {
                    if configured.key.chars().count() != 1 {
                        return Err(format!(
                            "scheme: key must be a single character, got {:?}",
                            configured.key
                        ));
                    }
                    keys.push(DoublePinyinKey {
                        key: configured.key.chars().next().expect("length checked"),
                        initial: configured.initial.clone(),
                        finals: configured.finals.clone(),
                        single: configured.single,
                    });
                }
                Self::compile(&keys)
            }
            (None, true) => Err(String::from("scheme: preset or keys required")),
        }
    }

    /// Compile a scheme against the valid-pinyin syllable set.
    ///
    /// Pair rule: a consonant-initial first key combines its initial with the
    /// second key's finals; a zero-initial first key `k1` accepts the second
    /// key's final `f2` only when `f2` starts with `k1` (Flypy `a`+`d` → "ai",
    /// `e`+`r` → "er"). Every candidate must be a valid syllable — invalid
    /// combinations disappear at compile time.
    pub fn compile(keys: &[DoublePinyinKey]) -> Result<Self, String> {
        if keys.is_empty() || keys.len() > 26 {
            return Err(format!("scheme needs 1..=26 keys, got {}", keys.len()));
        }
        let mut by_key: [Option<&DoublePinyinKey>; 26] = [None; 26];
        for key in keys {
            if !key.key.is_ascii_lowercase() {
                return Err(format!("key {:?} must be a lowercase ascii char", key.key));
            }
            let index = (key.key as u8 - b'a') as usize;
            if by_key[index].is_some() {
                return Err(format!("duplicate key {:?}", key.key));
            }
            if key.single && key.initial.is_some() {
                return Err(format!(
                    "key {:?}: single=true requires a zero-initial key",
                    key.key
                ));
            }
            by_key[index] = Some(key);
        }

        let mut pair = vec![Vec::new(); 26 * 26];
        let mut single = vec![Vec::new(); 26];
        for k1 in keys {
            let i1 = (k1.key as u8 - b'a') as usize;
            for k2 in keys {
                let i2 = (k2.key as u8 - b'a') as usize;
                if let Some(initial) = &k1.initial {
                    for f2 in &k2.finals {
                        let mut candidate = String::with_capacity(initial.len() + f2.len());
                        candidate.push_str(initial);
                        candidate.push_str(f2);
                        if PINYIN_SYLLABLES.binary_search(&candidate.as_str()).is_ok() {
                            pair[pair_index(k1.key as u8, k2.key as u8)].push(candidate);
                        }
                    }
                } else {
                    for f2 in &k2.finals {
                        if f2.starts_with(k1.key)
                            && PINYIN_SYLLABLES.binary_search(&f2.as_str()).is_ok()
                        {
                            pair[i1 * 26 + i2].push(f2.clone());
                        }
                    }
                }
            }
            if k1.initial.is_none() && k1.single {
                for f in &k1.finals {
                    if PINYIN_SYLLABLES.binary_search(&f.as_str()).is_ok() {
                        single[i1].push(f.clone());
                    }
                }
            }
        }

        let initials: Vec<Option<String>> = by_key
            .iter()
            .map(|entry| entry.and_then(|key| key.initial.clone()))
            .collect();

        Ok(Self {
            pair: pair.into_boxed_slice(),
            single: single.into_boxed_slice(),
            initials: initials.into_boxed_slice(),
        })
    }

    pub(crate) fn pair_for(&self, k1: char, k2: char) -> &[String] {
        &self.pair[pair_index(k1 as u8, k2 as u8)]
    }

    pub(crate) fn single_for(&self, k: char) -> &[String] {
        &self.single[(k as u8 - b'a') as usize]
    }

    pub(crate) fn initial_for(&self, k: char) -> Option<&str> {
        self.initials[(k as u8 - b'a') as usize].as_deref()
    }
}

use crate::Segmentor;
use crate::segmentation::{InputSpan, SegmentationGraph, SyllableEdge, SyllableKind};

/// Stateless double-pinyin segmentor: raw key composition → canonical graph.
///
/// The segmentor never carries state between calls — `segment` is a pure
/// function of the composition, so backspace, session restore, and candidate
/// recomputation cannot desync from the raw input.
#[derive(Clone, Debug)]
pub struct DoublePinyinSegmentor {
    table: CompiledDoublePinyinTable,
    keyboard: Option<KeyboardMistouchModel>,
    confusion: Option<CodeConfusionModel>,
}

impl DoublePinyinSegmentor {
    pub fn new(table: CompiledDoublePinyinTable) -> Self {
        Self {
            table,
            keyboard: None,
            confusion: None,
        }
    }

    pub fn flypy() -> Self {
        Self::new(CompiledDoublePinyinTable::flypy())
    }

    /// Enable keyboard-mistouch substitution (adjacent-key typos).
    pub fn with_keyboard(mut self, model: KeyboardMistouchModel) -> Self {
        self.keyboard = Some(model);
        self
    }

    /// Enable directed code-confusion rules (observed pair → intended pair).
    pub fn with_confusion(mut self, model: CodeConfusionModel) -> Self {
        self.confusion = Some(model);
        self
    }
}

/// 8-neighborhood adjacency on a standard QWERTY layout, as byte strings,
/// indexed by key position 0..26 (`QWERTY_NEIGHBORS[(k - b'a') as usize]` = neighbors of key `k`),
const QWERTY_NEIGHBORS: &[&[u8]] = &[
    b"qwszx",    // a
    b"vghn",     // b
    b"xdfv",     // c
    b"sefrxcv",  // d
    b"wrsdf",    // e
    b"drgtcvb",  // f
    b"fthyvbn",  // g
    b"gyjubnm",  // h
    b"uojkl",    // i
    b"hukinm",   // j
    b"jilom",    // k
    b"kop",      // l
    b"njk",      // m
    b"bhjm",     // n
    b"ipkl",     // o
    b"ol",       // p
    b"was",      // q
    b"etdfg",    // r
    b"awdexzc",  // s
    b"ryfgh",    // t
    b"yihjk",    // u
    b"cfgb",     // v
    b"qeasd",    // w
    b"zsdc",     // x
    b"tughj",    // y
    b"asx",      // z
];

/// Keyboard mistouch model: one key of a two-key pair typed as an adjacent
/// key (substitution only — no insert/delete/swap in this version).
///
/// For observed pair `(k1, k2)` the model re-queries the compiled table with
/// `k1` replaced by each of its neighbors and with `k2` replaced by each of
/// its neighbors. Only pairs that actually compile produce edges, so the
/// expansion stays at a handful per position — never 676.
#[derive(Clone, Debug)]
pub struct KeyboardMistouchModel {
    cost: i64,
    neighbors: &'static [&'static [u8]],
}

impl KeyboardMistouchModel {
    pub fn qwerty(cost: i64) -> Self {
        Self {
            cost,
            neighbors: QWERTY_NEIGHBORS,
        }
    }

    pub(crate) fn append_edges(
        &self,
        composition: &str,
        start: usize,
        bytes: &[u8],
        table: &CompiledDoublePinyinTable,
        graph: &mut SegmentationGraph,
    ) {
        if start + 1 >= bytes.len() || !bytes[start + 1].is_ascii_lowercase() {
            return;
        }
        let k1 = (bytes[start] - b'a') as usize;
        let k2 = (bytes[start + 1] - b'a') as usize;
        let raw = composition[start..start + 2].to_owned();
        for &neighbor in self.neighbors[k1] {
            for syllable in table.pair_for(neighbor as char, bytes[start + 1] as char) {
                graph.add_edge_with_cost(
                    SyllableEdge {
                        span: InputSpan::new(start, start + 2),
                        raw: raw.clone(),
                        canonical: syllable.clone(),
                        kind: SyllableKind::Complete,
                    },
                    self.cost,
                );
            }
        }
        for &neighbor in self.neighbors[k2] {
            for syllable in table.pair_for(bytes[start] as char, neighbor as char) {
                graph.add_edge_with_cost(
                    SyllableEdge {
                        span: InputSpan::new(start, start + 2),
                        raw: raw.clone(),
                        canonical: syllable.clone(),
                        kind: SyllableKind::Complete,
                    },
                    self.cost,
                );
            }
        }
    }
}

/// Code confusion model: the user typed a valid double-pinyin pair but
/// confused the scheme rules. Rules are directional — `from → to` never
/// implies `to → from`.
#[derive(Clone, Debug)]
pub struct CodeConfusionModel {
    /// rules[observed_pair_index] = (intended_pair_index, cost)
    rules: Box<[Vec<(usize, i64)>]>,
}

impl CodeConfusionModel {
    /// Build from `(observed, intended, per-rule cost override)` triples.
    /// `default_cost` applies when a rule has no override.
    pub fn from_rules(
        default_cost: i64,
        rules: &[(String, String, Option<i64>)],
    ) -> Result<Self, String> {
        let mut table = vec![Vec::new(); 26 * 26];
        for (from, to, cost) in rules {
            if from.len() != 2
                || to.len() != 2
                || !from.bytes().all(|byte| byte.is_ascii_lowercase())
                || !to.bytes().all(|byte| byte.is_ascii_lowercase())
            {
                return Err(format!(
                    "confusion rule must be two lowercase keys, got {from:?} → {to:?}"
                ));
            }
            if from == to {
                return Err(format!("confusion rule must not map a pair to itself: {from:?}"));
            }
            let observed = pair_index(from.as_bytes()[0], from.as_bytes()[1]);
            let intended = pair_index(to.as_bytes()[0], to.as_bytes()[1]);
            table[observed].push((intended, cost.unwrap_or(default_cost).max(0)));
        }
        Ok(Self {
            rules: table.into_boxed_slice(),
        })
    }

    pub(crate) fn append_edges(
        &self,
        composition: &str,
        start: usize,
        bytes: &[u8],
        table: &CompiledDoublePinyinTable,
        graph: &mut SegmentationGraph,
    ) {
        if start + 1 >= bytes.len() || !bytes[start + 1].is_ascii_lowercase() {
            return;
        }
        let observed = pair_index(bytes[start], bytes[start + 1]);
        if self.rules[observed].is_empty() {
            return;
        }
        let raw = composition[start..start + 2].to_owned();
        for &(intended, cost) in &self.rules[observed] {
            let k1 = (intended / 26) as u8 + b'a';
            let k2 = (intended % 26) as u8 + b'a';
            for syllable in table.pair_for(k1 as char, k2 as char) {
                graph.add_edge_with_cost(
                    SyllableEdge {
                        span: InputSpan::new(start, start + 2),
                        raw: raw.clone(),
                        canonical: syllable.clone(),
                        kind: SyllableKind::Complete,
                    },
                    cost,
                );
            }
        }
    }
}

impl Segmentor for DoublePinyinSegmentor {
    fn segment(&self, composition: &str) -> SegmentationGraph {
        let bytes = composition.as_bytes();
        let mut graph = SegmentationGraph::new(composition.len());
        for start in 0..bytes.len() {
            if !composition.is_char_boundary(start) {
                // Skip UTF-8 continuation bytes; only char starts are handled.
                continue;
            }
            let byte = bytes[start];
            if !byte.is_ascii_lowercase() {
                // Raw edge spanning one char: `'` delimiter, punctuation, …
                let end = composition[start..]
                    .char_indices()
                    .nth(1)
                    .map(|(offset, _)| start + offset)
                    .unwrap_or(composition.len());
                graph.add_edge(SyllableEdge {
                    span: InputSpan::new(start, end),
                    raw: composition[start..end].to_owned(),
                    canonical: composition[start..end].to_owned(),
                    kind: SyllableKind::Raw,
                });
                continue;
            }

            let k1 = byte;

            // Exact two-key pairs — checked first so the single-key
            // Incomplete edge can be suppressed when a pair completes at
            // this start (real IMEs stop offering zh-prefix completion once
            // "vs" fully types zhong).
            let mut pair_added = false;
            if start + 1 < bytes.len() && bytes[start + 1].is_ascii_lowercase() {
                let k2 = bytes[start + 1];
                for syllable in self.table.pair_for(k1 as char, k2 as char) {
                    graph.add_edge(SyllableEdge {
                        span: InputSpan::new(start, start + 2),
                        raw: composition[start..start + 2].to_owned(),
                        canonical: syllable.clone(),
                        kind: SyllableKind::Complete,
                    });
                    pair_added = true;
                }
            }

            // Keyboard-mistouch variants: the same span re-queried with one
            // key replaced by an adjacent key, at the model's cost. Exact
            // edges above stay zero-cost.
            if let Some(keyboard) = &self.keyboard {
                keyboard.append_edges(composition, start, bytes, &self.table, &mut graph);
            }

            if let Some(confusion) = &self.confusion {
                confusion.append_edges(composition, start, bytes, &self.table, &mut graph);
            }

            // Single-key complete syllables (zero-initial standalone keys).
            for syllable in self.table.single_for(k1 as char) {
                graph.add_edge(SyllableEdge {
                    span: InputSpan::new(start, start + 1),
                    raw: composition[start..start + 1].to_owned(),
                    canonical: syllable.clone(),
                    kind: SyllableKind::Complete,
                });
            }

            // Incomplete initial prefix (v → "zh") for prefix completion —
            // only when no complete pair exists at this start.
            if !pair_added {
                if let Some(initial) = self.table.initial_for(k1 as char) {
                    graph.add_edge(SyllableEdge {
                        span: InputSpan::new(start, start + 1),
                        raw: composition[start..start + 1].to_owned(),
                        canonical: initial.to_owned(),
                        kind: SyllableKind::Incomplete,
                    });
                }
            }
        }
        graph.finish();
        graph
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::segmentation::{InputSpan, SyllableKind};
    use crate::Segmentor;

    fn expect_pair(table: &CompiledDoublePinyinTable, k1: char, k2: char, syllables: &[&str]) {
        let actual: Vec<&str> = table.pair_for(k1, k2).iter().map(String::as_str).collect();
        assert_eq!(actual, syllables, "pair {k1}{k2}");
    }

    #[test]
    fn flypy_pair_compilation_spot_checks() {
        let table = CompiledDoublePinyinTable::flypy();
        expect_pair(&table, 'v', 's', &["zhong"]);
        expect_pair(&table, 'g', 'o', &["guo"]);
        expect_pair(&table, 's', 'h', &["sang"]);
        expect_pair(&table, 'u', 'l', &["shuang"]);
        expect_pair(&table, 'v', 'x', &["zhua"]);
        expect_pair(&table, 'k', 'k', &["kuai"]);
        expect_pair(&table, 'x', 's', &["xiong"]);
        expect_pair(&table, 'x', 't', &["xue"]);
        expect_pair(&table, 'l', 't', &["lve"]);
        expect_pair(&table, 'l', 'v', &["lv"]);
        expect_pair(&table, 'n', 't', &["nve"]);
        expect_pair(&table, 'w', 'o', &["wo"]);
        expect_pair(&table, 'y', 't', &["yue"]);
        expect_pair(&table, 'y', 'r', &["yuan"]);
        expect_pair(&table, 'i', 'o', &["chuo"]);
        expect_pair(&table, 'v', 'v', &["zhui"]);
        expect_pair(&table, 'd', 'o', &["duo"]);
        expect_pair(&table, 'b', 'k', &["bing"]);
        expect_pair(&table, 'j', 'y', &["jun"]);
        expect_pair(&table, 'y', 'y', &["yun"]);
        expect_pair(&table, 'n', 'j', &["nan"]);
        expect_pair(&table, 'a', 'b', &[]);
    }

    #[test]
    fn flypy_zero_initial_pairs() {
        let table = CompiledDoublePinyinTable::flypy();
        expect_pair(&table, 'a', 'a', &["a"]);
        expect_pair(&table, 'a', 'd', &["ai"]);
        expect_pair(&table, 'a', 'j', &["an"]);
        expect_pair(&table, 'a', 'h', &["ang"]);
        expect_pair(&table, 'a', 'c', &["ao"]);
        expect_pair(&table, 'e', 'e', &["e"]);
        expect_pair(&table, 'e', 'w', &["ei"]);
        expect_pair(&table, 'e', 'f', &["en"]);
        expect_pair(&table, 'e', 'g', &["eng"]);
        expect_pair(&table, 'e', 'r', &["er"]);
        expect_pair(&table, 'o', 'o', &["o"]);
        expect_pair(&table, 'o', 'z', &["ou"]);
        // There is no zero-initial "ong" syllable (weng = w+g).
        expect_pair(&table, 'o', 's', &[]);
    }

    #[test]
    fn flypy_single_keys() {
        let table = CompiledDoublePinyinTable::flypy();
        assert_eq!(table.single_for('a'), &[String::from("a")]);
        assert_eq!(table.single_for('e'), &[String::from("e")]);
        assert_eq!(table.single_for('o'), &[String::from("o")]);
        assert!(table.single_for('v').is_empty());
    }

    #[test]
    fn flypy_incomplete_initials() {
        let table = CompiledDoublePinyinTable::flypy();
        assert_eq!(table.initial_for('v'), Some("zh"));
        assert_eq!(table.initial_for('i'), Some("ch"));
        assert_eq!(table.initial_for('u'), Some("sh"));
        assert_eq!(table.initial_for('b'), Some("b"));
        assert_eq!(table.initial_for('a'), None);
    }

    #[test]
    fn flypy_covers_every_valid_syllable() {
        let table = CompiledDoublePinyinTable::flypy();
        let mut reverse = std::collections::HashMap::<&str, String>::new();
        for k1 in b'a'..=b'z' {
            for k2 in b'a'..=b'z' {
                for syllable in table.pair_for(k1 as char, k2 as char) {
                    reverse.insert(syllable, format!("{}{}", k1 as char, k2 as char));
                }
            }
        }
        for k in b'a'..=b'z' {
            for syllable in table.single_for(k as char) {
                reverse.insert(syllable, format!("{}", k as char));
            }
        }
        let untypable: Vec<&str> = crate::segmentor::PINYIN_SYLLABLES
            .iter()
            .copied()
            .filter(|syllable| !reverse.contains_key(*syllable))
            .collect();
        assert!(
            untypable.is_empty(),
            "untypable syllables under flypy: {untypable:?}"
        );
    }

    #[test]
    fn compile_rejects_invalid_keys() {
        let duplicate = vec![
            DoublePinyinKey { key: 'a', initial: None, finals: vec![String::from("a")], single: true },
            DoublePinyinKey { key: 'a', initial: None, finals: vec![String::from("ai")], single: true },
        ];
        assert!(CompiledDoublePinyinTable::compile(&duplicate).is_err());

        let uppercase = vec![DoublePinyinKey {
            key: 'A',
            initial: None,
            finals: vec![String::from("a")],
            single: true,
        }];
        assert!(CompiledDoublePinyinTable::compile(&uppercase).is_err());

        let empty: Vec<DoublePinyinKey> = vec![];
        assert!(CompiledDoublePinyinTable::compile(&empty).is_err());

        let single_with_initial = vec![DoublePinyinKey {
            key: 'a',
            initial: Some(String::from("zh")),
            finals: vec![String::from("a")],
            single: true,
        }];
        assert!(CompiledDoublePinyinTable::compile(&single_with_initial).is_err());
    }

    #[test]
    fn ms_and_ziranma_presets_compile_spot_checks() {
        // MS: zhong = v+s; guo = g+o (o carries uo); shuang = u+d (d carries uang);
        // lai = l+l (l carries ai); u+l would be shai, not shuang.
        let ms = CompiledDoublePinyinTable::ms_double();
        expect_pair(&ms, 'v', 's', &["zhong"]);
        expect_pair(&ms, 'g', 'o', &["guo"]);
        expect_pair(&ms, 'u', 'd', &["shuang"]);
        expect_pair(&ms, 'l', 'l', &["lai"]);
        // Ziranma: zhong = v+s; xue = x+t (t → ue/ve)
        let zr = CompiledDoublePinyinTable::ziranma();
        expect_pair(&zr, 'v', 's', &["zhong"]);
        expect_pair(&zr, 'x', 't', &["xue"]);
        expect_pair(&zr, 'l', 't', &["lve"]);
    }
    #[test]
    fn segment_vsgo_spans_raw_offsets() {
        let graph = DoublePinyinSegmentor::flypy().segment("vsgo");
        let zhong = graph
            .edges_from(0)
            .iter()
            .find(|edge| edge.canonical == "zhong")
            .unwrap();
        assert_eq!(zhong.span, InputSpan::new(0, 2));
        assert_eq!(zhong.raw, "vs");
        assert_eq!(zhong.kind, SyllableKind::Complete);
        assert_eq!(graph.edge_cost(zhong), 0);
        let guo = graph
            .edges_from(2)
            .iter()
            .find(|edge| edge.canonical == "guo")
            .unwrap();
        assert_eq!(guo.span, InputSpan::new(2, 4));
        assert_eq!(guo.raw, "go");
        assert_eq!(guo.kind, SyllableKind::Complete);
        assert_eq!(graph.edge_cost(guo), 0);
        assert_eq!(graph.input_len(), 4);
    }

    #[test]
    fn segment_single_v_is_incomplete_zh() {
        let graph = DoublePinyinSegmentor::flypy().segment("v");
        let edges = graph.edges_from(0);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].span, InputSpan::new(0, 1));
        assert_eq!(edges[0].raw, "v");
        assert_eq!(edges[0].canonical, "zh");
        assert_eq!(edges[0].kind, SyllableKind::Incomplete);
        assert_eq!(graph.edge_cost(&edges[0]), 0);
    }

    #[test]
    fn segment_odd_length_keeps_trailing_incomplete() {
        let graph = DoublePinyinSegmentor::flypy().segment("vsg");
        assert!(graph
            .edges_from(0)
            .iter()
            .any(|edge| edge.canonical == "zhong" && edge.span == InputSpan::new(0, 2)));
        let trailing = graph.edges_from(2);
        assert_eq!(trailing.len(), 1);
        assert_eq!(trailing[0].canonical, "g");
        assert_eq!(trailing[0].kind, SyllableKind::Incomplete);
    }

    #[test]
    fn segment_apostrophe_is_raw_boundary() {
        let graph = DoublePinyinSegmentor::flypy().segment("vs'go");
        let quote = graph.edges_from(2);
        assert_eq!(quote.len(), 1);
        assert_eq!(quote[0].raw, "'");
        assert_eq!(quote[0].kind, SyllableKind::Raw);
        assert!(graph
            .edges_from(3)
            .iter()
            .any(|edge| edge.canonical == "guo" && edge.span == InputSpan::new(3, 5)));
        assert!(graph.has_complete_path());
    }

    #[test]
    fn segment_flypy_codes_are_complete_and_free() {
        let segmentor = DoublePinyinSegmentor::flypy();
        // code → expected canonical syllable (小鹤 codes)
        let codes: &[(&str, &str)] = &[
            ("vs", "zhong"),
            ("go", "guo"),
            ("ul", "shuang"),
            ("vx", "zhua"),
            ("xt", "xue"),
            ("lt", "lve"),
            ("er", "er"),
            ("ad", "ai"),
            ("ah", "ang"),
            ("oo", "o"),
            ("wg", "weng"),
            ("yt", "yue"),
            ("yr", "yuan"),
            ("xs", "xiong"),
            ("kk", "kuai"),
            ("aa", "a"),
        ];
        for (code, expected) in codes {
            let graph = segmentor.segment(code);
            let edge = graph
                .edges_from(0)
                .iter()
                .find(|edge| edge.canonical == *expected && edge.span.end == code.len())
                .unwrap_or_else(|| panic!("{code} must segment to {expected}"));
            assert_eq!(edge.kind, SyllableKind::Complete);
            assert_eq!(graph.edge_cost(edge), 0, "{code} → {expected} must be free");
        }
    }

    #[test]
    fn segment_primary_path_prefers_complete_pairs() {
        let graph = DoublePinyinSegmentor::flypy().segment("vsgo");
        let codes: Vec<String> = graph
            .primary_path()
            .into_iter()
            .map(|segment| segment.code)
            .collect();
        assert_eq!(codes, ["zhong", "guo"]);
    }

    #[test]
    fn segment_empty_composition_is_empty() {
        let graph = DoublePinyinSegmentor::flypy().segment("");
        assert!(graph.is_empty());
    }

    #[test]
    fn segment_non_ascii_does_not_panic() {
        // "vs中go": the 3-byte char becomes one Raw edge; pairs still work
        // around it. The byte loop must never slice at a continuation byte.
        let graph = DoublePinyinSegmentor::flypy().segment("vs中go");
        assert!(graph
            .edges_from(0)
            .iter()
            .any(|edge| edge.canonical == "zhong" && edge.span == InputSpan::new(0, 2)));
        assert!(graph
            .edges_from(5)
            .iter()
            .any(|edge| edge.canonical == "guo" && edge.span == InputSpan::new(5, 7)));
        let raw = graph.edges_from(2);
        assert_eq!(raw.len(), 1);
        assert_eq!(raw[0].span, InputSpan::new(2, 5));
        assert_eq!(raw[0].kind, SyllableKind::Raw);
    }

    #[test]
    fn segment_pair_suppresses_mid_pair_incomplete() {
        // "vs" fully types zhong — no zh-prefix incomplete edge remains,
        // matching real IME behavior and keeping quanpin/flypy top-K equal.
        let graph = DoublePinyinSegmentor::flypy().segment("vs");
        assert!(graph
            .edges_from(0)
            .iter()
            .any(|edge| edge.canonical == "zhong" && edge.kind == SyllableKind::Complete));
        assert!(
            !graph.edges_from(0).iter().any(|edge| edge.kind == SyllableKind::Incomplete),
            "a complete pair must suppress the single-key incomplete edge"
        );
        // "v" alone keeps the incomplete edge (prefix completion).
        let single = DoublePinyinSegmentor::flypy().segment("v");
        assert!(single
            .edges_from(0)
            .iter()
            .any(|edge| edge.kind == SyllableKind::Incomplete));
    }

    #[test]
    fn keyboard_vd_offers_zhong_with_cost() {
        let segmentor = DoublePinyinSegmentor::flypy()
            .with_keyboard(KeyboardMistouchModel::qwerty(350_000));
        let graph = segmentor.segment("vd");
        let zhong = graph
            .edges_from(0)
            .iter()
            .find(|edge| edge.canonical == "zhong")
            .expect("d's neighbor s turns vd into vs → zhong");
        assert_eq!(zhong.kind, SyllableKind::Complete);
        assert_eq!(graph.edge_cost(zhong), 350_000);
        let zhai = graph
            .edges_from(0)
            .iter()
            .find(|edge| edge.canonical == "zhai")
            .expect("vd is exact zhai");
        assert_eq!(graph.edge_cost(zhai), 0, "exact path must stay free");
    }

    #[test]
    fn keyboard_exact_path_stays_zero() {
        let segmentor = DoublePinyinSegmentor::flypy()
            .with_keyboard(KeyboardMistouchModel::qwerty(350_000));
        let graph = segmentor.segment("vsgo");
        let zhong = graph
            .edges_from(0)
            .iter()
            .find(|edge| edge.canonical == "zhong")
            .unwrap();
        assert_eq!(graph.edge_cost(zhong), 0);
        let guo = graph
            .edges_from(2)
            .iter()
            .find(|edge| edge.canonical == "guo")
            .unwrap();
        assert_eq!(graph.edge_cost(guo), 0);
    }

    #[test]
    fn no_corrections_means_no_cost_edges() {
        let graph = DoublePinyinSegmentor::flypy().segment("vd");
        assert!(!graph.has_costs());
    }

    #[test]
    fn keyboard_alternative_count_is_bounded() {
        let segmentor = DoublePinyinSegmentor::flypy()
            .with_keyboard(KeyboardMistouchModel::qwerty(350_000));
        let graph = segmentor.segment("vd");
        // exact zhai + neighbor variants (cai, gai, bai, zhong, zhe, zhen,
        // zhuan, zhua, zhao, zhui) — never 676.
        assert!(graph.edges_from(0).len() <= 16);
    }

    #[test]
    fn confusion_rule_adds_intended_edge() {
        let model = CodeConfusionModel::from_rules(250_000, &[("vd".into(), "vs".into(), None)])
            .unwrap();
        let segmentor = DoublePinyinSegmentor::flypy().with_confusion(model);
        let graph = segmentor.segment("vd");
        let zhong = graph
            .edges_from(0)
            .iter()
            .find(|edge| edge.canonical == "zhong")
            .expect("rule vd→vs must recover zhong");
        assert_eq!(graph.edge_cost(zhong), 250_000);
    }

    #[test]
    fn confusion_rules_are_directional() {
        let model = CodeConfusionModel::from_rules(250_000, &[("vd".into(), "vs".into(), None)])
            .unwrap();
        let segmentor = DoublePinyinSegmentor::flypy().with_confusion(model);
        let graph = segmentor.segment("vs");
        assert!(
            !graph.has_costs(),
            "rule vd→vs must not apply to vs (no reverse edge)"
        );
    }

    #[test]
    fn confusion_rule_cost_override() {
        let model = CodeConfusionModel::from_rules(
            250_000,
            &[("vd".into(), "vs".into(), Some(180_000))],
        )
        .unwrap();
        let segmentor = DoublePinyinSegmentor::flypy().with_confusion(model);
        let graph = segmentor.segment("vd");
        let zhong = graph
            .edges_from(0)
            .iter()
            .find(|edge| edge.canonical == "zhong")
            .unwrap();
        assert_eq!(graph.edge_cost(zhong), 180_000);
    }

    #[test]
    fn confusion_rejects_malformed_rules() {
        for (from, to) in [
            ("v", "vs"),     // too short
            ("Vd", "vs"),    // uppercase
            ("vd", "v"),     // too short target
            ("vd", "vd"),    // self mapping
        ] {
            assert!(
                CodeConfusionModel::from_rules(
                    250_000,
                    &[(from.to_owned(), to.to_owned(), None)],
                )
                .is_err(),
                "rule {from}→{to} must be rejected"
            );
        }
    }
}
