#![forbid(unsafe_code)]

use crate::index::CompiledIndex;
use cheime_model::DeploymentGeneration;
use std::sync::Arc;
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeploymentHandle(Arc<CompiledIndex>);

impl DeploymentHandle {
    pub fn index(&self) -> &CompiledIndex {
        &self.0
    }

    pub fn generation(&self) -> DeploymentGeneration {
        self.0.generation
    }
}

#[derive(Clone, Debug, Default)]
pub struct DeploymentManager {
    current: Option<DeploymentHandle>,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum DeployError {
    #[error("cannot deploy an empty index")]
    EmptyIndex,
}

impl DeploymentManager {
    pub fn new() -> Self {
        Self { current: None }
    }

    pub fn deploy(&mut self, index: CompiledIndex) -> Result<DeploymentHandle, DeployError> {
        if index.total_entries == 0 {
            return Err(DeployError::EmptyIndex);
        }
        let handle = DeploymentHandle(Arc::new(index));
        self.current = Some(handle.clone());
        Ok(handle)
    }

    pub fn current(&self) -> Option<&DeploymentHandle> {
        self.current.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::body::DictEntry;

    fn entry(text: &str, code: &str) -> DictEntry {
        DictEntry {
            text: text.into(),
            code: code.into(),
            weight: Some(10),
            stem: None,
        }
    }

    #[test]
    fn deploy_returns_handle_with_index() {
        let index = CompiledIndex::build(vec![entry("你", "ni")], DeploymentGeneration::new(1));
        let mut mgr = DeploymentManager::new();
        let handle = mgr.deploy(index).unwrap();
        assert_eq!(handle.generation().get(), 1);
        assert_eq!(handle.index().query("ni")[0].text, "你");
    }

    #[test]
    fn old_handle_stays_valid_after_new_deploy() {
        let mut mgr = DeploymentManager::new();
        let old = mgr
            .deploy(CompiledIndex::build(
                vec![entry("你", "ni")],
                DeploymentGeneration::new(1),
            ))
            .unwrap();
        let _new = mgr
            .deploy(CompiledIndex::build(
                vec![entry("好", "hao")],
                DeploymentGeneration::new(2),
            ))
            .unwrap();
        assert_eq!(old.index().query("ni")[0].text, "你");
        assert_eq!(mgr.current().unwrap().generation().get(), 2);
    }

    #[test]
    fn empty_index_is_rejected() {
        let index = CompiledIndex::build(vec![], DeploymentGeneration::new(1));
        let mut mgr = DeploymentManager::new();
        assert_eq!(mgr.deploy(index), Err(DeployError::EmptyIndex));
    }
}
