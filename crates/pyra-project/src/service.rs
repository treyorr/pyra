//! Project-domain service facade.
//!
//! This service owns project discovery, config updates, and centralized
//! environment preparation while leaving Python release resolution to the
//! dedicated Python subsystem.

use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

use camino::Utf8PathBuf;
use pep508_rs::MarkerEnvironmentBuilder;
use pep508_rs::Requirement;
use pyra_core::AppContext;
use pyra_python::{InstalledPythonRecord, PythonVersion, PythonVersionRequest};
use pyra_resolver::{
    ResolutionRequestTemplate, ResolutionRoot, ResolutionRootToken, ResolutionRootTokenKind,
    Resolver, ResolverEnvironment,
};
use sha2::{Digest, Sha256};

use crate::{
    ProjectError,
    doctor::{DoctorIssue, DoctorIssueCode, DoctorProjectOutcome},
    environment::{ProjectEnvironmentRecord, ProjectEnvironmentStore, ProjectPythonSelection},
    execution::{ProjectExecutionRequest, ProjectExecutionService},
    identity::{ProjectIdentity, find_project_root},
    init::{InitProjectOutcome, create_initial_layout, validate_initial_layout},
    outdated::{OutdatedPackage, OutdatedProjectOutcome},
    pyproject::{
        DependencyDeclarationScope, LockTargetSet, add_dependency_requirement,
        read_python_selector, remove_dependency_requirement, update_python_selector,
        validate_project_requires_python,
    },
    sync::{
        CURRENT_RESOLUTION_STRATEGY, EnvironmentInstaller, LockArtifact, LockDependencyRef,
        LockEnvironment, LockFile, LockFreshness, LockMarker, LockMarkerClause, LockPackage,
        LockSelection, MULTI_TARGET_RESOLUTION_STRATEGY, ProjectSyncInput, ProjectSyncInputLoader,
        ReconciliationPlan, SyncSelectionRequest, SyncSelectionResolver,
    },
};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UseProjectPythonRequest {
    pub python: ProjectPythonSelection,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UseProjectPythonOutcome {
    pub project_root: Utf8PathBuf,
    pub project_id: String,
    pub pyproject_path: Utf8PathBuf,
    pub environment: ProjectEnvironmentRecord,
}

/// Request to add one declared dependency before reusing the normal sync flow.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AddProjectRequest {
    pub requirement: String,
    pub scope: DependencyDeclarationScope,
}

/// Outcome for `pyra add`, including whether the manifest changed before sync.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AddProjectOutcome {
    pub requirement: String,
    pub scope: DependencyDeclarationScope,
    pub manifest_updated: bool,
    pub sync: SyncProjectOutcome,
}

/// Request to remove one declared dependency by package name before reusing the
/// normal sync flow.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RemoveProjectRequest {
    pub package: String,
    pub scope: DependencyDeclarationScope,
}

/// Outcome for `pyra remove` after the manifest update and follow-on sync.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RemoveProjectOutcome {
    pub package: String,
    pub scope: DependencyDeclarationScope,
    pub sync: SyncProjectOutcome,
}

/// Request to execute one command target through the synchronized project
/// environment.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RunProjectRequest {
    pub target: String,
    pub args: Vec<String>,
}

/// Outcome for `pyra run`, including the child process exit code.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RunProjectOutcome {
    pub exit_code: i32,
}

/// Request to generate or refresh `pylock.toml` without reconciling the
/// centralized environment.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct LockProjectRequest {
    pub lock_targets: Vec<String>,
}

/// Freshness-aware status for one explicit `pyra lock` execution.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum LockProjectStatus {
    ReusedFresh,
    GeneratedMissing,
    RegeneratedStale,
}

/// Outcome for `pyra lock`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LockProjectOutcome {
    pub project_root: Utf8PathBuf,
    pub pyproject_path: Utf8PathBuf,
    pub pylock_path: Utf8PathBuf,
    pub project_id: String,
    pub python_version: String,
    pub lock_targets: Vec<String>,
    pub status: LockProjectStatus,
}

/// Request to run read-only project diagnostics.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct DoctorProjectRequest;

/// Request to compute read-only dependency upgrade opportunities from the
/// current dependency intent and lock state.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct OutdatedProjectRequest;

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct SyncProjectRequest {
    pub selection: SyncSelectionRequest,
    pub lock_mode: SyncLockMode,
    pub lock_targets: Vec<String>,
}

