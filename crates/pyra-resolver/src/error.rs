//! Typed resolver errors.

use thiserror::Error;

/// Structured conflict context that higher layers can render without parsing
/// the full solver report string.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ResolverConflict {
    pub summary: String,
    pub report: String,
}

#[derive(Debug, Error)]
pub enum ResolverError {
    #[error("failed to request package metadata from {url}")]
    RequestIndex {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("failed to read package metadata fixture from {path}")]
    ReadIndexFile {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to decode simple index response for {package}")]
    ParseIndex {
        package: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to request distribution metadata from {url}")]
    RequestMetadata {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("failed to read distribution metadata fixture from {path}")]
    ReadMetadataFile {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("package `{package}` does not expose core metadata through the simple API")]
    MissingCoreMetadata { package: String },
    #[error(
        "package `{package}` has no installable files for the current interpreter and platform"
    )]
    NoInstallableArtifacts { package: String },
    #[error("package `{package}` has no version satisfying `{requirement}`")]
    NoMatchingVersion {
        package: String,
        requirement: String,
    },
    #[error("package `{package}` uses an unsupported direct URL requirement")]
    UnsupportedDirectUrlRequirement { package: String },
    #[error("package `{package}` uses an unsupported version specifier `{specifier}`")]
    UnsupportedVersionSpecifier { package: String, specifier: String },
    #[error("failed to parse version `{value}` for package `{package}`")]
    ParseVersion { package: String, value: String },
    #[error("failed to parse dependency `{value}` in package `{package}`")]
    ParseRequirement { package: String, value: String },
    #[error("resolution failed")]
    Solve {
        detail: String,
        conflict: Option<ResolverConflict>,
    },
}

impl ResolverError {
    /// Return the concise conflict summary when the solver found an
    /// incompatibility that should surface in normal CLI output.
    pub fn conflict_summary(&self) -> Option<&str> {
        match self {
            Self::Solve {
                conflict: Some(conflict),
                ..
            } => Some(conflict.summary.as_str()),
            _ => None,
        }
    }

    /// Return the most useful verbose explanation for this resolver failure.
    pub fn verbose_detail(&self) -> String {
        match self {
            Self::Solve {
                conflict: Some(conflict),
                ..
            } => conflict.report.clone(),
            Self::Solve { detail, .. } => detail.clone(),
            _ => self.to_string(),
        }
    }
}
