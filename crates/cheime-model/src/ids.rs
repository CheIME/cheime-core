use serde::{Deserialize, Serialize};

macro_rules! id_type {
    ($name:ident) => {
        #[derive(
            Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
        )]
        #[serde(transparent)]
        pub struct $name(u64);

        impl $name {
            #[must_use]
            pub const fn new(value: u64) -> Self {
                Self(value)
            }

            #[must_use]
            pub const fn get(self) -> u64 {
                self.0
            }
        }
    };
}

id_type!(ClientInstanceId);
id_type!(SessionId);
id_type!(SessionEpoch);
id_type!(Sequence);
id_type!(DeploymentGeneration);
id_type!(CandidateId);
id_type!(ActionId);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Revision(u64);

impl Revision {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    #[must_use]
    pub const fn next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_advances_monotonically() {
        let current = Revision::new(41);
        assert_eq!(current.next(), Some(Revision::new(42)));
    }

    #[test]
    fn revision_does_not_wrap() {
        assert_eq!(Revision::new(u64::MAX).next(), None);
    }

    #[test]
    fn typed_ids_do_not_compare_across_domains() {
        assert_eq!(SessionId::new(7).get(), 7);
        assert_eq!(SessionEpoch::new(7).get(), 7);
    }
}
