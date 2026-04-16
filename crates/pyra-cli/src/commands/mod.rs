mod add;
mod doctor;
mod init;
mod lock;
mod outdated;
mod project_python;
mod python;
mod remove;
mod run;
mod self_update;
mod sync;
mod update;
mod use_python;

use pyra_core::AppContext;
use pyra_errors::{ErrorKind, ErrorReport, UserFacingError};
use pyra_project::ProjectError;
use pyra_python::PythonError;
use pyra_ui::Output;
use thiserror::Error;

use crate::cli::{
    AddArgs, Command, DoctorArgs, InitArgs, LockArgs, OutdatedArgs, PythonArgs, RemoveArgs,
    RunArgs, SelfArgs, SyncArgs, UpdateArgs, UseArgs,
};

#[derive(Debug, Error)]
pub enum CommandError {
    #[error(transparent)]
    Python(#[from] PythonError),
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error(transparent)]
    SelfUpdate(#[from] SelfUpdateError),
}

#[derive(Debug, Error)]
pub enum SelfUpdateError {
    #[error("Pyra could not prepare the self-update request.")]
    Configure {
        #[source]
        source: ::self_update::errors::Error,
    },
    #[error("Pyra could not replace the installed binary.")]
    Apply {
        #[source]
        source: ::self_update::errors::Error,
    },
}

impl UserFacingError for SelfUpdateError {
    fn report(&self) -> ErrorReport {
        match self {
            Self::Configure { source } => ErrorReport::new(
                ErrorKind::System,
                "Pyra could not contact the release backend for self-update.",
            )
            .with_detail(
                "The GitHub Releases request could not be prepared or the release metadata could not be read.",
            )
            .with_suggestion(
                "Check your network connection and release availability, then run `pyra self update` again.",
            )
            .with_verbose_detail(source.to_string()),
            Self::Apply { source } => ErrorReport::new(
                ErrorKind::System,
                "Pyra could not install the updated binary in place.",
            )
            .with_detail(
                "Pyra found release metadata but could not download or apply a compatible binary for this installation.",
            )
            .with_suggestion(
                "Reinstall with `curl -fsSL https://tlo3.com/pyra-install.sh | sh` or adjust permissions for the existing install location.",
            )
            .with_verbose_detail(source.to_string()),
        }
    }
}

impl UserFacingError for CommandError {
    fn report(&self) -> ErrorReport {
        match self {
            Self::Python(error) => error.report(),
            Self::Project(error) => error.report(),
            Self::SelfUpdate(error) => error.report(),
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
        Command::Self_(args) => execute_self(args, context)
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
        Command::Update(args) => execute_update(args, context)
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

async fn execute_self(args: SelfArgs, context: &AppContext) -> Result<Output, CommandError> {
    self_update::execute(args, context).await
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

async fn execute_update(args: UpdateArgs, context: &AppContext) -> Result<Output, CommandError> {
    update::execute(args, context).await
}

async fn execute_run(
    args: RunArgs,
    context: &AppContext,
) -> Result<CommandExecution, CommandError> {
    run::execute(args, context).await
}
