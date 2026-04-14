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
fn run_prefers_project_scripts_over_environment_console_scripts() {
    let home = temp_env_root();
    let project_root = home
        .path()
        .join("workspace")
        .join("sample-run-project-script");
    fs::create_dir_all(&project_root).expect("project root");
    let python_version = system_python_version().expect("system python version");
    fs::write(
        project_root.join("pyproject.toml"),
        format!(
            r#"[project]
name = "sample-run-project-script"
version = "0.1.0"
dependencies = []

[project.scripts]
demo = "app:main"

[tool.pyra]
python = "{python_version}"
"#,
        ),
    )
    .expect("pyproject");
    fs::write(
        project_root.join("app.py"),
        "def main():\n    print('project script')\n    return 0\n",
    )
    .expect("app module");

    seed_managed_install(&home, &python_version).expect("managed install");

    base_command(&home)
        .current_dir(&project_root)
        .args(["sync"])
        .assert()
        .success();

    write_conflicting_console_script(home.path(), &project_root, "demo", "console script")
        .expect("console script");
    fs::remove_file(project_root.join("pylock.toml")).expect("remove lock");

    base_command(&home)
        .current_dir(&project_root)
        .args(["run", "demo"])
        .assert()
        .success()
        .stdout(contains("project script"));
}

#[test]
fn run_uses_console_scripts_from_the_synchronized_environment() {
    let home = temp_env_root();
    let index = start_fixture_index();
    let project_root = home
        .path()
        .join("workspace")
        .join("sample-run-console-script");
    fs::create_dir_all(&project_root).expect("project root");
    let python_version = system_python_version().expect("system python version");
    fs::write(
        project_root.join("pyproject.toml"),
        format!(
            r#"[project]
name = "sample-run-console-script"
version = "0.1.0"
dependencies = ["hello-tool==1.0.0"]

[tool.pyra]
python = "{python_version}"
"#,
        ),
    )
    .expect("pyproject");

    seed_managed_install(&home, &python_version).expect("managed install");

    base_command(&home)
        .current_dir(&project_root)
        .env("PYRA_INDEX_URL", &index.base_url)
        .args(["run", "hello-tool"])
        .assert()
        .success()
        .stdout(contains("hello from console script"));
}

#[test]
fn run_falls_back_to_python_file_execution() {
    let home = temp_env_root();
    let project_root = home.path().join("workspace").join("sample-run-python-file");
    fs::create_dir_all(&project_root).expect("project root");
    let python_version = system_python_version().expect("system python version");
    fs::write(
        project_root.join("pyproject.toml"),
        format!(
            r#"[project]
name = "sample-run-python-file"
version = "0.1.0"
dependencies = []

[tool.pyra]
python = "{python_version}"
"#,
        ),
    )
    .expect("pyproject");
    fs::write(
        project_root.join("hello.py"),
        "print('hello from python file')\n",
    )
    .expect("python file");

    seed_managed_install(&home, &python_version).expect("managed install");

    base_command(&home)
        .current_dir(&project_root)
        .args(["run", "hello.py"])
        .assert()
        .success()
        .stdout(contains("hello from python file"));
}

#[test]
fn run_passes_arguments_through_to_project_scripts() {
    let home = temp_env_root();
    let project_root = home
        .path()
        .join("workspace")
        .join("sample-run-project-script-args");
    fs::create_dir_all(&project_root).expect("project root");
    let python_version = system_python_version().expect("system python version");
    fs::write(
        project_root.join("pyproject.toml"),
        format!(
            r#"[project]
name = "sample-run-project-script-args"
version = "0.1.0"
dependencies = []

[project.scripts]
demo = "app:main"

[tool.pyra]
python = "{python_version}"
"#,
        ),
    )
    .expect("pyproject");
    fs::write(
        project_root.join("app.py"),
        "import json\nimport sys\n\ndef main():\n    print(json.dumps(sys.argv))\n    return 0\n",
    )
    .expect("app module");

    seed_managed_install(&home, &python_version).expect("managed install");

    base_command(&home)
        .current_dir(&project_root)
        .args(["run", "demo", "alpha", "--flag", "-x"])
        .assert()
        .success()
        .stdout(contains("[\"demo\", \"alpha\", \"--flag\", \"-x\"]"));
}

#[test]
fn run_syncs_the_project_before_execution() {
    let home = temp_env_root();
    let project_root = home.path().join("workspace").join("sample-run-sync-first");
    fs::create_dir_all(&project_root).expect("project root");
    let python_version = system_python_version().expect("system python version");
    fs::write(
        project_root.join("pyproject.toml"),
        format!(
            r#"[project]
name = "sample-run-sync-first"
version = "0.1.0"
dependencies = []

[tool.pyra]
python = "{python_version}"
"#,
        ),
    )
    .expect("pyproject");
    fs::write(
        project_root.join("hello.py"),
        "print('synced before run')\n",
    )
    .expect("python file");

    seed_managed_install(&home, &python_version).expect("managed install");
    assert!(!project_root.join("pylock.toml").exists());

    base_command(&home)
        .current_dir(&project_root)
        .args(["run", "hello.py"])
        .assert()
        .success()
        .stdout(contains("synced before run"));

    assert!(project_root.join("pylock.toml").exists());
    assert_environment_prepared(home.path(), &project_root);
}

