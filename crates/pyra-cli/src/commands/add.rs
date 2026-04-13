//! CLI orchestration for `pyra add`.
//!
//! This command only chooses the target declaration scope, delegates manifest
//! mutation plus sync to the project service, and maps the result into terminal
//! output.

use pyra_core::AppContext;
use pyra_project::{AddProjectRequest, DependencyDeclarationScope, ProjectService};
use pyra_ui::{Block, Message, Output};

use crate::cli::AddArgs;
use crate::commands::CommandError;

pub async fn execute(args: AddArgs, context: &AppContext) -> Result<Output, CommandError> {
    let AddArgs {
        requirement,
        group,
        extra,
    } = args;
    let scope = if let Some(group) = group {
        DependencyDeclarationScope::Group(group)
    } else if let Some(extra) = extra {
        DependencyDeclarationScope::Extra(extra)
    } else {
        DependencyDeclarationScope::Base
    };

    let outcome = ProjectService
        .add(context, AddProjectRequest { requirement, scope })
        .await?;

    let detail = match (outcome.manifest_updated, outcome.sync.lock_refreshed) {
        (true, true) => {
            "Updated `pyproject.toml`, refreshed `pylock.toml`, and reconciled the centralized environment."
        }
        (true, false) => {
            "Updated `pyproject.toml`, reused the current lock, and reconciled the centralized environment."
        }
        (false, true) => {
            "Kept `pyproject.toml` unchanged, refreshed `pylock.toml`, and reconciled the centralized environment."
        }
        (false, false) => {
            "Kept `pyproject.toml` unchanged, reused the current lock, and reconciled the centralized environment."
        }
    };

    let message = if outcome.manifest_updated {
        Message::success(format!(
            "Added `{}` to {}.",
            outcome.requirement,
            scope_label(&outcome.scope)
        ))
    } else {
        Message::info(format!(
            "`{}` is already declared in {}.",
            outcome.requirement,
            scope_label(&outcome.scope)
        ))
    }
    .with_detail(detail)
    .with_verbose_line(format!("project root: {}", outcome.sync.project_root))
    .with_verbose_line(format!("pyproject: {}", outcome.sync.pyproject_path))
    .with_verbose_line(format!("pylock: {}", outcome.sync.pylock_path))
    .with_verbose_line(format!("project id: {}", outcome.sync.project_id))
    .with_verbose_line(format!("python: {}", outcome.sync.python_version));

    Ok(Output::single(Block::Message(message)))
}

fn scope_label(scope: &DependencyDeclarationScope) -> String {
    match scope {
        DependencyDeclarationScope::Base => "`[project].dependencies`".to_string(),
        DependencyDeclarationScope::Group(name) => format!("dependency group `{name}`"),
        DependencyDeclarationScope::Extra(name) => format!("extra `{name}`"),
    }
}
