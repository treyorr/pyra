//! Exact reconciliation planning and installer backends.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::process::Command;

use camino::{Utf8Path, Utf8PathBuf};
use pyra_core::AppPaths;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::task::JoinSet;

use crate::{
    ProjectError,
    sync::{LockArtifact, LockPackage, LockSelection},
};

const STUB_STATE_ENV: &str = "PYRA_SYNC_INSTALLER_STATE_PATH";
// Artifact downloads are network- and IO-bound, but the installer still keeps
// a small cap here so preparation never turns into unbounded fan-out.
const MAX_PARALLEL_ARTIFACT_PREPARATIONS: usize = 4;
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

#[derive(Debug, Clone)]
struct PendingArtifactPreparation {
    action_index: usize,
    package: LockPackage,
    artifact: LockArtifact,
}

#[derive(Debug)]
struct PreparedArtifact {
    action_index: usize,
    path: Utf8PathBuf,
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

        // Artifact preparation can overlap, but the later apply loop still
        // walks one deterministic reconciliation plan.
        let prepared_artifacts = prepare_install_artifacts(paths, plan, packages).await?;

        for (action_index, action) in plan.actions.iter().enumerate() {
            match action {
                ReconciliationPlanAction::Install { name, version: _ } => {
                    let verified_artifact = prepared_artifacts
                        .get(&action_index)
                        .expect("prepared artifact for install action");
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
        // Reconciliation planning is part of Pyra's stable sync model, so the
        // desired package set is normalized into sorted maps before actions are
        // emitted. That keeps apply order deterministic across runs.
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
            .filter_map(|package| package_for_current_host(package, selection))
            .collect()
    }
}

fn marker_matches(marker: Option<&crate::sync::LockMarker>, selection: &LockSelection) -> bool {
    let Some(marker) = marker else {
        return true;
    };

    marker.matches(selection)
}

fn package_for_current_host(
    package: &LockPackage,
    selection: &LockSelection,
) -> Option<LockPackage> {
    // Multi-target locks may carry wheels for several environments. Narrowing
    // to the current host here keeps reconciliation exact without leaking
    // foreign-target artifacts into later installer steps.
    let compatible_wheels = package
        .wheels
        .iter()
        .filter(|artifact| wheel_compatible_for_selection(&artifact.name, selection))
        .cloned()
        .collect::<Vec<_>>();

    if compatible_wheels.is_empty() && package.sdist.is_none() {
        return None;
    }

    let mut package = package.clone();
    package.wheels = compatible_wheels;
    Some(package)
}

fn wheel_compatible_for_selection(filename: &str, selection: &LockSelection) -> bool {
    let Some(stem) = filename.strip_suffix(".whl") else {
        return false;
    };
    let parts = stem.split('-').collect::<Vec<_>>();
    if parts.len() < 5 {
        return false;
    }

    let py_tag = parts[parts.len() - 3];
    let abi_tag = parts[parts.len() - 2];
    let platform_tag = parts[parts.len() - 1];

    python_tag_compatible(py_tag, selection)
        && abi_tag_compatible(abi_tag, selection)
        && platform_tag_compatible(platform_tag, selection)
}

fn python_tag_compatible(tag: &str, selection: &LockSelection) -> bool {
    let Some(exact) = exact_python_tag(selection) else {
        return false;
    };

    tag.split('.').any(|candidate| {
        candidate == "py3"
            || candidate == "py2.py3"
            || candidate == "py3-none"
            || candidate == exact
    })
}

fn abi_tag_compatible(tag: &str, selection: &LockSelection) -> bool {
    let Some(exact) = exact_python_tag(selection) else {
        return false;
    };

    tag.split('.')
        .any(|candidate| candidate == "none" || candidate == "abi3" || candidate == exact)
}

fn exact_python_tag(selection: &LockSelection) -> Option<String> {
    let mut release = selection.python_full_version.split('.');
    let major = release.next()?;
    let minor = release.next()?;
    Some(format!("cp{major}{minor}"))
}

fn platform_tag_compatible(tag: &str, selection: &LockSelection) -> bool {
    if tag == "any" {
        return true;
    }

    match selection.target_triple.as_str() {
        "aarch64-apple-darwin" => tag.contains("macosx") && tag.contains("arm64"),
        "x86_64-apple-darwin" => tag.contains("macosx") && tag.contains("x86_64"),
        "x86_64-unknown-linux-gnu" => {
            (tag.contains("manylinux") || tag.contains("linux")) && tag.contains("x86_64")
        }
        "aarch64-unknown-linux-gnu" => {
            (tag.contains("manylinux") || tag.contains("linux")) && tag.contains("aarch64")
        }
        _ => false,
    }
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

async fn prepare_install_artifacts(
    paths: &AppPaths,
    plan: &ReconciliationPlan,
    packages: &[LockPackage],
) -> Result<BTreeMap<usize, Utf8PathBuf>, ProjectError> {
    let pending = pending_artifact_preparations(plan, packages);
    if pending.is_empty() {
        return Ok(BTreeMap::new());
    }

    let concurrency = pending.len().min(MAX_PARALLEL_ARTIFACT_PREPARATIONS).max(1);
    let mut next_pending = pending.into_iter();
    let mut join_set: JoinSet<Result<PreparedArtifact, ProjectError>> = JoinSet::new();

    for _ in 0..concurrency {
        if let Some(pending) = next_pending.next() {
            spawn_artifact_preparation(&mut join_set, paths.clone(), pending);
        }
    }

    let mut prepared = BTreeMap::new();
    while let Some(result) = join_set.join_next().await {
        let prepared_artifact = match result {
            Ok(Ok(prepared_artifact)) => prepared_artifact,
            Ok(Err(error)) => {
                cancel_pending_preparations(&mut join_set).await;
                return Err(error);
            }
            Err(error) => {
                cancel_pending_preparations(&mut join_set).await;
                return Err(ProjectError::ArtifactPreparationTask {
                    detail: error.to_string(),
                });
            }
        };
        prepared.insert(prepared_artifact.action_index, prepared_artifact.path);

        if let Some(pending) = next_pending.next() {
            spawn_artifact_preparation(&mut join_set, paths.clone(), pending);
        }
    }

    Ok(prepared)
}

fn pending_artifact_preparations(
    plan: &ReconciliationPlan,
    packages: &[LockPackage],
) -> Vec<PendingArtifactPreparation> {
    plan.actions
        .iter()
        .enumerate()
        .filter_map(|(action_index, action)| match action {
            ReconciliationPlanAction::Install { name, version } => {
                let package = packages
                    .iter()
                    .find(|package| &package.name == name && &package.version == version)
                    .expect("lock package for install action");
                Some(PendingArtifactPreparation {
                    action_index,
                    package: package.clone(),
                    artifact: selected_artifact(package).clone(),
                })
            }
            ReconciliationPlanAction::Remove { .. } => None,
        })
        .collect()
}

fn selected_artifact(package: &LockPackage) -> &LockArtifact {
    package
        .wheels
        .first()
        .or(package.sdist.as_ref())
        .expect("lock package should include an artifact")
}

fn spawn_artifact_preparation(
    join_set: &mut JoinSet<Result<PreparedArtifact, ProjectError>>,
    paths: AppPaths,
    pending: PendingArtifactPreparation,
) {
    join_set.spawn(async move {
        let prepared_path =
            prepare_verified_artifact(&paths, &pending.package, &pending.artifact).await?;
        Ok(PreparedArtifact {
            action_index: pending.action_index,
            path: prepared_path,
        })
    });
}

async fn cancel_pending_preparations(
    join_set: &mut JoinSet<Result<PreparedArtifact, ProjectError>>,
) {
    join_set.abort_all();
    while join_set.join_next().await.is_some() {}
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
    let cached_parent = cached_path
        .parent()
        .expect("artifact cache file should have a parent");
    ensure_artifact_dir(cached_parent)?;
    // The persistent artifact cache is purely a performance layer. Even cache
    // hits are re-hashed against the lock so stale or corrupted entries are
    // discarded instead of silently trusted.
    if cached_path.exists() && file_matches_sha256(&cached_path, &artifact.sha256)? {
        return Ok(cached_path);
    }
    remove_file_if_exists(&cached_path)?;

    let staged_path = paths.package_artifact_staging_file(&artifact.sha256, &artifact_name);
    let staged_parent = staged_path
        .parent()
        .expect("artifact staging file should have a parent");
    ensure_artifact_dir(staged_parent)?;
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
    #[cfg(test)]
    test_support::before_download(&artifact.url).await?;

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

#[cfg(test)]
mod test_support {
    use std::collections::BTreeSet;
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;

    use tokio::time::sleep;

    use crate::ProjectError;

    #[derive(Debug, Default, Clone, Eq, PartialEq)]
    pub(super) struct DownloadHookSnapshot {
        pub max_in_flight: usize,
        pub started_urls: Vec<String>,
    }

    #[derive(Debug, Default)]
    struct DownloadHookState {
        enabled: AtomicBool,
        delay_ms: AtomicU64,
        in_flight: AtomicUsize,
        max_in_flight: AtomicUsize,
        delayed_urls: Mutex<BTreeSet<String>>,
        failed_urls: Mutex<BTreeSet<String>>,
        started_urls: Mutex<Vec<String>>,
    }

    static DOWNLOAD_HOOK: OnceLock<DownloadHookState> = OnceLock::new();
    static DOWNLOAD_HOOK_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn hook() -> &'static DownloadHookState {
        DOWNLOAD_HOOK.get_or_init(DownloadHookState::default)
    }

    fn hook_lock() -> &'static Mutex<()> {
        DOWNLOAD_HOOK_LOCK.get_or_init(|| Mutex::new(()))
    }

    pub(super) struct DownloadHookGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl Drop for DownloadHookGuard {
        fn drop(&mut self) {
            reset();
        }
    }

    struct InFlightGuard<'a> {
        hook: &'a DownloadHookState,
    }

    impl Drop for InFlightGuard<'_> {
        fn drop(&mut self) {
            self.hook.in_flight.fetch_sub(1, Ordering::SeqCst);
        }
    }

