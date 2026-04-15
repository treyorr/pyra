use assert_cmd::Command;
use tempfile::TempDir;

#[test]
fn json_success_envelope_snapshot_for_python_list() {
    let home = temp_env_root();
    let output = base_command(&home)
        .args(["--json", "python", "list"])
        .output()
        .expect("json output");

    assert!(output.status.success(), "command should succeed");
    assert!(
        output.stderr.is_empty(),
        "json mode should not emit stderr on success"
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout utf-8");
    let expected = r#"{
  "status": "success",
  "exit": {
    "code": 0,
    "category": "success"
  },
  "output": {
    "blocks": [
      {
        "type": "list",
        "value": {
          "heading": "Installed Python versions",
          "items": [],
          "empty_message": {
            "tone": "info",
            "summary": "No Python versions are managed by Pyra yet.",
            "detail": null,
            "hint": "Install one with `pyra python install 3.13`.",
            "verbose": []
          }
        }
      }
    ]
  },
  "error": null
}
"#;
    assert_eq!(stdout, expected);
}

#[test]
fn json_failure_envelope_snapshot_for_user_error() {
    let home = temp_env_root();
    let output = base_command(&home)
        .arg("--json")
        .arg("python")
        .arg("uninstall")
        .arg("3.13.12")
        .output()
        .expect("json output");

    assert!(!output.status.success(), "command should fail");
    assert!(
        output.stderr.is_empty(),
        "json mode should not emit stderr on failure"
    );
    assert_eq!(
        output.status.code(),
        Some(2),
        "user errors should map to exit code 2"
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout utf-8");
    let expected = r#"{
  "status": "fail",
  "exit": {
    "code": 2,
    "category": "user"
  },
  "output": null,
  "error": {
    "summary": "Pyra could not find an installed Python matching `3.13.12`.",
    "detail": "No managed Python installation matched that selector.",
    "suggestion": "Run `pyra python list` to see which versions are currently installed."
  }
}
"#;
    assert_eq!(stdout, expected);
}

fn temp_env_root() -> TempDir {
    tempfile::tempdir().expect("temporary directory")
}

fn base_command(home: &TempDir) -> Command {
    let mut command = Command::cargo_bin("pyra").expect("pyra binary");
    command
        .env("PYRA_CONFIG_DIR", home.path().join("config"))
        .env("PYRA_DATA_DIR", home.path().join("data"))
        .env("PYRA_CACHE_DIR", home.path().join("cache"))
        .env("PYRA_STATE_DIR", home.path().join("state"));
    command
}
