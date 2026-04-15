//! CLI orchestration for `pyra lock`.
//!
//! This command is intentionally lock-only: it delegates dependency resolution
//! and lock freshness checks to the project service, then renders freshness
//! messaging without reconciling the centralized environment.

use pyra_core::AppContext;
use pyra_project::{LockProjectRequest, LockProjectStatus, ProjectService};
use pyra_ui::{Block, Message, Output};

use crate::cli::LockArgs;
use crate::commands::CommandError;

pub async fn execute(args: LockArgs, context: &AppContext) -> Result<Output, CommandError> {
    let outcome = ProjectService
        .lock(
            context,
            LockProjectRequest {
                lock_targets: args.targets,
            },
        )
        .await?;

    let message = match outcome.status {
        LockProjectStatus::GeneratedMissing => Message::success(format!(
            "Generated `pylock.toml` for `{}`.",
            outcome.project_id
        ))
        .with_detail("No lock file existed, so Pyra resolved dependencies and wrote a new lock."),
        LockProjectStatus::RegeneratedStale => Message::success(format!(
            "Regenerated `pylock.toml` for `{}`.",
            outcome.project_id
        ))
        .with_detail(
            "The existing lock was stale for the current inputs, so Pyra resolved and rewrote it.",
        ),
        LockProjectStatus::ReusedFresh => Message::info(format!(
            "Reused fresh `pylock.toml` for `{}`.",
            outcome.project_id
        ))
        .with_detail(
            "The current lock already matches the current inputs, so Pyra left it unchanged.",
        ),
    }
    .with_verbose_line(format!("project root: {}", outcome.project_root))
    .with_verbose_line(format!("pyproject: {}", outcome.pyproject_path))
    .with_verbose_line(format!("pylock: {}", outcome.pylock_path))
    .with_verbose_line(format!("python: {}", outcome.python_version))
    .with_verbose_line(format!("lock targets: {}", outcome.lock_targets.join(", ")));

    Ok(Output::single(Block::Message(message)))
}
