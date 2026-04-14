//! Typed parsing and evaluation for the limited lock marker grammar Pyra writes.
//!
//! Pyra currently records only root-membership markers for `dependency_groups`
//! and `extras`. Keeping that grammar explicit avoids fragile string matching in
//! install selection while preserving the current lock contract.

use std::fmt;

use super::LockSelection;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LockMarker {
    clauses: Vec<LockMarkerClause>,
}

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd)]
enum LockMarkerScope {
    DependencyGroups,
    Extras,
}

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd)]
pub struct LockMarkerClause {
    scope: LockMarkerScope,
    name: String,
}

impl LockMarker {
    pub fn parse(input: &str) -> Result<Self, String> {
        let mut parser = Parser::new(input);
        let mut clauses = Vec::new();

        loop {
            clauses.push(parser.parse_clause()?);
            parser.skip_whitespace();
            if parser.is_eof() {
                break;
            }

            parser.expect_keyword("or")?;
            parser.skip_whitespace();
            if parser.is_eof() {
                return Err(parser.error("expected marker clause after `or`"));
            }
        }

        Ok(Self { clauses })
    }

    pub fn from_clauses(mut clauses: Vec<LockMarkerClause>) -> Option<Self> {
        clauses.sort();
        clauses.dedup();
        (!clauses.is_empty()).then_some(Self { clauses })
    }

    pub fn matches(&self, selection: &LockSelection) -> bool {
        self.clauses.iter().any(|clause| clause.matches(selection))
    }
}

impl fmt::Display for LockMarker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, clause) in self.clauses.iter().enumerate() {
            if index > 0 {
                f.write_str(" or ")?;
            }
            write!(f, "{clause}")?;
        }
        Ok(())
    }
}

impl LockMarkerClause {
    pub fn dependency_group(name: impl Into<String>) -> Self {
        Self {
            scope: LockMarkerScope::DependencyGroups,
            name: name.into(),
        }
    }

    pub fn extra(name: impl Into<String>) -> Self {
        Self {
            scope: LockMarkerScope::Extras,
            name: name.into(),
        }
    }

    fn matches(&self, selection: &LockSelection) -> bool {
        match self.scope {
            LockMarkerScope::DependencyGroups => selection.groups.contains(&self.name),
            LockMarkerScope::Extras => selection.extras.contains(&self.name),
        }
    }
}

impl fmt::Display for LockMarkerClause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.scope {
            LockMarkerScope::DependencyGroups => {
                write!(f, "'{}' in dependency_groups", self.name)
            }
            LockMarkerScope::Extras => write!(f, "'{}' in extras", self.name),
        }
    }
}

struct Parser<'a> {
    input: &'a str,
    position: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, position: 0 }
    }

    fn parse_clause(&mut self) -> Result<LockMarkerClause, String> {
        self.skip_whitespace();
        let name = self.parse_quoted_name()?;
        self.skip_whitespace();
        self.expect_keyword("in")?;
        self.skip_whitespace();

        if self.consume_literal("dependency_groups") {
            return Ok(LockMarkerClause::dependency_group(name));
        }
        if self.consume_literal("extras") {
            return Ok(LockMarkerClause::extra(name));
        }

        Err(self.error("expected `dependency_groups` or `extras`"))
    }

    fn parse_quoted_name(&mut self) -> Result<String, String> {
        self.expect_byte(b'\'', "expected opening quote for marker name")?;
        let start = self.position;
        while let Some(byte) = self.peek_byte() {
            if byte == b'\'' {
                let name = &self.input[start..self.position];
                if name.is_empty() {
                    return Err(self.error("marker names must not be empty"));
                }
                self.position += 1;
                return Ok(name.to_string());
            }
            self.position += 1;
        }

        Err(self.error("unterminated marker name"))
    }

    fn expect_keyword(&mut self, keyword: &str) -> Result<(), String> {
        if self.consume_literal(keyword) {
            Ok(())
        } else {
            Err(self.error(&format!("expected `{keyword}`")))
        }
    }

    fn expect_byte(&mut self, expected: u8, detail: &str) -> Result<(), String> {
        match self.peek_byte() {
            Some(byte) if byte == expected => {
                self.position += 1;
                Ok(())
            }
            _ => Err(self.error(detail)),
        }
    }

    fn consume_literal(&mut self, literal: &str) -> bool {
        let remaining = &self.input[self.position..];
        if remaining.starts_with(literal) {
            self.position += literal.len();
            true
        } else {
            false
        }
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek_byte(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.position += 1;
        }
    }

    fn peek_byte(&self) -> Option<u8> {
        self.input.as_bytes().get(self.position).copied()
    }

    fn is_eof(&self) -> bool {
        self.position >= self.input.len()
    }

    fn error(&self, detail: &str) -> String {
        format!("{detail} at byte {}", self.position)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{LockMarker, LockMarkerClause};
    use crate::sync::LockSelection;

    #[test]
    fn parses_supported_group_and_extra_clauses() {
        let marker = LockMarker::parse(
            "'pyra-default' in dependency_groups or 'dev' in dependency_groups or 'feature' in extras",
        )
        .expect("marker");

        assert_eq!(
            marker.to_string(),
            "'pyra-default' in dependency_groups or 'dev' in dependency_groups or 'feature' in extras"
        );
    }

    #[test]
    fn rejects_malformed_markers() {
        let error = LockMarker::parse("'dev' in dependency_groups or").expect_err("invalid marker");

        assert!(error.contains("expected marker clause after `or`"));
    }

    #[test]
    fn evaluates_group_and_extra_combinations() {
        let marker = LockMarker::from_clauses(vec![
            LockMarkerClause::dependency_group("dev"),
            LockMarkerClause::extra("feature"),
        ])
        .expect("marker");

        let group_selection = LockSelection {
            groups: ["dev".to_string()].into_iter().collect(),
            extras: BTreeSet::new(),
            python_full_version: "3.13.12".to_string(),
            target_triple: "aarch64-apple-darwin".to_string(),
        };
        let extra_selection = LockSelection {
            groups: BTreeSet::new(),
            extras: ["feature".to_string()].into_iter().collect(),
            python_full_version: "3.13.12".to_string(),
            target_triple: "aarch64-apple-darwin".to_string(),
        };
        let missing_selection = LockSelection {
            groups: ["docs".to_string()].into_iter().collect(),
            extras: ["other".to_string()].into_iter().collect(),
            python_full_version: "3.13.12".to_string(),
            target_triple: "aarch64-apple-darwin".to_string(),
        };

        assert!(marker.matches(&group_selection));
        assert!(marker.matches(&extra_selection));
        assert!(!marker.matches(&missing_selection));
    }
}
