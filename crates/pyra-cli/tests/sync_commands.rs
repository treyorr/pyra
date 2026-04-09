use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use assert_cmd::Command;
use predicates::str::contains;
use pyra_python::{ArchiveFormat, HostTarget, InstalledPythonRecord, PythonVersion};
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
        executable_path: camino::Utf8PathBuf::from_path_buf(system_python()?)
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

fn read_state(path: &Path) -> std::collections::BTreeMap<String, String> {
    serde_json::from_slice(&fs::read(path).expect("state")).expect("state json")
}

struct FixtureIndex {
    base_url: String,
    _root: TempDir,
}

fn start_fixture_index() -> FixtureIndex {
    let root = tempfile::tempdir().expect("fixture root");
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
            "hashes": {"sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
            "core-metadata": true
        }]
    })
    .to_string()
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
