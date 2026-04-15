use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use assert_cmd::Command;
use predicates::str::contains;
use pyra_python::{ArchiveFormat, HostTarget, InstalledPythonRecord, PythonVersion};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

#[test]
fn sync_installs_base_and_dev_by_default() {
    let home = temp_env_root();
    let index = start_fixture_index();
    let project_root = home.path().join("workspace").join("sample-sync-default");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-sync-default"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["attrs==25.1.0"]

[dependency-groups]
dev = ["pytest==8.3.0"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["sync"])
        .assert()
        .success()
        .stdout(contains("Synced"))
        .stdout(contains("Python 3.13.12"));

    let state = read_state(&state_path);
    assert_eq!(state.get("attrs"), Some(&"25.1.0".to_string()));
    assert_eq!(state.get("pytest"), Some(&"8.3.0".to_string()));
    assert_eq!(state.get("pluggy"), Some(&"1.5.0".to_string()));

    let lock = fs::read_to_string(project_root.join("pylock.toml")).expect("pylock");
    assert!(lock.contains("default-groups = [\"pyra-default\", \"dev\"]"));
    assert!(lock.contains("dependency-groups = [\"dev\"]"));
}

#[test]
fn sync_json_contract_snapshot_for_default_sync() {
    let home = temp_env_root();
    let project_root = home.path().join("workspace").join("sample-sync-json");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-sync-json"
version = "0.1.0"
dependencies = []

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    let output = base_command(&home, &state_path)
        .current_dir(&project_root)
        .args(["--json", "sync"])
        .output()
        .expect("sync json output");

    let project_id = project_id_for(&project_root);
    assert_json_contract(
        &output,
        json!({
            "status": "success",
            "exit": {
                "code": 0,
                "category": "success"
            },
            "output": {
                "blocks": [
                    {
                        "type": "message",
                        "value": {
                            "tone": "success",
                            "summary": format!("Synced `{project_id}` with Python 3.13.12."),
                            "detail": "Updated `pylock.toml` and reconciled the centralized environment.",
                            "hint": Value::Null,
                            "verbose": []
                        }
                    }
                ]
            },
            "error": Value::Null
        }),
    );
}

#[test]
fn sync_only_group_excludes_base_dependencies() {
    let home = temp_env_root();
    let index = start_fixture_index();
    let project_root = home.path().join("workspace").join("sample-sync-group");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-sync-group"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["attrs==25.1.0"]

[dependency-groups]
docs = ["sphinx==7.0.0"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["sync", "--only-group", "docs"])
        .assert()
        .success();

    let state = read_state(&state_path);
    assert!(!state.contains_key("attrs"));
    assert_eq!(state.get("sphinx"), Some(&"7.0.0".to_string()));
}

#[test]
fn sync_reuses_current_lock_and_removes_extraneous_packages() {
    let home = temp_env_root();
    let index = start_fixture_index();
    let project_root = home.path().join("workspace").join("sample-sync-reuse");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-sync-reuse"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["attrs==25.1.0"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");
    fs::write(
        &state_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "attrs": "24.0.0",
            "orphan": "1.0.0"
        }))
        .unwrap(),
    )
    .expect("state");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["sync"])
        .assert()
        .success();
    let first_lock = fs::read_to_string(project_root.join("pylock.toml")).expect("pylock");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["sync"])
        .assert()
        .success()
        .stdout(contains("Reused the current lock"));

    let second_lock = fs::read_to_string(project_root.join("pylock.toml")).expect("pylock");
    assert_eq!(first_lock, second_lock);

    let state = read_state(&state_path);
    assert_eq!(state.get("attrs"), Some(&"25.1.0".to_string()));
    assert!(!state.contains_key("orphan"));
}

#[test]
fn sync_reuses_empty_lock_without_parse_failure() {
    let home = temp_env_root();
    let project_root = home
        .path()
        .join("workspace")
        .join("sample-sync-empty-lock-reuse");
    fs::create_dir_all(&project_root).expect("project root");
    let python_version = system_python_version().expect("system python version");
    fs::write(
        project_root.join("pyproject.toml"),
        format!(
            r#"[project]
name = "sample-sync-empty-lock-reuse"
version = "0.1.0"
dependencies = []

[tool.pyra]
python = "{python_version}"
"#,
        ),
    )
    .expect("pyproject");

    let managed_env = home.path().join("managed-python");
    create_virtualenv(&system_python().expect("system python"), &managed_env)
        .expect("managed virtualenv");
    let managed_python = venv_python_path(&managed_env);
    seed_managed_install_with_executable(&home, &python_version, &managed_python)
        .expect("managed install");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .args(["sync"])
        .assert()
        .success()
        .stdout(contains("Updated `pylock.toml`"));

    let first_lock = fs::read_to_string(project_root.join("pylock.toml")).expect("pylock");
    assert!(!first_lock.contains("[[packages]]"));

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .args(["sync"])
        .assert()
        .success()
        .stdout(contains("Reused the current lock"));

    let second_lock = fs::read_to_string(project_root.join("pylock.toml")).expect("pylock");
    assert_eq!(first_lock, second_lock);
}

#[test]
fn sync_inspects_environment_without_pip_list() {
    let home = temp_env_root();
    let project_root = home
        .path()
        .join("workspace")
        .join("sample-sync-importlib-inspection");
    fs::create_dir_all(&project_root).expect("project root");
    let python_version = system_python_version().expect("system python version");
    fs::write(
        project_root.join("pyproject.toml"),
        format!(
            r#"[project]
name = "sample-sync-importlib-inspection"
version = "0.1.0"
dependencies = []

[tool.pyra]
python = "{python_version}"
"#,
        ),
    )
    .expect("pyproject");

    let managed_env = home.path().join("managed-python");
    create_virtualenv(&system_python().expect("system python"), &managed_env)
        .expect("managed virtualenv");
    let managed_python = venv_python_path(&managed_env);
    seed_managed_install_with_executable(&home, &python_version, &managed_python)
        .expect("managed install");

    let poison_root = home.path().join("poisoned-pythonpath");
    fs::create_dir_all(poison_root.join("pip")).expect("poisoned pip package");
    fs::write(poison_root.join("pip").join("__init__.py"), "").expect("pip __init__");
    fs::write(
        poison_root.join("pip").join("__main__.py"),
        "raise SystemExit('pip list should not be used during environment inspection')\n",
    )
    .expect("pip __main__");

    let state_path = home.path().join("unused-installer-state.json");
    let mut command = base_command(&home, &state_path);
    command
        .env_remove("PYRA_SYNC_INSTALLER_STATE_PATH")
        .env("PYTHONPATH", &poison_root)
        .current_dir(&project_root)
        .args(["sync"])
        .assert()
        .success()
        .stdout(contains("Synced"));
}

#[cfg(unix)]
#[test]
fn sync_installs_from_verified_local_artifact_path() {
    let home = temp_env_root();
    let index = start_fixture_index();
    let project_root = home
        .path()
        .join("workspace")
        .join("sample-sync-verified-artifact");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-sync-verified-artifact"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["attrs==25.1.0"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    let log_path = home.path().join("fake-python-log.jsonl");
    let fake_python = build_fake_managed_python(home.path(), &log_path).expect("fake python");
    seed_managed_install_with_executable(&home, "3.13.12", &fake_python).expect("managed install");
    let state_path = home.path().join("unused-installer-state.json");

    base_command(&home, &state_path)
        .env_remove("PYRA_SYNC_INSTALLER_STATE_PATH")
        .env("PYRA_FAKE_PYTHON_LOG", &log_path)
        .env("PYRA_INDEX_URL", &index.base_url)
        .current_dir(&project_root)
        .args(["sync"])
        .assert()
        .success()
        .stdout(contains("Synced"));

    let install_target = fake_python_install_target(&log_path).expect("install target");
    assert!(!install_target.starts_with("file://"));
    assert!(Path::new(&install_target).is_absolute());
    assert!(Path::new(&install_target).starts_with(home.path().join("cache")));
    assert!(Path::new(&install_target).exists());
}

#[cfg(unix)]
#[test]
fn sync_target_override_regenerates_lock_when_target_set_changes() {
    let home = temp_env_root();
    let index = start_fixture_index();
    let project_root = home
        .path()
        .join("workspace")
        .join("sample-sync-target-override-refresh");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-sync-target-override-refresh"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["attrs==25.1.0"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");
    let (host_target, foreign_target) = host_and_foreign_targets().expect("supported host target");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["sync", "--target", &host_target])
        .assert()
        .success()
        .stdout(contains("Synced"));

    let initial_lock = fs::read_to_string(project_root.join("pylock.toml")).expect("pylock");
    assert!(initial_lock.contains(&format!("target-triple = \"{host_target}\"")));
    assert!(!initial_lock.contains(&format!("target-triple = \"{foreign_target}\"")));
    assert!(initial_lock.contains("resolution-strategy = \"environment-scoped-union-v1\""));

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args([
            "sync",
            "--target",
            &host_target,
            "--target",
            &foreign_target,
        ])
        .assert()
        .success()
        .stdout(contains("Updated `pylock.toml`"));

    let refreshed_lock = fs::read_to_string(project_root.join("pylock.toml")).expect("pylock");
    assert!(refreshed_lock.contains(&format!(
        "id = \"{}\"",
        lock_environment_id("3.13.12", &host_target)
    )));
    assert!(refreshed_lock.contains(&format!(
        "id = \"{}\"",
        lock_environment_id("3.13.12", &foreign_target)
    )));
    assert!(refreshed_lock.contains("resolution-strategy = \"environment-scoped-matrix-v1\""));
}

#[cfg(unix)]
#[test]
fn sync_target_override_beats_project_targets_for_one_invocation() {
    let home = temp_env_root();
    let index = start_fixture_index();
    let project_root = home
        .path()
        .join("workspace")
        .join("sample-sync-target-override-project-config");
    fs::create_dir_all(&project_root).expect("project root");
    let (host_target, foreign_target) = host_and_foreign_targets().expect("supported host target");
    fs::write(
        project_root.join("pyproject.toml"),
        format!(
            r#"[project]
name = "sample-sync-target-override-project-config"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["attrs==25.1.0"]

[tool.pyra]
python = "3.13.12"
targets = ["{host_target}", "{foreign_target}"]
"#,
        ),
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["sync", "--target", &host_target])
        .assert()
        .success()
        .stdout(contains("Synced"));

    let lock = fs::read_to_string(project_root.join("pylock.toml")).expect("pylock");
    assert!(lock.contains(&format!(
        "id = \"{}\"",
        lock_environment_id("3.13.12", &host_target)
    )));
    assert!(!lock.contains(&format!(
        "id = \"{}\"",
        lock_environment_id("3.13.12", &foreign_target)
    )));
    assert!(lock.contains("resolution-strategy = \"environment-scoped-union-v1\""));
}

