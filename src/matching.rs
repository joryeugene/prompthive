//! Fuzzy matching and search functionality for prompts
//!
//! This module provides sophisticated matching capabilities including exact matching,
//! fuzzy search with scoring, and enhanced search algorithms that consider multiple
//! factors for optimal results.

use colored::*;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

/// A prompt with associated metadata for matching and display
#[derive(Debug, Clone)]
pub struct Prompt {
    /// The primary name/identifier of the prompt
    pub name: String,
    /// Abbreviated code for quick access
    pub short_code: String,
    /// Human-readable description of the prompt's purpose
    pub description: String,
    /// Semantic version if versioned
    pub version: Option<String>,
    /// ISO 8601 creation timestamp
    pub created_at: Option<String>,
    /// ISO 8601 last update timestamp
    pub updated_at: Option<String>,
    /// Git commit hash if under version control
    pub git_hash: Option<String>,
}

/// Fuzzy matcher for finding prompts by name or description
///
/// Provides sophisticated matching algorithms that combine exact matching,
/// fuzzy matching, and enhanced scoring based on multiple criteria.
pub struct Matcher {
    prompts: Vec<Prompt>,
    fuzzy: SkimMatcherV2,
}

impl Matcher {
    /// Create a new matcher with the given list of prompts
    ///
    /// # Arguments
    ///
    /// * `prompts` - Vector of prompts to search through
    pub fn new(prompts: Vec<Prompt>) -> Self {
        Self {
            prompts,
            fuzzy: SkimMatcherV2::default(),
        }
    }

    /// Find the best matching prompt(s) for a given query
    ///
    /// Uses a tiered matching approach:
    /// 1. Exact name match
    /// 2. Exact short code match  
    /// 3. Fuzzy matching with enhanced scoring
    ///
    /// # Arguments
    ///
    /// * `query` - The search query string
    ///
    /// # Returns
    ///
    /// A `MatchResult` indicating the type and quality of match found
    pub fn find(&self, query: &str) -> MatchResult {
        // First try exact match
        if let Some(prompt) = self.prompts.iter().find(|p| p.name == query) {
            return MatchResult::Exact(prompt.clone());
        }

        // Then try short code match
        if let Some(prompt) = self.prompts.iter().find(|p| p.short_code == query) {
            return MatchResult::Exact(prompt.clone());
        }

        // Enhanced fuzzy matching with multiple scoring criteria
        let mut matches: Vec<_> = self
            .prompts
            .iter()
            .filter_map(|prompt| {
                // Calculate multiple scores for better ranking
                let fuzzy_score = self.fuzzy.fuzzy_match(&prompt.name, query)?;
                let enhanced_score =
                    self.calculate_enhanced_score(&prompt.name, query, fuzzy_score);
                Some((prompt, enhanced_score))
            })
            .collect();

        // Sort by enhanced score (highest first)
        matches.sort_by(|a, b| b.1.cmp(&a.1));

        // Deduplicate by name (keep highest scoring version)
        matches.dedup_by(|a, b| a.0.name == b.0.name);

        match matches.len() {
            0 => MatchResult::None,
            1 => MatchResult::Exact(matches[0].0.clone()),
            _ => {
                // Check if first match is significantly better (higher threshold for enhanced scoring)
                if matches.len() >= 2 && matches[0].1 > matches[1].1 + 1000 {
                    MatchResult::Exact(matches[0].0.clone())
                } else {
                    MatchResult::Multiple(
                        matches
                            .into_iter()
                            .map(|(p, _)| p.clone())
                            .take(8) // Show more suggestions with better ranking
                            .collect(),
                    )
                }
            }
        }
    }

