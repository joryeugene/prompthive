//! Storage layer for prompt management
//!
//! This module provides the core storage functionality for PromptHive, including
//! persistent storage of prompts with metadata, hierarchical organization through
//! banks (directories), and efficient retrieval operations.

use anyhow::{anyhow, Context, Result};
use dirs::home_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Metadata structure for prompts
///
/// Contains all metadata associated with a prompt, including versioning information,
/// timestamps, and organizational tags.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptMetadata {
    /// Unique identifier for the prompt
    pub id: String,
    /// Human-readable description of the prompt's purpose
    pub description: String,
    /// Optional tags for categorization and search
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// ISO 8601 timestamp when the prompt was created
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// ISO 8601 timestamp when the prompt was last updated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// Semantic version string (e.g., "1.2.0")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Git commit hash if prompt is under version control
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_hash: Option<String>,
    /// Previous version for tracking prompt evolution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_version: Option<String>,
}

/// Core storage system for prompts and metadata
///
/// Provides persistent storage for prompts with hierarchical organization
/// through banks (subdirectories) and comprehensive metadata management.
pub struct Storage {
    base_dir: PathBuf,
}

impl Clone for Storage {
    fn clone(&self) -> Self {
        Self {
            base_dir: self.base_dir.clone(),
        }
    }
}

impl Storage {
    /// Create a new Storage instance with default configuration
    ///
    /// Uses `~/.prompthive` as the base directory, or the value of the
    /// `PROMPTHIVE_BASE_DIR` environment variable if set.
    ///
    /// # Errors
    ///
    /// Returns an error if the home directory cannot be determined.
    pub fn new() -> Result<Self> {
        let base_dir = if let Ok(custom_dir) = std::env::var("PROMPTHIVE_BASE_DIR") {
            PathBuf::from(custom_dir)
        } else {
            home_dir()
                .context("Could not find home directory")?
                .join(".prompthive")
        };

        Ok(Self { base_dir })
    }

    /// Create a new Storage instance with a custom base directory
    ///
    /// # Arguments
    ///
    /// * `base_dir` - The base directory path for storing prompts
    pub fn new_with_base(base_dir: PathBuf) -> Result<Self> {
        Ok(Self { base_dir })
    }

    /// Get the base directory path for this storage instance
    pub fn base_dir(&self) -> &PathBuf {
        &self.base_dir
    }

    /// Initialize the storage directory structure
    ///
    /// Creates the necessary directories for storing prompts, metadata,
    /// and registry information.
    ///
    /// # Errors
    ///
    /// Returns an error if directory creation fails due to permissions
    /// or filesystem issues.
    pub fn init(&self) -> Result<()> {
        // Create directory structure
        fs::create_dir_all(&self.base_dir)?;
        fs::create_dir_all(self.registry_dir())?;

        // Create banks directory (unified storage location)
        let banks_dir = self.base_dir.join("banks");
        fs::create_dir_all(&banks_dir)?;

        // Create teams directory for team namespaces
        let teams_dir = self.base_dir.join("teams");
        fs::create_dir_all(&teams_dir)?;

        // Copy default banks if they don't exist
        self.copy_default_banks()?;

        // Create default config if it doesn't exist
        let config_path = self.config_path();
        if !config_path.exists() {
            let default_config = r#"[prompthive]
default_registry = "https://registry.prompthive.sh"

[preferences]
editor = "vim"
"#;
            fs::write(&config_path, default_config)?;
        }

        Ok(())
    }

