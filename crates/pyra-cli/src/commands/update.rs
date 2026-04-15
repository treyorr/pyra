//! CLI orchestration for `pyra update`.
//!
//! This command refreshes lock state from existing dependency intent without
//! mutating `pyproject.toml` declarations or reconciling the environment.

use pyra_core::AppContext;
use pyra_project::{ProjectService, UpdatePackageChangeKind, UpdateProjectRequest};
use pyra_ui::{Block, ListBlock, ListItem, Message, Output};

use crate::cli::UpdateArgs;
use crate::commands::CommandError;

pub async fn execute(args: UpdateArgs, context: &AppContext) -> Result<Output, CommandError> {
    let outcome = ProjectService
        .update(
            context,
            UpdateProjectRequest {
                dry_run: args.dry_run,
            },
        )
        .await?;

    let mut message = if outcome.dry_run {
        if outcome.has_changes() {
            Message::warn(format!(
                "Dry run: `pyra update` would change {} package(s) in `{}`.",
                outcome.package_changes.len(),
                outcome.project_id
            ))
            .with_detail(
                "Resolved latest versions allowed by current specifiers, but left `pylock.toml` unchanged.",
            )
            .with_hint("Run `pyra update` to apply these lock changes.")
        } else {
            Message::success(format!(
                "Dry run: no lock changes are needed for `{}`.",
                outcome.project_id
            ))
            .with_detail(
                "The current lock already matches the latest versions allowed by current specifiers.",
            )
        }
    } else if outcome.has_changes() {
        Message::success(format!(
            "Updated `pylock.toml` for `{}` with {} package change(s).",
            outcome.project_id,
            outcome.package_changes.len()
        ))
        .with_detail("Refreshed lock state to latest versions allowed by current specifiers.")
    } else {
        Message::success(format!(
            "Refreshed `pylock.toml` for `{}` with no package changes.",
            outcome.project_id
        ))
        .with_detail(
            "Resolved and rewrote lock state deterministically; package versions were already current.",
        )
    };
    message = message
        .with_hint(
            "This command only refreshes lock state. Use `pyra add`/`pyra remove` to change declared dependency intent.",
        )
        .with_verbose_line(format!("project root: {}", outcome.project_root))
        .with_verbose_line(format!("pyproject: {}", outcome.pyproject_path))
        .with_verbose_line(format!("pylock: {}", outcome.pylock_path))
        .with_verbose_line(format!("python: {}", outcome.python_version))
        .with_verbose_line(format!(
            "previous lock exists: {}",
            outcome.previous_lock_exists
        ))
        .with_verbose_line(format!("resolved packages: {}", outcome.total_packages))
        .with_verbose_line(format!("unchanged packages: {}", outcome.unchanged_packages));

    let mut output = Output::single(Block::Message(message));
    if outcome.has_changes() {
        let items = outcome
            .package_changes
            .into_iter()
            .map(|change| match change.kind {
                UpdatePackageChangeKind::Updated => ListItem::new(format!(
                    "{}: {} -> {}",
                    change.package,
                    change.previous_version.as_deref().unwrap_or("unknown"),
                    change.resolved_version.as_deref().unwrap_or("unknown"),
                ))
                .with_detail("updated"),
                UpdatePackageChangeKind::Added => ListItem::new(format!(
                    "{}: {}",
                    change.package,
                    change.resolved_version.as_deref().unwrap_or("unknown")
                ))
                .with_detail("added"),
                UpdatePackageChangeKind::Removed => ListItem::new(format!(
                    "{}: {}",
                    change.package,
                    change.previous_version.as_deref().unwrap_or("unknown")
                ))
                .with_detail("removed"),
            })
            .collect::<Vec<_>>();
        output = output.with_block(Block::List(
            ListBlock::new()
                .with_heading(if args.dry_run {
                    "Planned lock changes"
                } else {
                    "Applied lock changes"
                })
                .with_items(items),
        ));
    }

    Ok(output)
}
