//! Read-only models for `pyra outdated`.
//!
//! These types describe upgrade opportunities derived from current project
//! intent versus the currently locked package versions.

use camino::Utf8PathBuf;

/// One package with a newer available version than the currently locked
/// version under the same dependency intent.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct OutdatedPackage {
    pub package: String,
    pub current_version: String,
    pub latest_version: String,
    pub requirements: Vec<String>,
    pub declaration_scopes: Vec<String>,
}

/// Read-only outdated report for one project.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct OutdatedProjectOutcome {
    pub project_root: Utf8PathBuf,
    pub pyproject_path: Utf8PathBuf,
    pub pylock_path: Utf8PathBuf,
    pub project_id: String,
    pub python_version: String,
    pub checked_packages: usize,
    pub outdated_packages: Vec<OutdatedPackage>,
}

impl OutdatedProjectOutcome {
    pub fn has_outdated_packages(&self) -> bool {
        !self.outdated_packages.is_empty()
    }
}
