//! `pyproject.toml` loading for `pyra sync`.
//!
//! This module turns project metadata into a typed, normalized model that the
//! resolver and installer can consume without depending on `toml_edit`.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::str::FromStr;

use camino::Utf8PathBuf;
use pep440_rs::{Version, VersionSpecifiers};
use pep508_rs::Requirement;
use pyra_python::{PythonVersion, PythonVersionRequest};
use toml_edit::{Array, DocumentMut, InlineTable, Item, Table, Value};

use crate::{
    ProjectError,
    identity::{ProjectIdentity, find_project_root},
    pyproject::read_python_selector,
    sync::selection::{SYNTHETIC_DEFAULT_GROUP, normalize_name},
};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ProjectSyncRequirement {
    pub requirement: Requirement,
    pub source: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DependencyGroupRequirement {
    pub requirement: Requirement,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DependencyGroupName {
    pub display_name: String,
    pub normalized_name: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DependencyGroupDefinition {
    pub name: DependencyGroupName,
    pub requirements: Vec<DependencyGroupRequirement>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ProjectSyncInput {
    pub project_root: Utf8PathBuf,
    pub pyproject_path: Utf8PathBuf,
    pub pylock_path: Utf8PathBuf,
    pub project_name: String,
    pub pinned_python: PythonVersionRequest,
    pub requires_python: Option<String>,
    pub build_system_present: bool,
    pub dependencies: Vec<ProjectSyncRequirement>,
    pub optional_dependencies: Vec<DependencyGroupDefinition>,
    pub dependency_groups: Vec<DependencyGroupDefinition>,
}

impl ProjectSyncInput {
    pub fn project_identity(&self) -> Result<ProjectIdentity, ProjectError> {
        ProjectIdentity::from_root(&self.project_root)
    }

    pub fn has_dev_group(&self) -> bool {
        self.dependency_groups
            .iter()
            .any(|group| group.name.normalized_name == "dev")
    }

    /// Enforces the project's declared interpreter support before sync tries to
    /// reuse a lock or resolve a new one.
    pub fn validate_selected_interpreter(
        &self,
        interpreter: &PythonVersion,
    ) -> Result<(), ProjectError> {
        let Some(requires_python) = &self.requires_python else {
            return Ok(());
        };

        let specifiers = VersionSpecifiers::from_str(requires_python).map_err(|error| {
            ProjectError::InvalidRequiresPython {
                path: self.pyproject_path.to_string(),
                value: requires_python.clone(),
                detail: error.to_string(),
            }
        })?;
        let interpreter = Version::from_str(&interpreter.to_string()).map_err(|error| {
            ProjectError::InvalidManagedPythonVersion {
                value: interpreter.to_string(),
                detail: error.to_string(),
            }
        })?;

        if specifiers.contains(&interpreter) {
            Ok(())
        } else {
            Err(ProjectError::PinnedPythonIncompatibleWithProject {
                interpreter: interpreter.to_string(),
                requires_python: requires_python.clone(),
            })
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ProjectSyncInputLoader;

impl ProjectSyncInputLoader {
    pub fn load(self, context: &pyra_core::AppContext) -> Result<ProjectSyncInput, ProjectError> {
        let project_root = find_project_root(&context.cwd)?;
        let pyproject_path = project_root.join("pyproject.toml");
        let pylock_path = project_root.join("pylock.toml");
        let document = load_document(&pyproject_path)?;
        let pinned_python = read_python_selector(&pyproject_path)?
            .ok_or(ProjectError::PinnedPythonNotConfigured)?;
        let project = document
            .as_table()
            .get("project")
            .and_then(Item::as_table)
            .ok_or(ProjectError::MissingProjectMetadata)?;
        let project_name = project
            .get("name")
            .and_then(Item::as_str)
            .ok_or(ProjectError::MissingProjectName)?
            .to_string();

        let dependencies = parse_requirements(
            project.get("dependencies").and_then(Item::as_array),
            SYNTHETIC_DEFAULT_GROUP,
        )?
        .into_iter()
        .map(|requirement| ProjectSyncRequirement {
            requirement,
            source: SYNTHETIC_DEFAULT_GROUP.to_string(),
        })
        .collect::<Vec<_>>();

        let optional_dependencies = parse_requirement_table(
            project
                .get("optional-dependencies")
                .and_then(Item::as_table),
            "optional dependency",
        )?;
        let dependency_groups = parse_dependency_groups(
            document
                .as_table()
                .get("dependency-groups")
                .and_then(Item::as_table),
        )?;

        Ok(ProjectSyncInput {
            project_root,
            pyproject_path,
            pylock_path,
            project_name,
            pinned_python,
            requires_python: project
                .get("requires-python")
                .and_then(Item::as_str)
                .map(ToString::to_string),
            build_system_present: document
                .as_table()
                .get("build-system")
                .is_some_and(|item| item.is_table()),
            dependencies,
            optional_dependencies,
            dependency_groups,
        })
    }
}

fn load_document(pyproject_path: &camino::Utf8Path) -> Result<DocumentMut, ProjectError> {
    let contents =
        fs::read_to_string(pyproject_path).map_err(|source| ProjectError::ReadPyproject {
            path: pyproject_path.to_string(),
            source,
        })?;

    contents
        .parse::<DocumentMut>()
        .map_err(|source| ProjectError::ParsePyproject {
            path: pyproject_path.to_string(),
            source,
        })
}

fn parse_requirement_table(
    table: Option<&Table>,
    label: &'static str,
) -> Result<Vec<DependencyGroupDefinition>, ProjectError> {
    let Some(table) = table else {
        return Ok(Vec::new());
    };

    let mut seen = BTreeMap::new();
    let mut groups = Vec::new();
    for (name, item) in table.iter() {
        let normalized = normalize_name(name);
        if let Some(previous) = seen.insert(normalized.clone(), name.to_string()) {
            return Err(ProjectError::DuplicateNormalizedDependencyGroup {
                first: previous,
                second: name.to_string(),
            });
        }

        let requirements = parse_requirements(item.as_array(), label)?
            .into_iter()
            .map(|requirement| DependencyGroupRequirement { requirement })
            .collect();
        groups.push(DependencyGroupDefinition {
            name: DependencyGroupName {
                display_name: name.to_string(),
                normalized_name: normalized,
            },
            requirements,
        });
    }

    groups.sort_by(|left, right| left.name.normalized_name.cmp(&right.name.normalized_name));
    Ok(groups)
}

fn parse_dependency_groups(
    table: Option<&Table>,
) -> Result<Vec<DependencyGroupDefinition>, ProjectError> {
    let Some(table) = table else {
        return Ok(Vec::new());
    };

    let mut raw_groups = HashMap::new();
    let mut seen = BTreeMap::new();
    for (name, item) in table.iter() {
        let normalized = normalize_name(name);
        if let Some(previous) = seen.insert(normalized.clone(), name.to_string()) {
            return Err(ProjectError::DuplicateNormalizedDependencyGroup {
                first: previous,
                second: name.to_string(),
            });
        }

        let array =
            item.as_array()
                .ok_or_else(|| ProjectError::InvalidDependencyGroupDefinition {
                    name: name.to_string(),
                })?;
        raw_groups.insert(
            normalized,
            RawDependencyGroup {
                display_name: name.to_string(),
                entries: parse_dependency_group_entries(name, array)?,
            },
        );
    }

    let mut expanded = Vec::new();
    for key in raw_groups.keys().cloned().collect::<Vec<_>>() {
        let mut visiting = Vec::new();
        let requirements = expand_dependency_group(&key, &raw_groups, &mut visiting)?;
        let group = raw_groups.get(&key).expect("raw group exists");
        expanded.push(DependencyGroupDefinition {
            name: DependencyGroupName {
                display_name: group.display_name.clone(),
                normalized_name: key,
            },
            requirements: requirements
                .into_iter()
                .map(|requirement| DependencyGroupRequirement { requirement })
                .collect(),
        });
    }
    expanded.sort_by(|left, right| left.name.normalized_name.cmp(&right.name.normalized_name));
    Ok(expanded)
}

fn parse_requirements(
    array: Option<&Array>,
    label: &'static str,
) -> Result<Vec<Requirement>, ProjectError> {
    let Some(array) = array else {
        return Ok(Vec::new());
    };

    let mut requirements = Vec::new();
    for item in array {
        let requirement_text = item.as_str().ok_or(ProjectError::InvalidRequirementValue {
            context: label.to_string(),
        })?;
        let requirement = Requirement::from_str(requirement_text).map_err(|source| {
            ProjectError::InvalidRequirement {
                context: label.to_string(),
                value: requirement_text.to_string(),
                detail: source.to_string(),
            }
        })?;
        requirements.push(requirement);
    }
    Ok(requirements)
}

#[derive(Debug, Clone)]
struct RawDependencyGroup {
    display_name: String,
    entries: Vec<RawDependencyGroupEntry>,
}

#[derive(Debug, Clone)]
enum RawDependencyGroupEntry {
    Requirement(Requirement),
    Include { normalized_name: String },
}

fn parse_dependency_group_entries(
    group_name: &str,
    array: &Array,
) -> Result<Vec<RawDependencyGroupEntry>, ProjectError> {
    let mut entries = Vec::new();
    for item in array {
        if let Some(requirement_text) = item.as_str() {
            let requirement = Requirement::from_str(requirement_text).map_err(|source| {
                ProjectError::InvalidRequirement {
                    context: format!("dependency group `{group_name}`"),
                    value: requirement_text.to_string(),
                    detail: source.to_string(),
                }
            })?;
            entries.push(RawDependencyGroupEntry::Requirement(requirement));
            continue;
        }

        let Some(table) = item.as_inline_table() else {
            return Err(ProjectError::InvalidDependencyGroupEntry {
                group: group_name.to_string(),
            });
        };
        entries.push(parse_dependency_group_include(group_name, table)?);
    }
    Ok(entries)
}

fn parse_dependency_group_include(
    group_name: &str,
    table: &InlineTable,
) -> Result<RawDependencyGroupEntry, ProjectError> {
    if table.len() != 1 {
        return Err(ProjectError::InvalidDependencyGroupEntry {
            group: group_name.to_string(),
        });
    }

    let include_name = table
        .get("include-group")
        .and_then(Value::as_str)
        .ok_or_else(|| ProjectError::InvalidDependencyGroupEntry {
            group: group_name.to_string(),
        })?;

    Ok(RawDependencyGroupEntry::Include {
        normalized_name: normalize_name(include_name),
    })
}

fn expand_dependency_group(
    normalized_name: &str,
    groups: &HashMap<String, RawDependencyGroup>,
    visiting: &mut Vec<String>,
) -> Result<Vec<Requirement>, ProjectError> {
    if visiting.iter().any(|name| name == normalized_name) {
        let cycle = visiting
            .iter()
            .cloned()
            .chain(std::iter::once(normalized_name.to_string()))
            .collect::<Vec<_>>();
        return Err(ProjectError::DependencyGroupCycle {
            cycle: cycle.join(" -> "),
        });
    }

    let group = groups.get(normalized_name).ok_or_else(|| {
        ProjectError::UnknownIncludedDependencyGroup {
            name: normalized_name.to_string(),
        }
    })?;
    visiting.push(normalized_name.to_string());

    let mut requirements = Vec::new();
    for entry in &group.entries {
        match entry {
            RawDependencyGroupEntry::Requirement(requirement) => {
                requirements.push(requirement.clone())
            }
            RawDependencyGroupEntry::Include { normalized_name } => {
                requirements.extend(expand_dependency_group(normalized_name, groups, visiting)?)
            }
        }
    }

    visiting.pop();
    Ok(requirements)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use camino::Utf8PathBuf;
    use pyra_core::{AppContext, AppPaths, Verbosity};

    use super::ProjectSyncInputLoader;

    #[test]
    fn expands_dependency_group_includes_without_deduplicating() {
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).expect("utf-8 path");
        let paths = AppPaths::from_roots(
            root.join("config"),
            root.join("data"),
            root.join("cache"),
            root.join("state"),
        );
        let context = AppContext::new(root.clone(), paths, Verbosity::Normal);
        write_pyproject(
            &root.join("pyproject.toml"),
            r#"[project]
name = "example"
version = "0.1.0"
requires-python = "==3.13.*"
dependencies = ["rich>=13"]

[project.optional-dependencies]
feature = ["httpx>=0.27"]

[dependency-groups]
base = ["pytest>=8"]
dev = [{include-group = "base"}, "ruff>=0.5", "pytest>=8"]

[tool.pyra]
python = "3.13"
"#,
        );

        let input = ProjectSyncInputLoader.load(&context).expect("input");
        let dev = input
            .dependency_groups
            .iter()
            .find(|group| group.name.normalized_name == "dev")
            .expect("dev group");

        assert_eq!(dev.requirements.len(), 3);
    }

    #[test]
    fn rejects_duplicate_normalized_group_names() {
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).expect("utf-8 path");
        let paths = AppPaths::from_roots(
            root.join("config"),
            root.join("data"),
            root.join("cache"),
            root.join("state"),
        );
        let context = AppContext::new(root.clone(), paths, Verbosity::Normal);
        write_pyproject(
            &root.join("pyproject.toml"),
            r#"[project]
name = "example"
version = "0.1.0"
dependencies = []

[dependency-groups]
Docs = ["sphinx"]
docs = ["mkdocs"]

[tool.pyra]
python = "3.13"
"#,
        );

        let error = ProjectSyncInputLoader
            .load(&context)
            .expect_err("duplicate groups fail");
        assert!(matches!(
            error,
            crate::ProjectError::DuplicateNormalizedDependencyGroup { .. }
        ));
    }

    #[test]
    fn rejects_dependency_group_cycles() {
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).expect("utf-8 path");
        let paths = AppPaths::from_roots(
            root.join("config"),
            root.join("data"),
            root.join("cache"),
            root.join("state"),
        );
        let context = AppContext::new(root.clone(), paths, Verbosity::Normal);
        write_pyproject(
            &root.join("pyproject.toml"),
            r#"[project]
name = "example"
version = "0.1.0"
dependencies = []

[dependency-groups]
a = [{include-group = "b"}]
b = [{include-group = "a"}]

[tool.pyra]
python = "3.13"
"#,
        );

        let error = ProjectSyncInputLoader
            .load(&context)
            .expect_err("cycle fails");
        assert!(matches!(
            error,
            crate::ProjectError::DependencyGroupCycle { .. }
        ));
    }

    fn write_pyproject(path: &camino::Utf8Path, contents: &str) {
        let mut file = std::fs::File::create(path).expect("pyproject");
        file.write_all(contents.as_bytes()).expect("pyproject");
    }
}
