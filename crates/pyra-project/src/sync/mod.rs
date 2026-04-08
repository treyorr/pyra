//! Project sync workflow building blocks.
//!
//! The `pyra sync` command has three distinct phases: loading project inputs,
//! validating or generating a lock file, and reconciling the centralized
//! environment from that lock. These modules keep those responsibilities
//! separate so future add/remove/run commands can reuse them without pulling
//! terminal or clap concerns into the domain layer.

mod install;
mod lockfile;
mod project_input;
mod selection;

pub use install::{EnvironmentInstaller, ReconciliationPlan};
pub use lockfile::{
    LockArtifact, LockDependencyRef, LockFile, LockPackage, LockSelection, LockToolPyraMetadata,
};
pub use project_input::{ProjectSyncInput, ProjectSyncInputLoader};
pub use selection::{SyncSelectionRequest, SyncSelectionResolver};
