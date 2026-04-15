//! CLI orchestration for `pyra outdated`.
//!
//! This command is intentionally read-only: it delegates lock-aware upgrade
//! analysis to the project service and maps the typed outcome into UI blocks.

use pyra_core::AppContext;
use pyra_project::{OutdatedProjectRequest, ProjectService};
use pyra_ui::{Block, ListBlock, ListItem, Message, Output};

use crate::cli::OutdatedArgs;
use crate::commands::CommandError;

pub async fn execute(_args: OutdatedArgs, context: &AppContext) -> Result<Output, CommandError> {
    let outcome = ProjectService
        .outdated(context, OutdatedProjectRequest)
        .await?;

    if !outcome.has_outdated_packages() {
        let message = Message::success(format!(
            "All declared dependencies are up to date for `{}`.",
            outcome.project_id
        ))
        .with_detail("No newer package versions are available from the current dependency intent.")
        .with_verbose_line(format!("project root: {}", outcome.project_root))
        .with_verbose_line(format!("pyproject: {}", outcome.pyproject_path))
        .with_verbose_line(format!("pylock: {}", outcome.pylock_path))
        .with_verbose_line(format!("python: {}", outcome.python_version))
        .with_verbose_line(format!("checked packages: {}", outcome.checked_packages));
        return Ok(Output::single(Block::Message(message)));
    }

    let mut output = Output::single(Block::Message(
        Message::warn(format!(
            "Found {} outdated package(s) in `{}`.",
            outcome.outdated_packages.len(),
            outcome.project_id
        ))
        .with_detail("Newer versions are available while preserving the current dependency intent.")
        .with_verbose_line(format!("project root: {}", outcome.project_root))
        .with_verbose_line(format!("pyproject: {}", outcome.pyproject_path))
        .with_verbose_line(format!("pylock: {}", outcome.pylock_path))
        .with_verbose_line(format!("python: {}", outcome.python_version))
        .with_verbose_line(format!("checked packages: {}", outcome.checked_packages)),
    ));

    let items = outcome
        .outdated_packages
        .into_iter()
        .map(|package| {
            ListItem::new(format!(
                "{}: {} -> {}",
                package.package, package.current_version, package.latest_version
            ))
            .with_detail(format!("declared as {}", package.requirements.join(", ")))
            .with_verbose_line(format!(
                "declaration scopes: {}",
                package.declaration_scopes.join(", ")
            ))
        })
        .collect::<Vec<_>>();
    output = output.with_block(Block::List(
        ListBlock::new()
            .with_heading("Outdated packages")
            .with_items(items),
    ));

    Ok(output)
}
