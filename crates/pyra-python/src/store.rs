//! Local storage for managed Python installations and manifests.

use std::fs;
use std::io::{self, Write};

use camino::{Utf8Path, Utf8PathBuf};
use flate2::read::GzDecoder;
use pyra_core::AppContext;
use tempfile::tempdir_in;

use crate::{
    ArchiveFormat, HostTarget, InstalledPythonRecord, PythonError, PythonRelease,
    PythonVersionRequest,
};

const INSTALLATION_MANIFEST_FILE: &str = "installation.json";

#[derive(Debug, Default, Clone, Copy)]
pub struct PythonInstallStore;

impl PythonInstallStore {
    pub fn list_installed(
        self,
        context: &AppContext,
    ) -> Result<Vec<InstalledPythonRecord>, PythonError> {
        let install_root = context.paths.python_installations_dir();
        let entries = match fs::read_dir(&install_root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(PythonError::ReadInstallDirectory {
                    path: install_root.to_string(),
                    source,
                });
            }
        };

        let mut installations = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| PythonError::InspectInstallEntry {
                path: install_root.to_string(),
                source,
            })?;

            let file_type =
                entry
                    .file_type()
                    .map_err(|source| PythonError::InspectInstallEntry {
                        path: install_root.to_string(),
                        source,
                    })?;
            if !file_type.is_dir() {
                continue;
            }

            let path = Utf8PathBuf::from_path_buf(entry.path())
                .map_err(|path| PythonError::NonUtf8EntryPath { path })?;
            let file_name = entry
                .file_name()
                .into_string()
                .map_err(|name| PythonError::NonUtf8EntryName { name })?;

            let record = self.read_manifest(&path)?;
            if record.install_dir != path {
                return Err(PythonError::InvalidInstallEntry { entry: file_name });
            }

            installations.push(record);
        }

        installations.sort_by(|left, right| left.version.cmp(&right.version));
        Ok(installations)
    }

    pub fn read_existing_install(
        self,
        context: &AppContext,
        version: &crate::PythonVersion,
    ) -> Result<Option<InstalledPythonRecord>, PythonError> {
        let install_dir = context.paths.python_version_dir(&version.to_string());
        if !install_dir.exists() {
            return Ok(None);
        }

        Ok(Some(self.read_manifest(&install_dir)?))
    }

    pub async fn ensure_cached_archive(
        self,
        context: &AppContext,
        release: &PythonRelease,
        archive_bytes: Option<Vec<u8>>,
    ) -> Result<Utf8PathBuf, PythonError> {
        let archive_path = context.paths.python_download_archive(&release.asset_name);

        if archive_path.exists() && self.cached_archive_matches(&archive_path, release).await? {
            return Ok(archive_path);
        }

        let archive_bytes = archive_bytes.expect("archive bytes are required when cache is stale");
        verify_checksum(release, &archive_bytes)?;

        let temp_path = archive_path.with_extension("tmp");
        tokio::fs::write(&temp_path, archive_bytes)
            .await
            .map_err(|source| PythonError::WriteArchive {
                path: temp_path.to_string(),
                source,
            })?;
        tokio::fs::rename(&temp_path, &archive_path)
            .await
            .map_err(|source| PythonError::WriteArchive {
                path: archive_path.to_string(),
                source,
            })?;

        Ok(archive_path)
    }

    pub async fn activate_install(
        self,
        context: &AppContext,
        host: &HostTarget,
        release: &PythonRelease,
        archive_path: &Utf8Path,
    ) -> Result<InstalledPythonRecord, PythonError> {
        let install_dir = context
            .paths
            .python_version_dir(&release.version.to_string());
        let temp_root = tempdir_in(context.paths.state_dir.as_std_path()).map_err(|source| {
            PythonError::CreateStagingDirectory {
                path: context.paths.state_dir.to_string(),
                source,
            }
        })?;
        let staging_dir = Utf8PathBuf::from_path_buf(
            temp_root
                .path()
                .join(format!("{}-staging", release.version)),
        )
        .map_err(|path| PythonError::NonUtf8EntryPath { path })?;
        fs::create_dir_all(&staging_dir).map_err(|source| PythonError::CreateStagingDirectory {
            path: staging_dir.to_string(),
            source,
        })?;

        let archive_path = archive_path.to_owned();
        let archive_path_for_task = archive_path.clone();
        let staging_dir_for_task = staging_dir.clone();
        tokio::task::spawn_blocking(move || {
            extract_archive(&archive_path_for_task, &staging_dir_for_task)
        })
        .await
        .map_err(|join_error| PythonError::ExtractArchive {
            archive: archive_path.to_string(),
            source: io::Error::other(join_error.to_string()),
        })??;

        let executable_path = host.executable_path(&staging_dir);
        if !executable_path.exists() {
            return Err(PythonError::InvalidExtractedArchive {
                path: staging_dir.to_string(),
            });
        }

        let record = InstalledPythonRecord {
            version: release.version,
            implementation: release.implementation.clone(),
            build_id: release.build_id.clone(),
            target_triple: release.target_triple.clone(),
            asset_name: release.asset_name.clone(),
            archive_format: release.archive_format,
            download_url: release.download_url.clone(),
            checksum_sha256: release.checksum_sha256.clone(),
            install_dir: install_dir.clone(),
            executable_path: host.executable_path(&install_dir),
        };
        self.write_manifest(&staging_dir, &record)?;

        fs::rename(&staging_dir, &install_dir).map_err(|source| PythonError::ActivateInstall {
            path: install_dir.to_string(),
            source,
        })?;

        Ok(record)
    }

    pub fn uninstall(
        self,
        installation: &InstalledPythonRecord,
    ) -> Result<InstalledPythonRecord, PythonError> {
        fs::remove_dir_all(&installation.install_dir).map_err(|source| {
            PythonError::RemoveInstall {
                path: installation.install_dir.to_string(),
                source,
            }
        })?;
        Ok(installation.clone())
    }

    pub fn select_installed(
        self,
        installations: &[InstalledPythonRecord],
        selector: &PythonVersionRequest,
    ) -> Result<InstalledPythonRecord, PythonError> {
        let matches = installations
            .iter()
            .filter(|installation| selector.matches(&installation.version))
            .cloned()
            .collect::<Vec<_>>();

        if matches.is_empty() {
            return Err(PythonError::InstalledVersionNotFound {
                request: selector.to_string(),
            });
        }

        if matches.len() > 1 {
            return Err(PythonError::AmbiguousInstalledVersion {
                request: selector.to_string(),
                matches: matches
                    .iter()
                    .map(|installation| installation.version.to_string())
                    .collect(),
            });
        }

        Ok(matches[0].clone())
    }

    fn cached_archive_matches(
        self,
        archive_path: &Utf8Path,
        release: &PythonRelease,
    ) -> impl std::future::Future<Output = Result<bool, PythonError>> + Send {
        let archive_path = archive_path.to_owned();
        let release = release.clone();
        async move {
            let bytes = tokio::fs::read(&archive_path).await.map_err(|source| {
                PythonError::ReadCachedArchive {
                    path: archive_path.to_string(),
                    source,
                }
            })?;

            match verify_checksum(&release, &bytes) {
                Ok(()) => Ok(true),
                Err(PythonError::ChecksumMismatch { .. }) => Ok(false),
                Err(error) => Err(error),
            }
        }
    }

    fn read_manifest(self, install_dir: &Utf8Path) -> Result<InstalledPythonRecord, PythonError> {
        let manifest_path = install_dir.join(INSTALLATION_MANIFEST_FILE);
        let manifest =
            fs::read_to_string(&manifest_path).map_err(|source| PythonError::ReadManifest {
                path: manifest_path.to_string(),
                source,
            })?;

        serde_json::from_str(&manifest).map_err(|source| PythonError::ParseManifest {
            path: manifest_path.to_string(),
            source,
        })
    }

    fn write_manifest(
        self,
        install_dir: &Utf8Path,
        record: &InstalledPythonRecord,
    ) -> Result<(), PythonError> {
        let manifest_path = install_dir.join(INSTALLATION_MANIFEST_FILE);
        let manifest =
            serde_json::to_vec_pretty(record).map_err(|source| PythonError::SerializeManifest {
                path: manifest_path.to_string(),
                source,
            })?;

        let mut file =
            fs::File::create(&manifest_path).map_err(|source| PythonError::WriteManifest {
                path: manifest_path.to_string(),
                source,
            })?;
        file.write_all(&manifest)
            .and_then(|_| file.write_all(b"\n"))
            .map_err(|source| PythonError::WriteManifest {
                path: manifest_path.to_string(),
                source,
            })
    }
}

