//! Hermetic file-backed Simple API fixtures for resolver tests.
//!
//! The roadmap calls for resolver hardening through local fixtures rather than
//! CLI-driven or network-backed tests. This module keeps that test surface close
//! to the resolver crate while staying flexible enough for future marker and
//! root-membership cases.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use pep508_rs::MarkerEnvironmentBuilder;
use serde::{Deserialize, Serialize};
use tempfile::TempDir;
use url::Url;

use crate::{
    ResolutionRequest, ResolutionRoot, ResolutionRootToken, ResolutionRootTokenKind,
    ResolvedPackage, Resolver, ResolverEnvironment, ResolverError,
};

pub(crate) struct ResolverFixtureHarness {
    _temp_dir: TempDir,
    index_dir: PathBuf,
    artifacts_dir: PathBuf,
}

impl ResolverFixtureHarness {
    pub(crate) fn new() -> std::io::Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        let index_dir = temp_dir.path().join("simple");
        let artifacts_dir = temp_dir.path().join("artifacts");
        fs::create_dir_all(&index_dir)?;
        fs::create_dir_all(&artifacts_dir)?;
        Ok(Self {
            _temp_dir: temp_dir,
            index_dir,
            artifacts_dir,
        })
    }

    pub(crate) fn add_package(&self, package: PackageFixture) -> std::io::Result<()> {
        let mut files = Vec::new();
        for (position, artifact) in package.artifacts.iter().enumerate() {
            let filename = artifact.filename(&package.name);
            let artifact_path = self.artifacts_dir.join(&filename);
            let metadata_path = self.artifacts_dir.join(format!("{filename}.metadata"));

            fs::write(&artifact_path, format!("fixture artifact: {filename}\n"))?;
            if artifact.core_metadata {
                fs::write(&metadata_path, artifact.metadata_contents())?;
            }

            files.push(SimpleFixtureFile {
                filename,
                url: Url::from_file_path(&artifact_path)
                    .expect("fixture artifact path")
                    .to_string(),
                hashes: BTreeMap::from([("sha256".to_string(), format!("{:064x}", position + 1))]),
                requires_python: artifact.requires_python.clone(),
                size: Some(fs::metadata(&artifact_path)?.len()),
                core_metadata: artifact.core_metadata.then_some(true),
                yanked: artifact.yanked.then_some(true),
            });
        }

        let project_path = self.index_dir.join(format!("{}.json", package.name));
        let mut project = if project_path.exists() {
            serde_json::from_slice::<SimpleFixtureProject>(&fs::read(&project_path)?)
                .map_err(std::io::Error::other)?
        } else {
            SimpleFixtureProject { files: Vec::new() }
        };
        project.files.extend(files);
        project
            .files
            .sort_by(|left, right| left.filename.cmp(&right.filename));
        fs::write(
            project_path,
            serde_json::to_vec_pretty(&project).map_err(std::io::Error::other)?,
        )?;
        Ok(())
    }

    pub(crate) async fn resolve(
        &self,
        requirements: &[&str],
    ) -> Result<Vec<ResolvedPackage>, ResolverError> {
        self.resolve_roots(vec![FixtureRoot::new(
            ResolutionRootTokenKind::DependencyGroup,
            "pyra-default",
            requirements,
        )])
        .await
    }

    pub(crate) async fn resolve_roots(
        &self,
        roots: Vec<FixtureRoot>,
    ) -> Result<Vec<ResolvedPackage>, ResolverError> {
        Resolver::new()
            .resolve(ResolutionRequest {
                environment: fixture_environment(),
                roots: roots
                    .into_iter()
                    .map(FixtureRoot::into_resolution_root)
                    .collect(),
                index_url: self.index_url(),
            })
            .await
    }

    fn index_url(&self) -> String {
        Url::from_directory_path(&self.index_dir)
            .expect("fixture index dir")
            .to_string()
            .trim_end_matches('/')
            .to_string()
    }
}

pub(crate) struct FixtureRoot {
    kind: ResolutionRootTokenKind,
    name: String,
    requirements: Vec<String>,
}

impl FixtureRoot {
    pub(crate) fn new(
        kind: ResolutionRootTokenKind,
        name: impl Into<String>,
        requirements: &[&str],
    ) -> Self {
        Self {
            kind,
            name: name.into(),
            requirements: requirements.iter().map(ToString::to_string).collect(),
        }
    }

    fn into_resolution_root(self) -> ResolutionRoot {
        ResolutionRoot {
            token: ResolutionRootToken {
                kind: self.kind,
                name: self.name,
            },
            requirements: self.requirements,
        }
    }
}

