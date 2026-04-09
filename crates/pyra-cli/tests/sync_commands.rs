use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use assert_cmd::Command;
use predicates::str::contains;
use pyra_python::{ArchiveFormat, HostTarget, InstalledPythonRecord, PythonVersion};
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
    assert!(first_lock.contains("resolution-strategy = \"current-platform-union-v1\""));

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
        "resolution-strategy = \"current-platform-union-v1\"",
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
    assert!(regenerated_lock.contains("resolution-strategy = \"current-platform-union-v1\""));
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
        r#"import json
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
"#,
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
    let contents = fs::read_to_string(log_path)?;
    for line in contents.lines() {
        let entry: serde_json::Value = serde_json::from_str(line)?;
        if entry.get("kind") == Some(&serde_json::Value::String("install".to_string())) {
            assert_eq!(entry.get("exists"), Some(&serde_json::Value::Bool(true)));
            return entry
                .get("target")
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string)
                .ok_or_else(|| "missing install target".into());
        }
    }
    Err("missing install log entry".into())
}

fn read_state(path: &Path) -> std::collections::BTreeMap<String, String> {
    serde_json::from_slice(&fs::read(path).expect("state")).expect("state json")
}

struct FixtureIndex {
    base_url: String,
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
    ] {
        write_fixture_bytes(
            root.path(),
            &format!("files/{file}"),
            &fixture_artifact_bytes(file),
        );
    }
    write_fixture_file(
        root.path(),
        "attrs.json",
        fixture_project_json("attrs", root.path()),
    );
    write_fixture_file(
        root.path(),
        "pytest.json",
        fixture_project_json("pytest", root.path()),
    );
    write_fixture_file(
        root.path(),
        "pluggy.json",
        fixture_project_json("pluggy", root.path()),
    );
    write_fixture_file(
        root.path(),
        "sphinx.json",
        fixture_project_json("sphinx", root.path()),
    );
    write_fixture_file(
        root.path(),
        "httpx.json",
        fixture_project_json("httpx", root.path()),
    );
    write_fixture_file(
        root.path(),
        "anyio.json",
        fixture_project_json("anyio", root.path()),
    );
    write_fixture_file(
        root.path(),
        "files/attrs-25.1.0-py3-none-any.whl.metadata",
        fixture_metadata("attrs-25.1.0-py3-none-any.whl.metadata"),
    );
    write_fixture_file(
        root.path(),
        "files/pytest-8.3.0-py3-none-any.whl.metadata",
        fixture_metadata("pytest-8.3.0-py3-none-any.whl.metadata"),
    );
    write_fixture_file(
        root.path(),
        "files/pluggy-1.5.0-py3-none-any.whl.metadata",
        fixture_metadata("pluggy-1.5.0-py3-none-any.whl.metadata"),
    );
    write_fixture_file(
        root.path(),
        "files/sphinx-7.0.0-py3-none-any.whl.metadata",
        fixture_metadata("sphinx-7.0.0-py3-none-any.whl.metadata"),
    );
    write_fixture_file(
        root.path(),
        "files/httpx-0.27.0-py3-none-any.whl.metadata",
        fixture_metadata("httpx-0.27.0-py3-none-any.whl.metadata"),
    );
    write_fixture_file(
        root.path(),
        "files/anyio-4.4.0-py3-none-any.whl.metadata",
        fixture_metadata("anyio-4.4.0-py3-none-any.whl.metadata"),
    );

    FixtureIndex {
        base_url: format!("file://{}", root.path().to_string_lossy()),
        _root: root,
    }
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

fn fixture_artifact_bytes(filename: &str) -> Vec<u8> {
    format!("pyra fixture artifact: {filename}\n").into_bytes()
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
        other => panic!("unexpected metadata request {other}"),
    }
}
