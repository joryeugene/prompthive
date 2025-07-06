use anyhow::Result;
use chrono::Utc;
use is_terminal::IsTerminal;
use std::time::Instant;

use crate::{Clipboard, Storage};

/// Command categories for smart default behavior
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CommandCategory {
    /// Text transformation commands (use, show, clean) - auto-clipboard in TTY
    TextTransform,
    /// Query/list commands (ls, find) - no auto-clipboard (too noisy)
    Query,
    /// Creation commands (new) - auto-clipboard when content created
    Creation,
    /// Utility commands (compose, diff, etc) - context-dependent
    Utility,
}

/// Unified I/O options for all commands that support file operations
#[derive(Debug, Clone, Default)]
pub struct IoOptions {
    pub save: Option<String>,
    pub append: Option<String>,
    pub clipboard: bool,
    pub file: Option<String>,
    pub quiet: bool,
    pub command_category: CommandCategory,
}

impl Default for CommandCategory {
    fn default() -> Self {
        CommandCategory::Utility
    }
}

impl IoOptions {
    /// Create IoOptions from command line arguments
    pub fn new(
        save: Option<&str>,
        append: Option<&str>,
        clipboard: bool,
        file: Option<&str>,
        quiet: bool,
    ) -> Self {
        Self {
            save: save.map(|s| s.to_string()),
            append: append.map(|s| s.to_string()),
            clipboard,
            file: file.map(|s| s.to_string()),
            quiet,
            command_category: CommandCategory::default(),
        }
    }
    
    /// Create IoOptions with command category
    pub fn with_category(mut self, category: CommandCategory) -> Self {
        self.command_category = category;
        self
    }

    /// Apply unified I/O logic to content
    pub fn apply_unified_io(
        &self,
        storage: &Storage,
        content: &str,
        description: &str,
        start: Instant,
    ) -> Result<()> {
        self.apply_unified_io_with_prompt(storage, content, description, None, start)
    }
    
