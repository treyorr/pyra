//! Centralized path resolution for every Pyra-owned storage location.
//!
//! Keeping this logic in one place avoids duplicated path conventions across
//! feature crates and makes cross-platform behavior easier to evolve safely.

use std::env;
use std::fs;
use std::path::PathBuf;

use camino::{Utf8Path, Utf8PathBuf};
use directories::ProjectDirs;

use crate::CoreError;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AppPaths {
    pub config_dir: Utf8PathBuf,
    pub data_dir: Utf8PathBuf,
    pub cache_dir: Utf8PathBuf,
    pub state_dir: Utf8PathBuf,
}

impl AppPaths {
    pub fn discover() -> Result<Self, CoreError> {
        let project_dirs = ProjectDirs::from("dev", "Pyra", "Pyra");

        // Environment overrides make CLI tests and sandboxed runs deterministic
        // without weakening the default platform-native directory strategy.
        let config_dir = resolve_dir(
            "config directory",
            "PYRA_CONFIG_DIR",
            project_dirs
                .as_ref()
                .map(|dirs| dirs.config_dir().to_path_buf()),
        )?;
        let data_dir = resolve_dir(
            "data directory",
            "PYRA_DATA_DIR",
            project_dirs
                .as_ref()
                .map(|dirs| dirs.data_dir().to_path_buf()),
        )?;
        let cache_dir = resolve_dir(
            "cache directory",
            "PYRA_CACHE_DIR",
            project_dirs
                .as_ref()
                .map(|dirs| dirs.cache_dir().to_path_buf()),
        )?;
        let state_root = resolve_dir(
            "state root",
            "PYRA_STATE_DIR",
            project_dirs
                .as_ref()
                .map(|dirs| dirs.data_local_dir().to_path_buf().join("state")),
        )?;

        Ok(Self {
            config_dir,
            data_dir,
            cache_dir,
            state_dir: state_root,
        })
    }

    pub fn from_roots(
        config_dir: Utf8PathBuf,
        data_dir: Utf8PathBuf,
        cache_dir: Utf8PathBuf,
        state_dir: Utf8PathBuf,
    ) -> Self {
        Self {
            config_dir,
            data_dir,
            cache_dir,
            state_dir,
        }
    }

    pub fn ensure_base_layout(&self) -> Result<(), CoreError> {
        for path in [
            &self.config_dir,
            &self.data_dir,
            &self.cache_dir,
            &self.state_dir,
            // Python installations are a first-class part of the long-term data
            // layout, so the base directory exists even before real installs do.
            &self.python_installations_dir(),
            &self.python_downloads_dir(),
            // Locked package artifacts are staged and verified under Pyra-owned
            // cache paths so sync never hands lock URLs directly to pip.
            &self.package_artifact_cache_dir(),
            &self.package_artifact_staging_dir(),
            &self.project_environments_dir(),
        ] {
            ensure_dir(path)?;
        }

        Ok(())
    }

    pub fn config_file(&self) -> Utf8PathBuf {
        self.config_dir.join("config.toml")
    }

    pub fn python_installations_dir(&self) -> Utf8PathBuf {
        self.data_dir.join("pythons")
    }

    pub fn python_version_dir(&self, version: &str) -> Utf8PathBuf {
        self.python_installations_dir().join(version)
    }

    pub fn python_downloads_dir(&self) -> Utf8PathBuf {
        self.cache_dir.join("python").join("downloads")
    }

    pub fn python_download_archive(&self, asset_name: &str) -> Utf8PathBuf {
        self.python_downloads_dir().join(asset_name)
    }

    pub fn package_artifact_cache_dir(&self) -> Utf8PathBuf {
        self.cache_dir.join("artifacts").join("verified")
    }

    pub fn package_artifact_staging_dir(&self) -> Utf8PathBuf {
        self.cache_dir.join("artifacts").join("staging")
    }

    pub fn package_artifact_cache_file(&self, sha256: &str, artifact_name: &str) -> Utf8PathBuf {
        self.package_artifact_cache_dir()
            .join(format!("{sha256}-{artifact_name}"))
    }

