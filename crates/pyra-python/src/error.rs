//! Typed errors for Python management workflows.
//!
//! Errors stay domain-specific here so the CLI can render them consistently
//! without losing the actionable context that users need.

use std::ffi::OsString;
use std::io;
use std::path::PathBuf;

use pyra_errors::{ErrorKind, ErrorReport, UserFacingError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PythonError {
    #[error("invalid Python version request: {input}")]
    InvalidVersionRequest { input: String },
    #[error("invalid concrete Python version: {input}")]
    InvalidConcreteVersion { input: String },
    #[error("Pyra does not support Python installs on {host}")]
    UnsupportedHost { host: String },
    #[error("failed to request upstream Python release metadata from {url}")]
    CatalogRequest {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("failed to read Python release catalog from {path}")]
    ReadCatalogFile {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse upstream Python release metadata")]
    CatalogParse {
        #[source]
        source: serde_json::Error,
    },
    #[error("no installable Python release matched `{request}` for {host}")]
    NoMatchingRelease { request: String, host: String },
    #[error("failed to download Python archive from {url}")]
    DownloadArchive {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("failed to read Python archive from {path}")]
    ReadLocalArchive {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to read cached archive at {path}")]
    ReadCachedArchive {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to write archive at {path}")]
    WriteArchive {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("downloaded archive checksum did not match for {asset}")]
    ChecksumMismatch {
        asset: String,
        expected: String,
        actual: String,
    },
    #[error("failed to create install staging directory at {path}")]
    CreateStagingDirectory {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to extract Python archive {archive}")]
    ExtractArchive {
        archive: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to validate extracted Python archive at {path}")]
    InvalidExtractedArchive { path: String },
    #[error("failed to activate Python installation at {path}")]
    ActivateInstall {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to remove managed Python installation at {path}")]
    RemoveInstall {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to read install directory at {path}")]
    ReadInstallDirectory {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to inspect entry in {path}")]
    InspectInstallEntry {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to read installation manifest at {path}")]
    ReadManifest {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse installation manifest at {path}")]
    ParseManifest {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to serialize installation manifest for {path}")]
    SerializeManifest {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to write installation manifest at {path}")]
    WriteManifest {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("managed install entry name is not valid UTF-8: {name:?}")]
    NonUtf8EntryName { name: OsString },
    #[error("managed install entry path is not valid UTF-8: {path:?}")]
    NonUtf8EntryPath { path: PathBuf },
    #[error("managed Python install entry is invalid: {entry}")]
    InvalidInstallEntry { entry: String },
    #[error("managed Python installation for `{request}` was not found")]
    InstalledVersionNotFound { request: String },
    #[error("managed Python selector `{request}` matched multiple installations")]
    AmbiguousInstalledVersion {
        request: String,
        matches: Vec<String>,
    },
}

impl UserFacingError for PythonError {
    fn report(&self) -> ErrorReport {
        match self {
            Self::InvalidVersionRequest { input } => ErrorReport::new(
                ErrorKind::User,
                format!("`{input}` is not a valid Python version request."),
            )
            .with_detail("Pyra accepts numeric version requests like `3`, `3.13`, or `3.13.2`.")
            .with_suggestion("Retry with a numeric CPython version request."),
            Self::InvalidConcreteVersion { input } => ErrorReport::new(
                ErrorKind::Internal,
                format!("Pyra parsed an invalid concrete Python version: `{input}`."),
            )
            .with_detail("The upstream release metadata did not match Pyra's expected stable version format.")
            .with_suggestion("Retry later or report this upstream metadata issue."),
            Self::UnsupportedHost { host } => ErrorReport::new(
                ErrorKind::User,
                format!("Pyra does not support managed Python installs on {host} yet."),
            )
            .with_detail("The MVP only supports a small set of host targets for python-build-standalone installs.")
            .with_suggestion("Retry on a supported macOS or Linux host."),
            Self::CatalogRequest { url, source } => ErrorReport::new(
                ErrorKind::System,
                "Pyra could not fetch the upstream Python release catalog.",
            )
            .with_detail("The upstream machine-readable release metadata request failed.")
            .with_suggestion("Check your network connection and retry.")
            .with_verbose_detail(format!("{url}: {source}")),
            Self::ReadCatalogFile { path, source } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not read `{path}`."),
            )
            .with_detail("Pyra was configured to read Python release metadata from a local file, but that file could not be read.")
            .with_suggestion("Check the local catalog path and retry.")
            .with_verbose_detail(source.to_string()),
            Self::CatalogParse { source } => ErrorReport::new(
                ErrorKind::Internal,
                "Pyra could not parse the upstream Python release catalog.",
            )
            .with_detail("The upstream metadata response did not match the format Pyra expects.")
            .with_suggestion("Retry later or report this issue.")
            .with_verbose_detail(source.to_string()),
            Self::NoMatchingRelease { request, host } => ErrorReport::new(
                ErrorKind::User,
                format!("No installable Python release matched `{request}` for {host}."),
            )
            .with_detail("Pyra could not find a compatible stable CPython build for that request in the live upstream catalog.")
            .with_suggestion("Run `pyra python search` to see which versions are currently available."),
            Self::DownloadArchive { url, source } => ErrorReport::new(
                ErrorKind::System,
                "Pyra could not download the requested Python archive.",
            )
            .with_detail("The upstream Python distribution download failed.")
            .with_suggestion("Check your network connection and retry.")
            .with_verbose_detail(format!("{url}: {source}")),
            Self::ReadLocalArchive { path, source } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not read `{path}`."),
            )
            .with_detail("Pyra was configured to install from a local archive path, but that archive could not be read.")
            .with_suggestion("Check the local archive path and retry.")
            .with_verbose_detail(source.to_string()),
            Self::ReadCachedArchive { path, source } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not read `{path}`."),
            )
            .with_detail("A cached Python archive could not be read from disk.")
            .with_suggestion("Remove the cached archive and retry the install.")
            .with_verbose_detail(source.to_string()),
            Self::WriteArchive { path, source } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not write `{path}`."),
            )
            .with_detail("Pyra could not persist the downloaded Python archive.")
            .with_suggestion("Check filesystem permissions and available disk space, then retry.")
            .with_verbose_detail(source.to_string()),
            Self::ChecksumMismatch {
                asset,
                expected,
                actual,
            } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra rejected the downloaded archive `{asset}`."),
            )
            .with_detail("The archive checksum did not match the upstream digest, so Pyra refused to install it.")
            .with_suggestion("Retry the install. If the problem persists, report the upstream release.")
            .with_verbose_detail(format!("expected {expected}, got {actual}")),
            Self::CreateStagingDirectory { path, source } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not prepare `{path}`."),
            )
            .with_detail("Pyra could not create a temporary staging directory for the Python install.")
            .with_suggestion("Check filesystem permissions and retry.")
            .with_verbose_detail(source.to_string()),
            Self::ExtractArchive { archive, source } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not extract `{archive}`."),
            )
            .with_detail("The downloaded Python archive could not be unpacked.")
            .with_suggestion("Retry the install or remove the cached archive first.")
            .with_verbose_detail(source.to_string()),
            Self::InvalidExtractedArchive { path } => ErrorReport::new(
                ErrorKind::Internal,
                "Pyra extracted the archive but the layout was not valid.",
            )
            .with_detail("The install-only archive did not contain the expected Python executable layout.")
            .with_suggestion("Retry later or report this upstream package layout issue.")
            .with_verbose_detail(path.clone()),
            Self::ActivateInstall { path, source } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not activate `{path}`."),
            )
            .with_detail("Pyra prepared the Python files but could not move them into the managed install directory.")
            .with_suggestion("Check filesystem permissions and retry.")
            .with_verbose_detail(source.to_string()),
            Self::RemoveInstall { path, source } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not remove `{path}`."),
            )
            .with_detail("Pyra could not delete the managed Python installation.")
            .with_suggestion("Check filesystem permissions and retry.")
            .with_verbose_detail(source.to_string()),
            Self::ReadInstallDirectory { path, source } => ErrorReport::new(
                ErrorKind::System,
                "Pyra could not read its managed Python install directory.",
            )
            .with_detail("Pyra was unable to inspect the directory that stores managed Python versions.")
            .with_suggestion("Check filesystem permissions for Pyra's data directory and retry.")
            .with_verbose_detail(format!("{path}: {source}")),
            Self::InspectInstallEntry { path, source } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not inspect `{path}`."),
            )
            .with_detail("A filesystem entry inside the managed Python storage area could not be inspected.")
            .with_suggestion("Check the storage directory permissions or remove the broken entry.")
            .with_verbose_detail(source.to_string()),
            Self::ReadManifest { path, source } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not read `{path}`."),
            )
            .with_detail("A managed Python installation manifest could not be read from disk.")
            .with_suggestion("Repair or remove the broken installation and retry.")
            .with_verbose_detail(source.to_string()),
            Self::ParseManifest { path, source } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not parse `{path}`."),
            )
            .with_detail("A managed Python installation manifest exists but is invalid.")
            .with_suggestion("Repair or remove the broken installation and retry.")
            .with_verbose_detail(source.to_string()),
            Self::SerializeManifest { path, source } => ErrorReport::new(
                ErrorKind::Internal,
                format!("Pyra could not serialize the installation manifest for `{path}`."),
            )
            .with_detail("Pyra assembled managed Python installation metadata but could not encode it.")
            .with_suggestion("Retry later or report this issue.")
            .with_verbose_detail(source.to_string()),
            Self::WriteManifest { path, source } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not write `{path}`."),
            )
            .with_detail("Pyra could not persist the managed Python installation metadata.")
            .with_suggestion("Check filesystem permissions and retry.")
            .with_verbose_detail(source.to_string()),
            Self::NonUtf8EntryName { name } => ErrorReport::new(
                ErrorKind::System,
                "Pyra found a managed Python entry with a non-UTF-8 name.",
            )
            .with_detail("Managed Python version directories must be UTF-8-safe so Pyra can reason about them consistently.")
            .with_suggestion("Rename or remove the problematic directory, then retry.")
            .with_verbose_detail(format!("{name:?}")),
            Self::NonUtf8EntryPath { path } => ErrorReport::new(
                ErrorKind::System,
                "Pyra found a managed Python entry with a non-UTF-8 path.",
            )
            .with_detail("Pyra currently expects UTF-8-safe managed storage paths.")
            .with_suggestion("Move or remove the problematic directory, then retry.")
            .with_verbose_detail(path.display().to_string()),
            Self::InvalidInstallEntry { entry } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra found an invalid managed Python entry: `{entry}`."),
            )
            .with_detail("Managed Python entries should contain a valid installation manifest and expected directory layout.")
            .with_suggestion("Remove the invalid entry from the managed Python directory and retry.")
            .with_verbose_detail(entry.clone()),
            Self::InstalledVersionNotFound { request } => ErrorReport::new(
                ErrorKind::User,
                format!("Pyra could not find an installed Python matching `{request}`."),
            )
            .with_detail("No managed Python installation matched that selector.")
            .with_suggestion("Run `pyra python list` to see which versions are currently installed."),
            Self::AmbiguousInstalledVersion { request, matches } => ErrorReport::new(
                ErrorKind::User,
                format!("`{request}` matched multiple installed Python versions."),
            )
            .with_detail("Pyra needs a more specific version selector before it can safely uninstall a Python version.")
            .with_suggestion("Retry with one of the installed concrete versions shown by `pyra python list`.")
            .with_verbose_detail(matches.join(", ")),
        }
    }
}
