//! Deterministic language-model scoring for the decoder.
//!
//! Scores are fixed-point log-like values supplied by the model. Keeping the
//! decoder on integers makes candidate ordering reproducible across processes
//! and avoids NaN/partial-order failure modes in the realtime input path.

use std::collections::HashMap;

/// The bounded history exposed to a language model.
///
/// This is sufficient for unigram, bigram, and trigram models without
/// allocating a history vector for every expanded beam state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LanguageModelContext<'a> {
    pub previous_previous: Option<&'a str>,
    pub previous: Option<&'a str>,
}

/// Adds a context score when a lexeme is appended to a decode path.
///
/// Implementations must be deterministic and should return fixed-point scores.
/// The decoder uses saturating arithmetic, so an extreme model score cannot
/// overflow and unwind through a host DLL boundary.
pub trait LanguageModel: Send + Sync {
    fn score(&self, context: LanguageModelContext<'_>, token: &str) -> i64;
}

/// Default model used when no trained language model is configured.
///
/// A zero score preserves the pre-language-model candidate ordering exactly.
#[derive(Clone, Copy, Debug, Default)]
pub struct NullLanguageModel;

impl LanguageModel for NullLanguageModel {
    fn score(&self, _context: LanguageModelContext<'_>, _token: &str) -> i64 {
        0
    }
}

/// A compact backoff n-gram model using pre-quantized integer scores.
///
/// The lookup order is trigram, bigram, unigram, then `unknown_score`.
/// Training and probability quantization intentionally live outside the
/// realtime decoder; this type only performs bounded hash lookups.
#[derive(Clone, Debug)]
pub struct BackoffNgramModel {
    unigrams: HashMap<String, i64>,
    bigrams: HashMap<String, HashMap<String, i64>>,
    trigrams: HashMap<String, HashMap<String, HashMap<String, i64>>>,
    unknown_score: i64,
}

impl BackoffNgramModel {
    pub fn new(unknown_score: i64) -> Self {
        Self {
            unigrams: HashMap::new(),
            bigrams: HashMap::new(),
            trigrams: HashMap::new(),
            unknown_score,
        }
    }

    pub fn with_unigram(mut self, token: impl Into<String>, score: i64) -> Self {
        self.unigrams.insert(token.into(), score);
        self
    }

    pub fn with_bigram(
        mut self,
        previous: impl Into<String>,
        token: impl Into<String>,
        score: i64,
    ) -> Self {
        self.bigrams
            .entry(previous.into())
            .or_default()
            .insert(token.into(), score);
        self
    }

    pub fn with_trigram(
        mut self,
        previous_previous: impl Into<String>,
        previous: impl Into<String>,
        token: impl Into<String>,
        score: i64,
    ) -> Self {
        self.trigrams
            .entry(previous_previous.into())
            .or_default()
            .entry(previous.into())
            .or_default()
            .insert(token.into(), score);
        self
    }
}

impl LanguageModel for BackoffNgramModel {
    fn score(&self, context: LanguageModelContext<'_>, token: &str) -> i64 {
        if let (Some(previous_previous), Some(previous)) =
            (context.previous_previous, context.previous)
        {
            if let Some(score) = self
                .trigrams
                .get(previous_previous)
                .and_then(|next| next.get(previous))
                .and_then(|next| next.get(token))
            {
                return *score;
            }
        }
        if let Some(previous) = context.previous {
            if let Some(score) = self.bigrams.get(previous).and_then(|next| next.get(token)) {
                return *score;
            }
        }
        self.unigrams
            .get(token)
            .copied()
            .unwrap_or(self.unknown_score)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_prefers_the_most_specific_available_order() {
        let model = BackoffNgramModel::new(-40)
            .with_unigram("好", -30)
            .with_bigram("你", "好", -20)
            .with_trigram("祝", "你", "好", -10);

        assert_eq!(
            model.score(
                LanguageModelContext {
                    previous_previous: Some("祝"),
                    previous: Some("你"),
                },
                "好",
            ),
            -10
        );
        assert_eq!(
            model.score(
                LanguageModelContext {
                    previous_previous: Some("很"),
                    previous: Some("你"),
                },
                "好",
            ),
            -20
        );
        assert_eq!(model.score(LanguageModelContext::default(), "好"), -30);
        assert_eq!(model.score(LanguageModelContext::default(), "未知"), -40);
    }

    #[test]
    fn null_model_is_an_exact_zero_cost_default() {
        assert_eq!(
            NullLanguageModel.score(
                LanguageModelContext {
                    previous_previous: Some("任意"),
                    previous: Some("历史"),
                },
                "词",
            ),
            0
        );
    }
}