#[cfg(unix)]
#[test]
fn sync_frozen_uses_only_the_current_host_slice_from_a_multi_target_lock() {
    let home = temp_env_root();
    let (host_target, foreign_target) = host_and_foreign_targets().expect("supported host target");
    let project_root = home
        .path()
        .join("workspace")
        .join("sample-sync-frozen-multi-target");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        format!(
            r#"[project]
name = "sample-sync-frozen-multi-target"
version = "0.1.0"
dependencies = []

[tool.pyra]
python = "3.13.12"
targets = ["{host_target}", "{foreign_target}"]
"#,
        ),
    )
    .expect("pyproject");

    let log_path = home.path().join("fake-python-log.jsonl");
    let fake_python = build_fake_managed_python(home.path(), &log_path).expect("fake python");
    seed_managed_install_with_executable(&home, "3.13.12", &fake_python).expect("managed install");
    let state_path = home.path().join("unused-installer-state.json");

    let mut input_fingerprint_digest = Sha256::new();
    input_fingerprint_digest.update("sample-sync-frozen-multi-target".as_bytes());
    input_fingerprint_digest.update("3.13.12".as_bytes());
    let input_fingerprint = format!("{:x}", input_fingerprint_digest.finalize());
    let host_wheel_name = wheel_name_for_target("shared", "1.0.0", &host_target);
    let foreign_wheel_name = wheel_name_for_target("shared", "1.0.0", &foreign_target);
    let foreign_only_wheel_name = wheel_name_for_target("foreign-only", "2.0.0", &foreign_target);
    let host_wheel_path = home.path().join(&host_wheel_name);
    let foreign_wheel_path = home.path().join(&foreign_wheel_name);
    let foreign_only_wheel_path = home.path().join(&foreign_only_wheel_name);
    let host_wheel_bytes = fixture_artifact_bytes(&host_wheel_name);
    let foreign_wheel_bytes = fixture_artifact_bytes(&foreign_wheel_name);
    let foreign_only_wheel_bytes = fixture_artifact_bytes(&foreign_only_wheel_name);
    fs::write(&host_wheel_path, &host_wheel_bytes).expect("host wheel");
    fs::write(&foreign_wheel_path, &foreign_wheel_bytes).expect("foreign shared wheel");
    fs::write(&foreign_only_wheel_path, &foreign_only_wheel_bytes).expect("foreign-only wheel");

    fs::write(
        project_root.join("pylock.toml"),
        format!(
            r#"lock-version = "1.0"
extras = []
dependency-groups = []
default-groups = ["pyra-default"]
created-by = "pyra"

[[environments]]
id = "{host_environment_id}"
marker = "{host_environment_marker}"
interpreter-version = "3.13.12"
target-triple = "{host_target}"

[[environments]]
id = "{foreign_environment_id}"
marker = "{foreign_environment_marker}"
interpreter-version = "3.13.12"
target-triple = "{foreign_target}"

[[packages]]
name = "foreign-only"
version = "2.0.0"
marker = "'pyra-default' in dependency_groups"
index = "https://example.test/simple"
[[packages.wheels]]
name = "{foreign_only_wheel_name}"
url = "file://{foreign_only_wheel_path}"
hashes = {{ sha256 = "{foreign_only_sha256}" }}

[[packages]]
name = "shared"
version = "1.0.0"
marker = "'pyra-default' in dependency_groups"
index = "https://example.test/simple"
[[packages.wheels]]
name = "{foreign_wheel_name}"
url = "file://{foreign_wheel_path}"
hashes = {{ sha256 = "{foreign_sha256}" }}
[[packages.wheels]]
name = "{host_wheel_name}"
url = "file://{host_wheel_path}"
hashes = {{ sha256 = "{host_sha256}" }}

[tool.pyra]
input-fingerprint = "{input_fingerprint}"
interpreter-version = "3.13.12"
target-triple = "{host_target}"
index-url = "https://pypi.org/simple"
resolution-strategy = "environment-scoped-matrix-v1"
"#,
            input_fingerprint = input_fingerprint,
            host_environment_id = lock_environment_id("3.13.12", &host_target),
            host_environment_marker = lock_environment_marker("3.13.12", &host_target),
            foreign_environment_id = lock_environment_id("3.13.12", &foreign_target),
            foreign_environment_marker = lock_environment_marker("3.13.12", &foreign_target),
            host_target = host_target,
            foreign_target = foreign_target,
            foreign_only_wheel_name = foreign_only_wheel_name,
            foreign_only_wheel_path = foreign_only_wheel_path.display(),
            foreign_only_sha256 = format!("{:x}", Sha256::digest(&foreign_only_wheel_bytes)),
            foreign_wheel_name = foreign_wheel_name,
            foreign_wheel_path = foreign_wheel_path.display(),
            foreign_sha256 = format!("{:x}", Sha256::digest(&foreign_wheel_bytes)),
            host_wheel_name = host_wheel_name,
            host_wheel_path = host_wheel_path.display(),
            host_sha256 = format!("{:x}", Sha256::digest(&host_wheel_bytes)),
        ),
    )
    .expect("pylock");

    base_command(&home, &state_path)
        .env_remove("PYRA_SYNC_INSTALLER_STATE_PATH")
        .env("PYRA_FAKE_PYTHON_LOG", &log_path)
        .current_dir(&project_root)
        .args(["sync", "--frozen"])
        .assert()
        .success()
        .stdout(contains("Reused the current lock"));

    let install_targets = fake_python_install_targets(&log_path).expect("install targets");
    assert_eq!(install_targets.len(), 1);
    let expected_cached_host_artifact = home
        .path()
        .join("cache")
        .join("artifacts")
        .join("verified")
        .join(format!("{:x}", Sha256::digest(&host_wheel_bytes)))
        .join(&host_wheel_name);
    assert_eq!(
        PathBuf::from(&install_targets[0]),
        expected_cached_host_artifact
    );
    assert!(expected_cached_host_artifact.exists());
}

#[test]
fn sync_reuses_verified_artifact_cache_for_warm_reinstall() {
    let home = temp_env_root();
    let package_name = "cachedemo";
    let package_version = "0.1.0";
    let index =
        start_installable_fixture_index(package_name, package_version).expect("installable index");
    let project_root = home.path().join("workspace").join("sample-sync-warm-cache");
    fs::create_dir_all(&project_root).expect("project root");
    let python_version = system_python_version().expect("system python version");
    fs::write(
        project_root.join("pyproject.toml"),
        format!(
            r#"[project]
name = "sample-sync-warm-cache"
version = "0.1.0"
requires-python = "{requires_python}"
dependencies = ["{package_name}=={package_version}"]

[tool.pyra]
python = "{python_version}"
"#,
            requires_python = requires_python_series(&python_version),
        ),
    )
    .expect("pyproject");

    let managed_env = home.path().join("managed-python");
    create_virtualenv(&system_python().expect("system python"), &managed_env)
        .expect("managed virtualenv");
    let managed_python = venv_python_path(&managed_env);
    seed_managed_install_with_executable(&home, &python_version, &managed_python)
        .expect("managed install");

    let state_path = home.path().join("unused-installer-state.json");
    base_command(&home, &state_path)
        .env_remove("PYRA_SYNC_INSTALLER_STATE_PATH")
        .env("PYRA_INDEX_URL", &index.base_url)
        .current_dir(&project_root)
        .args(["sync"])
        .assert()
        .success()
        .stdout(contains("Synced"));

    let cached_artifact = home
        .path()
        .join("cache")
        .join("artifacts")
        .join("verified")
        .join(&index.artifact_sha256)
        .join(&index.artifact_name);
    assert!(cached_artifact.exists());

    fs::remove_dir_all(home.path().join("data").join("environments")).expect("remove environments");
    fs::remove_file(&index.artifact_path).expect("remove source artifact");
    assert!(!index.artifact_path.exists());

    base_command(&home, &state_path)
        .env_remove("PYRA_SYNC_INSTALLER_STATE_PATH")
        .env("PYRA_INDEX_URL", &index.base_url)
        .current_dir(&project_root)
        .args(["sync"])
        .assert()
        .success()
        .stdout(contains("Reused the current lock"));

    assert!(cached_artifact.exists());
}

#[test]
fn sync_reuses_fresh_lock_when_freshness_inputs_are_unchanged() {
    let home = temp_env_root();
    let index = start_fixture_index();
    let project_root = home
        .path()
        .join("workspace")
        .join("sample-sync-fresh-reuse");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-sync-fresh-reuse"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["attrs==25.1.0"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["sync"])
        .assert()
        .success();

    let first_lock = fs::read_to_string(project_root.join("pylock.toml")).expect("pylock");
    assert!(first_lock.contains("resolution-strategy = \"environment-scoped-union-v1\""));

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["sync"])
        .assert()
        .success()
        .stdout(contains("Reused the current lock"));

    let second_lock = fs::read_to_string(project_root.join("pylock.toml")).expect("pylock");
    assert_eq!(first_lock, second_lock);
}

