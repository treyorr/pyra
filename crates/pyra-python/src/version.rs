use std::cmp::Ordering;
use std::fmt;

use crate::PythonError;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PythonVersionRequest {
    normalized: String,
    segments: Vec<u64>,
}

impl PythonVersionRequest {
    pub fn parse(input: &str) -> Result<Self, PythonError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(PythonError::InvalidVersion {
                input: input.to_string(),
            });
        }

        let mut segments = Vec::new();
        for segment in trimmed.split('.') {
            if segment.is_empty() || !segment.chars().all(|char| char.is_ascii_digit()) {
                return Err(PythonError::InvalidVersion {
                    input: input.to_string(),
                });
            }

            let value = segment
                .parse::<u64>()
                .map_err(|_| PythonError::InvalidVersion {
                    input: input.to_string(),
                })?;
            segments.push(value);
        }

        if !(1..=3).contains(&segments.len()) {
            return Err(PythonError::InvalidVersion {
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
}

impl fmt::Display for PythonVersionRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.normalized)
    }
}

impl Ord for PythonVersionRequest {
    fn cmp(&self, other: &Self) -> Ordering {
        self.segments.cmp(&other.segments)
    }
}

impl PartialOrd for PythonVersionRequest {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::PythonVersionRequest;

    #[test]
    fn normalizes_numeric_segments() {
        let version = PythonVersionRequest::parse("03.013.0002").expect("valid version");
        assert_eq!(version.normalized(), "3.13.2");
    }

    #[test]
    fn rejects_non_numeric_versions() {
        assert!(PythonVersionRequest::parse("3.13-dev").is_err());
        assert!(PythonVersionRequest::parse("abc").is_err());
    }
}
