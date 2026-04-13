use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use assert_cmd::Command;
use flate2::{Compression, write::GzEncoder};
use predicates::str::contains;
use pyra_python::{ArchiveFormat, HostTarget, InstalledPythonRecord, PythonVersion};
use tempfile::TempDir;

#[test]
fn use_pins_python_and_prepares_centralized_environment() {
    let home = temp_env_root();
    let fixture = write_catalog_fixture(home.path(), &["3.13.12"]);
    let project_root = home.path().join("workspace").join("sample-use");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        "[project]\nname = \"sample-use\"\nversion = \"0.1.0\"\n",
    )
    .expect("pyproject");

    base_command(&home)
        .current_dir(&project_root)
        .env("PYRA_PYTHON_RELEASE_CATALOG_PATH", &fixture.catalog_path)
        .args(["use", "3.13"])
        .assert()
        .success()
        .stdout(contains("Using Python 3.13.12 for this project."));

    let pyproject = fs::read_to_string(project_root.join("pyproject.toml")).expect("pyproject");
    assert!(pyproject.contains("[tool.pyra]"));
    assert!(pyproject.contains("python = \"3.13\""));
    assert_environment_prepared(home.path(), &project_root, "3.13");
}

#[cfg(unix)]
#[test]
fn use_repin_to_different_patch_version_rebuilds_environment_and_invalidates_lock() {
    let home = temp_env_root();
    let fixture = write_catalog_fixture(home.path(), &["3.13.12", "3.13.13"]);
    let project_root = home.path().join("workspace").join("sample-use-repin-patch");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-use-repin-patch"
version = "0.1.0"
requires-python = ">=3.13,<3.14"
dependencies = []

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    let first_log = home.path().join("fake-python-3.13.12.jsonl");
    let second_log = home.path().join("fake-python-3.13.13.jsonl");
    let first_python =
        build_fake_managed_python(home.path(), "3.13.12", &first_log).expect("fake python");
    let second_python =
        build_fake_managed_python(home.path(), "3.13.13", &second_log).expect("fake python");
    seed_managed_install_with_executable(&home, "3.13.12", &first_python).expect("managed install");
    seed_managed_install_with_executable(&home, "3.13.13", &second_python)
        .expect("managed install");

    base_command(&home)
        .current_dir(&project_root)
        .env("PYRA_PYTHON_RELEASE_CATALOG_PATH", &fixture.catalog_path)
        .args(["use", "3.13.12"])
        .assert()
        .success()
        .stdout(contains("Using Python 3.13.12 for this project."));
    assert_eq!(fake_python_venv_count(&first_log).expect("venv count"), 1);

    write_lock_fixture(
        &project_root,
        "sample-use-repin-patch",
        "3.13.12",
        Some(">=3.13,<3.14"),
    )
    .expect("lock fixture");
    let first_lock = fs::read_to_string(project_root.join("pylock.toml")).expect("pylock");

    base_command(&home)
        .current_dir(&project_root)
        .env("PYRA_PYTHON_RELEASE_CATALOG_PATH", &fixture.catalog_path)
        .args(["use", "3.13.13"])
        .assert()
        .success()
        .stdout(contains("Using Python 3.13.13 for this project."));
    assert_eq!(fake_python_venv_count(&second_log).expect("venv count"), 1);

    let metadata = read_environment_metadata(home.path(), &project_root);
    assert_eq!(metadata["python_selector"].as_str(), Some("3.13.13"));

    base_command(&home)
        .current_dir(&project_root)
        .args(["sync", "--locked"])
        .assert()
        .failure()
        .stderr(contains("stale"))
        .stderr(contains("sync --locked"));

    let second_lock = fs::read_to_string(project_root.join("pylock.toml")).expect("pylock");
    assert_eq!(first_lock, second_lock);
}

