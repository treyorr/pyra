//! Shared interpreter-selection orchestration for project workflows.
//!
//! `pyra init` and `pyra use` should resolve Python in one consistent way even
//! though the project domain and Python domain remain separate crates.

use pyra_core::AppContext;
use pyra_project::{ProjectPythonSelection, ProjectService};
use pyra_python::{InstallPythonRequest, PythonService, PythonVersionRequest};

use crate::commands::CommandError;

pub async fn resolve_requested_or_latest(
    context: &AppContext,
    requested_selector: Option<&str>,
) -> Result<ProjectPythonSelection, CommandError> {
    let python = PythonService::new();

    match requested_selector {
        Some(selector) => {
            let selector = PythonVersionRequest::parse(selector)?;
            let outcome = python
                .install(
                    context,
                    InstallPythonRequest {
                        selector: selector.clone(),
                    },
                )
                .await?;

            Ok(ProjectPythonSelection {
                selector,
                installation: outcome.installation,
            })
        }
        None => {
            let installed = python.list_installed(context).await?;
            let installation =
                ProjectService::select_latest_installed_python(&installed.installations)?;
            let selector = PythonVersionRequest::parse(&installation.version.to_string())
                .expect("installed versions are always concrete and valid");

            Ok(ProjectPythonSelection {
                selector,
                installation,
            })
        }
    }
}
