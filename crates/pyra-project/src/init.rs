//! Project initialization logic for `pyra init`.
//!
//! The service writes a small but real project skeleton while keeping file
//! generation rules out of the CLI parsing layer.

use std::fs;

use camino::Utf8PathBuf;
use pyra_core::AppContext;

use crate::ProjectError;

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct InitProjectRequest {
    pub python_version: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct InitProjectOutcome {
    pub project_root: Utf8PathBuf,
    pub project_name: String,
    pub created_files: Vec<Utf8PathBuf>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ProjectService;

impl ProjectService {
    pub fn init(
        self,
        context: &AppContext,
        request: InitProjectRequest,
    ) -> Result<InitProjectOutcome, ProjectError> {
        let project_name = derive_project_name(&context.cwd)?;

        let pyproject_path = context.cwd.join("pyproject.toml");
        let main_path = context.cwd.join("main.py");
        let readme_path = context.cwd.join("README.md");
        let gitignore_path = context.cwd.join(".gitignore");

        // `pyra init` should be safe-by-default and never partially overwrite an
        // existing project layout.
        for path in [&pyproject_path, &main_path, &readme_path, &gitignore_path] {
            if path.exists() {
                return Err(ProjectError::ExistingPath {
                    path: path.to_string(),
                });
            }
        }

        fs::write(
            &pyproject_path,
            render_pyproject(&project_name, request.python_version.as_deref()),
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
        fs::write(&gitignore_path, render_gitignore()).map_err(|source| {
            ProjectError::WriteFile {
                path: gitignore_path.to_string(),
                source,
            }
        })?;

        Ok(InitProjectOutcome {
            project_root: context.cwd.clone(),
            project_name,
            created_files: vec![pyproject_path, main_path, readme_path, gitignore_path],
        })
    }
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
        // Collapse runs of common separators so generated names stay predictable
        // and close to Python package naming expectations.
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

fn render_pyproject(project_name: &str, python_version: Option<&str>) -> String {
    let mut file =
        format!("[project]\nname = \"{project_name}\"\nversion = \"0.1.0\"\ndependencies = []\n");

    // The Pyra-specific table stays isolated so future standard project metadata
    // work can evolve without coupling it to tool-managed settings.
    if let Some(version) = python_version {
        file.push_str("\n[tool.pyra]\n");
        file.push_str(&format!("python = \"{version}\"\n"));
    }

    file
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
    use tempfile::tempdir;

    use super::{InitProjectRequest, ProjectService};

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

        let outcome = ProjectService
            .init(
                &context,
                InitProjectRequest {
                    python_version: Some("3.13".to_string()),
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
