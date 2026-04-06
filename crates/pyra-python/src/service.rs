//! Domain service for managed Python version state.
//!
//! This crate owns validation and filesystem-facing Python management behavior,
//! but it intentionally knows nothing about clap or terminal rendering.

use std::fs;

use camino::Utf8PathBuf;
use pyra_core::AppContext;

use crate::{PythonError, PythonVersionRequest};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct InstalledPython {
    pub version: PythonVersionRequest,
    pub path: Utf8PathBuf,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct InstalledPythonSet {
    pub versions: Vec<InstalledPython>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum InstallDisposition {
    PreparedPlaceholder,
    AlreadyPresent,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct InstallPythonOutcome {
    pub version: PythonVersionRequest,
    pub install_dir: Utf8PathBuf,
    pub metadata_file: Utf8PathBuf,
    pub disposition: InstallDisposition,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct PythonService;

impl PythonService {
    pub fn list_installed(self, context: &AppContext) -> Result<InstalledPythonSet, PythonError> {
        let install_root = context.paths.python_installations_dir();
        let entries = match fs::read_dir(&install_root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(InstalledPythonSet {
                    versions: Vec::new(),
                });
            }
            Err(source) => {
                return Err(PythonError::ReadInstallDirectory {
                    path: install_root.to_string(),
                    source,
                });
            }
        };

        let mut versions = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| PythonError::InspectInstallEntry {
                path: install_root.to_string(),
                source,
            })?;

            let file_type =
                entry
                    .file_type()
                    .map_err(|source| PythonError::InspectInstallEntry {
                        path: install_root.to_string(),
                        source,
                    })?;
            if !file_type.is_dir() {
                continue;
            }

            let file_name = entry
                .file_name()
                .into_string()
                .map_err(|name| PythonError::NonUtf8EntryName { name })?;
            let version = PythonVersionRequest::parse(&file_name).map_err(|_| {
                PythonError::InvalidInstallEntry {
                    entry: file_name.clone(),
                }
            })?;
            let path = Utf8PathBuf::from_path_buf(entry.path())
                .map_err(|path| PythonError::NonUtf8EntryPath { path })?;

            versions.push(InstalledPython { version, path });
        }

        // Stable ordering keeps CLI output deterministic and becomes the natural
        // base for later "latest installed" resolution rules.
        versions.sort_by(|left, right| left.version.cmp(&right.version));

        Ok(InstalledPythonSet { versions })
    }

    pub fn install(
        self,
        context: &AppContext,
        version: PythonVersionRequest,
    ) -> Result<InstallPythonOutcome, PythonError> {
        let install_dir = context.paths.python_version_dir(version.normalized());
        let metadata_file = install_dir.join("INSTALLATION_PENDING");

        if install_dir.exists() {
            return Ok(InstallPythonOutcome {
                version,
                install_dir,
                metadata_file,
                disposition: InstallDisposition::AlreadyPresent,
            });
        }

        // The initial foundation records intent and layout now so a real download
        // implementation can later replace this placeholder without changing the
        // command contract or storage convention.
        fs::create_dir_all(&install_dir).map_err(|source| PythonError::CreateInstallDirectory {
            path: install_dir.to_string(),
            source,
        })?;

        let metadata = format!(
            "version = \"{}\"\nstatus = \"placeholder\"\n",
            version.normalized()
        );
        fs::write(&metadata_file, metadata).map_err(|source| PythonError::WriteMetadata {
            path: metadata_file.to_string(),
            source,
        })?;

        Ok(InstallPythonOutcome {
            version,
            install_dir,
            metadata_file,
            disposition: InstallDisposition::PreparedPlaceholder,
        })
    }
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;
    use pyra_core::{AppContext, AppPaths, Verbosity};
    use tempfile::tempdir;

    use super::PythonService;
    use crate::PythonVersionRequest;

    #[test]
    fn lists_installed_versions_in_sorted_order() {
        let temp_dir = tempdir().expect("temporary directory");
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).expect("utf-8 path");
        let paths = AppPaths::from_roots(
            root.join("config"),
            root.join("data"),
            root.join("cache"),
            root.join("state"),
        );
        paths.ensure_base_layout().expect("base layout");
        std::fs::create_dir_all(paths.python_version_dir("3.12")).expect("python 3.12 dir");
        std::fs::create_dir_all(paths.python_version_dir("3.11")).expect("python 3.11 dir");

        let context = AppContext::new(root.clone(), paths, Verbosity::Normal);
        let installed = PythonService
            .list_installed(&context)
            .expect("listed installs");

        let versions = installed
            .versions
            .into_iter()
            .map(|item| item.version.to_string())
            .collect::<Vec<_>>();
        assert_eq!(versions, vec!["3.11", "3.12"]);
    }

    #[test]
    fn creates_placeholder_install_metadata() {
        let temp_dir = tempdir().expect("temporary directory");
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).expect("utf-8 path");
        let paths = AppPaths::from_roots(
            root.join("config"),
            root.join("data"),
            root.join("cache"),
            root.join("state"),
        );
        paths.ensure_base_layout().expect("base layout");

        let context = AppContext::new(root.clone(), paths.clone(), Verbosity::Normal);
        let version = PythonVersionRequest::parse("3.13").expect("valid version");
        let outcome = PythonService
            .install(&context, version)
            .expect("placeholder install");

        assert!(outcome.install_dir.exists());
        assert!(outcome.metadata_file.exists());
        assert_eq!(
            std::fs::read_to_string(
                paths
                    .python_version_dir("3.13")
                    .join("INSTALLATION_PENDING")
            )
            .expect("metadata"),
            "version = \"3.13\"\nstatus = \"placeholder\"\n"
        );
    }
}
