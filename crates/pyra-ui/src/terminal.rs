use std::io::{self, Stderr, Stdout, Write};

use anstream::AutoStream;
use pyra_core::Verbosity;
use pyra_errors::{ErrorKind, UserFacingError};

use crate::{Block, ListBlock, Message, Output, Tone};

pub struct Terminal {
    stdout: AutoStream<Stdout>,
    stderr: AutoStream<Stderr>,
    verbosity: Verbosity,
}

impl Terminal {
    pub fn new(verbosity: Verbosity) -> Self {
        Self {
            stdout: AutoStream::auto(io::stdout()),
            stderr: AutoStream::auto(io::stderr()),
            verbosity,
        }
    }

    pub fn render(&mut self, output: &Output) -> io::Result<()> {
        for (index, block) in output.blocks.iter().enumerate() {
            if index > 0 {
                writeln!(self.stdout)?;
            }

            match block {
                Block::Message(message) => self.render_message(message)?,
                Block::List(list) => self.render_list(list)?,
            }
        }

        self.stdout.flush()
    }

    pub fn render_error<E>(&mut self, error: &E) -> io::Result<()>
    where
        E: UserFacingError,
    {
        let report = error.report();
        let label = match report.kind {
            ErrorKind::User => "error",
            ErrorKind::System => "error",
            ErrorKind::Internal => "internal error",
        };

        writeln!(self.stderr, "{label}: {}", report.summary)?;

        if let Some(detail) = report.detail {
            writeln!(self.stderr, "{detail}")?;
        }

        if let Some(suggestion) = report.suggestion {
            writeln!(self.stderr, "next: {suggestion}")?;
        }

        if self.verbosity.is_verbose() {
            if let Some(verbose_detail) = report.verbose_detail {
                writeln!(self.stderr, "detail: {verbose_detail}")?;
            }
        }

        self.stderr.flush()
    }

    fn render_message(&mut self, message: &Message) -> io::Result<()> {
        match message.tone {
            Tone::Plain | Tone::Success | Tone::Info => {
                writeln!(self.stdout, "{}", message.summary)?
            }
            Tone::Warn => writeln!(self.stdout, "warning: {}", message.summary)?,
        }

        if let Some(detail) = &message.detail {
            writeln!(self.stdout, "{detail}")?;
        }

        if let Some(hint) = &message.hint {
            writeln!(self.stdout, "{hint}")?;
        }

        if self.verbosity.is_verbose() {
            for line in &message.verbose {
                writeln!(self.stdout, "{line}")?;
            }
        }

        Ok(())
    }

    fn render_list(&mut self, list: &ListBlock) -> io::Result<()> {
        if list.items.is_empty() {
            if let Some(message) = &list.empty_message {
                self.render_message(message)?;
            }
            return Ok(());
        }

        if let Some(heading) = &list.heading {
            writeln!(self.stdout, "{heading}")?;
        }

        for item in &list.items {
            writeln!(self.stdout, "- {}", item.label)?;

            if let Some(detail) = &item.detail {
                writeln!(self.stdout, "  {detail}")?;
            }

            if self.verbosity.is_verbose() {
                for line in &item.verbose {
                    writeln!(self.stdout, "  {line}")?;
                }
            }
        }

        Ok(())
    }
}
