use pyra_core::AppContext;
use pyra_project::{ProjectService, UseProjectPythonRequest};
use pyra_ui::{Block, Message, Output};

use crate::cli::UseArgs;
use crate::commands::CommandError;
use crate::commands::project_python::resolve_requested_or_latest;

pub async fn execute(args: UseArgs, context: &AppContext) -> Result<Output, CommandError> {
    let selected_python = resolve_requested_or_latest(context, Some(&args.version)).await?;
    let outcome = ProjectService.use_python(
        context,
        UseProjectPythonRequest {
            python: selected_python.clone(),
        },
    )?;

    let message = Message::success(format!(
        "Using Python {} for this project.",
        outcome.environment.python_version
    ))
    .with_detail(format!(
        "Pinned `{}` in `pyproject.toml` and refreshed the centralized environment.",
        selected_python.selector
    ))
    .with_verbose_line(format!("root: {}", outcome.project_root))
    .with_verbose_line(format!("project id: {}", outcome.project_id))
    .with_verbose_line(format!("pyproject: {}", outcome.pyproject_path))
    .with_verbose_line(format!(
        "interpreter: {}",
        outcome.environment.interpreter_path
    ))
    .with_verbose_line(format!(
        "environment: {}",
        outcome.environment.environment_path
    ));

    Ok(Output::single(Block::Message(message)))
}
