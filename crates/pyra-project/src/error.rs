use std::io;
use std::path::PathBuf;

use pyra_errors::{ErrorKind, ErrorReport, UserFacingError};
use pyra_python::PythonError;
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
    #[error("no Pyra project could be found from {start}")]
    ProjectNotFound { start: String },
    #[error("failed to resolve the canonical project root from {path}")]
    CanonicalizeProjectRoot {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("project root path is not valid UTF-8: {path:?}")]
    NonUtf8ProjectRoot { path: PathBuf },
    #[error("failed to read pyproject.toml at {path}")]
    ReadPyproject {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse pyproject.toml at {path}")]
    ParsePyproject {
        path: String,
        #[source]
        source: toml_edit::TomlError,
    },
    #[error("failed to write pyproject.toml at {path}")]
    WritePyproject {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("invalid pinned Python version `{value}` in {path}")]
    InvalidPinnedPython {
        path: String,
        value: String,
        #[source]
        source: PythonError,
    },
    #[error("no Pyra-managed Python is installed")]
    NoManagedPythonInstalled,
    #[error("failed to create the centralized environment at {path}")]
    CreateEnvironment {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("the interpreter at {interpreter} could not create the environment at {path}")]
    EnvironmentCommandFailed {
        interpreter: String,
        path: String,
        stderr: String,
    },
    #[error("failed to read environment metadata at {path}")]
    ReadEnvironmentMetadata {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse environment metadata at {path}")]
    ParseEnvironmentMetadata {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to serialize environment metadata for {path}")]
    SerializeEnvironmentMetadata {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to write environment metadata at {path}")]
    WriteEnvironmentMetadata {
        path: String,
        #[source]
        source: io::Error,
    },
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
            Self::ProjectNotFound { start } => ErrorReport::new(
                ErrorKind::User,
                "Pyra could not find a project in the current directory.",
            )
            .with_detail("`pyra use` needs a `pyproject.toml` in this directory or one of its parents.")
            .with_suggestion("Run `pyra init` first or change into an existing Pyra project.")
            .with_verbose_detail(start.clone()),
            Self::CanonicalizeProjectRoot { path, source } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not resolve `{path}`."),
            )
            .with_detail("Pyra could not determine the canonical project root path needed for stable environment storage.")
            .with_suggestion("Check that the project directory still exists and retry.")
            .with_verbose_detail(source.to_string()),
            Self::NonUtf8ProjectRoot { path } => ErrorReport::new(
                ErrorKind::System,
                "Pyra found a project path that is not valid UTF-8.",
            )
            .with_detail("Pyra currently expects UTF-8-safe project roots so it can derive stable storage keys consistently.")
            .with_suggestion("Move the project to a UTF-8-safe path and retry.")
            .with_verbose_detail(path.display().to_string()),
            Self::ReadPyproject { path, source } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not read `{path}`."),
            )
            .with_detail("The project configuration file exists, but Pyra could not read it.")
            .with_suggestion("Check filesystem permissions and retry.")
            .with_verbose_detail(source.to_string()),
            Self::ParsePyproject { path, source } => ErrorReport::new(
                ErrorKind::User,
                format!("Pyra could not parse `{path}`."),
            )
            .with_detail("The project configuration file is not valid TOML, so Pyra could not safely update `[tool.pyra]`.")
            .with_suggestion("Fix the TOML syntax and retry.")
            .with_verbose_detail(source.to_string()),
            Self::WritePyproject { path, source } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not write `{path}`."),
            )
            .with_detail("Pyra could not persist the updated project Python selection.")
            .with_suggestion("Check filesystem permissions and retry.")
            .with_verbose_detail(source.to_string()),
            Self::InvalidPinnedPython { path, value, source } => ErrorReport::new(
                ErrorKind::User,
                format!("`{path}` contains an invalid `[tool.pyra].python` value."),
            )
            .with_detail("Pyra only accepts numeric version selectors like `3`, `3.13`, or `3.13.2` in project configuration.")
            .with_suggestion("Update the pinned version to a valid numeric selector and retry.")
            .with_verbose_detail(format!("value `{value}`: {source}")),
            Self::NoManagedPythonInstalled => ErrorReport::new(
                ErrorKind::User,
                "No Pyra-managed Python is installed yet.",
            )
            .with_detail("`pyra init` without `--python` chooses the latest interpreter already managed by Pyra.")
            .with_suggestion("Install one with `pyra python install <version>` or rerun `pyra init --python <version>`."),
            Self::CreateEnvironment { path, source } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not prepare `{path}`."),
            )
            .with_detail("Pyra could not create the centralized project environment directory.")
            .with_suggestion("Check filesystem permissions and available disk space, then retry.")
            .with_verbose_detail(source.to_string()),
            Self::EnvironmentCommandFailed {
                interpreter,
                path,
                stderr,
            } => ErrorReport::new(
                ErrorKind::System,
                "Pyra could not create the centralized project environment.",
            )
            .with_detail("The selected managed interpreter failed while creating the environment.")
            .with_suggestion("Retry the command. If it keeps failing, reinstall that Python version and try again.")
            .with_verbose_detail(format!(
                "interpreter: {interpreter}\nenvironment: {path}\nstderr: {stderr}"
            )),
            Self::ReadEnvironmentMetadata { path, source } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not read `{path}`."),
            )
            .with_detail("The centralized environment metadata exists, but Pyra could not read it.")
            .with_suggestion("Repair or remove the broken environment metadata and retry.")
            .with_verbose_detail(source.to_string()),
            Self::ParseEnvironmentMetadata { path, source } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not parse `{path}`."),
            )
            .with_detail("The centralized environment metadata file is invalid.")
            .with_suggestion("Repair or remove the broken environment metadata and retry.")
            .with_verbose_detail(source.to_string()),
            Self::SerializeEnvironmentMetadata { path, source } => ErrorReport::new(
                ErrorKind::Internal,
                format!("Pyra could not serialize the environment metadata for `{path}`."),
            )
            .with_detail("Pyra assembled project environment metadata but could not encode it.")
            .with_suggestion("Retry later or report this issue.")
            .with_verbose_detail(source.to_string()),
            Self::WriteEnvironmentMetadata { path, source } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not write `{path}`."),
            )
            .with_detail("Pyra could not persist the centralized environment metadata.")
            .with_suggestion("Check filesystem permissions and retry.")
            .with_verbose_detail(source.to_string()),
        }
    }
}