    pub fn package_artifact_staging_file(&self, sha256: &str, artifact_name: &str) -> Utf8PathBuf {
        self.package_artifact_staging_dir()
            .join(format!("{sha256}-{artifact_name}.part"))
    }

    pub fn project_environments_dir(&self) -> Utf8PathBuf {
        self.data_dir.join("environments")
    }

    pub fn project_environment_root(&self, project_id: &str) -> Utf8PathBuf {
        self.project_environments_dir().join(project_id)
    }

    pub fn project_environment_dir(&self, project_id: &str) -> Utf8PathBuf {
        self.project_environment_root(project_id)
            .join("environment")
    }

    pub fn project_environment_metadata(&self, project_id: &str) -> Utf8PathBuf {
        self.project_environment_root(project_id)
            .join("metadata.json")
    }
}

fn utf8_path(label: &'static str, path: PathBuf) -> Result<Utf8PathBuf, CoreError> {
    Utf8PathBuf::from_path_buf(path).map_err(|path| CoreError::NonUtf8Path { label, path })
}

fn resolve_dir(
    label: &'static str,
    env_name: &'static str,
    fallback: Option<PathBuf>,
) -> Result<Utf8PathBuf, CoreError> {
    // Explicit overrides win so tests and automation can isolate Pyra state.
    if let Some(path) = env_override(env_name)? {
        return Ok(path);
    }

    let fallback = fallback.ok_or(CoreError::AppDirectoriesUnavailable)?;
    utf8_path(label, fallback)
}

fn env_override(name: &'static str) -> Result<Option<Utf8PathBuf>, CoreError> {
    match env::var(name) {
        Ok(value) => {
            if value.trim().is_empty() {
                return Err(CoreError::EmptyEnvironmentOverride { name });
            }

            Ok(Some(Utf8PathBuf::from(value)))
        }
        Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(_)) => Err(CoreError::NonUtf8EnvironmentOverride { name }),
    }
}

fn ensure_dir(path: &Utf8Path) -> Result<(), CoreError> {
    fs::create_dir_all(path).map_err(|source| CoreError::CreateDirectory {
        path: path.to_string(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;

    use super::AppPaths;

    #[test]
    fn maps_project_environment_paths_deterministically() {
        let paths = AppPaths::from_roots(
            Utf8PathBuf::from("/tmp/config"),
            Utf8PathBuf::from("/tmp/data"),
            Utf8PathBuf::from("/tmp/cache"),
            Utf8PathBuf::from("/tmp/state"),
        );

        assert_eq!(
            paths.project_environments_dir(),
            Utf8PathBuf::from("/tmp/data/environments")
        );
        assert_eq!(
            paths.project_environment_root("abc123"),
            Utf8PathBuf::from("/tmp/data/environments/abc123")
        );
        assert_eq!(
            paths.project_environment_dir("abc123"),
            Utf8PathBuf::from("/tmp/data/environments/abc123/environment")
        );
        assert_eq!(
            paths.project_environment_metadata("abc123"),
            Utf8PathBuf::from("/tmp/data/environments/abc123/metadata.json")
        );
        assert_eq!(
            paths.package_artifact_cache_dir(),
            Utf8PathBuf::from("/tmp/cache/artifacts/verified")
        );
        assert_eq!(
            paths.package_artifact_staging_dir(),
            Utf8PathBuf::from("/tmp/cache/artifacts/staging")
        );
        assert_eq!(
            paths.package_artifact_cache_file("deadbeef", "attrs-25.1.0-py3-none-any.whl"),
            Utf8PathBuf::from(
                "/tmp/cache/artifacts/verified/deadbeef-attrs-25.1.0-py3-none-any.whl"
            )
        );
        assert_eq!(
            paths.package_artifact_staging_file("deadbeef", "attrs-25.1.0-py3-none-any.whl"),
            Utf8PathBuf::from(
                "/tmp/cache/artifacts/staging/deadbeef-attrs-25.1.0-py3-none-any.whl.part"
            )
        );
    }
}
