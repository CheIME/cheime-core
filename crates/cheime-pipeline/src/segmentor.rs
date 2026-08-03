//! Pinyin syllable segmentor using a prefix trie.
//!
//! Complete syllable coverage is preferred over a local longest match.

use crate::Segmentor;
use crate::segmentation::{InputSpan, SegmentationGraph, SyllableEdge, SyllableKind};

const MAX_SUPPORTED_EDIT_DISTANCE: u8 = 2;
const MAX_CORRECTION_TOKEN_BYTES: usize = 8;

/// All valid Hanyu Pinyin syllables (without tones).
pub(crate) const PINYIN_SYLLABLES: &[&str] = &[
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

#[derive(Clone, Debug, Default)]
struct Trie {
    children: [Option<Box<Trie>>; 26],
    is_end: bool,
}

impl Trie {
    fn insert(&mut self, s: &str) {
        let mut node = self;
        for b in s.bytes() {
            let idx = (b - b'a') as usize;
            node = node.children[idx].get_or_insert_with(|| Box::new(Trie::default()));
        }
        node.is_end = true;
    }

    fn build(syllables: &[&str]) -> Self {
        let mut trie = Trie::default();
        for s in syllables {
            trie.insert(s);
        }
        trie
    }

    fn append_edges(&self, input: &str, start: usize, graph: &mut SegmentationGraph) {
        let bytes = input.as_bytes();
        let mut node = self;
        let mut advanced = false;
        for end in start..bytes.len() {
            let byte = bytes[end];
            if !byte.is_ascii_lowercase() {
                break;
            }
            let index = (byte - b'a') as usize;
            let Some(child) = &node.children[index] else {
                break;
            };
            node = child;
            advanced = true;
            if node.is_end {
                let end = end + 1;
                graph.add_edge(SyllableEdge {
                    span: InputSpan::new(start, end),
                    raw: input[start..end].to_owned(),
                    canonical: input[start..end].to_owned(),
                    kind: SyllableKind::Complete,
                });
            }
            if end + 1 == bytes.len() && !node.is_end {
                graph.add_edge(SyllableEdge {
                    span: InputSpan::new(start, bytes.len()),
                    raw: input[start..].to_owned(),
                    canonical: input[start..].to_owned(),
                    kind: SyllableKind::Incomplete,
                });
            }
        }

        if !advanced || graph.edges_from(start).is_empty() {
            let end = input[start..]
                .char_indices()
                .nth(1)
                .map(|(offset, _)| start + offset)
                .unwrap_or(input.len());
            graph.add_edge(SyllableEdge {
                span: InputSpan::new(start, end),
                raw: input[start..end].to_owned(),
                canonical: input[start..end].to_owned(),
                kind: SyllableKind::Raw,
            });
        }
    }
}

#[derive(Clone, Debug)]
pub struct PinyinSegmentor {
    trie: Trie,
    correction: PinyinCorrectionOptions,
}

/// Bounded typo-correction expansion for the segmentation graph.
///
/// Correction is opt-in. Limits are normalized by the segmentor so malformed
/// external configuration cannot create an unbounded graph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PinyinCorrectionOptions {
    pub enabled: bool,
    pub max_edit_distance: u8,
    pub max_candidates_per_start: usize,
    pub edit_penalty: i64,
}

impl Default for PinyinCorrectionOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            max_edit_distance: 1,
            max_candidates_per_start: 16,
            edit_penalty: 500_000,
        }
    }
}

impl PinyinCorrectionOptions {
    fn normalized(self) -> Self {
        Self {
            enabled: self.enabled,
            max_edit_distance: self.max_edit_distance.clamp(1, MAX_SUPPORTED_EDIT_DISTANCE),
            max_candidates_per_start: self.max_candidates_per_start.min(64),
            edit_penalty: self.edit_penalty.max(1),
        }
    }
}

impl PinyinSegmentor {
    pub fn new() -> Self {
        Self {
            trie: Trie::build(PINYIN_SYLLABLES),
            correction: PinyinCorrectionOptions::default(),
        }
    }

    pub fn with_correction(mut self, options: PinyinCorrectionOptions) -> Self {
        self.correction = options.normalized();
        self
    }

