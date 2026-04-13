//! `pyproject.toml` read/write support for Pyra-managed settings and dependency
//! declarations.
//!
//! `toml_edit` lets Pyra update `[tool.pyra]` without discarding unrelated
//! project formatting, which will matter more once project metadata grows and
//! package-manager commands start mutating standard dependency inputs.

use std::fs;
use std::str::FromStr;

use camino::Utf8Path;
use pep440_rs::{Version, VersionSpecifiers};
use pep508_rs::Requirement;
use pyra_python::{PythonVersion, PythonVersionRequest};
use toml_edit::{Array, DocumentMut, Item, Table, Value, value};

use crate::ProjectError;

/// The manifest scope a dependency declaration belongs to.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum DependencyDeclarationScope {
    Base,
    Group(String),
    Extra(String),
}

impl DependencyDeclarationScope {
    fn display_name(&self) -> String {
        match self {
            Self::Base => "[project].dependencies".to_string(),
            Self::Group(name) => format!("dependency group `{name}`"),
            Self::Extra(name) => format!("extra `{name}`"),
        }
    }
}

/// Reports whether a manifest mutation changed `pyproject.toml`.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct PyprojectMutationOutcome {
    pub changed: bool,
}

pub fn create_initial_pyproject(
    project_name: &str,
    selector: &PythonVersionRequest,
) -> Result<String, ProjectError> {
    let mut document =
        format!("[project]\nname = \"{project_name}\"\nversion = \"0.1.0\"\ndependencies = []\n")
            .parse::<DocumentMut>()
            .expect("generated base pyproject is valid TOML");
    set_python_selector(&mut document, selector);
    Ok(document.to_string())
}

pub fn read_python_selector(
    pyproject_path: &Utf8Path,
) -> Result<Option<PythonVersionRequest>, ProjectError> {
    let document = load_document(pyproject_path)?;
    read_selector_from_document(pyproject_path, &document)
}

pub fn update_python_selector(
    pyproject_path: &Utf8Path,
    selector: &PythonVersionRequest,
) -> Result<(), ProjectError> {
    let mut document = load_document(pyproject_path)?;
    set_python_selector(&mut document, selector);
    write_document(pyproject_path, &document)
}

/// Validates the project's declared Python support range, if present, against
/// one selected managed interpreter version.
pub fn validate_project_requires_python(
    pyproject_path: &Utf8Path,
    interpreter: &PythonVersion,
) -> Result<(), ProjectError> {
    let document = load_document(pyproject_path)?;
    let requires_python = document
        .as_table()
        .get("project")
        .and_then(Item::as_table)
        .and_then(|project| project.get("requires-python"))
        .and_then(Item::as_str);

    validate_requires_python_constraint(pyproject_path, requires_python, interpreter)
}

pub(crate) fn validate_requires_python_constraint(
    pyproject_path: &Utf8Path,
    requires_python: Option<&str>,
    interpreter: &PythonVersion,
) -> Result<(), ProjectError> {
    let Some(requires_python) = requires_python else {
        return Ok(());
    };

    let specifiers = VersionSpecifiers::from_str(requires_python).map_err(|error| {
        ProjectError::InvalidRequiresPython {
            path: pyproject_path.to_string(),
            value: requires_python.to_string(),
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
            requires_python: requires_python.to_string(),
        })
    }
}

/// Adds a dependency declaration to the selected manifest scope without
/// rewriting unrelated project metadata.
pub fn add_dependency_requirement(
    pyproject_path: &Utf8Path,
    scope: &DependencyDeclarationScope,
    requirement: &Requirement,
) -> Result<PyprojectMutationOutcome, ProjectError> {
    let mut document = load_document(pyproject_path)?;
    let array = dependency_array_mut(&mut document, scope, true)?;
    let changed = add_requirement_to_array(array, scope, requirement)?;

    if changed {
        write_document(pyproject_path, &document)?;
    }

    Ok(PyprojectMutationOutcome { changed })
}

