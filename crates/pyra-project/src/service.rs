//! Project-domain service facade.
//!
//! This service owns project discovery, config updates, and centralized
//! environment preparation while leaving Python release resolution to the
//! dedicated Python subsystem.

use camino::Utf8PathBuf;
use pep508_rs::MarkerEnvironmentBuilder;
use pyra_core::AppContext;
use pyra_python::{InstalledPythonRecord, PythonVersionRequest};
use pyra_resolver::Resolver;
use pyra_resolver::{
    ResolutionRequest, ResolutionRoot, ResolutionRootToken, ResolutionRootTokenKind,
    ResolverEnvironment,
};
use sha2::{Digest, Sha256};

use crate::{
    ProjectError,
    environment::{ProjectEnvironmentRecord, ProjectEnvironmentStore, ProjectPythonSelection},
    identity::{ProjectIdentity, find_project_root},
    init::{InitProjectOutcome, create_initial_layout, validate_initial_layout},
    pyproject::{read_python_selector, update_python_selector},
    sync::{
        EnvironmentInstaller, LockArtifact, LockDependencyRef, LockFile, LockPackage,
        LockSelection, LockToolPyraMetadata, ProjectSyncInput, ProjectSyncInputLoader,
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

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct SyncProjectRequest {
    pub selection: SyncSelectionRequest,
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
        update_python_selector(&pyproject_path, &request.python.selector)?;
        let project_context = AppContext::new(
            project_root.clone(),
            context.paths.clone(),
            context.verbosity,
        );
        let environment = ProjectEnvironmentStore.create_or_refresh(
            &project_context,
            &identity,
            &request.python,
        )?;

        Ok(UseProjectPythonOutcome {
            project_root,
            project_id: identity.id,
            pyproject_path,
            environment,
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
        let input = ProjectSyncInputLoader.load(context)?;
        let identity = input.project_identity()?;
        let installations = pyra_python::PythonInstallStore
            .list_installed(context)
            .map_err(|source| ProjectError::PinnedPythonNotInstalled {
                selector: input.pinned_python.to_string(),
                source,
            })?;
        let installation = pyra_python::PythonInstallStore
            .select_installed(&installations, &input.pinned_python)
            .map_err(|source| ProjectError::PinnedPythonNotInstalled {
                selector: input.pinned_python.to_string(),
                source,
            })?;
        let selection = SyncSelectionResolver.resolve(&input, &request.selection)?;
        let environment = ProjectEnvironmentStore.ensure(
            context,
            &identity,
            &ProjectPythonSelection {
                selector: input.pinned_python.clone(),
                installation: installation.clone(),
            },
        )?;
        let resolver_environment = resolver_environment(&installation)?;
        let index_url = std::env::var("PYRA_INDEX_URL")
            .unwrap_or_else(|_| "https://pypi.org/simple".to_string());
        let fingerprint = input_fingerprint(&input, &resolver_environment, &index_url);
        let (lock, lock_refreshed) = if input.pylock_path.exists() {
            let lock = LockFile::read(&input.pylock_path)?;
            if lock.is_fresh(
                &fingerprint,
                &installation.version.to_string(),
                &resolver_environment.target_triple,
                &index_url,
            ) {
                (lock, false)
            } else {
                let lock =
                    resolve_lock(&input, &resolver_environment, &index_url, &fingerprint).await?;
                lock.write()?;
                (lock, true)
            }
        } else {
            let lock =
                resolve_lock(&input, &resolver_environment, &index_url, &fingerprint).await?;
            lock.write()?;
            (lock, true)
        };

        let mut selected_groups = selection.groups.clone();
        if selection.include_base {
            selected_groups.insert("pyra-default".to_string());
        }
        let lock_selection = LockSelection {
            groups: selected_groups.clone(),
            extras: selection.extras.clone(),
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
            &input.project_name,
            input.build_system_present,
        );
        let applied = EnvironmentInstaller.apply(
            &environment.interpreter_path,
            &input.project_root,
            &input.project_name,
            input.build_system_present,
            &plan,
            &selected_packages,
        )?;

        Ok(SyncProjectOutcome {
            project_root: input.project_root,
            pyproject_path: input.pyproject_path,
            pylock_path: input.pylock_path,
            project_id: identity.id,
            python_version: installation.version.to_string(),
            lock_refreshed,
            selected_groups: selected_groups.into_iter().collect(),
            selected_extras: selection.extras.into_iter().collect(),
            installed_packages: applied.installed,
            removed_packages: applied.removed,
            project_installed: input.build_system_present,
        })
    }
}

fn resolver_environment(
    installation: &InstalledPythonRecord,
) -> Result<ResolverEnvironment, ProjectError> {
    let release = installation.version.segments();
    let python_full_version = installation.version.to_string();
    let python_version = if release.len() >= 2 {
        format!("{}.{}", release[0], release[1])
    } else {
        python_full_version.clone()
    };
    let (os_name, sys_platform, platform_system, platform_machine) =
        marker_platform_fields(&installation.target_triple);
    let markers = MarkerEnvironmentBuilder {
        implementation_name: "cpython",
        implementation_version: &python_full_version,
        os_name,
        platform_machine,
        platform_python_implementation: "CPython",
        platform_release: "",
        platform_system,
        platform_version: "",
        python_full_version: &python_full_version,
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
    ResolverEnvironment::new(
        markers,
        python_full_version,
        installation.target_triple.clone(),
    )
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

fn input_fingerprint(
    input: &ProjectSyncInput,
    env: &ResolverEnvironment,
    index_url: &str,
) -> String {
    let mut digest = Sha256::new();
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
    digest.update(env.python_full_version.as_bytes());
    digest.update(env.target_triple.as_bytes());
    digest.update(index_url.as_bytes());
    digest.update(b"current-platform-union-v1");
    format!("{:x}", digest.finalize())
}

async fn resolve_lock(
    input: &ProjectSyncInput,
    env: &ResolverEnvironment,
    index_url: &str,
    fingerprint: &str,
) -> Result<LockFile, ProjectError> {
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

    let packages = Resolver::new()
        .resolve(ResolutionRequest {
            environment: env.clone(),
            roots,
            index_url: index_url.to_string(),
        })
        .await
        .map_err(|source| ProjectError::ResolveDependencies { source })?;

    Ok(LockFile {
        path: input.pylock_path.clone(),
        requires_python: input.requires_python.clone(),
        environments: vec![environment_marker(env)],
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
        packages: packages
            .into_iter()
            .map(|package| {
                let marker = package_marker(&package.root_tokens);
                LockPackage {
                    name: package.name,
                    version: package.version,
                    marker: if marker.is_empty() {
                        None
                    } else {
                        Some(marker)
                    },
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
                        .find(|artifact| {
                            matches!(artifact.kind, pyra_resolver::ArtifactKind::Sdist)
                        })
                        .map(map_artifact),
                    wheels: package
                        .artifacts
                        .iter()
                        .filter(|artifact| {
                            matches!(artifact.kind, pyra_resolver::ArtifactKind::Wheel)
                        })
                        .map(map_artifact)
                        .collect(),
                }
            })
            .collect(),
        tool_pyra: LockToolPyraMetadata {
            input_fingerprint: fingerprint.to_string(),
            interpreter_version: env.python_full_version.clone(),
            target_triple: env.target_triple.clone(),
            index_url: index_url.to_string(),
            resolution_strategy: "current-platform-union-v1".to_string(),
        },
    })
}

fn environment_marker(env: &ResolverEnvironment) -> String {
    format!(
        "implementation_name == 'cpython' and python_full_version == '{}' and sys_platform == '{}' and platform_machine == '{}'",
        env.python_full_version,
        env.markers.sys_platform(),
        env.markers.platform_machine()
    )
}

fn default_group_names(input: &ProjectSyncInput) -> Vec<String> {
    let mut groups = vec!["pyra-default".to_string()];
    if input.has_dev_group() {
        groups.push("dev".to_string());
    }
    groups
}

fn package_marker(tokens: &[ResolutionRootToken]) -> String {
    let mut tokens = tokens
        .iter()
        .map(|token| match token.kind {
            ResolutionRootTokenKind::DependencyGroup => {
                format!("'{}' in dependency_groups", token.name)
            }
            ResolutionRootTokenKind::Extra => format!("'{}' in extras", token.name),
        })
        .collect::<Vec<_>>();
    tokens.sort();
    tokens.dedup();
    tokens.join(" or ")
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
    use camino::Utf8PathBuf;
    use pyra_python::{ArchiveFormat, InstalledPythonRecord, PythonVersion};

    use super::ProjectService;

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
}