#[cfg(unix)]
#[test]
fn use_repin_to_different_minor_version_rebuilds_environment_and_invalidates_lock() {
    let home = temp_env_root();
    let fixture = write_catalog_fixture(home.path(), &["3.12.9", "3.13.12"]);
    let project_root = home.path().join("workspace").join("sample-use-repin-minor");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-use-repin-minor"
version = "0.1.0"
requires-python = ">=3.12,<3.14"
dependencies = []

[tool.pyra]
python = "3.12.9"
"#,
    )
    .expect("pyproject");

    let first_log = home.path().join("fake-python-3.12.9.jsonl");
    let second_log = home.path().join("fake-python-3.13.12.jsonl");
    let first_python =
        build_fake_managed_python(home.path(), "3.12.9", &first_log).expect("fake python");
    let second_python =
        build_fake_managed_python(home.path(), "3.13.12", &second_log).expect("fake python");
    seed_managed_install_with_executable(&home, "3.12.9", &first_python).expect("managed install");
    seed_managed_install_with_executable(&home, "3.13.12", &second_python)
        .expect("managed install");

    base_command(&home)
        .current_dir(&project_root)
        .env("PYRA_PYTHON_RELEASE_CATALOG_PATH", &fixture.catalog_path)
        .args(["use", "3.12.9"])
        .assert()
        .success()
        .stdout(contains("Using Python 3.12.9 for this project."));
    assert_eq!(fake_python_venv_count(&first_log).expect("venv count"), 1);

    write_lock_fixture(
        &project_root,
        "sample-use-repin-minor",
        "3.12.9",
        Some(">=3.12,<3.14"),
    )
    .expect("lock fixture");
    let first_lock = fs::read_to_string(project_root.join("pylock.toml")).expect("pylock");

    base_command(&home)
        .current_dir(&project_root)
        .env("PYRA_PYTHON_RELEASE_CATALOG_PATH", &fixture.catalog_path)
        .args(["use", "3.13.12"])
        .assert()
        .success()
        .stdout(contains("Using Python 3.13.12 for this project."));
    assert_eq!(fake_python_venv_count(&second_log).expect("venv count"), 1);

    let metadata = read_environment_metadata(home.path(), &project_root);
    assert_eq!(metadata["python_selector"].as_str(), Some("3.13.12"));

    base_command(&home)
        .current_dir(&project_root)
        .args(["sync", "--locked"])
        .assert()
        .failure()
        .stderr(contains("stale"))
        .stderr(contains("sync --locked"));

    let second_lock = fs::read_to_string(project_root.join("pylock.toml")).expect("pylock");
    assert_eq!(first_lock, second_lock);
}

#[cfg(unix)]
#[test]
fn use_rejects_incompatible_repin_against_project_requires_python() {
    let home = temp_env_root();
    let fixture = write_catalog_fixture(home.path(), &["3.13.12", "3.12.9"]);
    let project_root = home
        .path()
        .join("workspace")
        .join("sample-use-incompatible-repin");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("pyproject.toml"),
        r#"[project]
name = "sample-use-incompatible-repin"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = []

[tool.pyra]
python = "3.13.12"
"#,
    )
    .expect("pyproject");

    let first_log = home.path().join("fake-python-3.13.12.jsonl");
    let second_log = home.path().join("fake-python-3.12.9.jsonl");
    let first_python =
        build_fake_managed_python(home.path(), "3.13.12", &first_log).expect("fake python");
    let second_python =
        build_fake_managed_python(home.path(), "3.12.9", &second_log).expect("fake python");
    seed_managed_install_with_executable(&home, "3.13.12", &first_python).expect("managed install");
    seed_managed_install_with_executable(&home, "3.12.9", &second_python).expect("managed install");

    base_command(&home)
        .current_dir(&project_root)
        .env("PYRA_PYTHON_RELEASE_CATALOG_PATH", &fixture.catalog_path)
        .args(["use", "3.13.12"])
        .assert()
        .success();
    write_lock_fixture(
        &project_root,
        "sample-use-incompatible-repin",
        "3.13.12",
        Some("==3.13.*"),
    )
    .expect("lock fixture");
    let original_lock = fs::read_to_string(project_root.join("pylock.toml")).expect("pylock");

    base_command(&home)
        .current_dir(&project_root)
        .env("PYRA_PYTHON_RELEASE_CATALOG_PATH", &fixture.catalog_path)
        .args(["use", "3.12.9"])
        .assert()
        .failure()
        .stderr(contains("==3.13.*"))
        .stderr(contains("3.12.9"));

    assert_eq!(fake_python_venv_count(&second_log).expect("venv count"), 0);
    let pyproject = fs::read_to_string(project_root.join("pyproject.toml")).expect("pyproject");
    assert!(pyproject.contains("python = \"3.13.12\""));
    assert!(!pyproject.contains("python = \"3.12.9\""));

    let metadata = read_environment_metadata(home.path(), &project_root);
    assert_eq!(metadata["python_selector"].as_str(), Some("3.13.12"));

    let current_lock = fs::read_to_string(project_root.join("pylock.toml")).expect("pylock");
    assert_eq!(original_lock, current_lock);
}

