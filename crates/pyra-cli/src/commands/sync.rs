use pyra_core::AppContext;
use pyra_project::{ProjectService, SyncLockMode, SyncProjectRequest, SyncSelectionRequest};
use pyra_ui::{Block, Message, Output};

use crate::cli::SyncArgs;
use crate::commands::CommandError;

pub async fn execute(args: SyncArgs, context: &AppContext) -> Result<Output, CommandError> {
    let lock_mode = if args.locked {
        SyncLockMode::Locked
    } else if args.frozen {
        SyncLockMode::Frozen
    } else {
        SyncLockMode::WriteIfNeeded
    };
    let outcome = ProjectService
        .sync(
            context,
            SyncProjectRequest {
                lock_mode,
                lock_targets: args.targets,
                selection: SyncSelectionRequest {
                    groups: args.groups,
                    extras: args.extras,
                    no_groups: args.no_groups,
                    all_groups: args.all_groups,
                    all_extras: args.all_extras,
                    no_dev: args.no_dev,
                    only_groups: args.only_groups,
                    only_dev: args.only_dev,
                },
            },
        )
        .await?;

    let detail = if outcome.lock_refreshed {
        "Updated `pylock.toml` and reconciled the centralized environment."
    } else {
        "Reused the current lock and reconciled the centralized environment."
    };

    let message = Message::success(format!(
        "Synced `{}` with Python {}.",
        outcome.project_id, outcome.python_version
    ))
    .with_detail(detail)
    .with_verbose_line(format!("project root: {}", outcome.project_root))
    .with_verbose_line(format!("pyproject: {}", outcome.pyproject_path))
    .with_verbose_line(format!("pylock: {}", outcome.pylock_path))
    .with_verbose_line(format!(
        "selected groups: {}",
        outcome.selected_groups.join(", ")
    ))
    .with_verbose_line(format!(
        "selected extras: {}",
        outcome.selected_extras.join(", ")
    ));

    Ok(Output::single(Block::Message(message)))
}