pub(crate) struct PackageFixture {
    name: String,
    artifacts: Vec<ArtifactFixture>,
}

impl PackageFixture {
    pub(crate) fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            artifacts: Vec::new(),
        }
    }

    pub(crate) fn with_artifact(mut self, artifact: ArtifactFixture) -> Self {
        self.artifacts.push(artifact);
        self
    }
}

pub(crate) enum ArtifactFixtureKind {
    Wheel {
        python_tag: String,
        abi_tag: String,
        platform_tag: String,
    },
    Sdist {
        extension: &'static str,
    },
}

pub(crate) struct ArtifactFixture {
    version: String,
    kind: ArtifactFixtureKind,
    requires_python: Option<String>,
    dependencies: Vec<String>,
    core_metadata: bool,
    yanked: bool,
}

impl ArtifactFixture {
    pub(crate) fn wheel(version: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            kind: ArtifactFixtureKind::Wheel {
                python_tag: "py3".to_string(),
                abi_tag: "none".to_string(),
                platform_tag: "any".to_string(),
            },
            requires_python: None,
            dependencies: Vec::new(),
            core_metadata: true,
            yanked: false,
        }
    }

    pub(crate) fn wheel_with_tags(
        version: impl Into<String>,
        python_tag: impl Into<String>,
        abi_tag: impl Into<String>,
        platform_tag: impl Into<String>,
    ) -> Self {
        Self {
            version: version.into(),
            kind: ArtifactFixtureKind::Wheel {
                python_tag: python_tag.into(),
                abi_tag: abi_tag.into(),
                platform_tag: platform_tag.into(),
            },
            requires_python: None,
            dependencies: Vec::new(),
            core_metadata: true,
            yanked: false,
        }
    }

    pub(crate) fn sdist(version: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            kind: ArtifactFixtureKind::Sdist {
                extension: "tar.gz",
            },
            requires_python: None,
            dependencies: Vec::new(),
            core_metadata: true,
            yanked: false,
        }
    }

    pub(crate) fn with_dependency(mut self, dependency: impl Into<String>) -> Self {
        self.dependencies.push(dependency.into());
        self
    }

    pub(crate) fn with_requires_python(mut self, requires_python: impl Into<String>) -> Self {
        self.requires_python = Some(requires_python.into());
        self
    }

    pub(crate) fn without_core_metadata(mut self) -> Self {
        self.core_metadata = false;
        self
    }

    pub(crate) fn yanked(mut self) -> Self {
        self.yanked = true;
        self
    }

    fn filename(&self, package: &str) -> String {
        match &self.kind {
            ArtifactFixtureKind::Wheel {
                python_tag,
                abi_tag,
                platform_tag,
            } => format!(
                "{package}-{}-{python_tag}-{abi_tag}-{platform_tag}.whl",
                self.version
            ),
            ArtifactFixtureKind::Sdist { extension } => {
                format!("{package}-{}.{}", self.version, extension)
            }
        }
    }

    fn metadata_contents(&self) -> String {
        let mut lines = Vec::new();
        if let Some(requires_python) = &self.requires_python {
            lines.push(format!("Requires-Python: {requires_python}"));
        }
        for dependency in &self.dependencies {
            lines.push(format!("Requires-Dist: {dependency}"));
        }
        if lines.is_empty() {
            String::new()
        } else {
            format!("{}\n", lines.join("\n"))
        }
    }
}

#[derive(Deserialize, Serialize)]
struct SimpleFixtureProject {
    files: Vec<SimpleFixtureFile>,
}

#[derive(Deserialize, Serialize)]
struct SimpleFixtureFile {
    filename: String,
    url: String,
    hashes: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    requires_python: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<u64>,
    #[serde(rename = "core-metadata", skip_serializing_if = "Option::is_none")]
    core_metadata: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    yanked: Option<bool>,
}

fn fixture_environment() -> ResolverEnvironment {
    let python_full_version = "3.13.2";
    let markers = MarkerEnvironmentBuilder {
        implementation_name: "cpython",
        implementation_version: python_full_version,
        os_name: "posix",
        platform_machine: "x86_64",
        platform_python_implementation: "CPython",
        platform_release: "",
        platform_system: "Linux",
        platform_version: "",
        python_full_version,
        python_version: "3.13",
        sys_platform: "linux",
    }
    .try_into()
    .expect("fixture marker environment");

    ResolverEnvironment::new(markers, python_full_version, "x86_64-unknown-linux-gnu")
        .expect("fixture resolver environment")
}