    fn copy_default_banks(&self) -> Result<()> {
        // Create essentials bank
        let essentials_dir = self.base_dir.join("banks").join("essentials");
        if !essentials_dir.exists() {
            fs::create_dir_all(&essentials_dir)?;

            // Commit message prompt
            let commit_content = r#"---
id: commit
description: Generate conventional commit message from diff
---

Based on the git diff below, generate a concise commit message following conventional commit format.

Format: type(scope): description

Types: feat, fix, docs, style, refactor, test, chore

Keep under 50 characters, use imperative mood.

Git diff:
{input}"#;
            fs::write(essentials_dir.join("commit.md"), commit_content)?;

            // Debug prompt
            let debug_content = r#"---
id: debug
description: Analyze errors and suggest fixes
---

Analyze the provided error and:

1. **Identify the root cause** - What exactly is failing and why
2. **Explain in simple terms** - Make it understandable 
3. **Suggest immediate fixes** - Quick solutions to try
4. **Recommend best practices** - How to prevent this
5. **Provide code examples** - Show the fix, don't just describe

Error details:
{input}"#;
            fs::write(essentials_dir.join("debug.md"), debug_content)?;

            // Review prompt
            let review_content = r#"---
id: review
description: Comprehensive code review
---

Review the provided code focusing on:

## Critical Issues
- Security vulnerabilities
- Performance problems
- Logic errors

## Code Quality
- Readability and clarity
- Design patterns
- Error handling

## Suggestions
- Specific improvements with examples
- Alternative approaches

Code to review:
{input}"#;
            fs::write(essentials_dir.join("review.md"), review_content)?;
        }

        // Create 10x bank
        let tenx_dir = self.base_dir.join("banks").join("10x");
        if !tenx_dir.exists() {
            fs::create_dir_all(&tenx_dir)?;

            // AFK task prompt
            let afk_content = r#"---
id: afk-task
description: Long-running autonomous task for calmhive
---

You are an autonomous AI agent tasked with completing the following objective independently:

{input}

## Instructions:
1. Break down the task into concrete steps
2. Execute each step systematically
3. Handle errors gracefully and retry if needed
4. Document your progress and decisions
5. Complete the entire task without human intervention

## Success Criteria:
- All subtasks completed
- Output is functional and tested
- Code follows best practices
- Documentation is clear

Begin by analyzing the task and creating an execution plan."#;
            fs::write(tenx_dir.join("afk-task.md"), afk_content)?;

            // Fix tests prompt
            let fix_tests_content = r#"---
id: fix-tests
description: Systematically fix all failing tests
---

Analyze and fix the failing tests:

{input}

## Approach:
1. Identify why each test is failing
2. Determine if test or implementation needs fixing
3. Make minimal changes to fix
4. Ensure no regressions
5. Run tests to verify

## For each failure:
- Root cause
- Fix applied
- Verification steps

Fix tests one by one, showing your work."#;
            fs::write(tenx_dir.join("fix-tests.md"), fix_tests_content)?;

            // Refactor prompt
            let refactor_content = r#"---
id: refactor
description: Refactor code for clarity and performance
---

Refactor this code to improve:

{input}

## Goals:
1. **Clarity** - Make intent obvious
2. **Performance** - Optimize hot paths
3. **Maintainability** - Reduce complexity
4. **Testability** - Make it easy to test

## Constraints:
- Maintain all existing functionality
- Keep public API stable
- Add comments for complex logic

Provide the refactored code with explanations."#;
            fs::write(tenx_dir.join("refactor.md"), refactor_content)?;
        }

        Ok(())
    }

    pub fn prompts_dir(&self) -> PathBuf {
        self.base_dir.join("prompts")
    }

    pub fn registry_dir(&self) -> PathBuf {
        self.base_dir.join("registry")
    }

    pub fn config_path(&self) -> PathBuf {
        self.base_dir.join("config.toml")
    }

    pub fn teams_dir(&self) -> PathBuf {
        self.base_dir.join("teams")
    }

    pub fn team_dir(&self, team_name: &str) -> PathBuf {
        self.teams_dir().join(self.sanitize_filename(team_name))
    }

    pub fn prompt_path(&self, name: &str) -> PathBuf {
        // Check if this is a team prompt (starts with @)
        if let Some(without_at) = name.strip_prefix('@') {
            if let Some(slash_pos) = without_at.find('/') {
                let team_name = &without_at[..slash_pos];
                let prompt_name = &without_at[slash_pos + 1..];
                let team_dir = self.team_dir(team_name);
                let sanitized_prompt = self.sanitize_filename(prompt_name);
                return team_dir.join(format!("{}.md", sanitized_prompt));
            }
        }

        // Check if this is a bank prompt (contains /)
        if name.contains('/') {
            let parts: Vec<&str> = name.split('/').collect();
            if parts.len() >= 2 {
                // Handle bank/prompt or bank/subdir/prompt
                let bank = self.sanitize_filename(parts[0]);
                let mut path = self.base_dir.join("banks").join(bank);

                // Add any subdirectories
                for part in parts.iter().take(parts.len() - 1).skip(1) {
                    path = path.join(self.sanitize_filename(part));
                }

                // Add the prompt file
                let prompt = self.sanitize_filename(parts[parts.len() - 1]);
                return path.join(format!("{}.md", prompt));
            }
        }

        // Regular prompt path
        let sanitized = self.sanitize_filename(name);
        self.prompts_dir().join(format!("{}.md", sanitized))
    }