    pub(super) fn install_download_hook<I, J>(
        delay_ms: u64,
        delayed_urls: I,
        failed_urls: J,
    ) -> DownloadHookGuard
    where
        I: IntoIterator<Item = String>,
        J: IntoIterator<Item = String>,
    {
        let lock = hook_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        reset();

        let hook = hook();
        hook.enabled.store(true, Ordering::SeqCst);
        hook.delay_ms.store(delay_ms, Ordering::SeqCst);
        *hook.delayed_urls.lock().expect("download hook urls") = delayed_urls.into_iter().collect();
        *hook.failed_urls.lock().expect("download hook urls") = failed_urls.into_iter().collect();

        DownloadHookGuard { _lock: lock }
    }

    pub(super) async fn before_download(url: &str) -> Result<(), ProjectError> {
        let hook = hook();
        if !hook.enabled.load(Ordering::SeqCst) {
            return Ok(());
        }

        let delay_ms = hook.delay_ms.load(Ordering::SeqCst);
        let (should_track, should_delay, should_fail) = {
            let delayed_urls = hook.delayed_urls.lock().expect("download hook urls");
            let failed_urls = hook.failed_urls.lock().expect("download hook urls");
            // When the hook declares explicit URL sets, ignore unrelated downloads
            // from other tests that might run in parallel.
            let unscoped = delayed_urls.is_empty() && failed_urls.is_empty();
            let listed = delayed_urls.contains(url) || failed_urls.contains(url);
            let should_track = unscoped || listed;
            let should_delay =
                should_track && delay_ms > 0 && (unscoped || delayed_urls.contains(url));
            let should_fail = should_track && failed_urls.contains(url);
            (should_track, should_delay, should_fail)
        };
        if !should_track {
            return Ok(());
        }

        hook.started_urls
            .lock()
            .expect("download hook urls")
            .push(url.to_string());

        let current = hook.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        let _in_flight = InFlightGuard { hook };
        update_max_in_flight(hook, current);

        if should_fail {
            return Err(ProjectError::ReadLockedArtifact {
                path: url.to_string(),
                source: std::io::Error::other("test hook forced download failure"),
            });
        }

        if should_delay {
            sleep(Duration::from_millis(delay_ms)).await;
        }

        Ok(())
    }

