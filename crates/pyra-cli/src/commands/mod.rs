mod add;
mod doctor;
mod init;
mod lock;
mod outdated;
mod project_python;
mod python;
mod remove;
mod run;
mod sync;
mod use_python;

use pyra_core::AppContext;
use pyra_errors::{ErrorReport, UserFacingError};
use pyra_project::ProjectError;
use pyra_python::PythonError;
use pyra_ui::Output;
use thiserror::Error;

use crate::cli::{
    AddArgs, Command, DoctorArgs, InitArgs, LockArgs, OutdatedArgs, PythonArgs, RemoveArgs,
    RunArgs, SyncArgs, UseArgs,
};

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

#[derive(Debug)]
pub struct CommandExecution {
    pub output: Output,
    pub exit_code: i32,
}

impl CommandExecution {
    fn success(output: Output) -> Self {
        Self {
            output,
            exit_code: 0,
        }
    }

    pub fn with_exit_code(output: Output, exit_code: i32) -> Self {
        Self { output, exit_code }
    }
}

pub async fn execute(
    command: Command,
    context: &AppContext,
) -> Result<CommandExecution, CommandError> {
    match command {
        Command::Python(args) => execute_python(args, context)
            .await
            .map(CommandExecution::success),
        Command::Init(args) => execute_init(args, context)
            .await
            .map(CommandExecution::success),
        Command::Use(args) => execute_use(args, context)
            .await
            .map(CommandExecution::success),
        Command::Add(args) => execute_add(args, context)
            .await
            .map(CommandExecution::success),
        Command::Remove(args) => execute_remove(args, context)
            .await
            .map(CommandExecution::success),
        Command::Sync(args) => execute_sync(args, context)
            .await
            .map(CommandExecution::success),
        Command::Lock(args) => execute_lock(args, context)
            .await
            .map(CommandExecution::success),
        Command::Doctor(args) => execute_doctor(args, context)
            .await
            .map(CommandExecution::success),
        Command::Outdated(args) => execute_outdated(args, context)
            .await
            .map(CommandExecution::success),
        Command::Run(args) => execute_run(args, context).await,
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

async fn execute_remove(args: RemoveArgs, context: &AppContext) -> Result<Output, CommandError> {
    remove::execute(args, context).await
}

async fn execute_sync(args: SyncArgs, context: &AppContext) -> Result<Output, CommandError> {
    sync::execute(args, context).await
}

async fn execute_lock(args: LockArgs, context: &AppContext) -> Result<Output, CommandError> {
    lock::execute(args, context).await
}

async fn execute_doctor(args: DoctorArgs, context: &AppContext) -> Result<Output, CommandError> {
    doctor::execute(args, context).await
}

async fn execute_outdated(
    args: OutdatedArgs,
    context: &AppContext,
) -> Result<Output, CommandError> {
    outdated::execute(args, context).await
}

async fn execute_run(
    args: RunArgs,
    context: &AppContext,
) -> Result<CommandExecution, CommandError> {
    run::execute(args, context).await
}
