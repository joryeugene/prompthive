//! Template processing and variable substitution
//!
//! This module provides template processing capabilities with support for multiple
//! variable types including system variables, environment variables, context variables,
//! and custom user-defined variables.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::env;
use std::process::Command;

/// Template variable processor for PromptHive
///
/// Supports multiple variable types:
/// - System variables (date, time, user info)
/// - Context variables (clipboard, current directory)
/// - Environment variables
/// - Custom user-defined variables
pub struct TemplateProcessor {
    custom_variables: HashMap<String, String>,
}

impl TemplateProcessor {
    /// Create a new template processor instance
    pub fn new() -> Self {
        Self {
            custom_variables: HashMap::new(),
        }
    }

    /// Process a template string, replacing all variables with their values
    ///
    /// Supports various variable types:
    /// - `{input}` - The input text provided to the template
    /// - `{date}` - Current date in YYYY-MM-DD format
    /// - `{time}` - Current time in HH:MM:SS format
    /// - `{user}` - Current username
    /// - `{hostname}` - System hostname
    /// - `{cwd}` - Current working directory
    /// - `{clipboard}` - Current clipboard contents
    /// - `{env:VAR_NAME}` - Environment variable value
    /// - Custom variables set via `set_variable`
    ///
    /// # Arguments
    ///
    /// * `template` - The template string containing variables to substitute
    /// * `input` - The input text to substitute for `{input}` variables
    ///
    /// # Returns
    ///
    /// The processed template with all variables substituted
    ///
    /// # Errors
    ///
    /// Returns an error if variable substitution fails or if required commands
    /// for system variables are not available.
    pub fn process(&self, template: &str, input: &str) -> Result<String> {
        // Early return if no variables to process
        if !template.contains('{') {
            if input.is_empty() {
                return Ok(template.to_string());
            } else {
                // Legacy behavior: append input if no placeholders
                return Ok(format!("{}\n\n{}", template, input));
            }
        }

        let mut result = template.to_string();

        // Process in order of precedence:
        // 1. Input variables (legacy compatibility)
        if !input.is_empty()
            && (result.contains("{input}")
                || result.contains("{INPUT}")
                || result.contains("{content}")
                || result.contains("{CONTENT}"))
        {
            result = self.process_input_variables(&result, input);
        }

        // 2. System variables - only if template contains them
        if result.contains("{date}")
            || result.contains("{time}")
            || result.contains("{datetime}")
            || result.contains("{timestamp}")
            || result.contains("{iso_date}")
            || result.contains("{user}")
            || result.contains("{hostname}")
            || result.contains("{uuid}")
        {
            result = self.process_system_variables(&result)?;
        }

        // 3. Context variables - only if template contains them
        if result.contains("{cwd}") || result.contains("{pwd}") || result.contains("{git_") {
            result = self.process_context_variables(&result)?;
        }

        // 4. Environment variables - only if template contains them
        if result.contains("{env:") {
            result = self.process_environment_variables(&result)?;
        }

        // 5. Custom variables - only if we have any and template might contain them
        if !self.custom_variables.is_empty() {
            result = self.process_custom_variables(&result);
        }

        Ok(result)
    }

    /// Add or update a custom variable
    pub fn set_custom_variable(&mut self, name: &str, value: &str) {
        self.custom_variables
            .insert(name.to_string(), value.to_string());
    }

    /// Remove a custom variable
    pub fn remove_custom_variable(&mut self, name: &str) {
        self.custom_variables.remove(name);
    }

    /// Get all custom variables
    pub fn get_custom_variables(&self) -> &HashMap<String, String> {
        &self.custom_variables
    }

    /// Process legacy input variables for backward compatibility
    fn process_input_variables(&self, template: &str, input: &str) -> String {
        let mut result = template.to_string();

        // Replace common placeholders (case-insensitive for compatibility)
        result = result.replace("{input}", input);
        result = result.replace("{INPUT}", input);
        result = result.replace("{content}", input);
        result = result.replace("{CONTENT}", input);

        result
    }