/// Removes a dependency declaration from the selected manifest scope by package
/// name. Matching is normalized through `pep508_rs` so `remove` can stay name-
/// based while the manifest stores full requirement strings.
pub fn remove_dependency_requirement(
    pyproject_path: &Utf8Path,
    scope: &DependencyDeclarationScope,
    package: &str,
) -> Result<PyprojectMutationOutcome, ProjectError> {
    let mut document = load_document(pyproject_path)?;
    if matches!(scope, DependencyDeclarationScope::Base)
        && project_table_mut(&mut document)?
            .get("dependencies")
            .is_none()
    {
        return Err(ProjectError::MissingDependencyDeclaration {
            scope: scope.display_name(),
            dependency: package.to_string(),
        });
    }
    let array = dependency_array_mut(&mut document, scope, false)?;
    let changed = remove_requirement_from_array(array, scope, package)?;

    if changed {
        write_document(pyproject_path, &document)?;
    }

    Ok(PyprojectMutationOutcome { changed })
}

fn load_document(pyproject_path: &Utf8Path) -> Result<DocumentMut, ProjectError> {
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

fn write_document(pyproject_path: &Utf8Path, document: &DocumentMut) -> Result<(), ProjectError> {
    fs::write(pyproject_path, document.to_string()).map_err(|source| ProjectError::WritePyproject {
        path: pyproject_path.to_string(),
        source,
    })
}

fn read_selector_from_document(
    pyproject_path: &Utf8Path,
    document: &DocumentMut,
) -> Result<Option<PythonVersionRequest>, ProjectError> {
    let Some(value) = document
        .as_table()
        .get("tool")
        .and_then(Item::as_table)
        .and_then(|tool| tool.get("pyra"))
        .and_then(Item::as_table)
        .and_then(|pyra| pyra.get("python"))
        .and_then(Item::as_str)
    else {
        return Ok(None);
    };

    PythonVersionRequest::parse(value)
        .map(Some)
        .map_err(|source| ProjectError::InvalidPinnedPython {
            path: pyproject_path.to_string(),
            value: value.to_string(),
            source,
        })
}

fn set_python_selector(document: &mut DocumentMut, selector: &PythonVersionRequest) {
    let root = document.as_table_mut();
    root.entry("tool").or_insert(Item::Table(Table::new()));
    let tool = root
        .get_mut("tool")
        .and_then(Item::as_table_mut)
        .expect("tool should be a table");
    tool.entry("pyra").or_insert(Item::Table(Table::new()));
    let pyra = tool
        .get_mut("pyra")
        .and_then(Item::as_table_mut)
        .expect("tool.pyra should be a table");

    // The pinned selector is the contract between project config and future sync
    // logic, so it always stays under `[tool.pyra]` rather than mixing with
    // standard packaging metadata.
    pyra["python"] = value(selector.to_string());
}

fn dependency_array_mut<'a>(
    document: &'a mut DocumentMut,
    scope: &DependencyDeclarationScope,
    create_missing: bool,
) -> Result<&'a mut Array, ProjectError> {
    match scope {
        DependencyDeclarationScope::Base => base_dependencies_array_mut(document, create_missing),
        DependencyDeclarationScope::Group(name) => {
            named_scope_array_mut(document, "dependency-groups", name, create_missing, true)
        }
        DependencyDeclarationScope::Extra(name) => {
            let project = project_table_mut(document)?;
            if !create_missing && !project.contains_key("optional-dependencies") {
                return Err(ProjectError::UnknownOptionalDependency { name: name.clone() });
            }
            let optional_dependencies = child_table_mut(
                project,
                "optional-dependencies",
                create_missing,
                "[project.optional-dependencies]",
            )?;
            named_scope_array_mut_in_table(
                optional_dependencies,
                "optional-dependencies",
                name,
                create_missing,
                false,
            )
        }
    }
}

