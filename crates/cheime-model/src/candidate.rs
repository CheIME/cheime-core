use crate::{CandidateId, DeploymentGeneration, Revision, SessionEpoch};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Candidate {
    pub id: CandidateId,
    pub text: String,
    pub annotation: Option<String>,
    pub source: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SessionStatus {
    Ready,
    Composing,
    CommitPending,
    Transparent,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CandidateSnapshot {
    pub epoch: SessionEpoch,
    pub revision: Revision,
    pub deployment: DeploymentGeneration,
    pub preedit: String,
    pub cursor: usize,
    pub candidates: Vec<Candidate>,
    pub highlighted: Option<CandidateId>,
    pub status: SessionStatus,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_owns_candidate_data() {
        let source = String::from("你");
        let snapshot = CandidateSnapshot {
            epoch: SessionEpoch::new(2),
            revision: Revision::new(3),
            deployment: DeploymentGeneration::new(5),
            preedit: String::from("ni"),
            cursor: 2,
            candidates: vec![Candidate {
                id: CandidateId::new(8),
                text: source.clone(),
                annotation: Some(String::from("nǐ")),
                source: String::from("builtin-test"),
            }],
            highlighted: Some(CandidateId::new(8)),
            status: SessionStatus::Composing,
        };
        drop(source);
        assert_eq!(snapshot.candidates[0].text, "你");
    }
}
