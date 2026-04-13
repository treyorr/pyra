//! Project execution helpers for `pyra run`.
//!
//! This module keeps lookup and subprocess execution out of the CLI crate while
//! still reusing the sync-owned centralized environment model.

use std::fs;
use std::process::{Command, ExitStatus};

use camino::{Utf8Path, Utf8PathBuf};
use toml_edit::{DocumentMut, Item};

use crate::ProjectError;

const PROJECT_SCRIPT_RUNNER: &str = r#"
import importlib
import sys

module_name = sys.argv[1]
callable_path = sys.argv[2]
script_name = sys.argv[3]

module = importlib.import_module(module_name)
target = module
for segment in callable_path.split("."):
    target = getattr(target, segment)

sys.argv = [script_name]
result = target()
raise SystemExit(0 if result is None else result)
"#;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ProjectExecutionPlan {
    target: ResolvedRunTarget,
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum ResolvedRunTarget {
    ProjectScript {
        script_name: String,
        module: String,
        callable_path: String,
    },
    ConsoleScript {
        executable_path: Utf8PathBuf,
    },
    PythonFile {
        script_path: Utf8PathBuf,
    },
}

impl ProjectExecutionPlan {
    /// Resolves one `pyra run` target using the documented lookup order.
    pub fn resolve(
        pyproject_path: &Utf8Path,
        project_root: &Utf8Path,
        environment_path: &Utf8Path,
        target: &str,
    ) -> Result<Self, ProjectError> {
        if let Some(project_script) = read_project_script(pyproject_path, target)? {
            return Ok(Self {
                target: project_script,
            });
        }

        if let Some(console_script) = resolve_console_script(environment_path, target) {
            return Ok(Self {
                target: ResolvedRunTarget::ConsoleScript {
                    executable_path: console_script,
                },
            });
        }

        if let Some(script_path) = resolve_python_file(project_root, target) {
            return Ok(Self {
                target: ResolvedRunTarget::PythonFile { script_path },
            });
        }

        Err(ProjectError::RunTargetNotFound {
            target: target.to_string(),
        })
    }

    /// Executes the resolved target through the synchronized managed
    /// interpreter or environment script path and returns the child exit code.
    pub fn execute(
        &self,
        interpreter_path: &Utf8Path,
        project_root: &Utf8Path,
    ) -> Result<i32, ProjectError> {
        let status = match &self.target {
            ResolvedRunTarget::ProjectScript {
                script_name,
                module,
                callable_path,
            } => Command::new(interpreter_path.as_std_path())
                .arg("-c")
                .arg(PROJECT_SCRIPT_RUNNER)
                .arg(module)
                .arg(callable_path)
                .arg(script_name)
                .current_dir(project_root.as_std_path())
                .status()
                .map_err(|source| ProjectError::StartRunTarget {
                    target: script_name.clone(),
                    source,
                })?,
            ResolvedRunTarget::ConsoleScript { executable_path } => {
                let target = executable_path
                    .file_name()
                    .unwrap_or(executable_path.as_str())
                    .to_string();
                Command::new(executable_path.as_std_path())
                    .current_dir(project_root.as_std_path())
                    .status()
                    .map_err(|source| ProjectError::StartRunTarget { target, source })?
            }
            ResolvedRunTarget::PythonFile { script_path } => {
                Command::new(interpreter_path.as_std_path())
                    .arg(script_path.as_std_path())
                    .current_dir(project_root.as_std_path())
                    .status()
                    .map_err(|source| ProjectError::StartRunTarget {
                        target: script_path.to_string(),
                        source,
                    })?
            }
        };

        Ok(exit_code(&status))
    }
}