    /// Apply unified I/O logic to content with optional prompt name for sync
    pub fn apply_unified_io_with_prompt(
        &self,
        storage: &Storage,
        content: &str,
        description: &str,
        prompt_name: Option<&str>,
        start: Instant,
    ) -> Result<()> {
        let is_tty = std::env::var("PROMPTHIVE_TEST_MODE").map(|v| v == "tty").unwrap_or_else(|_| std::io::stdout().is_terminal());
        let should_copy_to_clipboard = self.should_copy_to_clipboard(is_tty);

        // Copy to clipboard if determined by smart defaults
        let mut clipboard_success = false;
        if should_copy_to_clipboard {
            let mut clipboard_handler = Clipboard::new();
            clipboard_success = clipboard_handler.copy_to_clipboard(content)?;
            if !self.quiet {
                if clipboard_success {
                    println!("Copied to clipboard ({}ms)", start.elapsed().as_millis());
                } else {
                    eprintln!("Clipboard unavailable - showing content below:");
                }
            }
        }

        // Save as new prompt if requested
        if let Some(name) = &self.save {
            let metadata = crate::storage::PromptMetadata {
                id: name.clone(),
                description: description.to_string(),
                tags: Some(vec!["saved".to_string()]),
                created_at: Some(Utc::now().to_rfc3339()),
                updated_at: None,
                version: None,
                git_hash: None,
                parent_version: None,
            };
            storage.write_prompt(name, &metadata, content)?;
            if !self.quiet {
                println!("Saved as '{}' ({}ms)", name, start.elapsed().as_millis());
            }
        }

        // Append to existing prompt if requested
        if let Some(name) = &self.append {
            if storage.prompt_exists(name) {
                let (metadata, existing_content) = storage.read_prompt(name)?;
                let combined = format!("{}\n\n{}", existing_content, content);
                let updated_metadata = crate::storage::PromptMetadata {
                    updated_at: Some(Utc::now().to_rfc3339()),
                    ..metadata
                };
                storage.write_prompt(name, &updated_metadata, &combined)?;
                if !self.quiet {
                    println!("Appended to '{}' ({}ms)", name, start.elapsed().as_millis());
                }
            } else {
                return Err(anyhow::anyhow!("Prompt '{}' not found", name));
            }
        }

        // Write to file if requested
        if let Some(path) = &self.file {
            // Generate smart filename if path is empty
            let file_path = if path.is_empty() {
                self.generate_smart_filename(description)
            } else {
                path.clone()
            };
            
            std::fs::write(&file_path, content)?;
            if !self.quiet {
                println!("Wrote to '{}' ({}ms)", file_path, start.elapsed().as_millis());
            }
            
            // Implement bidirectional sync for -f operations if prompt name provided
            if let Some(prompt_name) = prompt_name {
                // Try to create bidirectional sync between file and prompt
                if let Ok(sync_manager) = crate::commands::SimpleSyncManager::new(storage.clone()) {
                    let file_path = std::path::PathBuf::from(&file_path);
                    if let Err(e) = sync_manager.sync_prompt(prompt_name, Some(file_path.clone())) {
                        if !self.quiet {
                            eprintln!("Warning: Could not create bidirectional sync: {}", e);
                            eprintln!("File saved, but changes won't sync automatically.");
                        }
                    } else if !self.quiet {
                        println!("Created bidirectional sync with prompt '{}'", prompt_name);
                    }
                }
            }
        }

        // Output to stdout based on context
        if !is_tty {
            // Piping mode: always output to stdout for pipe compatibility
            if self.save.is_none() && self.append.is_none() && self.file.is_none() {
                print!("{}", content);
            }
        } else {
            // TTY mode: handle clipboard success/failure properly
            match self.command_category {
                CommandCategory::TextTransform => {
                    // Text transform commands in TTY: content goes to clipboard OR stdout if clipboard failed
                    if should_copy_to_clipboard {
                        // If clipboard failed, show content as fallback
                        if !clipboard_success && self.save.is_none() && self.append.is_none() && self.file.is_none() {
                            print!("{}", content);
                        }
                    } else if self.quiet && self.save.is_none() && self.append.is_none() && self.file.is_none() {
                        // Quiet mode: always output to stdout
                        print!("{}", content);
                    }
                }
                _ => {
                    // Other commands: output to stdout if no other operations
                    if self.save.is_none() && self.append.is_none() && self.file.is_none() {
                        print!("{}", content);
                    }
                }
            }
        }

        Ok(())
    }

    /// Determine if content should be copied to clipboard based on smart defaults
    fn should_copy_to_clipboard(&self, is_tty: bool) -> bool {
        if self.quiet {
            // Quiet mode: only copy if -c flag explicitly set
            self.clipboard
        } else if !is_tty {
            // Piping: only copy if -c flag explicitly set (force clipboard)
            self.clipboard
        } else {
            // TTY mode: use smart defaults based on command category
            match self.command_category {
                CommandCategory::TextTransform => {
                    // Always auto-clipboard for text transform commands in TTY
                    true
                }
                CommandCategory::Query => {
                    // Never auto-clipboard for query commands (too noisy)
                    // Only if -c flag explicitly set
                    self.clipboard
                }
                CommandCategory::Creation => {
                    // Auto-clipboard when creating content
                    true
                }
                CommandCategory::Utility => {
                    // Context-dependent: auto-clipboard if saving/appending
                    self.clipboard || self.save.is_some() || self.append.is_some()
                }
            }
        }
    }

    /// Check if any output operations are specified
    pub fn has_output_operations(&self) -> bool {
        self.save.is_some() || self.append.is_some() || self.file.is_some()
    }
    
    /// Generate smart filename based on context
    fn generate_smart_filename(&self, description: &str) -> String {
        // Convert description to slug
        let slug = description
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-");
        
        // Limit length and add .md extension
        let truncated = if slug.len() > 50 {
            slug.chars().take(50).collect::<String>()
        } else {
            slug
        };
        
        format!("{}.md", truncated)
    }

