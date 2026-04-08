//! Typed resolver errors.

use thiserror::Error;

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
    Solve { detail: String },
}
