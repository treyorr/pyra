//! Project initialization file generation.
//!
//! This module owns only the project skeleton file set so higher-level project
//! workflows can reuse the same initialization rules without mixing them with
//! interpreter or environment orchestration.

use std::fs;

use camino::Utf8PathBuf;
use pyra_core::AppContext;
use pyra_python::PythonVersionRequest;

use crate::{ProjectError, pyproject::create_initial_pyproject};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct InitProjectRequest {
    pub python_selector: PythonVersionRequest,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct InitProjectOutcome {
    pub project_root: Utf8PathBuf,
    pub project_name: String,
    pub created_files: Vec<Utf8PathBuf>,
}

pub(crate) fn create_initial_layout(
    context: &AppContext,
    request: &InitProjectRequest,
) -> Result<InitProjectOutcome, ProjectError> {
    let project_name = derive_project_name(&context.cwd)?;
    let (pyproject_path, main_path, readme_path, gitignore_path) = initial_file_paths(context);
    ensure_initial_layout_is_empty([&pyproject_path, &main_path, &readme_path, &gitignore_path])?;

    fs::write(
        &pyproject_path,
        create_initial_pyproject(&project_name, &request.python_selector)?,
    )
    .map_err(|source| ProjectError::WriteFile {
        path: pyproject_path.to_string(),
        source,
    })?;
    fs::write(&main_path, render_main()).map_err(|source| ProjectError::WriteFile {
        path: main_path.to_string(),
        source,
    })?;
    fs::write(&readme_path, render_readme(&project_name)).map_err(|source| {
        ProjectError::WriteFile {
            path: readme_path.to_string(),
            source,
        }
    })?;
    fs::write(&gitignore_path, render_gitignore()).map_err(|source| ProjectError::WriteFile {
        path: gitignore_path.to_string(),
        source,
    })?;

    Ok(InitProjectOutcome {
        project_root: context.cwd.clone(),
        project_name,
        created_files: vec![pyproject_path, main_path, readme_path, gitignore_path],
    })
}

pub(crate) fn validate_initial_layout(context: &AppContext) -> Result<(), ProjectError> {
    let (pyproject_path, main_path, readme_path, gitignore_path) = initial_file_paths(context);
    ensure_initial_layout_is_empty([&pyproject_path, &main_path, &readme_path, &gitignore_path])
}

fn initial_file_paths(
    context: &AppContext,
) -> (Utf8PathBuf, Utf8PathBuf, Utf8PathBuf, Utf8PathBuf) {
    (
        context.cwd.join("pyproject.toml"),
        context.cwd.join("main.py"),
        context.cwd.join("README.md"),
        context.cwd.join(".gitignore"),
    )
}

fn ensure_initial_layout_is_empty<'a>(
    paths: impl IntoIterator<Item = &'a Utf8PathBuf>,
) -> Result<(), ProjectError> {
    // `pyra init` stays safe-by-default by refusing to overwrite any of the
    // initial project files it is responsible for generating.
    for path in paths {
        if path.exists() {
            return Err(ProjectError::ExistingPath {
                path: path.to_string(),
            });
        }
    }

    Ok(())
}

fn derive_project_name(cwd: &Utf8PathBuf) -> Result<String, ProjectError> {
    let raw_name = cwd
        .file_name()
        .ok_or_else(|| ProjectError::InvalidProjectName {
            path: cwd.to_string(),
        })?;

    let mut project_name = String::new();
    let mut previous_was_separator = false;

    for character in raw_name.chars() {
        if character.is_ascii_alphanumeric() {
            project_name.push(character.to_ascii_lowercase());
            previous_was_separator = false;
        // Collapse common separators so generated names stay close to Python
        // package naming expectations without leaking raw directory spelling.
        } else if matches!(character, '-' | '_' | ' ' | '.') && !previous_was_separator {
            project_name.push('-');
            previous_was_separator = true;
        }
    }

    project_name = project_name.trim_matches('-').to_string();

    if project_name.is_empty() {
        return Err(ProjectError::InvalidProjectName {
            path: cwd.to_string(),
        });
    }

    Ok(project_name)
}

fn render_main() -> &'static str {
    "def main() -> None:\n    print(\"Hello from Pyra.\")\n\n\nif __name__ == \"__main__\":\n    main()\n"
}

fn render_readme(project_name: &str) -> String {
    format!("# {project_name}\n\nCreated with `pyra init`.\n")
}

fn render_gitignore() -> &'static str {
    "__pycache__/\n*.pyc\n.pytest_cache/\n.venv/\n"
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;
    use pyra_core::{AppContext, AppPaths, Verbosity};
    use pyra_python::PythonVersionRequest;
    use tempfile::tempdir;

    use super::{InitProjectRequest, create_initial_layout};

    #[test]
    fn creates_initial_project_files() {
        let temp_dir = tempdir().expect("temporary directory");
        let root =
            Utf8PathBuf::from_path_buf(temp_dir.path().join("sample-app")).expect("utf-8 path");
        std::fs::create_dir_all(&root).expect("project root");

        let paths = AppPaths::from_roots(
            root.join(".pyra-config"),
            root.join(".pyra-data"),
            root.join(".pyra-cache"),
            root.join(".pyra-state"),
        );
        let context = AppContext::new(root.clone(), paths, Verbosity::Normal);

        let outcome = create_initial_layout(
            &context,
            &InitProjectRequest {
                python_selector: PythonVersionRequest::parse("3.13").unwrap(),
            },
        )
        .expect("initialized project");

        assert_eq!(outcome.project_name, "sample-app");
        assert!(root.join("pyproject.toml").exists());
        assert!(root.join("main.py").exists());
        assert!(root.join("README.md").exists());
        assert!(root.join(".gitignore").exists());
        assert!(
            std::fs::read_to_string(root.join("pyproject.toml"))
                .expect("pyproject")
                .contains("python = \"3.13\"")
        );
    }
}
