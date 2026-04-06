use pyra_core::AppContext;
use pyra_project::{InitProjectRequest, ProjectService};
use pyra_python::PythonVersionRequest;
use pyra_ui::{Block, ListBlock, ListItem, Message, Output};

use crate::cli::InitArgs;
use crate::commands::CommandError;

pub fn execute(args: InitArgs, context: &AppContext) -> Result<Output, CommandError> {
    let python_version = args
        .python
        .as_deref()
        .map(PythonVersionRequest::parse)
        .transpose()?
        .map(|version| version.to_string());

    let outcome = ProjectService.init(
        context,
        InitProjectRequest {
            python_version: python_version.clone(),
        },
    )?;

    let summary = match python_version {
        Some(version) => Message::success(format!(
            "Initialized `{}` with Python {}.",
            outcome.project_name, version
        ))
        .with_verbose_line(format!("root: {}", outcome.project_root)),
        None => Message::success(format!("Initialized `{}`.", outcome.project_name))
            .with_hint("Add `--python <version>` later to pin a managed interpreter.")
            .with_verbose_line(format!("root: {}", outcome.project_root)),
    };

    let files = outcome
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
