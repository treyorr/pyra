//! Reusable project execution helpers.
//!
//! `pyra run` and later runtime commands must all build on the same
//! sync-owned environment contract. This module owns the execution context
//! assembly that happens after command orchestration has decided to execute a
//! project target: sync-before-exec, managed interpreter selection,
//! centralized environment lookup, documented target resolution, and child
//! process launch.

use std::fs;
use std::process::{Command, ExitStatus};

use pyra_core::AppContext;

use crate::{
    ProjectError,
    environment::{ProjectEnvironmentRecord, ProjectEnvironmentStore, ProjectPythonSelection},
    service::{ProjectService, SyncProjectOutcome, SyncProjectRequest, selected_installation},
    sync::ProjectSyncInputLoader,
};
use camino::{Utf8Path, Utf8PathBuf};
use toml_edit::{DocumentMut, Item};

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

/// Request to execute one project target through the synchronized centralized
/// environment.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct ProjectExecutionRequest {
    pub target: String,
}

/// Outcome for one execution request.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct ProjectExecutionOutcome {
    pub exit_code: i32,
}

/// Shared execution entrypoint used by `pyra run` and future runtime
/// commands. Keeping this service in the project crate prevents the CLI layer
/// from reimplementing project discovery, sync, or environment rules.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ProjectExecutionService;

#[derive(Debug, Clone, Eq, PartialEq)]
struct ProjectExecutionContext {
    project_root: Utf8PathBuf,
    environment: ProjectEnvironmentRecord,
    plan: ProjectExecutionPlan,
}

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

impl ProjectExecutionService {
    pub(crate) async fn execute(
        self,
        context: &AppContext,
        request: ProjectExecutionRequest,
    ) -> Result<ProjectExecutionOutcome, ProjectError> {
        // Execution must always build on the same sync pipeline as normal
        // package management, so runtime commands never grow a separate
        // environment path.
        let sync = ProjectService
            .sync(context, SyncProjectRequest::default())
            .await?;
        let execution = self.assemble_context(context, &sync, &request.target)?;

        Ok(ProjectExecutionOutcome {
            exit_code: execution.execute()?,
        })
    }

    fn assemble_context(
        self,
        context: &AppContext,
        sync: &SyncProjectOutcome,
        target: &str,
    ) -> Result<ProjectExecutionContext, ProjectError> {
        let input = ProjectSyncInputLoader.load(context)?;
        let identity = input.project_identity()?;
        let installation = selected_installation(context, &input.pinned_python)?;
        input.validate_selected_interpreter(&installation.version)?;
        let environment = ProjectEnvironmentStore.ensure(
            context,
            &identity,
            &ProjectPythonSelection {
                selector: input.pinned_python.clone(),
                installation,
            },
        )?;
        let plan = ProjectExecutionPlan::resolve(
            &input.pyproject_path,
            &input.project_root,
            &environment.environment_path,
            target,
        )?;

        Ok(ProjectExecutionContext {
            project_root: sync.project_root.clone(),
            environment,
            plan,
        })
    }
}

impl ProjectExecutionContext {
    fn execute(&self) -> Result<i32, ProjectError> {
        self.plan
            .execute(&self.environment.interpreter_path, &self.project_root)
    }
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
    use std::path::PathBuf;
    use std::process::Command;

    use camino::Utf8PathBuf;
    use pyra_core::{AppContext, AppPaths, Verbosity};
    use pyra_python::{ArchiveFormat, HostTarget, InstalledPythonRecord, PythonVersion};

    use super::{
        ProjectExecutionPlan, ProjectExecutionService, ResolvedRunTarget, environment_scripts_dir,
    };
    use crate::environment::{ProjectEnvironmentStore, ProjectPythonSelection};
    use crate::service::SyncProjectOutcome;

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

    #[test]
    fn assembles_execution_context_for_python_file_targets() {
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().join("workspace").join("sample"))
            .expect("utf-8 root");
        fs::create_dir_all(&root).expect("project root");
        let python_version = system_python_version().expect("system python version");
        write_pyproject(
            &root.join("pyproject.toml"),
            &python_version,
            r#"
[project]
name = "sample"
version = "0.1.0"
dependencies = []

[tool.pyra]
python = "{python_version}"
"#,
        );
        fs::write(
            root.join("hello.py"),
            "print('hello from execution context')\n",
        )
        .expect("python file");
        let context = context_for_project(temp_dir.path(), &root);
        seed_managed_install(&context, &python_version).expect("managed install");

        let execution = ProjectExecutionService
            .assemble_context(&context, &sync_outcome(&root), "hello.py")
            .expect("execution context");

