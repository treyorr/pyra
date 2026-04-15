//! Shared typed error and exit-code contract primitives.
//!
//! This crate is intentionally small so all higher-level crates can map
//! user-facing failures onto one stable process-exit contract.

use std::error::Error;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ErrorKind {
    User,
    System,
    Internal,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ErrorReport {
    pub kind: ErrorKind,
    pub summary: String,
    pub detail: Option<String>,
    pub suggestion: Option<String>,
    pub verbose_detail: Option<String>,
}

impl ErrorReport {
    pub fn new(kind: ErrorKind, summary: impl Into<String>) -> Self {
        Self {
            kind,
            summary: summary.into(),
            detail: None,
            suggestion: None,
            verbose_detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    pub fn with_verbose_detail(mut self, verbose_detail: impl Into<String>) -> Self {
        self.verbose_detail = Some(verbose_detail.into());
        self
    }
}

pub trait UserFacingError: Error {
    fn report(&self) -> ErrorReport;
}

/// Stable process exit code for successful command execution.
pub const EXIT_CODE_SUCCESS: i32 = 0;
/// Stable process exit code for user/input failures.
pub const EXIT_CODE_USER: i32 = 2;
/// Stable process exit code for system/IO failures.
pub const EXIT_CODE_SYSTEM: i32 = 3;
/// Stable process exit code for internal invariant failures.
pub const EXIT_CODE_INTERNAL: i32 = 4;
/// Fallback process exit code when an external command fails without an
/// explicit status code.
pub const EXIT_CODE_EXTERNAL_FALLBACK: i32 = 1;

/// Maps typed failure kinds onto the shared non-success process exit codes.
pub const fn exit_code_from_error_kind(kind: ErrorKind) -> i32 {
    match kind {
        ErrorKind::User => EXIT_CODE_USER,
        ErrorKind::System => EXIT_CODE_SYSTEM,
        ErrorKind::Internal => EXIT_CODE_INTERNAL,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_code_constants_are_stable() {
        assert_eq!(EXIT_CODE_SUCCESS, 0);
        assert_eq!(EXIT_CODE_USER, 2);
        assert_eq!(EXIT_CODE_SYSTEM, 3);
        assert_eq!(EXIT_CODE_INTERNAL, 4);
        assert_eq!(EXIT_CODE_EXTERNAL_FALLBACK, 1);
    }

    #[test]
    fn error_kind_mapping_is_stable() {
        assert_eq!(exit_code_from_error_kind(ErrorKind::User), EXIT_CODE_USER);
        assert_eq!(
            exit_code_from_error_kind(ErrorKind::System),
            EXIT_CODE_SYSTEM
        );
        assert_eq!(
            exit_code_from_error_kind(ErrorKind::Internal),
            EXIT_CODE_INTERNAL
        );
    }
}
