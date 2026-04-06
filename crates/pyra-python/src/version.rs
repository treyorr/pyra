//! Version selector and concrete version models for managed Python installs.

use std::cmp::Ordering;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::PythonError;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PythonVersionRequest {
    normalized: String,
    segments: Vec<u64>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct PythonVersion {
    major: u64,
    minor: u64,
    patch: u64,
}

impl PythonVersionRequest {
    pub fn parse(input: &str) -> Result<Self, PythonError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(PythonError::InvalidVersionRequest {
                input: input.to_string(),
            });
        }

        let mut segments = Vec::new();
        for segment in trimmed.split('.') {
            if segment.is_empty() || !segment.chars().all(|character| character.is_ascii_digit()) {
                return Err(PythonError::InvalidVersionRequest {
                    input: input.to_string(),
                });
            }

            let value = segment
                .parse::<u64>()
                .map_err(|_| PythonError::InvalidVersionRequest {
                    input: input.to_string(),
                })?;
            segments.push(value);
        }

        if !(1..=3).contains(&segments.len()) {
            return Err(PythonError::InvalidVersionRequest {
                input: input.to_string(),
            });
        }

        let normalized = segments
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(".");

        Ok(Self {
            normalized,
            segments,
        })
    }

    pub fn normalized(&self) -> &str {
        &self.normalized
    }

    pub fn is_concrete(&self) -> bool {
        self.segments.len() == 3
    }

    pub fn matches(&self, version: &PythonVersion) -> bool {
        let concrete = version.segments();
        self.segments
            .iter()
            .enumerate()
            .all(|(index, segment)| concrete[index] == *segment)
    }
}

impl fmt::Display for PythonVersionRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.normalized)
    }
}

impl PythonVersion {
    pub fn parse(input: &str) -> Result<Self, PythonError> {
        let trimmed = input.trim();
        let segments = trimmed.split('.').map(str::trim).collect::<Vec<_>>();

        if segments.len() != 3
            || segments.is_empty()
            || segments.iter().any(|segment| {
                segment.is_empty() || !segment.chars().all(|char| char.is_ascii_digit())
            })
        {
            return Err(PythonError::InvalidConcreteVersion {
                input: input.to_string(),
            });
        }

        Ok(Self {
            major: segments[0]
                .parse()
                .map_err(|_| PythonError::InvalidConcreteVersion {
                    input: input.to_string(),
                })?,
            minor: segments[1]
                .parse()
                .map_err(|_| PythonError::InvalidConcreteVersion {
                    input: input.to_string(),
                })?,
            patch: segments[2]
                .parse()
                .map_err(|_| PythonError::InvalidConcreteVersion {
                    input: input.to_string(),
                })?,
        })
    }

    pub fn segments(self) -> [u64; 3] {
        [self.major, self.minor, self.patch]
    }
}

impl fmt::Display for PythonVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl Ord for PythonVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        self.segments().cmp(&other.segments())
    }
}

impl PartialOrd for PythonVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::{PythonVersion, PythonVersionRequest};

    #[test]
    fn normalizes_request_segments() {
        let version = PythonVersionRequest::parse("03.013.0002").expect("valid version");
        assert_eq!(version.normalized(), "3.13.2");
    }

    #[test]
    fn request_matches_concrete_versions_by_prefix() {
        let version = PythonVersion::parse("3.13.2").expect("concrete version");
        assert!(
            PythonVersionRequest::parse("3")
                .expect("selector")
                .matches(&version)
        );
        assert!(
            PythonVersionRequest::parse("3.13")
                .expect("selector")
                .matches(&version)
        );
        assert!(
            PythonVersionRequest::parse("3.13.2")
                .expect("selector")
                .matches(&version)
        );
        assert!(
            !PythonVersionRequest::parse("3.12")
                .expect("selector")
                .matches(&version)
        );
    }

    #[test]
    fn rejects_non_numeric_request_versions() {
        assert!(PythonVersionRequest::parse("3.13-dev").is_err());
        assert!(PythonVersionRequest::parse("abc").is_err());
    }

    #[test]
    fn rejects_non_concrete_python_versions() {
        assert!(PythonVersion::parse("3.13").is_err());
        assert!(PythonVersion::parse("3.13.0a1").is_err());
    }
}
