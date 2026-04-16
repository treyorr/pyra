//! Project sync workflow building blocks.
//!
//! The `pyra sync` command has three distinct phases: loading project inputs,
//! validating or generating a lock file, and reconciling the centralized
//! environment from that lock. These modules keep those responsibilities
//! separate so future add/remove/run commands can reuse them without pulling
//! terminal or clap concerns into the domain layer.

mod install;
mod lockfile;
mod marker;
mod project_input;
mod selection;

pub use install::{ApplyEnvironmentRequest, EnvironmentInstaller, ReconciliationPlan};
pub use lockfile::{
    CURRENT_RESOLUTION_STRATEGY, LockArtifact, LockDependencyRef, LockEnvironment, LockFile,
    LockFreshness, LockPackage, LockSelection, MULTI_TARGET_RESOLUTION_STRATEGY,
};
pub(crate) use marker::{LockMarker, LockMarkerClause};
pub use project_input::{ProjectSyncInput, ProjectSyncInputLoader};
pub use selection::{SyncSelectionRequest, SyncSelectionResolver};