#[test]
fn lock_generates_and_reuses_fresh_lock_without_reconciling_environment() {
    let home = temp_env_root();
    let project_root = home.path().join("workspace").join("sample-lock-generate");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-lock-generate"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = []

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");
    seed_managed_install(&home, "3.13.12").expect("managed install");

    let state_path = home.path().join("installer-state.json");
    fs::write(
        &state_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "sentinel": "1.0.0"
        }))
        .expect("state json"),
    )
    .expect("state");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .args(["lock"])
        .assert()
        .success()
        .stdout(contains("Generated `pylock.toml`"));

    let first_lock = fs::read_to_string(project_root.join("pylock.toml")).expect("pylock");
    let first_state = read_state(&state_path);
    assert_eq!(
        first_state,
        std::collections::BTreeMap::from([("sentinel".to_string(), "1.0.0".to_string())])
    );
    let first_environment_entries = fs::read_dir(home.path().join("data").join("environments"))
        .expect("environments dir")
        .count();
    assert_eq!(
        first_environment_entries, 0,
        "`pyra lock` should not reconcile the environment"
    );

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .args(["lock"])
        .assert()
        .success()
        .stdout(contains("Reused fresh `pylock.toml`"));

    let second_lock = fs::read_to_string(project_root.join("pylock.toml")).expect("pylock");
    assert_eq!(first_lock, second_lock);
    assert_eq!(
        read_state(&state_path),
        std::collections::BTreeMap::from([("sentinel".to_string(), "1.0.0".to_string())])
    );
    let second_environment_entries = fs::read_dir(home.path().join("data").join("environments"))
        .expect("environments dir")
        .count();
    assert_eq!(
        second_environment_entries, 0,
        "`pyra lock` should not reconcile the environment"
    );
}

#[test]
fn lock_json_contract_snapshot_for_missing_lock_generation() {
    let home = temp_env_root();
    let project_root = home.path().join("workspace").join("sample-lock-json");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-lock-json"
version = "0.1.0"
dependencies = []

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");
    seed_managed_install(&home, "3.13.12").expect("managed install");

    let state_path = home.path().join("installer-state.json");
    fs::write(
        &state_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "sentinel": "1.0.0"
        }))
        .expect("state json"),
    )
    .expect("state");

    let output = base_command(&home, &state_path)
        .current_dir(&project_root)
        .args(["--json", "lock"])
        .output()
        .expect("lock json output");

    let project_id = project_id_for(&project_root);
    assert_json_contract(
        &output,
        json!({
            "status": "success",
            "exit": {
                "code": 0,
                "category": "success"
            },
            "output": {
                "blocks": [
                    {
                        "type": "message",
                        "value": {
                            "tone": "success",
                            "summary": format!("Generated `pylock.toml` for `{project_id}`."),
                            "detail": "No lock file existed, so Pyra resolved dependencies and wrote a new lock.",
                            "hint": Value::Null,
                            "verbose": []
                        }
                    }
                ]
            },
            "error": Value::Null
        }),
    );
    assert_eq!(
        read_state(&state_path),
        std::collections::BTreeMap::from([("sentinel".to_string(), "1.0.0".to_string())])
    );
    let environment_entries = fs::read_dir(home.path().join("data").join("environments"))
        .expect("environments dir")
        .count();
    assert_eq!(
        environment_entries, 0,
        "`pyra lock` should not reconcile the environment"
    );
}

#[test]
fn lock_regenerates_stale_lock() {
    let home = temp_env_root();
    let project_root = home.path().join("workspace").join("sample-lock-stale");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-lock-stale"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = []

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");
    seed_managed_install(&home, "3.13.12").expect("managed install");

    let state_path = home.path().join("installer-state.json");
    fs::write(
        &state_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "sentinel": "1.0.0"
        }))
        .expect("state json"),
    )
    .expect("state");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .args(["lock"])
        .assert()
        .success();
    let fresh_lock = fs::read_to_string(project_root.join("pylock.toml")).expect("pylock");

    let stale_lock = fresh_lock.replace(
        "resolution-strategy = \"environment-scoped-union-v1\"",
        "resolution-strategy = \"legacy-strategy-v0\"",
    );
    assert_ne!(stale_lock, fresh_lock);
    fs::write(project_root.join("pylock.toml"), stale_lock).expect("stale lock");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .args(["lock"])
        .assert()
        .success()
        .stdout(contains("Regenerated `pylock.toml`"))
        .stdout(contains("stale"));

    let regenerated_lock = fs::read_to_string(project_root.join("pylock.toml")).expect("pylock");
    assert_eq!(regenerated_lock, fresh_lock);
    assert!(regenerated_lock.contains("resolution-strategy = \"environment-scoped-union-v1\""));
    assert_eq!(
        read_state(&state_path),
        std::collections::BTreeMap::from([("sentinel".to_string(), "1.0.0".to_string())])
    );
    let environment_entries = fs::read_dir(home.path().join("data").join("environments"))
        .expect("environments dir")
        .count();
    assert_eq!(
        environment_entries, 0,
        "`pyra lock` should not reconcile the environment"
    );
}

#[test]
fn doctor_reports_missing_lock_without_mutation() {
    let home = temp_env_root();
    let project_root = home
        .path()
        .join("workspace")
        .join("sample-doctor-missing-lock");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-doctor-missing-lock"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = []

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");
    seed_managed_install(&home, "3.13.12").expect("managed install");

    let state_path = home.path().join("installer-state.json");
    fs::write(
        &state_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "sentinel": "1.0.0"
        }))
        .expect("state json"),
    )
    .expect("state");

    let output = base_command(&home, &state_path)
        .current_dir(&project_root)
        .args(["doctor"])
        .output()
        .expect("doctor output");
    assert!(
        output.status.success(),
        "doctor should report warnings, not fail"
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout utf-8");
    assert!(stdout.contains("issue(s)"));
    assert!(stdout.contains("`pylock.toml` is missing."));

    assert!(!project_root.join("pylock.toml").exists());
    assert_eq!(
        read_state(&state_path),
        std::collections::BTreeMap::from([("sentinel".to_string(), "1.0.0".to_string())])
    );
    let environment_entries = fs::read_dir(home.path().join("data").join("environments"))
        .expect("environments dir")
        .count();
    assert_eq!(
        environment_entries, 0,
        "`pyra doctor` should not reconcile the environment"
    );
}

#[test]
fn doctor_reports_stale_lock_in_json_mode_without_mutation() {
    let home = temp_env_root();
    let project_root = home
        .path()
        .join("workspace")
        .join("sample-doctor-stale-lock");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-doctor-stale-lock"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = []

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");
    seed_managed_install(&home, "3.13.12").expect("managed install");

    let state_path = home.path().join("installer-state.json");
    fs::write(
        &state_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "sentinel": "1.0.0"
        }))
        .expect("state json"),
    )
    .expect("state");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .args(["lock"])
        .assert()
        .success();

    let lock_path = project_root.join("pylock.toml");
    let stale_lock = fs::read_to_string(&lock_path).expect("pylock").replace(
        "resolution-strategy = \"environment-scoped-union-v1\"",
        "resolution-strategy = \"legacy-strategy-v0\"",
    );
    fs::write(&lock_path, &stale_lock).expect("stale lock");

    let output = base_command(&home, &state_path)
        .current_dir(&project_root)
        .args(["--json", "doctor"])
        .output()
        .expect("doctor json output");
    let project_id = project_id_for(&project_root);
    let metadata_path = home
        .path()
        .join("data")
        .join("environments")
        .join(&project_id)
        .join("metadata.json");
    assert_json_contract(
        &output,
        json!({
            "status": "warn",
            "exit": {
                "code": 0,
                "category": "success"
            },
            "output": {
                "blocks": [
                    {
                        "type": "message",
                        "value": {
                            "tone": "warn",
                            "summary": format!("Found 2 issue(s) in `{project_id}`."),
                            "detail": "Run `pyra sync` or `pyra lock` based on the findings below.",
                            "hint": Value::Null,
                            "verbose": []
                        }
                    },
                    {
                        "type": "message",
                        "value": {
                            "tone": "warn",
                            "summary": "`pylock.toml` could not be parsed.",
                            "detail": "The lock file exists but is invalid for current lock semantics.",
                            "hint": "Run `pyra lock` to regenerate `pylock.toml`.",
                            "verbose": []
                        }
                    },
                    {
                        "type": "message",
                        "value": {
                            "tone": "warn",
                            "summary": "Centralized environment metadata is missing.",
                            "detail": format!(
                                "No environment record exists at `{}` for this project.",
                                metadata_path.display()
                            ),
                            "hint": "Run `pyra sync` to (re)build the centralized environment.",
                            "verbose": []
                        }
                    }
                ]
            },
            "error": Value::Null
        }),
    );

    let current_lock = fs::read_to_string(&lock_path).expect("pylock");
    assert_eq!(current_lock, stale_lock);
    assert_eq!(
        read_state(&state_path),
        std::collections::BTreeMap::from([("sentinel".to_string(), "1.0.0".to_string())])
    );
}

#[test]
fn doctor_reports_environment_drift_without_mutation() {
    let home = temp_env_root();
    let index = start_fixture_index();
    let project_root = home
        .path()
        .join("workspace")
        .join("sample-doctor-environment-drift");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-doctor-environment-drift"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["attrs==25.1.0"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["sync"])
        .assert()
        .success();

    let lock_path = project_root.join("pylock.toml");
    let lock_before = fs::read_to_string(&lock_path).expect("pylock");
    fs::write(
        &state_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "attrs": "24.0.0",
            "orphan": "1.0.0"
        }))
        .expect("state json"),
    )
    .expect("state");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["doctor"])
        .assert()
        .success()
        .stdout(contains(
            "Centralized environment has drifted from the selected lock state.",
        ));

    let lock_after = fs::read_to_string(&lock_path).expect("pylock");
    assert_eq!(lock_before, lock_after);
    assert_eq!(
        read_state(&state_path),
        std::collections::BTreeMap::from([
            ("attrs".to_string(), "24.0.0".to_string()),
            ("orphan".to_string(), "1.0.0".to_string()),
        ])
    );
}

#[test]
fn outdated_reports_package_level_upgrade_opportunities() {
    let home = temp_env_root();
    let index = start_conflict_fixture_index();
    let project_root = home.path().join("workspace").join("sample-outdated-report");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-outdated-report"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["shared>=1,<3"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["sync"])
        .assert()
        .success();

    let lock_path = project_root.join("pylock.toml");
    let downgraded_lock = fs::read_to_string(&lock_path)
        .expect("pylock")
        .replace("version = \"2.0.0\"", "version = \"1.5.0\"");
    fs::write(&lock_path, &downgraded_lock).expect("downgraded lock");

    let output = base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["outdated"])
        .output()
        .expect("outdated output");
    assert!(
        output.status.success(),
        "outdated should report warnings, not fail"
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout utf-8");
    assert!(stdout.contains("Found 1 outdated package(s)"));
    assert!(stdout.contains("shared: 1.5.0 -> 2.0.0"));

    let current_lock = fs::read_to_string(&lock_path).expect("pylock");
    assert_eq!(current_lock, downgraded_lock);
}

