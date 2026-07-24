//! User data event model and persistence.
//!
//! User events (learn, delete, pin, block) are the source of truth.
//! They flow to both an in-memory cache (for fast query) and a SQLite
//! database (for persistence across restarts).
//!
//! # CheIME advantage over Rime
//!
//! Rime's user dictionary is an opaque binary blob with no structured
//! event log. CheIME's event model means:
//! - Every change is an auditable, time-stamped event
//! - Undo = replay events minus the one you want to undo
//! - Sync = replay event stream on other devices
//! - Diagnostics = inspect event history per-word

use parking_lot::Mutex;
use cheime_model::{ActionId, CommitToken, SessionEpoch, SessionId};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

// ── Helpers ──────────────────────────────────────────────────────────

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

// ── UserCandidate ───────────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UserCandidate {
    pub text: String,
    pub code: String,
    pub frequency: i64,
    pub pinned: bool,
    pub blocked: bool,
}

// ── UserEvent ───────────────────────────────────────────────────────

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
        emoji: String,
    },
    #[serde(rename = "set_app_preference")]
    SetAppPreference {
        event_id: String,
        timestamp: u64,
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

// ── UserStore with SQLite persistence ───────────────────────────────

#[derive(Debug)]
pub struct UserStore {
    #[allow(dead_code)]
    device_id: String,
    events: Vec<UserEvent>,
    frequency: HashMap<(String, String), i64>,
    code_texts: HashMap<(String, String), HashSet<String>>,
    text_frequency: HashMap<(String, String), i64>,
    pinned: HashSet<(String, String)>,
    blocked: HashSet<(String, String)>,
    deleted: HashSet<(String, String)>,
    db: Option<Arc<Mutex<Connection>>>,
    /// Words pending confirmation — not yet learned.
    /// Cleared (confirmed) when the next word is committed or on timeout.
    pending: HashMap<CommitToken, PendingPhrase>,
    legacy_next_action: u64,
    legacy_last: Option<CommitToken>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingPhrase {
    pub text: String,
    pub code: String,
    pub schema: String,
    pub deadline_ms: u64,
}

impl UserStore {
    pub fn new(device_id: &str) -> Self {
        Self {
            device_id: device_id.to_owned(),
            events: Vec::new(),
            frequency: HashMap::new(),
            code_texts: HashMap::new(),
            text_frequency: HashMap::new(),
            pinned: HashSet::new(),
            blocked: HashSet::new(),
            deleted: HashSet::new(),
            db: None,
            pending: HashMap::new(),
            legacy_next_action: 1,
            legacy_last: None,
        }
    }

    pub fn open(device_id: &str, db_path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        Self::init_schema(&conn)?;
        let mut store = Self {
            device_id: device_id.to_owned(),
            events: Vec::new(),
            frequency: HashMap::new(),
            code_texts: HashMap::new(),
            text_frequency: HashMap::new(),
            pinned: HashSet::new(),
            blocked: HashSet::new(),
            deleted: HashSet::new(),
            db: Some(Arc::new(Mutex::new(conn))),
            pending: HashMap::new(),
            legacy_next_action: 1,
            legacy_last: None,
        };
        store.load_from_db()?;
        Ok(store)
    }

    fn init_schema(db: &Connection) -> Result<(), rusqlite::Error> {
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS events (
                id          TEXT PRIMARY KEY,
                timestamp   INTEGER NOT NULL,
                operation   TEXT NOT NULL,
                schema_name TEXT NOT NULL DEFAULT '',
                text        TEXT NOT NULL DEFAULT '',
                code        TEXT NOT NULL DEFAULT '',
                delta       INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS pinned (
                schema_name TEXT NOT NULL, text TEXT NOT NULL,
                PRIMARY KEY (schema_name, text)
            );
            CREATE TABLE IF NOT EXISTS blocked (
                schema_name TEXT NOT NULL, text TEXT NOT NULL,
                PRIMARY KEY (schema_name, text)
            );
            CREATE TABLE IF NOT EXISTS deleted (
                schema_name TEXT NOT NULL, text TEXT NOT NULL,
                PRIMARY KEY (schema_name, text)
            );",
        )?;
        Ok(())
    }

    fn load_from_db(&mut self) -> Result<(), rusqlite::Error> {
        let db_arc = self.db.as_ref().expect("no db connection");
        let db = db_arc.lock();

        // Read events
        let mut stmt = db.prepare(
            "SELECT id,timestamp,operation,schema_name,text,code,delta FROM events ORDER BY id",
        )?;
        let rows: Vec<(String, u64, String, String, String, String, i64)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();
        drop(stmt);

        let mut event_list = Vec::with_capacity(rows.len());
        for (eid, ts, op, schema, text, code, delta) in &rows {
            let ev = match op.as_str() {
                "learn_word" => UserEvent::LearnWord {
                    event_id: eid.clone(),
                    timestamp: *ts,
                    schema: schema.clone(),
                    text: text.clone(),
                    code: code.clone(),
                    delta: *delta,
                },
                "update_frequency" => UserEvent::UpdateFrequency {
                    event_id: eid.clone(),
                    timestamp: *ts,
                    schema: schema.clone(),
                    text: text.clone(),
                    delta: *delta,
                },
                "delete_word" => UserEvent::DeleteWord {
                    event_id: eid.clone(),
                    timestamp: *ts,
                    schema: schema.clone(),
                    text: text.clone(),
                },
                "pin_candidate" => UserEvent::PinCandidate {
                    event_id: eid.clone(),
                    timestamp: *ts,
                    schema: schema.clone(),
                    text: text.clone(),
                },
                "block_candidate" => UserEvent::BlockCandidate {
                    event_id: eid.clone(),
                    timestamp: *ts,
                    schema: schema.clone(),
                    text: text.clone(),
                },
                _ => continue,
            };
            event_list.push(ev);
        }

        // Read pinned/blocked/deleted tables
        let read_strs =
            |db: &Connection, table: &str| -> Result<Vec<(String, String)>, rusqlite::Error> {
                let sql = format!("SELECT schema_name, text FROM {table}");
                let mut s = db.prepare(&sql)?;
                let rows: Vec<_> = s
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .filter_map(|r| r.ok())
                    .collect();
                Ok(rows)
            };

        let pinned_rows = read_strs(&db, "pinned")?;
        let blocked_rows = read_strs(&db, "blocked")?;
        let deleted_rows = read_strs(&db, "deleted")?;
        drop(db);

        // Apply to cache
        for ev in &event_list {
            self.apply_to_cache(ev);
        }
        self.events = event_list;
        for (s, t) in pinned_rows {
            self.pinned.insert((s, t));
        }
        for (s, t) in blocked_rows {
            self.blocked.insert((s, t));
        }
        for (s, t) in deleted_rows {
            self.deleted.insert((s, t));
        }

        Ok(())
    }

    pub fn apply(&mut self, event: UserEvent) {
        if let Some(db) = &self.db {
            let _ = Self::persist_event(&db.lock(), &event);
        }
        self.apply_to_cache(&event);
        self.events.push(event);
    }

    fn persist_event(db: &Connection, event: &UserEvent) -> Result<(), rusqlite::Error> {
        match event {
            UserEvent::LearnWord {
                event_id,
                timestamp,
                schema,
                text,
                code,
                delta,
            } => {
                db.execute(
                    "INSERT OR REPLACE INTO events VALUES (?1,?2,'learn_word',?3,?4,?5,?6)",
                    params![event_id, *timestamp as i64, schema, text, code, *delta],
                )?;
            }
            UserEvent::UpdateFrequency {
                event_id,
                timestamp,
                schema,
                text,
                delta,
            } => {
                db.execute(
                    "INSERT OR REPLACE INTO events VALUES (?1,?2,'update_frequency',?3,?4,'',?5)",
                    params![event_id, *timestamp as i64, schema, text, *delta],
                )?;
            }
            UserEvent::DeleteWord {
                event_id,
                timestamp,
                schema,
                text,
            } => {
                db.execute("INSERT OR REPLACE INTO events(id,timestamp,operation,schema_name,text) VALUES (?1,?2,'delete_word',?3,?4)",
                    params![event_id, *timestamp as i64, schema, text])?;
                db.execute(
                    "INSERT OR REPLACE INTO deleted VALUES (?1,?2)",
                    params![schema, text],
                )?;
            }
            UserEvent::PinCandidate {
                event_id,
                timestamp,
                schema,
                text,
            } => {
                db.execute("INSERT OR REPLACE INTO events(id,timestamp,operation,schema_name,text) VALUES (?1,?2,'pin_candidate',?3,?4)",
                    params![event_id, *timestamp as i64, schema, text])?;
                db.execute(
                    "INSERT OR REPLACE INTO pinned VALUES (?1,?2)",
                    params![schema, text],
                )?;
            }
            UserEvent::BlockCandidate {
                event_id,
                timestamp,
                schema,
                text,
            } => {
                db.execute("INSERT OR REPLACE INTO events(id,timestamp,operation,schema_name,text) VALUES (?1,?2,'block_candidate',?3,?4)",
                    params![event_id, *timestamp as i64, schema, text])?;
                db.execute(
                    "INSERT OR REPLACE INTO blocked VALUES (?1,?2)",
                    params![schema, text],
                )?;
            }
            _ => {}
        }
        Ok(())
    }

    fn apply_to_cache(&mut self, event: &UserEvent) {
        match event {
            UserEvent::LearnWord {
                schema,
                text,
                code,
                delta,
                ..
            } => {
                let k = (schema.clone(), text.clone());
                if self.deleted.contains(&k) {
                    self.deleted.remove(&k);
                }
                *self
                    .frequency
                    .entry((schema.clone(), code.clone()))
                    .or_default() += delta;
                self.code_texts
                    .entry((schema.clone(), code.clone()))
                    .or_default()
                    .insert(text.clone());
                *self.text_frequency.entry(k).or_default() += delta;
            }
            UserEvent::UpdateFrequency {
                schema,
                text,
                delta,
                ..
            } => {
                let k = (schema.clone(), text.clone());
                if !self.deleted.contains(&k) {
                    *self.text_frequency.entry(k).or_default() += delta;
                }
            }
            UserEvent::DeleteWord { schema, text, .. } => {
                let k = (schema.clone(), text.clone());
                self.deleted.insert(k);
                self.pinned.remove(&(schema.clone(), text.clone()));
            }
            UserEvent::PinCandidate { schema, text, .. } => {
                self.pinned.insert((schema.clone(), text.clone()));
            }
            UserEvent::BlockCandidate { schema, text, .. } => {
                self.blocked.insert((schema.clone(), text.clone()));
            }
            _ => {}
        }
    }

    // ── Smart learning with typo detection ──────────────────────────

    pub fn stage_phrase(
        &mut self,
        token: CommitToken,
        mut phrase: PendingPhrase,
        deadline_ms: u64,
    ) {
        phrase.deadline_ms = deadline_ms;
        self.pending.insert(token, phrase);
    }

    pub fn cancel_phrase(&mut self, token: CommitToken) -> bool {
        self.pending.remove(&token).is_some()
    }

    pub fn confirm_expired(&mut self, now_ms: u64) {
        let mut expired: Vec<_> = self
            .pending
            .iter()
            .filter_map(|(token, phrase)| {
                (phrase.deadline_ms <= now_ms).then_some(*token)
            })
            .collect();
        expired.sort();
        for token in expired {
            if let Some(phrase) = self.pending.remove(&token) {
                self.apply(UserEvent::learn_word(
                    &self.device_id,
                    &phrase.schema,
                    &phrase.text,
                    &phrase.code,
                ));
            }
        }
    }

    /// Compatibility wrapper for older clients. New engine sessions use
    /// action-addressable `stage_phrase`.
    pub fn commit_pending(&mut self, text: &str, code: &str, schema: &str) {
        self.confirm_all_pending();
        let token = CommitToken {
            session: SessionId::new(0),
            epoch: SessionEpoch::new(0),
            action_id: ActionId::new(self.legacy_next_action),
        };
        self.legacy_next_action += 1;
        self.legacy_last = Some(token);
        self.stage_phrase(token, PendingPhrase {
            text: text.to_owned(),
            code: code.to_owned(),
            schema: schema.to_owned(),
            deadline_ms: u64::MAX,
        }, u64::MAX);
    }

    /// User hit backspace quickly after a commit — this was a typo.
    /// Remove the most recently staged word without learning it.
    pub fn undo_last(&mut self) -> bool {
        self.legacy_last
            .take()
            .is_some_and(|token| self.cancel_phrase(token))
    }

    /// Confirm all pending words as actually learned.
    pub fn confirm_all_pending(&mut self) {
        let mut tokens: Vec<_> = self.pending.keys().copied().collect();
        tokens.sort();
        for token in tokens {
            let Some(phrase) = self.pending.remove(&token) else {
                continue;
            };
            self.apply(UserEvent::learn_word(
                &self.device_id,
                &phrase.schema,
                &phrase.text,
                &phrase.code,
            ));
        }
        self.legacy_last = None;
    }

    pub fn query(&self, code: &str) -> Vec<UserCandidate> {
        self.query_matching(|stored| stored == code)
    }

    pub fn query_prefix(&self, prefix: &str) -> Vec<UserCandidate> {
        self.query_matching(|stored| stored.starts_with(prefix))
    }

    fn query_matching(&self, matches: impl Fn(&str) -> bool) -> Vec<UserCandidate> {
        let mut cs = Vec::new();
        let mut seen = HashSet::new();
        for ((schema, sc), texts) in &self.code_texts {
            if !matches(sc) {
                continue;
            }
            for text in texts {
                if !seen.insert(text.clone()) {
                    continue;
                }
                let freq = self
                    .text_frequency
                    .get(&(schema.clone(), text.clone()))
                    .copied()
                    .unwrap_or(0);
                cs.push(UserCandidate {
                    pinned: self.pinned.contains(&(schema.clone(), text.clone())),
                    blocked: self.blocked.contains(&(schema.clone(), text.clone())),
                    text: text.clone(),
                    code: sc.clone(),
                    frequency: freq,
                });
            }
        }
        cs.sort_by(|a, b| {
            b.pinned
                .cmp(&a.pinned)
                .then_with(|| b.frequency.cmp(&a.frequency))
        });
        cs
    }
    pub fn is_pinned(&self, schema: &str, text: &str) -> bool {
        self.pinned.contains(&(schema.to_owned(), text.to_owned()))
    }
    pub fn is_blocked(&self, schema: &str, text: &str) -> bool {
        self.blocked.contains(&(schema.to_owned(), text.to_owned()))
    }
    pub fn frequency(&self, schema: &str, text: &str) -> i64 {
        *self
            .text_frequency
            .get(&(schema.to_owned(), text.to_owned()))
            .unwrap_or(&0)
    }
    pub fn events(&self) -> &[UserEvent] {
        &self.events
    }
    pub fn is_persistent(&self) -> bool {
        self.db.is_some()
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use cheime_model::{ActionId, CommitToken, SessionEpoch, SessionId};

    fn token_for_session(session: u64, action: u64) -> CommitToken {
        CommitToken {
            session: SessionId::new(session),
            epoch: SessionEpoch::new(1),
            action_id: ActionId::new(action),
        }
    }

    fn phrase(text: &str, code: &str) -> PendingPhrase {
        PendingPhrase {
            text: text.to_owned(),
            code: code.to_owned(),
            schema: String::from("qp"),
            deadline_ms: 0,
        }
    }

    #[test]
    fn pending_phrase_is_learned_only_at_deadline() {
        let mut store = UserStore::new("test");
        let token = token_for_session(1, 1);
        store.stage_phrase(token, phrase("旎皓", "ni hao"), 10_000);
        store.confirm_expired(9_999);
        assert!(store.query("ni hao").is_empty());
        store.confirm_expired(10_000);
        assert_eq!(store.query("ni hao")[0].text, "旎皓");
    }

    #[test]
    fn rollback_before_deadline_never_persists_phrase() {
        let mut store = UserStore::new("test");
        let token = token_for_session(1, 2);
        store.stage_phrase(token, phrase("旎皓", "ni hao"), 10_000);
        assert!(store.cancel_phrase(token));
        assert!(!store.cancel_phrase(token));
        store.confirm_expired(20_000);
        assert!(store.query("ni hao").is_empty());
    }

    #[test]
    fn concurrent_session_action_ids_do_not_collide() {
        let mut store = UserStore::new("test");
        store.stage_phrase(
            token_for_session(1, 1),
            phrase("甲", "jia"),
            10,
        );
        store.stage_phrase(
            token_for_session(2, 1),
            phrase("乙", "yi"),
            10,
        );
        store.confirm_expired(10);
        assert_eq!(store.query("jia")[0].text, "甲");
        assert_eq!(store.query("yi")[0].text, "乙");
    }

    #[test]
    fn learn_word_increases_frequency() {
        let mut store = UserStore::new("test");
        store.apply(UserEvent::learn_word("test", "qp", "测试", "cs"));
        assert_eq!(store.frequency("qp", "测试"), 1);
    }

    #[test]
    fn query_returns_sorted() {
        let mut store = UserStore::new("test");
        store.apply(UserEvent::learn_word("test", "qp", "测试", "cs"));
        store.apply(UserEvent::learn_word("test", "qp", "测试", "cs"));
        let r = store.query("cs");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].frequency, 2);
    }

    #[test]
    fn pinned_first() {
        let mut store = UserStore::new("test");
        store.apply(UserEvent::learn_word("test", "qp", "测试", "cs"));
        store.apply(UserEvent::learn_word("test", "qp", "车市", "cs"));
        store.apply(UserEvent::pin_candidate("test", "qp", "车市"));
        let r = store.query("cs");
        assert!(r[0].pinned);
    }

    #[test]
    fn sqlite_roundtrip() {
        let dir = std::env::temp_dir();
        let path = dir.join("cheime_test_userdata.db");
        let _ = std::fs::remove_file(&path);

        {
            let mut store = UserStore::open("test", &path).unwrap();
            store.apply(UserEvent::learn_word("test", "qp", "测试", "cs"));
            store.apply(UserEvent::learn_word("test", "qp", "测试", "cs"));
            store.apply(UserEvent::pin_candidate("test", "qp", "测试"));
        }

        {
            let store = UserStore::open("test", &path).unwrap();
            assert_eq!(store.frequency("qp", "测试"), 2);
            assert!(store.is_pinned("qp", "测试"));
        }

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn commit_then_undo_does_not_learn() {
        let mut store = UserStore::new("test");
        store.commit_pending("你好", "nihao", "qp");
        assert!(store.undo_last()); // typo — backspace
        store.confirm_all_pending(); // should be no-op
        assert_eq!(store.frequency("qp", "你好"), 0);
    }

    #[test]
    fn commit_then_continue_confirms_previous() {
        let mut store = UserStore::new("test");
        store.commit_pending("测试", "ceshi", "qp");
        // User didn't backspace — next word commit confirms previous
        store.commit_pending("你好", "nihao", "qp");
        assert_eq!(store.frequency("qp", "测试"), 1);
        // "你好" is still pending
        store.confirm_all_pending();
        assert_eq!(store.frequency("qp", "你好"), 1);
    }

    // ── Concurrency ─────────────────────────────────────────────

    #[test]
    fn concurrent_reads_do_not_deadlock() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        let mut store = UserStore::new("concurrent-test");
        store.apply(UserEvent::learn_word("test", "qp", "你好", "nh"));
        store.apply(UserEvent::learn_word("test", "qp", "世界", "sj"));

        let shared = Arc::new(parking_lot::Mutex::new(store));
        let barrier = Arc::new(Barrier::new(5));

        let mut handles = Vec::new();
        for _ in 0..4 {
            let s = Arc::clone(&shared);
            let b = Arc::clone(&barrier);
            handles.push(thread::spawn(move || {
                b.wait();
                for _ in 0..50 {
                    let store = s.lock();
                    let _r = store.query("nh");
                    drop(store);
                }
            }));
        }

        // Writer thread
        let s = Arc::clone(&shared);
        let b = Arc::clone(&barrier);
        let writer = thread::spawn(move || {
            b.wait();
            for i in 0..50 {
                let mut store = s.lock();
                store.apply(UserEvent::learn_word(
                    "test",
                    "qp",
                    &format!("word_{i}"),
                    "wd",
                ));
                drop(store);
            }
        });

        for h in handles {
            h.join().unwrap();
        }
        writer.join().unwrap();

        // Verify store is still usable
        let store = shared.lock();
        let r = store.query("nh");
        assert!(!r.is_empty());
    }

    #[test]
    fn concurrent_writes_are_serialized() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        let store = UserStore::new("concurrent-writes");
        let shared = Arc::new(parking_lot::Mutex::new(store));
        let barrier = Arc::new(Barrier::new(4));

        let mut handles = Vec::new();
        for t in 0..4 {
            let s = Arc::clone(&shared);
            let b = Arc::clone(&barrier);
            handles.push(thread::spawn(move || {
                b.wait();
                for i in 0..25 {
                    let mut store = s.lock();
                    store.apply(UserEvent::learn_word(
                        "test",
                        "qp",
                        &format!("thread{t}_word{i}"),
                        "wd",
                    ));
                    drop(store);
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        let store = shared.lock();
        // 4 threads × 25 events each = 100 events
        assert_eq!(store.events().len(), 100);
        // All 100 words should be queryable
        let results = store.query("wd");
        assert_eq!(results.len(), 100);
    }
}
