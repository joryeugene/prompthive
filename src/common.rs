// Common utility functions used across commands

use crate::storage::Storage;
use crate::{MatchResult, Matcher, Prompt};
use anyhow::{Context, Result};
use colored::Colorize;
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use std::fs;
use std::path::PathBuf;

/// Get path to config file
pub fn get_config_path() -> Result<PathBuf> {
    let storage = Storage::new()?;
    Ok(storage.config_path())
}

/// Load configuration from file
pub fn load_config() -> Result<toml::Value> {
    let config_path = get_config_path()?;

    if !config_path.exists() {
        return Ok(toml::Value::Table(Default::default()));
    }

    let content = fs::read_to_string(&config_path).context("Failed to read config file")?;

    toml::from_str(&content).context("Failed to parse config file")
}

/// Save configuration to file
pub fn save_config(config: &toml::Value) -> Result<()> {
    let config_path = get_config_path()?;
    let content = toml::to_string_pretty(config).context("Failed to serialize config")?;

    fs::write(&config_path, content).context("Failed to write config file")?;

    // Set secure permissions (only readable by owner)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&config_path)?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&config_path, perms)?;
    }

    Ok(())
}

/// Load API key from config file
pub fn load_api_key() -> Result<Option<String>> {
    let config = load_config()?;

    if let toml::Value::Table(table) = config {
        if let Some(toml::Value::String(api_key)) = table.get("api_key") {
            return Ok(Some(api_key.clone()));
        }
    }

    Ok(None)
}

/// Store API key in config file
pub fn store_api_key(api_key: &str) -> Result<()> {
    let mut config = load_config().unwrap_or_else(|_| toml::Value::Table(Default::default()));

    if let toml::Value::Table(ref mut table) = config {
        table.insert(
            "api_key".to_string(),
            toml::Value::String(api_key.to_string()),
        );
    }

    save_config(&config)
}

/// Remove API key from config file
pub fn remove_api_key() -> Result<()> {
    let mut config = load_config().unwrap_or_else(|_| toml::Value::Table(Default::default()));

    if let toml::Value::Table(ref mut table) = config {
        table.remove("api_key");
    }

    save_config(&config)
}

/// Resolve a prompt name using fuzzy matching
pub fn resolve_prompt_name(storage: &Storage, query: &str) -> Result<String> {
    // Check if query contains bank syntax (bank/prompt)
    if query.contains('/') {
        let parts: Vec<&str> = query.splitn(2, '/').collect();
        if parts.len() == 2 {
            let bank = parts[0];
            let prompt = parts[1];

            // First try exact match
            let bank_prompt = format!("{}/{}", bank, prompt);
            if storage.prompt_exists(&bank_prompt) {
                return Ok(bank_prompt);
            }

            // Then try fuzzy matching within the bank
            let bank_prompts = storage.list_bank_prompts(bank)?;
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
        }
    }

    // Regular prompt resolution (no bank specified)
    let prompt_names = storage.list_prompts()?;
    let mut prompts = Vec::new();

    for prompt_name in &prompt_names {
        if let Ok((metadata, _)) = storage.read_prompt(prompt_name) {
            prompts.push(Prompt {
                name: prompt_name.clone(),
                short_code: Matcher::generate_short_code(
                    prompt_name,
                    &prompts
                        .iter()
                        .map(|p: &Prompt| p.short_code.clone())
                        .collect::<Vec<_>>(),
                ),
                description: metadata.description,
                version: metadata.version,
                created_at: metadata.created_at,
                updated_at: metadata.updated_at,
                git_hash: metadata.git_hash,
            });
        }
    }

    let matcher = Matcher::new(prompts);
    match matcher.find(query) {
        MatchResult::Exact(prompt) => Ok(prompt.name),
        MatchResult::Multiple(suggestions) => {
            eprintln!("Error: Multiple matches. Did you mean:");
            for prompt in suggestions {
                eprintln!(
                    "  {:<12} ({}) - {}",
                    prompt.name.bold(),
                    prompt.short_code.dimmed(),
                    prompt.description
                );
            }
            std::process::exit(1);
        }
        MatchResult::None => {
            eprintln!("Error: No prompt found matching '{}'", query);
            std::process::exit(1);
        }
    }
}
