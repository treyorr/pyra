//! Centralized project environment management.
//!
//! The environment itself lives under Pyra-managed storage so future sync and
//! execution commands can reuse one stable location regardless of project
//! checkout layout.

use std::fs;
use std::io::Write;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use camino::{Utf8Path, Utf8PathBuf};
use pyra_core::AppContext;
use pyra_python::{InstalledPythonRecord, PythonVersion, PythonVersionRequest};
use serde::{Deserialize, Serialize};

use crate::{ProjectError, identity::ProjectIdentity};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ProjectPythonSelection {
    pub selector: PythonVersionRequest,
    pub installation: InstalledPythonRecord,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProjectEnvironmentRecord {
    pub project_id: String,
    pub project_root: Utf8PathBuf,
    pub python_selector: String,
    pub python_version: PythonVersion,
    pub interpreter_path: Utf8PathBuf,
    pub environment_path: Utf8PathBuf,
    pub created_at_unix_seconds: u64,
    pub updated_at_unix_seconds: u64,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ProjectEnvironmentStore;

impl ProjectEnvironmentStore {
    pub fn ensure(
        self,
        context: &AppContext,
        identity: &ProjectIdentity,
        python: &ProjectPythonSelection,
    ) -> Result<ProjectEnvironmentRecord, ProjectError> {
        let metadata_path = context.paths.project_environment_metadata(&identity.id);
        if metadata_path.exists() {
            let record = self.read_record(&metadata_path)?;
            if record.environment_path.exists()
                && record.python_version == python.installation.version
                && record.interpreter_path == environment_interpreter_path(&record.environment_path)
            {
                return Ok(record);
            }
        }

        self.create_or_refresh(context, identity, python)
    }

    pub fn create_or_refresh(
        self,
        context: &AppContext,
        identity: &ProjectIdentity,
        python: &ProjectPythonSelection,
    ) -> Result<ProjectEnvironmentRecord, ProjectError> {
        let environment_root = context.paths.project_environment_root(&identity.id);
        let environment_path = context.paths.project_environment_dir(&identity.id);
        let metadata_path = context.paths.project_environment_metadata(&identity.id);

        fs::create_dir_all(&environment_root).map_err(|source| {
            ProjectError::CreateEnvironment {
                path: environment_root.to_string(),
                source,
            }
        })?;

        let output = Command::new(python.installation.executable_path.as_std_path())
            .args(["-m", "venv", "--clear"])
            .arg(environment_path.as_str())
            .output()
            .map_err(|source| ProjectError::CreateEnvironment {
                path: environment_path.to_string(),
                source,
            })?;
        if !output.status.success() {
            return Err(ProjectError::EnvironmentCommandFailed {
                interpreter: python.installation.executable_path.to_string(),
                path: environment_path.to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }

        let now = unix_timestamp();
        let created_at = if metadata_path.exists() {
            self.read_record(&metadata_path)
                .map(|record| record.created_at_unix_seconds)?
        } else {
            now
        };

        let record = ProjectEnvironmentRecord {
            project_id: identity.id.clone(),
            project_root: identity.root.clone(),
            python_selector: python.selector.to_string(),
            python_version: python.installation.version,
            // The environment record stores the environment's Python path
            // because sync and run must act on the centralized environment
            // itself, not on the base managed interpreter used to create it.
            interpreter_path: environment_interpreter_path(&environment_path),
            environment_path: environment_path.clone(),
            created_at_unix_seconds: created_at,
            updated_at_unix_seconds: now,
        };
        self.write_record(&metadata_path, &record)?;
        Ok(record)
    }

    fn read_record(
        self,
        metadata_path: &Utf8Path,
    ) -> Result<ProjectEnvironmentRecord, ProjectError> {
        let contents = fs::read_to_string(metadata_path).map_err(|source| {
            ProjectError::ReadEnvironmentMetadata {
                path: metadata_path.to_string(),
                source,
            }
        })?;
        serde_json::from_str(&contents).map_err(|source| ProjectError::ParseEnvironmentMetadata {
            path: metadata_path.to_string(),
            source,
        })
    }

    fn write_record(
        self,
        metadata_path: &Utf8Path,
        record: &ProjectEnvironmentRecord,
    ) -> Result<(), ProjectError> {
        let payload = serde_json::to_vec_pretty(record).map_err(|source| {
            ProjectError::SerializeEnvironmentMetadata {
                path: metadata_path.to_string(),
                source,
            }
        })?;

        let mut file = fs::File::create(metadata_path).map_err(|source| {
            ProjectError::WriteEnvironmentMetadata {
                path: metadata_path.to_string(),
                source,
            }
        })?;
        file.write_all(&payload)
            .and_then(|_| file.write_all(b"\n"))
            .map_err(|source| ProjectError::WriteEnvironmentMetadata {
                path: metadata_path.to_string(),
                source,
            })?;

        Ok(())
    }
}

fn environment_interpreter_path(environment_path: &Utf8Path) -> Utf8PathBuf {
    if cfg!(windows) {
        environment_path.join("Scripts").join("python.exe")
    } else {
        environment_path.join("bin").join("python")
    }
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_secs()
}