fn verify_checksum(release: &PythonRelease, bytes: &[u8]) -> Result<(), PythonError> {
    if let Some(expected) = &release.checksum_sha256 {
        use sha2::{Digest, Sha256};

        let actual = format!("{:x}", Sha256::digest(bytes));
        if &actual != expected {
            return Err(PythonError::ChecksumMismatch {
                asset: release.asset_name.clone(),
                expected: expected.clone(),
                actual,
            });
        }
    }

    Ok(())
}

fn extract_archive(archive_path: &Utf8Path, destination: &Utf8Path) -> Result<(), PythonError> {
    match ArchiveFormat::TarGz {
        ArchiveFormat::TarGz => {
            let file =
                fs::File::open(archive_path).map_err(|source| PythonError::ExtractArchive {
                    archive: archive_path.to_string(),
                    source,
                })?;
            let decoder = GzDecoder::new(file);
            let mut archive = tar::Archive::new(decoder);
            archive
                .unpack(destination)
                .map_err(|source| PythonError::ExtractArchive {
                    archive: archive_path.to_string(),
                    source,
                })?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use camino::Utf8PathBuf;
    use flate2::{Compression, write::GzEncoder};
    use pyra_core::{AppContext, AppPaths, Verbosity};

    use super::PythonInstallStore;
    use crate::{ArchiveFormat, InstalledPythonRecord, PythonVersion, PythonVersionRequest};

    #[test]
    fn reads_manifest_back_from_install_directory() {
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).expect("utf-8 path");
        let install_dir = root.join("data").join("pythons").join("3.13.12");
        std::fs::create_dir_all(install_dir.join("python/bin")).expect("install dir");
        std::fs::write(install_dir.join("python/bin/python3"), "python").expect("python binary");

        let record = InstalledPythonRecord {
            version: PythonVersion::parse("3.13.12").unwrap(),
            implementation: "cpython".to_string(),
            build_id: "20260325".to_string(),
            target_triple: "aarch64-apple-darwin".to_string(),
            asset_name: "asset.tar.gz".to_string(),
            archive_format: ArchiveFormat::TarGz,
            download_url: "https://example.test/asset.tar.gz".to_string(),
            checksum_sha256: Some("abc".to_string()),
            install_dir: install_dir.clone(),
            executable_path: install_dir.join("python/bin/python3"),
        };
        std::fs::write(
            install_dir.join("installation.json"),
            serde_json::to_vec_pretty(&record).unwrap(),
        )
        .expect("manifest");

        let paths = AppPaths::from_roots(
            root.join("config"),
            root.join("data"),
            root.join("cache"),
            root.join("state"),
        );
        let context = AppContext::new(root, paths, Verbosity::Normal);
        let installed = PythonInstallStore
            .list_installed(&context)
            .expect("installed");

        assert_eq!(installed, vec![record]);
    }

    #[test]
    fn selector_disambiguation_requires_specific_version() {
        let store = PythonInstallStore;
        let record = |version: &str| InstalledPythonRecord {
            version: PythonVersion::parse(version).unwrap(),
            implementation: "cpython".to_string(),
            build_id: "20260325".to_string(),
            target_triple: "aarch64-apple-darwin".to_string(),
            asset_name: "asset.tar.gz".to_string(),
            archive_format: ArchiveFormat::TarGz,
            download_url: "https://example.test/asset.tar.gz".to_string(),
            checksum_sha256: None,
            install_dir: Utf8PathBuf::from(format!("/tmp/{version}")),
            executable_path: Utf8PathBuf::from(format!("/tmp/{version}/python/bin/python3")),
        };
        let error = store
            .select_installed(
                &[record("3.13.11"), record("3.13.12")],
                &PythonVersionRequest::parse("3.13").unwrap(),
            )
            .expect_err("ambiguous selector");

        assert!(matches!(
            error,
            crate::PythonError::AmbiguousInstalledVersion { .. }
        ));
    }

    #[test]
    fn builds_extractable_install_archive_fixture() {
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let archive_path = temp_dir.path().join("python.tar.gz");
        let file = std::fs::File::create(&archive_path).expect("archive file");
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = tar::Builder::new(encoder);

        let mut header = tar::Header::new_gnu();
        let contents = b"python";
        header.set_path("python/bin/python3").unwrap();
        header.set_mode(0o755);
        header.set_size(contents.len() as u64);
        header.set_cksum();
        builder.append(&header, &contents[..]).unwrap();
        let encoder = builder.into_inner().unwrap();
        let mut file = encoder.finish().unwrap();
        file.flush().unwrap();

        assert!(archive_path.exists());
    }
}
