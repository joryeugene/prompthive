use anyhow::{Context, Result};
use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;
// Removed unused imports
use crate::storage::Storage;
use crate::template::TemplateProcessor;

pub struct Composer {
    storage: Storage,
    template_processor: TemplateProcessor,
}

impl Composer {
    pub fn new(storage: Storage) -> Self {
        let mut template_processor = TemplateProcessor::new();

        // Load custom variables configuration if it exists
        if let Some(config_dir) = dirs::config_dir() {
            let config_path = config_dir
                .join("prompthive")
                .join("template_variables.conf");
            if let Err(e) = template_processor.load_config(&config_path) {
                eprintln!("Warning: Failed to load template variables config: {}", e);
            }
        }

        Self {
            storage,
            template_processor,
        }
    }

    /// Compose prompts by chaining them together
    /// Format: ph compose a,b,c or ph compose a b c
    pub fn compose(&self, prompt_names: &[String]) -> Result<String> {
        if prompt_names.is_empty() {
            return Err(anyhow::anyhow!("No prompts specified for composition"));
        }

        let mut result = String::new();

        // Read stdin if available (for piping) - but don't block
        use is_terminal::IsTerminal;
        if !io::stdin().is_terminal() {
            // Only read stdin if there's actually data available
            use std::io::BufRead;
            let stdin = io::stdin();
            let mut handle = stdin.lock();

            // Check if data is available without blocking
            match handle.fill_buf() {
                Ok(buf) if !buf.is_empty() => {
                    // Data is available, read it
                    drop(handle); // Release the lock
                    io::stdin().read_to_string(&mut result)?;
                }
                _ => {
                    // No data available or error, don't block
                }
            }
        }

        // Chain prompts with fuzzy matching
        for prompt_name in prompt_names.iter() {
            let prompt_body = if prompt_name.starts_with('~')
                || prompt_name.starts_with('/')
                || prompt_name.contains('.')
            {
                // It's a file path - expand and read
                let expanded_path = shellexpand::tilde(prompt_name);
                let path = PathBuf::from(expanded_path.as_ref());

                fs::read_to_string(&path)
                    .with_context(|| format!("Failed to read file '{}'", prompt_name))?
            } else {
                // It's a prompt name - use resolve_prompt for fuzzy matching
                let resolved_name = self
                    .storage
                    .resolve_prompt(prompt_name)
                    .with_context(|| format!("Failed to resolve prompt '{}'", prompt_name))?;

                let (_, body) = self
                    .storage
                    .read_prompt(&resolved_name)
                    .with_context(|| format!("Failed to read prompt '{}'", resolved_name))?;
                body
            };

            // For first prompt, use stdin content as input
            // For subsequent prompts, use previous output as input
            let input = &result;

            // Replace placeholders in prompt
            let processed_prompt = self.process_prompt(&prompt_body, input)?;

            // Update result for next iteration
            result = processed_prompt;
        }

        Ok(result)
    }

    /// Compose prompts via Unix pipe
    /// Usage: ph use a | ph compose b,c
    pub fn compose_pipe(&self, prompt_names: &[String], input: &str) -> Result<String> {
        let mut current_input = input.to_string();

        for prompt_name in prompt_names {
            let prompt_body = if prompt_name.starts_with('~')
                || prompt_name.starts_with('/')
                || prompt_name.contains('.')
            {
                // It's a file path - expand and read
                let expanded_path = shellexpand::tilde(prompt_name);
                let path = PathBuf::from(expanded_path.as_ref());

                fs::read_to_string(&path)
                    .with_context(|| format!("Failed to read file '{}'", prompt_name))?
            } else {
                // It's a prompt name - use resolve_prompt for fuzzy matching
                let resolved_name = self
                    .storage
                    .resolve_prompt(prompt_name)
                    .with_context(|| format!("Failed to resolve prompt '{}'", prompt_name))?;

                let (_, body) = self
                    .storage
                    .read_prompt(&resolved_name)
                    .with_context(|| format!("Failed to read prompt '{}'", resolved_name))?;
                body
            };

            current_input = self.process_prompt(&prompt_body, &current_input)?;
        }

        Ok(current_input)
    }

    /// Process a single prompt, replacing placeholders with input
    fn process_prompt(&self, prompt: &str, input: &str) -> Result<String> {
        self.template_processor.process(prompt, input)
    }