    fn sanitize_filename(&self, name: &str) -> String {
        // Remove path separators and dangerous characters
        name.chars()
            .filter(|c| {
                c.is_alphanumeric() ||
                *c == '-' || *c == '_' || *c == '.' ||
                (*c >= '\u{4e00}' && *c <= '\u{9fff}') || // Chinese characters
                (*c >= '\u{3040}' && *c <= '\u{309f}') || // Hiragana
                (*c >= '\u{30a0}' && *c <= '\u{30ff}') || // Katakana
                (*c >= '\u{0080}' && *c <= '\u{024f}') // Extended Latin
            })
            .collect::<String>()
            .chars()
            .take(64) // Limit filename length
            .collect()
    }

    pub fn list_prompts(&self) -> Result<Vec<String>> {
        let mut prompts = Vec::new();

        // List regular prompts
        if let Ok(entries) = fs::read_dir(self.prompts_dir()) {
            for entry in entries {
                let entry = entry?;
                let path = entry.path();

                if path.extension().and_then(|s| s.to_str()) == Some("md") {
                    if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                        prompts.push(name.to_string());
                    }
                }
            }
        }

        // List bank prompts
        let banks_dir = self.base_dir.join("banks");
        if banks_dir.exists() {
            if let Ok(bank_entries) = fs::read_dir(&banks_dir) {
                for bank_entry in bank_entries {
                    let bank_entry = bank_entry?;
                    let bank_path = bank_entry.path();

                    if bank_path.is_dir() {
                        if let Some(bank_name) = bank_path.file_name().and_then(|s| s.to_str()) {
                            // Use recursive function to find all prompts in subdirectories
                            let _ = self.list_prompts_recursive(
                                &bank_path,
                                bank_name,
                                "",
                                &mut prompts,
                            );
                        }
                    }
                }
            }
        }

        // List team prompts
        let teams_dir = self.teams_dir();
        if teams_dir.exists() {
            if let Ok(team_entries) = fs::read_dir(&teams_dir) {
                for team_entry in team_entries {
                    let team_entry = team_entry?;
                    let team_path = team_entry.path();

                    if team_path.is_dir() {
                        if let Some(team_name) = team_path.file_name().and_then(|s| s.to_str()) {
                            if let Ok(team_prompts) = self.list_team_prompts(team_name) {
                                prompts.extend(team_prompts);
                            }
                        }
                    }
                }
            }
        }

