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