#[test]
fn run_blocks_nested_pip_install_mutations() {
    let home = temp_env_root();
    let project_root = home.path().join("workspace").join("sample-run-pip-guard");
    fs::create_dir_all(&project_root).expect("project root");
    let python_version = system_python_version().expect("system python version");
    fs::write(
        project_root.join("pyproject.toml"),
        format!(
            r#"[project]
name = "sample-run-pip-guard"
version = "0.1.0"
dependencies = []

[tool.pyra]
python = "{python_version}"
"#,
        ),
    )
    .expect("pyproject");

    seed_managed_install(&home, &python_version).expect("managed install");

    let nested_install = base_command(&home)
        .current_dir(&project_root)
        .args([
            "run",
            "python",
            "-c",
            "import subprocess, sys; subprocess.check_call([sys.executable, '-m', 'pip', 'install', 'attrs==25.1.0'])",
        ])
        .assert()
        .failure();
    let nested_install_stderr = String::from_utf8_lossy(&nested_install.get_output().stderr);
    assert!(nested_install_stderr.contains("blocked ad hoc pip mutation during `pyra run`"));
    assert!(!nested_install_stderr.contains("Fatal Python error"));
    assert!(!nested_install_stderr.contains("sitecustomize"));

    let direct_install = base_command(&home)
        .current_dir(&project_root)
        .args(["run", "pip", "install", "attrs==25.1.0"])
        .assert()
        .failure();
    let direct_install_stderr = String::from_utf8_lossy(&direct_install.get_output().stderr);
    assert!(direct_install_stderr.contains("blocked ad hoc pip mutation during `pyra run`"));
    assert!(!direct_install_stderr.contains("Fatal Python error"));
    assert!(!direct_install_stderr.contains("sitecustomize"));

    base_command(&home)
        .current_dir(&project_root)
        .args(["run", "python", "-c", "print('run still works')"])
        .assert()
        .success()
        .stdout(contains("run still works"));
}

#[test]
fn run_preserves_child_exit_codes() {
    let home = temp_env_root();
    let project_root = home.path().join("workspace").join("sample-run-exit-code");
    fs::create_dir_all(&project_root).expect("project root");
    let python_version = system_python_version().expect("system python version");
    fs::write(
        project_root.join("pyproject.toml"),
        format!(
            r#"[project]
name = "sample-run-exit-code"
version = "0.1.0"
dependencies = []

[tool.pyra]
python = "{python_version}"
"#,
        ),
    )
    .expect("pyproject");
    fs::write(project_root.join("fail.py"), "raise SystemExit(7)\n").expect("python file");

    seed_managed_install(&home, &python_version).expect("managed install");

    base_command(&home)
        .current_dir(&project_root)
        .args(["run", "fail.py"])
        .assert()
        .code(7);
}

fn base_command(home: &TempDir) -> Command {
    let mut command = Command::cargo_bin("pyra").expect("pyra binary");
    command
        .env("PYRA_CONFIG_DIR", home.path().join("config"))
        .env("PYRA_DATA_DIR", home.path().join("data"))
        .env("PYRA_CACHE_DIR", home.path().join("cache"))
        .env("PYRA_STATE_DIR", home.path().join("state"));
    command
}

fn temp_env_root() -> TempDir {
    tempfile::tempdir().expect("temporary directory")
}

fn seed_managed_install(home: &TempDir, version: &str) -> Result<(), Box<dyn std::error::Error>> {
    let managed_root = home.path().join("managed-python").join(version);
    create_virtualenv(&system_python()?, &managed_root)?;
    let executable_path = venv_python_path(&managed_root);
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
        executable_path: camino::Utf8PathBuf::from_path_buf(executable_path)
            .expect("utf-8 python path"),
    };

    fs::write(
        install_dir.join("installation.json"),
        serde_json::to_vec_pretty(&record)?,
    )?;

    Ok(())
}

fn assert_environment_prepared(home: &Path, project_root: &Path) {
    let environments_root = home.join("data").join("environments");
    let entries = fs::read_dir(&environments_root)
        .expect("environment directory")
        .collect::<Result<Vec<_>, _>>()
        .expect("environment entries");
    assert_eq!(entries.len(), 1);

    let metadata_path = entries[0].path().join("metadata.json");
    let metadata: serde_json::Value =
        serde_json::from_slice(&fs::read(&metadata_path).expect("metadata"))
            .expect("metadata json");
    let canonical_project_root = project_root
        .canonicalize()
        .expect("canonical project root")
        .display()
        .to_string();

    assert_eq!(
        metadata["project_root"].as_str(),
        Some(canonical_project_root.as_str())
    );
}

