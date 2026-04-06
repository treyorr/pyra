//! Shared Python domain types used by search, install, list, and uninstall.

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

use crate::PythonVersion;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum ArchiveFormat {
    TarGz,
}

impl ArchiveFormat {
    pub fn suffix(self) -> &'static str {
        match self {
            Self::TarGz => ".tar.gz",
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PythonRelease {
    pub version: PythonVersion,
    pub implementation: String,
    pub build_id: String,
    pub target_triple: String,
    pub asset_name: String,
    pub archive_format: ArchiveFormat,
    pub download_url: String,
    pub checksum_sha256: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct InstalledPythonRecord {
    pub version: PythonVersion,
    pub implementation: String,
    pub build_id: String,
    pub target_triple: String,
    pub asset_name: String,
    pub archive_format: ArchiveFormat,
    pub download_url: String,
    pub checksum_sha256: Option<String>,
    pub install_dir: Utf8PathBuf,
    pub executable_path: Utf8PathBuf,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum InstallDisposition {
    Installed,
    AlreadyInstalled,
}
