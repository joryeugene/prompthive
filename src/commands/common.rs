use anyhow::Result;
use colored::*;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

use super::configuration::load_api_key;
use crate::{MatchResult, Matcher, Prompt, Storage};

/// Resolve prompt name using fuzzy matching, supports bank/prompt syntax
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

/// Check if user has valid API key for Pro features
pub fn require_api_key(feature_name: &str) -> Result<String> {
    match load_api_key() {
        Ok(Some(key)) => Ok(key),
        Ok(None) => {
            eprintln!(
                "{}: {} requires authentication. Run {} first.",
                "Error".red(),
                feature_name,
                "ph login".bold()
            );
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("{}: Failed to load API key: {}", "Error".red(), e);
            std::process::exit(1);
        }
    }
}
