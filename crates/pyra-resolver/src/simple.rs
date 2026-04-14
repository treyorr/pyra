//! Simple Repository API access.

use std::collections::BTreeMap;
use std::fs;
use std::str::FromStr;

use regex::Regex;
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

impl SimpleFile {
    fn is_yanked(&self) -> bool {
        self.yanked.as_ref().is_some_and(|value| match value {
            serde_json::Value::Bool(flag) => *flag,
            serde_json::Value::Null => false,
            _ => true,
        })
    }

    fn exposes_core_metadata(&self) -> bool {
        self.core_metadata
            .as_ref()
            .is_some_and(|value| match value {
                serde_json::Value::Bool(flag) => *flag,
                serde_json::Value::Null => false,
                _ => true,
            })
    }
}

pub async fn fetch_candidates(
    client: &reqwest::Client,
    index_url: &str,
    package: &str,
    env: &ResolverEnvironment,
) -> Result<Vec<SimpleCandidate>, ResolverError> {
    let normalized = normalize_package_name(package);
    let project_url = project_index_url(index_url, &normalized);
    trace_line(format!(
        "fetch_candidates: package={} normalized={} url={}",
        package, normalized, project_url
    ));
    let body = read_index_text(client, &project_url).await?;
    let project = serde_json::from_str::<SimpleProjectResponse>(&body).map_err(|source| {
        ResolverError::ParseIndex {
            package: package.to_string(),
            source,
        }
    })?;

    let mut grouped = BTreeMap::new();
    for file in project.files {
        if file.is_yanked() {
            trace_line(format!("skip yanked file: {}", file.filename));
            continue;
        }
        if !artifact_compatible(&file, env) {
            trace_line(format!(
                "skip incompatible artifact: {} for target={}",
                file.filename, env.target_triple
            ));
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
                trace_line(format!(
                    "skip requires-python mismatch: {} requires {} but env is {}",
                    file.filename, requires_python, env.python_full_version
                ));
                continue;
            }
        }
        let Some(version) = version_from_filename(package, &file.filename)? else {
            trace_line(format!("skip unparsed version filename: {}", file.filename));
            continue;
        };
        grouped.entry(version).or_insert_with(Vec::new).push(file);
    }

    let mut candidates = Vec::new();
    let mut missing_core_metadata = false;
    for (version, files) in grouped {
        let chosen = choose_best_artifacts(files);
        let Some(metadata_url) = chosen
            .iter()
            .find(|file| file.exposes_core_metadata())
            .map(|file| format!("{}.metadata", file.url))
        else {
            // Keep missing metadata distinct from "no installable artifacts" so
            // resolver tests can verify incomplete index data explicitly.
            missing_core_metadata = true;
            trace_line(format!(
                "no core metadata among chosen artifacts for {}=={}",
                package, version
            ));
            continue;
        };

        trace_line(format!(
            "fetch metadata: {}=={} from {}",
            package, version, metadata_url
        ));
        let metadata = fetch_metadata(client, package, &metadata_url).await?;
        if let Some(requires_python) = &metadata.requires_python {
            if !requires_python.contains(&env.python_version) {
                trace_line(format!(
                    "skip metadata requires-python mismatch: {}=={} requires {} but env is {}",
                    package, version, requires_python, env.python_full_version
                ));
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
            trace_line(format!(
                "skip {}=={} because no chosen artifacts had sha256 hashes",
                package, version
            ));
            continue;
        }

        trace_line(format!(
            "candidate accepted: {}=={} artifacts={} deps={}",
            package,
            version,
            artifacts.len(),
            metadata.dependencies.len()
        ));
        candidates.push(SimpleCandidate {
            version,
            requires_python: metadata.requires_python,
            dependencies: metadata.dependencies,
            artifacts,
        });
    }

    if candidates.is_empty() {
        if missing_core_metadata {
            return Err(ResolverError::MissingCoreMetadata {
                package: package.to_string(),
            });
        }
        return Err(ResolverError::NoInstallableArtifacts {
            package: package.to_string(),
        });
    }

    Ok(candidates)
}

fn project_index_url(index_url: &str, normalized_package: &str) -> String {
    let trimmed = index_url.trim_end_matches('/');
    if file_url_to_path(index_url).is_some() {
        format!("{trimmed}/{normalized_package}.json")
    } else {
        format!("{trimmed}/{normalized_package}/")
    }
}

fn normalize_package_name(package: &str) -> String {
    static NORMALIZE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    NORMALIZE
        .get_or_init(|| Regex::new(r"[-_.]+").expect("valid package normalization regex"))
        .replace_all(&package.to_ascii_lowercase(), "-")
        .into_owned()
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
        .header("Accept", "application/vnd.pypi.simple.v1+json")
        .send()
        .await
        .map_err(|source| ResolverError::RequestIndex {
            url: url.to_string(),
            source,
        })?
        .error_for_status()
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

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|source| ResolverError::RequestMetadata {
            url: url.to_string(),
            source,
        })?
        .error_for_status()
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

fn trace_line(message: String) {
    if std::env::var_os("PYRA_RESOLVER_TRACE").is_some() {
        eprintln!("resolver-trace: {message}");
    }
}

#[cfg(test)]
mod tests {
    use super::{SimpleFile, normalize_package_name, project_index_url};
    use serde_json::json;

    #[test]
    fn normalizes_names_for_simple_api_lookups() {
        assert_eq!(normalize_package_name("Requests_SOCKS"), "requests-socks");
        assert_eq!(normalize_package_name("zope.interface"), "zope-interface");
    }

    #[test]
    fn remote_urls_use_pep_691_project_paths() {
        assert_eq!(
            project_index_url("https://pypi.org/simple", "click"),
            "https://pypi.org/simple/click/"
        );
    }

    #[test]
    fn file_urls_keep_fixture_json_shape() {
        assert_eq!(
            project_index_url("file:///tmp/simple", "click"),
            "file:///tmp/simple/click.json"
        );
    }

    #[test]
    fn false_yanked_flag_does_not_exclude_real_pypi_files() {
        let file = SimpleFile {
            filename: "click-8.3.1-py3-none-any.whl".to_string(),
            url: "https://files.pythonhosted.org/example.whl".to_string(),
            hashes: Default::default(),
            requires_python: Some(">=3.10".to_string()),
            size: Some(1),
            upload_time: None,
            core_metadata: Some(json!({"sha256": "abc"})),
            yanked: Some(json!(false)),
        };

        assert!(!file.is_yanked());
    }

    #[test]
    fn false_core_metadata_flag_does_not_claim_metadata_endpoint() {
        let file = SimpleFile {
            filename: "click-8.3.1.tar.gz".to_string(),
            url: "https://files.pythonhosted.org/example.tar.gz".to_string(),
            hashes: Default::default(),
            requires_python: Some(">=3.10".to_string()),
            size: Some(1),
            upload_time: None,
            core_metadata: Some(json!(false)),
            yanked: Some(json!(false)),
        };

        assert!(!file.exposes_core_metadata());
    }
}
