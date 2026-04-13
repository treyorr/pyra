mod add;
mod init;
mod project_python;
mod python;
mod sync;
mod use_python;

use pyra_core::AppContext;
use pyra_errors::{ErrorReport, UserFacingError};
use pyra_project::ProjectError;
use pyra_python::PythonError;
use pyra_ui::Output;
use thiserror::Error;

use crate::cli::{AddArgs, Command, InitArgs, PythonArgs, SyncArgs, UseArgs};

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

pub async fn execute(command: Command, context: &AppContext) -> Result<Output, CommandError> {
    match command {
        Command::Python(args) => execute_python(args, context).await,
        Command::Init(args) => execute_init(args, context).await,
        Command::Use(args) => execute_use(args, context).await,
        Command::Add(args) => execute_add(args, context).await,
        Command::Sync(args) => execute_sync(args, context).await,
    }
}

async fn execute_python(args: PythonArgs, context: &AppContext) -> Result<Output, CommandError> {
    python::execute(args, context).await
}

async fn execute_init(args: InitArgs, context: &AppContext) -> Result<Output, CommandError> {
    init::execute(args, context).await
}

async fn execute_use(args: UseArgs, context: &AppContext) -> Result<Output, CommandError> {
    use_python::execute(args, context).await
}

async fn execute_add(args: AddArgs, context: &AppContext) -> Result<Output, CommandError> {
    add::execute(args, context).await
}

async fn execute_sync(args: SyncArgs, context: &AppContext) -> Result<Output, CommandError> {
    sync::execute(args, context).await
}
