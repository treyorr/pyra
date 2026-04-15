use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use assert_cmd::Command;
use pyra_python::{ArchiveFormat, HostTarget, InstalledPythonRecord, PythonVersion};
use serde_json::Value;
use tempfile::TempDir;

#[test]
fn major_project_commands_without_project_return_user_exit_category() {
    let home = temp_env_root();
    let workspace = home.path().join("workspace").join("no-project");
    fs::create_dir_all(&workspace).expect("workspace");

    let command_args = [
        vec!["use", "not-a-version"],
        vec!["add", "rich>=13"],
        vec!["remove", "rich"],
        vec!["sync"],
        vec!["lock"],
        vec!["doctor"],
        vec!["outdated"],
        vec!["update"],
        vec!["run", "missing.py"],
    ];

    for args in command_args {
        let output = base_command(&home)
            .current_dir(&workspace)
            .arg("--json")
            .args(&args)
            .output()
            .expect("command output");

        assert_json_exit(&output, "fail", 2, "user", &format!("args: {args:?}"));
    }
}

#[test]
fn python_uninstall_missing_version_returns_user_exit_category() {
    let home = temp_env_root();
    let output = base_command(&home)
        .args(["--json", "python", "uninstall", "3.13.12"])
        .output()
        .expect("command output");

    assert_json_exit(&output, "fail", 2, "user", "python uninstall");
}

#[test]
fn init_conflict_returns_user_exit_category() {
    let home = temp_env_root();
    let project_root = home.path().join("workspace").join("sample-init-exit-codes");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        "[project]\nname='x'\nversion='0.1.0'\n",
    )
    .expect("existing pyproject");

    let output = base_command(&home)
        .current_dir(&project_root)
        .args(["--json", "init"])
        .output()
        .expect("init output");
    assert_json_exit(&output, "fail", 2, "user", "init conflict");
}

#[test]
fn storage_layout_creation_failure_returns_system_exit_category() {
    let home = temp_env_root();
    let blocked = home.path().join("blocked-config-path");
    fs::write(&blocked, "not a directory").expect("blocked file");

    let output = base_command(&home)
        .env("PYRA_CONFIG_DIR", &blocked)
        .args(["--json", "python", "list"])
        .output()
        .expect("command output");

    assert_json_exit(
        &output,
        "fail",
        3,
        "system",
        "storage layout creation failure",
    );
}

#[test]
fn catalog_parse_failure_returns_internal_exit_category() {
    let home = temp_env_root();
    let catalog = home.path().join("invalid-catalog.json");
    fs::write(&catalog, "{invalid json").expect("catalog file");

    let output = base_command(&home)
        .env("PYRA_PYTHON_RELEASE_CATALOG_PATH", &catalog)
        .args(["--json", "python", "search"])
        .output()
        .expect("command output");

    assert_json_exit(
        &output,
        "fail",
        4,
        "internal",
        "python search invalid catalog",
    );
}

#[test]
fn run_child_failures_preserve_external_exit_code() {
    let home = temp_env_root();
    let project_root = home
        .path()
        .join("workspace")
        .join("sample-run-external-exit");
    fs::create_dir_all(&project_root).expect("project root");
    let python_version = system_python_version().expect("system python version");
    fs::write(
        project_root.join("pyproject.toml"),
        format!(
            r#"[project]
name = "sample-run-external-exit"
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

    let output = base_command(&home)
        .current_dir(&project_root)
        .args(["--json", "run", "fail.py"])
        .output()
        .expect("command output");

    assert_json_exit(&output, "fail", 7, "external", "run external code");
}

fn assert_json_exit(
    output: &std::process::Output,
    expected_status: &str,
    expected_code: i32,
    expected_category: &str,
    context: &str,
) {
    assert_eq!(
        output.status.code(),
        Some(expected_code),
        "process exit code should match the contract ({context})"
    );
    assert!(
        output.stderr.is_empty(),
        "json mode should keep stderr empty so stdout remains machine-readable"
    );

    let stdout = String::from_utf8(output.stdout.clone()).expect("stdout utf-8");
    let envelope: Value = serde_json::from_str(&stdout).expect("json envelope");
    assert_eq!(envelope["status"], expected_status, "{context}");
    assert_eq!(envelope["exit"]["code"], expected_code, "{context}");
    assert_eq!(envelope["exit"]["category"], expected_category, "{context}");
}

fn temp_env_root() -> TempDir {
    tempfile::tempdir().expect("temporary directory")
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