fn base_dependencies_array_mut(
    document: &mut DocumentMut,
    create_missing: bool,
) -> Result<&mut Array, ProjectError> {
    let project = project_table_mut(document)?;
    if create_missing && !project.contains_key("dependencies") {
        project["dependencies"] = Item::Value(Value::Array(Array::new()));
    }

    project
        .get_mut("dependencies")
        .ok_or_else(|| ProjectError::MissingDependencyDeclaration {
            scope: DependencyDeclarationScope::Base.display_name(),
            dependency: "the requested dependency".to_string(),
        })?
        .as_array_mut()
        .ok_or_else(|| ProjectError::InvalidDependencyDeclarationType {
            context: "[project].dependencies".to_string(),
        })
}

fn named_scope_array_mut<'a>(
    document: &'a mut DocumentMut,
    table_name: &str,
    requested_name: &str,
    create_missing: bool,
    is_group: bool,
) -> Result<&'a mut Array, ProjectError> {
    let root = document.as_table_mut();
    if create_missing && !root.contains_key(table_name) {
        root[table_name] = Item::Table(Table::new());
    }

    let table = root
        .get_mut(table_name)
        .ok_or_else(|| unknown_scope_error(requested_name, is_group))?
        .as_table_mut()
        .ok_or_else(|| ProjectError::InvalidDependencyDeclarationType {
            context: format!("[{table_name}]"),
        })?;

    named_scope_array_mut_in_table(table, table_name, requested_name, create_missing, is_group)
}

fn child_table_mut<'a>(
    table: &'a mut Table,
    child_name: &str,
    create_missing: bool,
    context: &str,
) -> Result<&'a mut Table, ProjectError> {
    if create_missing && !table.contains_key(child_name) {
        table[child_name] = Item::Table(Table::new());
    }

    table
        .get_mut(child_name)
        .ok_or_else(|| ProjectError::InvalidDependencyDeclarationType {
            context: context.to_string(),
        })?
        .as_table_mut()
        .ok_or_else(|| ProjectError::InvalidDependencyDeclarationType {
            context: context.to_string(),
        })
}

fn named_scope_array_mut_in_table<'a>(
    table: &'a mut Table,
    table_name: &str,
    requested_name: &str,
    create_missing: bool,
    is_group: bool,
) -> Result<&'a mut Array, ProjectError> {
    let normalized = normalize_name(requested_name);
    let key = if let Some(existing_key) = find_normalized_key(table, &normalized)? {
        existing_key
    } else if create_missing {
        requested_name.to_string()
    } else {
        return Err(unknown_scope_error(requested_name, is_group));
    };

    if create_missing && !table.contains_key(&key) {
        table[&key] = Item::Value(Value::Array(Array::new()));
    }

    table
        .get_mut(&key)
        .ok_or_else(|| unknown_scope_error(requested_name, is_group))?
        .as_array_mut()
        .ok_or_else(|| ProjectError::InvalidDependencyDeclarationType {
            context: if table_name == "optional-dependencies" {
                format!("[project.optional-dependencies].{key}")
            } else {
                format!("[{table_name}].{key}")
            },
        })
}

fn project_table_mut(document: &mut DocumentMut) -> Result<&mut Table, ProjectError> {
    document
        .as_table_mut()
        .get_mut("project")
        .and_then(Item::as_table_mut)
        .ok_or(ProjectError::MissingProjectMetadata)
}

fn find_normalized_key(
    table: &Table,
    normalized_name: &str,
) -> Result<Option<String>, ProjectError> {
    let mut matched = None;
    for (name, _) in table.iter() {
        if normalize_name(name) != normalized_name {
            continue;
        }

        if let Some(previous) = matched.replace(name.to_string()) {
            return Err(ProjectError::DuplicateNormalizedDependencyGroup {
                first: previous,
                second: name.to_string(),
            });
        }
    }

    Ok(matched)
}

fn add_requirement_to_array(
    array: &mut Array,
    scope: &DependencyDeclarationScope,
    requirement: &Requirement,
) -> Result<bool, ProjectError> {
    if array_contains_requirement(array, scope, requirement)? {
        return Ok(false);
    }

    array.push(requirement.to_string());
    Ok(true)
}