    /// Process system variables like {date}, {time}, {user}, etc.
    fn process_system_variables(&self, template: &str) -> Result<String> {
        let mut result = template.to_string();

        // Date and time variables
        let now = chrono::Utc::now();
        let local = chrono::Local::now();

        result = result.replace("{date}", &now.format("%Y-%m-%d").to_string());
        result = result.replace("{time}", &local.format("%H:%M:%S").to_string());
        result = result.replace("{datetime}", &local.format("%Y-%m-%d %H:%M:%S").to_string());
        result = result.replace("{timestamp}", &now.timestamp().to_string());
        result = result.replace("{iso_date}", &now.to_rfc3339());

        // User and system variables
        result = result.replace(
            "{user}",
            &env::var("USER").unwrap_or_else(|_| "unknown".to_string()),
        );
        result = result.replace("{hostname}", &gethostname::gethostname().to_string_lossy());

        // Process UUID variable
        result = result.replace("{uuid}", &uuid::Uuid::new_v4().to_string());

        Ok(result)
    }

    /// Process context variables like {cwd}, {git_branch}, etc.
    fn process_context_variables(&self, template: &str) -> Result<String> {
        let mut result = template.to_string();

        // Current working directory
        if let Ok(cwd) = env::current_dir() {
            result = result.replace("{cwd}", &cwd.to_string_lossy());
            if let Some(dir_name) = cwd.file_name() {
                result = result.replace("{pwd}", &dir_name.to_string_lossy());
            }
        }

        // Git context variables
        result = self.process_git_variables(&result)?;

        Ok(result)
    }

    /// Process git-related variables
    fn process_git_variables(&self, template: &str) -> Result<String> {
        let mut result = template.to_string();

        // Get git branch
        if let Ok(output) = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
        {
            if output.status.success() {
                let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
                result = result.replace("{git_branch}", &branch);
            } else {
                result = result.replace("{git_branch}", "");
            }
        } else {
            result = result.replace("{git_branch}", "");
        }

        // Get git status (clean/dirty)
        if let Ok(output) = Command::new("git").args(["status", "--porcelain"]).output() {
            if output.status.success() {
                let status = if output.stdout.is_empty() {
                    "clean"
                } else {
                    "dirty"
                };
                result = result.replace("{git_status}", status);
            } else {
                result = result.replace("{git_status}", "");
            }
        } else {
            result = result.replace("{git_status}", "");
        }

        // Get git commit hash
        if let Ok(output) = Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .output()
        {
            if output.status.success() {
                let hash = String::from_utf8_lossy(&output.stdout).trim().to_string();
                result = result.replace("{git_hash}", &hash);
            } else {
                result = result.replace("{git_hash}", "");
            }
        } else {
            result = result.replace("{git_hash}", "");
        }

        Ok(result)
    }

    /// Process environment variables in format {env:VAR_NAME}
    fn process_environment_variables(&self, template: &str) -> Result<String> {
        let mut result = template.to_string();

        // Find all {env:VAR_NAME} patterns
        let env_regex = regex::Regex::new(r"\{env:([^}]+)\}").unwrap();

        // Process each environment variable
        for caps in env_regex.captures_iter(template) {
            let full_match = &caps[0];
            let var_name = &caps[1];

            let value = env::var(var_name).unwrap_or_else(|_| {
                eprintln!("Warning: Environment variable '{}' not found", var_name);
                String::new()
            });

            result = result.replace(full_match, &value);
        }

        Ok(result)
    }

    /// Process custom variables
    fn process_custom_variables(&self, template: &str) -> String {
        let mut result = template.to_string();

        for (name, value) in &self.custom_variables {
            let placeholder = format!("{{{}}}", name);
            result = result.replace(&placeholder, value);
        }

        result
    }

