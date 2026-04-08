use std::fs;
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