fn read_project_script(
    pyproject_path: &Utf8Path,
    target: &str,
) -> Result<Option<ResolvedRunTarget>, ProjectError> {
    let contents =
        fs::read_to_string(pyproject_path).map_err(|source| ProjectError::ReadPyproject {
            path: pyproject_path.to_string(),
            source,
        })?;
    let document =
        contents
            .parse::<DocumentMut>()
            .map_err(|source| ProjectError::ParsePyproject {
                path: pyproject_path.to_string(),
                source,
            })?;
    let Some(project) = document.as_table().get("project").and_then(Item::as_table) else {
        return Ok(None);
    };
    let Some(scripts) = project.get("scripts").and_then(Item::as_table) else {
        return Ok(None);
    };
    let Some(script_item) = scripts.get(target) else {
        return Ok(None);
    };
    let Some(entrypoint) = script_item.as_str() else {
        return Err(ProjectError::InvalidProjectScriptDefinition {
            path: pyproject_path.to_string(),
            name: target.to_string(),
        });
    };
    let Some((module, callable_path)) = entrypoint.split_once(':') else {
        return Err(ProjectError::InvalidProjectScriptEntryPoint {
            path: pyproject_path.to_string(),
            name: target.to_string(),
            value: entrypoint.to_string(),
        });
    };
    if module.is_empty() || callable_path.is_empty() {
        return Err(ProjectError::InvalidProjectScriptEntryPoint {
            path: pyproject_path.to_string(),
            name: target.to_string(),
            value: entrypoint.to_string(),
        });
    }

    Ok(Some(ResolvedRunTarget::ProjectScript {
        script_name: target.to_string(),
        module: module.to_string(),
        callable_path: callable_path.to_string(),
    }))
}

fn resolve_console_script(environment_path: &Utf8Path, target: &str) -> Option<Utf8PathBuf> {
    console_script_candidates(environment_path, target)
        .into_iter()
        .find(|path| path.exists())
}

fn console_script_candidates(environment_path: &Utf8Path, target: &str) -> Vec<Utf8PathBuf> {
    let scripts_dir = environment_scripts_dir(environment_path);
    let mut candidates = vec![scripts_dir.join(target)];
    if cfg!(windows) {
        candidates.push(scripts_dir.join(format!("{target}.exe")));
        candidates.push(scripts_dir.join(format!("{target}.cmd")));
        candidates.push(scripts_dir.join(format!("{target}.bat")));
    }
    candidates
}

fn environment_scripts_dir(environment_path: &Utf8Path) -> Utf8PathBuf {
    if cfg!(windows) {
        environment_path.join("Scripts")
    } else {
        environment_path.join("bin")
    }
}

fn resolve_python_file(project_root: &Utf8Path, target: &str) -> Option<Utf8PathBuf> {
    if !target.ends_with(".py") {
        return None;
    }

    let candidate = Utf8PathBuf::from(target);
    let script_path = if candidate.is_absolute() {
        candidate
    } else {
        project_root.join(candidate)
    };

    script_path.exists().then_some(script_path)
}

#[cfg(unix)]
fn exit_code(status: &ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;

    status
        .code()
        .or_else(|| status.signal().map(|signal| 128 + signal))
        .unwrap_or(1)
}

#[cfg(not(unix))]
fn exit_code(status: &ExitStatus) -> i32 {
    status.code().unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use camino::Utf8PathBuf;

    use super::{ProjectExecutionPlan, environment_scripts_dir};

    #[test]
    fn resolves_project_scripts_before_console_scripts() {
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).expect("utf-8 path");
        let pyproject_path = root.join("pyproject.toml");
        let environment_path = root.join("environment");
        let scripts_dir = environment_scripts_dir(&environment_path);
        fs::create_dir_all(&scripts_dir).expect("scripts dir");
        fs::write(
            &pyproject_path,
            r#"[project]
name = "sample"
version = "0.1.0"

[project.scripts]
demo = "app:main"
"#,
        )
        .expect("pyproject");
        fs::write(scripts_dir.join("demo"), "").expect("console script");

        let plan = ProjectExecutionPlan::resolve(&pyproject_path, &root, &environment_path, "demo")
            .expect("plan");

        assert!(matches!(plan, ProjectExecutionPlan { .. }));
    }
}
