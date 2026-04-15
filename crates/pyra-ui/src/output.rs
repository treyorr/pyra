//! Presentation model for terminal output.
//!
//! Commands build these plain Rust data structures and hand them to `pyra-ui`
//! for rendering, which keeps terminal formatting out of domain crates.

use pyra_errors::{ErrorKind, ErrorReport};
use serde::Serialize;

/// Shared command lifecycle status used by both human and machine-readable
/// output modes.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandStatus {
    Success,
    Warn,
    Fail,
}

/// Shared exit-code categories consumed by CLI automation and tests.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExitCategory {
    Success,
    User,
    System,
    Internal,
    External,
}

impl ExitCategory {
    pub fn default_code(self) -> i32 {
        match self {
            Self::Success => 0,
            Self::User => 2,
            Self::System => 3,
            Self::Internal => 4,
            // External command failures (for example `pyra run`) pass through
            // concrete process exit codes when available.
            Self::External => 1,
        }
    }
}

/// Maps typed user-facing error kinds onto stable command exit categories.
pub fn exit_category_from_error_kind(kind: ErrorKind) -> ExitCategory {
    match kind {
        ErrorKind::User => ExitCategory::User,
        ErrorKind::System => ExitCategory::System,
        ErrorKind::Internal => ExitCategory::Internal,
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
pub struct ExitEnvelope {
    pub code: i32,
    pub category: ExitCategory,
}

impl ExitEnvelope {
    pub fn success() -> Self {
        Self::from_category(ExitCategory::Success)
    }

    pub fn from_category(category: ExitCategory) -> Self {
        Self {
            code: category.default_code(),
            category,
        }
    }

    pub fn external(code: i32) -> Self {
        Self {
            code,
            category: ExitCategory::External,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct ErrorEnvelope {
    pub summary: String,
    pub detail: Option<String>,
    pub suggestion: Option<String>,
}

impl ErrorEnvelope {
    pub fn from_report(report: ErrorReport) -> Self {
        Self {
            summary: report.summary,
            detail: report.detail,
            suggestion: report.suggestion,
        }
    }
}

/// Shared machine-readable command contract for both successful and failed
/// command execution paths.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct CommandEnvelope {
    pub status: CommandStatus,
    pub exit: ExitEnvelope,
    pub output: Option<Output>,
    pub error: Option<ErrorEnvelope>,
}

impl CommandEnvelope {
    pub fn from_execution(output: Output, exit_code: i32) -> Self {
        if exit_code == 0 {
            let status = if output.has_warnings() {
                CommandStatus::Warn
            } else {
                CommandStatus::Success
            };
            return Self {
                status,
                exit: ExitEnvelope::success(),
                output: Some(output),
                error: None,
            };
        }

        Self {
            status: CommandStatus::Fail,
            exit: ExitEnvelope::external(exit_code),
            output: Some(output),
            error: None,
        }
    }

    pub fn from_error_report(report: ErrorReport, exit: ExitEnvelope) -> Self {
        Self {
            status: CommandStatus::Fail,
            exit,
            output: None,
            error: Some(ErrorEnvelope::from_report(report)),
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    Plain,
    Success,
    Info,
    Warn,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct Message {
    pub tone: Tone,
    pub summary: String,
    pub detail: Option<String>,
    pub hint: Option<String>,
    pub verbose: Vec<String>,
}

impl Message {
    pub fn new(tone: Tone, summary: impl Into<String>) -> Self {
        Self {
            tone,
            summary: summary.into(),
            detail: None,
            hint: None,
            verbose: Vec::new(),
        }
    }

    pub fn plain(summary: impl Into<String>) -> Self {
        Self::new(Tone::Plain, summary)
    }

    pub fn success(summary: impl Into<String>) -> Self {
        Self::new(Tone::Success, summary)
    }

    pub fn info(summary: impl Into<String>) -> Self {
        Self::new(Tone::Info, summary)
    }

    pub fn warn(summary: impl Into<String>) -> Self {
        Self::new(Tone::Warn, summary)
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn with_verbose_line(mut self, line: impl Into<String>) -> Self {
        self.verbose.push(line.into());
        self
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct ListItem {
    pub label: String,
    pub detail: Option<String>,
    pub verbose: Vec<String>,
}

impl ListItem {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            detail: None,
            verbose: Vec::new(),
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn with_verbose_line(mut self, line: impl Into<String>) -> Self {
        self.verbose.push(line.into());
        self
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct ListBlock {
    pub heading: Option<String>,
    pub items: Vec<ListItem>,
    pub empty_message: Option<Message>,
}

impl ListBlock {
    pub fn new() -> Self {
        Self {
            heading: None,
            items: Vec::new(),
            // Empty states are represented explicitly so handlers can return one
            // consistent output tree instead of branching on rendering details.
            empty_message: None,
        }
    }

    pub fn with_heading(mut self, heading: impl Into<String>) -> Self {
        self.heading = Some(heading.into());
        self
    }

    pub fn with_empty_message(mut self, message: Message) -> Self {
        self.empty_message = Some(message);
        self
    }

    pub fn with_items(mut self, items: Vec<ListItem>) -> Self {
        self.items = items;
        self
    }
}

impl Default for ListBlock {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Block {
    Message(Message),
    List(ListBlock),
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize)]
pub struct Output {
    pub blocks: Vec<Block>,
}

impl Output {
    pub fn new() -> Self {
        Self { blocks: Vec::new() }
    }

    pub fn single(block: Block) -> Self {
        // A dedicated constructor keeps the common single-block path readable in
        // command handlers without encouraging ad hoc rendering shortcuts.
        Self {
            blocks: vec![block],
        }
    }

    pub fn with_block(mut self, block: Block) -> Self {
        self.blocks.push(block);
        self
    }

    fn has_warnings(&self) -> bool {
        self.blocks.iter().any(|block| match block {
            Block::Message(message) => message.tone == Tone::Warn,
            Block::List(list) => list
                .empty_message
                .as_ref()
                .is_some_and(|message| message.tone == Tone::Warn),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_status_is_success_when_output_has_no_warning_blocks() {
        let output = Output::single(Block::Message(Message::success("Sync completed.")));
        let envelope = CommandEnvelope::from_execution(output, 0);
        assert_eq!(envelope.status, CommandStatus::Success);
    }

    #[test]
    fn envelope_status_is_warn_when_output_contains_warning_message() {
        let output = Output::single(Block::Message(Message::warn("Lock is stale.")));
        let envelope = CommandEnvelope::from_execution(output, 0);
        assert_eq!(envelope.status, CommandStatus::Warn);
    }

    #[test]
    fn envelope_status_is_fail_when_exit_code_is_nonzero() {
        let envelope = CommandEnvelope::from_execution(Output::new(), 17);
        assert_eq!(envelope.status, CommandStatus::Fail);
        assert_eq!(envelope.exit, ExitEnvelope::external(17));
    }

    #[test]
    fn json_snapshot_for_warning_envelope() {
        let envelope = CommandEnvelope::from_execution(
            Output::single(Block::Message(Message::warn("Lock is stale."))),
            0,
        );

        let json = serde_json::to_string_pretty(&envelope).expect("serialize warning envelope");
        let expected = r#"{
  "status": "warn",
  "exit": {
    "code": 0,
    "category": "success"
  },
  "output": {
    "blocks": [
      {
        "type": "message",
        "value": {
          "tone": "warn",
          "summary": "Lock is stale.",
          "detail": null,
          "hint": null,
          "verbose": []
        }
      }
    ]
  },
  "error": null
}"#;
        assert_eq!(json, expected);
    }

    #[test]
    fn json_snapshot_for_error_envelope() {
        let report = ErrorReport::new(ErrorKind::User, "No project found.")
            .with_detail("A pyproject.toml file is required.")
            .with_suggestion("Run `pyra init` first.");
        let envelope = CommandEnvelope::from_error_report(
            report,
            ExitEnvelope::from_category(ExitCategory::User),
        );

        let json = serde_json::to_string_pretty(&envelope).expect("serialize error envelope");
        let expected = r#"{
  "status": "fail",
  "exit": {
    "code": 2,
    "category": "user"
  },
  "output": null,
  "error": {
    "summary": "No project found.",
    "detail": "A pyproject.toml file is required.",
    "suggestion": "Run `pyra init` first."
  }
}"#;
        assert_eq!(json, expected);
    }
}
