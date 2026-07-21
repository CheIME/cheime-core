//! Emoji translator — first-class emoji candidate support.
//!
//! CheIME advantage: emoji is a dedicated Translator with keyword/pinyin
//! indexing, not an OpenCC simplifier filter like Rime's `simplifier@emoji`.
//!
//! ## Data Format (Rime OpenCC emoji compatible)
//!
//! Two-column TSV: `keyword<TAB>emoji`. Drop any Rime emoji `.txt` file
//! directly into `data/` and point `emoji_data` at it.
//!
//! ```text
//! 笑	😀
//! nihao	👋
//! hello	👋
//! ```
//!
//! Lookup: per-segment code (e.g. "zan"→👍) + concatenated segments
//! (e.g. ["ni","hao"]→"ni hao"→👋).

use crate::{CodeSegment, Translator};
use cheime_model::{Candidate, CandidateId};
use std::collections::HashMap;
use std::path::Path;

pub struct EmojiTranslator {
    /// keyword → list of emoji chars
    index: HashMap<String, Vec<String>>,
    counter: u64,
}

impl EmojiTranslator {
    /// Load emoji data from a Rime-compatible OpenCC TSV file.
    /// Format: `keyword<TAB>emoji`. Falls back to builtin if missing.
    pub fn from_file(path: &Path) -> Self {
        let mut t = Self { index: HashMap::new(), counter: 2_000_000 };
        match std::fs::read_to_string(path) {
            Ok(content) => {
                let count = t.load(&content);
                eprintln!("Emoji: loaded {count} entries from {}", path.display());
            }
            Err(e) => {
                eprintln!("Emoji: cannot read {}: {e}, using builtin", path.display());
                t.load_builtin();
            }
        }
        t
    }

    /// Load from Rime OpenCC emoji content (keyword<TAB>emoji).
    pub fn load(&mut self, content: &str) -> usize {
        let mut count = 0;
        for line in content.lines() {
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') { continue; }
            let parts: Vec<&str> = t.splitn(2, '\t').collect();
            if parts.len() < 2 { continue; }
            let kw = parts[0].trim().to_lowercase();
            let em = parts[1].trim();
            if kw.is_empty() || em.is_empty() { continue; }
            self.index.entry(kw).or_default().push(em.to_owned());
            count += 1;
        }
        count
    }

    fn load_builtin(&mut self) {
        self.load("\
笑	😀
笑哭	😂
笑滚	🤣
微笑	😊
喜欢	😍
爱	❤️
亲亲	😘
哭	😭
生气	😡
赞	👍
好	👍
你好	👋
hello	👋
nihao	👋
踩	👎
鼓掌	👏
祈祷	🙏
加油	💪
握手	🤝
胜利	✌️
火	🔥
星	⭐
钱	💰
庆祝	🎉
生日	🎂
礼物	🎁
灯泡	💡
链接	🔗
笔记	📝
满分	💯
完成	✅
错误	❌
警告	⚠️
禁止	🚫
问号	❓
xiao	😀
daxiao	😂
weixiao	😊
xihuan	😍
ai	❤️
xin	❤️
ku	😭
shengqi	😡
zan	👍
hui	👋
huishou	👋
cai	👎
guzhang	👏
qidao	🙏
jiayou	💪
woshou	🤝
shengli	✌️
huo	🔥
re	🔥
xing	⭐
qian	💰
qingzhu	🎉
shengri	🎂
liwu	🎁
dengpao	💡
lianjie	🔗
biji	📝
manfen	💯
wancheng	✅
cuowu	❌
jinggao	⚠️
jinzhi	🚫
wenhao	❓
ni hao	👋
");
    }

    /// Create with empty index (for testing).
    pub fn empty() -> Self {
        Self { index: HashMap::new(), counter: 2_000_000 }
    }
}

impl Translator for EmojiTranslator {
    fn name(&self) -> &str { "emoji" }

    fn translate(&self, segments: &[CodeSegment]) -> Vec<Candidate> {
        let mut results: Vec<String> = Vec::new();

        // Per-segment lookup
        for seg in segments {
            if let Some(emojis) = self.index.get(&seg.code) {
                results.extend(emojis.clone());
            }
        }

        // Concatenated segment lookup (e.g. ["ni","hao"] → "ni hao")
        if segments.len() > 1 {
            let joined = segments.iter().map(|s| s.code.as_str()).collect::<Vec<_>>().join(" ");
            if let Some(emojis) = self.index.get(&joined) {
                results.extend(emojis.clone());
            }
        }

        results.into_iter().enumerate().map(|(i, text)| {
            Candidate::emoji(CandidateId::new(self.counter + i as u64), text)
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_openc_format() {
        let mut t = EmojiTranslator::empty();
        let n = t.load("笑\t😀\n笑哭\t😂\nnihao\t👋\n");
        assert_eq!(n, 3);
        let cs = t.translate(&[CodeSegment { code: "笑".into(), tag: "kw".into() }]);
        assert_eq!(cs[0].text, "😀");
    }

    #[test]
    fn concatenated_segments_match() {
        let mut t = EmojiTranslator::empty();
        t.load("ni hao\t👋\n");
        let segs = &[CodeSegment { code: "ni".into(), tag: "py".into() }, CodeSegment { code: "hao".into(), tag: "py".into() }];
        let cs = t.translate(segs);
        assert_eq!(cs[0].text, "👋");
    }

    #[test]
    fn builtin_has_nihao_wave() {
        let t = EmojiTranslator::from_file(Path::new("/nonexistent/emoji.txt"));
        let segs = &[CodeSegment { code: "ni".into(), tag: "py".into() }, CodeSegment { code: "hao".into(), tag: "py".into() }];
        let cs = t.translate(segs);
        assert!(cs.iter().any(|c| c.text == "👋"), "builtin should have 👋 for ni hao");
    }

    #[test]
    fn comments_skipped() {
        let mut t = EmojiTranslator::empty();
        let n = t.load("# header\n笑\t😀\n# footer\n");
        assert_eq!(n, 1);
    }
}