fn remove_requirement_from_array(
    array: &mut Array,
    scope: &DependencyDeclarationScope,
    package: &str,
) -> Result<bool, ProjectError> {
    let normalized_package = normalize_name(package);
    let mut removed = false;
    let mut index = 0;

    while index < array.len() {
        let requirement = parse_array_requirement(array, index, scope)?;
        if normalize_name(requirement.name.as_ref()) == normalized_package {
            array.remove(index);
            removed = true;
        } else {
            index += 1;
        }
    }

    if removed {
        Ok(true)
    } else {
        Err(ProjectError::MissingDependencyDeclaration {
            scope: scope.display_name(),
            dependency: package.to_string(),
        })
    }
}

fn array_contains_requirement(
    array: &Array,
    scope: &DependencyDeclarationScope,
    requirement: &Requirement,
) -> Result<bool, ProjectError> {
    for index in 0..array.len() {
        if parse_array_requirement(array, index, scope)? == *requirement {
            return Ok(true);
        }
    }

    Ok(false)
}

fn parse_array_requirement(
    array: &Array,
    index: usize,
    scope: &DependencyDeclarationScope,
) -> Result<Requirement, ProjectError> {
    let context = scope.display_name();
    let Some(item) = array.get(index) else {
        return Err(ProjectError::InvalidDependencyDeclarationType { context });
    };
    let Some(requirement_text) = item.as_str() else {
        return Err(ProjectError::InvalidRequirementValue { context });
    };

    Requirement::from_str(requirement_text).map_err(|source| ProjectError::InvalidRequirement {
        context,
        value: requirement_text.to_string(),
        detail: source.to_string(),
    })
}

fn unknown_scope_error(name: &str, is_group: bool) -> ProjectError {
    if is_group {
        ProjectError::UnknownDependencyGroup {
            name: name.to_string(),
        }
    } else {
        ProjectError::UnknownOptionalDependency {
            name: name.to_string(),
        }
    }
}

