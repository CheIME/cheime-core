use crate::{CandidateId, DeploymentGeneration, Revision, SessionEpoch};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Candidate {
    pub id: CandidateId,
    pub text: String,
    pub annotation: Option<String>,
    pub source: String,
    #[serde(default)]
    pub is_emoji: bool,
}

impl Candidate {
    pub fn text(id: CandidateId, text: impl Into<String>, source: impl Into<String>) -> Self {
        Self { id, text: text.into(), annotation: None, source: source.into(), is_emoji: false }
    }
    pub fn emoji(id: CandidateId, emoji: impl Into<String>) -> Self {
        Self { id, text: emoji.into(), annotation: None, source: "emoji".into(), is_emoji: true }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SessionStatus { Ready, Composing, CommitPending, Transparent }

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CandidateSnapshot {
    pub epoch: SessionEpoch, pub revision: Revision, pub deployment: DeploymentGeneration,
    pub preedit: String, pub cursor: usize, pub candidates: Vec<Candidate>,
    pub highlighted: Option<CandidateId>, pub status: SessionStatus,
    pub page_size: usize, pub page: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_owns_candidate_data() {
        let source = String::from("你");
        let s = CandidateSnapshot {
            epoch: SessionEpoch::new(2), revision: Revision::new(3), deployment: DeploymentGeneration::new(5),
            preedit: "ni".into(), cursor: 2,
            candidates: vec![Candidate { id: CandidateId::new(8), text: source.clone(), annotation: Some("nǐ".into()), source: "builtin-test".into(), is_emoji: false }],
            highlighted: Some(CandidateId::new(8)), status: SessionStatus::Composing,
            page_size: 9, page: 0,
        };
        drop(source);
        assert_eq!(s.candidates[0].text, "你");
    }
    #[test]
    fn emoji_candidate_has_flag() {
        let c = Candidate::emoji(CandidateId::new(1), "😄");
        assert!(c.is_emoji);
        assert_eq!(c.source, "emoji");
    }
}