/// `pyra sync` supports one normal mode plus two CI-oriented lock-discipline
/// modes. Keeping the policy typed here prevents clap booleans from leaking
/// into project-domain logic.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub enum SyncLockMode {
    #[default]
    WriteIfNeeded,
    Locked,
    Frozen,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SyncProjectOutcome {
    pub project_root: Utf8PathBuf,
    pub pyproject_path: Utf8PathBuf,
    pub pylock_path: Utf8PathBuf,
    pub project_id: String,
    pub python_version: String,
    pub lock_refreshed: bool,
    pub selected_groups: Vec<String>,
    pub selected_extras: Vec<String>,
    pub installed_packages: usize,
    pub removed_packages: usize,
    pub project_installed: bool,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ProjectService;

impl ProjectService {
    pub fn init(
        self,
        context: &AppContext,
        request: InitProjectRequest,
    ) -> Result<InitProjectWithPythonOutcome, ProjectError> {
        validate_initial_layout(context)?;
        let InitProjectRequest {
            python_selector,
            installation,
        } = request;
        let identity = ProjectIdentity::from_root(&context.cwd)?;
        let python = ProjectPythonSelection {
            selector: python_selector.clone(),
            installation,
        };
        let environment = ProjectEnvironmentStore.create_or_refresh(context, &identity, &python)?;
        let init = create_initial_layout(
            context,
            &crate::init::InitProjectRequest {
                python_selector: python_selector.clone(),
            },
        )?;

        Ok(InitProjectWithPythonOutcome {
            init,
            project_id: identity.id,
            environment,
        })
    }

    pub fn use_python(
        self,
        context: &AppContext,
        request: UseProjectPythonRequest,
    ) -> Result<UseProjectPythonOutcome, ProjectError> {
        let project_root = find_project_root(&context.cwd)?;
        let identity = ProjectIdentity::from_root(&project_root)?;
        let pyproject_path = project_root.join("pyproject.toml");
        let _ = read_python_selector(&pyproject_path)?;
        validate_project_requires_python(&pyproject_path, &request.python.installation.version)?;
        update_python_selector(&pyproject_path, &request.python.selector)?;
        let project_context = AppContext::new(
            project_root.clone(),
            context.paths.clone(),
            context.verbosity,
        );
        let environment =
            ProjectEnvironmentStore.ensure(&project_context, &identity, &request.python)?;

        Ok(UseProjectPythonOutcome {
            project_root,
            project_id: identity.id,
            pyproject_path,
            environment,
        })
    }

    pub async fn add(
        self,
        context: &AppContext,
        request: AddProjectRequest,
    ) -> Result<AddProjectOutcome, ProjectError> {
        let project_root = find_project_root(&context.cwd)?;
        let pyproject_path = project_root.join("pyproject.toml");
        let requirement = Requirement::from_str(&request.requirement).map_err(|source| {
            ProjectError::InvalidRequirement {
                context: "`pyra add` input".to_string(),
                value: request.requirement.clone(),
                detail: source.to_string(),
            }
        })?;
        let mutation = add_dependency_requirement(&pyproject_path, &request.scope, &requirement)?;
        let sync = self.sync(context, SyncProjectRequest::default()).await?;

        Ok(AddProjectOutcome {
            requirement: requirement.to_string(),
            scope: request.scope,
            manifest_updated: mutation.changed,
            sync,
        })
    }

    pub async fn remove(
        self,
        context: &AppContext,
        request: RemoveProjectRequest,
    ) -> Result<RemoveProjectOutcome, ProjectError> {
        let project_root = find_project_root(&context.cwd)?;
        let pyproject_path = project_root.join("pyproject.toml");
        remove_dependency_requirement(&pyproject_path, &request.scope, &request.package)?;
        let sync = self.sync(context, SyncProjectRequest::default()).await?;

        Ok(RemoveProjectOutcome {
            package: request.package,
            scope: request.scope,
            sync,
        })
    }

    pub async fn run(
        self,
        context: &AppContext,
        request: RunProjectRequest,
    ) -> Result<RunProjectOutcome, ProjectError> {
        let outcome = ProjectExecutionService
            .execute(
                context,
                ProjectExecutionRequest {
                    target: request.target,
                    args: request.args,
                },
            )
            .await?;

        Ok(RunProjectOutcome {
            exit_code: outcome.exit_code,
        })
    }

    pub async fn lock(
        self,
        context: &AppContext,
        request: LockProjectRequest,
    ) -> Result<LockProjectOutcome, ProjectError> {
        let prepared = prepare_lock_context(context, &request.lock_targets)?;
        let (_, status) = resolve_or_refresh_lock(
            &prepared.input,
            &prepared.lock_targets,
            &prepared.resolver_environments,
            &prepared.freshness,
        )
        .await?;

        Ok(LockProjectOutcome {
            project_root: prepared.input.project_root.clone(),
            pyproject_path: prepared.input.pyproject_path.clone(),
            pylock_path: prepared.input.pylock_path.clone(),
            project_id: prepared.identity.id,
            python_version: prepared.installation.version.to_string(),
            lock_targets: prepared.lock_targets.as_slice().to_vec(),
            status,
        })
    }

    pub fn doctor(
        self,
        context: &AppContext,
        _request: DoctorProjectRequest,
    ) -> Result<DoctorProjectOutcome, ProjectError> {
        let input = ProjectSyncInputLoader.load(context)?;
        let identity = input.project_identity()?;
        let mut issues = Vec::new();

        let installation = match selected_installation(context, &input.pinned_python) {
            Ok(installation) => {
                if let Err(error) = input.validate_selected_interpreter(&installation.version) {
                    match error {
                        ProjectError::PinnedPythonIncompatibleWithProject {
                            interpreter,
                            requires_python,
                        } => issues.push(doctor_issue(
                            DoctorIssueCode::InterpreterMismatch,
                            "Pinned interpreter does not satisfy `[project].requires-python`.",
                            format!(
                                "The pinned interpreter is `{interpreter}`, but the project declares `{requires_python}`."
                            ),
                            "Update the pin with `pyra use <version>` or adjust `requires-python` in `pyproject.toml`.",
                        )),
                        other => return Err(other),
                    }
                }
                Some(installation)
            }
            Err(ProjectError::PinnedPythonNotInstalled { selector, .. }) => {
                issues.push(doctor_issue(
                    DoctorIssueCode::InterpreterMismatch,
                    "Pinned interpreter is not installed.",
                    format!(
                        "The project pins `{selector}`, but no matching Pyra-managed Python is installed."
                    ),
                    format!("Install it with `pyra python install {selector}`."),
                ));
                None
            }
            Err(error) => return Err(error),
        };

        let lock = if !input.pylock_path.exists() {
            issues.push(doctor_issue(
                DoctorIssueCode::MissingLock,
                "`pylock.toml` is missing.",
                "The project has no resolved lock state yet.".to_string(),
                "Run `pyra lock` (or `pyra sync`) to generate `pylock.toml`.".to_string(),
            ));
            None
        } else {
            match LockFile::read(&input.pylock_path) {
                Ok(lock) => {
                    if let Some(installation) = &installation {
                        let resolver_environment = resolver_environment(installation)?;
                        let lock_targets =
                            effective_lock_targets(&input, &installation.target_triple, &[])?;
                        let index_url = std::env::var("PYRA_INDEX_URL")
                            .unwrap_or_else(|_| "https://pypi.org/simple".to_string());
                        let freshness = lock_freshness(
                            &input,
                            &resolver_environment,
                            &index_url,
                            resolution_strategy(&lock_targets),
                        );
                        if !lock.is_fresh_for(&freshness, &lock_targets) {
                            issues.push(doctor_issue(
                                DoctorIssueCode::StaleLock,
                                "`pylock.toml` is stale for current project inputs.",
                                "The current lock does not match the active interpreter and/or dependency inputs."
                                    .to_string(),
                                "Run `pyra lock` to refresh the lock file.".to_string(),
                            ));
                        }
                    }
                    Some(lock)
                }
                Err(ProjectError::ParseLockfile { .. }) => {
                    issues.push(doctor_issue(
                        DoctorIssueCode::StaleLock,
                        "`pylock.toml` could not be parsed.",
                        "The lock file exists but is invalid for current lock semantics."
                            .to_string(),
                        "Run `pyra lock` to regenerate `pylock.toml`.".to_string(),
                    ));
                    None
                }
                Err(error) => return Err(error),
            }
        };

        let metadata_path = context.paths.project_environment_metadata(&identity.id);
        let environment = if !metadata_path.exists() {
            issues.push(doctor_issue(
                DoctorIssueCode::EnvironmentDrift,
                "Centralized environment metadata is missing.",
                format!(
                    "No environment record exists at `{}` for this project.",
                    metadata_path
                ),
                "Run `pyra sync` to (re)build the centralized environment.".to_string(),
            ));
            None
        } else {
            match ProjectEnvironmentStore.read(&metadata_path) {
                Ok(record) => {
                    if let Some(installation) = &installation {
                        if record.python_version != installation.version {
                            issues.push(doctor_issue(
                                DoctorIssueCode::InterpreterMismatch,
                                "Environment interpreter does not match the pinned interpreter.",
                                format!(
                                    "Environment metadata records `{}`, but the pinned interpreter resolves to `{}`.",
                                    record.python_version,
                                    installation.version
                                ),
                                "Run `pyra sync` after updating the project pin with `pyra use`."
                                    .to_string(),
                            ));
                        }
                    }
                    if !record.environment_path.exists() || !record.interpreter_path.exists() {
                        issues.push(doctor_issue(
                            DoctorIssueCode::EnvironmentDrift,
                            "Centralized environment path is missing.",
                            format!(
                                "Expected environment at `{}` with interpreter `{}`.",
                                record.environment_path, record.interpreter_path
                            ),
                            "Run `pyra sync` to rebuild the centralized environment.".to_string(),
                        ));
                    }
                    Some(record)
                }
                Err(ProjectError::ReadEnvironmentMetadata { .. })
                | Err(ProjectError::ParseEnvironmentMetadata { .. }) => {
                    issues.push(doctor_issue(
                        DoctorIssueCode::EnvironmentDrift,
                        "Centralized environment metadata is unreadable.",
                        format!("Could not read `{metadata_path}`."),
                        "Run `pyra sync` to refresh environment metadata.".to_string(),
                    ));
                    None
                }
                Err(error) => return Err(error),
            }
        };

        if let (Some(lock), Some(environment), Some(installation)) =
            (lock.as_ref(), environment.as_ref(), installation.as_ref())
        {
            if environment.interpreter_path.exists() {
                let selection =
                    SyncSelectionResolver.resolve(&input, &SyncSelectionRequest::default())?;
                let mut selected_groups = selection.groups.clone();
                if selection.include_base {
                    selected_groups.insert("pyra-default".to_string());
                }
                let lock_selection = LockSelection {
                    groups: selected_groups,
                    extras: selection.extras.clone(),
                    python_full_version: installation.version.to_string(),
                    target_triple: installation.target_triple.clone(),
                };
                let selected_packages =
                    ReconciliationPlan::for_selection(&lock.packages, &lock_selection);
                let installed_packages =
                    match EnvironmentInstaller.inspect_installed(&environment.interpreter_path) {
                        Ok(packages) => Some(packages),
                        Err(ProjectError::InspectEnvironment { .. }) => {
                            issues.push(doctor_issue(
                                DoctorIssueCode::EnvironmentDrift,
                                "Installed package state could not be inspected.",
                                format!(
                                    "Pyra could not inspect packages through `{}`.",
                                    environment.interpreter_path
                                ),
                                "Repair the environment with `pyra sync`.".to_string(),
                            ));
                            None
                        }
                        Err(error) => return Err(error),
                    };
                if let Some(installed_packages) = installed_packages {
                    let protected_packages = ["pip", "setuptools", "wheel"]
                        .into_iter()
                        .map(ToString::to_string)
                        .collect::<BTreeSet<_>>();
                    let plan = ReconciliationPlan::build(
                        &selected_packages,
                        &installed_packages,
                        &protected_packages,
                        &input.project_name,
                        input.build_system_present,
                    );
                    if !plan.actions.is_empty() {
                        let action_count = plan.actions.len();
                        issues.push(doctor_issue(
                            DoctorIssueCode::EnvironmentDrift,
                            "Centralized environment has drifted from the selected lock state.",
                            format!(
                                "Reconciling this environment would apply {action_count} install/remove action(s)."
                            ),
                            "Run `pyra sync` to reconcile the environment.".to_string(),
                        ));
                    }
                }
            }
        }

        Ok(DoctorProjectOutcome {
            project_root: input.project_root,
            pyproject_path: input.pyproject_path,
            pylock_path: input.pylock_path,
            project_id: identity.id,
            python_selector: input.pinned_python.to_string(),
            python_version: installation.map(|record| record.version.to_string()),
            issues,
        })
    }

    pub async fn outdated(
        self,
        context: &AppContext,
        _request: OutdatedProjectRequest,
    ) -> Result<OutdatedProjectOutcome, ProjectError> {
        let prepared = prepare_lock_context(context, &[])?;
        if !prepared.input.pylock_path.exists() {
            return Err(ProjectError::MissingLockfileForOutdated {
                path: prepared.input.pylock_path.to_string(),
            });
        }

        let lock = LockFile::read(&prepared.input.pylock_path)?;
        if !lock.is_fresh_for(&prepared.freshness, &prepared.lock_targets) {
            return Err(ProjectError::StaleLockfileForOutdated {
                path: prepared.input.pylock_path.to_string(),
            });
        }

        // `pyra outdated` queries available versions against the same index and
        // environment model as sync/lock, but never writes lock or manifest
        // state.
        let resolver_request = ResolutionRequestTemplate::new(
            build_resolution_roots(&prepared.input),
            prepared.freshness.index_url.clone(),
        )
        .for_environment(resolver_environment(&prepared.installation)?);
        let latest_packages = Resolver::new()
            .resolve(resolver_request)
            .await
            .map_err(|source| ProjectError::ResolveDependencies { source })?;
        let latest_versions = package_versions_by_name(
            latest_packages
                .iter()
                .map(|package| (package.name.as_str(), package.version.as_str())),
        );
        let locked_versions = package_versions_by_name(
            lock.packages
                .iter()
                .map(|package| (package.name.as_str(), package.version.as_str())),
        );

        let intents = collect_declared_package_intents(&prepared.input);
        let mut outdated_packages = Vec::new();
        for intent in &intents {
            let Some(current_version) = locked_versions.get(&intent.normalized_name) else {
                continue;
            };
            let Some(latest_version) = latest_versions.get(&intent.normalized_name) else {
                continue;
            };
            if !is_version_newer(latest_version, current_version) {
                continue;
            }

            outdated_packages.push(OutdatedPackage {
                package: intent.package.clone(),
                current_version: current_version.clone(),
                latest_version: latest_version.clone(),
                requirements: intent.requirements.clone(),
                declaration_scopes: intent.declaration_scopes.clone(),
            });
        }

        Ok(OutdatedProjectOutcome {
            project_root: prepared.input.project_root,
            pyproject_path: prepared.input.pyproject_path,
            pylock_path: prepared.input.pylock_path,
            project_id: prepared.identity.id,
            python_version: prepared.installation.version.to_string(),
            checked_packages: intents.len(),
            outdated_packages,
        })
    }

    pub fn select_latest_installed_python(
        installations: &[InstalledPythonRecord],
    ) -> Result<InstalledPythonRecord, ProjectError> {
        installations
            .iter()
            .max_by(|left, right| left.version.cmp(&right.version))
            .cloned()
            .ok_or(ProjectError::NoManagedPythonInstalled)
    }

    pub async fn sync(
        self,
        context: &AppContext,
        request: SyncProjectRequest,
    ) -> Result<SyncProjectOutcome, ProjectError> {
        let prepared = prepare_lock_context(context, &request.lock_targets)?;
        let selection = SyncSelectionResolver.resolve(&prepared.input, &request.selection)?;
        let environment = ProjectEnvironmentStore.ensure(
            context,
            &prepared.identity,
            &ProjectPythonSelection {
                selector: prepared.input.pinned_python.clone(),
                installation: prepared.installation.clone(),
            },
        )?;
        let (lock, lock_status) = match request.lock_mode {
            SyncLockMode::WriteIfNeeded => {
                resolve_or_refresh_lock(
                    &prepared.input,
                    &prepared.lock_targets,
                    &prepared.resolver_environments,
                    &prepared.freshness,
                )
                .await?
            }
            SyncLockMode::Locked => {
                if !prepared.input.pylock_path.exists() {
                    return Err(ProjectError::MissingLockfileForLockedSync {
                        path: prepared.input.pylock_path.to_string(),
                    });
                }

                let lock = LockFile::read(&prepared.input.pylock_path)?;
                if !lock.is_fresh_for(&prepared.freshness, &prepared.lock_targets) {
                    return Err(ProjectError::StaleLockfileForLockedSync {
                        path: prepared.input.pylock_path.to_string(),
                    });
                }

                (lock, LockProjectStatus::ReusedFresh)
            }
            SyncLockMode::Frozen => {
                if !prepared.input.pylock_path.exists() {
                    return Err(ProjectError::MissingLockfileForFrozenSync {
                        path: prepared.input.pylock_path.to_string(),
                    });
                }

                let lock = LockFile::read(&prepared.input.pylock_path)?;
                if !lock.is_fresh_for(&prepared.freshness, &prepared.lock_targets) {
                    return Err(ProjectError::StaleLockfileForFrozenSync {
                        path: prepared.input.pylock_path.to_string(),
                    });
                }

                (lock, LockProjectStatus::ReusedFresh)
            }
        };

        let mut selected_groups = selection.groups.clone();
        if selection.include_base {
            selected_groups.insert("pyra-default".to_string());
        }
        let lock_selection = LockSelection {
            groups: selected_groups.clone(),
            extras: selection.extras.clone(),
            python_full_version: prepared.installation.version.to_string(),
            target_triple: prepared.installation.target_triple.clone(),
        };
        let selected_packages = ReconciliationPlan::for_selection(&lock.packages, &lock_selection);
        let installed_packages =
            EnvironmentInstaller.inspect_installed(&environment.interpreter_path)?;
        let protected_packages = ["pip", "setuptools", "wheel"]
            .into_iter()
            .map(ToString::to_string)
            .collect();
        let plan = ReconciliationPlan::build(
            &selected_packages,
            &installed_packages,
            &protected_packages,
            &prepared.input.project_name,
            prepared.input.build_system_present,
        );
        let applied = EnvironmentInstaller
            .apply(
                &context.paths,
                &environment.interpreter_path,
                &prepared.input.project_root,
                &prepared.input.project_name,
                prepared.input.build_system_present,
                &plan,
                &selected_packages,
            )
            .await?;

        Ok(SyncProjectOutcome {
            project_root: prepared.input.project_root,
            pyproject_path: prepared.input.pyproject_path,
            pylock_path: prepared.input.pylock_path,
            project_id: prepared.identity.id,
            python_version: prepared.installation.version.to_string(),
            lock_refreshed: lock_status != LockProjectStatus::ReusedFresh,
            selected_groups: selected_groups.into_iter().collect(),
            selected_extras: selection.extras.into_iter().collect(),
            installed_packages: applied.installed,
            removed_packages: applied.removed,
            project_installed: prepared.input.build_system_present,
        })
    }
}

fn doctor_issue(
    code: DoctorIssueCode,
    summary: impl Into<String>,
    detail: impl Into<String>,
    suggestion: impl Into<String>,
) -> DoctorIssue {
    DoctorIssue {
        code,
        summary: summary.into(),
        detail: detail.into(),
        suggestion: suggestion.into(),
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct DeclaredPackageIntent {
    package: String,
    normalized_name: String,
    requirements: Vec<String>,
    declaration_scopes: Vec<String>,
}

fn collect_declared_package_intents(input: &ProjectSyncInput) -> Vec<DeclaredPackageIntent> {
    let mut grouped = BTreeMap::<String, DeclaredPackageIntentAccumulator>::new();

    for requirement in &input.dependencies {
        merge_declared_requirement(
            &mut grouped,
            &requirement.requirement,
            "[project].dependencies",
        );
    }
    for group in &input.dependency_groups {
        for requirement in &group.requirements {
            merge_declared_requirement(
                &mut grouped,
                &requirement.requirement,
                &format!("[dependency-groups].{}", group.name.display_name),
            );
        }
    }
    for extra in &input.optional_dependencies {
        for requirement in &extra.requirements {
            merge_declared_requirement(
                &mut grouped,
                &requirement.requirement,
                &format!(
                    "[project.optional-dependencies].{}",
                    extra.name.display_name
                ),
            );
        }
    }

    grouped
        .into_iter()
        .map(|(_, accumulator)| accumulator.finish())
        .collect()
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct DeclaredPackageIntentAccumulator {
    package: String,
    normalized_name: String,
    requirements: BTreeSet<String>,
    declaration_scopes: BTreeSet<String>,
}

impl DeclaredPackageIntentAccumulator {
    fn finish(self) -> DeclaredPackageIntent {
        DeclaredPackageIntent {
            package: self.package,
            normalized_name: self.normalized_name,
            requirements: self.requirements.into_iter().collect(),
            declaration_scopes: self.declaration_scopes.into_iter().collect(),
        }
    }
}

fn merge_declared_requirement(
    grouped: &mut BTreeMap<String, DeclaredPackageIntentAccumulator>,
    requirement: &Requirement,
    scope: &str,
) {
    let package = requirement.name.to_string();
    let normalized_name = normalize_package_name(&package);
    let entry = grouped.entry(normalized_name.clone()).or_insert_with(|| {
        DeclaredPackageIntentAccumulator {
            package: package.clone(),
            normalized_name: normalized_name.clone(),
            requirements: BTreeSet::new(),
            declaration_scopes: BTreeSet::new(),
        }
    });
    entry.requirements.insert(requirement.to_string());
    entry.declaration_scopes.insert(scope.to_string());
}

fn normalize_package_name(name: &str) -> String {
    let mut normalized = String::with_capacity(name.len());
    let mut previous_was_dash = false;
    for character in name.chars() {
        let is_separator = matches!(character, '-' | '_' | '.');
        if is_separator {
            if !previous_was_dash {
                normalized.push('-');
                previous_was_dash = true;
            }
            continue;
        }
        normalized.push(character.to_ascii_lowercase());
        previous_was_dash = false;
    }
    normalized
}

fn package_versions_by_name<'a>(
    packages: impl Iterator<Item = (&'a str, &'a str)>,
) -> BTreeMap<String, String> {
    let mut versions: BTreeMap<String, String> = BTreeMap::new();
    for (name, version) in packages {
        let key = normalize_package_name(name);
        versions
            .entry(key)
            .and_modify(|current| {
                if is_version_newer(version, current.as_str()) {
                    *current = version.to_string();
                }
            })
            .or_insert_with(|| version.to_string());
    }
    versions
}

fn is_version_newer(candidate: &str, current: &str) -> bool {
    match (
        pep440_rs::Version::from_str(candidate),
        pep440_rs::Version::from_str(current),
    ) {
        (Ok(candidate), Ok(current)) => candidate > current,
        _ => candidate > current,
    }
}

struct PreparedLockContext {
    input: ProjectSyncInput,
    identity: ProjectIdentity,
    installation: InstalledPythonRecord,
    lock_targets: LockTargetSet,
    resolver_environments: Vec<ResolverEnvironment>,
    freshness: LockFreshness,
}

fn prepare_lock_context(
    context: &AppContext,
    override_targets: &[String],
) -> Result<PreparedLockContext, ProjectError> {
    let input = ProjectSyncInputLoader.load(context)?;
    let identity = input.project_identity()?;
    let installation = selected_installation(context, &input.pinned_python)?;
    input.validate_selected_interpreter(&installation.version)?;
    let resolver_environment = resolver_environment(&installation)?;
    let lock_targets =
        effective_lock_targets(&input, &installation.target_triple, override_targets)?;
    let resolver_environments =
        resolver_environments_for_targets(&installation.version.to_string(), &lock_targets)?;
    let index_url =
        std::env::var("PYRA_INDEX_URL").unwrap_or_else(|_| "https://pypi.org/simple".to_string());
    let freshness = lock_freshness(
        &input,
        &resolver_environment,
        &index_url,
        resolution_strategy(&lock_targets),
    );

    Ok(PreparedLockContext {
        input,
        identity,
        installation,
        lock_targets,
        resolver_environments,
        freshness,
    })
}

async fn resolve_or_refresh_lock(
    input: &ProjectSyncInput,
    lock_targets: &LockTargetSet,
    resolver_environments: &[ResolverEnvironment],
    freshness: &LockFreshness,
) -> Result<(LockFile, LockProjectStatus), ProjectError> {
    if input.pylock_path.exists() {
        match LockFile::read(&input.pylock_path) {
            Ok(lock) if lock.is_fresh_for(freshness, lock_targets) => {
                return Ok((lock, LockProjectStatus::ReusedFresh));
            }
            Ok(_) | Err(ProjectError::ParseLockfile { .. }) => {
                // Default lock generation owns freshness and regeneration, so
                // stale and unreadable lockfiles are regenerated via the same
                // deterministic resolve->write path.
                let lock = resolve_lock(input, resolver_environments, freshness).await?;
                lock.write()?;
                return Ok((lock, LockProjectStatus::RegeneratedStale));
            }
            Err(error) => return Err(error),
        }
    }

    let lock = resolve_lock(input, resolver_environments, freshness).await?;
    lock.write()?;
    Ok((lock, LockProjectStatus::GeneratedMissing))
}

pub(crate) fn selected_installation(
    context: &AppContext,
    pinned_python: &PythonVersionRequest,
) -> Result<InstalledPythonRecord, ProjectError> {
    let installations = pyra_python::PythonInstallStore
        .list_installed(context)
        .map_err(|source| ProjectError::PinnedPythonNotInstalled {
            selector: pinned_python.to_string(),
            source,
        })?;
    pyra_python::PythonInstallStore
        .select_installed(&installations, pinned_python)
        .map_err(|source| ProjectError::PinnedPythonNotInstalled {
            selector: pinned_python.to_string(),
            source,
        })
}

fn resolver_environment(
    installation: &InstalledPythonRecord,
) -> Result<ResolverEnvironment, ProjectError> {
    resolver_environment_for_target(
        &installation.version.to_string(),
        &installation.target_triple,
    )
}

fn resolver_environments_for_targets(
    python_full_version: &str,
    targets: &LockTargetSet,
) -> Result<Vec<ResolverEnvironment>, ProjectError> {
    targets
        .as_slice()
        .iter()
        .map(|target| resolver_environment_for_target(python_full_version, target))
        .collect()
}

fn resolver_environment_for_target(
    python_full_version: &str,
    target_triple: &str,
) -> Result<ResolverEnvironment, ProjectError> {
    let release = PythonVersion::parse(python_full_version)
        .map_err(|source| ProjectError::InvalidManagedPythonVersion {
            value: python_full_version.to_string(),
            detail: source.to_string(),
        })?
        .segments();
    let python_version = if release.len() >= 2 {
        format!("{}.{}", release[0], release[1])
    } else {
        python_full_version.to_string()
    };
    let (os_name, sys_platform, platform_system, platform_machine) =
        marker_platform_fields(target_triple);
    let markers = MarkerEnvironmentBuilder {
        implementation_name: "cpython",
        implementation_version: python_full_version,
        os_name,
        platform_machine,
        platform_python_implementation: "CPython",
        platform_release: "",
        platform_system,
        platform_version: "",
        python_full_version,
        python_version: &python_version,
        sys_platform,
    }
    .try_into()
    .map_err(
        |error: pep440_rs::VersionParseError| ProjectError::ParseLockfile {
            path: "marker environment".to_string(),
            detail: error.to_string(),
        },
    )?;
    ResolverEnvironment::new(markers, python_full_version, target_triple.to_string())
        .map_err(|error| ProjectError::ResolveDependencies { source: error })
}

fn marker_platform_fields(
    target_triple: &str,
) -> (&'static str, &'static str, &'static str, &'static str) {
    match target_triple {
        "aarch64-apple-darwin" => ("posix", "darwin", "Darwin", "arm64"),
        "x86_64-apple-darwin" => ("posix", "darwin", "Darwin", "x86_64"),
        "x86_64-unknown-linux-gnu" => ("posix", "linux", "Linux", "x86_64"),
        "aarch64-unknown-linux-gnu" => ("posix", "linux", "Linux", "aarch64"),
        _ => ("posix", "linux", "Linux", "x86_64"),
    }
}

fn effective_lock_targets(
    input: &ProjectSyncInput,
    host_target: &str,
    override_targets: &[String],
) -> Result<LockTargetSet, ProjectError> {
    let targets = if override_targets.is_empty() {
        input
            .declared_lock_targets
            .clone()
            .unwrap_or_else(|| LockTargetSet::single(host_target.to_string()))
    } else {
        LockTargetSet::from_override(override_targets)?
    };

    if !targets.contains(host_target) {
        return Err(ProjectError::CurrentHostMissingFromLockTargets {
            host: host_target.to_string(),
            targets: targets.as_slice().to_vec(),
        });
    }

    Ok(targets)
}

fn resolution_strategy(targets: &LockTargetSet) -> &'static str {
    if targets.len() > 1 {
        MULTI_TARGET_RESOLUTION_STRATEGY
    } else {
        CURRENT_RESOLUTION_STRATEGY
    }
}

fn dependency_fingerprint(input: &ProjectSyncInput) -> String {
    let mut digest = Sha256::new();
    // Keep the fingerprint scoped to declared project inputs. Interpreter,
    // target, index, and strategy stay as separate typed freshness fields so
    // lock reuse can report and compare each documented dimension explicitly.
    digest.update(input.project_name.as_bytes());
    digest.update(input.pinned_python.to_string().as_bytes());
    if let Some(requires_python) = &input.requires_python {
        digest.update(requires_python.as_bytes());
    }
    for requirement in &input.dependencies {
        digest.update(requirement.requirement.to_string().as_bytes());
    }
    for group in &input.optional_dependencies {
        digest.update(group.name.normalized_name.as_bytes());
        for requirement in &group.requirements {
            digest.update(requirement.requirement.to_string().as_bytes());
        }
    }
    for group in &input.dependency_groups {
        digest.update(group.name.normalized_name.as_bytes());
        for requirement in &group.requirements {
            digest.update(requirement.requirement.to_string().as_bytes());
        }
    }
    format!("{:x}", digest.finalize())
}

fn lock_freshness(
    input: &ProjectSyncInput,
    env: &ResolverEnvironment,
    index_url: &str,
    resolution_strategy: &str,
) -> LockFreshness {
    LockFreshness {
        dependency_fingerprint: dependency_fingerprint(input),
        interpreter_version: env.python_full_version.clone(),
        target_triple: env.target_triple.clone(),
        index_url: index_url.to_string(),
        resolution_strategy: resolution_strategy.to_string(),
    }
}

async fn resolve_lock(
    input: &ProjectSyncInput,
    environments: &[ResolverEnvironment],
    freshness: &LockFreshness,
) -> Result<LockFile, ProjectError> {
    if environments.len() == 1 {
        let environment = &environments[0];
        let request = ResolutionRequestTemplate::new(
            build_resolution_roots(input),
            freshness.index_url.clone(),
        )
        .for_environment(environment.clone());
        let packages = Resolver::new()
            .resolve(request)
            .await
            .map_err(|source| ProjectError::ResolveDependencies { source })?
            .into_iter()
            .map(|package| map_resolved_package(package, &freshness.index_url))
            .collect::<Vec<_>>();

        return Ok(LockFile {
            path: input.pylock_path.clone(),
            requires_python: input.requires_python.clone(),
            environments: vec![lock_environment(environment)],
            extras: input
                .optional_dependencies
                .iter()
                .map(|extra| extra.name.normalized_name.clone())
                .collect(),
            dependency_groups: input
                .dependency_groups
                .iter()
                .map(|group| group.name.normalized_name.clone())
                .collect(),
            default_groups: default_group_names(input),
            packages,
            tool_pyra: freshness.clone(),
        });
    }

    resolve_lock_for_environments(input, environments, freshness).await
}

/// Multi-target lock generation resolves one environment at a time so the
/// existing resolver contract stays unchanged. The current merge step is
/// intentionally strict: only identical package graphs are merged until
/// target-scoped install selection exists.
async fn resolve_lock_for_environments(
    input: &ProjectSyncInput,
    environments: &[ResolverEnvironment],
    freshness: &LockFreshness,
) -> Result<LockFile, ProjectError> {
    let request_template =
        ResolutionRequestTemplate::new(build_resolution_roots(input), freshness.index_url.clone());
    let resolver = Resolver::new();
    let mut merged_packages = Vec::new();
    let mut expected_shape: Option<Vec<LockPackage>> = None;

    for environment in environments {
        let target_packages = resolver
            .resolve(request_template.for_environment(environment.clone()))
            .await
            .map_err(|source| ProjectError::ResolveDependenciesForTarget {
                environment: environment_id(environment),
                source,
            })?
            .into_iter()
            .map(|package| map_resolved_package(package, &freshness.index_url))
            .collect::<Vec<_>>();

        let current_shape = package_shape(&target_packages);
        if let Some(expected_shape) = &expected_shape {
            if current_shape != *expected_shape {
                return Err(ProjectError::MultiTargetLockMergeMismatch {
                    environment: environment_id(environment),
                    detail: describe_package_shape_mismatch(expected_shape, &current_shape),
                });
            }
            merge_target_artifacts(&mut merged_packages, &target_packages);
        } else {
            expected_shape = Some(current_shape);
            merged_packages = target_packages;
        }
    }

    Ok(LockFile {
        path: input.pylock_path.clone(),
        requires_python: input.requires_python.clone(),
        environments: environments.iter().map(lock_environment).collect(),
        extras: input
            .optional_dependencies
            .iter()
            .map(|extra| extra.name.normalized_name.clone())
            .collect(),
        dependency_groups: input
            .dependency_groups
            .iter()
            .map(|group| group.name.normalized_name.clone())
            .collect(),
        default_groups: default_group_names(input),
        packages: merged_packages,
        tool_pyra: freshness.clone(),
    })
}

fn build_resolution_roots(input: &ProjectSyncInput) -> Vec<ResolutionRoot> {
    let mut roots = Vec::new();
    roots.push(ResolutionRoot {
        token: ResolutionRootToken {
            kind: ResolutionRootTokenKind::DependencyGroup,
            name: "pyra-default".to_string(),
        },
        requirements: input
            .dependencies
            .iter()
            .map(|requirement| requirement.requirement.to_string())
            .collect(),
    });
    for group in &input.dependency_groups {
        roots.push(ResolutionRoot {
            token: ResolutionRootToken {
                kind: ResolutionRootTokenKind::DependencyGroup,
                name: group.name.normalized_name.clone(),
            },
            requirements: group
                .requirements
                .iter()
                .map(|requirement| requirement.requirement.to_string())
                .collect(),
        });
    }
    for extra in &input.optional_dependencies {
        roots.push(ResolutionRoot {
            token: ResolutionRootToken {
                kind: ResolutionRootTokenKind::Extra,
                name: extra.name.normalized_name.clone(),
            },
            requirements: extra
                .requirements
                .iter()
                .map(|requirement| requirement.requirement.to_string())
                .collect(),
        });
    }
    roots
}

// Keep the initial environment id deterministic so later multi-target locks can
// add more slices without renaming the existing single-environment shape.
fn environment_id(env: &ResolverEnvironment) -> String {
    format!("cpython-{}-{}", env.python_full_version, env.target_triple)
}

fn environment_marker(env: &ResolverEnvironment) -> String {
    format!(
        "implementation_name == 'cpython' and python_full_version == '{}' and sys_platform == '{}' and platform_machine == '{}'",
        env.python_full_version,
        env.markers.sys_platform(),
        env.markers.platform_machine()
    )
}

fn lock_environment(env: &ResolverEnvironment) -> LockEnvironment {
    LockEnvironment {
        id: environment_id(env),
        marker: environment_marker(env),
        interpreter_version: env.python_full_version.clone(),
        target_triple: env.target_triple.clone(),
    }
}

fn default_group_names(input: &ProjectSyncInput) -> Vec<String> {
    let mut groups = vec!["pyra-default".to_string()];
    if input.has_dev_group() {
        groups.push("dev".to_string());
    }
    groups
}

fn package_marker(tokens: &[ResolutionRootToken]) -> Option<LockMarker> {
    let clauses = tokens
        .iter()
        .map(|token| match token.kind {
            ResolutionRootTokenKind::DependencyGroup => {
                LockMarkerClause::dependency_group(token.name.clone())
            }
            ResolutionRootTokenKind::Extra => LockMarkerClause::extra(token.name.clone()),
        })
        .collect::<Vec<_>>();
    LockMarker::from_clauses(clauses)
}

fn map_resolved_package(package: pyra_resolver::ResolvedPackage, index_url: &str) -> LockPackage {
    LockPackage {
        name: package.name,
        version: package.version,
        marker: package_marker(&package.root_tokens),
        requires_python: package.requires_python,
        index: Some(index_url.to_string()),
        dependencies: package
            .dependencies
            .into_iter()
            .map(|dependency| LockDependencyRef {
                name: dependency.name,
                version: dependency.version,
            })
            .collect(),
        sdist: package
            .artifacts
            .iter()
            .find(|artifact| matches!(artifact.kind, pyra_resolver::ArtifactKind::Sdist))
            .map(map_artifact),
        wheels: package
            .artifacts
            .iter()
            .filter(|artifact| matches!(artifact.kind, pyra_resolver::ArtifactKind::Wheel))
            .map(map_artifact)
            .collect(),
    }
}

fn map_artifact(artifact: &pyra_resolver::ArtifactRecord) -> LockArtifact {
    LockArtifact {
        name: artifact.name.clone(),
        url: artifact.url.clone(),
        size: artifact.size,
        upload_time: artifact.upload_time.clone(),
        sha256: artifact.sha256.clone(),
    }
}

fn package_shape(packages: &[LockPackage]) -> Vec<LockPackage> {
    let mut shape = packages
        .iter()
        .map(|package| LockPackage {
            name: package.name.clone(),
            version: package.version.clone(),
            marker: package.marker.clone(),
            requires_python: package.requires_python.clone(),
            index: package.index.clone(),
            dependencies: package.dependencies.clone(),
            sdist: None,
            wheels: Vec::new(),
        })
        .collect::<Vec<_>>();
    shape.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then(left.version.cmp(&right.version))
    });
    shape
}

