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
use pyra_project::ProjectErrorCategory;
use pyra_ui::{
    CommandEnvelope, ExitCategory, ExitEnvelope, Terminal, exit_category_from_error_kind,
};
use thiserror::Error;

use crate::cli::{Cli, Command};

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

#[derive(Debug, Clone, Copy)]
enum OutputMode {
    Human,
    Json,
}

pub async fn main_entry() -> ExitCode {
    let cli = Cli::parse();
    let verbosity = Verbosity::from_occurrences(cli.verbose);
    let output_mode = if cli.json {
        OutputMode::Json
    } else {
        OutputMode::Human
    };
    let mut terminal = Terminal::new(verbosity);

    match run(cli.command, verbosity, output_mode, &mut terminal).await {
        Ok(code) => normalized_exit_code(code),
        Err(error) => {
            let report = error.report();
            let exit = exit_envelope_for_app_error(&error, report.kind);

            match output_mode {
                OutputMode::Human => {
                    let _ = terminal.render_error(&error);
                }
                OutputMode::Json => {
                    let envelope = CommandEnvelope::from_error_report(report, exit);
                    let _ = terminal.render_json(&envelope);
                }
            }

            normalized_exit_code(exit.code)
        }
    }
}

async fn run(
    command: Command,
    verbosity: Verbosity,
    output_mode: OutputMode,
    terminal: &mut Terminal,
) -> Result<i32, AppError> {
    // The shared app context is the one place where we resolve runtime paths and
    // runtime-wide settings before handing control to feature-specific handlers.
    let context = AppContext::discover(verbosity)?;
    context.paths.ensure_base_layout()?;

    let result = commands::execute(command, &context).await?;
    let envelope = CommandEnvelope::from_execution(result.output, result.exit_code);
    render_command_envelope(terminal, output_mode, &envelope)?;

    Ok(envelope.exit.code)
}

fn render_command_envelope(
    terminal: &mut Terminal,
    output_mode: OutputMode,
    envelope: &CommandEnvelope,
) -> Result<(), AppError> {
    match output_mode {
        // Rendering happens only after command execution has returned a
        // presentation model, which keeps terminal concerns out of domain and
        // orchestration code.
        OutputMode::Human => {
            if let Some(output) = &envelope.output {
                terminal
                    .render(output)
                    .map_err(|source| CoreError::CreateDirectory {
                        path: "stdout".to_string(),
                        source,
                    })?;
            }
        }
        OutputMode::Json => {
            terminal
                .render_json(envelope)
                .map_err(|source| CoreError::CreateDirectory {
                    path: "stdout".to_string(),
                    source,
                })?;
        }
    }

    Ok(())
}

fn exit_envelope_for_app_error(error: &AppError, kind: pyra_errors::ErrorKind) -> ExitEnvelope {
    match error {
        AppError::Command(commands::CommandError::Project(project_error)) => {
            ExitEnvelope::from_category(exit_category_from_project_error(project_error.category()))
        }
        _ => ExitEnvelope::from_category(exit_category_from_error_kind(kind)),
    }
}

fn exit_category_from_project_error(category: ProjectErrorCategory) -> ExitCategory {
    match category {
        ProjectErrorCategory::User => ExitCategory::User,
        ProjectErrorCategory::System => ExitCategory::System,
        ProjectErrorCategory::Internal => ExitCategory::Internal,
    }
}

fn normalized_exit_code(code: i32) -> ExitCode {
    if code == 0 {
        return ExitCode::SUCCESS;
    }

    u8::try_from(code)
        .map(ExitCode::from)
        .unwrap_or(ExitCode::from(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyra_errors::ErrorKind;

    #[test]
    fn error_kind_to_exit_category_mapping_is_stable() {
        assert_eq!(
            ExitEnvelope::from_category(exit_category_from_error_kind(ErrorKind::User)),
            ExitEnvelope {
                code: 2,
                category: ExitCategory::User,
            }
        );
        assert_eq!(
            ExitEnvelope::from_category(exit_category_from_error_kind(ErrorKind::System)),
            ExitEnvelope {
                code: 3,
                category: ExitCategory::System,
            }
        );
        assert_eq!(
            ExitEnvelope::from_category(exit_category_from_error_kind(ErrorKind::Internal)),
            ExitEnvelope {
                code: 4,
                category: ExitCategory::Internal,
            }
        );
    }

    #[test]
    fn project_error_category_to_exit_category_mapping_is_stable() {
        assert_eq!(
            exit_category_from_project_error(ProjectErrorCategory::User),
            ExitCategory::User
        );
        assert_eq!(
            exit_category_from_project_error(ProjectErrorCategory::System),
            ExitCategory::System
        );
        assert_eq!(
            exit_category_from_project_error(ProjectErrorCategory::Internal),
            ExitCategory::Internal
        );
    }
}
