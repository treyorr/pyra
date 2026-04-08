//! Simple Repository API access.

use std::collections::BTreeMap;
use std::fs;
use std::str::FromStr;

use serde::Deserialize;

use crate::{
    ResolverEnvironment, ResolverError,
    metadata::DistributionMetadata,
    metadata::parse_distribution_metadata,
    version::{artifact_compatible, version_from_filename},
};

#[derive(Debug, Clone)]
pub struct SimpleCandidate {
    pub version: pep440_rs::Version,
    pub requires_python: Option<pep440_rs::VersionSpecifiers>,
    pub dependencies: Vec<pep508_rs::Requirement>,
    pub artifacts: Vec<crate::ArtifactRecord>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SimpleProjectResponse {
    #[serde(default)]
    pub files: Vec<SimpleFile>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SimpleFile {
    pub filename: String,
    pub url: String,
    #[serde(default)]
    pub hashes: BTreeMap<String, String>,
    #[serde(default)]
    pub requires_python: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default, rename = "upload-time")]
    pub upload_time: Option<String>,
    #[serde(default, rename = "core-metadata")]
    pub core_metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub yanked: Option<serde_json::Value>,
}

pub async fn fetch_candidates(
    client: &reqwest::Client,
    index_url: &str,
    package: &str,
    env: &ResolverEnvironment,
) -> Result<Vec<SimpleCandidate>, ResolverError> {
    let normalized = package.to_string();
    let project_url = format!("{}/{}.json", index_url.trim_end_matches('/'), normalized);
    let body = read_index_text(client, &project_url).await?;
    let project = serde_json::from_str::<SimpleProjectResponse>(&body).map_err(|source| {
        ResolverError::ParseIndex {
            package: package.to_string(),
            source,
        }
    })?;

    let mut grouped = BTreeMap::new();
    for file in project.files {
        if file.yanked.is_some() || !artifact_compatible(&file, env) {
            continue;
        }
        if let Some(requires_python) = &file.requires_python {
            let specifiers =
                pep440_rs::VersionSpecifiers::from_str(requires_python).map_err(|_| {
                    ResolverError::ParseVersion {
                        package: package.to_string(),
                        value: requires_python.clone(),
                    }
                })?;
            if !specifiers.contains(&env.python_version) {
                continue;
            }
        }
        let Some(version) = version_from_filename(package, &file.filename)? else {
            continue;
        };
        grouped.entry(version).or_insert_with(Vec::new).push(file);
    }

    let mut candidates = Vec::new();
    for (version, files) in grouped {
        let chosen = choose_best_artifacts(files);
        let Some(metadata_url) = chosen
            .iter()
            .find(|file| file.core_metadata.is_some())
            .map(|file| format!("{}.metadata", file.url))
        else {
            continue;
        };

        let metadata = fetch_metadata(client, package, &metadata_url).await?;
        if let Some(requires_python) = &metadata.requires_python {
            if !requires_python.contains(&env.python_version) {
                continue;
            }
        }

        let mut artifacts = Vec::new();
        for file in chosen {
            let Some(sha256) = file.hashes.get("sha256").cloned() else {
                continue;
            };
            artifacts.push(crate::ArtifactRecord {
                kind: if file.filename.ends_with(".whl") {
                    crate::ArtifactKind::Wheel
                } else {
                    crate::ArtifactKind::Sdist
                },
                name: file.filename.clone(),
                url: file.url.clone(),
                size: file.size,
                upload_time: file.upload_time.clone(),
                sha256,
            });
        }
        if artifacts.is_empty() {
            continue;
        }

        candidates.push(SimpleCandidate {
            version,
            requires_python: metadata.requires_python,
            dependencies: metadata.dependencies,
            artifacts,
        });
    }

    if candidates.is_empty() {
        return Err(ResolverError::NoInstallableArtifacts {
            package: package.to_string(),
        });
    }

    Ok(candidates)
}

async fn fetch_metadata(
    client: &reqwest::Client,
    package: &str,
    url: &str,
) -> Result<DistributionMetadata, ResolverError> {
    let contents = read_metadata_text(client, url).await?;
    parse_distribution_metadata(package, &contents)
}

fn choose_best_artifacts(files: Vec<SimpleFile>) -> Vec<SimpleFile> {
    let mut wheels = files
        .iter()
        .filter(|file| file.filename.ends_with(".whl"))
        .cloned()
        .collect::<Vec<_>>();
    wheels.sort_by(|left, right| left.filename.cmp(&right.filename));
    if !wheels.is_empty() {
        return wheels;
    }
    files
        .into_iter()
        .filter(|file| file.filename.ends_with(".tar.gz") || file.filename.ends_with(".zip"))
        .collect()
}

async fn read_index_text(client: &reqwest::Client, url: &str) -> Result<String, ResolverError> {
    if let Some(path) = file_url_to_path(url) {
        return fs::read_to_string(&path).map_err(|source| ResolverError::ReadIndexFile {
            path: path.display().to_string(),
            source,
        });
    }
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|source| ResolverError::RequestIndex {
            url: url.to_string(),
            source,
        })?;
    response
        .text()
        .await
        .map_err(|source| ResolverError::RequestIndex {
            url: url.to_string(),
            source,
        })
}

async fn read_metadata_text(client: &reqwest::Client, url: &str) -> Result<String, ResolverError> {
    if let Some(path) = file_url_to_path(url) {
        return fs::read_to_string(&path).map_err(|source| ResolverError::ReadMetadataFile {
            path: path.display().to_string(),
            source,
        });
    }

    let response =
        client
            .get(url)
            .send()
            .await
            .map_err(|source| ResolverError::RequestMetadata {
                url: url.to_string(),
                source,
            })?;
    response
        .text()
        .await
        .map_err(|source| ResolverError::RequestMetadata {
            url: url.to_string(),
            source,
        })
}

fn file_url_to_path(url: &str) -> Option<std::path::PathBuf> {
    let parsed = url::Url::parse(url).ok()?;
    if parsed.scheme() != "file" {
        return None;
    }
    parsed.to_file_path().ok()
}