    fn append_correction_edges(&self, input: &str, start: usize, graph: &mut SegmentationGraph) {
        if !self.correction.enabled || self.correction.max_candidates_per_start == 0 {
            return;
        }
        let bytes = input.as_bytes();
        if bytes
            .get(start)
            .is_none_or(|byte| !byte.is_ascii_lowercase())
        {
            return;
        }

        let contiguous_end = bytes[start..]
            .iter()
            .position(|byte| !byte.is_ascii_lowercase())
            .map(|offset| start + offset)
            .unwrap_or(bytes.len());
        let available = contiguous_end - start;
        let max_distance = usize::from(self.correction.max_edit_distance);
        let mut candidates = Vec::<(i64, usize, &'static str)>::new();

        for &canonical in PINYIN_SYLLABLES {
            let shortest = canonical.len().saturating_sub(max_distance).max(2);
            let longest = canonical
                .len()
                .saturating_add(max_distance)
                .min(MAX_CORRECTION_TOKEN_BYTES)
                .min(available);
            if shortest > longest {
                continue;
            }
            for consumed in shortest..=longest {
                let raw = &input[start..start + consumed];
                let Some(distance) = bounded_damerau_levenshtein(
                    raw.as_bytes(),
                    canonical.as_bytes(),
                    self.correction.max_edit_distance,
                ) else {
                    continue;
                };
                if distance == 0 {
                    continue;
                }
                let cost = self
                    .correction
                    .edit_penalty
                    .saturating_mul(i64::from(distance));
                candidates.push((cost, start + consumed, canonical));
            }
        }

        candidates.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.cmp(&right.1))
                .then_with(|| left.2.cmp(right.2))
        });
        candidates.dedup_by(|left, right| left.1 == right.1 && left.2 == right.2);
        candidates.truncate(self.correction.max_candidates_per_start);

        for (cost, end, canonical) in candidates {
            graph.add_edge_with_cost(
                SyllableEdge {
                    span: InputSpan::new(start, end),
                    raw: input[start..end].to_owned(),
                    canonical: canonical.to_owned(),
                    kind: SyllableKind::Complete,
                },
                cost,
            );
        }
    }
}

impl Default for PinyinSegmentor {
    fn default() -> Self {
        Self::new()
    }
}

impl Segmentor for PinyinSegmentor {
    fn segment(&self, composition: &str) -> SegmentationGraph {
        let mut graph = SegmentationGraph::new(composition.len());
        for (start, _) in composition.char_indices() {
            self.trie.append_edges(composition, start, &mut graph);
            self.append_correction_edges(composition, start, &mut graph);
        }
        graph.finish();
        graph
    }
}

