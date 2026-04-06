//! Managed Python domain logic for Pyra.
//!
//! This crate provides the standardized pattern future Pyra subsystems should
//! follow: typed requests, typed outcomes, typed errors, and a small facade that
//! hides filesystem and network details behind domain-level operations.

mod client;
mod error;
mod host;
mod model;
mod requests;
mod service;
mod store;
mod version;

pub use client::PythonCatalogClient;
pub use error::PythonError;
pub use host::HostTarget;
pub use model::{ArchiveFormat, InstallDisposition, InstalledPythonRecord, PythonRelease};
pub use requests::{
    InstallPythonOutcome, InstallPythonRequest, ListInstalledPythonsOutcome, SearchPythonOutcome,
    SearchPythonRequest, UninstallPythonOutcome, UninstallPythonRequest, UsePythonOutcome,
    UsePythonRequest,
};
pub use service::PythonService;
pub use store::PythonInstallStore;
pub use version::{PythonVersion, PythonVersionRequest};