    /// Apply I/O for display operations (like show command)
    pub fn apply_display_io(
        &self,
        content: &str,
        prompt_name: &str,
        description: &str,
        start: Instant,
    ) -> Result<()> {
        let is_tty = std::env::var("PROMPTHIVE_TEST_MODE").map(|v| v == "tty").unwrap_or_else(|_| std::io::stdout().is_terminal());
        let should_copy_to_clipboard = self.clipboard || (!self.quiet && is_tty);

        // Copy to clipboard if requested
        if should_copy_to_clipboard {
            let mut clipboard_handler = Clipboard::new();
            let clipboard_success = clipboard_handler.copy_to_clipboard(content)?;
            if !self.quiet {
                if clipboard_success {
                    println!("Copied to clipboard ({}ms)", start.elapsed().as_millis());
                } else {
                    eprintln!("Clipboard unavailable - showing content below:");
                }
            }
        }

        // Write to file if requested
        if let Some(path) = &self.file {
            std::fs::write(path, content)?;
            if !self.quiet {
                println!("Wrote to '{}' ({}ms)", path, start.elapsed().as_millis());
            }
        }

        // Display prompt only if TTY and not quiet
        if is_tty && !self.quiet {
            println!("# {}", prompt_name);
            println!("Description: {}", description);
            println!();
            println!("{}", content);
            println!();
            println!("Displayed ({}ms)", start.elapsed().as_millis());
        } else if !is_tty && self.file.is_none() {
            // Output to stdout for piping when not TTY
            print!("{}", content);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_io_options_creation() {
        let opts = IoOptions::new(Some("test"), None, true, Some("file.txt"), false);
        assert_eq!(opts.save, Some("test".to_string()));
        assert!(opts.clipboard);
        assert_eq!(opts.file, Some("file.txt".to_string()));
        assert!(!opts.quiet);
    }

    #[test]
    fn test_should_copy_to_clipboard() {
        // Default category (Utility) - no auto-clipboard in TTY
        let opts = IoOptions::new(None, None, false, None, false);
        assert!(!opts.should_copy_to_clipboard(true)); // Default category doesn't auto-clipboard
        assert!(!opts.should_copy_to_clipboard(false)); // Non-TTY default

        // TextTransform category - auto-clipboard in TTY
        let opts = IoOptions::new(None, None, false, None, false)
            .with_category(CommandCategory::TextTransform);
        assert!(opts.should_copy_to_clipboard(true)); // TextTransform auto-clipboard in TTY
        assert!(!opts.should_copy_to_clipboard(false)); // No auto-clipboard when piping

        // Query category - never auto-clipboard
        let opts = IoOptions::new(None, None, false, None, false)
            .with_category(CommandCategory::Query);
        assert!(!opts.should_copy_to_clipboard(true)); // Query never auto-clipboard
        assert!(!opts.should_copy_to_clipboard(false)); // Query never auto-clipboard

        // Utility with save operation - auto-clipboard
        let opts = IoOptions::new(Some("save"), None, false, None, false);
        assert!(opts.should_copy_to_clipboard(true)); // Auto-clipboard when saving
        assert!(!opts.should_copy_to_clipboard(false)); // No auto-clipboard when piping

        // Explicit clipboard flag
        let opts = IoOptions::new(None, None, true, None, false);
        assert!(opts.should_copy_to_clipboard(false)); // Explicit clipboard
        assert!(opts.should_copy_to_clipboard(true)); // Explicit clipboard
    }

    #[test]
    fn test_has_output_operations() {
        let opts = IoOptions::new(None, None, true, None, false);
        assert!(!opts.has_output_operations()); // Only clipboard

        let opts = IoOptions::new(Some("save"), None, false, None, false);
        assert!(opts.has_output_operations()); // Has save

        let opts = IoOptions::new(None, None, false, Some("file.txt"), false);
        assert!(opts.has_output_operations()); // Has file
    }
}
