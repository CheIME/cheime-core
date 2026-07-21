//! Emoji translator — first-class emoji candidate support.
//!
//! CheIME advantage: emoji is a dedicated Translator with keyword/pinyin
//! indexing, not an OpenCC simplifier filter like Rime's `simplifier@emoji`.

use crate::{CodeSegment, Translator};
use cheime_model::{Candidate, CandidateId};
use std::collections::HashMap;

pub struct EmojiTranslator {
    /// keyword → list of emoji chars
    by_keyword: HashMap<String, Vec<String>>,
    /// pinyin → list of emoji chars
    by_pinyin: HashMap<String, Vec<String>>,
    counter: u64,
}

impl EmojiTranslator {
    pub fn new() -> Self {
        let mut t = Self { by_keyword: HashMap::new(), by_pinyin: HashMap::new(), counter: 2_000_000 };
        t.load_builtin();
        t
    }

    fn add(&mut self, emoji: &str, keywords: &[&str], pinyin: &[&str]) {
        for kw in keywords { self.by_keyword.entry(kw.to_lowercase()).or_default().push(emoji.to_owned()); }
        for py in pinyin { self.by_pinyin.entry(py.to_lowercase()).or_default().push(emoji.to_owned()); }
    }

    fn load_builtin(&mut self) {
        // Smileys & Emotion
        self.add("😀", &["笑", "哈哈", "开心", "smile", "grin"], &["xiao", "haha", "kaixin"]);
        self.add("😂", &["笑哭了", "笑哭", "大笑", "joy", "tears"], &["xiao", "daxiao"]);
        self.add("🤣", &["笑滚", "笑死", "rofl"], &["xiao", "xiaosi"]);
        self.add("😊", &["微笑", "害羞", "blush", "smile"], &["weixiao", "haixiu"]);
        self.add("😍", &["喜欢", "爱", "花痴", "heart", "love"], &["xihuan", "ai"]);
        self.add("😘", &["亲亲", "飞吻", "kiss"], &["qinqin"]);
        self.add("😜", &["调皮", "吐舌", "wink", "tongue"], &["tiaopi"]);
        self.add("😎", &["酷", "墨镜", "cool", "sunglasses"], &["ku"]);
        self.add("🤩", &["星星眼", "惊艳", "star"], &["xingxing"]);
        self.add("🥳", &["庆祝", "派对", "party", "celebrate"], &["qingzhu"]);
        self.add("😭", &["哭", "大哭", "伤心", "cry", "sad"], &["ku", "daku", "shangxin"]);
        self.add("😡", &["生气", "愤怒", "angry", "rage"], &["shengqi", "fennu"]);
        self.add("🤯", &["爆炸", "震惊", "mindblown"], &["baozha", "zhenjing"]);
        self.add("🥺", &["可怜", "求求", "pleading"], &["kelian"]);
        self.add("😴", &["困", "睡觉", "sleep", "tired"], &["kun", "shuijiao"]);
        self.add("🤒", &["生病", "发烧", "sick"], &["shengbing", "fashao"]);

        // Gestures
        self.add("👍", &["赞", "好", "顶", "thumbsup", "like"], &["zan", "hao"]);
        self.add("👎", &["踩", "差", "反对", "thumbsdown"], &["cai", "cha"]);
        self.add("👏", &["鼓掌", "拍手", "clap"], &["guzhang"]);
        self.add("🙏", &["祈祷", "感谢", "拜托", "pray", "thanks"], &["qidao", "ganxie", "baituo"]);
        self.add("💪", &["强壮", "肌肉", "加油", "muscle", "strong"], &["qiangzhuang", "jiayou"]);
        self.add("🤝", &["握手", "合作", "handshake"], &["woshou", "hezuo"]);
        self.add("✌️", &["胜利", "耶", "peace", "victory"], &["shengli", "ye"]);

        // Common objects
        self.add("❤️", &["爱", "心", "heart", "love"], &["ai", "xin"]);
        self.add("🔥", &["火", "热", "热门", "fire", "hot"], &["huo", "re"]);
        self.add("⭐", &["星", "收藏", "star", "favorite"], &["xing", "shoucang"]);
        self.add("💰", &["钱", "财富", "money", "rich"], &["qian", "caifu"]);
        self.add("🎉", &["庆祝", "恭喜", "party", "confetti"], &["qingzhu", "gongxi"]);
        self.add("🎂", &["生日", "蛋糕", "birthday", "cake"], &["shengri", "dangao"]);
        self.add("🍕", &["披萨", "pizza"], &["pisa"]);
        self.add("🍺", &["啤酒", "beer"], &["pijiu"]);
        self.add("☕", &["咖啡", "coffee"], &["kafei"]);
        self.add("🎵", &["音乐", "音符", "music"], &["yinyue"]);
        self.add("📚", &["书", "学习", "book"], &["shu", "xuexi"]);
        self.add("💻", &["电脑", "工作", "computer", "laptop"], &["diannao", "gongzuo"]);
        self.add("🚀", &["火箭", "起飞", "rocket", "launch"], &["huojian", "qifei"]);
        self.add("🎯", &["目标", "靶心", "target", "bullseye"], &["mubiao"]);
        self.add("💡", &["灯泡", "想法", "idea", "lightbulb"], &["dengpao", "xiangfa"]);

        // Weather / nature
        self.add("🌞", &["太阳", "晴天", "sun"], &["taiyang", "qingtian"]);
        self.add("🌈", &["彩虹", "rainbow"], &["caihong"]);
        self.add("🌧", &["下雨", "rain"], &["xiayu"]);
        self.add("🌸", &["花", "樱花", "flower", "sakura"], &["hua", "yinghua"]);

        // Animals
        self.add("🐶", &["狗", "dog", "puppy"], &["gou"]);
        self.add("🐱", &["猫", "cat", "kitten"], &["mao"]);
        self.add("🐼", &["熊猫", "panda"], &["xiongmao"]);
    }
}

impl Translator for EmojiTranslator {
    fn name(&self) -> &str { "emoji" }

    fn translate(&self, segments: &[CodeSegment]) -> Vec<Candidate> {
        let mut results: Vec<String> = Vec::new();

        for seg in segments {
            let code = &seg.code;
            if let Some(emojis) = self.by_pinyin.get(code) {
                results.extend(emojis.clone());
            }
            if let Some(emojis) = self.by_keyword.get(code) {
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
    fn smile_emoji_by_pinyin() {
        let t = EmojiTranslator::new();
        let s = CodeSegment { code: "xiao".into(), tag: "pinyin".into() };
        let cs = t.translate(&[s]);
        assert!(!cs.is_empty());
        assert!(cs.iter().all(|c| c.is_emoji));
    }

    #[test]
    fn heart_by_keyword() {
        let t = EmojiTranslator::new();
        let s = CodeSegment { code: "heart".into(), tag: "ascii".into() };
        let cs = t.translate(&[s]);
        assert!(!cs.is_empty());
    }
}
