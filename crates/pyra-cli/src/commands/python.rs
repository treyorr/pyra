//! Command handlers for `pyra python`.
//!
//! These functions translate parsed CLI arguments into domain service calls and
//! then map service results into reusable UI output objects.

use pyra_core::AppContext;
use pyra_python::{InstallDisposition, PythonService, PythonVersionRequest};
use pyra_ui::{Block, ListBlock, ListItem, Message, Output};

use crate::cli::{PythonArgs, PythonCommand};
use crate::commands::CommandError;

pub fn execute(args: PythonArgs, context: &AppContext) -> Result<Output, CommandError> {
    match args.command {
        PythonCommand::List => list(context),
        PythonCommand::Install(args) => install(context, &args.version),
    }
}

fn list(context: &AppContext) -> Result<Output, CommandError> {
    let installed = PythonService.list_installed(context)?;

    let items = installed
        .versions
        .into_iter()
        .map(|python| {
            let mut item = ListItem::new(python.version.to_string());
            // Paths are useful for debugging and support requests, but stay out of
            // normal output so the default UX remains calm and minimal.
            item = item.with_verbose_line(format!("path: {}", python.path));
            item
        })
        .collect::<Vec<_>>();

    Ok(Output::single(Block::List(
        ListBlock::new()
            .with_heading("Installed Python versions")
            .with_items(items)
            .with_empty_message(
                Message::info("No Python versions are managed by Pyra yet.")
                    .with_hint("Install one with `pyra python install 3.13`."),
            ),
    )))
}

fn install(context: &AppContext, requested_version: &str) -> Result<Output, CommandError> {
    // Normalize at the command edge so downstream code operates on a single
    // canonical version representation.
    let version = PythonVersionRequest::parse(requested_version)?;
    let outcome = PythonService.install(context, version)?;

    let message = match outcome.disposition {
        InstallDisposition::PreparedPlaceholder => {
            Message::success(format!("Prepared Python {}.", outcome.version))
                .with_detail("Created the managed install location and placeholder metadata.")
                .with_hint("Real downloads are not implemented yet.")
                .with_verbose_line(format!("install dir: {}", outcome.install_dir))
                .with_verbose_line(format!("metadata: {}", outcome.metadata_file))
        }
        InstallDisposition::AlreadyPresent => Message::info(format!(
            "Python {} is already tracked by Pyra.",
            outcome.version
        ))
        .with_detail("Pyra found an existing managed install directory for that version request.")
        .with_verbose_line(format!("install dir: {}", outcome.install_dir)),
    };

    Ok(Output::single(Block::Message(message)))
}