fn write_conflicting_console_script(
    home: &Path,
    project_root: &Path,
    name: &str,
    message: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let environment_path = locate_environment_path(home, project_root)?;
    let scripts_dir = if cfg!(windows) {
        environment_path.join("Scripts")
    } else {
        environment_path.join("bin")
    };
    fs::create_dir_all(&scripts_dir)?;

    #[cfg(windows)]
    {
        fs::write(
            scripts_dir.join(format!("{name}.cmd")),
            format!("@echo off\r\necho {message}\r\n"),
        )?;
    }
    #[cfg(unix)]
    {
        let script_path = scripts_dir.join(name);
        fs::write(&script_path, format!("#!/bin/sh\necho '{message}'\n"))?;
        let mut permissions = fs::metadata(&script_path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions)?;
    }

    Ok(())
}

fn locate_environment_path(
    home: &Path,
    project_root: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let environments_root = home.join("data").join("environments");
    for entry in fs::read_dir(&environments_root)? {
        let entry = entry?;
        let metadata_path = entry.path().join("metadata.json");
        if !metadata_path.exists() {
            continue;
        }

        let metadata: serde_json::Value = serde_json::from_slice(&fs::read(&metadata_path)?)?;
        let Some(stored_root) = metadata
            .get("project_root")
            .and_then(serde_json::Value::as_str)
        else {
            continue;
        };
        if Path::new(stored_root) == project_root.canonicalize()? {
            let environment_path = metadata
                .get("environment_path")
                .and_then(serde_json::Value::as_str)
                .ok_or("missing environment path")?;
            return Ok(PathBuf::from(environment_path));
        }
    }

    Err("missing environment path".into())
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

struct FixtureIndex {
    base_url: String,
    _root: TempDir,
}

fn start_fixture_index() -> FixtureIndex {
    let root = tempfile::tempdir().expect("fixture root");
    let wheel_name = "hello_tool-1.0.0-py3-none-any.whl";
    let wheel_path = root.path().join("files").join(wheel_name);
    if let Some(parent) = wheel_path.parent() {
        fs::create_dir_all(parent).expect("wheel parent");
    }
    write_console_script_wheel(&wheel_path).expect("console script wheel");
    let wheel_bytes = fs::read(&wheel_path).expect("wheel bytes");
    let metadata = fixture_metadata();
    write_fixture_file(root.path(), "hello-tool.json", {
        serde_json::json!({
            "files": [{
                "filename": wheel_name,
                "url": format!("file://{}", wheel_path.display()),
                "hashes": {"sha256": format!("{:x}", Sha256::digest(&wheel_bytes))},
                "core-metadata": true
            }]
        })
        .to_string()
    });
    write_fixture_file(
        root.path(),
        &format!("files/{wheel_name}.metadata"),
        metadata.to_string(),
    );

    FixtureIndex {
        base_url: format!("file://{}", root.path().to_string_lossy()),
        _root: root,
    }
}

fn write_console_script_wheel(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let script = r#"
import pathlib
import sys
import zipfile
import hashlib
import base64

wheel_path = pathlib.Path(sys.argv[1])
dist_info = "hello_tool-1.0.0.dist-info"
files = {
    "hello_tool.py": "def main():\n    print('hello from console script')\n    return 0\n",
    f"{dist_info}/METADATA": "Metadata-Version: 2.1\nName: hello-tool\nVersion: 1.0.0\n",
    f"{dist_info}/WHEEL": "Wheel-Version: 1.0\nGenerator: pyra-test\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
    f"{dist_info}/entry_points.txt": "[console_scripts]\nhello-tool = hello_tool:main\n",
}
records = []
with zipfile.ZipFile(wheel_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
    for name, text in files.items():
        data = text.encode("utf-8")
        archive.writestr(name, data)
        digest = base64.urlsafe_b64encode(hashlib.sha256(data).digest()).rstrip(b"=").decode("ascii")
        records.append((name, f"sha256={digest}", str(len(data))))
    record_name = f"{dist_info}/RECORD"
    record_lines = ["{},{},{}\n".format(*row) for row in records]
    record_lines.append(f"{record_name},,\n")
    archive.writestr(record_name, "".join(record_lines).encode("utf-8"))
"#;

    let output = ProcessCommand::new(system_python()?)
        .arg("-c")
        .arg(script)
        .arg(path)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "failed to build console script wheel: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }

    Ok(())
}

fn fixture_metadata() -> &'static str {
    "Metadata-Version: 2.1\nName: hello-tool\nVersion: 1.0.0\n"
}

fn write_fixture_file(root: &Path, relative: &str, contents: String) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("fixture parent");
    }
    fs::write(path, contents).expect("fixture file");
}
