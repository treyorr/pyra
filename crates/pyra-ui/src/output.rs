//! Presentation model for terminal output.
//!
//! Commands build these plain Rust data structures and hand them to `pyra-ui`
//! for rendering, which keeps terminal formatting out of domain crates.

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Tone {
    Plain,
    Success,
    Info,
    Warn,
}

#[derive(Debug, Clone, Eq, PartialEq)]
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

#[derive(Debug, Clone, Eq, PartialEq)]
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

#[derive(Debug, Clone, Eq, PartialEq)]
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

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Block {
    Message(Message),
    List(ListBlock),
}

#[derive(Debug, Clone, Default, Eq, PartialEq)]
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
}
