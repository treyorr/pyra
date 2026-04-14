//! `pylock.toml` models and persistence helpers.
//!
//! The writer keeps output ordering deterministic to reduce lockfile noise.

use std::fmt::Write as _;
use std::fs;

use camino::Utf8PathBuf;
use toml_edit::{DocumentMut, Item, Table, Value};

use crate::ProjectError;

use super::LockMarker;

/// The current lock semantics use explicit environment ids so one lock can
/// grow toward multiple environment slices without changing package metadata
/// or selection rules again first.
pub const CURRENT_RESOLUTION_STRATEGY: &str = "environment-scoped-union-v1";
/// Multi-target lock generation resolves each target independently and merges
/// only the compatible shared package graph into one lock.
pub const MULTI_TARGET_RESOLUTION_STRATEGY: &str = "environment-scoped-matrix-v1";

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LockArtifact {
    pub name: String,
    pub url: String,
    pub size: Option<u64>,
    pub upload_time: Option<String>,
    pub sha256: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LockDependencyRef {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LockPackage {
    pub name: String,
    pub version: String,
    pub marker: Option<LockMarker>,
    pub requires_python: Option<String>,
    pub index: Option<String>,
    pub dependencies: Vec<LockDependencyRef>,
    pub sdist: Option<LockArtifact>,
    pub wheels: Vec<LockArtifact>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LockFreshness {
    pub dependency_fingerprint: String,
    pub interpreter_version: String,
    pub target_triple: String,
    pub index_url: String,
    pub resolution_strategy: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LockEnvironment {
    /// Stable identifier for one target environment slice in the lock.
    pub id: String,
    /// Marker describing when this environment slice applies.
    pub marker: String,
    /// Interpreter version used when this environment slice was resolved.
    pub interpreter_version: String,
    /// Target triple used when this environment slice was resolved.
    pub target_triple: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LockFile {
    pub path: Utf8PathBuf,
    pub requires_python: Option<String>,
    pub environments: Vec<LockEnvironment>,
    pub extras: Vec<String>,
    pub dependency_groups: Vec<String>,
    pub default_groups: Vec<String>,
    pub packages: Vec<LockPackage>,
    pub tool_pyra: LockFreshness,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LockSelection {
    pub groups: std::collections::BTreeSet<String>,
    pub extras: std::collections::BTreeSet<String>,
}

impl LockFile {
    pub fn read(path: &camino::Utf8Path) -> Result<Self, ProjectError> {
        let contents = fs::read_to_string(path).map_err(|source| ProjectError::ReadLockfile {
            path: path.to_string(),
            source,
        })?;
        let document =
            contents
                .parse::<DocumentMut>()
                .map_err(|error| ProjectError::ParseLockfile {
                    path: path.to_string(),
                    detail: error.to_string(),
                })?;

        let packages = document
            .as_table()
            .get("packages")
            .and_then(Item::as_array_of_tables)
            .ok_or_else(|| ProjectError::ParseLockfile {
                path: path.to_string(),
                detail: "missing [[packages]]".to_string(),
            })?;

        let tool_pyra =
            parse_tool_pyra(&document).map_err(|detail| ProjectError::ParseLockfile {
                path: path.to_string(),
                detail,
            })?;
        let environments = parse_environments(document.as_table().get("environments"), &tool_pyra)
            .map_err(|detail| ProjectError::ParseLockfile {
                path: path.to_string(),
                detail,
            })?;
        let extras = string_array(document.as_table().get("extras")).map_err(|detail| {
            ProjectError::ParseLockfile {
                path: path.to_string(),
                detail,
            }
        })?;
        let dependency_groups = string_array(document.as_table().get("dependency-groups"))
            .map_err(|detail| ProjectError::ParseLockfile {
                path: path.to_string(),
                detail,
            })?;
        let default_groups =
            string_array(document.as_table().get("default-groups")).map_err(|detail| {
                ProjectError::ParseLockfile {
                    path: path.to_string(),
                    detail,
                }
            })?;

        Ok(Self {
            path: path.to_path_buf(),
            requires_python: string_value(document.as_table().get("requires-python")),
            environments,
            extras,
            dependency_groups,
            default_groups,
            packages: packages
                .iter()
                .map(parse_package)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|detail| ProjectError::ParseLockfile {
                    path: path.to_string(),
                    detail,
                })?,
            tool_pyra,
        })
    }

    pub fn write(&self) -> Result<(), ProjectError> {
        let mut output = String::new();
        writeln!(output, "lock-version = \"1.0\"").expect("string write");
        if let Some(requires_python) = &self.requires_python {
            writeln!(output, "requires-python = {:?}", requires_python).expect("string write");
        }
        write_string_array(&mut output, "extras", &self.extras);
        write_string_array(&mut output, "dependency-groups", &self.dependency_groups);
        write_string_array(&mut output, "default-groups", &self.default_groups);
        writeln!(output, "created-by = \"pyra\"").expect("string write");
        writeln!(output).expect("string write");
        write_environments(&mut output, &self.environments);
        if !self.environments.is_empty() {
            writeln!(output).expect("string write");
        }

        for package in &self.packages {
            writeln!(output, "[[packages]]").expect("string write");
            writeln!(output, "name = {:?}", package.name).expect("string write");
            writeln!(output, "version = {:?}", package.version).expect("string write");
            if let Some(marker) = &package.marker {
                writeln!(output, "marker = {:?}", marker.to_string()).expect("string write");
            }
            if let Some(requires_python) = &package.requires_python {
                writeln!(output, "requires-python = {:?}", requires_python).expect("string write");
            }
            if let Some(index) = &package.index {
                writeln!(output, "index = {:?}", index).expect("string write");
            }
            for dependency in &package.dependencies {
                writeln!(output, "[[packages.dependencies]]").expect("string write");
                writeln!(output, "name = {:?}", dependency.name).expect("string write");
                writeln!(output, "version = {:?}", dependency.version).expect("string write");
            }
            if let Some(sdist) = &package.sdist {
                write_artifact(&mut output, "packages.sdist", sdist);
            }
            for wheel in &package.wheels {
                writeln!(output, "[[packages.wheels]]").expect("string write");
                write_artifact_body(&mut output, wheel);
            }
            writeln!(output).expect("string write");
        }

        writeln!(output, "[tool.pyra]").expect("string write");
        writeln!(
            output,
            "input-fingerprint = {:?}",
            self.tool_pyra.dependency_fingerprint
        )
        .expect("string write");
        writeln!(
            output,
            "interpreter-version = {:?}",
            self.tool_pyra.interpreter_version
        )
        .expect("string write");
        writeln!(output, "target-triple = {:?}", self.tool_pyra.target_triple)
            .expect("string write");
        writeln!(output, "index-url = {:?}", self.tool_pyra.index_url).expect("string write");
        writeln!(
            output,
            "resolution-strategy = {:?}",
            self.tool_pyra.resolution_strategy
        )
        .expect("string write");

        fs::write(&self.path, output).map_err(|source| ProjectError::WriteLockfile {
            path: self.path.to_string(),
            source,
        })
    }

    /// Lock reuse is valid only when the full freshness model matches exactly.
    /// Keeping this comparison typed prevents the service layer from drifting
    /// into incomplete ad hoc checks.
    pub fn is_fresh(&self, freshness: &LockFreshness) -> bool {
        self.tool_pyra == *freshness
    }
}

fn write_string_array(output: &mut String, key: &str, values: &[String]) {
    let rendered = values
        .iter()
        .map(|value| format!("{value:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    writeln!(output, "{key} = [{rendered}]").expect("string write");
}

fn write_environments(output: &mut String, environments: &[LockEnvironment]) {
    for environment in environments {
        writeln!(output, "[[environments]]").expect("string write");
        writeln!(output, "id = {:?}", environment.id).expect("string write");
        writeln!(output, "marker = {:?}", environment.marker).expect("string write");
        writeln!(
            output,
            "interpreter-version = {:?}",
            environment.interpreter_version
        )
        .expect("string write");
        writeln!(output, "target-triple = {:?}", environment.target_triple).expect("string write");
    }
}

fn write_artifact(output: &mut String, table_name: &str, artifact: &LockArtifact) {
    writeln!(output, "[{table_name}]").expect("string write");
    write_artifact_body(output, artifact);
}

fn write_artifact_body(output: &mut String, artifact: &LockArtifact) {
    writeln!(output, "name = {:?}", artifact.name).expect("string write");
    writeln!(output, "url = {:?}", artifact.url).expect("string write");
    if let Some(size) = artifact.size {
        writeln!(output, "size = {size}").expect("string write");
    }
    writeln!(output, "hashes = {{ sha256 = {:?} }}", artifact.sha256).expect("string write");
}

fn string_array(item: Option<&Item>) -> Result<Vec<String>, String> {
    let Some(item) = item else {
        return Ok(Vec::new());
    };
    let Some(array) = item.as_array() else {
        return Err("expected string array".to_string());
    };
    array
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(ToString::to_string)
                .ok_or_else(|| "expected string array entry".to_string())
        })
        .collect()
}

fn string_value(item: Option<&Item>) -> Option<String> {
    item.and_then(Item::as_str).map(ToString::to_string)
}

fn parse_environments(
    item: Option<&Item>,
    tool_pyra: &LockFreshness,
) -> Result<Vec<LockEnvironment>, String> {
    let Some(item) = item else {
        return Ok(Vec::new());
    };
    if let Some(array) = item.as_array_of_tables() {
        return array
            .iter()
            .map(|table| parse_environment(table, tool_pyra))
            .collect();
    }
    if let Some(array) = item.as_array() {
        // Legacy locks stored environments as a bare string array. Keep
        // parsing them so freshness can reject the old strategy cleanly.
        return array
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let marker = value
                    .as_str()
                    .ok_or_else(|| "expected string array entry".to_string())?;
                Ok(LockEnvironment {
                    id: format!("legacy-env-{index}"),
                    marker: marker.to_string(),
                    interpreter_version: tool_pyra.interpreter_version.clone(),
                    target_triple: tool_pyra.target_triple.clone(),
                })
            })
            .collect();
    }
    Err("expected environments array or array-of-tables".to_string())
}

fn parse_package(table: &Table) -> Result<LockPackage, String> {
    let dependencies = table
        .get("dependencies")
        .and_then(Item::as_array_of_tables)
        .map(|entries| {
            entries
                .iter()
                .map(|entry| {
                    Ok(LockDependencyRef {
                        name: entry
                            .get("name")
                            .and_then(Item::as_str)
                            .ok_or_else(|| "dependency missing name".to_string())?
                            .to_string(),
                        version: entry
                            .get("version")
                            .and_then(Item::as_str)
                            .ok_or_else(|| "dependency missing version".to_string())?
                            .to_string(),
                    })
                })
                .collect::<Result<Vec<_>, String>>()
        })
        .transpose()?
        .unwrap_or_default();

    let sdist = table
        .get("sdist")
        .and_then(Item::as_table)
        .map(parse_artifact)
        .transpose()?;
    let wheels = table
        .get("wheels")
        .and_then(Item::as_array_of_tables)
        .map(|entries| {
            entries
                .iter()
                .map(parse_artifact)
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();

    Ok(LockPackage {
        name: table
            .get("name")
            .and_then(Item::as_str)
            .ok_or_else(|| "package missing name".to_string())?
            .to_string(),
        version: table
            .get("version")
            .and_then(Item::as_str)
            .ok_or_else(|| "package missing version".to_string())?
            .to_string(),
        marker: parse_marker(table.get("marker"))?,
        requires_python: string_value(table.get("requires-python")),
        index: string_value(table.get("index")),
        dependencies,
        sdist,
        wheels,
    })
}

fn parse_environment(table: &Table, tool_pyra: &LockFreshness) -> Result<LockEnvironment, String> {
    Ok(LockEnvironment {
        id: table
            .get("id")
            .and_then(Item::as_str)
            .ok_or_else(|| "environment missing id".to_string())?
            .to_string(),
        marker: table
            .get("marker")
            .and_then(Item::as_str)
            .ok_or_else(|| "environment missing marker".to_string())?
            .to_string(),
        interpreter_version: table
            .get("interpreter-version")
            .and_then(Item::as_str)
            .unwrap_or(&tool_pyra.interpreter_version)
            .to_string(),
        target_triple: table
            .get("target-triple")
            .and_then(Item::as_str)
            .unwrap_or(&tool_pyra.target_triple)
            .to_string(),
    })
}

fn parse_artifact(table: &Table) -> Result<LockArtifact, String> {
    Ok(LockArtifact {
        name: table
            .get("name")
            .and_then(Item::as_str)
            .ok_or_else(|| "artifact missing name".to_string())?
            .to_string(),
        url: table
            .get("url")
            .and_then(Item::as_str)
            .ok_or_else(|| "artifact missing url".to_string())?
            .to_string(),
        size: table
            .get("size")
            .and_then(Item::as_integer)
            .map(|value| value as u64),
        upload_time: string_value(table.get("upload-time")),
        sha256: table
            .get("hashes")
            .and_then(Item::as_inline_table)
            .and_then(|table| table.get("sha256"))
            .and_then(Value::as_str)
            .ok_or_else(|| "artifact missing sha256".to_string())?
            .to_string(),
    })
}

fn parse_marker(item: Option<&Item>) -> Result<Option<LockMarker>, String> {
    let Some(marker) = string_value(item) else {
        return Ok(None);
    };
    LockMarker::parse(&marker)
        .map(Some)
        .map_err(|detail| format!("invalid package marker `{marker}`: {detail}"))
}

fn parse_tool_pyra(document: &DocumentMut) -> Result<LockFreshness, String> {
    let table = document
        .as_table()
        .get("tool")
        .and_then(Item::as_table)
        .and_then(|table| table.get("pyra"))
        .and_then(Item::as_table)
        .ok_or_else(|| "missing [tool.pyra]".to_string())?;

    Ok(LockFreshness {
        dependency_fingerprint: table
            .get("input-fingerprint")
            .and_then(Item::as_str)
            .ok_or_else(|| "missing tool.pyra input-fingerprint".to_string())?
            .to_string(),
        interpreter_version: table
            .get("interpreter-version")
            .and_then(Item::as_str)
            .ok_or_else(|| "missing tool.pyra interpreter-version".to_string())?
            .to_string(),
        target_triple: table
            .get("target-triple")
            .and_then(Item::as_str)
            .ok_or_else(|| "missing tool.pyra target-triple".to_string())?
            .to_string(),
        index_url: table
            .get("index-url")
            .and_then(Item::as_str)
            .ok_or_else(|| "missing tool.pyra index-url".to_string())?
            .to_string(),
        resolution_strategy: table
            .get("resolution-strategy")
            .and_then(Item::as_str)
            .ok_or_else(|| "missing tool.pyra resolution-strategy".to_string())?
            .to_string(),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use camino::Utf8PathBuf;

    use super::{
        CURRENT_RESOLUTION_STRATEGY, LockArtifact, LockDependencyRef, LockEnvironment, LockFile,
        LockFreshness, LockMarker, LockPackage, MULTI_TARGET_RESOLUTION_STRATEGY,
    };
    use crate::sync::LockMarkerClause;

    #[test]
    fn writes_and_reads_lockfile_round_trip_including_resolution_strategy() {
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let path =
            Utf8PathBuf::from_path_buf(temp_dir.path().join("pylock.toml")).expect("utf-8 path");
        let lock = sample_lock(path.clone());
        let freshness = sample_freshness();

        lock.write().expect("write lock");
        let reread = LockFile::read(&path).expect("read lock");
        assert_eq!(reread, lock);
        assert_eq!(
            reread.tool_pyra.resolution_strategy,
            CURRENT_RESOLUTION_STRATEGY
        );
        assert!(reread.is_fresh(&freshness));
    }

    #[test]
    fn round_trips_multiple_environment_identifiers() {
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let path =
            Utf8PathBuf::from_path_buf(temp_dir.path().join("pylock.toml")).expect("utf-8 path");
        let mut lock = sample_lock(path.clone());
        lock.environments = vec![
            LockEnvironment {
                id: "cpython-3.13.12-aarch64-apple-darwin".to_string(),
                marker: "sys_platform == 'darwin' and platform_machine == 'arm64'".to_string(),
                interpreter_version: "3.13.12".to_string(),
                target_triple: "aarch64-apple-darwin".to_string(),
            },
            LockEnvironment {
                id: "cpython-3.13.12-x86_64-unknown-linux-gnu".to_string(),
                marker: "sys_platform == 'linux' and platform_machine == 'x86_64'".to_string(),
                interpreter_version: "3.13.12".to_string(),
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
            },
        ];

        lock.write().expect("write lock");
        let reread = LockFile::read(&path).expect("read lock");

        assert_eq!(reread.environments, lock.environments);
        assert_eq!(
            reread
                .environments
                .iter()
                .map(|environment| environment.id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "cpython-3.13.12-aarch64-apple-darwin",
                "cpython-3.13.12-x86_64-unknown-linux-gnu",
            ]
        );
    }

    #[test]
    fn round_trips_generated_markers() {
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let path =
            Utf8PathBuf::from_path_buf(temp_dir.path().join("pylock.toml")).expect("utf-8 path");
        let mut lock = sample_lock(path.clone());
        lock.packages[0].marker = LockMarker::from_clauses(vec![
            LockMarkerClause::dependency_group("dev"),
            LockMarkerClause::extra("feature"),
            LockMarkerClause::dependency_group("pyra-default"),
        ]);

        lock.write().expect("write lock");
        let reread = LockFile::read(&path).expect("read lock");

        assert_eq!(reread.packages[0].marker, lock.packages[0].marker);
        assert_eq!(
            reread.packages[0].marker.as_ref().map(ToString::to_string),
            Some(
                "'dev' in dependency_groups or 'pyra-default' in dependency_groups or 'feature' in extras"
                    .to_string()
            )
        );
    }

    #[test]
    fn rejects_malformed_package_markers() {
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let path =
            Utf8PathBuf::from_path_buf(temp_dir.path().join("pylock.toml")).expect("utf-8 path");
        fs::write(
            path.as_std_path(),
            r#"
lock-version = "1.0"
environments = []
extras = []
dependency-groups = []
default-groups = []
created-by = "pyra"

[[packages]]
name = "attrs"
version = "25.1.0"
marker = "'dev' in dependency_groups or"

[tool.pyra]
input-fingerprint = "fingerprint"
interpreter-version = "3.13.12"
target-triple = "aarch64-apple-darwin"
index-url = "https://pypi.org/simple"
resolution-strategy = "environment-scoped-union-v1"
"#,
        )
        .expect("write invalid lock");

        let error = LockFile::read(&path).expect_err("invalid lock");
        match error {
            crate::ProjectError::ParseLockfile { detail, .. } => {
                assert!(detail.contains("invalid package marker"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn rejects_malformed_environment_schema() {
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let path =
            Utf8PathBuf::from_path_buf(temp_dir.path().join("pylock.toml")).expect("utf-8 path");
        fs::write(
            path.as_std_path(),
            r#"
lock-version = "1.0"

[[environments]]
marker = "sys_platform == 'darwin'"

extras = []
dependency-groups = []
default-groups = []
created-by = "pyra"

[[packages]]
name = "attrs"
version = "25.1.0"

[tool.pyra]
input-fingerprint = "fingerprint"
interpreter-version = "3.13.12"
target-triple = "aarch64-apple-darwin"
index-url = "https://pypi.org/simple"
resolution-strategy = "environment-scoped-union-v1"
"#,
        )
        .expect("write invalid lock");

        let error = LockFile::read(&path).expect_err("invalid lock");
        match error {
            crate::ProjectError::ParseLockfile { detail, .. } => {
                assert!(detail.contains("environment missing id"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn backfills_environment_metadata_from_top_level_freshness_for_older_locks() {
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let path =
            Utf8PathBuf::from_path_buf(temp_dir.path().join("pylock.toml")).expect("utf-8 path");
        fs::write(
            path.as_std_path(),
            r#"
lock-version = "1.0"

[[environments]]
id = "cpython-3.13.12-aarch64-apple-darwin"
marker = "sys_platform == 'darwin'"

extras = []
dependency-groups = []
default-groups = []
created-by = "pyra"

[[packages]]
name = "attrs"
version = "25.1.0"

[tool.pyra]
input-fingerprint = "fingerprint"
interpreter-version = "3.13.12"
target-triple = "aarch64-apple-darwin"
index-url = "https://pypi.org/simple"
resolution-strategy = "environment-scoped-union-v1"
"#,
        )
        .expect("write legacy lock");

        let lock = LockFile::read(&path).expect("read legacy lock");

        assert_eq!(lock.environments.len(), 1);
        assert_eq!(lock.environments[0].interpreter_version, "3.13.12");
        assert_eq!(lock.environments[0].target_triple, "aarch64-apple-darwin");
    }

    #[test]
    fn freshness_rejects_dependency_fingerprint_changes() {
        let mut freshness = sample_freshness();
        freshness.dependency_fingerprint = "different".to_string();

        assert!(!sample_lock(Utf8PathBuf::from("/tmp/pylock.toml")).is_fresh(&freshness));
    }

    #[test]
    fn freshness_rejects_interpreter_version_changes() {
        let mut freshness = sample_freshness();
        freshness.interpreter_version = "3.13.13".to_string();

        assert!(!sample_lock(Utf8PathBuf::from("/tmp/pylock.toml")).is_fresh(&freshness));
    }

    #[test]
    fn freshness_rejects_target_triple_changes() {
        let mut freshness = sample_freshness();
        freshness.target_triple = "x86_64-apple-darwin".to_string();

        assert!(!sample_lock(Utf8PathBuf::from("/tmp/pylock.toml")).is_fresh(&freshness));
    }

    #[test]
    fn freshness_rejects_index_url_changes() {
        let mut freshness = sample_freshness();
        freshness.index_url = "https://mirror.example/simple".to_string();

        assert!(!sample_lock(Utf8PathBuf::from("/tmp/pylock.toml")).is_fresh(&freshness));
    }

    #[test]
    fn freshness_rejects_resolution_strategy_changes() {
        let mut freshness = sample_freshness();
        freshness.resolution_strategy = "future-strategy-v2".to_string();

        assert!(!sample_lock(Utf8PathBuf::from("/tmp/pylock.toml")).is_fresh(&freshness));
    }

    fn sample_lock(path: Utf8PathBuf) -> LockFile {
        LockFile {
            path,
            requires_python: Some("==3.13.*".to_string()),
            environments: vec![LockEnvironment {
                id: "cpython-3.13.12-aarch64-apple-darwin".to_string(),
                marker: "sys_platform == 'darwin'".to_string(),
                interpreter_version: "3.13.12".to_string(),
                target_triple: "aarch64-apple-darwin".to_string(),
            }],
            extras: vec!["feature".to_string()],
            dependency_groups: vec!["dev".to_string()],
            default_groups: vec!["pyra-default".to_string(), "dev".to_string()],
            packages: vec![LockPackage {
                name: "attrs".to_string(),
                version: "25.1.0".to_string(),
                marker: LockMarker::from_clauses(vec![LockMarkerClause::dependency_group(
                    "pyra-default",
                )]),
                requires_python: None,
                index: Some("https://pypi.org/simple".to_string()),
                dependencies: vec![LockDependencyRef {
                    name: "pluggy".to_string(),
                    version: "1.5.0".to_string(),
                }],
                sdist: None,
                wheels: vec![LockArtifact {
                    name: "attrs-25.1.0-py3-none-any.whl".to_string(),
                    url: "https://example.test/attrs.whl".to_string(),
                    size: Some(123),
                    upload_time: None,
                    sha256: "abc".to_string(),
                }],
            }],
            tool_pyra: sample_freshness(),
        }
    }

    fn sample_freshness() -> LockFreshness {
        LockFreshness {
            dependency_fingerprint: "fingerprint".to_string(),
            interpreter_version: "3.13.12".to_string(),
            target_triple: "aarch64-apple-darwin".to_string(),
            index_url: "https://pypi.org/simple".to_string(),
            resolution_strategy: CURRENT_RESOLUTION_STRATEGY.to_string(),
        }
    }

    #[test]
    fn multi_target_strategy_is_distinct_from_single_target_strategy() {
        assert_ne!(
            CURRENT_RESOLUTION_STRATEGY,
            MULTI_TARGET_RESOLUTION_STRATEGY
        );
    }
}
