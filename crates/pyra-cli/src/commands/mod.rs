mod init;
mod python;

use pyra_core::AppContext;
use pyra_errors::{ErrorReport, UserFacingError};
use pyra_project::ProjectError;
use pyra_python::PythonError;
use pyra_ui::Output;
use thiserror::Error;

use crate::cli::{Command, InitArgs, PythonArgs};

#[derive(Debug, Error)]
pub enum CommandError {
    #[error(transparent)]
    Python(#[from] PythonError),
    #[error(transparent)]
    Project(#[from] ProjectError),
}

impl UserFacingError for CommandError {
    fn report(&self) -> ErrorReport {
        match self {
            Self::Python(error) => error.report(),
            Self::Project(error) => error.report(),
        }
    }
}

pub fn execute(command: Command, context: &AppContext) -> Result<Output, CommandError> {
    match command {
        Command::Python(args) => execute_python(args, context),
        Command::Init(args) => execute_init(args, context),
    }
}

fn execute_python(args: PythonArgs, context: &AppContext) -> Result<Output, CommandError> {
    python::execute(args, context)
}

fn execute_init(args: InitArgs, context: &AppContext) -> Result<Output, CommandError> {
    init::execute(args, context)
}
