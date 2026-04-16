//! Live upstream catalog client for python-build-standalone releases.

use std::cmp::Reverse;

use serde::Deserialize;

use crate::{ArchiveFormat, HostTarget, PythonError, PythonRelease, PythonVersion};

const DEFAULT_RELEASE_API_BASE_URL: &str = "https://api.github.com";
const RELEASES_LATEST_PATH: &str = "/repos/astral-sh/python-build-standalone/releases/latest";
const API_OVERRIDE_ENV: &str = "PYRA_PYTHON_RELEASE_API_BASE_URL";
const CATALOG_PATH_OVERRIDE_ENV: &str = "PYRA_PYTHON_RELEASE_CATALOG_PATH";

#[derive(Debug, Clone)]
pub struct PythonCatalogClient {
    client: reqwest::Client,
    api_base_url: String,
    catalog_path_override: Option<String>,
}

impl PythonCatalogClient {
    pub fn new() -> Self {
        Self::with_api_base_url(
            std::env::var(API_OVERRIDE_ENV)
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_RELEASE_API_BASE_URL.to_string()),
        )
    }

    pub fn with_api_base_url(api_base_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_base_url: api_base_url.into().trim_end_matches('/').to_string(),
            catalog_path_override: None,
        }
    }

    #[cfg(test)]
    pub fn with_catalog_path(catalog_path: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_base_url: DEFAULT_RELEASE_API_BASE_URL.to_string(),
            catalog_path_override: Some(catalog_path.into()),
        }
    }

    pub async fn fetch_releases(
        &self,
        host: &HostTarget,
    ) -> Result<Vec<PythonRelease>, PythonError> {
        if let Some(catalog_path) = self.catalog_path_override.clone().or_else(|| {
            std::env::var(CATALOG_PATH_OVERRIDE_ENV)
                .ok()
                .filter(|value| !value.trim().is_empty())
        }) {
            let body = tokio::fs::read_to_string(&catalog_path)
                .await
                .map_err(|source| PythonError::ReadCatalogFile {
                    path: catalog_path.clone(),
                    source,
                })?;
            return parse_release_body(&body, host);
        }

        let url = format!("{}{}", self.api_base_url, RELEASES_LATEST_PATH);
        let response = self
            .client
            .get(&url)
            .header(reqwest::header::USER_AGENT, "pyra/0.1.0")
            .send()
            .await
            .map_err(|source| PythonError::CatalogRequest {
                url: url.clone(),
                source,
            })?
            .error_for_status()
            .map_err(|source| PythonError::CatalogRequest {
                url: url.clone(),
                source,
            })?;

        let body = response
            .text()
            .await
            .map_err(|source| PythonError::CatalogRequest {
                url: url.clone(),
                source,
            })?;
        parse_release_body(&body, host)
    }

    pub async fn download_release(&self, release: &PythonRelease) -> Result<Vec<u8>, PythonError> {
        if let Some(path) = release.download_url.strip_prefix("file://") {
            return tokio::fs::read(path)
                .await
                .map_err(|source| PythonError::ReadLocalArchive {
                    path: path.to_string(),
                    source,
                });
        }

        let response = self
            .client
            .get(&release.download_url)
            .header(reqwest::header::USER_AGENT, "pyra/0.1.0")
            .send()
            .await
            .map_err(|source| PythonError::DownloadArchive {
                url: release.download_url.clone(),
                source,
            })?
            .error_for_status()
            .map_err(|source| PythonError::DownloadArchive {
                url: release.download_url.clone(),
                source,
            })?;

        let bytes = response
            .bytes()
            .await
            .map_err(|source| PythonError::DownloadArchive {
                url: release.download_url.clone(),
                source,
            })?;
        Ok(bytes.to_vec())
    }
}

impl Default for PythonCatalogClient {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_release_body(body: &str, host: &HostTarget) -> Result<Vec<PythonRelease>, PythonError> {
    let release: GitHubRelease =
        serde_json::from_str(body).map_err(|source| PythonError::CatalogParse { source })?;

    let mut releases = release
        .assets
        .into_iter()
        .filter_map(|asset| parse_asset(asset, host).transpose())
        .collect::<Result<Vec<_>, _>>()?;
    releases.sort_by_key(|release| Reverse(release.version));
    Ok(releases)
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    assets: Vec<GitHubReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubReleaseAsset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
}

fn parse_asset(
    asset: GitHubReleaseAsset,
    host: &HostTarget,
) -> Result<Option<PythonRelease>, PythonError> {
    let suffix = format!("-install_only{}", host.archive_format().suffix());
    if !asset.name.starts_with("cpython-") || !asset.name.ends_with(&suffix) {
        return Ok(None);
    }

    let trimmed = asset
        .name
        .trim_end_matches(host.archive_format().suffix())
        .trim_end_matches("-install_only");
    let trimmed =
        trimmed
            .strip_prefix("cpython-")
            .ok_or_else(|| PythonError::InvalidInstallEntry {
                entry: asset.name.clone(),
            })?;

    let (version, build_and_target) =
        trimmed
            .split_once('+')
            .ok_or_else(|| PythonError::InvalidInstallEntry {
                entry: asset.name.clone(),
            })?;
    let (build_id, target_triple) =
        build_and_target
            .split_once('-')
            .ok_or_else(|| PythonError::InvalidInstallEntry {
                entry: asset.name.clone(),
            })?;

    if target_triple != host.target_triple() {
        return Ok(None);
    }

    let version = match PythonVersion::parse(version) {
        Ok(version) => version,
        Err(PythonError::InvalidConcreteVersion { .. }) => return Ok(None),
        Err(error) => return Err(error),
    };

    Ok(Some(PythonRelease {
        version,
        implementation: "cpython".to_string(),
        build_id: build_id.to_string(),
        target_triple: target_triple.to_string(),
        asset_name: asset.name,
        archive_format: ArchiveFormat::TarGz,
        download_url: asset.browser_download_url,
        checksum_sha256: asset
            .digest
            .and_then(|digest| digest.strip_prefix("sha256:").map(str::to_string)),
    }))
}

#[cfg(test)]
mod tests {
    use super::parse_release_body;
    use crate::{HostTarget, PythonVersion};

    #[test]
    fn parses_installable_assets_for_host() {
        let body = serde_json::json!({
            "assets": [
                {
                    "name": "cpython-3.13.12+20260325-aarch64-apple-darwin-install_only.tar.gz",
                    "browser_download_url": "file:///tmp/python.tar.gz",
                    "digest": "sha256:abc123"
                },
                {
                    "name": "cpython-3.15.0a7+20260325-aarch64-apple-darwin-install_only.tar.gz",
                    "browser_download_url": "file:///tmp/prerelease.tar.gz",
                    "digest": "sha256:def456"
                },
                {
                    "name": "cpython-3.13.12+20260325-x86_64-apple-darwin-install_only.tar.gz",
                    "browser_download_url": "file:///tmp/other-host.tar.gz",
                    "digest": "sha256:ghi789"
                }
            ]
        });
        let host = HostTarget::detect().expect("supported host");
        if host.target_triple() == "aarch64-apple-darwin" {
            let releases = parse_release_body(&body.to_string(), &host).expect("releases");
            assert_eq!(releases.len(), 1);
            assert_eq!(
                releases[0].version,
                PythonVersion::parse("3.13.12").unwrap()
            );
            assert_eq!(releases[0].checksum_sha256.as_deref(), Some("abc123"));
        }
    }
}
