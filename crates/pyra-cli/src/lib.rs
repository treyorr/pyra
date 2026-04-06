//! Application entrypoint and top-level composition for the Pyra CLI.
//!
//! This crate owns parsing, context creation, command dispatch, and final
//! terminal rendering. Domain crates stay unaware of clap and terminal output.

mod cli;
mod commands;

use std::process::ExitCode;

use clap::Parser;
use pyra_core::{AppContext, CoreError, Verbosity};
use pyra_errors::{ErrorReport, UserFacingError};
use pyra_ui::Terminal;
use thiserror::Error;

use crate::cli::Cli;

#[derive(Debug, Error)]
enum AppError {
    #[error(transparent)]
    Core(#[from] CoreError),
    // Command errors already carry typed user-facing detail from the domain layer.
    #[error(transparent)]
    Command(#[from] commands::CommandError),
}

impl UserFacingError for AppError {
    fn report(&self) -> ErrorReport {
        match self {
            Self::Core(error) => error.report(),
            Self::Command(error) => error.report(),
        }
    }
}

pub async fn main_entry() -> ExitCode {
    let cli = Cli::parse();
    let verbosity = Verbosity::from_occurrences(cli.verbose);
    let mut terminal = Terminal::new(verbosity);

    match run(cli, verbosity, &mut terminal).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = terminal.render_error(&error);
            ExitCode::from(1)
        }
    }
}

async fn run(cli: Cli, verbosity: Verbosity, terminal: &mut Terminal) -> Result<(), AppError> {
    // The shared app context is the one place where we resolve runtime paths and
    // runtime-wide settings before handing control to feature-specific handlers.
    let context = AppContext::discover(verbosity)?;
    context.paths.ensure_base_layout()?;

    let output = commands::execute(cli.command, &context).await?;
    // Rendering happens only after command execution has returned a presentation
    // model, which keeps terminal concerns out of domain and orchestration code.
    terminal
        .render(&output)
        .map_err(|source| CoreError::CreateDirectory {
            path: "stdout".to_string(),
            source,
        })?;

    Ok(())
}
