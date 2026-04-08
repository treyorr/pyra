//! Project-domain service facade.
//!
//! This service owns project discovery, config updates, and centralized
//! environment preparation while leaving Python release resolution to the
//! dedicated Python subsystem.

use camino::Utf8PathBuf;
use pyra_core::AppContext;
use pyra_python::{InstalledPythonRecord, PythonVersionRequest};

use crate::{
    ProjectError,
    environment::{ProjectEnvironmentRecord, ProjectEnvironmentStore, ProjectPythonSelection},
    identity::{ProjectIdentity, find_project_root},
    init::{InitProjectOutcome, create_initial_layout, validate_initial_layout},
    pyproject::{read_python_selector, update_python_selector},
};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UseProjectPythonRequest {
    pub python: ProjectPythonSelection,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UseProjectPythonOutcome {
    pub project_root: Utf8PathBuf,
    pub project_id: String,
    pub pyproject_path: Utf8PathBuf,
    pub environment: ProjectEnvironmentRecord,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ProjectService;

impl ProjectService {
    pub fn init(
        self,
        context: &AppContext,
        request: InitProjectRequest,
    ) -> Result<InitProjectWithPythonOutcome, ProjectError> {
        validate_initial_layout(context)?;
        let InitProjectRequest {
            python_selector,
            installation,
        } = request;
        let identity = ProjectIdentity::from_root(&context.cwd)?;
        let python = ProjectPythonSelection {
            selector: python_selector.clone(),
            installation,
        };
        let environment = ProjectEnvironmentStore.create_or_refresh(context, &identity, &python)?;
        let init = create_initial_layout(
            context,
            &crate::init::InitProjectRequest {
                python_selector: python_selector.clone(),
            },
        )?;

        Ok(InitProjectWithPythonOutcome {
            init,
            project_id: identity.id,
            environment,
        })
    }

    pub fn use_python(
        self,
        context: &AppContext,
        request: UseProjectPythonRequest,
    ) -> Result<UseProjectPythonOutcome, ProjectError> {
        let project_root = find_project_root(&context.cwd)?;
        let identity = ProjectIdentity::from_root(&project_root)?;
        let pyproject_path = project_root.join("pyproject.toml");
        let _ = read_python_selector(&pyproject_path)?;
        update_python_selector(&pyproject_path, &request.python.selector)?;
        let project_context = AppContext::new(
            project_root.clone(),
            context.paths.clone(),
            context.verbosity,
        );
        let environment = ProjectEnvironmentStore.create_or_refresh(
            &project_context,
            &identity,
            &request.python,
        )?;

        Ok(UseProjectPythonOutcome {
            project_root,
            project_id: identity.id,
            pyproject_path,
            environment,
        })
    }

    pub fn select_latest_installed_python(
        installations: &[InstalledPythonRecord],
    ) -> Result<InstalledPythonRecord, ProjectError> {
        installations
            .iter()
            .max_by(|left, right| left.version.cmp(&right.version))
            .cloned()
            .ok_or(ProjectError::NoManagedPythonInstalled)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct InitProjectRequest {
    pub python_selector: PythonVersionRequest,
    pub installation: InstalledPythonRecord,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct InitProjectWithPythonOutcome {
    pub init: InitProjectOutcome,
    pub project_id: String,
    pub environment: ProjectEnvironmentRecord,
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;
    use pyra_python::{ArchiveFormat, InstalledPythonRecord, PythonVersion};

    use super::ProjectService;

    #[test]
    fn selects_latest_installed_python_version() {
        let latest = ProjectService::select_latest_installed_python(&[
            record("3.12.9"),
            record("3.13.2"),
            record("3.13.12"),
        ])
        .expect("latest installation");

        assert_eq!(latest.version, PythonVersion::parse("3.13.12").unwrap());
    }

    fn record(version: &str) -> InstalledPythonRecord {
        InstalledPythonRecord {
            version: PythonVersion::parse(version).unwrap(),
            implementation: "cpython".to_string(),
            build_id: "20260325".to_string(),
            target_triple: "aarch64-apple-darwin".to_string(),
            asset_name: "asset.tar.gz".to_string(),
            archive_format: ArchiveFormat::TarGz,
            download_url: "https://example.test/asset.tar.gz".to_string(),
            checksum_sha256: None,
            install_dir: Utf8PathBuf::from(format!("/tmp/{version}")),
            executable_path: Utf8PathBuf::from(format!("/tmp/{version}/python/bin/python3")),
        }
    }
}