        assert_eq!(execution.project_root, root);
        assert!(execution.environment.environment_path.exists());
        assert!(execution.environment.interpreter_path.exists());
        assert!(matches!(
            execution.plan.target,
            ResolvedRunTarget::PythonFile { ref script_path } if script_path == &root.join("hello.py")
        ));
    }

    #[test]
    fn assembles_execution_context_for_console_script_targets() {
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let root = Utf8PathBuf::from_path_buf(temp_dir.path().join("workspace").join("sample"))
            .expect("utf-8 root");
        fs::create_dir_all(&root).expect("project root");
        let python_version = system_python_version().expect("system python version");
        write_pyproject(
            &root.join("pyproject.toml"),
            &python_version,
            r#"
[project]
name = "sample"
version = "0.1.0"
dependencies = []

[tool.pyra]
python = "{python_version}"
"#,
        );
        let context = context_for_project(temp_dir.path(), &root);
        seed_managed_install(&context, &python_version).expect("managed install");
        let identity = crate::sync::ProjectSyncInputLoader
            .load(&context)
            .expect("project input")
            .project_identity()
            .expect("project identity");
        let environment = ProjectEnvironmentStore
            .ensure(
                &context,
                &identity,
                &ProjectPythonSelection {
                    selector: pyra_python::PythonVersionRequest::parse(&python_version)
                        .expect("python selector"),
                    installation: crate::service::selected_installation(
                        &context,
                        &pyra_python::PythonVersionRequest::parse(&python_version)
                            .expect("python selector"),
                    )
                    .expect("selected installation"),
                },
            )
            .expect("environment");
        let scripts_dir = environment_scripts_dir(&environment.environment_path);
        fs::create_dir_all(&scripts_dir).expect("scripts dir");
        fs::write(scripts_dir.join("demo"), "").expect("console script");

        let execution = ProjectExecutionService
            .assemble_context(&context, &sync_outcome(&root), "demo")
            .expect("execution context");

        assert_eq!(
            execution.environment.environment_path,
            environment.environment_path
        );
        assert!(matches!(
            execution.plan.target,
            ResolvedRunTarget::ConsoleScript { ref executable_path }
                if executable_path == &scripts_dir.join("demo")
        ));
    }

    fn sync_outcome(project_root: &camino::Utf8Path) -> SyncProjectOutcome {
        SyncProjectOutcome {
            project_root: project_root.to_path_buf(),
            pyproject_path: project_root.join("pyproject.toml"),
            pylock_path: project_root.join("pylock.toml"),
            project_id: "project-id".to_string(),
            python_version: "3.13.12".to_string(),
            lock_refreshed: false,
            selected_groups: Vec::new(),
            selected_extras: Vec::new(),
            installed_packages: 0,
            removed_packages: 0,
            project_installed: false,
        }
    }

    fn context_for_project(
        temp_root: &std::path::Path,
        project_root: &camino::Utf8Path,
    ) -> AppContext {
        let config_dir =
            Utf8PathBuf::from_path_buf(temp_root.join("config")).expect("utf-8 config");
        let data_dir = Utf8PathBuf::from_path_buf(temp_root.join("data")).expect("utf-8 data");
        let cache_dir = Utf8PathBuf::from_path_buf(temp_root.join("cache")).expect("utf-8 cache");
        let state_dir = Utf8PathBuf::from_path_buf(temp_root.join("state")).expect("utf-8 state");
        let paths = AppPaths::from_roots(config_dir, data_dir, cache_dir, state_dir);
        AppContext::new(project_root.to_path_buf(), paths, Verbosity::Normal)
    }

    fn write_pyproject(path: &camino::Utf8Path, python_version: &str, template: &str) {
        let rendered = template.replace("{python_version}", python_version);
        fs::write(path, rendered.trim_start()).expect("pyproject");
    }

    fn seed_managed_install(
        context: &AppContext,
        version: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let install_dir = context.paths.python_version_dir(version);
        fs::create_dir_all(&install_dir)?;

        let record = InstalledPythonRecord {
            version: PythonVersion::parse(version)?,
            implementation: "cpython".to_string(),
            build_id: "20260325".to_string(),
            target_triple: HostTarget::detect()?.target_triple().to_string(),
            asset_name: format!("cpython-{version}.tar.gz"),
            archive_format: ArchiveFormat::TarGz,
            download_url: "file:///dev/null".to_string(),
            checksum_sha256: None,
            install_dir,
            executable_path: Utf8PathBuf::from_path_buf(system_python()?)
                .expect("utf-8 python path"),
        };

        fs::write(
            record.install_dir.join("installation.json"),
            serde_json::to_vec_pretty(&record)?,
        )?;

        Ok(())
    }

    fn system_python() -> Result<PathBuf, Box<dyn std::error::Error>> {
        for candidate in ["python3", "python"] {
            let output = Command::new(candidate)
                .args(["-c", "import sys; print(sys.executable)"])
                .output();
            match output {
                Ok(output) if output.status.success() => {
                    let path = String::from_utf8(output.stdout)?.trim().to_string();
                    if !path.is_empty() {
                        return Ok(PathBuf::from(path));
                    }
                }
                Ok(_) | Err(_) => {}
            }
        }

        Err("no usable system python was found for execution tests".into())
    }

    fn system_python_version() -> Result<String, Box<dyn std::error::Error>> {
        let output = Command::new(system_python()?)
            .args([
                "-c",
                "import sys; print('.'.join(map(str, sys.version_info[:3])))",
            ])
            .output()?;
        if !output.status.success() {
            return Err("failed to determine system python version".into());
        }

        Ok(String::from_utf8(output.stdout)?.trim().to_string())
    }
}
