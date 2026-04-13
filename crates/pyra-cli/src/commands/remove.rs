//! CLI orchestration for `pyra remove`.
//!
//! This command only chooses the target declaration scope, delegates manifest
//! mutation plus sync to the project service, and maps the result into terminal
//! output.

use pyra_core::AppContext;
use pyra_project::{DependencyDeclarationScope, ProjectService, RemoveProjectRequest};
use pyra_ui::{Block, Message, Output};

use crate::cli::RemoveArgs;
use crate::commands::CommandError;

pub async fn execute(args: RemoveArgs, context: &AppContext) -> Result<Output, CommandError> {
    let RemoveArgs {
        package,
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
        .remove(context, RemoveProjectRequest { package, scope })
        .await?;

    let detail = if outcome.sync.lock_refreshed {
        "Updated `pyproject.toml`, refreshed `pylock.toml`, and reconciled the centralized environment."
    } else {
        "Updated `pyproject.toml`, reused the current lock, and reconciled the centralized environment."
    };

    let message = Message::success(format!(
        "Removed `{}` from {}.",
        outcome.package,
        scope_label(&outcome.scope)
    ))
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
