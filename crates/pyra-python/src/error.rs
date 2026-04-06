use std::ffi::OsString;
use std::io;
use std::path::PathBuf;

use pyra_errors::{ErrorKind, ErrorReport, UserFacingError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PythonError {
    #[error("invalid Python version request: {input}")]
    InvalidVersion { input: String },
    #[error("failed to read install directory at {path}")]
    ReadInstallDirectory {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to create install directory at {path}")]
    CreateInstallDirectory {
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
    #[error("failed to write placeholder metadata at {path}")]
    WriteMetadata {
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
}

impl UserFacingError for PythonError {
    fn report(&self) -> ErrorReport {
        match self {
            Self::InvalidVersion { input } => ErrorReport::new(
                ErrorKind::User,
                format!("`{input}` is not a valid Python version request."),
            )
            .with_detail("Pyra currently accepts numeric version requests like `3`, `3.13`, or `3.13.2`.")
            .with_suggestion("Retry with a numeric CPython version request."),
            Self::ReadInstallDirectory { path, source } => ErrorReport::new(
                ErrorKind::System,
                "Pyra could not read its managed Python install directory.",
            )
            .with_detail("Pyra was unable to inspect the directory that stores managed Python versions.")
            .with_suggestion("Check filesystem permissions for Pyra's data directory and retry.")
            .with_verbose_detail(format!("{path}: {source}")),
            Self::CreateInstallDirectory { path, source } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not prepare `{path}`."),
            )
            .with_detail("Pyra was unable to create the managed install directory for the requested Python version.")
            .with_suggestion("Check filesystem permissions and available disk space, then retry.")
            .with_verbose_detail(source.to_string()),
            Self::InspectInstallEntry { path, source } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not inspect `{path}`."),
            )
            .with_detail("A filesystem entry inside the managed Python storage area could not be inspected.")
            .with_suggestion("Check the storage directory permissions or remove the broken entry.")
            .with_verbose_detail(source.to_string()),
            Self::WriteMetadata { path, source } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not write `{path}`."),
            )
            .with_detail("Pyra creates placeholder metadata for managed Python installs so later installers can reuse the location.")
            .with_suggestion("Check filesystem permissions and retry the install command.")
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
            .with_detail("Managed Python entries should match a normalized version request so Pyra can sort and inspect them.")
            .with_suggestion("Remove the invalid entry from the managed Python directory and retry.")
            .with_verbose_detail(entry.clone()),
        }
    }
}
