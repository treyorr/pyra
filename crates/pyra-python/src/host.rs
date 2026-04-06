//! Host detection and target mapping for python-build-standalone assets.

use camino::Utf8PathBuf;

use crate::{ArchiveFormat, PythonError};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct HostTarget {
    target_triple: &'static str,
    display_name: &'static str,
    executable_relative_path: &'static str,
    archive_format: ArchiveFormat,
}

impl HostTarget {
    pub fn detect() -> Result<Self, PythonError> {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        let host = Some(Self {
            target_triple: "aarch64-apple-darwin",
            display_name: "macOS arm64",
            executable_relative_path: "python/bin/python3",
            archive_format: ArchiveFormat::TarGz,
        });
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        let host = Some(Self {
            target_triple: "x86_64-apple-darwin",
            display_name: "macOS x86_64",
            executable_relative_path: "python/bin/python3",
            archive_format: ArchiveFormat::TarGz,
        });
        #[cfg(all(target_os = "linux", target_arch = "aarch64", target_env = "gnu"))]
        let host = Some(Self {
            target_triple: "aarch64-unknown-linux-gnu",
            display_name: "Linux aarch64 GNU",
            executable_relative_path: "python/bin/python3",
            archive_format: ArchiveFormat::TarGz,
        });
        #[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
        let host = Some(Self {
            target_triple: "x86_64-unknown-linux-gnu",
            display_name: "Linux x86_64 GNU",
            executable_relative_path: "python/bin/python3",
            archive_format: ArchiveFormat::TarGz,
        });
        #[cfg(not(any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "macos", target_arch = "x86_64"),
            all(target_os = "linux", target_arch = "aarch64", target_env = "gnu"),
            all(target_os = "linux", target_arch = "x86_64", target_env = "gnu")
        )))]
        let host: Option<Self> = None;

        host.ok_or_else(|| PythonError::UnsupportedHost {
            host: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        })
    }

    pub fn target_triple(&self) -> &str {
        self.target_triple
    }

    pub fn display_name(&self) -> &str {
        self.display_name
    }

    pub fn executable_relative_path(&self) -> &str {
        self.executable_relative_path
    }

    pub fn executable_path(&self, install_dir: &camino::Utf8Path) -> Utf8PathBuf {
        install_dir.join(self.executable_relative_path)
    }

    pub fn archive_format(&self) -> ArchiveFormat {
        self.archive_format
    }
}

#[cfg(test)]
mod tests {
    use super::HostTarget;

    #[test]
    fn supported_host_maps_to_expected_archive_format() {
        let host = HostTarget::detect();
        if let Ok(host) = host {
            assert_eq!(host.archive_format().suffix(), ".tar.gz");
            assert!(
                host.executable_relative_path()
                    .contains("python/bin/python3")
            );
        }
    }
}
