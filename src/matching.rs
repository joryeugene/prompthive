//! Ranked discovery is separate from stable execution identity.
use crate::Storage;
use anyhow::Result;
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
#[derive(Debug, Clone)]
pub struct Prompt {
    pub name: String,
    pub short_code: String,
    pub description: String,
    pub version: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub git_hash: Option<String>,
}
pub fn ranked_prompts(query: &str, prompts: &[Prompt]) -> Vec<Prompt> {
    let fuzzy = SkimMatcherV2::default();
    let mut ranked: Vec<_> = prompts
        .iter()
        .filter_map(|p| {
            let score = if query.is_empty() {
                0
            } else {
                let name = fuzzy.fuzzy_match(&p.name, query).map(|s| s + 100);
                let description = fuzzy.fuzzy_match(&p.description, query);
                name.into_iter().chain(description).max()?
            };
            Some((p.clone(), score))
        })
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.name.cmp(&b.0.name)));
    ranked.into_iter().map(|(p, _)| p).collect()
}
pub fn ranked_suggestions(query: &str, names: &[String]) -> Vec<String> {
    let prompts: Vec<_> = names
        .iter()
        .map(|name| Prompt {
            name: name.clone(),
            short_code: String::new(),
            description: String::new(),
            version: None,
            created_at: None,
            updated_at: None,
            git_hash: None,
        })
        .collect();
    ranked_prompts(query, &prompts)
        .into_iter()
        .map(|p| p.name)
        .collect()
}
pub struct Matcher {
    prompts: Vec<Prompt>,
}
#[derive(Debug)]
pub enum MatchResult {
    Exact(Prompt),
    Multiple(Vec<Prompt>),
    None,
}
impl Matcher {
    pub fn new(prompts: Vec<Prompt>) -> Self {
        Self { prompts }
    }
    pub fn find(&self, query: &str) -> MatchResult {
        if let Some(p) = self.prompts.iter().find(|p| p.name == query) {
            return MatchResult::Exact(p.clone());
        }
        let matches = ranked_prompts(query, &self.prompts);
        if matches.is_empty() {
            MatchResult::None
        } else {
            MatchResult::Multiple(matches)
        }
    }
    /// Historical algorithm, used only to preview migration; never used to execute.
    pub fn generate_short_code(name: &str, existing: &[String]) -> String {
        let words: Vec<_> = name.split(['-', '_']).collect();
        let first: String = words
            .iter()
            .map(|w| w.chars().next().unwrap_or('x'))
            .collect();
        if !existing.contains(&first) {
            return first;
        }
        let second: String = words.iter().flat_map(|w| w.chars().take(2)).collect();
        if !existing.contains(&second) {
            return second;
        }
        for i in 1..=9 {
            let code = format!("{second}{i}");
            if !existing.contains(&code) {
                return code;
            }
        }
        name.into()
    }
}
pub fn legacy_code_report(storage: &Storage) -> Result<Vec<String>> {
    let names = storage.list_prompts()?;
    let fuzzy = SkimMatcherV2::default();
    let mut codes = Vec::new();
    let mut report = Vec::new();
    for name in &names {
        if storage.read_prompt(name).is_err() {
            continue;
        }
        let code = Matcher::generate_short_code(name, &codes);
        codes.push(code.clone());
        let mut best = None;
        let mut score = 0;
        for candidate in &names {
            if let Some(s) = fuzzy.fuzzy_match(candidate, &code) {
                if s > score {
                    score = s;
                    best = Some(candidate.as_str());
                }
            }
        }
        let old_use = if names.contains(&code) {
            code.as_str()
        } else {
            best.unwrap_or(name)
        };
        let old_show = if names.contains(&code) {
            code.as_str()
        } else {
            name.as_str()
        };
        report.push(format!("legacy {code}: use={old_use}; show={old_show}; compose={}; proposed exact name={name}; verify any binding manually",best.unwrap_or("unresolved")));
    }
    Ok(report)
}
