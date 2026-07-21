//! Pinyin syllable segmentor using a prefix trie.
//!
//! Extracted from the benchmark prototype and upgraded to support
//! multiple segmentation paths (instead of greedy leftmost-longest).
//!
//! For now uses the simpler greedy approach. The BFS syllable-graph
//! upgrade is planned for phase 1.3.

use crate::{CodeSegment, Segmentor};

/// All valid Hanyu Pinyin syllables (without tones).
pub(crate) const PINYIN_SYLLABLES: &[&str] = &[
    "a", "ai", "an", "ang", "ao",
    "ba", "bai", "ban", "bang", "bao", "bei", "ben", "beng", "bi", "bian", "biao", "bie",
    "bin", "bing", "bo", "bu",
    "ca", "cai", "can", "cang", "cao", "ce", "cen", "ceng", "cha", "chai", "chan", "chang",
    "chao", "che", "chen", "cheng", "chi", "chong", "chou", "chu", "chua", "chuai", "chuan",
    "chuang", "chui", "chun", "chuo", "ci", "cong", "cou", "cu", "cuan", "cui", "cun", "cuo",
    "da", "dai", "dan", "dang", "dao", "de", "dei", "den", "deng", "di", "dian", "diao",
    "die", "ding", "diu", "dong", "dou", "du", "duan", "dui", "dun", "duo",
    "e", "ei", "en", "eng", "er",
    "fa", "fan", "fang", "fei", "fen", "feng", "fo", "fou", "fu",
    "ga", "gai", "gan", "gang", "gao", "ge", "gei", "gen", "geng", "gong", "gou", "gu",
    "gua", "guai", "guan", "guang", "gui", "gun", "guo",
    "ha", "hai", "han", "hang", "hao", "he", "hei", "hen", "heng", "hong", "hou", "hu",
    "hua", "huai", "huan", "huang", "hui", "hun", "huo",
    "ji", "jia", "jian", "jiang", "jiao", "jie", "jin", "jing", "jiong", "jiu", "ju",
    "juan", "jue", "jun",
    "ka", "kai", "kan", "kang", "kao", "ke", "ken", "keng", "kong", "kou", "ku", "kua",
    "kuai", "kuan", "kuang", "kui", "kun", "kuo",
    "la", "lai", "lan", "lang", "lao", "le", "lei", "leng", "li", "lia", "lian", "liang",
    "liao", "lie", "lin", "ling", "liu", "long", "lou", "lu", "luan", "lun", "luo", "lv", "lve",
    "ma", "mai", "man", "mang", "mao", "me", "mei", "men", "meng", "mi", "mian", "miao",
    "mie", "min", "ming", "miu", "mo", "mou", "mu",
    "na", "nai", "nan", "nang", "nao", "ne", "nei", "nen", "neng", "ni", "nian", "niang",
    "niao", "nie", "nin", "ning", "niu", "nong", "nou", "nu", "nuan", "nuo", "nv", "nve",
    "o", "ou",
    "pa", "pai", "pan", "pang", "pao", "pei", "pen", "peng", "pi", "pian", "piao", "pie",
    "pin", "ping", "po", "pou", "pu",
    "qi", "qia", "qian", "qiang", "qiao", "qie", "qin", "qing", "qiong", "qiu", "qu",
    "quan", "que", "qun",
    "ran", "rang", "rao", "re", "ren", "reng", "ri", "rong", "rou", "ru", "ruan", "rui",
    "run", "ruo",
    "sa", "sai", "san", "sang", "sao", "se", "sen", "seng", "sha", "shai", "shan", "shang",
    "shao", "she", "shei", "shen", "sheng", "shi", "shou", "shu", "shua", "shuai", "shuan",
    "shuang", "shui", "shun", "shuo", "si", "song", "sou", "su", "suan", "sui", "sun", "suo",
    "ta", "tai", "tan", "tang", "tao", "te", "tei", "teng", "ti", "tian", "tiao", "tie",
    "ting", "tong", "tou", "tu", "tuan", "tui", "tun", "tuo",
    "wa", "wai", "wan", "wang", "wei", "wen", "weng", "wo", "wu",
    "xi", "xia", "xian", "xiang", "xiao", "xie", "xin", "xing", "xiong", "xiu", "xu",
    "xuan", "xue", "xun",
    "ya", "yan", "yang", "yao", "ye", "yi", "yin", "ying", "yo", "yong", "you", "yu",
    "yuan", "yue", "yun",
    "za", "zai", "zan", "zang", "zao", "ze", "zei", "zen", "zeng", "zha", "zhai", "zhan",
    "zhang", "zhao", "zhe", "zhei", "zhen", "zheng", "zhi", "zhong", "zhou", "zhu", "zhua",
    "zhuai", "zhuan", "zhuang", "zhui", "zhun", "zhuo", "zi", "zong", "zou", "zu", "zuan",
    "zui", "zun", "zuo",
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

    /// Greedy leftmost-longest segmentation.
    fn segment(&self, input: &str) -> Vec<String> {
        let mut result = Vec::new();
        let bytes = input.as_bytes();
        let mut pos = 0;
        while pos < bytes.len() {
            let mut node = self;
            let mut longest = pos;
            for i in pos..bytes.len() {
                let b = bytes[i];
                if !b.is_ascii_lowercase() {
                    break;
                }
                let idx = (b - b'a') as usize;
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
                // No valid syllable — take the whole remainder
                longest = bytes.len();
            }
            result.push(input[pos..longest].to_owned());
            pos = longest;
        }
        result
    }
}

#[derive(Clone, Debug)]
pub struct PinyinSegmentor {
    trie: Trie,
}

impl PinyinSegmentor {
    pub fn new() -> Self {
        Self {
            trie: Trie::build(PINYIN_SYLLABLES),
        }
    }
}

impl Default for PinyinSegmentor {
    fn default() -> Self {
        Self::new()
    }
}

impl Segmentor for PinyinSegmentor {
    fn segment(&self, composition: &str) -> Vec<CodeSegment> {
        if composition.is_empty() {
            return Vec::new();
        }
        let codes = self.trie.segment(composition);
        codes
            .into_iter()
            .map(|code| CodeSegment {
                code,
                tag: String::from("pinyin"),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_zhongguo() {
        let seg = PinyinSegmentor::new();
        let result = seg.segment("zhongguo");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].code, "zhong");
        assert_eq!(result[0].tag, "pinyin");
        assert_eq!(result[1].code, "guo");
    }

    #[test]
    fn segment_nihao() {
        let seg = PinyinSegmentor::new();
        let result = seg.segment("nihao");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].code, "ni");
        assert_eq!(result[1].code, "hao");
    }

    #[test]
    fn segment_partial_input() {
        let seg = PinyinSegmentor::new();
        let result = seg.segment("zhongg");
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
    fn ambiguous_xianshiqi() {
        let seg = PinyinSegmentor::new();
        let result = seg.segment("xianshiqi");
        // greedy gives: xian-shi-qi (not xi-an-shi-qi)
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].code, "xian");
        assert_eq!(result[1].code, "shi");
        assert_eq!(result[2].code, "qi");
    }
}