        prompts.sort();
        Ok(prompts)
    }

    /// Recursively list prompts in a directory, flattening subdirectories
    #[allow(clippy::only_used_in_recursion)]
    fn list_prompts_recursive(
        &self,
        dir_path: &std::path::Path,
        bank_name: &str,
        relative_path: &str,
        prompts: &mut Vec<String>,
    ) -> Result<()> {
        if let Ok(entries) = fs::read_dir(dir_path) {
            for entry in entries {
                let entry = entry?;
                let path = entry.path();

                if path.is_dir() {
                    // Recursively process subdirectory
                    if let Some(subdir_name) = path.file_name().and_then(|s| s.to_str()) {
                        // Skip hidden directories and known non-prompt directories
                        if subdir_name.starts_with('.') {
                            continue;
                        }
                        let new_relative_path = if relative_path.is_empty() {
                            subdir_name.to_string()
                        } else {
                            format!("{}/{}", relative_path, subdir_name)
                        };
                        let _ = self.list_prompts_recursive(
                            &path,
                            bank_name,
                            &new_relative_path,
                            prompts,
                        );
                    }
                } else if path.extension().and_then(|s| s.to_str()) == Some("md") {
                    if let Some(prompt_name) = path.file_stem().and_then(|s| s.to_str()) {
                        // Skip README files (but only if it's actually named README)
                        if prompt_name.eq_ignore_ascii_case("readme") {
                            continue;
                        }

                        // Create flattened name
                        let full_name = if relative_path.is_empty() {
                            format!("{}/{}", bank_name, prompt_name)
                        } else {
                            format!("{}/{}/{}", bank_name, relative_path, prompt_name)
                        };
                        prompts.push(full_name);
                    }
                }
            }
        }
        Ok(())
    }

    pub fn list_bank_prompts(&self, bank: &str) -> Result<Vec<String>> {
        let mut prompts = Vec::new();
        let bank_dir = self.base_dir.join("banks").join(bank);

        if bank_dir.exists() {
            if let Ok(entries) = fs::read_dir(&bank_dir) {
                for entry in entries {
                    let entry = entry?;
                    let path = entry.path();

                    if path.extension().and_then(|s| s.to_str()) == Some("md") {
                        if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                            prompts.push(format!("{}/{}", bank, name));
                        }
                    }
                }
            }
        }

        prompts.sort();
        Ok(prompts)
    }

    pub fn prompt_exists(&self, name: &str) -> bool {
        self.prompt_path(name).exists()
    }

    pub fn resolve_prompt(&self, query: &str) -> Result<String> {
        use fuzzy_matcher::skim::SkimMatcherV2;
        use fuzzy_matcher::FuzzyMatcher;

        // Check if query contains bank syntax (bank/prompt)
        if query.contains('/') {
            let parts: Vec<&str> = query.splitn(2, '/').collect();
            if parts.len() == 2 {
                let bank = parts[0];
                let prompt = parts[1];

                // First try exact match
                let bank_prompt = format!("{}/{}", bank, prompt);
                if self.prompt_exists(&bank_prompt) {
                    return Ok(bank_prompt);
                }

                // Then try fuzzy matching within the bank
                let bank_prompts = self.list_bank_prompts(bank)?;
                if !bank_prompts.is_empty() {
                    let fuzzy = SkimMatcherV2::default();
                    let mut best_match = None;
                    let mut best_score = 0;

                    for bank_prompt_name in &bank_prompts {
                        // Extract just the prompt name part for matching
                        let prompt_part = bank_prompt_name
                            .split('/')
                            .next_back()
                            .unwrap_or(bank_prompt_name);
                        if let Some(score) = fuzzy.fuzzy_match(prompt_part, prompt) {
                            if score > best_score {
                                best_score = score;
                                best_match = Some(bank_prompt_name.clone());
                            }
                        }
                    }

                    if let Some(matched) = best_match {
                        return Ok(matched);
                    }
                }

                return Err(anyhow::anyhow!(
                    "No prompt found matching '{}/{}'",
                    bank,
                    prompt
                ));
            }
        }

        // Try exact match first
        if self.prompt_exists(query) {
            return Ok(query.to_string());
        }

        // Fuzzy match across all prompts
        let all_prompts = self.list_prompts()?;
        let fuzzy = SkimMatcherV2::default();
        let mut best_match = None;
        let mut best_score = 0;

        for prompt_name in &all_prompts {
            if let Some(score) = fuzzy.fuzzy_match(prompt_name, query) {
                if score > best_score {
                    best_score = score;
                    best_match = Some(prompt_name.clone());
                }
            }
        }

        if let Some(matched) = best_match {
            Ok(matched)
        } else {
            Err(anyhow::anyhow!("No prompt found matching '{}'", query))
        }
    }

    pub fn read_prompt(&self, name: &str) -> Result<(PromptMetadata, String)> {
        let path = self.prompt_path(name);
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Could not read prompt '{}'", name))?;

        // Parse frontmatter
        let (metadata, body) = self.parse_prompt(&content)?;

        Ok((metadata, body))
    }

    /// Fast prompt resolution that avoids loading all prompts
    pub fn resolve_prompt_fast(&self, query: &str) -> Result<String> {
        // Try exact match first - this is the fastest path
        if self.prompt_exists(query) {
            return Ok(query.to_string());
        }

        // If query contains bank syntax, try exact bank/prompt match
        if query.contains('/') {
            let parts: Vec<&str> = query.splitn(2, '/').collect();
            if parts.len() == 2 {
                let bank = parts[0];
                let prompt = parts[1];

                // Try exact match with bank
                let bank_prompt = format!("{}/{}", bank, prompt);
                if self.prompt_exists(&bank_prompt) {
                    return Ok(bank_prompt);
                }

                // Only scan the specific bank directory for fuzzy matching
                let bank_dir = self.base_dir.join("banks").join(bank);
                if bank_dir.exists() {
                    let prompts = self.list_bank_prompts(bank)?;
                    if let Some(matched) = self.fuzzy_match_in_list(&prompts, prompt) {
                        return Ok(matched);
                    }
                }

                return Err(anyhow::anyhow!(
                    "No prompt found matching '{}/{}'",
                    bank,
                    prompt
                ));
            }
        }

        // For fuzzy matching, we need to fall back to listing all prompts
        // This is slower but necessary for non-exact matches
        self.resolve_prompt(query)
    }

    /// Helper for fuzzy matching within a specific list
    fn fuzzy_match_in_list(&self, prompts: &[String], query: &str) -> Option<String> {
        use fuzzy_matcher::skim::SkimMatcherV2;
        use fuzzy_matcher::FuzzyMatcher;

        let fuzzy = SkimMatcherV2::default();
        let mut best_match = None;
        let mut best_score = 0;

        for prompt_name in prompts {
            let prompt_part = prompt_name.split('/').next_back().unwrap_or(prompt_name);
            if let Some(score) = fuzzy.fuzzy_match(prompt_part, query) {
                if score > best_score {
                    best_score = score;
                    best_match = Some(prompt_name.clone());
                }
            }
        }

        best_match
    }

    pub fn write_prompt(&self, name: &str, metadata: &PromptMetadata, body: &str) -> Result<()> {
        let path = self.prompt_path(name);

        // Ensure the parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }

        // Serialize metadata as proper YAML to handle special characters
        let yaml_metadata =
            serde_yaml::to_string(metadata).context("Failed to serialize prompt metadata")?;

        // Format as markdown with frontmatter
        let content = format!("---\n{}---\n\n{}", yaml_metadata, body);

        fs::write(&path, content)?;
        Ok(())
    }

    pub fn parse_prompt_content(&self, content: &str) -> Result<(PromptMetadata, String)> {
        self.parse_prompt(content)
    }

    pub fn delete_prompt(&self, name: &str) -> Result<()> {
        let path = self.prompt_path(name);
        if !path.exists() {
            return Err(anyhow!("Prompt '{}' does not exist", name));
        }
        fs::remove_file(path)?;
        Ok(())
    }

    pub fn delete_bank(&self, bank_name: &str) -> Result<()> {
        let bank_path = self.base_dir.join("banks").join(bank_name);
        if !bank_path.exists() {
            return Err(anyhow!("Bank '{}' does not exist", bank_name));
        }

        if !bank_path.is_dir() {
            return Err(anyhow!("'{}' is not a directory", bank_name));
        }

        // Check if bank is empty
        let entries = fs::read_dir(&bank_path)?;
        let count = entries.count();

        if count > 0 {
            return Err(anyhow!(
                "Bank '{}' is not empty. Delete all prompts first.",
                bank_name
            ));
        }

        fs::remove_dir(bank_path)?;
        Ok(())
    }

    pub fn rename_bank(&self, old_name: &str, new_name: &str) -> Result<()> {
        let old_path = self.base_dir.join("banks").join(old_name);
        let new_path = self.base_dir.join("banks").join(new_name);

        if !old_path.exists() {
            return Err(anyhow!("Bank '{}' does not exist", old_name));
        }

        if new_path.exists() {
            return Err(anyhow!("Bank '{}' already exists", new_name));
        }

        fs::rename(old_path, new_path)?;
        Ok(())
    }

    fn parse_prompt(&self, content: &str) -> Result<(PromptMetadata, String)> {
        let lines: Vec<&str> = content.lines().collect();

        // Find frontmatter boundaries
        let start = lines.iter().position(|&line| line == "---");
        let end = lines.iter().skip(1).position(|&line| line == "---");

        if let (Some(0), Some(end_idx)) = (start, end) {
            // Parse YAML frontmatter
            let yaml_content = lines[1..=end_idx].join("\n");
            let metadata: PromptMetadata =
                serde_yaml::from_str(&yaml_content).context("Failed to parse prompt metadata")?;

            // Get body (everything after second ---)
            let body = lines[(end_idx + 2)..].join("\n").trim().to_string();

            Ok((metadata, body))
        } else {
            // No frontmatter, create default
            let metadata = PromptMetadata {
                id: "unknown".to_string(),
                description: "No description".to_string(),
                tags: None,
                created_at: None,
                updated_at: None,
                version: None,
                git_hash: None,
                parent_version: None,
            };

            Ok((metadata, content.to_string()))
        }
    }

    pub fn read_prompt_metadata(&self, name: &str) -> Result<PromptMetadata> {
        let prompt_path = self.prompt_path(name);
        let content = fs::read_to_string(&prompt_path)
            .with_context(|| format!("Failed to read prompt: {}", name))?;

        let (metadata, _) = self.parse_prompt(&content)?;
        Ok(metadata)
    }

    pub fn write_prompt_metadata(&self, name: &str, metadata: &PromptMetadata) -> Result<()> {
        let prompt_path = self.prompt_path(name);
        let content = fs::read_to_string(&prompt_path)
            .with_context(|| format!("Failed to read prompt: {}", name))?;

        let (_, body) = self.parse_prompt(&content)?;

        // Reconstruct the file with updated metadata
        let new_content = format!("---\n{}---\n\n{}", serde_yaml::to_string(metadata)?, body);

        fs::write(&prompt_path, new_content)
            .with_context(|| format!("Failed to write prompt: {}", name))?;

        Ok(())
    }

    // Team namespace methods
    pub fn create_team_namespace(&self, team_name: &str) -> Result<()> {
        let team_dir = self.team_dir(team_name);
        fs::create_dir_all(&team_dir)?;
        Ok(())
    }

    pub fn delete_team_namespace(&self, team_name: &str) -> Result<()> {
        let team_dir = self.team_dir(team_name);
        if !team_dir.exists() {
            return Err(anyhow!("Team '{}' does not exist", team_name));
        }

        // Check if team directory is empty
        let entries = fs::read_dir(&team_dir)?;
        let count = entries.count();

        if count > 0 {
            return Err(anyhow!(
                "Team '{}' has prompts. Remove all prompts first.",
                team_name
            ));
        }

        fs::remove_dir(team_dir)?;
        Ok(())
    }

    pub fn list_teams(&self) -> Result<Vec<String>> {
        let mut teams = Vec::new();

        if let Ok(entries) = fs::read_dir(self.teams_dir()) {
            for entry in entries {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    if let Some(name) = path.file_name() {
                        if let Some(name_str) = name.to_str() {
                            teams.push(name_str.to_string());
                        }
                    }
                }
            }
        }

        teams.sort();
        Ok(teams)
    }

    pub fn list_team_prompts(&self, team_name: &str) -> Result<Vec<String>> {
        let mut prompts = Vec::new();
        let team_dir = self.team_dir(team_name);

        if !team_dir.exists() {
            return Ok(prompts); // Return empty list if team doesn't exist
        }

        if let Ok(entries) = fs::read_dir(&team_dir) {
            for entry in entries {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|ext| ext == "md") {
                    if let Some(stem) = path.file_stem() {
                        if let Some(name) = stem.to_str() {
                            prompts.push(format!("@{}/{}", team_name, name));
                        }
                    }
                }
            }
        }

        prompts.sort();
        Ok(prompts)
    }

    pub fn write_team_prompt(
        &self,
        team_name: &str,
        prompt_name: &str,
        metadata: &PromptMetadata,
        content: &str,
    ) -> Result<()> {
        // Ensure team namespace exists
        self.create_team_namespace(team_name)?;

        // Write the prompt using the full team prompt path
        let full_name = format!("@{}/{}", team_name, prompt_name);
        self.write_prompt(&full_name, metadata, content)
    }

    pub fn read_team_prompt(
        &self,
        team_name: &str,
        prompt_name: &str,
    ) -> Result<(PromptMetadata, String)> {
        let full_name = format!("@{}/{}", team_name, prompt_name);
        self.read_prompt(&full_name)
    }

    pub fn delete_team_prompt(&self, team_name: &str, prompt_name: &str) -> Result<()> {
        let full_name = format!("@{}/{}", team_name, prompt_name);
        self.delete_prompt(&full_name)
    }

    pub fn team_prompt_exists(&self, team_name: &str, prompt_name: &str) -> bool {
        let full_name = format!("@{}/{}", team_name, prompt_name);
        self.prompt_exists(&full_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_prompt_with_frontmatter() {
        let content = r#"---
id: api
description: REST API design
---

Design a REST API with these requirements:
- Resource: {resource}
- Operations: {operations}"#;

        let storage = Storage::new().unwrap();
        let (metadata, body) = storage.parse_prompt(content).unwrap();

        assert_eq!(metadata.id, "api");
        assert_eq!(metadata.description, "REST API design");
        assert!(body.contains("Design a REST API"));
    }
}
