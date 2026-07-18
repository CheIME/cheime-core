use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

fn make_event_id(device_id: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let seq = NEXT.fetch_add(1, Ordering::Relaxed);
    format!("{device_id}:{seq}")
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UserCandidate {
    pub text: String,
    pub code: String,
    pub frequency: i64,
    pub pinned: bool,
    pub blocked: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "operation")]
pub enum UserEvent {
    #[serde(rename = "learn_word")]
    LearnWord {
        event_id: String,
        timestamp: u64,
        schema: String,
        text: String,
        code: String,
        delta: i64,
    },
    #[serde(rename = "update_frequency")]
    UpdateFrequency {
        event_id: String,
        timestamp: u64,
        schema: String,
        text: String,
        delta: i64,
    },
    #[serde(rename = "delete_word")]
    DeleteWord {
        event_id: String,
        timestamp: u64,
        schema: String,
        text: String,
    },
    #[serde(rename = "pin_candidate")]
    PinCandidate {
        event_id: String,
        timestamp: u64,
        schema: String,
        text: String,
    },
    #[serde(rename = "block_candidate")]
    BlockCandidate {
        event_id: String,
        timestamp: u64,
        schema: String,
        text: String,
    },
    #[serde(rename = "use_emoji")]
    UseEmoji {
        event_id: String,
        timestamp: u64,
        schema: String,
        emoji: String,
    },
    #[serde(rename = "set_app_preference")]
    SetAppPreference {
        event_id: String,
        timestamp: u64,
        schema: String,
        app: String,
        key: String,
        value: String,
    },
}

impl UserEvent {
    pub fn learn_word(device_id: &str, schema: &str, text: &str, code: &str) -> Self {
        Self::LearnWord {
            event_id: make_event_id(device_id),
            timestamp: now_secs(),
            schema: schema.to_owned(),
            text: text.to_owned(),
            code: code.to_owned(),
            delta: 1,
        }
    }

    pub fn pin_candidate(device_id: &str, schema: &str, text: &str) -> Self {
        Self::PinCandidate {
            event_id: make_event_id(device_id),
            timestamp: now_secs(),
            schema: schema.to_owned(),
            text: text.to_owned(),
        }
    }

    pub fn block_candidate(device_id: &str, schema: &str, text: &str) -> Self {
        Self::BlockCandidate {
            event_id: make_event_id(device_id),
            timestamp: now_secs(),
            schema: schema.to_owned(),
            text: text.to_owned(),
        }
    }

    pub fn delete_word(device_id: &str, schema: &str, text: &str) -> Self {
        Self::DeleteWord {
            event_id: make_event_id(device_id),
            timestamp: now_secs(),
            schema: schema.to_owned(),
            text: text.to_owned(),
        }
    }

    pub fn update_frequency(device_id: &str, schema: &str, text: &str, delta: i64) -> Self {
        Self::UpdateFrequency {
            event_id: make_event_id(device_id),
            timestamp: now_secs(),
            schema: schema.to_owned(),
            text: text.to_owned(),
            delta,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::LearnWord { .. } => "learn_word",
            Self::UpdateFrequency { .. } => "update_frequency",
            Self::DeleteWord { .. } => "delete_word",
            Self::PinCandidate { .. } => "pin_candidate",
            Self::BlockCandidate { .. } => "block_candidate",
            Self::UseEmoji { .. } => "use_emoji",
            Self::SetAppPreference { .. } => "set_app_preference",
        }
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            Self::LearnWord { text, .. }
            | Self::UpdateFrequency { text, .. }
            | Self::DeleteWord { text, .. }
            | Self::PinCandidate { text, .. }
            | Self::BlockCandidate { text, .. } => Some(text),
            Self::UseEmoji { emoji, .. } => Some(emoji),
            Self::SetAppPreference { .. } => None,
        }
    }

    pub fn event_id(&self) -> &str {
        match self {
            Self::LearnWord { event_id, .. }
            | Self::UpdateFrequency { event_id, .. }
            | Self::DeleteWord { event_id, .. }
            | Self::PinCandidate { event_id, .. }
            | Self::BlockCandidate { event_id, .. }
            | Self::UseEmoji { event_id, .. }
            | Self::SetAppPreference { event_id, .. } => event_id,
        }
    }
}

#[derive(Clone, Debug)]
pub struct UserStore {
    #[allow(dead_code)]
    device_id: String,
    events: Vec<UserEvent>,
    frequency: HashMap<(String, String), i64>,
    pinned: HashSet<(String, String)>,
    blocked: HashSet<(String, String)>,
    deleted: HashSet<(String, String)>,
}