#[test]
fn outdated_json_mode_does_not_mutate_manifest_or_lock() {
    let home = temp_env_root();
    let index = start_conflict_fixture_index();
    let project_root = home
        .path()
        .join("workspace")
        .join("sample-outdated-no-mutation");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-outdated-no-mutation"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["shared>=1,<3"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["sync"])
        .assert()
        .success();

    let pyproject_path = project_root.join("pyproject.toml");
    let lock_path = project_root.join("pylock.toml");
    let downgraded_lock = fs::read_to_string(&lock_path)
        .expect("pylock")
        .replace("version = \"2.0.0\"", "version = \"1.5.0\"");
    fs::write(&lock_path, &downgraded_lock).expect("downgraded lock");

    let pyproject_before = fs::read_to_string(&pyproject_path).expect("pyproject");
    let lock_before = fs::read_to_string(&lock_path).expect("pylock");

    let output = base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["--json", "outdated"])
        .output()
        .expect("outdated json output");
    let project_id = project_id_for(&project_root);
    assert_json_contract(
        &output,
        json!({
            "status": "warn",
            "exit": {
                "code": 0,
                "category": "success"
            },
            "output": {
                "blocks": [
                    {
                        "type": "message",
                        "value": {
                            "tone": "warn",
                            "summary": format!("Found 1 outdated package(s) in `{project_id}`."),
                            "detail": "Newer versions are available while preserving the current dependency intent.",
                            "hint": Value::Null,
                            "verbose": []
                        }
                    },
                    {
                        "type": "list",
                        "value": {
                            "heading": "Outdated packages",
                            "items": [
                                {
                                    "label": "shared: 1.5.0 -> 2.0.0",
                                    "detail": "declared as shared>=1,<3",
                                    "verbose": []
                                }
                            ],
                            "empty_message": Value::Null
                        }
                    }
                ]
            },
            "error": Value::Null
        }),
    );

    let pyproject_after = fs::read_to_string(&pyproject_path).expect("pyproject");
    let lock_after = fs::read_to_string(&lock_path).expect("pylock");
    assert_eq!(pyproject_after, pyproject_before);
    assert_eq!(lock_after, lock_before);
}

#[test]
fn update_rewrites_lock_to_latest_allowed_versions() {
    let home = temp_env_root();
    let initial_index = start_fixture_index();
    let update_index = start_conflict_fixture_index();
    let project_root = home.path().join("workspace").join("sample-update-rewrite");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-update-rewrite"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["shared>=1,<3"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");
    fs::write(
        &state_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "sentinel": "1.0.0"
        }))
        .expect("state json"),
    )
    .expect("state");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &initial_index.base_url)
        .args(["lock"])
        .assert()
        .success();

    let pyproject_path = project_root.join("pyproject.toml");
    let pyproject_before = fs::read_to_string(&pyproject_path).expect("pyproject");
    let lock_path = project_root.join("pylock.toml");
    let initial_lock = fs::read_to_string(&lock_path).expect("pylock");
    assert!(initial_lock.contains("name = \"shared\""));
    assert!(initial_lock.contains("version = \"1.5.0\""));

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &update_index.base_url)
        .args(["update"])
        .assert()
        .success()
        .stdout(contains("Updated `pylock.toml`"))
        .stdout(contains("shared: 1.5.0 -> 2.0.0"));

    let updated_lock = fs::read_to_string(&lock_path).expect("pylock");
    assert!(updated_lock.contains("name = \"shared\""));
    assert!(updated_lock.contains("version = \"2.0.0\""));
    assert!(!updated_lock.contains("version = \"1.5.0\""));
    assert_eq!(
        fs::read_to_string(&pyproject_path).expect("pyproject"),
        pyproject_before
    );
    assert_eq!(
        read_state(&state_path),
        std::collections::BTreeMap::from([("sentinel".to_string(), "1.0.0".to_string())])
    );
    let environment_entries = fs::read_dir(home.path().join("data").join("environments"))
        .expect("environments dir")
        .count();
    assert_eq!(
        environment_entries, 0,
        "`pyra update` should not reconcile the environment"
    );
}

#[test]
fn update_dry_run_reports_summary_without_writing_lock() {
    let home = temp_env_root();
    let initial_index = start_fixture_index();
    let update_index = start_conflict_fixture_index();
    let project_root = home.path().join("workspace").join("sample-update-dry-run");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-update-dry-run"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["shared>=1,<3"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");
    fs::write(
        &state_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "sentinel": "1.0.0"
        }))
        .expect("state json"),
    )
    .expect("state");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &initial_index.base_url)
        .args(["lock"])
        .assert()
        .success();

    let pyproject_path = project_root.join("pyproject.toml");
    let pyproject_before = fs::read_to_string(&pyproject_path).expect("pyproject");
    let lock_path = project_root.join("pylock.toml");
    let lock_before = fs::read_to_string(&lock_path).expect("pylock");
    assert!(lock_before.contains("version = \"1.5.0\""));

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &update_index.base_url)
        .args(["update", "--dry-run"])
        .assert()
        .success()
        .stdout(contains("Dry run"))
        .stdout(contains("shared: 1.5.0 -> 2.0.0"))
        .stdout(contains("left `pylock.toml` unchanged"));

    assert_eq!(
        fs::read_to_string(&pyproject_path).expect("pyproject"),
        pyproject_before
    );
    assert_eq!(fs::read_to_string(&lock_path).expect("pylock"), lock_before);
    assert_eq!(
        read_state(&state_path),
        std::collections::BTreeMap::from([("sentinel".to_string(), "1.0.0".to_string())])
    );
}

#[test]
fn update_dry_run_json_contract_snapshot_for_planned_lock_changes() {
    let home = temp_env_root();
    let initial_index = start_fixture_index();
    let update_index = start_conflict_fixture_index();
    let project_root = home.path().join("workspace").join("sample-update-json");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-update-json"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["shared>=1,<3"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");
    fs::write(
        &state_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "sentinel": "1.0.0"
        }))
        .expect("state json"),
    )
    .expect("state");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &initial_index.base_url)
        .args(["lock"])
        .assert()
        .success();

    let lock_path = project_root.join("pylock.toml");
    let lock_before = fs::read_to_string(&lock_path).expect("pylock");

    let output = base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &update_index.base_url)
        .args(["--json", "update", "--dry-run"])
        .output()
        .expect("update json output");

    let project_id = project_id_for(&project_root);
    assert_json_contract(
        &output,
        json!({
            "status": "warn",
            "exit": {
                "code": 0,
                "category": "success"
            },
            "output": {
                "blocks": [
                    {
                        "type": "message",
                        "value": {
                            "tone": "warn",
                            "summary": format!(
                                "Dry run: `pyra update` would change 1 package(s) in `{project_id}`."
                            ),
                            "detail": "Resolved latest versions allowed by current specifiers, but left `pylock.toml` unchanged.",
                            "hint": "This command only refreshes lock state. Use `pyra add`/`pyra remove` to change declared dependency intent.",
                            "verbose": []
                        }
                    },
                    {
                        "type": "list",
                        "value": {
                            "heading": "Planned lock changes",
                            "items": [
                                {
                                    "label": "shared: 1.5.0 -> 2.0.0",
                                    "detail": "updated",
                                    "verbose": []
                                }
                            ],
                            "empty_message": Value::Null
                        }
                    }
                ]
            },
            "error": Value::Null
        }),
    );
    assert_eq!(fs::read_to_string(&lock_path).expect("pylock"), lock_before);
}

#[test]
fn update_lock_rewrite_is_deterministic_for_unchanged_inputs() {
    let home = temp_env_root();
    let initial_index = start_fixture_index();
    let update_index = start_conflict_fixture_index();
    let project_root = home
        .path()
        .join("workspace")
        .join("sample-update-deterministic");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-update-deterministic"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["shared>=1,<3"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");
    fs::write(
        &state_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "sentinel": "1.0.0"
        }))
        .expect("state json"),
    )
    .expect("state");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &initial_index.base_url)
        .args(["lock"])
        .assert()
        .success();

    let lock_path = project_root.join("pylock.toml");
    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &update_index.base_url)
        .args(["update"])
        .assert()
        .success()
        .stdout(contains("shared: 1.5.0 -> 2.0.0"));
    let first_rewrite = fs::read_to_string(&lock_path).expect("pylock");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &update_index.base_url)
        .args(["update"])
        .assert()
        .success()
        .stdout(contains("with no package changes"));
    let second_rewrite = fs::read_to_string(&lock_path).expect("pylock");

    assert_eq!(first_rewrite, second_rewrite);
    assert_eq!(
        read_state(&state_path),
        std::collections::BTreeMap::from([("sentinel".to_string(), "1.0.0".to_string())])
    );
}

#[test]
fn sync_locked_fails_when_lock_is_missing() {
    let home = temp_env_root();
    let project_root = home
        .path()
        .join("workspace")
        .join("sample-sync-locked-missing");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-sync-locked-missing"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["attrs==25.1.0"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .args(["sync", "--locked"])
        .assert()
        .failure()
        .stderr(contains("sync --locked"))
        .stderr(contains("pylock.toml"));

    assert!(!project_root.join("pylock.toml").exists());
}

#[test]
fn sync_locked_fails_when_lock_is_stale() {
    let home = temp_env_root();
    let index = start_fixture_index();
    let project_root = home
        .path()
        .join("workspace")
        .join("sample-sync-locked-stale");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-sync-locked-stale"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["attrs==25.1.0"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["sync"])
        .assert()
        .success();

    let lock_path = project_root.join("pylock.toml");
    let original_lock = fs::read_to_string(&lock_path).expect("pylock");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-sync-locked-stale"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["attrs==25.1.0", "httpx==0.27.0"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("updated pyproject");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["sync", "--locked"])
        .assert()
        .failure()
        .stderr(contains("stale"))
        .stderr(contains("sync --locked"));

    let current_lock = fs::read_to_string(&lock_path).expect("pylock");
    assert_eq!(original_lock, current_lock);
    let state = read_state(&state_path);
    assert_eq!(state.get("attrs"), Some(&"25.1.0".to_string()));
    assert!(!state.contains_key("httpx"));
}

