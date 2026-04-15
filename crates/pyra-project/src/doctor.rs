//! Read-only project diagnostics models for `pyra doctor`.
//!
//! The doctor command reports actionable project and environment health issues
//! without mutating lock, manifest, or environment state.

use camino::Utf8PathBuf;

/// Stable diagnostic code families so orchestration and future tests can
/// reason about doctor findings without parsing display text.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DoctorIssueCode {
    InterpreterMismatch,
    MissingLock,
    StaleLock,
    EnvironmentDrift,
}

/// One actionable finding produced by `pyra doctor`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DoctorIssue {
    pub code: DoctorIssueCode,
    pub summary: String,
    pub detail: String,
    pub suggestion: String,
}

/// Read-only diagnostic outcome for one project.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DoctorProjectOutcome {
    pub project_root: Utf8PathBuf,
    pub pyproject_path: Utf8PathBuf,
    pub pylock_path: Utf8PathBuf,
    pub project_id: String,
    pub python_selector: String,
    pub python_version: Option<String>,
    pub issues: Vec<DoctorIssue>,
}

impl DoctorProjectOutcome {
    pub fn has_issues(&self) -> bool {
        !self.issues.is_empty()
    }
}
