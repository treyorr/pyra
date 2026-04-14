//! Helpers for reusing one logical resolution request across multiple targets.
//!
//! Multi-target lock generation still resolves one environment at a time. This
//! template keeps the shared root set and index URL stable while the caller
//! swaps in each target environment explicitly.

use crate::{ResolutionRequest, ResolutionRoot, ResolverEnvironment};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ResolutionRequestTemplate {
    roots: Vec<ResolutionRoot>,
    index_url: String,
}

impl ResolutionRequestTemplate {
    pub fn new(roots: Vec<ResolutionRoot>, index_url: impl Into<String>) -> Self {
        Self {
            roots,
            index_url: index_url.into(),
        }
    }

    pub fn for_environment(&self, environment: ResolverEnvironment) -> ResolutionRequest {
        ResolutionRequest {
            environment,
            roots: self.roots.clone(),
            index_url: self.index_url.clone(),
        }
    }
}