#[test]
fn sync_locked_reuses_fresh_lock() {
    let home = temp_env_root();
    let index = start_fixture_index();
    let project_root = home
        .path()
        .join("workspace")
        .join("sample-sync-locked-fresh");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-sync-locked-fresh"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["attrs==25.1.0"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["sync"])
        .assert()
        .success();

    let first_lock = fs::read_to_string(project_root.join("pylock.toml")).expect("pylock");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["sync", "--locked"])
        .assert()
        .success()
        .stdout(contains("Reused the current lock"));

    let second_lock = fs::read_to_string(project_root.join("pylock.toml")).expect("pylock");
    assert_eq!(first_lock, second_lock);
}

#[test]
fn sync_frozen_fails_when_lock_is_stale() {
    let home = temp_env_root();
    let index = start_fixture_index();
    let project_root = home
        .path()
        .join("workspace")
        .join("sample-sync-frozen-stale");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-sync-frozen-stale"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["attrs==25.1.0"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["sync"])
        .assert()
        .success();

    let lock_path = project_root.join("pylock.toml");
    let original_lock = fs::read_to_string(&lock_path).expect("pylock");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-sync-frozen-stale"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["attrs==25.1.0", "httpx==0.27.0"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("updated pyproject");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["sync", "--frozen"])
        .assert()
        .failure()
        .stderr(contains("stale"))
        .stderr(contains("sync --frozen"));

    let current_lock = fs::read_to_string(&lock_path).expect("pylock");
    assert_eq!(original_lock, current_lock);
    let state = read_state(&state_path);
    assert_eq!(state.get("attrs"), Some(&"25.1.0".to_string()));
    assert!(!state.contains_key("httpx"));
    assert!(!state.contains_key("anyio"));
}

#[test]
fn sync_rejects_locked_and_frozen_together() {
    let home = temp_env_root();
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .args(["sync", "--locked", "--frozen"])
        .assert()
        .failure()
        .stderr(contains("--locked"))
        .stderr(contains("--frozen"));
}

#[test]
fn sync_regenerates_stale_lock_when_resolution_strategy_changes() {
    let home = temp_env_root();
    let index = start_fixture_index();
    let project_root = home
        .path()
        .join("workspace")
        .join("sample-sync-stale-strategy");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-sync-stale-strategy"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["attrs==25.1.0"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["sync"])
        .assert()
        .success();

    let lock_path = project_root.join("pylock.toml");
    let stale_lock = fs::read_to_string(&lock_path).expect("pylock").replace(
        "resolution-strategy = \"environment-scoped-union-v1\"",
        "resolution-strategy = \"legacy-strategy-v0\"",
    );
    fs::write(&lock_path, stale_lock).expect("stale lock");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["sync"])
        .assert()
        .success()
        .stdout(contains("Updated `pylock.toml`"));

    let regenerated_lock = fs::read_to_string(&lock_path).expect("pylock");
    assert!(regenerated_lock.contains("resolution-strategy = \"environment-scoped-union-v1\""));
}

#[test]
fn sync_regenerates_stale_lock_after_dependency_change() {
    let home = temp_env_root();
    let index = start_fixture_index();
    let project_root = home.path().join("workspace").join("sample-sync-stale");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-sync-stale"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["attrs==25.1.0"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["sync"])
        .assert()
        .success();

    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-sync-stale"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["attrs==25.1.0", "httpx==0.27.0"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["sync"])
        .assert()
        .success()
        .stdout(contains("Updated `pylock.toml`"));

    let state = read_state(&state_path);
    assert_eq!(state.get("httpx"), Some(&"0.27.0".to_string()));
    assert_eq!(state.get("anyio"), Some(&"4.4.0".to_string()));
}

#[test]
fn sync_fails_without_pinned_python() {
    let home = temp_env_root();
    let project_root = home.path().join("workspace").join("sample-sync-unpinned");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-sync-unpinned"
version = "0.1.0"
dependencies = []
"#,
    )
    .expect("pyproject");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .args(["sync"])
        .assert()
        .failure()
        .stderr(contains("no Python is pinned yet"));
}

#[test]
fn sync_fails_when_project_requires_python_excludes_pinned_interpreter() {
    let home = temp_env_root();
    let project_root = home
        .path()
        .join("workspace")
        .join("sample-sync-requires-python-mismatch");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-sync-requires-python-mismatch"
version = "0.1.0"
requires-python = "<3.13"
dependencies = []

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");
    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .args(["sync"])
        .assert()
        .failure()
        .stderr(contains("3.13.12"))
        .stderr(contains("<3.13"));
}

#[test]
fn sync_renders_conflicts_with_the_incompatible_constraints() {
    let home = temp_env_root();
    let index = start_conflict_fixture_index();
    let project_root = home.path().join("workspace").join("sample-sync-conflict");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-sync-conflict"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["alpha", "bravo"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");
    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    let output = base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["sync"])
        .output()
        .expect("sync output");

    assert!(!output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        concat!(
            "error: Pyra could not resolve a compatible dependency set.\n",
            "`alpha` requires `shared` `<2`, but `bravo` requires `shared` `>=2`.\n",
            "next: Adjust the declared dependency constraints and retry.\n",
        )
    );
}

#[test]
fn sync_fails_for_invalid_dependency_group_config() {
    let home = temp_env_root();
    let project_root = home
        .path()
        .join("workspace")
        .join("sample-sync-invalid-group");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-sync-invalid-group"
version = "0.1.0"
dependencies = []

[dependency-groups]
a = [{include-group = "b"}]
b = [{include-group = "a"}]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .args(["sync"])
        .assert()
        .failure()
        .stderr(contains("include cycle"));
}

#[test]
fn add_updates_base_dependencies_in_pyproject() {
    let home = temp_env_root();
    let index = start_fixture_index();
    let project_root = home.path().join("workspace").join("sample-add-base");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-add-base"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["attrs==25.1.0"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["add", "httpx==0.27.0"])
        .assert()
        .success()
        .stdout(contains("Added `httpx==0.27.0`"));

    let pyproject = fs::read_to_string(project_root.join("pyproject.toml")).expect("pyproject");
    assert!(pyproject.contains("dependencies = [\"attrs==25.1.0\", \"httpx==0.27.0\"]"));
}

#[test]
fn add_json_contract_snapshot_for_base_dependency_mutation() {
    let home = temp_env_root();
    let index = start_fixture_index();
    let project_root = home.path().join("workspace").join("sample-add-json");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-add-json"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["attrs==25.1.0"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    let output = base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["--json", "add", "httpx==0.27.0"])
        .output()
        .expect("add json output");

    assert_json_contract(
        &output,
        json!({
            "status": "success",
            "exit": {
                "code": 0,
                "category": "success"
            },
            "output": {
                "blocks": [
                    {
                        "type": "message",
                        "value": {
                            "tone": "success",
                            "summary": "Added `httpx==0.27.0` to `[project].dependencies`.",
                            "detail": "Updated `pyproject.toml`, refreshed `pylock.toml`, and reconciled the centralized environment.",
                            "hint": Value::Null,
                            "verbose": []
                        }
                    }
                ]
            },
            "error": Value::Null
        }),
    );
}

#[test]
fn add_resolves_click_fixture_end_to_end() {
    let home = temp_env_root();
    let index = start_fixture_index();
    let project_root = home.path().join("workspace").join("sample-add-click");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-add-click"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = []

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["add", "click"])
        .assert()
        .success()
        .stdout(contains("Added `click`"));

    let pyproject = fs::read_to_string(project_root.join("pyproject.toml")).expect("pyproject");
    assert!(pyproject.contains("dependencies = [\"click\"]"));

    let lock = fs::read_to_string(project_root.join("pylock.toml")).expect("pylock");
    assert!(lock.contains("name = \"click\""));
    assert!(lock.contains("version = \"8.1.7\""));

    let state = read_state(&state_path);
    assert_eq!(state.get("click"), Some(&"8.1.7".to_string()));
}

#[test]
fn add_updates_dependency_group_in_pyproject() {
    let home = temp_env_root();
    let index = start_fixture_index();
    let project_root = home.path().join("workspace").join("sample-add-group");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-add-group"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = []

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["add", "pytest==8.3.0", "--group", "dev"])
        .assert()
        .success()
        .stdout(contains("dependency group `dev`"));

    let pyproject = fs::read_to_string(project_root.join("pyproject.toml")).expect("pyproject");
    assert!(pyproject.contains("[dependency-groups]"));
    assert!(pyproject.contains("dev = [\"pytest==8.3.0\"]"));

    let state = read_state(&state_path);
    assert_eq!(state.get("pytest"), Some(&"8.3.0".to_string()));
    assert_eq!(state.get("pluggy"), Some(&"1.5.0".to_string()));
}

#[test]
fn add_updates_extra_in_pyproject() {
    let home = temp_env_root();
    let index = start_fixture_index();
    let project_root = home.path().join("workspace").join("sample-add-extra");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-add-extra"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = []

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["add", "httpx==0.27.0", "--extra", "cli-tools"])
        .assert()
        .success()
        .stdout(contains("extra `cli-tools`"));

    let pyproject = fs::read_to_string(project_root.join("pyproject.toml")).expect("pyproject");
    assert!(pyproject.contains("[project.optional-dependencies]"));
    assert!(pyproject.contains("cli-tools = [\"httpx==0.27.0\"]"));

    let lock = fs::read_to_string(project_root.join("pylock.toml")).expect("pylock");
    assert!(lock.contains("extras = [\"cli-tools\"]"));
    assert!(lock.contains("name = \"httpx\""));

    let state = read_state(&state_path);
    assert!(!state.contains_key("httpx"));
}

#[test]
fn sync_resolves_urllib3_fixture_end_to_end() {
    let home = temp_env_root();
    let index = start_fixture_index();
    let project_root = home.path().join("workspace").join("sample-sync-urllib3");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-sync-urllib3"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["urllib3"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["sync"])
        .assert()
        .success();

    let state = read_state(&state_path);
    assert_eq!(state.get("urllib3"), Some(&"2.2.1".to_string()));
}

