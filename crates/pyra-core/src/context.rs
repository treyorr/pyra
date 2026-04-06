use std::env;

use camino::Utf8PathBuf;

use crate::{AppPaths, CoreError, Verbosity};

#[derive(Debug, Clone)]
pub struct AppContext {
    pub cwd: Utf8PathBuf,
    pub paths: AppPaths,
    pub verbosity: Verbosity,
}

impl AppContext {
    pub fn discover(verbosity: Verbosity) -> Result<Self, CoreError> {
        let cwd = env::current_dir()
            .map_err(|source| CoreError::CurrentDirectoryUnavailable { source })?;
        let cwd = Utf8PathBuf::from_path_buf(cwd).map_err(|path| CoreError::NonUtf8Path {
            label: "current working directory",
            path,
        })?;

        Ok(Self {
            cwd,
            paths: AppPaths::discover()?,
            verbosity,
        })
    }

    pub fn new(cwd: Utf8PathBuf, paths: AppPaths, verbosity: Verbosity) -> Self {
        Self {
            cwd,
            paths,
            verbosity,
        }
    }
}