#[test]
fn init_with_python_uses_managed_install_flow() {
    let home = temp_env_root();
    let fixture = write_catalog_fixture(home.path(), &["3.13.12"]);
    let project_root = home.path().join("workspace").join("sample-init-explicit");
    fs::create_dir_all(&project_root).expect("project root");

    base_command(&home)
        .current_dir(&project_root)
        .env("PYRA_PYTHON_RELEASE_CATALOG_PATH", &fixture.catalog_path)
        .args(["init", "--python", "3.13"])
        .assert()
        .success()
        .stdout(contains(
            "Initialized `sample-init-explicit` with Python 3.13.12.",
        ));

    let pyproject = fs::read_to_string(project_root.join("pyproject.toml")).expect("pyproject");
    assert!(pyproject.contains("python = \"3.13\""));
    assert_environment_prepared(home.path(), &project_root, "3.13");
}

#[test]
fn init_without_python_chooses_latest_managed_installation() {
    let home = temp_env_root();
    let project_root = home.path().join("workspace").join("sample-init-latest");
    fs::create_dir_all(&project_root).expect("project root");

    seed_managed_install(&home, "3.12.9").expect("managed install");
    seed_managed_install(&home, "3.13.4").expect("managed install");

    base_command(&home)
        .current_dir(&project_root)
        .args(["init"])
        .assert()
        .success()
        .stdout(contains(
            "Initialized `sample-init-latest` with Python 3.13.4.",
        ));

    let pyproject = fs::read_to_string(project_root.join("pyproject.toml")).expect("pyproject");
    assert!(pyproject.contains("python = \"3.13.4\""));
    assert_environment_prepared(home.path(), &project_root, "3.13.4");
}

