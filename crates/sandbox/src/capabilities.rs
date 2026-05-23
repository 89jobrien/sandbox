use std::collections::HashSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Cap {
    ReadFs,
    WriteFs,
    RealFs,
    Network,
    NetAllowlist,
    Exec,
    EnvRead,
    EnvWrite,
    Signal,
}

#[derive(Debug, Clone)]
pub struct CapabilitySet {
    caps: HashSet<Cap>,
}

impl CapabilitySet {
    pub fn new(caps: impl IntoIterator<Item = Cap>) -> Self {
        Self {
            caps: caps.into_iter().collect(),
        }
    }

    pub fn default_set() -> Self {
        Self::new([Cap::ReadFs, Cap::WriteFs, Cap::EnvRead, Cap::EnvWrite])
    }

    // qual:api
    pub fn empty() -> Self {
        Self::new([])
    }

    pub fn has(&self, cap: Cap) -> bool {
        self.caps.contains(&cap)
    }

    pub fn check(&self, cap: Cap) -> crate::error::ShellResult<()> {
        if self.has(cap) {
            Ok(())
        } else {
            Err(crate::error::ShellError::CapabilityDenied(cap))
        }
    }

    pub fn grant(&mut self, cap: Cap) {
        self.caps.insert(cap);
    }
}
