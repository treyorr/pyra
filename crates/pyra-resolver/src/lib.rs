//! Dependency resolution and index access for Pyra sync workflows.
//!
//! This crate intentionally stops at typed resolution results. Project parsing,
//! lockfile persistence, and environment reconciliation stay in `pyra-project`.

mod error;
mod marker;
mod metadata;
mod model;
mod provider;
mod simple;
#[cfg(test)]
mod test_support;
mod version;

pub use error::{ResolverConflict, ResolverError};
pub use marker::ResolverEnvironment;
pub use model::{
    ArtifactKind, ArtifactRecord, PackageDependencyRecord, ResolutionRequest, ResolutionRoot,
    ResolutionRootToken, ResolutionRootTokenKind, ResolvedPackage, Resolver,
};
