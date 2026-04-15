//! CLI orchestration for `pyra doctor`.
//!
//! This command is intentionally read-only: it delegates diagnostics to the
//! project service and only maps typed findings into presentation blocks.

use pyra_core::AppContext;
use pyra_project::{DoctorProjectRequest, ProjectService};
use pyra_ui::{Block, Message, Output};

use crate::cli::DoctorArgs;
use crate::commands::CommandError;

pub async fn execute(_args: DoctorArgs, context: &AppContext) -> Result<Output, CommandError> {
    let outcome = ProjectService.doctor(context, DoctorProjectRequest)?;

    if !outcome.has_issues() {
        let message = Message::success(format!("`{}` is healthy.", outcome.project_id))
            .with_detail(
                "Pinned interpreter, lock freshness, and centralized environment state are aligned.",
            )
            .with_verbose_line(format!("project root: {}", outcome.project_root))
            .with_verbose_line(format!("pyproject: {}", outcome.pyproject_path))
            .with_verbose_line(format!("pylock: {}", outcome.pylock_path))
            .with_verbose_line(format!("python selector: {}", outcome.python_selector))
            .with_verbose_line(format!(
                "python version: {}",
                outcome.python_version.as_deref().unwrap_or("unresolved")
            ));
        return Ok(Output::single(Block::Message(message)));
    }

    let mut output = Output::single(Block::Message(
        Message::warn(format!(
            "Found {} issue(s) in `{}`.",
            outcome.issues.len(),
            outcome.project_id
        ))
        .with_detail("Run `pyra sync` or `pyra lock` based on the findings below."),
    ));
    for issue in outcome.issues {
        output = output.with_block(Block::Message(
            Message::warn(issue.summary)
                .with_detail(issue.detail)
                .with_hint(issue.suggestion)
                .with_verbose_line(format!("issue code: {:?}", issue.code))
                .with_verbose_line(format!("project root: {}", outcome.project_root))
                .with_verbose_line(format!("pyproject: {}", outcome.pyproject_path))
                .with_verbose_line(format!("pylock: {}", outcome.pylock_path))
                .with_verbose_line(format!("python selector: {}", outcome.python_selector))
                .with_verbose_line(format!(
                    "python version: {}",
                    outcome.python_version.as_deref().unwrap_or("unresolved")
                )),
        ));
    }

    Ok(output)
}
