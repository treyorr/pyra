use std::io;
use std::path::PathBuf;

use pyra_errors::{ErrorKind, ErrorReport, UserFacingError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("unable to resolve Pyra application directories")]
    AppDirectoriesUnavailable,
    #[error("unable to resolve the current working directory")]
    CurrentDirectoryUnavailable {
        #[source]
        source: io::Error,
    },
    #[error("path for {label} is not valid UTF-8: {path:?}")]
    NonUtf8Path { label: &'static str, path: PathBuf },
    #[error("environment variable {name} is not valid UTF-8")]
    NonUtf8EnvironmentOverride { name: &'static str },
    #[error("environment variable {name} must not be empty")]
    EmptyEnvironmentOverride { name: &'static str },
    #[error("failed to create directory at {path}")]
    CreateDirectory {
        path: String,
        #[source]
        source: io::Error,
    },
}

impl UserFacingError for CoreError {
    fn report(&self) -> ErrorReport {
        match self {
            Self::AppDirectoriesUnavailable => ErrorReport::new(
                ErrorKind::Internal,
                "Pyra could not determine its application directories.",
            )
            .with_detail("The current platform did not provide standard config, data, cache, or state locations.")
            .with_suggestion("Retry on a supported platform or report this issue."),
            Self::CurrentDirectoryUnavailable { source } => ErrorReport::new(
                ErrorKind::System,
                "Pyra could not read the current working directory.",
            )
            .with_detail("The operating system did not allow Pyra to inspect the current directory.")
            .with_suggestion("Check your shell session and directory permissions, then try again.")
            .with_verbose_detail(source.to_string()),
            Self::NonUtf8Path { label, path } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra found a non-UTF-8 {label} path."),
            )
            .with_detail("Pyra currently expects UTF-8-safe application paths for consistent cross-platform behavior.")
            .with_suggestion("Move the workspace or home directory to a UTF-8-safe path and retry.")
            .with_verbose_detail(path.display().to_string()),
            Self::NonUtf8EnvironmentOverride { name } => ErrorReport::new(
                ErrorKind::User,
                format!("`{name}` is not a valid UTF-8 path override."),
            )
            .with_detail("Pyra path override environment variables must contain UTF-8-safe directory paths.")
            .with_suggestion("Update the environment variable to a valid UTF-8 path or unset it."),
            Self::EmptyEnvironmentOverride { name } => ErrorReport::new(
                ErrorKind::User,
                format!("`{name}` is set but empty."),
            )
            .with_detail("Pyra path override environment variables must point to real directories.")
            .with_suggestion("Set the variable to a directory path or unset it."),
            Self::CreateDirectory { path, source } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not prepare `{path}`."),
            )
            .with_detail("The operating system rejected directory creation for a required Pyra storage path.")
            .with_suggestion("Check filesystem permissions and available disk space, then retry.")
            .with_verbose_detail(source.to_string()),
        }
    }
}
