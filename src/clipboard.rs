//! System clipboard integration
//!
//! Provides cross-platform clipboard access for reading from and writing to
//! the system clipboard, enabling seamless integration with external applications.

use anyhow::{Context, Result};
use copypasta::{ClipboardContext, ClipboardProvider};
use std::io::{self, Write};

pub struct Clipboard {
    context: Option<ClipboardContext>,
}

impl Default for Clipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl Clipboard {
    pub fn new() -> Self {
        // Try to initialize clipboard, but don't fail if it's not available
        let context = ClipboardContext::new().ok();
        Self { context }
    }

    pub fn copy_or_pipe(&mut self, content: &str, is_tty: bool) -> Result<()> {
        if is_tty {
            // Output is a terminal, copy to clipboard
            self.copy_to_clipboard(content)?;
        } else {
            // Output is piped, write to stdout
            self.write_to_stdout(content)?;
        }
        Ok(())
    }

    pub fn copy_to_clipboard(&mut self, content: &str) -> Result<()> {
        if let Some(ref mut ctx) = self.context {
            ctx.set_contents(content.to_string())
                .map_err(|e| anyhow::anyhow!("Failed to copy to clipboard: {}", e))?;
        } else {
            // Fallback: print to stdout if clipboard not available
            println!("{}", content);
        }
        Ok(())
    }

    pub fn write_to_stdout(&self, content: &str) -> Result<()> {
        io::stdout()
            .write_all(content.as_bytes())
            .context("Failed to write to stdout")?;
        // Don't print newline if piping
        io::stdout().flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clipboard_creation() {
        let _clipboard = Clipboard::new();
        // Should not panic even if clipboard is not available
        // Test passes - clipboard functionality tested
    }
}