    pub(super) fn snapshot() -> DownloadHookSnapshot {
        let hook = hook();
        DownloadHookSnapshot {
            max_in_flight: hook.max_in_flight.load(Ordering::SeqCst),
            started_urls: hook
                .started_urls
                .lock()
                .expect("download hook urls")
                .clone(),
        }
    }

    fn update_max_in_flight(hook: &DownloadHookState, current: usize) {
        let mut max = hook.max_in_flight.load(Ordering::SeqCst);
        while current > max {
            match hook.max_in_flight.compare_exchange(
                max,
                current,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(observed) => max = observed,
            }
        }
    }

    fn reset() {
        let hook = hook();
        hook.enabled.store(false, Ordering::SeqCst);
        hook.delay_ms.store(0, Ordering::SeqCst);
        hook.in_flight.store(0, Ordering::SeqCst);
        hook.max_in_flight.store(0, Ordering::SeqCst);
        hook.delayed_urls
            .lock()
            .expect("download hook urls")
            .clear();
        hook.failed_urls.lock().expect("download hook urls").clear();
        hook.started_urls
            .lock()
            .expect("download hook urls")
            .clear();
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
        MAX_PARALLEL_ARTIFACT_PREPARATIONS, ReconciliationPlan, ReconciliationPlanAction,
        parse_inspected_distributions, prepare_install_artifacts, prepare_verified_artifact,
        selected_artifact, sha256_hex, test_support,
    };
    use crate::ProjectError;
    use crate::sync::{LockArtifact, LockMarker, LockMarkerClause, LockPackage, LockSelection};

