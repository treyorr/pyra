use std::fs;

use assert_cmd::Command;
use flate2::{Compression, write::GzEncoder};
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use pyra_python::HostTarget;
use tempfile::TempDir;

#[test]
fn python_list_shows_empty_state() {
    let home = temp_env_root();

    let mut command = Command::cargo_bin("pyra").expect("pyra binary");
    command
        .env("PYRA_CONFIG_DIR", home.path().join("config"))
        .env("PYRA_DATA_DIR", home.path().join("data"))
        .env("PYRA_CACHE_DIR", home.path().join("cache"))
        .env("PYRA_STATE_DIR", home.path().join("state"))
        .args(["python", "list"])
        .assert()
        .success()
        .stdout(contains("No Python versions are managed by Pyra yet."));
}

#[test]
fn python_search_filters_versions_for_current_host() {
    let home = temp_env_root();
    let fixture = write_catalog_fixture(home.path(), &["3.13.12", "3.12.13"]);
    let mut command = Command::cargo_bin("pyra").expect("pyra binary");
    command
        .env("PYRA_CONFIG_DIR", home.path().join("config"))
        .env("PYRA_DATA_DIR", home.path().join("data"))
        .env("PYRA_CACHE_DIR", home.path().join("cache"))
        .env("PYRA_STATE_DIR", home.path().join("state"))
        .env("PYRA_PYTHON_RELEASE_CATALOG_PATH", fixture.catalog_path)
        .args(["python", "search", "3.13"])
        .assert()
        .success()
        .stdout(contains("Available Python versions"))
        .stdout(contains("3.13.12"))
        .stdout(predicates::str::contains("3.12.13").not());
}

#[test]
fn python_install_and_uninstall_use_real_binary_flow() {
    let home = temp_env_root();
    let fixture = write_catalog_fixture(home.path(), &["3.13.12"]);

    let mut install = Command::cargo_bin("pyra").expect("pyra binary");
    install
        .env("PYRA_CONFIG_DIR", home.path().join("config"))
        .env("PYRA_DATA_DIR", home.path().join("data"))
        .env("PYRA_CACHE_DIR", home.path().join("cache"))
        .env("PYRA_STATE_DIR", home.path().join("state"))
        .env("PYRA_PYTHON_RELEASE_CATALOG_PATH", &fixture.catalog_path)
        .args(["python", "install", "3"])
        .assert()
        .success()
        .stdout(contains("Installed Python 3.13.12."));

    let mut uninstall = Command::cargo_bin("pyra").expect("pyra binary");
    uninstall
        .env("PYRA_CONFIG_DIR", home.path().join("config"))
        .env("PYRA_DATA_DIR", home.path().join("data"))
        .env("PYRA_CACHE_DIR", home.path().join("cache"))
        .env("PYRA_STATE_DIR", home.path().join("state"))
        .env("PYRA_PYTHON_RELEASE_CATALOG_PATH", &fixture.catalog_path)
        .args(["python", "uninstall", "3.13.12"])
        .assert()
        .success()
        .stdout(contains("Removed Python 3.13.12."));
}

fn temp_env_root() -> TempDir {
    tempfile::tempdir().expect("temporary directory")
}

struct FixturePaths {
    catalog_path: String,
}

fn write_catalog_fixture(root: &std::path::Path, versions: &[&str]) -> FixturePaths {
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
    let mut writer = Vec::new();
    let encoder = GzEncoder::new(&mut writer, Compression::default());
    let mut builder = tar::Builder::new(encoder);

    let mut header = tar::Header::new_gnu();
    let contents = b"python";
    header.set_path("python/bin/python3").unwrap();
    header.set_mode(0o755);
    header.set_size(contents.len() as u64);
    header.set_cksum();
    builder.append(&header, &contents[..]).unwrap();
    builder.finish().unwrap();
    let encoder = builder.into_inner().unwrap();
    encoder.finish().unwrap();

    writer
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    format!("{:x}", Sha256::digest(bytes))
}