impl UserStore {
    pub fn new(device_id: &str) -> Self {
        Self {
            device_id: device_id.to_owned(),
            events: Vec::new(),
            frequency: HashMap::new(),
            pinned: HashSet::new(),
            blocked: HashSet::new(),
            deleted: HashSet::new(),
        }
    }

    pub fn apply(&mut self, event: UserEvent) {
        match &event {
            UserEvent::LearnWord {
                schema,
                text,
                code,
                delta,
                ..
            } => {
                let key = (schema.clone(), text.clone());
                if self.deleted.contains(&key) {
                    self.deleted.remove(&key);
                }
                *self
                    .frequency
                    .entry((schema.clone(), code.clone()))
                    .or_default() += delta;
            }
            UserEvent::UpdateFrequency {
                schema,
                text,
                delta,
                ..
            } => {
                let key = (schema.clone(), text.clone());
                if !self.deleted.contains(&key) {
                    *self.frequency.entry(key).or_default() += delta;
                }
            }
            UserEvent::DeleteWord { schema, text, .. } => {
                let key = (schema.clone(), text.clone());
                self.deleted.insert(key);
                self.pinned.remove(&(schema.clone(), text.clone()));
            }
            UserEvent::PinCandidate { schema, text, .. } => {
                self.pinned.insert((schema.clone(), text.clone()));
            }
            UserEvent::BlockCandidate { schema, text, .. } => {
                self.blocked.insert((schema.clone(), text.clone()));
            }
            UserEvent::UseEmoji { .. } | UserEvent::SetAppPreference { .. } => {}
        }
        self.events.push(event);
    }

    pub fn query(&self, code: &str) -> Vec<UserCandidate> {
        let mut candidates: Vec<UserCandidate> = Vec::new();
        let mut seen = HashSet::new();

        for ((schema, stored_code), freq) in &self.frequency {
            if stored_code != code {
                continue;
            }
            // Find the text for this entry by checking frequency keys
            let text = self
                .frequency
                .keys()
                .filter(|(s, _c)| s == schema)
                .map(|(_s, t)| t.clone())
                .next()
                .unwrap_or_default();

            if seen.contains(&text) {
                continue;
            }
            seen.insert(text.clone());

            candidates.push(UserCandidate {
                pinned: self.pinned.contains(&(schema.clone(), text.clone())),
                blocked: self.blocked.contains(&(schema.clone(), text.clone())),
                text,
                code: stored_code.clone(),
                frequency: *freq,
            });
        }
        candidates.sort_by(|a, b| {
            b.pinned
                .cmp(&a.pinned)
                .then_with(|| b.frequency.cmp(&a.frequency))
        });
        candidates
    }

    pub fn is_pinned(&self, schema: &str, text: &str) -> bool {
        self.pinned.contains(&(schema.to_owned(), text.to_owned()))
    }

    pub fn is_blocked(&self, schema: &str, text: &str) -> bool {
        self.blocked.contains(&(schema.to_owned(), text.to_owned()))
    }

    pub fn frequency(&self, schema: &str, text: &str) -> i64 {
        *self
            .frequency
            .get(&(schema.to_owned(), text.to_owned()))
            .unwrap_or(&0)
    }

    pub fn events(&self) -> &[UserEvent] {
        &self.events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learn_word_event_roundtrips() {
        let event = UserEvent::learn_word("device-a", "dp", "异步运行时", "yibu yunxingshi");
        assert_eq!(event.kind(), "learn_word");
        assert_eq!(event.text(), Some("异步运行时"));
    }

    #[test]
    fn pinned_candidate_is_tracked() {
        let mut store = UserStore::new("device-a");
        store.apply(UserEvent::learn_word("device-a", "dp", "你好", "ni hao"));
        store.apply(UserEvent::pin_candidate("device-a", "dp", "你好"));
        assert!(store.is_pinned("dp", "你好"));
    }

    #[test]
    fn deleted_word_is_removed_and_can_be_relearned() {
        let mut store = UserStore::new("device-a");
        store.apply(UserEvent::learn_word("device-a", "dp", "你好", "ni hao"));
        assert!(!store.query("ni hao").is_empty());
        store.apply(UserEvent::delete_word("device-a", "dp", "你好"));
        assert!(!store.is_pinned("dp", "你好"));
    }

    #[test]
    fn event_ids_are_monotonic() {
        let user_a = "device-a";
        let e1 = UserEvent::learn_word(user_a, "dp", "你", "ni");
        let e2 = UserEvent::learn_word(user_a, "dp", "好", "hao");
        let id1: u64 = e1.event_id().split(':').nth(1).unwrap().parse().unwrap();
        let id2: u64 = e2.event_id().split(':').nth(1).unwrap().parse().unwrap();
        assert!(id2 > id1);
    }
}
