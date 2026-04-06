//! Command handlers for `pyra python`.
//!
//! These functions translate parsed CLI arguments into typed Python service
//! requests and then map service results into reusable UI output objects.

use pyra_core::AppContext;
use pyra_python::{
    InstallDisposition, InstallPythonRequest, InstalledPythonRecord, ListInstalledPythonsOutcome,
    PythonRelease, PythonService, PythonVersionRequest, SearchPythonRequest,
    UninstallPythonRequest,
};
use pyra_ui::{Block, ListBlock, ListItem, Message, Output};

use crate::cli::{PythonArgs, PythonCommand};
use crate::commands::CommandError;

pub async fn execute(args: PythonArgs, context: &AppContext) -> Result<Output, CommandError> {
    let service = PythonService::new();

    match args.command {
        PythonCommand::List => list(&service, context).await,
        PythonCommand::Search(args) => search(&service, context, args.version.as_deref()).await,
        PythonCommand::Install(args) => install(&service, context, &args.version).await,
        PythonCommand::Uninstall(args) => uninstall(&service, context, &args.version).await,
    }
}

async fn list(service: &PythonService, context: &AppContext) -> Result<Output, CommandError> {
    let installed = service.list_installed(context).await?;
    Ok(render_installed_versions(installed))
}

async fn search(
    service: &PythonService,
    context: &AppContext,
    requested_version: Option<&str>,
) -> Result<Output, CommandError> {
    let selector = requested_version
        .map(PythonVersionRequest::parse)
        .transpose()?;
    let outcome = service
        .search(
            context,
            SearchPythonRequest {
                selector: selector.clone(),
            },
        )
        .await?;

    Ok(render_available_versions(
        outcome.releases,
        selector.as_ref(),
    ))
}

async fn install(
    service: &PythonService,
    context: &AppContext,
    requested_version: &str,
) -> Result<Output, CommandError> {
    // Normalize at the command edge so downstream code operates on one canonical
    // selector representation.
    let selector = PythonVersionRequest::parse(requested_version)?;
    let outcome = service
        .install(
            context,
            InstallPythonRequest {
                selector: selector.clone(),
            },
        )
        .await?;

    let message = match outcome.disposition {
        InstallDisposition::Installed => Message::success(format!(
            "Installed Python {}.",
            outcome.installation.version
        ))
        .with_detail("Downloaded and activated a managed python-build-standalone distribution.")
        .with_verbose_line(format!("request: {}", selector))
        .with_verbose_line(format!("build: {}", outcome.release.build_id))
        .with_verbose_line(format!("target: {}", outcome.release.target_triple))
        .with_verbose_line(format!("install dir: {}", outcome.installation.install_dir))
        .with_verbose_line(format!("python: {}", outcome.installation.executable_path)),
        InstallDisposition::AlreadyInstalled => Message::info(format!(
            "Python {} is already installed.",
            outcome.installation.version
        ))
        .with_detail("Pyra found an existing managed installation for the resolved version.")
        .with_verbose_line(format!("request: {}", selector))
        .with_verbose_line(format!("install dir: {}", outcome.installation.install_dir))
        .with_verbose_line(format!("python: {}", outcome.installation.executable_path)),
    };

    Ok(Output::single(Block::Message(message)))
}

async fn uninstall(
    service: &PythonService,
    context: &AppContext,
    requested_version: &str,
) -> Result<Output, CommandError> {
    let selector = PythonVersionRequest::parse(requested_version)?;
    let outcome = service
        .uninstall(
            context,
            UninstallPythonRequest {
                selector: selector.clone(),
            },
        )
        .await?;

    let message = Message::success(format!("Removed Python {}.", outcome.removed.version))
        .with_detail("Deleted the managed Python installation from Pyra storage.")
        .with_verbose_line(format!("request: {}", selector))
        .with_verbose_line(format!("install dir: {}", outcome.removed.install_dir));

    Ok(Output::single(Block::Message(message)))
}

fn render_installed_versions(outcome: ListInstalledPythonsOutcome) -> Output {
    let items = outcome
        .installations
        .into_iter()
        .map(installed_item)
        .collect::<Vec<_>>();

    Output::single(Block::List(
        ListBlock::new()
            .with_heading("Installed Python versions")
            .with_items(items)
            .with_empty_message(
                Message::info("No Python versions are managed by Pyra yet.")
                    .with_hint("Install one with `pyra python install 3.13`."),
            ),
    ))
}

fn render_available_versions(
    releases: Vec<PythonRelease>,
    selector: Option<&PythonVersionRequest>,
) -> Output {
    let empty_message = match selector {
        Some(selector) => Message::info(format!(
            "No installable Python versions matched `{selector}`."
        ))
        .with_hint("Run `pyra python search` to see the currently available versions."),
        None => Message::info("No installable Python versions are currently available.")
            .with_hint("Retry in a moment or check your network connection."),
    };

    let items = releases.into_iter().map(release_item).collect::<Vec<_>>();
    Output::single(Block::List(
        ListBlock::new()
            .with_heading("Available Python versions")
            .with_items(items)
            .with_empty_message(empty_message),
    ))
}

fn installed_item(record: InstalledPythonRecord) -> ListItem {
    ListItem::new(record.version.to_string())
        .with_verbose_line(format!("build: {}", record.build_id))
        .with_verbose_line(format!("target: {}", record.target_triple))
        .with_verbose_line(format!("install dir: {}", record.install_dir))
        .with_verbose_line(format!("python: {}", record.executable_path))
}

fn release_item(release: PythonRelease) -> ListItem {
    ListItem::new(release.version.to_string())
        .with_verbose_line(format!("build: {}", release.build_id))
        .with_verbose_line(format!("target: {}", release.target_triple))
        .with_verbose_line(format!("asset: {}", release.asset_name))
        .with_verbose_line(format!("download: {}", release.download_url))
}
