//! Public service facade for managed Python workflows.

use pyra_core::AppContext;

use crate::{
    HostTarget, InstallDisposition, InstallPythonOutcome, InstallPythonRequest,
    ListInstalledPythonsOutcome, PythonCatalogClient, PythonError, PythonInstallStore,
    PythonRelease, SearchPythonOutcome, SearchPythonRequest, UninstallPythonOutcome,
    UninstallPythonRequest,
};

#[derive(Debug, Clone)]
pub struct PythonService {
    catalog: PythonCatalogClient,
    store: PythonInstallStore,
}

impl PythonService {
    pub fn new() -> Self {
        Self {
            catalog: PythonCatalogClient::new(),
            store: PythonInstallStore,
        }
    }

    #[cfg(test)]
    pub fn with_api_base_url(api_base_url: impl Into<String>) -> Self {
        Self {
            catalog: PythonCatalogClient::with_api_base_url(api_base_url),
            store: PythonInstallStore,
        }
    }

    #[cfg(test)]
    pub fn with_catalog_path(catalog_path: impl Into<String>) -> Self {
        Self {
            catalog: PythonCatalogClient::with_catalog_path(catalog_path),
            store: PythonInstallStore,
        }
    }

    pub async fn list_installed(
        &self,
        context: &AppContext,
    ) -> Result<ListInstalledPythonsOutcome, PythonError> {
        let installations = self.store.list_installed(context)?;
        Ok(ListInstalledPythonsOutcome { installations })
    }

    pub async fn search(
        &self,
        _context: &AppContext,
        request: SearchPythonRequest,
    ) -> Result<SearchPythonOutcome, PythonError> {
        let host = HostTarget::detect()?;
        let mut releases = self.catalog.fetch_releases(&host).await?;
        if let Some(selector) = request.selector {
            releases.retain(|release| selector.matches(&release.version));
        }

        Ok(SearchPythonOutcome { releases })
    }

    pub async fn install(
        &self,
        context: &AppContext,
        request: InstallPythonRequest,
    ) -> Result<InstallPythonOutcome, PythonError> {
        let host = HostTarget::detect()?;
        let release = self.resolve_release(&request.selector, &host).await?;

        if let Some(existing) = self
            .store
            .read_existing_install(context, &release.version)?
        {
            return Ok(InstallPythonOutcome {
                installation: existing,
                release,
                disposition: InstallDisposition::AlreadyInstalled,
            });
        }

        let archive_bytes = self.catalog.download_release(&release).await?;
        let archive_path = self
            .store
            .ensure_cached_archive(context, &release, Some(archive_bytes))
            .await?;
        let installation = self
            .store
            .activate_install(context, &host, &release, &archive_path)
            .await?;

        Ok(InstallPythonOutcome {
            installation,
            release,
            disposition: InstallDisposition::Installed,
        })
    }

    pub async fn uninstall(
        &self,
        context: &AppContext,
        request: UninstallPythonRequest,
    ) -> Result<UninstallPythonOutcome, PythonError> {
        let installations = self.store.list_installed(context)?;
        let installation = self
            .store
            .select_installed(&installations, &request.selector)?;
        let removed = self.store.uninstall(&installation)?;
        Ok(UninstallPythonOutcome { removed })
    }

    async fn resolve_release(
        &self,
        selector: &crate::PythonVersionRequest,
        host: &HostTarget,
    ) -> Result<PythonRelease, PythonError> {
        let releases = self.catalog.fetch_releases(host).await?;
        releases
            .into_iter()
            .find(|release| selector.matches(&release.version))
            .ok_or_else(|| PythonError::NoMatchingRelease {
                request: selector.to_string(),
                host: host.display_name().to_string(),
            })
    }
}

