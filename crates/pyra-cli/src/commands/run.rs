use pyra_core::AppContext;
use pyra_project::{ProjectService, RunProjectRequest};
use pyra_ui::Output;

use crate::cli::RunArgs;
use crate::commands::{CommandError, CommandExecution};

pub async fn execute(
    args: RunArgs,
    context: &AppContext,
) -> Result<CommandExecution, CommandError> {
    let outcome = ProjectService
        .run(
            context,
            RunProjectRequest {
                target: args.target,
                args: args.args,
            },
        )
        .await?;

    Ok(CommandExecution::with_exit_code(
        Output::new(),
        outcome.exit_code,
    ))
}