    /// Load custom variables from configuration
    pub fn load_config(&mut self, config_path: &std::path::Path) -> Result<()> {
        if !config_path.exists() {
            return Ok(()); // No config file is fine
        }

        let content = std::fs::read_to_string(config_path)
            .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;

        // Parse simple key=value format
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim();

                // Remove quotes if present
                let value = if (value.starts_with('"') && value.ends_with('"'))
                    || (value.starts_with('\'') && value.ends_with('\''))
                {
                    &value[1..value.len() - 1]
                } else {
                    value
                };

                self.custom_variables
                    .insert(key.to_string(), value.to_string());
            }
        }

        Ok(())
    }

    /// Save custom variables to configuration
    pub fn save_config(&self, config_path: &std::path::Path) -> Result<()> {
        let mut content = String::new();
        content.push_str("# PromptHive Template Variables Configuration\n");
        content.push_str("# Format: variable_name=value\n");
        content.push_str("# Use quotes for values with spaces\n\n");

        let mut vars: Vec<_> = self.custom_variables.iter().collect();
        vars.sort_by_key(|(k, _)| *k);

        for (name, value) in vars {
            if value.contains(' ') || value.contains('\t') {
                content.push_str(&format!("{}=\"{}\"\n", name, value));
            } else {
                content.push_str(&format!("{}={}\n", name, value));
            }
        }

        std::fs::write(config_path, content)
            .with_context(|| format!("Failed to write config file: {}", config_path.display()))?;

        Ok(())
    }

    /// List all available variables (for help/documentation)
    pub fn list_available_variables(&self) -> Vec<(String, String)> {
        let mut vars = vec![
            // System variables
            (
                "{date}".to_string(),
                "Current date (YYYY-MM-DD)".to_string(),
            ),
            ("{time}".to_string(), "Current time (HH:MM:SS)".to_string()),
            (
                "{datetime}".to_string(),
                "Current date and time".to_string(),
            ),
            ("{timestamp}".to_string(), "Unix timestamp".to_string()),
            ("{iso_date}".to_string(), "ISO 8601 date/time".to_string()),
            ("{user}".to_string(), "Current username".to_string()),
            ("{hostname}".to_string(), "System hostname".to_string()),
            ("{uuid}".to_string(), "Random UUID".to_string()),
            // Context variables
            (
                "{cwd}".to_string(),
                "Current working directory (full path)".to_string(),
            ),
            ("{pwd}".to_string(), "Current directory name".to_string()),
            ("{git_branch}".to_string(), "Current git branch".to_string()),
            (
                "{git_status}".to_string(),
                "Git status (clean/dirty)".to_string(),
            ),
            (
                "{git_hash}".to_string(),
                "Git commit hash (short)".to_string(),
            ),
            // Legacy input variables
            ("{input}".to_string(), "Input content".to_string()),
            ("{content}".to_string(), "Input content (alias)".to_string()),
            // Environment variables
            (
                "{env:VAR_NAME}".to_string(),
                "Environment variable".to_string(),
            ),
        ];

        // Custom variables
        for (name, value) in &self.custom_variables {
            vars.push((format!("{{{}}}", name), format!("Custom: {}", value)));
        }

        vars.sort_by(|a, b| a.0.cmp(&b.0));
        vars
    }
}

impl Default for TemplateProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_input_variables() {
        let processor = TemplateProcessor::new();

        let result = processor
            .process("Process this: {input}", "test data")
            .unwrap();
        assert_eq!(result, "Process this: test data");

        let result = processor.process("Content: {content}", "hello").unwrap();
        assert_eq!(result, "Content: hello");
    }

    #[test]
    fn test_system_variables() {
        let processor = TemplateProcessor::new();

        let result = processor.process("Today is {date}", "").unwrap();
        assert!(result.contains("Today is 2"));

        let result = processor.process("User: {user}", "").unwrap();
        assert!(result.starts_with("User: "));
    }

    #[test]
    fn test_environment_variables() {
        let processor = TemplateProcessor::new();

        env::set_var("TEST_VAR", "test_value");
        let result = processor.process("Env: {env:TEST_VAR}", "").unwrap();
        assert_eq!(result, "Env: test_value");

        let result = processor.process("Missing: {env:NONEXISTENT}", "").unwrap();
        assert_eq!(result, "Missing: ");
    }

    #[test]
    fn test_custom_variables() {
        let mut processor = TemplateProcessor::new();
        processor.set_custom_variable("project", "PromptHive");

        let result = processor.process("Working on {project}", "").unwrap();
        assert_eq!(result, "Working on PromptHive");
    }

    #[test]
    fn test_config_file() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("variables.conf");

        let mut processor = TemplateProcessor::new();
        processor.set_custom_variable("author", "John Doe");
        processor.set_custom_variable("project", "Test Project");

        processor.save_config(&config_path).unwrap();

        let mut new_processor = TemplateProcessor::new();
        new_processor.load_config(&config_path).unwrap();

        assert_eq!(
            new_processor.custom_variables.get("author").unwrap(),
            "John Doe"
        );
        assert_eq!(
            new_processor.custom_variables.get("project").unwrap(),
            "Test Project"
        );
    }

    #[test]
    fn test_legacy_behavior() {
        let processor = TemplateProcessor::new();

        // No placeholders should append input
        let result = processor.process("Simple prompt", "input data").unwrap();
        assert_eq!(result, "Simple prompt\n\ninput data");

        // With placeholders should not append
        let result = processor.process("Process {input}", "input data").unwrap();
        assert_eq!(result, "Process input data");
    }
}