#[test]
fn init_without_python_fails_when_no_managed_python_exists() {
    let home = temp_env_root();
    let project_root = home.path().join("workspace").join("sample-init-empty");
    fs::create_dir_all(&project_root).expect("project root");

    base_command(&home)
        .current_dir(&project_root)
        .args(["init"])
        .assert()
        .failure()
        .stderr(contains("No Pyra-managed Python is installed yet."));
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

fn assert_environment_prepared(home: &Path, project_root: &Path, selector: &str) {
    let metadata = read_environment_metadata(home, project_root);
    let canonical_project_root = project_root
        .canonicalize()
        .expect("canonical project root")
        .display()
        .to_string();

    assert_eq!(metadata["python_selector"].as_str(), Some(selector));
    assert_eq!(
        metadata["project_root"].as_str(),
        Some(canonical_project_root.as_str())
    );

    let environment_path = PathBuf::from(
        metadata["environment_path"]
            .as_str()
            .expect("environment path"),
    );
    assert!(environment_path.exists());
    assert!(environment_path.join("pyvenv.cfg").exists());
}

fn read_environment_metadata(home: &Path, _project_root: &Path) -> serde_json::Value {
    let environments_root = home.join("data").join("environments");
    let entries = fs::read_dir(&environments_root)
        .expect("environment directory")
        .collect::<Result<Vec<_>, _>>()
        .expect("environment entries");
    assert_eq!(entries.len(), 1);

    let metadata_path = entries[0].path().join("metadata.json");
    serde_json::from_slice(&fs::read(&metadata_path).expect("metadata")).expect("metadata json")
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

#[cfg(unix)]
fn build_fake_managed_python(
    root: &Path,
    label: &str,
    log_path: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let runner_path = root.join(format!("fake-python-{label}-runner.py"));
    let runner = r##"import json
import pathlib
import sys

log_path = pathlib.Path("__LOG_PATH__")

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
    (target / "pyvenv.cfg").write_text("home = fake\n", encoding="utf-8")
    log({"kind": "venv", "target": str(target)})
    raise SystemExit(0)

if args[:4] == ["-m", "pip", "install", "--no-deps"] and len(args) == 5:
    log({"kind": "install", "target": args[4]})
    raise SystemExit(0)

if args[:4] == ["-m", "pip", "uninstall", "-y"] and len(args) == 5:
    log({"kind": "uninstall", "target": args[4]})
    raise SystemExit(0)

raise SystemExit(f"unexpected fake interpreter args: {args}")
"##;
    fs::write(
        &runner_path,
        runner.replace("__LOG_PATH__", &log_path.display().to_string()),
    )?;

    let wrapper_path = root.join(format!("fake-python-{label}"));
    let system_python = system_python()?;
    fs::write(
        &wrapper_path,
        format!(
            "#!/bin/sh\nexport PYRA_FAKE_PYTHON_LOG=\"{}\"\nexec \"{}\" \"{}\" \"$@\"\n",
            log_path.display(),
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
fn fake_python_venv_count(log_path: &Path) -> Result<usize, Box<dyn std::error::Error>> {
    let contents = match fs::read_to_string(log_path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(Box::new(error)),
    };

    let mut count = 0;
    for line in contents.lines() {
        let entry: serde_json::Value = serde_json::from_str(line)?;
        if entry.get("kind") == Some(&serde_json::Value::String("venv".to_string())) {
            count += 1;
        }
    }

    Ok(count)
}

fn write_lock_fixture(
    project_root: &Path,
    project_name: &str,
    pinned_python: &str,
    requires_python: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    use sha2::{Digest, Sha256};

    let mut digest = Sha256::new();
    digest.update(project_name.as_bytes());
    digest.update(pinned_python.as_bytes());
    if let Some(requires_python) = requires_python {
        digest.update(requires_python.as_bytes());
    }
    let fingerprint = format!("{:x}", digest.finalize());
    let target_triple = HostTarget::detect()?.target_triple().to_string();

    let mut lock = String::from("lock-version = \"1.0\"\n");
    if let Some(requires_python) = requires_python {
        lock.push_str(&format!("requires-python = {:?}\n", requires_python));
    }
    lock.push_str("extras = []\n");
    lock.push_str("dependency-groups = []\n");
    lock.push_str("default-groups = [\"pyra-default\"]\n");
    lock.push_str("created-by = \"pyra\"\n\n");
    lock.push_str("[[environments]]\n");
    lock.push_str(&format!(
        "id = {:?}\n",
        format!("cpython-{pinned_python}-{target_triple}")
    ));
    lock.push_str(&format!(
        "marker = {:?}\n\n",
        format!("implementation_name == 'cpython' and python_full_version == '{pinned_python}'")
    ));
    lock.push_str("[[packages]]\n");
    lock.push_str("name = \"fixture\"\n");
    lock.push_str("version = \"1.0.0\"\n");
    lock.push_str("index = \"https://pypi.org/simple\"\n\n");
    lock.push_str("[tool.pyra]\n");
    lock.push_str(&format!("input-fingerprint = {:?}\n", fingerprint));
    lock.push_str(&format!("interpreter-version = {:?}\n", pinned_python));
    lock.push_str(&format!("target-triple = {:?}\n", target_triple));
    lock.push_str("index-url = \"https://pypi.org/simple\"\n");
    lock.push_str("resolution-strategy = \"environment-scoped-union-v1\"\n");

    fs::write(project_root.join("pylock.toml"), lock)?;
    Ok(())
}

struct FixturePaths {
    catalog_path: String,
}

fn write_catalog_fixture(root: &Path, versions: &[&str]) -> FixturePaths {
    let archive_path = root.join("python-install.tar.gz");
    fs::write(&archive_path, install_archive_fixture()).expect("archive fixture");
    let digest = sha256_hex(&fs::read(&archive_path).expect("archive bytes"));
    let archive_url = format!("file://{}", archive_path.display());

    let assets = versions
        .iter()
        .map(|version| {
            serde_json::json!({
                "name": host_asset_name(version),
                "browser_download_url": archive_url,
                "digest": format!("sha256:{digest}")
            })
        })
        .collect::<Vec<_>>();
    let catalog_path = root.join("catalog.json");
    fs::write(
        &catalog_path,
        serde_json::to_vec_pretty(&serde_json::json!({ "assets": assets })).unwrap(),
    )
    .expect("catalog");

    FixturePaths {
        catalog_path: catalog_path.display().to_string(),
    }
}

fn host_asset_name(version: &str) -> String {
    let target = HostTarget::detect()
        .expect("supported host")
        .target_triple()
        .to_string();
    format!("cpython-{version}+20260325-{target}-install_only.tar.gz")
}

fn install_archive_fixture() -> Vec<u8> {
    let python = system_python().expect("system python");
    let wrapper = format!(
        "#!/bin/sh\nexec {} \"$@\"\n",
        shell_quote(&python.display().to_string())
    );

    let mut writer = Vec::new();
    let encoder = GzEncoder::new(&mut writer, Compression::default());
    let mut builder = tar::Builder::new(encoder);

    let mut header = tar::Header::new_gnu();
    header.set_path("python/bin/python3").unwrap();
    header.set_mode(0o755);
    header.set_size(wrapper.len() as u64);
    header.set_cksum();
    builder.append(&header, wrapper.as_bytes()).unwrap();
    builder.finish().unwrap();
    let encoder = builder.into_inner().unwrap();
    encoder.finish().unwrap();

    writer
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

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    format!("{:x}", Sha256::digest(bytes))
}
