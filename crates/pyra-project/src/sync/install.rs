//! Exact reconciliation planning and installer backends.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::process::Command;

use camino::{Utf8Path, Utf8PathBuf};
use pyra_core::AppPaths;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{
    ProjectError,
    sync::{LockArtifact, LockPackage, LockSelection},
};

const STUB_STATE_ENV: &str = "PYRA_SYNC_INSTALLER_STATE_PATH";
// Environment inspection is a read-only installer concern. Querying the
// managed interpreter's stdlib metadata keeps that step independent from pip's
// CLI health while still reflecting the interpreter's installed distributions.
const IMPORTLIB_METADATA_INSPECTION_SCRIPT: &str = r#"
import importlib.metadata
import json
import sys

packages = [
    {
        "name": distribution.metadata["Name"],
        "version": distribution.version,
    }
    for distribution in importlib.metadata.distributions()
]
json.dump(packages, sys.stdout)
"#;

#[derive(Debug, Deserialize)]
struct InspectedDistribution {
    name: String,
    version: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ReconciliationPlanAction {
    Install { name: String, version: String },
    Remove { name: String },
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ReconciliationPlan {
    pub actions: Vec<ReconciliationPlanAction>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ApplyReconciliationOutcome {
    pub installed: usize,
    pub removed: usize,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct EnvironmentInstaller;

impl EnvironmentInstaller {
    pub fn inspect_installed(
        self,
        interpreter: &Utf8Path,
    ) -> Result<BTreeMap<String, String>, ProjectError> {
        if let Ok(state_path) = std::env::var(STUB_STATE_ENV) {
            return read_stub_state(&state_path);
        }

        let output = Command::new(interpreter.as_std_path())
            .args(["-c", IMPORTLIB_METADATA_INSPECTION_SCRIPT])
            .output()
            .map_err(|source| ProjectError::InspectEnvironment {
                interpreter: interpreter.to_string(),
                detail: source.to_string(),
            })?;
        if !output.status.success() {
            return Err(ProjectError::InspectEnvironment {
                interpreter: interpreter.to_string(),
                detail: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }

        parse_inspected_distributions(interpreter, &output.stdout)
    }

    pub async fn apply(
        self,
        paths: &AppPaths,
        interpreter: &Utf8Path,
        project_root: &Utf8Path,
        project_name: &str,
        build_system_present: bool,
        plan: &ReconciliationPlan,
        packages: &[LockPackage],
    ) -> Result<ApplyReconciliationOutcome, ProjectError> {
        if let Ok(state_path) = std::env::var(STUB_STATE_ENV) {
            return apply_stub_state(
                &state_path,
                project_name,
                build_system_present,
                plan,
                packages,
            );
        }

        for action in &plan.actions {
            match action {
                ReconciliationPlanAction::Install { name, version } => {
                    let package = packages
                        .iter()
                        .find(|package| &package.name == name && &package.version == version)
                        .expect("lock package for install action");
                    let artifact = package
                        .wheels
                        .first()
                        .or(package.sdist.as_ref())
                        .expect("lock package should include an artifact");
                    let verified_artifact =
                        prepare_verified_artifact(paths, package, artifact).await?;
                    let output = Command::new(interpreter.as_std_path())
                        .args(["-m", "pip", "install", "--no-deps"])
                        .arg(verified_artifact.as_std_path())
                        .output()
                        .map_err(|source| ProjectError::CreateEnvironment {
                            path: interpreter.to_string(),
                            source,
                        })?;
                    if !output.status.success() {
                        return Err(ProjectError::InstallLockedPackage {
                            package: name.clone(),
                            interpreter: interpreter.to_string(),
                            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
                        });
                    }
                }
                ReconciliationPlanAction::Remove { name } => {
                    let output = Command::new(interpreter.as_std_path())
                        .args(["-m", "pip", "uninstall", "-y", name])
                        .output()
                        .map_err(|source| ProjectError::CreateEnvironment {
                            path: interpreter.to_string(),
                            source,
                        })?;
                    if !output.status.success() {
                        return Err(ProjectError::RemoveLockedPackage {
                            package: name.clone(),
                            interpreter: interpreter.to_string(),
                            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
                        });
                    }
                }
            }
        }

        if build_system_present {
            let output = Command::new(interpreter.as_std_path())
                .args(["-m", "pip", "install", "--no-deps", "-e"])
                .arg(project_root.as_std_path())
                .output()
                .map_err(|source| ProjectError::CreateEnvironment {
                    path: interpreter.to_string(),
                    source,
                })?;
            if !output.status.success() {
                return Err(ProjectError::InstallEditableProject {
                    interpreter: interpreter.to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
                });
            }
        }

        Ok(ApplyReconciliationOutcome {
            installed: plan
                .actions
                .iter()
                .filter(|action| matches!(action, ReconciliationPlanAction::Install { .. }))
                .count(),
            removed: plan
                .actions
                .iter()
                .filter(|action| matches!(action, ReconciliationPlanAction::Remove { .. }))
                .count(),
        })
    }
}

impl ReconciliationPlan {
    pub fn build(
        selected_packages: &[LockPackage],
        installed_packages: &BTreeMap<String, String>,
        protected_packages: &BTreeSet<String>,
        project_name: &str,
        build_system_present: bool,
    ) -> Self {
        let desired = selected_packages
            .iter()
            .map(|package| (package.name.clone(), package.version.clone()))
            .collect::<BTreeMap<_, _>>();

        let mut actions = Vec::new();
        for (name, version) in &desired {
            match installed_packages.get(name) {
                Some(installed_version) if installed_version == version => {}
                _ => actions.push(ReconciliationPlanAction::Install {
                    name: name.clone(),
                    version: version.clone(),
                }),
            }
        }
        for name in installed_packages.keys() {
            if !desired.contains_key(name)
                && !protected_packages.contains(name)
                && (build_system_present || name != project_name)
            {
                actions.push(ReconciliationPlanAction::Remove { name: name.clone() });
            }
        }
        Self { actions }
    }

    pub fn for_selection(packages: &[LockPackage], selection: &LockSelection) -> Vec<LockPackage> {
        packages
            .iter()
            .filter(|package| marker_matches(package.marker.as_ref(), selection))
            .cloned()
            .collect()
    }
}

fn marker_matches(marker: Option<&crate::sync::LockMarker>, selection: &LockSelection) -> bool {
    let Some(marker) = marker else {
        return true;
    };

    marker.matches(selection)
}

fn parse_inspected_distributions(
    interpreter: &Utf8Path,
    stdout: &[u8],
) -> Result<BTreeMap<String, String>, ProjectError> {
    let packages =
        serde_json::from_slice::<Vec<InspectedDistribution>>(stdout).map_err(|error| {
            ProjectError::InspectEnvironment {
                interpreter: interpreter.to_string(),
                detail: format!("invalid importlib.metadata output: {error}"),
            }
        })?;
    Ok(packages
        .into_iter()
        .map(|package| {
            (
                package.name.to_ascii_lowercase().replace('_', "-"),
                package.version,
            )
        })
        .collect())
}

fn read_stub_state(path: &str) -> Result<BTreeMap<String, String>, ProjectError> {
    if !Utf8Path::new(path).exists() {
        return Ok(BTreeMap::new());
    }
    let bytes = fs::read(path).map_err(|source| ProjectError::ReadLockfile {
        path: path.to_string(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|error| ProjectError::ParseLockfile {
        path: path.to_string(),
        detail: error.to_string(),
    })
}

async fn prepare_verified_artifact(
    paths: &AppPaths,
    package: &LockPackage,
    artifact: &LockArtifact,
) -> Result<Utf8PathBuf, ProjectError> {
    ensure_artifact_dir(&paths.package_artifact_cache_dir())?;
    ensure_artifact_dir(&paths.package_artifact_staging_dir())?;

    let artifact_name = normalized_artifact_name(artifact);
    let cached_path = paths.package_artifact_cache_file(&artifact.sha256, &artifact_name);
    if cached_path.exists() && file_matches_sha256(&cached_path, &artifact.sha256)? {
        return Ok(cached_path);
    }
    remove_file_if_exists(&cached_path)?;

    let staged_path = paths.package_artifact_staging_file(&artifact.sha256, &artifact_name);
    remove_file_if_exists(&staged_path)?;

    let bytes = download_artifact_bytes(artifact).await?;
    let actual_sha256 = sha256_hex(&bytes);
    if actual_sha256 != artifact.sha256 {
        return Err(ProjectError::LockedArtifactHashMismatch {
            package: package.name.clone(),
            artifact: artifact.name.clone(),
            expected: artifact.sha256.clone(),
            actual: actual_sha256,
        });
    }

    if let Err(source) = fs::write(staged_path.as_std_path(), &bytes) {
        cleanup_artifact_file(&staged_path)?;
        return Err(ProjectError::WriteLockedArtifact {
            path: staged_path.to_string(),
            source,
        });
    }

    if let Err(source) = fs::rename(staged_path.as_std_path(), cached_path.as_std_path()) {
        cleanup_artifact_file(&staged_path)?;
        return Err(ProjectError::PromoteLockedArtifact {
            from: staged_path.to_string(),
            to: cached_path.to_string(),
            source,
        });
    }

    Ok(cached_path)
}

fn ensure_artifact_dir(path: &Utf8Path) -> Result<(), ProjectError> {
    fs::create_dir_all(path).map_err(|source| ProjectError::PrepareArtifactDirectory {
        path: path.to_string(),
        source,
    })
}

fn normalized_artifact_name(artifact: &LockArtifact) -> String {
    Utf8Path::new(&artifact.name)
        .file_name()
        .unwrap_or(artifact.name.as_str())
        .to_string()
}

async fn download_artifact_bytes(artifact: &LockArtifact) -> Result<Vec<u8>, ProjectError> {
    if let Some(path) = artifact.url.strip_prefix("file://") {
        return fs::read(path).map_err(|source| ProjectError::ReadLockedArtifact {
            path: path.to_string(),
            source,
        });
    }

    let response = reqwest::Client::new()
        .get(&artifact.url)
        .header(reqwest::header::USER_AGENT, "pyra/0.1.0")
        .send()
        .await
        .map_err(|source| ProjectError::DownloadLockedArtifact {
            url: artifact.url.clone(),
            source,
        })?
        .error_for_status()
        .map_err(|source| ProjectError::DownloadLockedArtifact {
            url: artifact.url.clone(),
            source,
        })?;

    let bytes = response
        .bytes()
        .await
        .map_err(|source| ProjectError::DownloadLockedArtifact {
            url: artifact.url.clone(),
            source,
        })?;
    Ok(bytes.to_vec())
}

fn file_matches_sha256(path: &Utf8Path, expected: &str) -> Result<bool, ProjectError> {
    let bytes = fs::read(path).map_err(|source| ProjectError::ReadLockedArtifact {
        path: path.to_string(),
        source,
    })?;
    Ok(sha256_hex(&bytes) == expected)
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn cleanup_artifact_file(path: &Utf8Path) -> Result<(), ProjectError> {
    if path.exists() {
        fs::remove_file(path).map_err(|source| ProjectError::RemoveArtifactFile {
            path: path.to_string(),
            source,
        })?;
    }
    Ok(())
}

fn remove_file_if_exists(path: &Utf8Path) -> Result<(), ProjectError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(ProjectError::RemoveArtifactFile {
            path: path.to_string(),
            source,
        }),
    }
}

fn apply_stub_state(
    path: &str,
    project_name: &str,
    build_system_present: bool,
    plan: &ReconciliationPlan,
    packages: &[LockPackage],
) -> Result<ApplyReconciliationOutcome, ProjectError> {
    let mut state = read_stub_state(path)?;
    for action in &plan.actions {
        match action {
            ReconciliationPlanAction::Install { name, version } => {
                let _ = packages;
                state.insert(name.clone(), version.clone());
            }
            ReconciliationPlanAction::Remove { name } => {
                state.remove(name);
            }
        }
    }
    if build_system_present {
        state.insert(project_name.to_string(), "editable".to_string());
    } else {
        state.remove(project_name);
    }
    fs::write(
        path,
        serde_json::to_vec_pretty(&state).map_err(|error| ProjectError::ParseLockfile {
            path: path.to_string(),
            detail: error.to_string(),
        })?,
    )
    .map_err(|source| ProjectError::WriteLockfile {
        path: path.to_string(),
        source,
    })?;
    Ok(ApplyReconciliationOutcome {
        installed: plan
            .actions
            .iter()
            .filter(|action| matches!(action, ReconciliationPlanAction::Install { .. }))
            .count(),
        removed: plan
            .actions
            .iter()
            .filter(|action| matches!(action, ReconciliationPlanAction::Remove { .. }))
            .count(),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::path::Path;

    use camino::{Utf8Path, Utf8PathBuf};
    use pyra_core::AppPaths;
    use tempfile::tempdir;

    use super::{
        ReconciliationPlan, ReconciliationPlanAction, parse_inspected_distributions,
        prepare_verified_artifact, sha256_hex,
    };
    use crate::ProjectError;
    use crate::sync::{LockArtifact, LockMarker, LockMarkerClause, LockPackage, LockSelection};

    #[test]
    fn builds_exact_reconciliation_plan() {
        let packages = vec![
            package(
                "attrs",
                "25.1.0",
                LockMarker::from_clauses(vec![LockMarkerClause::dependency_group("pyra-default")]),
            ),
            package(
                "pytest",
                "8.3.0",
                LockMarker::from_clauses(vec![LockMarkerClause::dependency_group("dev")]),
            ),
        ];
        let selected = ReconciliationPlan::for_selection(
            &packages,
            &LockSelection {
                groups: ["pyra-default".to_string(), "dev".to_string()]
                    .into_iter()
                    .collect(),
                extras: BTreeSet::new(),
            },
        );
        let installed = BTreeMap::from([
            ("attrs".to_string(), "25.1.0".to_string()),
            ("click".to_string(), "8.1.7".to_string()),
        ]);
        let protected = ["pip".to_string()].into_iter().collect();

        let plan = ReconciliationPlan::build(&selected, &installed, &protected, "example", false);
        assert!(plan.actions.contains(&ReconciliationPlanAction::Install {
            name: "pytest".to_string(),
            version: "8.3.0".to_string()
        }));
        assert!(plan.actions.contains(&ReconciliationPlanAction::Remove {
            name: "click".to_string()
        }));
    }

    #[test]
    fn selects_mixed_group_and_extra_markers() {
        let packages = vec![
            package(
                "attrs",
                "25.1.0",
                LockMarker::from_clauses(vec![LockMarkerClause::dependency_group("pyra-default")]),
            ),
            package(
                "pytest",
                "8.3.0",
                LockMarker::from_clauses(vec![
                    LockMarkerClause::dependency_group("dev"),
                    LockMarkerClause::extra("feature"),
                ]),
            ),
            package(
                "sphinx",
                "7.4.0",
                LockMarker::from_clauses(vec![LockMarkerClause::dependency_group("docs")]),
            ),
            package(
                "rich-extra",
                "1.0.0",
                LockMarker::from_clauses(vec![LockMarkerClause::extra("feature")]),
            ),
        ];

        let selected = ReconciliationPlan::for_selection(
            &packages,
            &LockSelection {
                groups: ["pyra-default".to_string(), "dev".to_string()]
                    .into_iter()
                    .collect(),
                extras: ["feature".to_string()].into_iter().collect(),
            },
        );

        let selected_names = selected
            .iter()
            .map(|package| package.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(selected_names, vec!["attrs", "pytest", "rich-extra"]);
    }

    #[test]
    fn normalizes_inspected_package_names_and_versions() {
        let installed = parse_inspected_distributions(
            Utf8Path::new("/tmp/python"),
            br#"
[
  {"name": "Friendly_Bard", "version": "1.2.3"},
  {"name": "zope.interface", "version": "7.0"}
]
"#,
        )
        .expect("inspection output");

        assert_eq!(
            installed,
            BTreeMap::from([
                ("friendly-bard".to_string(), "1.2.3".to_string()),
                ("zope.interface".to_string(), "7.0".to_string()),
            ])
        );
    }

    #[test]
    fn rejects_malformed_inspection_output() {
        let error = parse_inspected_distributions(Utf8Path::new("/tmp/python"), b"{not json}")
            .expect_err("malformed inspection output should fail");

        assert!(matches!(
            error,
            ProjectError::InspectEnvironment {
                ref interpreter,
                ref detail,
            } if interpreter == "/tmp/python"
                && detail.contains("invalid importlib.metadata output")
        ));
    }

    #[tokio::test]
    async fn stages_verified_artifact_with_matching_hash() {
        let temp = tempdir().expect("temporary directory");
        let source_path = temp.path().join("attrs-25.1.0-py3-none-any.whl");
        let bytes = b"attrs wheel fixture bytes";
        fs::write(&source_path, bytes).expect("artifact bytes");
        let paths = test_paths(temp.path());
        let artifact = artifact(
            source_path.as_path(),
            sha256_hex(bytes),
            "attrs-25.1.0-py3-none-any.whl",
        );
        let package = package_with_artifact("attrs", artifact.clone());

        let prepared = prepare_verified_artifact(&paths, &package, &artifact)
            .await
            .expect("verified artifact");

        assert_eq!(
            prepared,
            paths.package_artifact_cache_file(&artifact.sha256, &artifact.name)
        );
        assert_eq!(
            fs::read(prepared.as_std_path()).expect("cached artifact"),
            bytes
        );
        assert!(staging_dir_entries(&paths).is_empty());
    }

    #[tokio::test]
    async fn rejects_artifact_with_mismatched_hash_before_install() {
        let temp = tempdir().expect("temporary directory");
        let source_path = temp.path().join("attrs-25.1.0-py3-none-any.whl");
        fs::write(&source_path, b"attrs wheel fixture bytes").expect("artifact bytes");
        let paths = test_paths(temp.path());
        let artifact = artifact(
            source_path.as_path(),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
            "attrs-25.1.0-py3-none-any.whl",
        );
        let package = package_with_artifact("attrs", artifact.clone());

        let error = prepare_verified_artifact(&paths, &package, &artifact)
            .await
            .expect_err("hash mismatch should fail");

        assert!(matches!(
            error,
            ProjectError::LockedArtifactHashMismatch { ref package, .. } if package == "attrs"
        ));
        assert!(staging_dir_entries(&paths).is_empty());
    }

    #[tokio::test]
    async fn fails_when_artifact_download_source_is_missing() {
        let temp = tempdir().expect("temporary directory");
        let missing_path = temp.path().join("missing.whl");
        let paths = test_paths(temp.path());
        let artifact = artifact(
            missing_path.as_path(),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            "missing.whl",
        );
        let package = package_with_artifact("attrs", artifact.clone());

        let error = prepare_verified_artifact(&paths, &package, &artifact)
            .await
            .expect_err("missing artifact should fail");

        assert!(matches!(
            error,
            ProjectError::ReadLockedArtifact { ref path, .. }
                if path == missing_path.to_string_lossy().as_ref()
        ));
        assert!(staging_dir_entries(&paths).is_empty());
    }

    fn package(name: &str, version: &str, marker: Option<LockMarker>) -> LockPackage {
        LockPackage {
            name: name.to_string(),
            version: version.to_string(),
            marker,
            requires_python: None,
            index: None,
            dependencies: Vec::new(),
            sdist: None,
            wheels: Vec::new(),
        }
    }

    fn package_with_artifact(name: &str, artifact: LockArtifact) -> LockPackage {
        let mut package = package(name, "25.1.0", None);
        package.wheels = vec![artifact];
        package
    }

    fn artifact(path: &Path, sha256: String, name: &str) -> LockArtifact {
        LockArtifact {
            name: name.to_string(),
            url: format!("file://{}", path.display()),
            size: None,
            upload_time: None,
            sha256,
        }
    }

    fn test_paths(root: &Path) -> AppPaths {
        let root = Utf8PathBuf::from_path_buf(root.to_path_buf()).expect("utf-8 temp dir");
        let paths = AppPaths::from_roots(
            root.join("config"),
            root.join("data"),
            root.join("cache"),
            root.join("state"),
        );
        paths.ensure_base_layout().expect("base layout");
        paths
    }

    fn staging_dir_entries(paths: &AppPaths) -> Vec<String> {
        fs::read_dir(paths.package_artifact_staging_dir().as_std_path())
            .expect("staging dir")
            .map(|entry| {
                entry
                    .expect("staging entry")
                    .file_name()
                    .to_string_lossy()
                    .to_string()
            })
            .collect()
    }
}
