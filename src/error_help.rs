//! Error helper module for user-friendly error messages
//! 
//! This module provides helpful error messages with suggestions and examples
//! to guide users when they encounter issues.

use colored::*;
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};

/// Format a "prompt not found" error with helpful suggestions
pub fn format_prompt_not_found(prompt_name: &str, available_prompts: &[String]) -> String {
    let mut message = format!("Error: Prompt '{}' not found\n\n", prompt_name).red().to_string();
    
    // Find similar prompts using fuzzy matching
    let matcher = SkimMatcherV2::default();
    let mut suggestions: Vec<(&String, i64)> = available_prompts
        .iter()
        .filter_map(|p| {
            matcher.fuzzy_match(p, prompt_name)
                .map(|score| (p, score))
        })
        .collect();
    
    suggestions.sort_by(|a, b| b.1.cmp(&a.1));
    
    if !suggestions.is_empty() {
        message.push_str(&"Did you mean one of these?\n".yellow().to_string());
        for (prompt, _) in suggestions.iter().take(3) {
            message.push_str(&format!("  - {}\n", prompt));
        }
        message.push('\n');
    }
    
    message.push_str(&format!(
        "Try '{}' to see all available prompts or '{}' to search.\n",
        "ph ls".cyan(),
        format!("ph find {}", prompt_name).cyan()
    ));
    
    message
}

/// Format a command typo error with suggestions
pub fn format_command_typo(command: &str, available_commands: &[&str]) -> String {
    let mut message = format!("Error: Unknown command '{}'\n\n", command).red().to_string();
    
    // Special handling for common beginner mistakes
    if command == "ph" {
        message.push_str(&format!("{}\n", "Did you type 'ph ph' instead of just 'ph'? Try removing the extra 'ph'.".yellow()));
        message.push_str(&format!("{}\n\n", "Example: 'ph use essentials/commit' (not 'ph ph use commit')".cyan()));
    } else {
        // Find similar commands
        let matcher = SkimMatcherV2::default();
        let mut suggestions: Vec<(&str, i64)> = available_commands
            .iter()
            .filter_map(|&cmd| {
                matcher.fuzzy_match(cmd, command)
                    .map(|score| (cmd, score))
            })
            .collect();
        
        suggestions.sort_by(|a, b| b.1.cmp(&a.1));
        
        if !suggestions.is_empty() && suggestions[0].1 > 50 {
            message.push_str(&format!("Did you mean '{}'?\n\n", suggestions[0].0).yellow().to_string());
        }
    }
    
    message.push_str("Available commands:\n");
    for cmd in available_commands {
        message.push_str(&format!("  {}\n", cmd));
    }
    
    message.push_str(&format!("\nRun '{}' for more information\n", "ph --help".cyan()));
    
    message
}

/// Format a permission denied error with actionable steps
pub fn format_permission_error(path: &str, operation: &str) -> String {
    let mut message = format!("Error: Permission denied: cannot {} '{}'\n\n", operation, path).red().to_string();
    
    message.push_str("To fix this:\n");
    message.push_str(&format!("  - Check file permissions: {}\n", format!("ls -la {}", path).cyan()));
    message.push_str(&format!("  - Make writable: {}\n", format!("chmod +w {}", path).cyan()));
    
    #[cfg(unix)]
    message.push_str(&format!("  - Or run with sudo if needed: {}\n", "sudo ph ...".cyan()));
    
    message
}

/// Format an invalid file path error
pub fn format_file_not_found(path: &str) -> String {
    let mut message = format!("Error: File not found: '{}'\n\n", path).red().to_string();
    
    message.push_str("Please check:\n");
    message.push_str(&format!("  - Current directory: {}\n", "pwd".cyan()));
    message.push_str(&format!("  - List files: {}\n", "ls -la".cyan()));
    message.push_str("  - Ensure the path is correct and the file exists\n");
    
    message
}

/// Format a syntax error in prompt files
pub fn format_prompt_syntax_error(file: &str, error: &str) -> String {
    let mut message = format!("Error: Invalid prompt syntax in '{}'\n\n", file).red().to_string();
    
    message.push_str(&format!("The YAML frontmatter has a syntax error:\n{}\n\n", error));
    
    message.push_str("Correct format:\n");
    message.push_str(&"---\n".dimmed().to_string());
    message.push_str(&"id: my-prompt\n".green().to_string());
    message.push_str(&"description: What this prompt does\n".green().to_string());
    message.push_str(&"tags: [optional, tags]\n".green().dimmed().to_string());
    message.push_str(&"---\n".dimmed().to_string());
    message.push_str(&"\nYour prompt content here\n".to_string());
    
    message
}

/// Format a network error with troubleshooting steps
pub fn format_network_error(error: &str) -> String {
    let mut message = format!("Error: Network connection failed\n{}\n\n", error).red().to_string();
    
    message.push_str("Please check:\n");
    message.push_str("  - Your internet connection\n");
    message.push_str(&format!("  - Try offline mode: {}\n", "PROMPTHIVE_OFFLINE=1 ph ...".cyan()));
    message.push_str("  - Retry the command in a few moments\n");
    
    message
}

/// Format a disk space error
pub fn format_disk_space_error() -> String {
    let mut message = "Error: Not enough disk space\n\n".red().to_string();
    
    message.push_str("To fix this:\n");
    message.push_str(&format!("  - Check disk usage: {}\n", "df -h".cyan()));
    message.push_str("  - Free up some space\n");
    message.push_str("  - Try a different location\n");
    
    message
}