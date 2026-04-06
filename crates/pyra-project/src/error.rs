use std::io;

use pyra_errors::{ErrorKind, ErrorReport, UserFacingError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("target file already exists: {path}")]
    ExistingPath { path: String },
    #[error("failed to write file at {path}")]
    WriteFile {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("unable to determine a project name from {path}")]
    InvalidProjectName { path: String },
}

impl UserFacingError for ProjectError {
    fn report(&self) -> ErrorReport {
        match self {
            Self::ExistingPath { path } => ErrorReport::new(
                ErrorKind::User,
                format!("Pyra will not overwrite `{path}`."),
            )
            .with_detail("Project initialization found an existing file that would be replaced.")
            .with_suggestion("Run `pyra init` in an empty directory or remove the conflicting file first."),
            Self::WriteFile { path, source } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not write `{path}`."),
            )
            .with_detail("The operating system rejected one of the files needed for project initialization.")
            .with_suggestion("Check filesystem permissions and retry.")
            .with_verbose_detail(source.to_string()),
            Self::InvalidProjectName { path } => ErrorReport::new(
                ErrorKind::User,
                "Pyra could not derive a valid project name from the current directory.",
            )
            .with_detail("Project names must be based on the current directory name and remain Python-package-friendly.")
            .with_suggestion("Rename the directory to a simple ASCII name and retry.")
            .with_verbose_detail(path.clone()),
        }
    }
}