    /// Execute composition and handle output
    pub fn execute_composition(
        &self,
        prompt_names: &[String],
        input: Option<String>,
        edit: bool,
    ) -> Result<()> {
        let mut result = if let Some(initial_input) = input {
            // Start with provided input
            self.compose_pipe(prompt_names, &initial_input)?
        } else {
            // Original behavior - read from stdin if available
            self.compose(prompt_names)?
        };

        // Handle --edit flag
        if edit {
            result = crate::edit::edit_content(&result)?;
        }

        // Output to stdout (for piping) or clipboard
        use is_terminal::IsTerminal;
        if io::stdout().is_terminal() {
            // Terminal output - copy to clipboard
            let mut clipboard = crate::Clipboard::new();
            clipboard.copy_or_pipe(&result, true)?;
        } else {
            // Piped output - write to stdout
            io::stdout().write_all(result.as_bytes())?;
        }

        Ok(())
    }

    /// Execute composition and return the result (for unified I/O)
    pub fn compose_and_return(
        &self,
        prompt_names: &[String],
        input: Option<String>,
        edit: bool,
    ) -> Result<String> {
        let mut result = if let Some(initial_input) = input {
            // Start with provided input
            self.compose_pipe(prompt_names, &initial_input)?
        } else {
            // Original behavior - read from stdin if available
            self.compose(prompt_names)?
        };

        // Handle --edit flag
        if edit {
            result = crate::edit::edit_content(&result)?;
        }

        Ok(result)
    }

    /// Get access to template processor for variable management
    pub fn template_processor(&mut self) -> &mut TemplateProcessor {
        &mut self.template_processor
    }

    /// Set a custom template variable
    pub fn set_template_variable(&mut self, name: &str, value: &str) {
        self.template_processor.set_custom_variable(name, value);
    }

    /// Remove a custom template variable
    pub fn remove_template_variable(&mut self, name: &str) {
        self.template_processor.remove_custom_variable(name);
    }

    /// List all available template variables
    pub fn list_template_variables(&self) -> Vec<(String, String)> {
        self.template_processor.list_available_variables()
    }

    /// Save template variables configuration
    pub fn save_template_config(&self) -> Result<()> {
        if let Some(config_dir) = dirs::config_dir() {
            let config_dir = config_dir.join("prompthive");
            std::fs::create_dir_all(&config_dir)?;
            let config_path = config_dir.join("template_variables.conf");
            self.template_processor.save_config(&config_path)?;
        }
        Ok(())
    }
}

// Helper function to parse comma-separated prompt names
pub fn parse_prompt_list(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PromptMetadata;
    use tempfile::TempDir;

    fn create_test_storage() -> (Storage, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let storage = Storage::new_with_base(temp_dir.path().to_path_buf()).unwrap();
        storage.init().unwrap();

        // Create test prompts
        let metadata1 = PromptMetadata {
            id: "format".to_string(),
            description: "Format text".to_string(),
            tags: None,
            created_at: None,
            updated_at: None,
            version: None,
            git_hash: None,
            parent_version: None,
        };
        let body1 = "Format this text nicely:\n{input}";
        storage.write_prompt("format", &metadata1, body1).unwrap();

        let metadata2 = PromptMetadata {
            id: "summarize".to_string(),
            description: "Summarize content".to_string(),
            tags: None,
            created_at: None,
            updated_at: None,
            version: None,
            git_hash: None,
            parent_version: None,
        };
        let body2 = "Summarize the following:\n{input}";
        storage
            .write_prompt("summarize", &metadata2, body2)
            .unwrap();

        (storage, temp_dir)
    }

    #[test]
    fn test_compose_single_prompt() {
        let (storage, _temp) = create_test_storage();
        let composer = Composer::new(storage);

        let result = composer
            .compose_pipe(&["format".to_string()], "hello world")
            .unwrap();
        assert!(result.contains("Format this text nicely:"));
        assert!(result.contains("hello world"));
    }

    #[test]
    fn test_compose_multiple_prompts() {
        let (storage, _temp) = create_test_storage();
        let composer = Composer::new(storage);

        let prompts = vec!["format".to_string(), "summarize".to_string()];
        let result = composer.compose_pipe(&prompts, "raw data").unwrap();

        // Should contain both prompt templates
        assert!(result.contains("Summarize the following:"));
        assert!(result.contains("Format this text nicely:"));
    }

    #[test]
    fn test_parse_prompt_list() {
        assert_eq!(parse_prompt_list("a,b,c"), vec!["a", "b", "c"]);
        assert_eq!(parse_prompt_list("a, b , c "), vec!["a", "b", "c"]);
        assert_eq!(parse_prompt_list("single"), vec!["single"]);
        assert_eq!(parse_prompt_list(""), Vec::<String>::new());
    }

    #[test]
    fn test_placeholder_replacement() {
        let (storage, _temp) = create_test_storage();
        let composer = Composer::new(storage);

        let result = composer.process_prompt("Hello {input}!", "world").unwrap();
        assert_eq!(result, "Hello world!");

        let result = composer.process_prompt("Process: {INPUT}", "data").unwrap();
        assert_eq!(result, "Process: data");
    }
}
