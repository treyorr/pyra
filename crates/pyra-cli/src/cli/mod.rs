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
    /// Emit machine-readable JSON envelopes instead of human-readable terminal output.
    #[arg(long, global = true)]
    pub json: bool,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Manage Pyra-owned Python installations.
    Python(PythonArgs),
    /// Manage the installed Pyra CLI binary.
    #[command(name = "self")]
    Self_(SelfArgs),
    /// Initialize a new Python project in the current directory.
    Init(InitArgs),
    /// Pin a managed Python version for the current project.
    Use(UseArgs),
    /// Add a dependency declaration to `pyproject.toml` and sync the project.
    Add(AddArgs),
    /// Remove a dependency declaration from `pyproject.toml` and sync the project.
    Remove(RemoveArgs),
    /// Reconcile the centralized environment from `pyproject.toml` and `pylock.toml`.
    Sync(SyncArgs),
    /// Generate or refresh `pylock.toml` without reconciling the environment.
    Lock(LockArgs),
    /// Diagnose project lock and environment health without mutating state.
    Doctor(DoctorArgs),
    /// Report newer available package versions without mutating project state.
    Outdated(OutdatedArgs),
    /// Refresh lock state to newer versions allowed by existing specifiers.
    Update(UpdateArgs),
    /// Execute a project command through the synchronized centralized environment.
    Run(RunArgs),
}

#[derive(Debug, Args)]
pub struct PythonArgs {
    #[command(subcommand)]
    pub command: PythonCommand,
}

#[derive(Debug, Args)]
pub struct SelfArgs {
    #[command(subcommand)]
    pub command: SelfCommand,
}

#[derive(Debug, Subcommand)]
pub enum SelfCommand {
    /// Update the installed Pyra binary from GitHub Releases.
    Update(SelfUpdateArgs),
}

#[derive(Debug, Args, Default)]
pub struct SelfUpdateArgs {}

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

#[derive(Debug, Args)]
pub struct UseArgs {
    /// Python version request like 3, 3.13, or 3.13.2.
    pub version: String,
}

#[derive(Debug, Args)]
pub struct AddArgs {
    /// PEP 508 requirement like `rich>=13` or `httpx[socks]==0.27.0`.
    pub requirement: String,
    /// Add the dependency to a named dependency group.
    #[arg(long, conflicts_with = "extra")]
    pub group: Option<String>,
    /// Add the dependency to a named optional dependency / extra.
    #[arg(long, conflicts_with = "group")]
    pub extra: Option<String>,
}

#[derive(Debug, Args)]
pub struct RemoveArgs {
    /// Package name to remove from the selected declaration scope.
    pub package: String,
    /// Remove the dependency from a named dependency group.
    #[arg(long, conflicts_with = "extra")]
    pub group: Option<String>,
    /// Remove the dependency from a named optional dependency / extra.
    #[arg(long, conflicts_with = "group")]
    pub extra: Option<String>,
}

#[derive(Debug, Args)]
pub struct SyncArgs {
    /// Require an existing fresh lock file and never regenerate it.
    #[arg(long, conflicts_with = "frozen")]
    pub locked: bool,
    /// Require an existing fresh lock file and use it without rewriting.
    #[arg(long, conflicts_with = "locked")]
    pub frozen: bool,
    /// Add one lock-generation target triple for this sync invocation only.
    #[arg(long = "target")]
    pub targets: Vec<String>,
    /// Include a dependency group in addition to the defaults.
    #[arg(long = "group")]
    pub groups: Vec<String>,
    /// Include an extra from `[project.optional-dependencies]`.
    #[arg(long = "extra")]
    pub extras: Vec<String>,
    /// Include all dependency groups.
    #[arg(long)]
    pub all_groups: bool,
    /// Include all extras.
    #[arg(long)]
    pub all_extras: bool,
    /// Exclude a dependency group after applying inclusions.
    #[arg(long = "no-group")]
    pub no_groups: Vec<String>,
    /// Exclude the `dev` dependency group if it exists.
    #[arg(long)]
    pub no_dev: bool,
    /// Sync only the specified dependency groups and exclude base dependencies.
    #[arg(long = "only-group")]
    pub only_groups: Vec<String>,
    /// Sync only the `dev` dependency group and exclude base dependencies.
    #[arg(long)]
    pub only_dev: bool,
}

#[derive(Debug, Args)]
pub struct LockArgs {
    /// Add one lock-generation target triple for this lock invocation only.
    #[arg(long = "target")]
    pub targets: Vec<String>,
}

#[derive(Debug, Args)]
pub struct DoctorArgs {}

#[derive(Debug, Args)]
pub struct OutdatedArgs {}

#[derive(Debug, Args)]
pub struct UpdateArgs {
    /// Show the lock rewrite summary without writing `pylock.toml`.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
#[command(trailing_var_arg = true)]
pub struct RunArgs {
    /// Project script, installed console script, or `.py` file to execute.
    pub target: String,
    /// Arguments forwarded unchanged to the executed child process.
    #[arg(allow_hyphen_values = true)]
    pub args: Vec<String>,
}
