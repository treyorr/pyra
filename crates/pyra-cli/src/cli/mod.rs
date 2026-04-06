use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "pyra",
    version,
    about = "Pyra is a modern Python toolchain.",
    long_about = None
)]
pub struct Cli {
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Manage Pyra-owned Python installations.
    Python(PythonArgs),
    /// Initialize a new Python project in the current directory.
    Init(InitArgs),
}

#[derive(Debug, Args)]
pub struct PythonArgs {
    #[command(subcommand)]
    pub command: PythonCommand,
}

#[derive(Debug, Subcommand)]
pub enum PythonCommand {
    /// List the Python versions currently managed by Pyra.
    List,
    /// Search for installable Python versions for the current host.
    Search(PythonSearchArgs),
    /// Download and install a managed Python version for the current host.
    Install(PythonInstallArgs),
    /// Remove a managed Python installation.
    Uninstall(PythonUninstallArgs),
}

#[derive(Debug, Args)]
pub struct PythonInstallArgs {
    /// Python version request like 3, 3.13, or 3.13.2.
    pub version: String,
}

#[derive(Debug, Args)]
pub struct PythonSearchArgs {
    /// Optional version selector like 3, 3.13, or 3.13.2.
    pub version: Option<String>,
}

#[derive(Debug, Args)]
pub struct PythonUninstallArgs {
    /// Installed Python version selector like 3.13.12.
    pub version: String,
}

#[derive(Debug, Args)]
pub struct InitArgs {
    /// Pin a Pyra-managed Python version in the generated pyproject.toml.
    #[arg(long)]
    pub python: Option<String>,
}
