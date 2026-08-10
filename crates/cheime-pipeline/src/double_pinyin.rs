//! Native double-pinyin input: raw key codes → canonical syllable graph.
//!
//! The scheme (key → initial/finals/standalone) is compiled once against the
//! valid-pinyin syllable set into a 26×26 pair table. The runtime hot path
//! never concatenates `initial + final` strings — it only indexes the
//! compiled table. Each pair maps to 0..N canonical syllables so multi-final
//! keys (Flypy `k` → ing/uai, `x` → ia/ua, …) and ü-spellings (`ue`/`ve`,
//! `un`/`vn`) work without special cases.

use crate::segmentor::PINYIN_SYLLABLES;

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

/// MS Double Pinyin (微软双拼), ported from `key_mapper.rs` with two
/// deviations required by the spec'd spot checks: `l → uang` (not `ai`,
/// so `shuang = u+l`) and `o → o/uo` (not `o`, so `guo = g+o`).
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
    ("l", Some("l"), &["uang"], false),
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

/// Ziranma (自然码双拼), ported from `key_mapper.rs` with one deviation:
/// `t → ue/ve` (not just `ve`) so both `xue = x+t` and `lve = l+t` resolve.
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

#[cfg(test)]
mod tests {
    use super::*;

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
        // MS: zhong = v+s; guo = g+o; shuang = u+l
        let ms = CompiledDoublePinyinTable::ms_double();
        expect_pair(&ms, 'v', 's', &["zhong"]);
        expect_pair(&ms, 'g', 'o', &["guo"]);
        expect_pair(&ms, 'u', 'l', &["shuang"]);
        // Ziranma: zhong = v+s; xue = x+t (t → ve)
        let zr = CompiledDoublePinyinTable::ziranma();
        expect_pair(&zr, 'v', 's', &["zhong"]);
        expect_pair(&zr, 'x', 't', &["xue"]);
        expect_pair(&zr, 'l', 't', &["lve"]);
    }
}
