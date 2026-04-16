//! CLI orchestration for `pyra self update`.
//!
//! This command owns only the installed Pyra binary lifecycle. It does not
//! overlap with `pyra update`, which remains the lock refresh command for
//! project dependencies.

use ::self_update::{Status, backends::github::Update};
use pyra_core::AppContext;
use pyra_ui::{Block, Message, Output};

use crate::cli::{SelfArgs, SelfCommand, SelfUpdateArgs};
use crate::commands::{CommandError, SelfUpdateError};

const RELEASE_REPO_OWNER: &str = "treyorr";
const RELEASE_REPO_NAME: &str = "pyra";
const BINARY_NAME: &str = "pyra";

pub async fn execute(args: SelfArgs, context: &AppContext) -> Result<Output, CommandError> {
    match args.command {
        SelfCommand::Update(args) => execute_update(args, context).await,
    }
}

async fn execute_update(
    _args: SelfUpdateArgs,
    _context: &AppContext,
) -> Result<Output, CommandError> {
    let updater = GitHubSelfUpdater;
    execute_update_with(&updater)
}

fn execute_update_with(updater: &impl SelfUpdater) -> Result<Output, CommandError> {
    let outcome = updater.update()?;

    let message = match outcome.status {
        SelfUpdateStatus::UpToDate => Message::success(format!(
            "Pyra is already up to date at {}.",
            outcome.version
        ))
        .with_detail(
            "The installed binary already matches the latest compatible GitHub Release for this platform.",
        )
        .with_hint("Run `pyra --version` to confirm the active binary on your PATH."),
        SelfUpdateStatus::Updated => Message::success(format!(
            "Updated Pyra from {} to {}.",
            outcome.previous_version, outcome.version
        ))
        .with_detail(
            "Downloaded the latest compatible GitHub Release asset and replaced the installed binary in place.",
        )
        .with_hint("Run `pyra --version` to confirm the new version."),
    }
    .with_verbose_line(format!(
        "release repo: {}/{}",
        RELEASE_REPO_OWNER, RELEASE_REPO_NAME
    ))
    .with_verbose_line(format!("target: {}", outcome.target));

    Ok(Output::single(Block::Message(message)))
}

trait SelfUpdater {
    fn update(&self) -> Result<SelfUpdateOutcome, SelfUpdateError>;
}

struct GitHubSelfUpdater;

impl SelfUpdater for GitHubSelfUpdater {
    fn update(&self) -> Result<SelfUpdateOutcome, SelfUpdateError> {
        let target = ::self_update::get_target().to_string();
        let status = Update::configure()
            .repo_owner(RELEASE_REPO_OWNER)
            .repo_name(RELEASE_REPO_NAME)
            .bin_name(BINARY_NAME)
            .target(&target)
            .show_download_progress(false)
            .show_output(false)
            .no_confirm(true)
            .current_version(env!("CARGO_PKG_VERSION"))
            .build()
            .map_err(|source| SelfUpdateError::Configure { source })?
            .update()
            .map_err(|source| SelfUpdateError::Apply { source })?;

        Ok(SelfUpdateOutcome::from_status(status, target))
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct SelfUpdateOutcome {
    status: SelfUpdateStatus,
    previous_version: String,
    version: String,
    target: String,
}

impl SelfUpdateOutcome {
    fn from_status(status: Status, target: String) -> Self {
        match status {
            Status::UpToDate(version) => Self {
                status: SelfUpdateStatus::UpToDate,
                previous_version: version.clone(),
                version,
                target,
            },
            Status::Updated(version) => Self {
                status: SelfUpdateStatus::Updated,
                previous_version: env!("CARGO_PKG_VERSION").to_string(),
                version,
                target,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum SelfUpdateStatus {
    UpToDate,
    Updated,
}

#[cfg(test)]
mod tests {
    use pyra_errors::UserFacingError;

    use super::*;

    struct FakeUpdater {
        outcome: SelfUpdateOutcome,
    }

    impl SelfUpdater for FakeUpdater {
        fn update(&self) -> Result<SelfUpdateOutcome, SelfUpdateError> {
            Ok(self.outcome.clone())
        }
    }

    #[test]
    fn output_reports_up_to_date_status_clearly() {
        let output = execute_update_with(&FakeUpdater {
            outcome: SelfUpdateOutcome {
                status: SelfUpdateStatus::UpToDate,
                previous_version: "0.1.0".to_string(),
                version: "0.1.0".to_string(),
                target: "aarch64-apple-darwin".to_string(),
            },
        })
        .expect("self update output");

        let expected = Output::single(Block::Message(
            Message::success("Pyra is already up to date at 0.1.0.")
                .with_detail(
                    "The installed binary already matches the latest compatible GitHub Release for this platform.",
                )
                .with_hint("Run `pyra --version` to confirm the active binary on your PATH.")
                .with_verbose_line("release repo: treyorr/pyra")
                .with_verbose_line("target: aarch64-apple-darwin"),
        ));
        assert_eq!(output, expected);
    }

    #[test]
    fn output_reports_completed_update_clearly() {
        let output = execute_update_with(&FakeUpdater {
            outcome: SelfUpdateOutcome {
                status: SelfUpdateStatus::Updated,
                previous_version: "0.1.0".to_string(),
                version: "0.1.1".to_string(),
                target: "x86_64-unknown-linux-gnu".to_string(),
            },
        })
        .expect("self update output");

        let expected = Output::single(Block::Message(
            Message::success("Updated Pyra from 0.1.0 to 0.1.1.")
                .with_detail(
                    "Downloaded the latest compatible GitHub Release asset and replaced the installed binary in place.",
                )
                .with_hint("Run `pyra --version` to confirm the new version.")
                .with_verbose_line("release repo: treyorr/pyra")
                .with_verbose_line("target: x86_64-unknown-linux-gnu"),
        ));
        assert_eq!(output, expected);
    }

    #[test]
    fn apply_failures_remain_actionable() {
        let error = SelfUpdateError::Apply {
            source: ::self_update::errors::Error::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "permission denied",
            )),
        };

        let report = error.report();
        assert_eq!(
            report.summary,
            "Pyra could not install the updated binary in place."
        );
        assert_eq!(
            report.detail.as_deref(),
            Some(
                "Pyra found release metadata but could not download or apply a compatible binary for this installation."
            )
        );
        assert_eq!(
            report.suggestion.as_deref(),
            Some(
                "Reinstall with `curl -fsSL https://tlo3.com/pyra-install.sh | sh` or adjust permissions for the existing install location."
            )
        );
    }
}