    #[test]
    fn builds_exact_reconciliation_plan() {
        let mut attrs = package_with_named_artifact(
            "attrs",
            "25.1.0",
            artifact_from_name(
                "attrs-25.1.0-py3-none-any.whl",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ),
        );
        attrs.marker =
            LockMarker::from_clauses(vec![LockMarkerClause::dependency_group("pyra-default")]);

        let mut pytest = package_with_named_artifact(
            "pytest",
            "8.3.0",
            artifact_from_name(
                "pytest-8.3.0-py3-none-any.whl",
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            ),
        );
        pytest.marker = LockMarker::from_clauses(vec![LockMarkerClause::dependency_group("dev")]);

        let packages = vec![attrs, pytest];
        let selected = ReconciliationPlan::for_selection(
            &packages,
            &LockSelection {
                groups: ["pyra-default".to_string(), "dev".to_string()]
                    .into_iter()
                    .collect(),
                extras: BTreeSet::new(),
                python_full_version: "3.13.12".to_string(),
                target_triple: "aarch64-apple-darwin".to_string(),
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
        let mut attrs = package_with_named_artifact(
            "attrs",
            "25.1.0",
            artifact_from_name(
                "attrs-25.1.0-py3-none-any.whl",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ),
        );
        attrs.marker =
            LockMarker::from_clauses(vec![LockMarkerClause::dependency_group("pyra-default")]);

        let mut pytest = package_with_named_artifact(
            "pytest",
            "8.3.0",
            artifact_from_name(
                "pytest-8.3.0-py3-none-any.whl",
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            ),
        );
        pytest.marker = LockMarker::from_clauses(vec![
            LockMarkerClause::dependency_group("dev"),
            LockMarkerClause::extra("feature"),
        ]);

        let mut sphinx = package_with_named_artifact(
            "sphinx",
            "7.4.0",
            artifact_from_name(
                "sphinx-7.4.0-py3-none-any.whl",
                "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
            ),
        );
        sphinx.marker =
            LockMarker::from_clauses(vec![LockMarkerClause::dependency_group("docs")]);

        let mut rich_extra = package_with_named_artifact(
            "rich-extra",
            "1.0.0",
            artifact_from_name(
                "rich_extra-1.0.0-py3-none-any.whl",
                "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
            ),
        );
        rich_extra.marker = LockMarker::from_clauses(vec![LockMarkerClause::extra("feature")]);

        let packages = vec![attrs, pytest, sphinx, rich_extra];

        let selected = ReconciliationPlan::for_selection(
            &packages,
            &LockSelection {
                groups: ["pyra-default".to_string(), "dev".to_string()]
                    .into_iter()
                    .collect(),
                extras: ["feature".to_string()].into_iter().collect(),
                python_full_version: "3.13.12".to_string(),
                target_triple: "aarch64-apple-darwin".to_string(),
            },
        );

        let selected_names = selected
            .iter()
            .map(|package| package.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(selected_names, vec!["attrs", "pytest", "rich-extra"]);
    }

    #[test]
    fn ignores_foreign_target_packages_and_artifacts_for_the_current_host() {
        let packages = vec![
            package_with_named_artifacts(
                "shared",
                "1.0.0",
                vec![
                    artifact_from_name(
                        "shared-1.0.0-cp313-abi3-macosx_11_0_arm64.whl",
                        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    ),
                    artifact_from_name(
                        "shared-1.0.0-cp313-abi3-manylinux_2_17_x86_64.whl",
                        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                    ),
                ],
                None,
            ),
            package_with_named_artifacts(
                "linux-only",
                "2.0.0",
                vec![artifact_from_name(
                    "linux-only-2.0.0-cp313-abi3-manylinux_2_17_x86_64.whl",
                    "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                )],
                None,
            ),
            package_with_named_artifacts(
                "sdist-fallback",
                "3.0.0",
                Vec::new(),
                Some(artifact_from_name(
                    "sdist-fallback-3.0.0.tar.gz",
                    "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
                )),
            ),
        ];

        let selected = ReconciliationPlan::for_selection(
            &packages,
            &LockSelection {
                groups: ["pyra-default".to_string()].into_iter().collect(),
                extras: BTreeSet::new(),
                python_full_version: "3.13.12".to_string(),
                target_triple: "aarch64-apple-darwin".to_string(),
            },
        );

        assert_eq!(
            selected
                .iter()
                .map(|package| package.name.as_str())
                .collect::<Vec<_>>(),
            vec!["shared", "sdist-fallback"]
        );
        assert_eq!(
            selected[0]
                .wheels
                .iter()
                .map(|wheel| wheel.name.as_str())
                .collect::<Vec<_>>(),
            vec!["shared-1.0.0-cp313-abi3-macosx_11_0_arm64.whl"]
        );
        assert!(selected[1].wheels.is_empty());
        assert_eq!(
            selected[1]
                .sdist
                .as_ref()
                .map(|artifact| artifact.name.as_str()),
            Some("sdist-fallback-3.0.0.tar.gz")
        );
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
    async fn prepares_artifacts_with_bounded_concurrency() {
        let temp = tempdir().expect("temporary directory");
        let paths = test_paths(temp.path());

        let mut packages = Vec::new();
        let mut delayed_urls = Vec::new();
        let mut actions = Vec::new();

        for index in 0..(MAX_PARALLEL_ARTIFACT_PREPARATIONS + 2) {
            let name = format!("pkg-{index}");
            let version = format!("1.0.{index}");
            let source_path = temp
                .path()
                .join(format!("{name}-{version}-py3-none-any.whl"));
            let bytes = format!("{name} fixture bytes").into_bytes();
            fs::write(&source_path, &bytes).expect("artifact bytes");

            let artifact = artifact(
                source_path.as_path(),
                sha256_hex(&bytes),
                &format!("{name}-{version}-py3-none-any.whl"),
            );
            delayed_urls.push(artifact.url.clone());
            packages.push(package_with_named_artifact(&name, &version, artifact));
            actions.push(ReconciliationPlanAction::Install { name, version });
        }

        let _hook = test_support::install_download_hook(50, delayed_urls, Vec::new());
        let prepared =
            prepare_install_artifacts(&paths, &ReconciliationPlan { actions }, &packages)
                .await
                .expect("all artifacts should prepare successfully");
        let snapshot = test_support::snapshot();

        assert_eq!(
            snapshot.max_in_flight, MAX_PARALLEL_ARTIFACT_PREPARATIONS,
            "preparation should never exceed the installer concurrency bound"
        );
        assert_eq!(prepared.len(), packages.len());
        assert_eq!(snapshot.started_urls.len(), packages.len());
        assert!(staging_dir_entries(&paths).is_empty());
    }

    #[tokio::test]
    async fn cancels_parallel_preparation_after_first_failure() {
        let temp = tempdir().expect("temporary directory");
        let paths = test_paths(temp.path());
        let failing_path = temp.path().join("failing-1.0.0.whl");
        let failing_bytes = b"failing fixture bytes";
        fs::write(&failing_path, failing_bytes).expect("artifact bytes");
        let failing_artifact = artifact(
            failing_path.as_path(),
            sha256_hex(failing_bytes),
            "failing-1.0.0.whl",
        );

        let mut packages = vec![package_with_named_artifact(
            "failing",
            "1.0.0",
            failing_artifact.clone(),
        )];
        let mut actions = vec![ReconciliationPlanAction::Install {
            name: "failing".to_string(),
            version: "1.0.0".to_string(),
        }];
        let mut delayed_urls = Vec::new();

        for index in 0..MAX_PARALLEL_ARTIFACT_PREPARATIONS {
            let name = format!("slow-{index}");
            let version = "2.0.0".to_string();
            let source_path = temp
                .path()
                .join(format!("{name}-{version}-py3-none-any.whl"));
            let bytes = format!("{name} fixture bytes").into_bytes();
            fs::write(&source_path, &bytes).expect("artifact bytes");

            let artifact = artifact(
                source_path.as_path(),
                sha256_hex(&bytes),
                &format!("{name}-{version}-py3-none-any.whl"),
            );
            delayed_urls.push(artifact.url.clone());
            packages.push(package_with_named_artifact(
                &name,
                &version,
                artifact.clone(),
            ));
            actions.push(ReconciliationPlanAction::Install {
                name: name.clone(),
                version: version.clone(),
            });
        }

        let never_started_name = "slow-never-started";
        let never_started_version = "2.0.0";
        let never_started_path = temp.path().join(format!(
            "{never_started_name}-{never_started_version}-py3-none-any.whl"
        ));
        let never_started_bytes = b"never-started fixture bytes";
        fs::write(&never_started_path, never_started_bytes).expect("artifact bytes");
        let never_started_artifact = artifact(
            never_started_path.as_path(),
            sha256_hex(never_started_bytes),
            &format!("{never_started_name}-{never_started_version}-py3-none-any.whl"),
        );
        let never_started_url = never_started_artifact.url.clone();
        packages.push(package_with_named_artifact(
            never_started_name,
            never_started_version,
            never_started_artifact.clone(),
        ));
        actions.push(ReconciliationPlanAction::Install {
            name: never_started_name.to_string(),
            version: never_started_version.to_string(),
        });

        let _hook =
            test_support::install_download_hook(200, delayed_urls, [failing_artifact.url.clone()]);
        let error = prepare_install_artifacts(&paths, &ReconciliationPlan { actions }, &packages)
            .await
            .expect_err("preparation should fail once the forced download failure is encountered");
        let snapshot = test_support::snapshot();

        assert!(matches!(
            error,
            ProjectError::ReadLockedArtifact { ref path, .. }
                if path == &failing_artifact.url
        ));
        assert!(
            snapshot.max_in_flight <= MAX_PARALLEL_ARTIFACT_PREPARATIONS,
            "cancellation should never exceed the preparation bound"
        );
        assert!(
            snapshot.max_in_flight >= 2,
            "the test should observe overlapping preparation before cancellation"
        );
        assert!(
            !snapshot
                .started_urls
                .iter()
                .any(|url| url == &never_started_url),
            "later install actions should not start once preparation fails"
        );
        for package in packages.iter().skip(1) {
            let artifact = selected_artifact(package);
            let cached_path = paths.package_artifact_cache_file(&artifact.sha256, &artifact.name);
            assert!(
                !cached_path.exists(),
                "canceled preparation should not leave cached artifacts behind"
            );
        }
        assert!(staging_dir_entries(&paths).is_empty());
    }

    #[test]
    fn builds_deterministic_action_order() {
        let selected = vec![
            package("beta", "2.0.0", None),
            package("alpha", "1.0.0", None),
            package("gamma", "3.0.0", None),
        ];
        let installed = BTreeMap::from([
            ("delta".to_string(), "4.0.0".to_string()),
            ("beta".to_string(), "1.5.0".to_string()),
            ("gamma".to_string(), "3.0.0".to_string()),
        ]);
        let protected = BTreeSet::new();

        let plan = ReconciliationPlan::build(&selected, &installed, &protected, "example", false);

        assert_eq!(
            plan.actions,
            vec![
                ReconciliationPlanAction::Install {
                    name: "alpha".to_string(),
                    version: "1.0.0".to_string(),
                },
                ReconciliationPlanAction::Install {
                    name: "beta".to_string(),
                    version: "2.0.0".to_string(),
                },
                ReconciliationPlanAction::Remove {
                    name: "delta".to_string(),
                },
            ]
        );
    }

    #[tokio::test]
    async fn caches_verified_artifact_on_cache_miss() {
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
    async fn reuses_verified_artifact_on_cache_hit_without_source_download() {
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

        let first = prepare_verified_artifact(&paths, &package, &artifact)
            .await
            .expect("first verified artifact");
        fs::remove_file(&source_path).expect("remove original source");

        let second = prepare_verified_artifact(&paths, &package, &artifact)
            .await
            .expect("cache hit should avoid source download");

        assert_eq!(first, second);
        assert_eq!(
            fs::read(second.as_std_path()).expect("cached artifact"),
            bytes
        );
        assert!(staging_dir_entries(&paths).is_empty());
    }

    #[tokio::test]
    async fn discards_corrupted_cached_artifact_and_recaches_verified_bytes() {
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
        let cached_path = paths.package_artifact_cache_file(&artifact.sha256, &artifact.name);
        let cached_parent = cached_path.parent().expect("cached artifact parent");
        fs::create_dir_all(cached_parent).expect("cache parent");
        fs::write(&cached_path, b"corrupted bytes").expect("corrupted cache contents");

        let prepared = prepare_verified_artifact(&paths, &package, &artifact)
            .await
            .expect("corrupted cache entry should be replaced");

        assert_eq!(prepared, cached_path);
        assert_eq!(
            fs::read(prepared.as_std_path()).expect("recached artifact"),
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
        package_with_named_artifact(name, "25.1.0", artifact)
    }

    fn package_with_named_artifact(
        name: &str,
        version: &str,
        artifact: LockArtifact,
    ) -> LockPackage {
        let mut package = package(name, version, None);
        package.wheels = vec![artifact];
        package
    }

    fn package_with_named_artifacts(
        name: &str,
        version: &str,
        wheels: Vec<LockArtifact>,
        sdist: Option<LockArtifact>,
    ) -> LockPackage {
        let mut package = package(name, version, None);
        package.wheels = wheels;
        package.sdist = sdist;
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

    fn artifact_from_name(name: &str, sha256: &str) -> LockArtifact {
        LockArtifact {
            name: name.to_string(),
            url: format!("https://example.test/{name}"),
            size: None,
            upload_time: None,
            sha256: sha256.to_string(),
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
        let mut entries = Vec::new();
        let mut pending = vec![paths.package_artifact_staging_dir()];

        while let Some(dir) = pending.pop() {
            for entry in fs::read_dir(dir.as_std_path()).expect("staging dir") {
                let entry = entry.expect("staging entry");
                let path = Utf8PathBuf::from_path_buf(entry.path()).expect("utf-8 staging path");
                if path.is_dir() {
                    pending.push(path);
                    continue;
                }

                entries.push(path.to_string());
            }
        }

        entries
    }
}
