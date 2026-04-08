//! Resolution-time environment data.

use std::str::FromStr;

use pep440_rs::Version;
use pep508_rs::MarkerEnvironment;

use crate::ResolverError;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ResolverEnvironment {
    pub markers: MarkerEnvironment,
    pub python_version: Version,
    pub python_full_version: String,
    pub target_triple: String,
}

impl ResolverEnvironment {
    pub fn new(
        markers: MarkerEnvironment,
        python_full_version: impl Into<String>,
        target_triple: impl Into<String>,
    ) -> Result<Self, ResolverError> {
        let python_full_version = python_full_version.into();
        let python_version =
            Version::from_str(&python_full_version).map_err(|_| ResolverError::ParseVersion {
                package: "python".to_string(),
                value: python_full_version.clone(),
            })?;
        Ok(Self {
            markers,
            python_version,
            python_full_version,
            target_triple: target_triple.into(),
        })
    }
}
