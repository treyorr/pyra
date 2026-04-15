//! Lock-refresh outcome models for `pyra update`.
//!
//! `pyra update` refreshes resolved lock state from current dependency intent
//! without mutating declared requirements in `pyproject.toml`.

use camino::Utf8PathBuf;

/// Package-level lock change kinds reported by `pyra update`.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum UpdatePackageChangeKind {
    Added,
    Removed,
    Updated,
}

/// One package-level difference between the previous lock and a refreshed lock.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UpdatePackageChange {
    pub kind: UpdatePackageChangeKind,
    pub package: String,
    pub previous_version: Option<String>,
    pub resolved_version: Option<String>,
}

/// Outcome for one `pyra update` invocation.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UpdateProjectOutcome {
    pub project_root: Utf8PathBuf,
    pub pyproject_path: Utf8PathBuf,
    pub pylock_path: Utf8PathBuf,
    pub project_id: String,
    pub python_version: String,
    pub dry_run: bool,
    pub previous_lock_exists: bool,
    pub total_packages: usize,
    pub unchanged_packages: usize,
    pub package_changes: Vec<UpdatePackageChange>,
}

impl UpdateProjectOutcome {
    pub fn has_changes(&self) -> bool {
        !self.package_changes.is_empty()
    }
}