#[test]
fn sync_resolves_requests_extra_fixture_end_to_end() {
    let home = temp_env_root();
    let index = start_fixture_index();
    let project_root = home
        .path()
        .join("workspace")
        .join("sample-sync-requests-extra");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-sync-requests-extra"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["requests[socks]"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["sync"])
        .assert()
        .success();

    let state = read_state(&state_path);
    assert_eq!(state.get("requests"), Some(&"2.32.3".to_string()));
    assert_eq!(state.get("urllib3"), Some(&"2.2.1".to_string()));
    assert_eq!(state.get("pysocks"), Some(&"1.7.1".to_string()));
}

#[test]
fn sync_falls_back_to_sdist_fixture_end_to_end() {
    let home = temp_env_root();
    let index = start_fixture_index();
    let project_root = home
        .path()
        .join("workspace")
        .join("sample-sync-sdist-fallback");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-sync-sdist-fallback"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["sdistonly"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["sync"])
        .assert()
        .success();

    let lock = fs::read_to_string(project_root.join("pylock.toml")).expect("pylock");
    assert!(lock.contains("[packages.sdist]"));
    assert!(lock.contains("name = \"sdistonly-1.2.3.tar.gz\""));

    let state = read_state(&state_path);
    assert_eq!(state.get("sdistonly"), Some(&"1.2.3".to_string()));
    assert_eq!(state.get("shared"), Some(&"1.5.0".to_string()));
}

#[test]
fn add_does_not_duplicate_existing_requirement() {
    let home = temp_env_root();
    let index = start_fixture_index();
    let project_root = home.path().join("workspace").join("sample-add-existing");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-add-existing"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["attrs==25.1.0"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["add", "attrs==25.1.0"])
        .assert()
        .success()
        .stdout(contains("already declared"));

    let pyproject = fs::read_to_string(project_root.join("pyproject.toml")).expect("pyproject");
    assert_eq!(pyproject.matches("attrs==25.1.0").count(), 1);
    assert!(project_root.join("pylock.toml").exists());

    let state = read_state(&state_path);
    assert_eq!(state.get("attrs"), Some(&"25.1.0".to_string()));
}

#[test]
fn add_rejects_invalid_requirement_before_mutation() {
    let home = temp_env_root();
    let project_root = home.path().join("workspace").join("sample-add-invalid");
    fs::create_dir_all(&project_root).expect("project root");
    let original_pyproject = r#"[project]
name = "sample-add-invalid"
version = "0.1.0"
dependencies = []
"#;
    fs::write(project_root.join("pyproject.toml"), original_pyproject).expect("pyproject");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .args(["add", "not a valid requirement"])
        .assert()
        .failure()
        .stderr(contains("could not parse"))
        .stderr(contains("PEP 508"));

    let pyproject = fs::read_to_string(project_root.join("pyproject.toml")).expect("pyproject");
    assert_eq!(pyproject, original_pyproject);
    assert!(!project_root.join("pylock.toml").exists());
}

#[test]
fn add_updates_lockfile_and_environment_after_mutation() {
    let home = temp_env_root();
    let index = start_fixture_index();
    let project_root = home.path().join("workspace").join("sample-add-sync");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-add-sync"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["attrs==25.1.0"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["add", "httpx==0.27.0"])
        .assert()
        .success()
        .stdout(contains("reconciled the centralized environment"));

    let lock = fs::read_to_string(project_root.join("pylock.toml")).expect("pylock");
    assert!(lock.contains("name = \"httpx\""));
    assert!(lock.contains("name = \"anyio\""));

    let state = read_state(&state_path);
    assert_eq!(state.get("attrs"), Some(&"25.1.0".to_string()));
    assert_eq!(state.get("httpx"), Some(&"0.27.0".to_string()));
    assert_eq!(state.get("anyio"), Some(&"4.4.0".to_string()));
}

#[test]
fn remove_updates_base_dependencies_in_pyproject_only() {
    let home = temp_env_root();
    let index = start_fixture_index();
    let project_root = home.path().join("workspace").join("sample-remove-base");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-remove-base"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["attrs==25.1.0", "httpx==0.27.0"]

[dependency-groups]
docs = ["httpx==0.27.0"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["remove", "httpx"])
        .assert()
        .success()
        .stdout(contains("Removed `httpx`"))
        .stdout(contains("[project].dependencies"));

    let pyproject = fs::read_to_string(project_root.join("pyproject.toml")).expect("pyproject");
    assert!(pyproject.contains("dependencies = [\"attrs==25.1.0\"]"));
    assert!(pyproject.contains("docs = [\"httpx==0.27.0\"]"));
}

#[test]
fn remove_json_contract_snapshot_for_base_dependency_mutation() {
    let home = temp_env_root();
    let index = start_fixture_index();
    let project_root = home.path().join("workspace").join("sample-remove-json");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-remove-json"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["attrs==25.1.0", "httpx==0.27.0"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    let output = base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["--json", "remove", "httpx"])
        .output()
        .expect("remove json output");

    assert_json_contract(
        &output,
        json!({
            "status": "success",
            "exit": {
                "code": 0,
                "category": "success"
            },
            "output": {
                "blocks": [
                    {
                        "type": "message",
                        "value": {
                            "tone": "success",
                            "summary": "Removed `httpx` from `[project].dependencies`.",
                            "detail": "Updated `pyproject.toml`, refreshed `pylock.toml`, and reconciled the centralized environment.",
                            "hint": Value::Null,
                            "verbose": []
                        }
                    }
                ]
            },
            "error": Value::Null
        }),
    );
}

#[test]
fn remove_updates_dependency_group_in_pyproject_only() {
    let home = temp_env_root();
    let index = start_fixture_index();
    let project_root = home.path().join("workspace").join("sample-remove-group");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-remove-group"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["pytest==8.3.0"]

[dependency-groups]
dev = ["pytest==8.3.0", "pluggy==1.5.0"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["remove", "pytest", "--group", "dev"])
        .assert()
        .success()
        .stdout(contains("dependency group `dev`"));

    let pyproject = fs::read_to_string(project_root.join("pyproject.toml")).expect("pyproject");
    assert!(pyproject.contains("dependencies = [\"pytest==8.3.0\"]"));
    assert_eq!(pyproject.matches("pytest==8.3.0").count(), 1);
    assert_eq!(pyproject.matches("pluggy==1.5.0").count(), 1);
}

#[test]
fn remove_updates_extra_in_pyproject_only() {
    let home = temp_env_root();
    let index = start_fixture_index();
    let project_root = home.path().join("workspace").join("sample-remove-extra");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-remove-extra"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["httpx==0.27.0"]

[project.optional-dependencies]
cli-tools = ["httpx==0.27.0", "attrs==25.1.0"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["remove", "httpx", "--extra", "cli-tools"])
        .assert()
        .success()
        .stdout(contains("extra `cli-tools`"));

    let pyproject = fs::read_to_string(project_root.join("pyproject.toml")).expect("pyproject");
    assert!(pyproject.contains("dependencies = [\"httpx==0.27.0\"]"));
    assert_eq!(pyproject.matches("httpx==0.27.0").count(), 1);
    assert!(pyproject.contains("attrs==25.1.0"));
}

#[test]
fn remove_fails_when_dependency_is_missing_from_selected_scope() {
    let home = temp_env_root();
    let project_root = home.path().join("workspace").join("sample-remove-missing");
    fs::create_dir_all(&project_root).expect("project root");
    let original_pyproject = r#"[project]
name = "sample-remove-missing"
version = "0.1.0"
dependencies = ["attrs==25.1.0"]
"#;
    fs::write(project_root.join("pyproject.toml"), original_pyproject).expect("pyproject");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .args(["remove", "httpx"])
        .assert()
        .failure()
        .stderr(contains("Dependency `httpx` is not declared"))
        .stderr(contains("[project].dependencies"));

    let pyproject = fs::read_to_string(project_root.join("pyproject.toml")).expect("pyproject");
    assert_eq!(pyproject, original_pyproject);
    assert!(!project_root.join("pylock.toml").exists());
}

#[test]
fn remove_updates_lockfile_and_cleans_up_environment() {
    let home = temp_env_root();
    let index = start_fixture_index();
    let project_root = home.path().join("workspace").join("sample-remove-sync");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-remove-sync"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["attrs==25.1.0", "httpx==0.27.0"]

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    seed_managed_install(&home, "3.13.12").expect("managed install");
    let state_path = home.path().join("installer-state.json");

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["sync"])
        .assert()
        .success();

    base_command(&home, &state_path)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["remove", "httpx"])
        .assert()
        .success()
        .stdout(contains("reconciled the centralized environment"));

    let lock = fs::read_to_string(project_root.join("pylock.toml")).expect("pylock");
    assert!(lock.contains("name = \"attrs\""));
    assert!(!lock.contains("name = \"httpx\""));
    assert!(!lock.contains("name = \"anyio\""));

    let state = read_state(&state_path);
    assert_eq!(state.get("attrs"), Some(&"25.1.0".to_string()));
    assert!(!state.contains_key("httpx"));
    assert!(!state.contains_key("anyio"));
}

fn base_command(home: &TempDir, state_path: &Path) -> Command {
    let mut command = Command::cargo_bin("pyra").expect("pyra binary");
    command
        .env("PYRA_CONFIG_DIR", home.path().join("config"))
        .env("PYRA_DATA_DIR", home.path().join("data"))
        .env("PYRA_CACHE_DIR", home.path().join("cache"))
        .env("PYRA_STATE_DIR", home.path().join("state"))
        .env("PYRA_SYNC_INSTALLER_STATE_PATH", state_path);
    command
}

fn temp_env_root() -> TempDir {
    tempfile::tempdir().expect("temporary directory")
}

fn seed_managed_install(home: &TempDir, version: &str) -> Result<(), Box<dyn std::error::Error>> {
    seed_managed_install_with_executable(home, version, &system_python()?)
}

fn seed_managed_install_with_executable(
    home: &TempDir,
    version: &str,
    executable_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let install_dir = home.path().join("data").join("pythons").join(version);
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
        install_dir: camino::Utf8PathBuf::from_path_buf(install_dir.clone())
            .expect("utf-8 install dir"),
        executable_path: camino::Utf8PathBuf::from_path_buf(executable_path.to_path_buf())
            .expect("utf-8 python path"),
    };

    fs::write(
        install_dir.join("installation.json"),
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

    Err("no usable system python was found for integration tests".into())
}

