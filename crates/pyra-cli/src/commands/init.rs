use pyra_core::AppContext;
use pyra_project::{InitProjectRequest, ProjectService};
use pyra_ui::{Block, ListBlock, ListItem, Message, Output};

use crate::cli::InitArgs;
use crate::commands::CommandError;
use crate::commands::project_python::resolve_requested_or_latest;

pub async fn execute(args: InitArgs, context: &AppContext) -> Result<Output, CommandError> {
    let selected_python = resolve_requested_or_latest(context, args.python.as_deref()).await?;
    let outcome = ProjectService.init(
        context,
        InitProjectRequest {
            python_selector: selected_python.selector.clone(),
            installation: selected_python.installation.clone(),
        },
    )?;

    let summary = Message::success(format!(
        "Initialized `{}` with Python {}.",
        outcome.init.project_name, outcome.environment.python_version
    ))
    .with_detail(format!(
        "Pinned `{}` in `pyproject.toml` and prepared the centralized environment.",
        selected_python.selector
    ))
    .with_verbose_line(format!("root: {}", outcome.init.project_root))
    .with_verbose_line(format!("project id: {}", outcome.project_id))
    .with_verbose_line(format!(
        "interpreter: {}",
        outcome.environment.interpreter_path
    ))
    .with_verbose_line(format!(
        "environment: {}",
        outcome.environment.environment_path
    ));

    let files = outcome
        .init
        .created_files
        .into_iter()
        .map(|path| ListItem::new(path.to_string()))
        .collect::<Vec<_>>();

    Ok(Output::new()
        .with_block(Block::Message(summary))
        .with_block(Block::List(
            ListBlock::new().with_heading("Created").with_items(files),
        )))
}
