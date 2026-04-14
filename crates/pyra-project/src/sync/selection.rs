//! Dependency group and extra selection semantics for `pyra sync`.
//!
//! This logic is separate from clap parsing so the command handler can translate
//! CLI flags into one typed request and reuse the same behavior in tests.

use std::collections::BTreeSet;

use crate::{ProjectError, sync::ProjectSyncInput};

pub const SYNTHETIC_DEFAULT_GROUP: &str = "pyra-default";

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct SyncSelectionRequest {
    pub groups: Vec<String>,
    pub extras: Vec<String>,
    pub no_groups: Vec<String>,
    pub all_groups: bool,
    pub all_extras: bool,
    pub no_dev: bool,
    pub only_groups: Vec<String>,
    pub only_dev: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DependencySelection {
    pub include_base: bool,
    pub groups: BTreeSet<String>,
    pub extras: BTreeSet<String>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SyncSelectionResolver;

impl SyncSelectionResolver {
    pub fn resolve(
        self,
        input: &ProjectSyncInput,
        request: &SyncSelectionRequest,
    ) -> Result<DependencySelection, ProjectError> {
        let public_groups = input
            .dependency_groups
            .iter()
            .map(|group| group.name.normalized_name.clone())
            .collect::<BTreeSet<_>>();
        let available_extras = input
            .optional_dependencies
            .iter()
            .map(|extra| extra.name.normalized_name.clone())
            .collect::<BTreeSet<_>>();

        let only_mode = !request.only_groups.is_empty() || request.only_dev;
        let mut groups = BTreeSet::new();
        let mut include_base = !only_mode;

        if only_mode {
            for group in &request.only_groups {
                let normalized = normalize_name(group);
                validate_public_group(&normalized, &public_groups)?;
                groups.insert(normalized);
            }
            if request.only_dev {
                validate_public_group("dev", &public_groups)?;
                groups.insert("dev".to_string());
            }
        } else {
            if input.has_dev_group() && !request.no_dev {
                groups.insert("dev".to_string());
            }
            if request.all_groups {
                groups.extend(public_groups.iter().cloned());
            }
            for group in &request.groups {
                let normalized = normalize_name(group);
                validate_public_group(&normalized, &public_groups)?;
                groups.insert(normalized);
            }
        }

        for group in &request.no_groups {
            let normalized = normalize_name(group);
            validate_public_group(&normalized, &public_groups)?;
            groups.remove(&normalized);
        }
        if request.no_dev {
            groups.remove("dev");
        }

        let mut extras = BTreeSet::new();
        if request.all_extras {
            extras.extend(available_extras.iter().cloned());
        }
        for extra in &request.extras {
            let normalized = normalize_name(extra);
            if !available_extras.contains(&normalized) {
                return Err(ProjectError::UnknownOptionalDependency {
                    name: extra.clone(),
                });
            }
            extras.insert(normalized);
        }

        if only_mode && groups.is_empty() && !request.only_dev {
            include_base = false;
        }

        Ok(DependencySelection {
            include_base,
            groups,
            extras,
        })
    }
}

fn validate_public_group(name: &str, groups: &BTreeSet<String>) -> Result<(), ProjectError> {
    if !groups.contains(name) {
        return Err(ProjectError::UnknownDependencyGroup {
            name: name.to_string(),
        });
    }
    Ok(())
}

pub fn normalize_name(name: &str) -> String {
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
    use pep508_rs::Requirement;
    use pyra_python::PythonVersionRequest;

    use super::{
        SYNTHETIC_DEFAULT_GROUP, SyncSelectionRequest, SyncSelectionResolver, normalize_name,
    };
    use crate::sync::ProjectSyncInput;
    use crate::sync::project_input::{
        DependencyGroupDefinition, DependencyGroupName, ProjectSyncRequirement,
    };

    #[test]
    fn normalizes_dependency_group_names() {
        assert_eq!(normalize_name("Dev_Tools"), "dev-tools");
        assert_eq!(normalize_name("docs...group"), "docs-group");
    }

    #[test]
    fn default_selection_includes_base_and_dev() {
        let selection = SyncSelectionResolver
            .resolve(&sample_input(), &SyncSelectionRequest::default())
            .expect("selection");

        assert!(selection.include_base);
        assert!(selection.groups.contains("dev"));
        assert!(selection.extras.is_empty());
    }

    #[test]
    fn exclusions_win_over_inclusions() {
        let selection = SyncSelectionResolver
            .resolve(
                &sample_input(),
                &SyncSelectionRequest {
                    all_groups: true,
                    groups: vec!["docs".to_string()],
                    no_groups: vec!["dev".to_string(), "docs".to_string()],
                    extras: vec!["feature".to_string()],
                    ..SyncSelectionRequest::default()
                },
            )
            .expect("selection");

        assert!(selection.include_base);
        assert!(!selection.groups.contains("dev"));
        assert!(!selection.groups.contains("docs"));
        assert!(selection.extras.contains("feature"));
    }

    #[test]
    fn only_group_excludes_base() {
        let selection = SyncSelectionResolver
            .resolve(
                &sample_input(),
                &SyncSelectionRequest {
                    only_groups: vec!["docs".to_string()],
                    ..SyncSelectionRequest::default()
                },
            )
            .expect("selection");

        assert!(!selection.include_base);
        assert_eq!(selection.groups, ["docs".to_string()].into_iter().collect());
    }

    fn sample_input() -> ProjectSyncInput {
        ProjectSyncInput {
            project_root: Utf8PathBuf::from("/tmp/example"),
            pyproject_path: Utf8PathBuf::from("/tmp/example/pyproject.toml"),
            pylock_path: Utf8PathBuf::from("/tmp/example/pylock.toml"),
            project_name: "example".to_string(),
            pinned_python: PythonVersionRequest::parse("3.13").unwrap(),
            declared_lock_targets: None,
            requires_python: Some("==3.13.*".to_string()),
            build_system_present: false,
            dependencies: vec![ProjectSyncRequirement {
                requirement: Requirement::from_str("rich>=13").unwrap(),
                source: SYNTHETIC_DEFAULT_GROUP.to_string(),
            }],
            optional_dependencies: vec![DependencyGroupDefinition {
                name: DependencyGroupName {
                    display_name: "feature".to_string(),
                    normalized_name: "feature".to_string(),
                },
                requirements: Vec::new(),
            }],
            dependency_groups: vec![
                DependencyGroupDefinition {
                    name: DependencyGroupName {
                        display_name: "dev".to_string(),
                        normalized_name: "dev".to_string(),
                    },
                    requirements: Vec::new(),
                },
                DependencyGroupDefinition {
                    name: DependencyGroupName {
                        display_name: "docs".to_string(),
                        normalized_name: "docs".to_string(),
                    },
                    requirements: Vec::new(),
                },
            ],
        }
    }
}
