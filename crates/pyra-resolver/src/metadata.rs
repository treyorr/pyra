//! Core metadata parsing for distributions served by the Simple API.

use std::str::FromStr;

use pep440_rs::VersionSpecifiers;
use pep508_rs::Requirement;

use crate::ResolverError;

#[derive(Debug, Clone)]
pub struct DistributionMetadata {
    pub requires_python: Option<VersionSpecifiers>,
    pub dependencies: Vec<Requirement>,
}

pub fn parse_distribution_metadata(
    package: &str,
    contents: &str,
) -> Result<DistributionMetadata, ResolverError> {
    let mut requires_python = None;
    let mut dependencies = Vec::new();
    for line in contents.lines() {
        if let Some(value) = line.strip_prefix("Requires-Python:") {
            let value = value.trim();
            requires_python = Some(VersionSpecifiers::from_str(value).map_err(|_| {
                ResolverError::ParseVersion {
                    package: package.to_string(),
                    value: value.to_string(),
                }
            })?);
        } else if let Some(value) = line.strip_prefix("Requires-Dist:") {
            let value = value.trim();
            dependencies.push(Requirement::from_str(value).map_err(|_| {
                ResolverError::ParseRequirement {
                    package: package.to_string(),
                    value: value.to_string(),
                }
            })?);
        }
    }
    Ok(DistributionMetadata {
        requires_python,
        dependencies,
    })
}
