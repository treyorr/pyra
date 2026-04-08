//! `pylock.toml` models and persistence helpers.
//!
//! The writer keeps output ordering deterministic to reduce lockfile noise.

use std::fmt::Write as _;
use std::fs;

use camino::Utf8PathBuf;
use toml_edit::{DocumentMut, Item, Table, Value};

use crate::ProjectError;

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
    pub marker: Option<String>,
    pub requires_python: Option<String>,
    pub index: Option<String>,
    pub dependencies: Vec<LockDependencyRef>,
    pub sdist: Option<LockArtifact>,
    pub wheels: Vec<LockArtifact>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LockToolPyraMetadata {
    pub input_fingerprint: String,
    pub interpreter_version: String,
    pub target_triple: String,
    pub index_url: String,
    pub resolution_strategy: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LockFile {
    pub path: Utf8PathBuf,
    pub requires_python: Option<String>,
    pub environments: Vec<String>,
    pub extras: Vec<String>,
    pub dependency_groups: Vec<String>,
    pub default_groups: Vec<String>,
    pub packages: Vec<LockPackage>,
    pub tool_pyra: LockToolPyraMetadata,
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

        let environments =
            string_array(document.as_table().get("environments")).map_err(|detail| {
                ProjectError::ParseLockfile {
                    path: path.to_string(),
                    detail,
                }
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
            tool_pyra: parse_tool_pyra(&document).map_err(|detail| {
                ProjectError::ParseLockfile {
                    path: path.to_string(),
                    detail,
                }
            })?,
        })
    }

    pub fn write(&self) -> Result<(), ProjectError> {
        let mut output = String::new();
        writeln!(output, "lock-version = \"1.0\"").expect("string write");
        write_string_array(&mut output, "environments", &self.environments);
        if let Some(requires_python) = &self.requires_python {
            writeln!(output, "requires-python = {:?}", requires_python).expect("string write");
        }
        write_string_array(&mut output, "extras", &self.extras);
        write_string_array(&mut output, "dependency-groups", &self.dependency_groups);
        write_string_array(&mut output, "default-groups", &self.default_groups);
        writeln!(output, "created-by = \"pyra\"").expect("string write");
        writeln!(output).expect("string write");

        for package in &self.packages {
            writeln!(output, "[[packages]]").expect("string write");
            writeln!(output, "name = {:?}", package.name).expect("string write");
            writeln!(output, "version = {:?}", package.version).expect("string write");
            if let Some(marker) = &package.marker {
                writeln!(output, "marker = {:?}", marker).expect("string write");
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
            self.tool_pyra.input_fingerprint
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

    pub fn is_fresh(
        &self,
        fingerprint: &str,
        interpreter_version: &str,
        target_triple: &str,
        index_url: &str,
    ) -> bool {
        self.tool_pyra.input_fingerprint == fingerprint
            && self.tool_pyra.interpreter_version == interpreter_version
            && self.tool_pyra.target_triple == target_triple
            && self.tool_pyra.index_url == index_url
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
        marker: string_value(table.get("marker")),
        requires_python: string_value(table.get("requires-python")),
        index: string_value(table.get("index")),
        dependencies,
        sdist,
        wheels,
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

fn parse_tool_pyra(document: &DocumentMut) -> Result<LockToolPyraMetadata, String> {
    let table = document
        .as_table()
        .get("tool")
        .and_then(Item::as_table)
        .and_then(|table| table.get("pyra"))
        .and_then(Item::as_table)
        .ok_or_else(|| "missing [tool.pyra]".to_string())?;

    Ok(LockToolPyraMetadata {
        input_fingerprint: table
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
    use camino::Utf8PathBuf;

    use super::{LockArtifact, LockDependencyRef, LockFile, LockPackage, LockToolPyraMetadata};

    #[test]
    fn writes_and_reads_lockfile_round_trip() {
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let path =
            Utf8PathBuf::from_path_buf(temp_dir.path().join("pylock.toml")).expect("utf-8 path");
        let lock = LockFile {
            path: path.clone(),
            requires_python: Some("==3.13.*".to_string()),
            environments: vec!["sys_platform == 'darwin'".to_string()],
            extras: vec!["feature".to_string()],
            dependency_groups: vec!["dev".to_string()],
            default_groups: vec!["pyra-default".to_string(), "dev".to_string()],
            packages: vec![LockPackage {
                name: "attrs".to_string(),
                version: "25.1.0".to_string(),
                marker: Some("'pyra-default' in dependency_groups".to_string()),
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
            tool_pyra: LockToolPyraMetadata {
                input_fingerprint: "fingerprint".to_string(),
                interpreter_version: "3.13.12".to_string(),
                target_triple: "aarch64-apple-darwin".to_string(),
                index_url: "https://pypi.org/simple".to_string(),
                resolution_strategy: "current-platform-union-v1".to_string(),
            },
        };

        lock.write().expect("write lock");
        let reread = LockFile::read(&path).expect("read lock");
        assert_eq!(reread, lock);
        assert!(reread.is_fresh(
            "fingerprint",
            "3.13.12",
            "aarch64-apple-darwin",
            "https://pypi.org/simple"
        ));
    }
}
