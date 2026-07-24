use crate::decoder::SelectedLexeme;
use cheime_model::CommitToken;
use cheime_user_data::{PendingPhrase, UserEvent, UserStore};
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

pub const LEARNING_DELAY_MS: u64 = 10_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitRecord {
    pub text: String,
    pub canonical_code: String,
    pub schema: String,
    pub lexemes: Vec<SelectedLexeme>,
    pub exact_phrase: bool,
}

pub trait Clock: Send + Sync {
    fn now_ms(&self) -> u64;
}

#[derive(Debug)]
pub struct SystemClock {
    origin: Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        self.origin
            .elapsed()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX)
    }
}

#[derive(Debug, Default)]
pub struct FakeClock {
    now_ms: AtomicU64,
}

impl FakeClock {
    pub fn new(now_ms: u64) -> Self {
        Self {
            now_ms: AtomicU64::new(now_ms),
        }
    }

    pub fn set(&self, now_ms: u64) {
        self.now_ms.store(now_ms, Ordering::Release);
    }
}

impl Clock for FakeClock {
    fn now_ms(&self) -> u64 {
        self.now_ms.load(Ordering::Acquire)
    }
}

pub struct LearningService {
    store: Arc<Mutex<UserStore>>,
    clock: Arc<dyn Clock>,
}

impl LearningService {
    pub fn new(store: Arc<Mutex<UserStore>>, clock: Arc<dyn Clock>) -> Self {
        Self { store, clock }
    }

    pub fn production(store: Arc<Mutex<UserStore>>) -> Self {
        Self::new(store, Arc::new(SystemClock::new()))
    }

    pub fn store(&self) -> Arc<Mutex<UserStore>> {
        Arc::clone(&self.store)
    }

    pub fn commit_applied(&self, token: CommitToken, record: CommitRecord) {
        if record.lexemes.is_empty() {
            return;
        }
        let mut store = self.store.lock();
        if record.exact_phrase {
            store.apply(UserEvent::learn_word(
                "engine",
                &record.schema,
                &record.text,
                &record.canonical_code,
            ));
            return;
        }
        let deadline = self.clock.now_ms().saturating_add(LEARNING_DELAY_MS);
        store.stage_phrase(
            token,
            PendingPhrase {
                text: record.text,
                code: record.canonical_code,
                schema: record.schema,
                deadline_ms: deadline,
            },
            deadline,
        );
    }

    pub fn rollback_learning(&self, token: CommitToken) -> bool {
        self.store.lock().cancel_phrase(token)
    }

    pub fn confirm_expired(&self) {
        self.store.lock().confirm_expired(self.clock.now_ms());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cheime_model::{ActionId, SessionEpoch, SessionId};

    fn token(action: u64) -> CommitToken {
        CommitToken {
            session: SessionId::new(1),
            epoch: SessionEpoch::new(1),
            action_id: ActionId::new(action),
        }
    }

    fn novel_record() -> CommitRecord {
        CommitRecord {
            text: String::from("旎皓"),
            canonical_code: String::from("ni hao"),
            schema: String::from("qp"),
            lexemes: vec![
                SelectedLexeme::test("旎", "ni"),
                SelectedLexeme::test("皓", "hao"),
            ],
            exact_phrase: false,
        }
    }

    #[test]
    fn novel_phrase_waits_for_shared_clock_deadline() {
        let store = Arc::new(Mutex::new(UserStore::new("test")));
        let clock = Arc::new(FakeClock::new(0));
        let service = LearningService::new(store.clone(), clock.clone());
        service.commit_applied(token(1), novel_record());
        clock.set(LEARNING_DELAY_MS - 1);
        service.confirm_expired();
        assert!(store.lock().query("ni hao").is_empty());
        clock.set(LEARNING_DELAY_MS);
        service.confirm_expired();
        assert_eq!(store.lock().query("ni hao")[0].text, "旎皓");
    }

    #[test]
    fn rollback_is_idempotent_before_deadline() {
        let store = Arc::new(Mutex::new(UserStore::new("test")));
        let clock = Arc::new(FakeClock::new(0));
        let service = LearningService::new(store.clone(), clock.clone());
        service.commit_applied(token(1), novel_record());
        assert!(service.rollback_learning(token(1)));
        assert!(!service.rollback_learning(token(1)));
        clock.set(LEARNING_DELAY_MS);
        service.confirm_expired();
        assert!(store.lock().query("ni hao").is_empty());
    }

    #[test]
    fn non_lexical_candidates_are_not_learned() {
        let store = Arc::new(Mutex::new(UserStore::new("test")));
        let clock = Arc::new(FakeClock::new(0));
        let service = LearningService::new(store.clone(), clock);
        let mut record = novel_record();
        record.lexemes.clear();
        service.commit_applied(token(1), record);
        service.confirm_expired();
        assert!(store.lock().query("ni hao").is_empty());
    }
}