fn merge_target_artifacts(merged: &mut [LockPackage], target: &[LockPackage]) {
    for (existing, incoming) in merged.iter_mut().zip(target.iter()) {
        if existing.sdist.is_none() {
            existing.sdist = incoming.sdist.clone();
        }
        existing.wheels.extend(incoming.wheels.iter().cloned());
        existing
            .wheels
            .sort_by(|left, right| left.name.cmp(&right.name));
        existing.wheels.dedup();
    }
}

fn describe_package_shape_mismatch(expected: &[LockPackage], actual: &[LockPackage]) -> String {
    let expected_names = expected
        .iter()
        .map(|package| format!("{}=={}", package.name, package.version))
        .collect::<Vec<_>>();
    let actual_names = actual
        .iter()
        .map(|package| format!("{}=={}", package.name, package.version))
        .collect::<Vec<_>>();
    format!(
        "expected packages [{}] but resolved [{}]",
        expected_names.join(", "),
        actual_names.join(", ")
    )
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct InitProjectRequest {
    pub python_selector: PythonVersionRequest,
    pub installation: InstalledPythonRecord,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct InitProjectWithPythonOutcome {
    pub init: InitProjectOutcome,
    pub project_id: String,
    pub environment: ProjectEnvironmentRecord,
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command as ProcessCommand;
    use std::sync::{Mutex, OnceLock};

    use camino::Utf8PathBuf;
    use pyra_core::{AppContext, AppPaths, Verbosity};
    use pyra_errors::UserFacingError;
    use pyra_python::{ArchiveFormat, HostTarget, InstalledPythonRecord, PythonVersion};
    use serde_json::json;

    use super::{
        ProjectService, SyncLockMode, SyncProjectRequest, lock_freshness,
        resolve_lock_for_environments, resolver_environment_for_target,
    };
    use crate::ProjectError;
    use crate::sync::MULTI_TARGET_RESOLUTION_STRATEGY;

    #[test]
    fn selects_latest_installed_python_version() {
        let latest = ProjectService::select_latest_installed_python(&[
            record("3.12.9"),
            record("3.13.2"),
            record("3.13.12"),
        ])
        .expect("latest installation");

        assert_eq!(latest.version, PythonVersion::parse("3.13.12").unwrap());
    }

    #[tokio::test]
    async fn sync_accepts_compatible_project_requires_python() {
        let _guard = installer_state_lock().lock().expect("installer state lock");
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().join("workspace").join("sample"))
            .expect("utf-8 project root");
        fs::create_dir_all(&root).expect("project root");
        let context = context_for_project(temp_dir.path(), &root);
        let _stub_state = stub_installer_state(&root.join("installer-state.json"));
        write_pyproject(
            &root.join("pyproject.toml"),
            r#"[project]
name = "sample"
version = "0.1.0"
requires-python = ">=3.13,<3.14"
dependencies = []

[tool.pyra]
python = "3.13.12"
"#,
        );
        seed_managed_install(&context, "3.13.12").expect("managed install");

        let outcome = ProjectService
            .sync(&context, SyncProjectRequest::default())
            .await
            .expect("compatible sync succeeds");

        assert_eq!(outcome.python_version, "3.13.12");
        assert!(outcome.lock_refreshed);
        assert!(root.join("pylock.toml").exists());
    }

    #[tokio::test]
    async fn sync_rejects_incompatible_project_requires_python_before_lock_reuse() {
        let _guard = installer_state_lock().lock().expect("installer state lock");
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().join("workspace").join("sample"))
            .expect("utf-8 project root");
        fs::create_dir_all(&root).expect("project root");
        let context = context_for_project(temp_dir.path(), &root);
        write_pyproject(
            &root.join("pyproject.toml"),
            r#"[project]
name = "sample"
version = "0.1.0"
requires-python = "<3.13"
dependencies = []

[tool.pyra]
python = "3.13.12"
"#,
        );
        fs::write(root.join("pylock.toml"), "not valid toml").expect("pylock");
        seed_managed_install(&context, "3.13.12").expect("managed install");

        let error = ProjectService
            .sync(&context, SyncProjectRequest::default())
            .await
            .expect_err("incompatible sync fails");

        assert!(matches!(
            error,
            ProjectError::PinnedPythonIncompatibleWithProject {
                ref interpreter,
                ref requires_python
            } if interpreter == "3.13.12" && requires_python == "<3.13"
        ));
        let report = error.report();
        assert!(report.summary.contains("3.13.12"));
        assert!(report.summary.contains("<3.13"));
        assert!(!context.paths.project_environments_dir().exists());
    }

    #[tokio::test]
    async fn sync_locked_requires_existing_lock_before_resolution() {
        let _guard = installer_state_lock().lock().expect("installer state lock");
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().join("workspace").join("sample"))
            .expect("utf-8 project root");
        fs::create_dir_all(&root).expect("project root");
        let context = context_for_project(temp_dir.path(), &root);
        let _stub_state = stub_installer_state(&root.join("installer-state.json"));
        write_pyproject(
            &root.join("pyproject.toml"),
            r#"[project]
name = "sample"
version = "0.1.0"
dependencies = []

[tool.pyra]
python = "3.13.12"
"#,
        );
        seed_managed_install(&context, "3.13.12").expect("managed install");

        let error = ProjectService
            .sync(
                &context,
                SyncProjectRequest {
                    lock_mode: SyncLockMode::Locked,
                    ..SyncProjectRequest::default()
                },
            )
            .await
            .expect_err("missing lock should fail");

        assert!(matches!(
            error,
            ProjectError::MissingLockfileForLockedSync { .. }
        ));
        assert!(!root.join("pylock.toml").exists());
    }

    #[tokio::test]
    async fn resolves_host_plus_linux_fixture_into_one_lock() {
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().join("workspace").join("sample"))
            .expect("utf-8 project root");
        fs::create_dir_all(&root).expect("project root");
        let context = context_for_project(temp_dir.path(), &root);
        write_pyproject(
            &root.join("pyproject.toml"),
            r#"[project]
name = "sample"
version = "0.1.0"
dependencies = ["shared==1.0.0"]

[tool.pyra]
python = "3.13.12"
"#,
        );
        let input = crate::sync::ProjectSyncInputLoader
            .load(&context)
            .expect("project input");
        let index = target_fixture_index(
            &[(
                "shared",
                &[
                    "shared-1.0.0-cp313-abi3-macosx_11_0_arm64.whl",
                    "shared-1.0.0-cp313-abi3-manylinux_2_17_x86_64.whl",
                ],
            )],
            "Metadata-Version: 2.3\nName: shared\nVersion: 1.0.0\n",
        )
        .expect("fixture index");
        let host = resolver_environment_for_target("3.13.12", "aarch64-apple-darwin")
            .expect("host environment");
        let linux = resolver_environment_for_target("3.13.12", "x86_64-unknown-linux-gnu")
            .expect("linux environment");
        let mut freshness = lock_freshness(
            &input,
            &host,
            &index.base_url,
            super::CURRENT_RESOLUTION_STRATEGY,
        );
        freshness.resolution_strategy = MULTI_TARGET_RESOLUTION_STRATEGY.to_string();

        let lock = resolve_lock_for_environments(&input, &[host, linux], &freshness)
            .await
            .expect("multi-target lock");

        assert_eq!(lock.environments.len(), 2);
        assert_eq!(
            lock.environments
                .iter()
                .map(|environment| {
                    (
                        environment.id.as_str(),
                        environment.interpreter_version.as_str(),
                        environment.target_triple.as_str(),
                    )
                })
                .collect::<Vec<_>>(),
            vec![
                (
                    "cpython-3.13.12-aarch64-apple-darwin",
                    "3.13.12",
                    "aarch64-apple-darwin",
                ),
                (
                    "cpython-3.13.12-x86_64-unknown-linux-gnu",
                    "3.13.12",
                    "x86_64-unknown-linux-gnu",
                ),
            ]
        );
        assert_eq!(
            lock.packages[0]
                .wheels
                .iter()
                .map(|wheel| wheel.name.as_str())
                .collect::<Vec<_>>(),
            vec![
                "shared-1.0.0-cp313-abi3-macosx_11_0_arm64.whl",
                "shared-1.0.0-cp313-abi3-manylinux_2_17_x86_64.whl",
            ]
        );
        assert_eq!(
            lock.tool_pyra.resolution_strategy,
            MULTI_TARGET_RESOLUTION_STRATEGY
        );
    }

    #[tokio::test]
    async fn resolves_host_plus_macos_fixture_into_one_lock() {
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().join("workspace").join("sample"))
            .expect("utf-8 project root");
        fs::create_dir_all(&root).expect("project root");
        let context = context_for_project(temp_dir.path(), &root);
        write_pyproject(
            &root.join("pyproject.toml"),
            r#"[project]
name = "sample"
version = "0.1.0"
dependencies = ["shared==1.0.0"]

[tool.pyra]
python = "3.13.12"
"#,
        );
        let input = crate::sync::ProjectSyncInputLoader
            .load(&context)
            .expect("project input");
        let index = target_fixture_index(
            &[(
                "shared",
                &[
                    "shared-1.0.0-cp313-abi3-manylinux_2_17_x86_64.whl",
                    "shared-1.0.0-cp313-abi3-macosx_11_0_x86_64.whl",
                ],
            )],
            "Metadata-Version: 2.3\nName: shared\nVersion: 1.0.0\n",
        )
        .expect("fixture index");
        let host = resolver_environment_for_target("3.13.12", "x86_64-unknown-linux-gnu")
            .expect("host environment");
        let macos = resolver_environment_for_target("3.13.12", "x86_64-apple-darwin")
            .expect("macOS environment");
        let mut freshness = lock_freshness(
            &input,
            &host,
            &index.base_url,
            super::CURRENT_RESOLUTION_STRATEGY,
        );
        freshness.resolution_strategy = MULTI_TARGET_RESOLUTION_STRATEGY.to_string();

        let lock = resolve_lock_for_environments(&input, &[host, macos], &freshness)
            .await
            .expect("multi-target lock");

        assert_eq!(lock.environments.len(), 2);
        assert_eq!(
            lock.packages[0]
                .wheels
                .iter()
                .map(|wheel| wheel.name.as_str())
                .collect::<Vec<_>>(),
            vec![
                "shared-1.0.0-cp313-abi3-macosx_11_0_x86_64.whl",
                "shared-1.0.0-cp313-abi3-manylinux_2_17_x86_64.whl",
            ]
        );
    }

    #[tokio::test]
    async fn reports_the_failing_target_environment_for_multi_target_resolution() {
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().join("workspace").join("sample"))
            .expect("utf-8 project root");
        fs::create_dir_all(&root).expect("project root");
        let context = context_for_project(temp_dir.path(), &root);
        write_pyproject(
            &root.join("pyproject.toml"),
            r#"[project]
name = "sample"
version = "0.1.0"
dependencies = ["shared==1.0.0"]

[tool.pyra]
python = "3.13.12"
"#,
        );
        let input = crate::sync::ProjectSyncInputLoader
            .load(&context)
            .expect("project input");
        let index = target_fixture_index(
            &[("shared", &["shared-1.0.0-cp313-abi3-macosx_11_0_arm64.whl"])],
            "Metadata-Version: 2.3\nName: shared\nVersion: 1.0.0\n",
        )
        .expect("fixture index");
        let host = resolver_environment_for_target("3.13.12", "aarch64-apple-darwin")
            .expect("host environment");
        let linux = resolver_environment_for_target("3.13.12", "x86_64-unknown-linux-gnu")
            .expect("linux environment");
        let mut freshness = lock_freshness(
            &input,
            &host,
            &index.base_url,
            super::CURRENT_RESOLUTION_STRATEGY,
        );
        freshness.resolution_strategy = MULTI_TARGET_RESOLUTION_STRATEGY.to_string();

        let error = resolve_lock_for_environments(&input, &[host, linux], &freshness)
            .await
            .expect_err("linux target should fail");

        assert!(matches!(
            error,
            ProjectError::ResolveDependenciesForTarget {
                ref environment,
                ref source,
            } if environment == "cpython-3.13.12-x86_64-unknown-linux-gnu"
                && matches!(
                    source,
                    pyra_resolver::ResolverError::NoInstallableArtifacts { package }
                    if package == "shared"
                )
        ));
        assert!(
            error
                .report()
                .summary
                .contains("cpython-3.13.12-x86_64-unknown-linux-gnu")
        );
    }

    fn installer_state_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn stub_installer_state(path: &camino::Utf8Path) -> StubInstallerState {
        fs::write(path, "{}\n").expect("stub installer state");
        // The installer stub is selected through a process-wide environment
        // variable, so tests serialize access before mutating it.
        unsafe {
            std::env::set_var("PYRA_SYNC_INSTALLER_STATE_PATH", path.as_str());
        }
        StubInstallerState
    }

    fn context_for_project(
        temp_root: &std::path::Path,
        project_root: &camino::Utf8Path,
    ) -> AppContext {
        let config_dir =
            Utf8PathBuf::from_path_buf(temp_root.join("config")).expect("utf-8 config");
        let data_dir = Utf8PathBuf::from_path_buf(temp_root.join("data")).expect("utf-8 data");
        let cache_dir = Utf8PathBuf::from_path_buf(temp_root.join("cache")).expect("utf-8 cache");
        let state_dir = Utf8PathBuf::from_path_buf(temp_root.join("state")).expect("utf-8 state");
        let paths = AppPaths::from_roots(config_dir, data_dir, cache_dir, state_dir);
        AppContext::new(project_root.to_path_buf(), paths, Verbosity::Normal)
    }

    fn write_pyproject(path: &camino::Utf8Path, contents: &str) {
        fs::write(path, contents).expect("pyproject");
    }

    fn seed_managed_install(
        context: &AppContext,
        version: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let install_dir = context.paths.python_version_dir(version);
        fs::create_dir_all(&install_dir)?;

        let record = InstalledPythonRecord {
            version: PythonVersion::parse(version)?,
            implementation: "cpython".to_string(),
            build_id: "20260325".to_string(),
            target_triple: HostTarget::detect()?.target_triple().to_string(),
            asset_name: format!("cpython-{version}.tar.gz"),
            archive_format: ArchiveFormat::TarGz,
            download_url: "file:///dev/null".to_string(),
            checksum_sha256: None,
            install_dir,
            executable_path: Utf8PathBuf::from_path_buf(system_python()?)
                .expect("utf-8 python path"),
        };

        fs::write(
            record.install_dir.join("installation.json"),
            serde_json::to_vec_pretty(&record)?,
        )?;

        Ok(())
    }

    fn system_python() -> Result<PathBuf, Box<dyn std::error::Error>> {
        for candidate in ["python3", "python"] {
            let output = ProcessCommand::new(candidate)
                .args(["-c", "import sys; print(sys.executable)"])
                .output();
            match output {
                Ok(output) if output.status.success() => {
                    let path = String::from_utf8(output.stdout)?.trim().to_string();
                    if !path.is_empty() {
                        return Ok(PathBuf::from(path));
                    }
                }
                Ok(_) | Err(_) => {}
            }
        }

        Err("no usable system python was found for service tests".into())
    }

    struct StubInstallerState;

    impl Drop for StubInstallerState {
        fn drop(&mut self) {
            unsafe {
                std::env::remove_var("PYRA_SYNC_INSTALLER_STATE_PATH");
            }
        }
    }

    fn record(version: &str) -> InstalledPythonRecord {
        InstalledPythonRecord {
            version: PythonVersion::parse(version).unwrap(),
            implementation: "cpython".to_string(),
            build_id: "20260325".to_string(),
            target_triple: "aarch64-apple-darwin".to_string(),
            asset_name: "asset.tar.gz".to_string(),
            archive_format: ArchiveFormat::TarGz,
            download_url: "https://example.test/asset.tar.gz".to_string(),
            checksum_sha256: None,
            install_dir: Utf8PathBuf::from(format!("/tmp/{version}")),
            executable_path: Utf8PathBuf::from(format!("/tmp/{version}/python/bin/python3")),
        }
    }

    struct TargetFixtureIndex {
        _temp_dir: tempfile::TempDir,
        base_url: String,
    }

    fn target_fixture_index(
        packages: &[(&str, &[&str])],
        metadata_contents: &str,
    ) -> Result<TargetFixtureIndex, Box<dyn std::error::Error>> {
        let temp_dir = tempfile::tempdir()?;
        let index_dir = temp_dir.path().join("simple");
        let files_dir = temp_dir.path().join("files");
        fs::create_dir_all(&index_dir)?;
        fs::create_dir_all(&files_dir)?;

        for (package, filenames) in packages {
            let files = filenames
                .iter()
                .enumerate()
                .map(|(index, filename)| {
                    let artifact_path = files_dir.join(filename);
                    fs::write(&artifact_path, format!("fixture artifact: {filename}\n"))?;
                    fs::write(
                        format!("{}.metadata", artifact_path.display()),
                        metadata_contents,
                    )?;
                    Ok(json!({
                        "filename": filename,
                        "url": format!("file://{}", artifact_path.display()),
                        "hashes": { "sha256": format!("{:064x}", index + 1) },
                        "size": fs::metadata(&artifact_path)?.len(),
                        "core-metadata": true,
                    }))
                })
                .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
            fs::write(
                index_dir.join(format!("{package}.json")),
                serde_json::to_vec_pretty(&json!({ "files": files }))?,
            )?;
        }

        Ok(TargetFixtureIndex {
            _temp_dir: temp_dir,
            base_url: format!("file://{}", index_dir.display()),
        })
    }
}
