//! Request and outcome types for the public Python service facade.

use crate::{InstallDisposition, InstalledPythonRecord, PythonRelease, PythonVersionRequest};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SearchPythonRequest {
    pub selector: Option<PythonVersionRequest>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SearchPythonOutcome {
    pub releases: Vec<PythonRelease>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ListInstalledPythonsOutcome {
    pub installations: Vec<InstalledPythonRecord>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct InstallPythonRequest {
    pub selector: PythonVersionRequest,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct InstallPythonOutcome {
    pub installation: InstalledPythonRecord,
    pub release: PythonRelease,
    pub disposition: InstallDisposition,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UninstallPythonRequest {
    pub selector: PythonVersionRequest,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UninstallPythonOutcome {
    pub removed: InstalledPythonRecord,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UsePythonRequest {
    pub selector: PythonVersionRequest,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UsePythonOutcome {
    pub selector: PythonVersionRequest,
}