fn system_python_version() -> Result<String, Box<dyn std::error::Error>> {
    let output = ProcessCommand::new(system_python()?)
        .args([
            "-c",
            "import sys; print('.'.join(map(str, sys.version_info[:3])))",
        ])
        .output()?;
    if !output.status.success() {
        return Err("failed to determine system python version".into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

fn requires_python_series(version: &str) -> String {
    let mut parts = version.split('.');
    let major = parts.next().expect("python major version");
    let minor = parts.next().expect("python minor version");
    format!("=={major}.{minor}.*")
}

fn create_virtualenv(interpreter: &Path, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let output = ProcessCommand::new(interpreter)
        .args(["-m", "venv"])
        .arg(path)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "failed to create virtualenv: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(())
}

fn venv_python_path(path: &Path) -> PathBuf {
    if cfg!(windows) {
        path.join("Scripts").join("python.exe")
    } else {
        path.join("bin").join("python")
    }
}

#[cfg(unix)]
fn build_fake_managed_python(
    root: &Path,
    log_path: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let runner_path = root.join("fake-python-runner.py");
    fs::write(
        &runner_path,
        r##"import json
import os
import pathlib
import sys

log_path = pathlib.Path(os.environ["PYRA_FAKE_PYTHON_LOG"])

def log(entry):
    with log_path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(entry) + "\n")

args = sys.argv[1:]
if args[:1] == ["-c"]:
    log({"kind": "inspect", "args": args})
    sys.stdout.write("[]")
    raise SystemExit(0)

if args[:3] == ["-m", "venv", "--clear"] and len(args) == 4:
    import shutil

    target = pathlib.Path(args[3])
    if target.exists():
        shutil.rmtree(target)
    target.mkdir(parents=True, exist_ok=True)
    bin_dir = target / "bin"
    bin_dir.mkdir(parents=True, exist_ok=True)
    python_path = bin_dir / "python"
    python_path.write_text(
        "#!/bin/sh\nexec \"{}\" \"{}\" \"$@\"\n".format(
            sys.executable,
            pathlib.Path(__file__),
        ),
        encoding="utf-8",
    )
    python_path.chmod(0o755)
    log({"kind": "venv", "target": str(target)})
    raise SystemExit(0)

if args[:4] == ["-m", "pip", "install", "--no-deps"] and len(args) == 5:
    target = args[4]
    log({"kind": "install", "target": target, "exists": pathlib.Path(target).exists()})
    raise SystemExit(0)

if args[:4] == ["-m", "pip", "uninstall", "-y"] and len(args) == 5:
    log({"kind": "uninstall", "target": args[4]})
    raise SystemExit(0)

raise SystemExit(f"unexpected fake interpreter args: {args}")
"##,
    )?;

    let wrapper_path = root.join("fake-python");
    let system_python = system_python()?;
    fs::write(
        &wrapper_path,
        format!(
            "#!/bin/sh\nexec \"{}\" \"{}\" \"$@\"\n",
            system_python.display(),
            runner_path.display()
        ),
    )?;
    let mut permissions = fs::metadata(&wrapper_path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&wrapper_path, permissions)?;

    let _ = fs::remove_file(log_path);
    Ok(wrapper_path)
}

#[cfg(unix)]
fn fake_python_install_target(log_path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    fake_python_install_targets(log_path)?
        .into_iter()
        .next()
        .ok_or_else(|| "missing install log entry".into())
}

#[cfg(unix)]
fn fake_python_install_targets(log_path: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let contents = fs::read_to_string(log_path)?;
    let mut targets = Vec::new();
    for line in contents.lines() {
        let entry: serde_json::Value = serde_json::from_str(line)?;
        if entry.get("kind") == Some(&serde_json::Value::String("install".to_string())) {
            assert_eq!(entry.get("exists"), Some(&serde_json::Value::Bool(true)));
            let target = entry
                .get("target")
                .and_then(serde_json::Value::as_str)
                .ok_or("missing install target")?;
            targets.push(target.to_string());
        }
    }
    Ok(targets)
}

fn read_state(path: &Path) -> std::collections::BTreeMap<String, String> {
    serde_json::from_slice(&fs::read(path).expect("state")).expect("state json")
}

fn assert_json_contract(output: &std::process::Output, expected: Value) {
    assert!(
        output.status.success(),
        "command should complete and return machine-readable output"
    );
    assert!(
        output.stderr.is_empty(),
        "json mode should not emit stderr on successful command paths"
    );

    let stdout = String::from_utf8(output.stdout.clone()).expect("stdout utf-8");
    let mut actual: Value = serde_json::from_str(&stdout).expect("json envelope");
    let mut expected = expected;
    strip_verbose_lines(&mut actual);
    strip_verbose_lines(&mut expected);
    assert_eq!(actual, expected);
}

fn strip_verbose_lines(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if let Some(verbose) = map.get_mut("verbose") {
                *verbose = json!([]);
            }
            for child in map.values_mut() {
                strip_verbose_lines(child);
            }
        }
        Value::Array(items) => {
            for item in items {
                strip_verbose_lines(item);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn project_id_for(project_root: &Path) -> String {
    let canonical = fs::canonicalize(project_root).expect("canonical project root");
    format!("{:x}", Sha256::digest(canonical.to_string_lossy().as_bytes()))
}

#[cfg(unix)]
fn host_and_foreign_targets() -> Result<(String, String), Box<dyn std::error::Error>> {
    let host = HostTarget::detect()?.target_triple().to_string();
    let foreign = match host.as_str() {
        "aarch64-apple-darwin" => "x86_64-unknown-linux-gnu",
        "x86_64-apple-darwin" => "x86_64-unknown-linux-gnu",
        "x86_64-unknown-linux-gnu" => "x86_64-apple-darwin",
        "aarch64-unknown-linux-gnu" => "aarch64-apple-darwin",
        other => return Err(format!("unsupported host target for test: {other}").into()),
    };
    Ok((host, foreign.to_string()))
}

#[cfg(unix)]
fn wheel_name_for_target(package: &str, version: &str, target: &str) -> String {
    let platform_tag = match target {
        "aarch64-apple-darwin" => "macosx_11_0_arm64",
        "x86_64-apple-darwin" => "macosx_11_0_x86_64",
        "x86_64-unknown-linux-gnu" => "manylinux_2_17_x86_64",
        "aarch64-unknown-linux-gnu" => "manylinux_2_17_aarch64",
        other => panic!("unsupported target triple for test wheel: {other}"),
    };
    format!("{package}-{version}-cp313-abi3-{platform_tag}.whl")
}

#[cfg(unix)]
fn lock_environment_id(version: &str, target: &str) -> String {
    format!("cpython-{version}-{target}")
}

#[cfg(unix)]
fn lock_environment_marker(version: &str, target: &str) -> String {
    let (sys_platform, platform_machine) = match target {
        "aarch64-apple-darwin" => ("darwin", "arm64"),
        "x86_64-apple-darwin" => ("darwin", "x86_64"),
        "x86_64-unknown-linux-gnu" => ("linux", "x86_64"),
        "aarch64-unknown-linux-gnu" => ("linux", "aarch64"),
        other => panic!("unsupported target triple for test environment: {other}"),
    };
    format!(
        "implementation_name == 'cpython' and python_full_version == '{version}' and sys_platform == '{sys_platform}' and platform_machine == '{platform_machine}'"
    )
}

struct FixtureIndex {
    base_url: String,
    _root: TempDir,
}

struct InstallableFixtureIndex {
    base_url: String,
    artifact_path: PathBuf,
    artifact_name: String,
    artifact_sha256: String,
    _root: TempDir,
}

fn start_fixture_index() -> FixtureIndex {
    let root = tempfile::tempdir().expect("fixture root");
    for file in [
        "attrs-25.1.0-py3-none-any.whl",
        "pytest-8.3.0-py3-none-any.whl",
        "pluggy-1.5.0-py3-none-any.whl",
        "sphinx-7.0.0-py3-none-any.whl",
        "httpx-0.27.0-py3-none-any.whl",
        "anyio-4.4.0-py3-none-any.whl",
        "click-8.1.7-py3-none-any.whl",
        "urllib3-2.2.1-py3-none-any.whl",
        "requests-2.32.3-py3-none-any.whl",
        "pysocks-1.7.1-py3-none-any.whl",
        "shared-1.5.0-py3-none-any.whl",
        "sdistonly-1.2.3.tar.gz",
    ] {
        write_fixture_bytes(
            root.path(),
            &format!("files/{file}"),
            &fixture_artifact_bytes(file),
        );
    }
    for package in [
        "attrs",
        "pytest",
        "pluggy",
        "sphinx",
        "httpx",
        "anyio",
        "click",
        "urllib3",
        "requests",
        "pysocks",
        "shared",
        "sdistonly",
    ] {
        write_fixture_file(
            root.path(),
            &format!("{package}.json"),
            fixture_project_json(package, root.path()),
        );
    }
    for metadata in [
        "attrs-25.1.0-py3-none-any.whl.metadata",
        "pytest-8.3.0-py3-none-any.whl.metadata",
        "pluggy-1.5.0-py3-none-any.whl.metadata",
        "sphinx-7.0.0-py3-none-any.whl.metadata",
        "httpx-0.27.0-py3-none-any.whl.metadata",
        "anyio-4.4.0-py3-none-any.whl.metadata",
        "click-8.1.7-py3-none-any.whl.metadata",
        "urllib3-2.2.1-py3-none-any.whl.metadata",
        "requests-2.32.3-py3-none-any.whl.metadata",
        "pysocks-1.7.1-py3-none-any.whl.metadata",
        "shared-1.5.0-py3-none-any.whl.metadata",
        "sdistonly-1.2.3.tar.gz.metadata",
    ] {
        write_fixture_file(
            root.path(),
            &format!("files/{metadata}"),
            fixture_metadata(metadata),
        );
    }

    FixtureIndex {
        base_url: format!("file://{}", root.path().to_string_lossy()),
        _root: root,
    }
}

fn start_conflict_fixture_index() -> FixtureIndex {
    let root = tempfile::tempdir().expect("fixture root");
    for file in [
        "alpha-1.0.0-py3-none-any.whl",
        "bravo-1.0.0-py3-none-any.whl",
        "shared-1.5.0-py3-none-any.whl",
        "shared-2.0.0-py3-none-any.whl",
    ] {
        write_fixture_bytes(
            root.path(),
            &format!("files/{file}"),
            &fixture_artifact_bytes(file),
        );
    }
    for package in ["alpha", "bravo", "shared"] {
        write_fixture_file(
            root.path(),
            &format!("{package}.json"),
            conflict_fixture_project_json(package, root.path()),
        );
    }
    for metadata in [
        "alpha-1.0.0-py3-none-any.whl.metadata",
        "bravo-1.0.0-py3-none-any.whl.metadata",
        "shared-1.5.0-py3-none-any.whl.metadata",
        "shared-2.0.0-py3-none-any.whl.metadata",
    ] {
        write_fixture_file(
            root.path(),
            &format!("files/{metadata}"),
            fixture_metadata(metadata),
        );
    }

    FixtureIndex {
        base_url: format!("file://{}", root.path().to_string_lossy()),
        _root: root,
    }
}

fn start_installable_fixture_index(
    package: &str,
    version: &str,
) -> Result<InstallableFixtureIndex, Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let files_dir = root.path().join("files");
    fs::create_dir_all(&files_dir)?;
    let artifact_path = build_installable_wheel(&files_dir, package, version)?;
    let artifact_name = artifact_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("installable artifact name must be valid utf-8")?
        .to_string();
    let artifact_sha256 = format!("{:x}", Sha256::digest(fs::read(&artifact_path)?));

    write_fixture_file(
        root.path(),
        &format!("{package}.json"),
        serde_json::json!({
            "files": [{
                "filename": artifact_name,
                "url": format!("file://{}", artifact_path.display()),
                "hashes": {"sha256": artifact_sha256},
                "core-metadata": true
            }]
        })
        .to_string(),
    );
    write_fixture_file(
        root.path(),
        &format!("files/{artifact_name}.metadata"),
        installable_fixture_metadata(package, version),
    );

    Ok(InstallableFixtureIndex {
        base_url: format!("file://{}", root.path().to_string_lossy()),
        artifact_path,
        artifact_name,
        artifact_sha256,
        _root: root,
    })
}

fn write_fixture_file(root: &Path, relative: &str, contents: String) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("fixture parent");
    }
    fs::write(path, contents).expect("fixture file");
}

fn write_fixture_bytes(root: &Path, relative: &str, contents: &[u8]) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("fixture parent");
    }
    fs::write(path, contents).expect("fixture bytes");
}

