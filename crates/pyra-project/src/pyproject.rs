//! `pyproject.toml` read/write support for Pyra-managed settings.
//!
//! `toml_edit` lets Pyra update `[tool.pyra]` without discarding unrelated
//! project formatting, which will matter more once project metadata grows.

use std::fs;

use camino::Utf8Path;
use pyra_python::PythonVersionRequest;
use toml_edit::{DocumentMut, Item, Table, value};

use crate::ProjectError;

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
    fs::write(pyproject_path, document.to_string()).map_err(|source| ProjectError::WritePyproject {
        path: pyproject_path.to_string(),
        source,
    })
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

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;

    use super::{create_initial_pyproject, read_python_selector, update_python_selector};
    use pyra_python::PythonVersionRequest;

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
}