impl Default for PythonService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use camino::Utf8PathBuf;
    use flate2::{Compression, write::GzEncoder};
    use pyra_core::{AppContext, AppPaths, Verbosity};

    use super::PythonService;
    use crate::{
        InstallDisposition, InstallPythonRequest, PythonVersion, PythonVersionRequest,
        SearchPythonRequest, UninstallPythonRequest,
    };

    #[tokio::test]
    async fn installs_and_lists_managed_python_versions() {
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let fixture = write_catalog_fixture(temp_dir.path(), &["3.13.12"]);
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).expect("utf-8 path");
        let paths = AppPaths::from_roots(
            root.join("config"),
            root.join("data"),
            root.join("cache"),
            root.join("state"),
        );
        paths.ensure_base_layout().expect("base layout");
        let context = AppContext::new(root, paths, Verbosity::Normal);
        let service = PythonService::with_catalog_path(fixture.catalog_path);

        let outcome = service
            .install(
                &context,
                InstallPythonRequest {
                    selector: PythonVersionRequest::parse("3.13").unwrap(),
                },
            )
            .await
            .expect("install");

        assert_eq!(outcome.disposition, InstallDisposition::Installed);
        assert_eq!(
            outcome.installation.version,
            PythonVersion::parse("3.13.12").unwrap()
        );

        let installed = service.list_installed(&context).await.expect("list");
        assert_eq!(installed.installations.len(), 1);
        assert!(installed.installations[0].executable_path.exists());
    }

    #[tokio::test]
    async fn reinstall_is_idempotent() {
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let fixture = write_catalog_fixture(temp_dir.path(), &["3.13.12"]);
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).expect("utf-8 path");
        let paths = AppPaths::from_roots(
            root.join("config"),
            root.join("data"),
            root.join("cache"),
            root.join("state"),
        );
        paths.ensure_base_layout().expect("base layout");
        let context = AppContext::new(root, paths, Verbosity::Normal);
        let service = PythonService::with_catalog_path(fixture.catalog_path);

        service
            .install(
                &context,
                InstallPythonRequest {
                    selector: PythonVersionRequest::parse("3.13").unwrap(),
                },
            )
            .await
            .expect("first install");
        let second = service
            .install(
                &context,
                InstallPythonRequest {
                    selector: PythonVersionRequest::parse("3.13").unwrap(),
                },
            )
            .await
            .expect("second install");

        assert_eq!(second.disposition, InstallDisposition::AlreadyInstalled);
    }

    #[tokio::test]
    async fn search_filters_by_selector() {
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let fixture = write_catalog_fixture(temp_dir.path(), &["3.13.12", "3.12.13"]);
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).expect("utf-8 path");
        let paths = AppPaths::from_roots(
            root.join("config"),
            root.join("data"),
            root.join("cache"),
            root.join("state"),
        );
        let context = AppContext::new(root, paths, Verbosity::Normal);
        let service = PythonService::with_catalog_path(fixture.catalog_path);

        let outcome = service
            .search(
                &context,
                SearchPythonRequest {
                    selector: Some(PythonVersionRequest::parse("3.13").unwrap()),
                },
            )
            .await
            .expect("search");

        assert_eq!(outcome.releases.len(), 1);
        assert_eq!(
            outcome.releases[0].version,
            PythonVersion::parse("3.13.12").unwrap()
        );
    }

    #[tokio::test]
    async fn uninstall_removes_selected_installation() {
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let fixture = write_catalog_fixture(temp_dir.path(), &["3.13.12"]);
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).expect("utf-8 path");
        let paths = AppPaths::from_roots(
            root.join("config"),
            root.join("data"),
            root.join("cache"),
            root.join("state"),
        );
        paths.ensure_base_layout().expect("base layout");
        let context = AppContext::new(root, paths, Verbosity::Normal);
        let service = PythonService::with_catalog_path(fixture.catalog_path);

        let install = service
            .install(
                &context,
                InstallPythonRequest {
                    selector: PythonVersionRequest::parse("3.13").unwrap(),
                },
            )
            .await
            .expect("install");

        let outcome = service
            .uninstall(
                &context,
                UninstallPythonRequest {
                    selector: PythonVersionRequest::parse("3.13.12").unwrap(),
                },
            )
            .await
            .expect("uninstall");

        assert_eq!(outcome.removed.version, install.installation.version);
        assert!(!install.installation.install_dir.exists());
    }

    struct FixturePaths {
        catalog_path: String,
    }

    fn write_catalog_fixture(root: &std::path::Path, versions: &[&str]) -> FixturePaths {
        let archive_path = root.join("python-install.tar.gz");
        fs::write(&archive_path, install_archive_fixture()).expect("archive fixture");
        let archive_url = format!("file://{}", archive_path.display());
        let digest = sha256_hex(&fs::read(&archive_path).expect("archive bytes"));

        let assets = versions
            .iter()
            .map(|version| {
                serde_json::json!({
                    "name": host_asset_name(version),
                    "browser_download_url": archive_url,
                    "digest": format!("sha256:{digest}")
                })
            })
            .collect::<Vec<_>>();
        let body = serde_json::json!({ "assets": assets });
        let catalog_path = root.join("catalog.json");
        fs::write(&catalog_path, serde_json::to_vec_pretty(&body).unwrap()).expect("catalog");

        FixturePaths {
            catalog_path: catalog_path.display().to_string(),
        }
    }

    fn install_archive_fixture() -> Vec<u8> {
        let mut writer = Vec::new();
        let encoder = GzEncoder::new(&mut writer, Compression::default());
        let mut builder = tar::Builder::new(encoder);

        let mut header = tar::Header::new_gnu();
        let contents = b"python";
        header.set_path("python/bin/python3").unwrap();
        header.set_mode(0o755);
        header.set_size(contents.len() as u64);
        header.set_cksum();
        builder.append(&header, &contents[..]).unwrap();
        builder.finish().unwrap();
        let encoder = builder.into_inner().unwrap();
        encoder.finish().unwrap();

        writer
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};

        format!("{:x}", Sha256::digest(bytes))
    }

    fn host_asset_name(version: &str) -> String {
        let target = crate::HostTarget::detect()
            .expect("supported host")
            .target_triple()
            .to_string();
        format!("cpython-{version}+20260325-{target}-install_only.tar.gz")
    }
}
