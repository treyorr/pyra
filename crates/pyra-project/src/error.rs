use std::io;
use std::path::PathBuf;

use pyra_errors::{ErrorKind, ErrorReport, UserFacingError};
use pyra_python::PythonError;
use pyra_resolver::ResolverError;
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
    #[error("the current project does not pin a Python version in [tool.pyra]")]
    PinnedPythonNotConfigured,
    #[error("the current project is missing [project] metadata")]
    MissingProjectMetadata,
    #[error("the current project is missing [project].name")]
    MissingProjectName,
    #[error("the current project has an invalid [project].requires-python constraint")]
    InvalidRequiresPython {
        path: String,
        value: String,
        detail: String,
    },
    #[error("the selected managed interpreter is outside the project's requires-python constraint")]
    PinnedPythonIncompatibleWithProject {
        interpreter: String,
        requires_python: String,
    },
    #[error("the selected managed interpreter version is not PEP 440-compatible")]
    InvalidManagedPythonVersion { value: String, detail: String },
    #[error("dependency group `{name}` is not valid")]
    InvalidDependencyGroupDefinition { name: String },
    #[error("dependency group `{group}` contains an invalid entry")]
    InvalidDependencyGroupEntry { group: String },
    #[error("dependency groups `{first}` and `{second}` normalize to the same name")]
    DuplicateNormalizedDependencyGroup { first: String, second: String },
    #[error("dependency group includes unknown group `{name}`")]
    UnknownIncludedDependencyGroup { name: String },
    #[error("dependency group expansion found a cycle: {cycle}")]
    DependencyGroupCycle { cycle: String },
    #[error("invalid dependency requirement `{value}` in {context}")]
    InvalidRequirement {
        context: String,
        value: String,
        detail: String,
    },
    #[error("invalid dependency value in {context}")]
    InvalidRequirementValue { context: String },
    #[error("unknown dependency group `{name}`")]
    UnknownDependencyGroup { name: String },
    #[error("unknown optional dependency `{name}`")]
    UnknownOptionalDependency { name: String },
    #[error("the pinned Python selector does not match an installed managed interpreter")]
    PinnedPythonNotInstalled {
        selector: String,
        #[source]
        source: PythonError,
    },
    #[error("failed to read pylock.toml at {path}")]
    ReadLockfile {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to write pylock.toml at {path}")]
    WriteLockfile {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse pylock.toml at {path}")]
    ParseLockfile { path: String, detail: String },
    #[error("dependency resolution failed")]
    ResolveDependencies {
        #[source]
        source: ResolverError,
    },
    #[error("failed to query installed packages from {interpreter}")]
    InspectEnvironment { interpreter: String, detail: String },
    #[error("failed to create locked artifact directory at {path}")]
    PrepareArtifactDirectory {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to download locked artifact from {url}")]
    DownloadLockedArtifact {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("failed to read locked artifact from {path}")]
    ReadLockedArtifact {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to write locked artifact at {path}")]
    WriteLockedArtifact {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to activate verified artifact from {from} to {to}")]
    PromoteLockedArtifact {
        from: String,
        to: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to remove artifact file at {path}")]
    RemoveArtifactFile {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("downloaded artifact hash did not match for `{package}`")]
    LockedArtifactHashMismatch {
        package: String,
        artifact: String,
        expected: String,
        actual: String,
    },
    #[error("failed to install package `{package}` into the environment")]
    InstallLockedPackage {
        package: String,
        interpreter: String,
        stderr: String,
    },
    #[error("failed to remove package `{package}` from the environment")]
    RemoveLockedPackage {
        package: String,
        interpreter: String,
        stderr: String,
    },
    #[error("failed to install the current project into the environment")]
    InstallEditableProject { interpreter: String, stderr: String },
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
            Self::PinnedPythonNotConfigured => ErrorReport::new(
                ErrorKind::User,
                "Pyra could not sync this project because no Python is pinned yet.",
            )
            .with_detail("`pyra sync` needs `[tool.pyra].python` so it can resolve and install for one managed interpreter.")
            .with_suggestion("Run `pyra use <version>` first, then retry `pyra sync`."),
            Self::MissingProjectMetadata => ErrorReport::new(
                ErrorKind::User,
                "Pyra could not find `[project]` metadata in `pyproject.toml`.",
            )
            .with_detail("`pyra sync` needs standard project metadata to determine the project name and dependencies.")
            .with_suggestion("Add a `[project]` table to `pyproject.toml` and retry."),
            Self::MissingProjectName => ErrorReport::new(
                ErrorKind::User,
                "Pyra could not find `[project].name` in `pyproject.toml`.",
            )
            .with_detail("The project name is needed for editable installation and lockfile metadata.")
            .with_suggestion("Add `name = \"...\"` under `[project]` and retry."),
            Self::InvalidRequiresPython {
                path,
                value,
                detail,
            } => ErrorReport::new(
                ErrorKind::User,
                format!("Pyra could not parse `[project].requires-python` in `{path}`."),
            )
            .with_detail(format!(
                "The project constraint `{value}` is not a valid PEP 440 version specifier set."
            ))
            .with_suggestion("Fix `[project].requires-python` and retry `pyra sync`.")
            .with_verbose_detail(detail.clone()),
            Self::PinnedPythonIncompatibleWithProject {
                interpreter,
                requires_python,
            } => ErrorReport::new(
                ErrorKind::User,
                format!(
                    "Project `requires-python` `{requires_python}` does not allow Python {interpreter}."
                ),
            )
            .with_detail(format!(
                "Pyra only syncs a project when the selected managed interpreter satisfies the project's declared Python support range. The current project pins Python {interpreter}."
            ))
            .with_suggestion(
                "Repin the project with `pyra use <version>` or update `[project].requires-python`, then retry.",
            ),
            Self::InvalidManagedPythonVersion { value, detail } => ErrorReport::new(
                ErrorKind::Internal,
                "Pyra could not validate the selected managed interpreter version.",
            )
            .with_detail("Pyra stored a concrete managed Python version that no longer matches the version format expected by `requires-python` enforcement.")
            .with_suggestion("Retry later or report this issue.")
            .with_verbose_detail(format!("value `{value}`: {detail}")),
            Self::InvalidDependencyGroupDefinition { name } => ErrorReport::new(
                ErrorKind::User,
                format!("Dependency group `{name}` is not valid."),
            )
            .with_detail("Dependency groups must be TOML arrays containing requirement strings or `{ include-group = \"...\" }` entries.")
            .with_suggestion("Fix the dependency group definition and retry."),
            Self::InvalidDependencyGroupEntry { group } => ErrorReport::new(
                ErrorKind::User,
                format!("Dependency group `{group}` contains an invalid entry."),
            )
            .with_detail("Only requirement strings and `{ include-group = \"...\" }` entries are allowed inside `[dependency-groups]`.")
            .with_suggestion("Fix the group entry and retry."),
            Self::DuplicateNormalizedDependencyGroup { first, second } => ErrorReport::new(
                ErrorKind::User,
                "Two dependency groups normalize to the same name.",
            )
            .with_detail("Dependency group names are compared case-insensitively with punctuation normalized, so Pyra cannot keep both groups distinct.")
            .with_suggestion("Rename one of the groups so their normalized names differ.")
            .with_verbose_detail(format!("{first} vs {second}")),
            Self::UnknownIncludedDependencyGroup { name } => ErrorReport::new(
                ErrorKind::User,
                format!("Dependency group include `{name}` does not exist."),
            )
            .with_detail("Pyra found an `{ include-group = \"...\" }` entry pointing at a group that is not defined.")
            .with_suggestion("Fix the include target or add the missing group and retry."),
            Self::DependencyGroupCycle { cycle } => ErrorReport::new(
                ErrorKind::User,
                "Dependency group includes form a cycle.",
            )
            .with_detail("Dependency groups may include other groups, but the include graph must stay acyclic.")
            .with_suggestion("Break the include cycle and retry.")
            .with_verbose_detail(cycle.clone()),
            Self::InvalidRequirement {
                context,
                value,
                detail,
            } => ErrorReport::new(
                ErrorKind::User,
                format!("Pyra could not parse `{value}`."),
            )
            .with_detail(format!("The requirement in {context} is not a valid PEP 508 dependency specifier."))
            .with_suggestion("Fix the requirement string and retry.")
            .with_verbose_detail(detail.clone()),
            Self::InvalidRequirementValue { context } => ErrorReport::new(
                ErrorKind::User,
                format!("Pyra found a non-string dependency entry in {context}."),
            )
            .with_detail("Project dependencies, optional dependencies, and dependency groups must contain PEP 508 requirement strings.")
            .with_suggestion("Replace the invalid value with a requirement string and retry."),
            Self::UnknownDependencyGroup { name } => ErrorReport::new(
                ErrorKind::User,
                format!("Dependency group `{name}` is not defined for this project."),
            )
            .with_detail("The requested sync selection referenced a dependency group that does not exist.")
            .with_suggestion("Check the group name in `pyproject.toml` and retry."),
            Self::UnknownOptionalDependency { name } => ErrorReport::new(
                ErrorKind::User,
                format!("Extra `{name}` is not defined for this project."),
            )
            .with_detail("The requested sync selection referenced an extra that does not exist under `[project.optional-dependencies]`.")
            .with_suggestion("Check the extra name in `pyproject.toml` and retry."),
            Self::PinnedPythonNotInstalled { selector, source } => ErrorReport::new(
                ErrorKind::User,
                "Pyra could not find the pinned managed Python interpreter.",
            )
            .with_detail("`pyra sync` only installs into a Pyra-managed interpreter selected for this project.")
            .with_suggestion(format!("Install the pinned interpreter with `pyra python install {selector}` or repin the project with `pyra use`."))
            .with_verbose_detail(source.to_string()),
            Self::ReadLockfile { path, source } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not read `{path}`."),
            )
            .with_detail("The project lock file exists, but Pyra could not read it.")
            .with_suggestion("Check filesystem permissions and retry.")
            .with_verbose_detail(source.to_string()),
            Self::WriteLockfile { path, source } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not write `{path}`."),
            )
            .with_detail("Pyra resolved the project successfully but could not persist `pylock.toml`.")
            .with_suggestion("Check filesystem permissions and retry.")
            .with_verbose_detail(source.to_string()),
            Self::ParseLockfile { path, detail } => ErrorReport::new(
                ErrorKind::User,
                format!("Pyra could not parse `{path}`."),
            )
            .with_detail("The lock file is not valid for Pyra's current sync implementation.")
            .with_suggestion("Delete the lock file and rerun `pyra sync` to regenerate it.")
            .with_verbose_detail(detail.clone()),
            Self::ResolveDependencies { source } => ErrorReport::new(
                ErrorKind::User,
                "Pyra could not resolve a compatible dependency set.",
            )
            .with_detail("The project dependency inputs could not be locked for the selected interpreter and current platform.")
            .with_suggestion("Adjust the declared dependency constraints and retry.")
            .with_verbose_detail(source.to_string()),
            Self::InspectEnvironment {
                interpreter,
                detail,
            } => ErrorReport::new(
                ErrorKind::System,
                "Pyra could not inspect the centralized environment state.",
            )
            .with_detail("Pyra asks the environment's Python to report currently installed distributions before applying an exact sync.")
            .with_suggestion("Retry the sync. If the problem persists, recreate the environment with `pyra use <version>` and retry.")
            .with_verbose_detail(format!("interpreter: {interpreter}\ndetail: {detail}")),
            Self::PrepareArtifactDirectory { path, source } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not prepare `{path}`."),
            )
            .with_detail("Pyra could not create the cache directory used for verified locked artifacts.")
            .with_suggestion("Check filesystem permissions and available disk space, then retry.")
            .with_verbose_detail(source.to_string()),
            Self::DownloadLockedArtifact { url, source } => ErrorReport::new(
                ErrorKind::System,
                "Pyra could not download a locked package artifact.",
            )
            .with_detail("Sync selected an artifact from `pylock.toml`, but the download failed before hash verification.")
            .with_suggestion("Check the artifact source and your network connection, then retry.")
            .with_verbose_detail(format!("{url}: {source}")),
            Self::ReadLockedArtifact { path, source } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not read `{path}`."),
            )
            .with_detail("Sync selected a local artifact source or cached artifact, but Pyra could not read it for verification.")
            .with_suggestion("Check that the local artifact still exists and retry.")
            .with_verbose_detail(source.to_string()),
            Self::WriteLockedArtifact { path, source } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not write `{path}`."),
            )
            .with_detail("Pyra downloaded and verified an artifact but could not stage it under the local cache.")
            .with_suggestion("Check filesystem permissions and available disk space, then retry.")
            .with_verbose_detail(source.to_string()),
            Self::PromoteLockedArtifact { from, to, source } => ErrorReport::new(
                ErrorKind::System,
                "Pyra could not activate a verified package artifact.",
            )
            .with_detail("Pyra verified the artifact hash but could not move the staged file into its stable local cache path.")
            .with_suggestion("Retry the sync. If the problem persists, clear the artifact cache and try again.")
            .with_verbose_detail(format!("from: {from}\nto: {to}\n{source}")),
            Self::RemoveArtifactFile { path, source } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not clean up `{path}`."),
            )
            .with_detail("Pyra tried to remove a verified-artifact cache or staging file after a failure.")
            .with_suggestion("Remove the artifact file manually and retry.")
            .with_verbose_detail(source.to_string()),
            Self::LockedArtifactHashMismatch {
                package,
                artifact,
                expected,
                actual,
            } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra rejected the locked artifact for `{package}`."),
            )
            .with_detail("The downloaded artifact hash did not match `pylock.toml`, so sync stopped before install.")
            .with_suggestion("Retry the sync. If it still fails, regenerate `pylock.toml` from a trusted index.")
            .with_verbose_detail(format!(
                "artifact: {artifact}\nexpected sha256: {expected}\nactual sha256: {actual}"
            )),
            Self::InstallLockedPackage {
                package,
                interpreter,
                stderr,
            } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not install `{package}` from the lock."),
            )
            .with_detail("Pyra selected a locked artifact, but the installer backend failed while applying it.")
            .with_suggestion("Retry the sync. If the failure persists, inspect the package artifact or regenerate the lock.")
            .with_verbose_detail(format!("interpreter: {interpreter}\nstderr: {stderr}")),
            Self::RemoveLockedPackage {
                package,
                interpreter,
                stderr,
            } => ErrorReport::new(
                ErrorKind::System,
                format!("Pyra could not remove `{package}` during exact sync."),
            )
            .with_detail("Pyra removes packages not present in the selected lock subset to keep the centralized environment exact.")
            .with_suggestion("Retry the sync. If the problem persists, recreate the environment and retry.")
            .with_verbose_detail(format!("interpreter: {interpreter}\nstderr: {stderr}")),
            Self::InstallEditableProject {
                interpreter,
                stderr,
            } => ErrorReport::new(
                ErrorKind::System,
                "Pyra could not install the current project editable.",
            )
            .with_detail("The project declares a build system, so Pyra attempted an editable install after syncing locked dependencies.")
            .with_suggestion("Check the project's build backend configuration and retry.")
            .with_verbose_detail(format!("interpreter: {interpreter}\nstderr: {stderr}")),
        }
    }
}
