//! Project identity helpers.
//!
//! Pyra stores environments outside the project tree, so every project needs a
//! stable identity derived from its canonical root path rather than from the
//! caller's current working directory spelling.

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use sha2::{Digest, Sha256};

use crate::ProjectError;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ProjectIdentity {
    pub root: Utf8PathBuf,
    pub id: String,
}

impl ProjectIdentity {
    pub fn from_root(root: &Utf8Path) -> Result<Self, ProjectError> {
        let canonical =
            fs::canonicalize(root).map_err(|source| ProjectError::CanonicalizeProjectRoot {
                path: root.to_string(),
                source,
            })?;
        let root = Utf8PathBuf::from_path_buf(canonical)
            .map_err(|path| ProjectError::NonUtf8ProjectRoot { path })?;

        Ok(Self {
            id: stable_project_id(&root),
            root,
        })
    }
}

pub fn find_project_root(start: &Utf8Path) -> Result<Utf8PathBuf, ProjectError> {
    let mut current = Some(start);
    while let Some(candidate) = current {
        if candidate.join("pyproject.toml").exists() {
            return Ok(candidate.to_path_buf());
        }
        current = candidate.parent();
    }

    Err(ProjectError::ProjectNotFound {
        start: start.to_string(),
    })
}

fn stable_project_id(root: &Utf8Path) -> String {
    let digest = Sha256::digest(root.as_str().as_bytes());
    format!("{digest:x}")
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;

    use super::{ProjectIdentity, find_project_root};

    #[test]
    fn canonical_project_root_produces_stable_identity() {
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let root =
            Utf8PathBuf::from_path_buf(temp_dir.path().join("sample-project")).expect("utf-8 path");
        std::fs::create_dir_all(&root).expect("project root");

        let alias = root.join("nested").join("..");
        std::fs::create_dir_all(root.join("nested")).expect("nested directory");

        let left = ProjectIdentity::from_root(&root).expect("identity");
        let right = ProjectIdentity::from_root(&alias).expect("identity");

        assert_eq!(left.root, right.root);
        assert_eq!(left.id, right.id);
    }

    #[test]
    fn finds_project_root_from_nested_directory() {
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let root =
            Utf8PathBuf::from_path_buf(temp_dir.path().join("sample-project")).expect("utf-8 path");
        let nested = root.join("src").join("package");
        std::fs::create_dir_all(&nested).expect("nested directory");
        std::fs::write(
            root.join("pyproject.toml"),
            "[project]\nname = \"sample\"\n",
        )
        .expect("pyproject");

        let found = find_project_root(&nested).expect("project root");
        assert_eq!(found, root);
    }
}
