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
    /// Prepare a managed Python install location for a version request.
    Install(PythonInstallArgs),
}

#[derive(Debug, Args)]
pub struct PythonInstallArgs {
    /// Python version request like 3, 3.13, or 3.13.2.
    pub version: String,
}

#[derive(Debug, Args)]
pub struct InitArgs {
    /// Pin a Pyra-managed Python version in the generated pyproject.toml.
    #[arg(long)]
    pub python: Option<String>,
}