fn fixture_project_json(package: &str, root: &Path) -> String {
    let file = match package {
        "attrs" => "attrs-25.1.0-py3-none-any.whl",
        "pytest" => "pytest-8.3.0-py3-none-any.whl",
        "pluggy" => "pluggy-1.5.0-py3-none-any.whl",
        "sphinx" => "sphinx-7.0.0-py3-none-any.whl",
        "httpx" => "httpx-0.27.0-py3-none-any.whl",
        "anyio" => "anyio-4.4.0-py3-none-any.whl",
        "click" => "click-8.1.7-py3-none-any.whl",
        "urllib3" => "urllib3-2.2.1-py3-none-any.whl",
        "requests" => "requests-2.32.3-py3-none-any.whl",
        "pysocks" => "pysocks-1.7.1-py3-none-any.whl",
        "shared" => "shared-1.5.0-py3-none-any.whl",
        "sdistonly" => "sdistonly-1.2.3.tar.gz",
        other => panic!("unexpected package {other}"),
    };
    serde_json::json!({
        "files": [{
            "filename": file,
            "url": format!("file://{}", root.join("files").join(file).display()),
            "hashes": {"sha256": format!("{:x}", Sha256::digest(&fixture_artifact_bytes(file)))},
            "core-metadata": true
        }]
    })
    .to_string()
}

fn conflict_fixture_project_json(package: &str, root: &Path) -> String {
    let files = match package {
        "alpha" => vec!["alpha-1.0.0-py3-none-any.whl"],
        "bravo" => vec!["bravo-1.0.0-py3-none-any.whl"],
        "shared" => vec![
            "shared-1.5.0-py3-none-any.whl",
            "shared-2.0.0-py3-none-any.whl",
        ],
        other => panic!("unexpected conflict package {other}"),
    };
    serde_json::json!({
        "files": files.into_iter().map(|file| serde_json::json!({
            "filename": file,
            "url": format!("file://{}", root.join("files").join(file).display()),
            "hashes": {"sha256": format!("{:x}", Sha256::digest(&fixture_artifact_bytes(file)))},
            "core-metadata": true
        })).collect::<Vec<_>>()
    })
    .to_string()
}

fn fixture_artifact_bytes(filename: &str) -> Vec<u8> {
    format!("pyra fixture artifact: {filename}\n").into_bytes()
}

fn installable_fixture_metadata(package: &str, version: &str) -> String {
    format!(
        "Metadata-Version: 2.1\nName: {package}\nVersion: {version}\nSummary: Pyra installable fixture\n"
    )
}

fn fixture_metadata(filename: &str) -> String {
    match filename {
        "attrs-25.1.0-py3-none-any.whl.metadata" => {
            "Metadata-Version: 2.3\nName: attrs\nVersion: 25.1.0\n".to_string()
        }
        "pytest-8.3.0-py3-none-any.whl.metadata" => {
            "Metadata-Version: 2.3\nName: pytest\nVersion: 8.3.0\nRequires-Dist: pluggy==1.5.0\n"
                .to_string()
        }
        "pluggy-1.5.0-py3-none-any.whl.metadata" => {
            "Metadata-Version: 2.3\nName: pluggy\nVersion: 1.5.0\n".to_string()
        }
        "sphinx-7.0.0-py3-none-any.whl.metadata" => {
            "Metadata-Version: 2.3\nName: sphinx\nVersion: 7.0.0\n".to_string()
        }
        "httpx-0.27.0-py3-none-any.whl.metadata" => {
            "Metadata-Version: 2.3\nName: httpx\nVersion: 0.27.0\nRequires-Dist: anyio==4.4.0\n"
                .to_string()
        }
        "anyio-4.4.0-py3-none-any.whl.metadata" => {
            "Metadata-Version: 2.3\nName: anyio\nVersion: 4.4.0\n".to_string()
        }
        "click-8.1.7-py3-none-any.whl.metadata" => {
            "Metadata-Version: 2.3\nName: click\nVersion: 8.1.7\n".to_string()
        }
        "urllib3-2.2.1-py3-none-any.whl.metadata" => {
            "Metadata-Version: 2.3\nName: urllib3\nVersion: 2.2.1\n".to_string()
        }
        "requests-2.32.3-py3-none-any.whl.metadata" => {
            "Metadata-Version: 2.3\nName: requests\nVersion: 2.32.3\nRequires-Dist: urllib3==2.2.1\nRequires-Dist: pysocks==1.7.1; extra == 'socks'\n"
                .to_string()
        }
        "pysocks-1.7.1-py3-none-any.whl.metadata" => {
            "Metadata-Version: 2.3\nName: pysocks\nVersion: 1.7.1\n".to_string()
        }
        "shared-1.5.0-py3-none-any.whl.metadata" => {
            "Metadata-Version: 2.3\nName: shared\nVersion: 1.5.0\n".to_string()
        }
        "sdistonly-1.2.3.tar.gz.metadata" => {
            "Metadata-Version: 2.3\nName: sdistonly\nVersion: 1.2.3\nRequires-Dist: shared==1.5.0\n"
                .to_string()
        }
        "alpha-1.0.0-py3-none-any.whl.metadata" => {
            "Metadata-Version: 2.3\nName: alpha\nVersion: 1.0.0\nRequires-Dist: shared<2\n"
                .to_string()
        }
        "bravo-1.0.0-py3-none-any.whl.metadata" => {
            "Metadata-Version: 2.3\nName: bravo\nVersion: 1.0.0\nRequires-Dist: shared>=2\n"
                .to_string()
        }
        "shared-2.0.0-py3-none-any.whl.metadata" => {
            "Metadata-Version: 2.3\nName: shared\nVersion: 2.0.0\n".to_string()
        }
        other => panic!("unexpected metadata request {other}"),
    }
}

fn build_installable_wheel(
    output_dir: &Path,
    package: &str,
    version: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let script = r#"
import base64
import hashlib
import pathlib
import sys
import zipfile

output_dir = pathlib.Path(sys.argv[1])
package = sys.argv[2]
version = sys.argv[3]
dist = package.replace("-", "_")
dist_info = f"{dist}-{version}.dist-info"
wheel_name = f"{dist}-{version}-py3-none-any.whl"
wheel_path = output_dir / wheel_name

metadata = (
    f"Metadata-Version: 2.1\n"
    f"Name: {package}\n"
    f"Version: {version}\n"
    f"Summary: Pyra installable fixture\n"
)
wheel = (
    "Wheel-Version: 1.0\n"
    "Generator: pyra integration test\n"
    "Root-Is-Purelib: true\n"
    "Tag: py3-none-any\n"
)
module_init = f"__version__ = '{version}'\n"

records = []

def encode_record(data):
    digest = base64.urlsafe_b64encode(hashlib.sha256(data).digest()).rstrip(b"=").decode("ascii")
    return f"sha256={digest}", str(len(data))

with zipfile.ZipFile(wheel_path, "w", compression=zipfile.ZIP_DEFLATED) as wheel_file:
    def write_file(path, data):
        wheel_file.writestr(path, data)
        records.append((path, *encode_record(data)))

    write_file(f"{dist}/__init__.py", module_init.encode("utf-8"))
    write_file(f"{dist_info}/METADATA", metadata.encode("utf-8"))
    write_file(f"{dist_info}/WHEEL", wheel.encode("utf-8"))
    record_path = f"{dist_info}/RECORD"
    record_body = "".join(
        f"{path},{digest},{size}\n" for path, digest, size in records
    ) + f"{record_path},,\n"
    wheel_file.writestr(record_path, record_body.encode("utf-8"))

print(wheel_path)
"#;

    let output = ProcessCommand::new(system_python()?)
        .args(["-c", script])
        .arg(output_dir)
        .arg(package)
        .arg(version)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "failed to build installable wheel fixture: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }

    Ok(PathBuf::from(String::from_utf8(output.stdout)?.trim()))
}