fn normalize_name(name: &str) -> String {
    let mut normalized = String::with_capacity(name.len());
    let mut previous_separator = false;
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            normalized.push(character.to_ascii_lowercase());
            previous_separator = false;
        } else if matches!(character, '-' | '_' | '.') && !previous_separator {
            normalized.push('-');
            previous_separator = true;
        }
    }

    normalized.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use camino::Utf8PathBuf;

    use super::{
        DependencyDeclarationScope, add_dependency_requirement, create_initial_pyproject,
        normalize_name, read_python_selector, remove_dependency_requirement,
        update_python_selector,
    };
    use pep508_rs::Requirement;
    use pyra_python::PythonVersionRequest;
    use toml_edit::DocumentMut;

    use crate::ProjectError;

    #[test]
    fn reads_and_updates_tool_pyra_python() {
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let path =
            Utf8PathBuf::from_path_buf(temp_dir.path().join("pyproject.toml")).expect("utf-8 path");
        std::fs::write(
            &path,
            r#"[project]
name = "sample"
version = "0.1.0"

[tool.pyra]
python = "3.12"
"#,
        )
        .expect("pyproject");

        let selector = read_python_selector(&path)
            .expect("read selector")
            .expect("selector");
        assert_eq!(selector.to_string(), "3.12");

        update_python_selector(&path, &PythonVersionRequest::parse("3.13").unwrap())
            .expect("update selector");
        let contents = std::fs::read_to_string(&path).expect("pyproject");
        assert!(contents.contains("python = \"3.13\""));
        assert!(contents.contains("name = \"sample\""));
    }

    #[test]
    fn initial_document_writes_tool_pyra_python() {
        let contents = create_initial_pyproject(
            "sample-project",
            &PythonVersionRequest::parse("3.13").unwrap(),
        )
        .expect("pyproject");

        assert!(contents.contains("[tool.pyra]"));
        assert!(contents.contains("python = \"3.13\""));
    }

    #[test]
    fn adds_to_base_dependencies_without_rewriting_unrelated_sections() {
        let fixture = write_pyproject(
            r#"# keep this comment
[project]
name = "sample"
version = "0.1.0"
dependencies = ["click>=8"]

[tool.pyra]
python = "3.13"
"#,
        );

        let outcome = add_dependency_requirement(
            &fixture.path,
            &DependencyDeclarationScope::Base,
            &Requirement::from_str("rich>=13").unwrap(),
        )
        .expect("add dependency");

        assert!(outcome.changed);
        let contents = std::fs::read_to_string(&fixture.path).expect("pyproject");
        assert!(contents.contains("# keep this comment"));
        assert!(contents.contains("[tool.pyra]"));
        assert_eq!(
            requirement_strings(&contents, &DependencyDeclarationScope::Base),
            vec!["click>=8".to_string(), "rich>=13".to_string()]
        );
    }

    #[test]
    fn adds_to_dependency_group_with_normalized_lookup() {
        let fixture = write_pyproject(
            r#"[project]
name = "sample"
version = "0.1.0"
dependencies = []

[dependency-groups]
Dev_Tools = ["pytest>=8"]
"#,
        );

        let outcome = add_dependency_requirement(
            &fixture.path,
            &DependencyDeclarationScope::Group("dev-tools".to_string()),
            &Requirement::from_str("ruff>=0.5").unwrap(),
        )
        .expect("add dependency");

        assert!(outcome.changed);
        let contents = std::fs::read_to_string(&fixture.path).expect("pyproject");
        assert_eq!(
            requirement_strings(
                &contents,
                &DependencyDeclarationScope::Group("dev-tools".to_string())
            ),
            vec!["pytest>=8".to_string(), "ruff>=0.5".to_string()]
        );
    }

    #[test]
    fn adds_to_extra_with_normalized_lookup() {
        let fixture = write_pyproject(
            r#"[project]
name = "sample"
version = "0.1.0"
dependencies = []

[project.optional-dependencies]
Feature_Flag = ["httpx>=0.27"]
"#,
        );

        let outcome = add_dependency_requirement(
            &fixture.path,
            &DependencyDeclarationScope::Extra("feature_flag".to_string()),
            &Requirement::from_str("rich>=13").unwrap(),
        )
        .expect("add dependency");

        assert!(outcome.changed);
        let contents = std::fs::read_to_string(&fixture.path).expect("pyproject");
        assert_eq!(
            requirement_strings(
                &contents,
                &DependencyDeclarationScope::Extra("feature_flag".to_string())
            ),
            vec!["httpx>=0.27".to_string(), "rich>=13".to_string()]
        );
    }

    #[test]
    fn removes_from_base_dependencies() {
        let fixture = write_pyproject(
            r#"[project]
name = "sample"
version = "0.1.0"
dependencies = ["click>=8", "rich>=13"]
"#,
        );

        let outcome =
            remove_dependency_requirement(&fixture.path, &DependencyDeclarationScope::Base, "rich")
                .expect("remove dependency");

        assert!(outcome.changed);
        let contents = std::fs::read_to_string(&fixture.path).expect("pyproject");
        assert_eq!(
            requirement_strings(&contents, &DependencyDeclarationScope::Base),
            vec!["click>=8".to_string()]
        );
    }

    #[test]
    fn removes_from_dependency_group() {
        let fixture = write_pyproject(
            r#"[project]
name = "sample"
version = "0.1.0"
dependencies = []

[dependency-groups]
Docs_Group = ["mkdocs>=1.6", "mike>=2"]
"#,
        );

        let outcome = remove_dependency_requirement(
            &fixture.path,
            &DependencyDeclarationScope::Group("docs-group".to_string()),
            "mkdocs",
        )
        .expect("remove dependency");

        assert!(outcome.changed);
        let contents = std::fs::read_to_string(&fixture.path).expect("pyproject");
        assert_eq!(
            requirement_strings(
                &contents,
                &DependencyDeclarationScope::Group("docs-group".to_string())
            ),
            vec!["mike>=2".to_string()]
        );
    }

    #[test]
    fn removes_from_extra() {
        let fixture = write_pyproject(
            r#"[project]
name = "sample"
version = "0.1.0"
dependencies = []

[project.optional-dependencies]
cli_tools = ["rich>=13", "typer>=0.12"]
"#,
        );

        let outcome = remove_dependency_requirement(
            &fixture.path,
            &DependencyDeclarationScope::Extra("cli-tools".to_string()),
            "typer",
        )
        .expect("remove dependency");

        assert!(outcome.changed);
        let contents = std::fs::read_to_string(&fixture.path).expect("pyproject");
        assert_eq!(
            requirement_strings(
                &contents,
                &DependencyDeclarationScope::Extra("cli-tools".to_string())
            ),
            vec!["rich>=13".to_string()]
        );
    }

    #[test]
    fn removing_missing_dependency_returns_typed_error() {
        let fixture = write_pyproject(
            r#"[project]
name = "sample"
version = "0.1.0"
dependencies = ["click>=8"]
"#,
        );

        let error =
            remove_dependency_requirement(&fixture.path, &DependencyDeclarationScope::Base, "rich")
                .expect_err("missing declaration");

        assert!(matches!(
            error,
            ProjectError::MissingDependencyDeclaration { scope, dependency }
            if scope == "[project].dependencies" && dependency == "rich"
        ));
    }

    #[test]
    fn adding_same_requirement_twice_does_not_duplicate_entries() {
        let fixture = write_pyproject(
            r#"[project]
name = "sample"
version = "0.1.0"
dependencies = ["rich>=13"]
"#,
        );

        let outcome = add_dependency_requirement(
            &fixture.path,
            &DependencyDeclarationScope::Base,
            &Requirement::from_str("rich>=13").unwrap(),
        )
        .expect("idempotent add");

        assert!(!outcome.changed);
        let contents = std::fs::read_to_string(&fixture.path).expect("pyproject");
        assert_eq!(contents.matches("rich>=13").count(), 1);
    }

    struct TestPyproject {
        _temp_dir: tempfile::TempDir,
        path: Utf8PathBuf,
    }

    fn write_pyproject(contents: &str) -> TestPyproject {
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let path =
            Utf8PathBuf::from_path_buf(temp_dir.path().join("pyproject.toml")).expect("utf-8 path");
        std::fs::write(&path, contents).expect("pyproject");
        TestPyproject {
            _temp_dir: temp_dir,
            path,
        }
    }

    fn requirement_strings(contents: &str, scope: &DependencyDeclarationScope) -> Vec<String> {
        let document = contents.parse::<DocumentMut>().expect("valid toml");
        match scope {
            DependencyDeclarationScope::Base => document["project"]["dependencies"]
                .as_array()
                .expect("base array")
                .iter()
                .map(|value: &toml_edit::Value| value.as_str().expect("requirement").to_string())
                .collect(),
            DependencyDeclarationScope::Group(name) => {
                let table = document["dependency-groups"]
                    .as_table()
                    .expect("group table");
                let mut array = None;
                for (key, item) in table.iter() {
                    if normalize_name(key) == normalize_name(name) {
                        array = item.as_array();
                        break;
                    }
                }

                array
                    .expect("group array")
                    .iter()
                    .map(|value: &toml_edit::Value| {
                        value.as_str().expect("requirement").to_string()
                    })
                    .collect()
            }
            DependencyDeclarationScope::Extra(name) => {
                let table = document["project"]["optional-dependencies"]
                    .as_table()
                    .expect("extra table");
                let mut array = None;
                for (key, item) in table.iter() {
                    if normalize_name(key) == normalize_name(name) {
                        array = item.as_array();
                        break;
                    }
                }

                array
                    .expect("extra array")
                    .iter()
                    .map(|value: &toml_edit::Value| {
                        value.as_str().expect("requirement").to_string()
                    })
                    .collect()
            }
        }
    }
}