/// Optimal-string-alignment distance with adjacent transpositions.
///
/// Pinyin syllables are at most six ASCII bytes, so a fixed matrix avoids heap
/// allocation in the realtime path. Inputs outside the supported bound are
/// rejected rather than resized.
fn bounded_damerau_levenshtein(left: &[u8], right: &[u8], limit: u8) -> Option<u8> {
    if left.len() > MAX_CORRECTION_TOKEN_BYTES || right.len() > MAX_CORRECTION_TOKEN_BYTES {
        return None;
    }
    let length_gap = left.len().abs_diff(right.len());
    if length_gap > usize::from(limit) {
        return None;
    }

    let mut distance = [[0u8; MAX_CORRECTION_TOKEN_BYTES + 1]; MAX_CORRECTION_TOKEN_BYTES + 1];
    for (index, row) in distance.iter_mut().enumerate().take(left.len() + 1) {
        row[0] = index as u8;
    }
    for index in 0..=right.len() {
        distance[0][index] = index as u8;
    }

    for left_index in 1..=left.len() {
        for right_index in 1..=right.len() {
            let substitution = u8::from(left[left_index - 1] != right[right_index - 1]);
            let mut best = distance[left_index - 1][right_index]
                .saturating_add(1)
                .min(distance[left_index][right_index - 1].saturating_add(1))
                .min(distance[left_index - 1][right_index - 1].saturating_add(substitution));
            if left_index > 1
                && right_index > 1
                && left[left_index - 1] == right[right_index - 2]
                && left[left_index - 2] == right[right_index - 1]
            {
                best = best.min(distance[left_index - 2][right_index - 2].saturating_add(1));
            }
            distance[left_index][right_index] = best;
        }
    }

    let result = distance[left.len()][right.len()];
    (result <= limit).then_some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segmentation::{InputSpan, SyllableKind};

    #[test]
    fn nih_keeps_complete_ni_and_incomplete_h() {
        let graph = PinyinSegmentor::new().segment("nih");
        assert!(graph.edges_from(0).iter().any(|edge| {
            edge.span == InputSpan::new(0, 2)
                && edge.canonical == "ni"
                && edge.kind == SyllableKind::Complete
        }));
        assert!(graph.edges_from(2).iter().any(|edge| {
            edge.span == InputSpan::new(2, 3)
                && edge.canonical == "h"
                && edge.kind == SyllableKind::Incomplete
        }));
    }

    #[test]
    fn xianshi_retains_ambiguous_first_edges() {
        let graph = PinyinSegmentor::new().segment("xianshi");
        let first: Vec<_> = graph
            .edges_from(0)
            .iter()
            .filter(|edge| edge.kind == SyllableKind::Complete)
            .map(|edge| edge.canonical.as_str())
            .collect();
        assert!(first.contains(&"xi"));
        assert!(first.contains(&"xian"));
    }

    #[test]
    fn invalid_fragment_advances_as_raw() {
        let graph = PinyinSegmentor::new().segment("ni1");
        assert!(
            graph.edges_from(2).iter().any(|edge| {
                edge.span == InputSpan::new(2, 3) && edge.kind == SyllableKind::Raw
            })
        );
    }

    #[test]
    fn segment_zhongguo() {
        let seg = PinyinSegmentor::new();
        let result = seg.segment("zhongguo").primary_path();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].code, "zhong");
        assert_eq!(result[0].tag, "pinyin");
        assert_eq!(result[1].code, "guo");
    }

    #[test]
    fn segment_nihao() {
        let seg = PinyinSegmentor::new();
        let result = seg.segment("nihao").primary_path();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].code, "ni");
        assert_eq!(result[1].code, "hao");
    }

    #[test]
    fn segment_partial_input() {
        let seg = PinyinSegmentor::new();
        let result = seg.segment("zhongg").primary_path();
        // "zhong" is a syllable, "g" is dangling
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].code, "zhong");
        assert_eq!(result[1].code, "g");
    }

    #[test]
    fn empty_input_returns_empty() {
        let seg = PinyinSegmentor::new();
        let result = seg.segment("");
        assert!(result.is_empty());
    }

    #[test]
    fn correction_is_disabled_by_default() {
        let graph = PinyinSegmentor::new().segment("em");

        assert!(
            graph
                .edges_from(0)
                .iter()
                .all(|edge| edge.canonical != "me")
        );
    }

    #[test]
    fn correction_adds_a_costed_transposition_edge() {
        let graph = PinyinSegmentor::new()
            .with_correction(PinyinCorrectionOptions {
                enabled: true,
                edit_penalty: 123,
                ..Default::default()
            })
            .segment("em");
        let edge = graph
            .edges_from(0)
            .iter()
            .find(|edge| edge.span == InputSpan::new(0, 2) && edge.canonical == "me")
            .expect("em should have a transposition edge to me");

        assert_eq!(graph.edge_cost(edge), 123);
    }

    #[test]
    fn correction_expansion_is_bounded_per_input_offset() {
        let options = PinyinCorrectionOptions {
            enabled: true,
            max_edit_distance: 2,
            max_candidates_per_start: 3,
            ..Default::default()
        };
        let baseline = PinyinSegmentor::new().segment("zz");
        let corrected = PinyinSegmentor::new()
            .with_correction(options)
            .segment("zz");

        assert!(
            corrected.edges_from(0).len() <= baseline.edges_from(0).len() + 3,
            "correction edges must respect the per-offset cap"
        );
    }

    #[test]
    fn damerau_distance_supports_all_single_edit_types() {
        assert_eq!(bounded_damerau_levenshtein(b"em", b"me", 1), Some(1));
        assert_eq!(bounded_damerau_levenshtein(b"shn", b"shen", 1), Some(1));
        assert_eq!(bounded_damerau_levenshtein(b"shenn", b"shen", 1), Some(1));
        assert_eq!(bounded_damerau_levenshtein(b"shrn", b"shen", 1), Some(1));
        assert_eq!(bounded_damerau_levenshtein(b"abc", b"shen", 1), None);
    }

    #[test]
    fn repeated_syllables_do_not_choose_a_dead_end() {
        let seg = PinyinSegmentor::new();
        let result = seg.segment("ninininini").primary_path();
        let codes: Vec<&str> = result.iter().map(|segment| segment.code.as_str()).collect();

        assert_eq!(codes, ["ni", "ni", "ni", "ni", "ni"]);
    }

    #[test]
    fn ambiguous_xianshiqi() {
        let seg = PinyinSegmentor::new();
        let result = seg.segment("xianshiqi").primary_path();
        // greedy gives: xian-shi-qi (not xi-an-shi-qi)
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].code, "xian");
        assert_eq!(result[1].code, "shi");
        assert_eq!(result[2].code, "qi");
    }
}