    /// Calculate enhanced score for better fuzzy matching
    /// Combines fuzzy score with additional heuristics for relevance
    fn calculate_enhanced_score(&self, prompt_name: &str, query: &str, fuzzy_score: i64) -> i64 {
        let mut score = fuzzy_score;
        let prompt_lower = prompt_name.to_lowercase();
        let query_lower = query.to_lowercase();

        // Boost for exact prefix matches (very important)
        if prompt_lower.starts_with(&query_lower) {
            score += 2000;
        }

        // Boost for word boundary matches (important)
        if prompt_lower
            .split(['-', '/', '_', ' '])
            .any(|word| word.starts_with(&query_lower))
        {
            score += 1000;
        }

        // Boost for bank-specific queries (e.g., "ess/com" should prefer essentials/commit)
        if query_lower.contains('/') {
            let parts: Vec<&str> = query_lower.split('/').collect();
            if parts.len() == 2 {
                let bank_query = parts[0];
                let prompt_query = parts[1];
                if prompt_lower.starts_with(bank_query) && prompt_lower.contains(prompt_query) {
                    score += 1500;
                }
            }
        }

        // Boost for shorter names (prefer concise matches)
        if prompt_name.len() < 20 {
            score += 300;
        }

        // Boost for common/essential banks
        if prompt_lower.starts_with("essentials/") || prompt_lower.starts_with("10x/") {
            score += 200;
        }

        // Penalize very long names or complex paths
        if prompt_name.len() > 50 {
            score -= 200;
        }

        score
    }

    pub fn generate_short_code(name: &str, existing: &[String]) -> String {
        let words: Vec<&str> = name.split(['-', '_']).collect();

        // Try first letters
        let mut code = words
            .iter()
            .map(|w| w.chars().next().unwrap_or('x'))
            .collect::<String>();

        if !existing.contains(&code) {
            return code;
        }

        // Try first two letters of each word
        code = words
            .iter()
            .map(|w| w.chars().take(2).collect::<String>())
            .collect::<Vec<_>>()
            .join("");

        if !existing.contains(&code) {
            return code;
        }

        // Add numbers if needed
        for i in 1..=9 {
            let numbered = format!("{}{}", code, i);
            if !existing.contains(&numbered) {
                return numbered;
            }
        }

        // Fallback: use full name
        name.to_string()
    }
}

/// Result of a prompt search operation
#[derive(Debug)]
pub enum MatchResult {
    /// Single exact match found
    Exact(Prompt),
    /// Multiple fuzzy matches found, ranked by relevance
    Multiple(Vec<Prompt>),
    /// No matches found for the query
    None,
}

impl MatchResult {
    pub fn display(&self) {
        match self {
            MatchResult::Exact(_) => {}
            MatchResult::Multiple(prompts) => {
                eprintln!("{}: Multiple matches. Did you mean:", "Error".red());
                for prompt in prompts {
                    eprintln!(
                        "  {:<12} ({}) - {}",
                        prompt.name.bold(),
                        prompt.short_code.dimmed(),
                        prompt.description
                    );
                }
            }
            MatchResult::None => {
                eprintln!("{}: No matching prompt found", "Error".red());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        let prompts = vec![Prompt {
            name: "api".to_string(),
            short_code: "ap".to_string(),
            description: "REST API design".to_string(),
            version: None,
            created_at: None,
            updated_at: None,
            git_hash: None,
        }];

        let matcher = Matcher::new(prompts);
        match matcher.find("api") {
            MatchResult::Exact(p) => assert_eq!(p.name, "api"),
            _ => panic!("Expected exact match"),
        }
    }

    #[test]
    fn test_ambiguous_match() {
        let prompts = vec![
            Prompt {
                name: "auth".to_string(),
                short_code: "au".to_string(),
                description: "JWT authentication".to_string(),
                version: None,
                created_at: None,
                updated_at: None,
                git_hash: None,
            },
            Prompt {
                name: "auth-basic".to_string(),
                short_code: "aub".to_string(),
                description: "Basic auth".to_string(),
                version: None,
                created_at: None,
                updated_at: None,
                git_hash: None,
            },
        ];

        let matcher = Matcher::new(prompts);
        match matcher.find("a") {
            MatchResult::Multiple(matches) => assert_eq!(matches.len(), 2),
            _ => panic!("Expected multiple matches"),
        }
    }

    #[test]
    fn test_short_code_generation() {
        let existing = vec!["a".to_string(), "ap".to_string()];

        assert_eq!(Matcher::generate_short_code("api", &existing), "ap1");
        assert_eq!(Matcher::generate_short_code("test", &[]), "t");
        assert_eq!(Matcher::generate_short_code("auth-basic", &[]), "ab");
    }
}
