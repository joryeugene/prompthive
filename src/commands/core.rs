// Core prompt management commands - extracted from main.rs

use crate::PromptMetadata;
use crate::{
    clean, edit, HistoryEntry, HistoryTracker, IoOptions, MatchResult, Matcher, Prompt, Storage,
};
#[cfg(feature = "compose")]
use crate::{parse_prompt_list, Composer};
use anyhow::{Context, Result};
use chrono;
use colored::*;
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use is_terminal::IsTerminal;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

// Core commands like use, new, edit, show, delete, ls, find, rename

#[allow(dead_code)]
pub fn handle_use(
    storage: &Storage,
    name: &str,
    input: Option<&str>,
    edit: bool,
    io_options: &IoOptions,
    with_directives: Option<&str>,
    start: Instant,
) -> Result<()> {
    let history_tracker = HistoryTracker::new(storage.base_dir().to_path_buf());
    let command = format!("use {}", name);
    let input_text = input.unwrap_or("").to_string();
    use std::io::Read;

    // Use fast resolution path that avoids loading all prompts
    let resolved_name = match storage.resolve_prompt_fast(name) {
        Ok(name) => name,
        Err(_) => {
            // Fall back to full fuzzy matching only if fast path fails
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

            // Find matching prompt
            let matcher = Matcher::new(prompts);
            match matcher.find(name) {
                MatchResult::Exact(prompt) => prompt.name,
                _ => {
                    // Record failed history
                    let entry = HistoryEntry::new(
                        command,
                        &input_text,
                        "",
                        false,
                        start.elapsed().as_millis(),
                    );
                    let _ = history_tracker.record(entry);
                    
                    // Use the error helper for better error messages
                    use crate::error_help;
                    let available_prompts = storage.list_prompts().unwrap_or_default();
                    eprintln!("{}", error_help::format_prompt_not_found(name, &available_prompts));
                    std::process::exit(1);
                }
            }
        }
    };

    // Read the prompt content
    let (_, mut body) = storage.read_prompt(&resolved_name)?;

    // Handle --with directive files
    if let Some(directives) = with_directives {
        let mut directive_content = String::new();
        let directive_files: Vec<&str> = directives.split(',').map(|s| s.trim()).collect();

        for directive_file in directive_files {
            // Check if it's a file path or a prompt name
            if directive_file.starts_with('~')
                || directive_file.starts_with('/')
                || directive_file.contains('.')
            {
                // It's a file path - expand and read
                let expanded_path = shellexpand::tilde(directive_file);
                let path = PathBuf::from(expanded_path.as_ref());

                if path.exists() {
                    match fs::read_to_string(&path) {
                        Ok(content) => {
                            directive_content.push_str(&content);
                            directive_content.push_str("\n\n");
                        }
                        Err(e) => {
                            eprintln!(
                                "Warning: Failed to read directive file '{}': {}",
                                directive_file, e
                            );
                        }
                    }
                } else {
                    eprintln!("Warning: Directive file '{}' not found", directive_file);
                }
            } else {
                // It's a prompt name - try to read from storage
                match storage.read_prompt(directive_file) {
                    Ok((_, content)) => {
                        directive_content.push_str(&content);
                        directive_content.push_str("\n\n");
                    }
                    Err(_) => {
                        eprintln!("Warning: Prompt '{}' not found", directive_file);
                    }
                }
            }
        }

        // Prepend directive content to the body
        if !directive_content.is_empty() {
            body = format!("{}{}", directive_content, body);
        }
    }

    // Get input from argument or stdin
    let input_content = if let Some(provided_input) = input {
        provided_input.to_string()
    } else if !std::io::stdin().is_terminal() {
        let mut stdin_content = String::new();
        std::io::stdin().read_to_string(&mut stdin_content)?;
        stdin_content.trim().to_string()
    } else {
        String::new()
    };

    // Process template variables and input
    let mut composer = Composer::new(storage.clone());
    body = composer
        .template_processor()
        .process(&body, &input_content)?;

    // Handle --edit flag
    if edit {
        body = edit::edit_content(&body)?;
    }

    // Apply unified I/O using IoOptions with prompt name for sync
    io_options.apply_unified_io_with_prompt(storage, &body, "Executed prompt result", Some(&resolved_name), start)?;

    // Record successful history
    let entry = HistoryEntry::new(
        command,
        &input_text,
        &body,
        true,
        start.elapsed().as_millis(),
    );
    let _ = history_tracker.record(entry); // Don't fail if history fails

    Ok(())
}

#[allow(dead_code)]
pub fn handle_new(
    storage: &Storage,
    arg1: &str,
    arg2: Option<&str>,
    edit: bool,
    clean: bool,
    sync: Option<&str>,  // Deprecated, use io_options.file instead
    io_options: &IoOptions,
    start: Instant,
) -> Result<()> {
    // Check for stdin content first
    let mut stdin_content = if !std::io::stdin().is_terminal() {
        use std::io::Read;
        let mut content = String::new();
        std::io::stdin().read_to_string(&mut content)?;
        content.trim().to_string()
    } else {
        String::new()
    };

    // Clean TUI artifacts if requested
    if clean && !stdin_content.is_empty() {
        stdin_content = clean::clean_text(&stdin_content);
    }

    // Smart detection for flexible argument order
    let is_arg1_content = arg1.contains(' ')
        || arg1.contains('{')  // Template variable
        || arg1.contains('?')  // Question
        || arg1.contains('\n') // Multiline
        || arg1.len() > 40; // Long text

    let (prompt_name, prompt_content) = match arg2 {
        Some(arg2_value) => {
            // Two arguments provided - figure out which is which
            let is_arg2_content = arg2_value.contains(' ')
                || arg2_value.contains('{')
                || arg2_value.contains('?')
                || arg2_value.len() > 40;

            match (is_arg1_content, is_arg2_content) {
                (true, false) => (arg2_value.to_string(), arg1.to_string()), // "content" name
                (false, true) => (arg1.to_string(), arg2_value.to_string()), // name "content"
                (false, false) => (arg1.to_string(), arg2_value.to_string()), // name name (second is content)
                (true, true) => {
                    // Both look like content - use first as content, generate name
                    (generate_prompt_name(arg1), arg1.to_string())
                }
            }
        }
        None => {
            // Single argument - check stdin first, then use smart detection
            if !stdin_content.is_empty() {
                (arg1.to_string(), stdin_content.clone())
            } else if is_arg1_content {
                (generate_prompt_name(arg1), arg1.to_string())
            } else {
                (arg1.to_string(), String::new())
            }
        }
    };

    let prompt_path = storage.prompt_path(&prompt_name);

    if prompt_path.exists() {
        eprintln!("Error: Prompt '{}' already exists", prompt_name);
        std::process::exit(1);
    }

    // Create prompt with metadata
    let metadata = PromptMetadata {
        id: prompt_name.clone(),
        description: if prompt_content.is_empty() {
            format!("Description for {}", prompt_name)
        } else {
            // Use first line or 50 chars as description
            prompt_content
                .lines()
                .next()
                .unwrap_or(&prompt_content)
                .chars()
                .take(50)
                .collect::<String>()
                .trim()
                .to_string()
        },
        tags: None,
        created_at: Some(chrono::Utc::now().to_rfc3339()),
        updated_at: None,
        version: None,
        git_hash: None,
        parent_version: None,
    };

    let body = if prompt_content.is_empty() {
        format!("# {}\n\nYour prompt here...", prompt_name)
    } else {
        format!("# {}\n\n{}", prompt_name, prompt_content)
    };

    storage.write_prompt(&prompt_name, &metadata, &body)?;

    if !io_options.quiet {
        // Linear-style success message - clean and minimal
        println!(
            "✓ Created {} ({}ms)",
            prompt_name,
            start.elapsed().as_millis()
        );
    }

    // If content provided, apply unified I/O with prompt name for sync
    if !prompt_content.is_empty() {
        io_options.apply_unified_io_with_prompt(storage, &prompt_content, "Created prompt content", Some(&prompt_name), start)?;
    } else if edit {
        // Open in editor if --edit flag
        let editor = env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
        println!("Opening in {}...", editor.dimmed());
        let prompt_path = storage.prompt_path(&prompt_name);

        Command::new(&editor)
            .arg(&prompt_path)
            .status()
            .context("Failed to launch editor")?;
    }
    // Note: Editor is only opened when --edit flag is explicitly provided

    // Handle file sync functionality (via -f or deprecated --sync)
    let sync_path = io_options.file.as_deref().or(sync);
    if let Some(file_path) = sync_path {
        use super::SimpleSyncManager;
        let sync_manager = SimpleSyncManager::new(storage.clone())?;
        
        let local_path = if file_path.is_empty() {
            // Smart default: ./prompt-name.md
            let file_name = if prompt_name.starts_with('@') {
                // Strip team prefix: @team/api-design -> api-design.md
                prompt_name.split('/').last().unwrap_or(&prompt_name)
            } else {
                &prompt_name
            };
            
            // Convert to kebab-case for filename
            let kebab_name = file_name.replace('_', "-").to_lowercase();
            std::env::current_dir()?.join(format!("{}.md", kebab_name))
        } else {
            // Use provided path
            if std::path::Path::new(file_path).is_absolute() {
                std::path::PathBuf::from(file_path)
            } else {
                std::env::current_dir()?.join(file_path)
            }
        };
        
        match sync_manager.sync_prompt(&prompt_name, Some(local_path.clone())) {
            Ok(created_path) => {
                if !io_options.quiet {
                    println!("🔄 Created bidirectional sync:");
                    println!("   📁 Local file: {:?}", created_path);
                    println!("   📦 PromptHive: {}", prompt_name);
                }
            }
            Err(e) => {
                if !io_options.quiet {
                    println!("⚠️  Failed to create sync: {}", e);
                    println!("💡 Create sync manually with: ph sync sync-file {:?} --name {}", local_path, prompt_name);
                }
            }
        }
    }

    Ok(())
}

#[allow(dead_code)]
pub fn handle_edit(storage: &Storage, name: &str, start: Instant) -> Result<()> {
    // Use fuzzy matching to resolve the prompt name
    let resolved_name = resolve_prompt_name(storage, name)?;
    let prompt_path = storage.prompt_path(&resolved_name);

    // Check if we can write to the file before launching editor
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if prompt_path.exists() {
            let metadata = fs::metadata(&prompt_path)?;
            let permissions = metadata.permissions();
            let mode = permissions.mode();
            
            // Check if file is writable by user
            if mode & 0o200 == 0 {
                use crate::error_help;
                let error_msg = error_help::format_permission_error(&prompt_path.display().to_string(), "edit");
                return Err(anyhow::anyhow!("{}", error_msg));
            }
        }
    }

    // Get editor from environment or config
    let editor = if cfg!(test) {
        // In test mode, always use echo to avoid hanging
        "echo".to_string()
    } else {
        env::var("EDITOR").unwrap_or_else(|_| "vim".to_string())
    };

    // Linear-style editor launch message
    println!(
        "✓ Opening in {} ({}ms)",
        editor,
        start.elapsed().as_millis()
    );

    // Launch editor
    Command::new(&editor)
        .arg(&prompt_path)
        .status()
        .with_context(|| format!("Failed to launch editor '{}'", editor))?;

    Ok(())
}

#[allow(dead_code)]
pub fn handle_show(
    storage: &Storage,
    name: &str,
    edit: bool,
    io_options: &IoOptions,
    start: Instant,
) -> Result<()> {
    // Get all prompts
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

    // Find matching prompt
    let matcher = Matcher::new(prompts);
    let result = matcher.find(name);

    match result {
        MatchResult::Exact(prompt) => {
            // Read and display the prompt content
            let (metadata, body) = storage.read_prompt(&prompt.name)?;

            let content = body;

            // If edit flag is set, open in editor before applying I/O
            if edit {
                let prompt_path = storage.prompt_path(&prompt.name);
                let editor = env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
                if !io_options.quiet {
                    println!("Opening in {}...", editor.dimmed());
                }
                Command::new(&editor)
                    .arg(&prompt_path)
                    .status()
                    .context("Failed to launch editor")?;
                    
                // Re-read the content after editing
                let (metadata, body) = storage.read_prompt(&prompt.name)?;
                io_options.apply_display_io(&body, &prompt.name, &metadata.description, start)?;
            } else {
                io_options.apply_display_io(&content, &prompt.name, &metadata.description, start)?;
            }
        }
        _ => {
            result.display();
            std::process::exit(1);
        }
    }

    Ok(())
}

#[allow(dead_code)]
pub fn handle_delete(storage: &Storage, name: &str, start: Instant) -> Result<()> {
    // Use fuzzy matching to resolve the prompt name
    let resolved_name = resolve_prompt_name(storage, name)?;
    let prompt_path = storage.prompt_path(&resolved_name);

    // Read the prompt to show description
    let (metadata, body) = storage.read_prompt(&resolved_name)?;

    // Show prompt details for confirmation
    println!("\n{}", "About to delete:".yellow().bold());
    println!("  {} - {}", resolved_name.bold(), metadata.description);

    // Show first few lines of content
    let preview = body.lines().take(3).collect::<Vec<_>>().join("\n  ");
    if !preview.trim().is_empty() {
        println!("  {}", preview.dimmed());
        if body.lines().count() > 3 {
            println!("  {}", "...".dimmed());
        }
    }

    println!("\n{}", format!("Delete '{}'? [y/N] ", resolved_name).red());

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    if input.trim().to_lowercase() != "y" {
        println!("Deletion cancelled");
        return Ok(());
    }

    // Delete the file
    fs::remove_file(&prompt_path)?;

    // Linear-style deletion confirmation
    println!(
        "✓ Deleted {} ({}ms)",
        resolved_name,
        start.elapsed().as_millis()
    );

    Ok(())
}

#[allow(dead_code)]
pub fn handle_ls(storage: &Storage, io_options: &IoOptions, start: Instant) -> Result<()> {
    let prompts = storage.list_prompts()?;

    // Build content string for I/O operations
    let mut content = String::new();

    if prompts.is_empty() {
        content = "No prompts yet. Create one with: ph new <name>".to_string();
    } else {
        for prompt_name in &prompts {
            if let Ok((metadata, _)) = storage.read_prompt(prompt_name) {
                content.push_str(&format!("{:<20} - {}\n", prompt_name, metadata.description));
            }
        }
        content = content.trim_end().to_string(); // Remove trailing newline
    }

    // Apply unified I/O
    io_options.apply_unified_io(storage, &content, "Prompt list", start)?;

    // Display to terminal if not quiet and TTY
    if !io_options.quiet && std::io::stdout().is_terminal() {
        println!(
            "📋 {} ({}ms)\n",
            "Your prompts:".green(),
            start.elapsed().as_millis()
        );

        if prompts.is_empty() {
            println!(
                "  No prompts yet. Create one with: {} new <name>",
                "ph".bold()
            );
        } else {
            for prompt_name in prompts {
                if let Ok((metadata, _)) = storage.read_prompt(&prompt_name) {
                    println!("  {:<20} - {}", prompt_name.bold(), metadata.description);
                }
            }
        }
    }

    Ok(())
}

#[allow(dead_code)]
pub fn handle_find(
    storage: &Storage,
    query: &str,
    io_options: &IoOptions,
    start: Instant,
) -> Result<()> {
    let prompt_names = storage.list_prompts()?;
    let mut prompts = Vec::new();

    // Build prompt list with metadata
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

    // Use fuzzy matcher for search
    let fuzzy = SkimMatcherV2::default();
    let mut matches: Vec<_> = prompts
        .iter()
        .filter_map(|prompt| {
            // Match against name or description
            let name_score = fuzzy.fuzzy_match(&prompt.name, query);
            let desc_score = fuzzy.fuzzy_match(&prompt.description, query);

            // Take the best score
            let best_score = match (name_score, desc_score) {
                (Some(n), Some(d)) => Some(n.max(d)),
                (Some(n), None) => Some(n),
                (None, Some(d)) => Some(d),
                (None, None) => None,
            };

            best_score.map(|score| (prompt, score))
        })
        .collect();

    // Sort by score (highest first)
    matches.sort_by(|a, b| b.1.cmp(&a.1));

    // Build content string for I/O operations
    let content = if matches.is_empty() {
        format!("No prompts found matching '{}'\n", query)
    } else {
        matches
            .iter()
            .map(|(prompt, _)| format!("{:<20} - {}", prompt.name, prompt.description))
            .collect::<Vec<_>>()
            .join("\n")
    };

    // Apply unified I/O
    io_options.apply_unified_io(storage, &content, "Search results", start)?;

    // Display to terminal if not quiet and TTY
    if !io_options.quiet && std::io::stdout().is_terminal() {
        println!(
            "🔍 {} '{}' ({}ms)\n",
            "Searching for".green(),
            query.bold(),
            start.elapsed().as_millis()
        );

        if matches.is_empty() {
            println!("  No prompts found matching '{}'", query);
        } else {
            for (prompt, _) in matches {
                println!("  {:<20} - {}", prompt.name.bold(), prompt.description);
            }
        }
    }

    Ok(())
}

#[allow(dead_code)]
pub fn handle_rename(
    storage: &Storage,
    old_name: &str,
    new_name: &str,
    start: Instant,
) -> Result<()> {
    // Resolve old name with fuzzy matching
    let resolved_old = resolve_prompt_name(storage, old_name)?;

    // Check if new name already exists
    if storage.prompt_exists(new_name) {
        eprintln!("Error: Prompt '{}' already exists", new_name);
        std::process::exit(1);
    }

    // Read the old prompt
    let (metadata, body) = storage.read_prompt(&resolved_old)?;

    // Write to new location
    storage.write_prompt(new_name, &metadata, &body)?;

    // Delete old prompt
    storage.delete_prompt(&resolved_old)?;

    // Linear-style rename confirmation
    println!(
        "✓ Renamed {} → {} ({}ms)",
        resolved_old,
        new_name,
        start.elapsed().as_millis()
    );

    Ok(())
}

// Helper function to generate prompt name from content
#[allow(dead_code)]
fn generate_prompt_name(content: &str) -> String {
    let stop_words = [
        "the", "a", "an", "and", "or", "but", "in", "on", "at", "to", "for", "of", "with", "by",
        "from", "as", "is", "was", "are", "were",
    ];

    let words: Vec<&str> = content
        .split_whitespace()
        .take(7) // Take first 7 words
        .filter(|w| !stop_words.contains(&w.to_lowercase().as_str()))
        .collect();

    let name = words
        .join("-")
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect::<String>();

    // Truncate to reasonable length
    name.chars().take(30).collect()
}

// Helper function to resolve a prompt name using fuzzy matching
#[allow(dead_code)]
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

#[cfg(feature = "import")]
#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
pub fn handle_import(
    storage: &Storage,
    path: &str,
    custom_name: Option<&str>,
    force: bool,
    version: bool,
    skip: bool,
    update: bool,
    start: Instant,
) -> Result<()> {
    let importer = crate::Importer::new(storage.clone());
    let result =
        importer.import_from_path_enhanced(path, custom_name, force, version, skip, update)?;

    result.display();
    println!(
        "\n⏱️  {} ({}ms)",
        result.summary().green(),
        start.elapsed().as_millis()
    );

    Ok(())
}

#[allow(dead_code)]
pub fn handle_compose(
    storage: &Storage,
    prompts: &str,
    input: Option<&str>,
    edit: bool,
    io_options: &IoOptions,
    start: Instant,
) -> Result<()> {
    let prompt_names = parse_prompt_list(prompts);

    if prompt_names.is_empty() {
        eprintln!("Error: No prompts specified");
        std::process::exit(1);
    }

    let composer = Composer::new(storage.clone());
    let result = composer.compose_and_return(&prompt_names, input.map(|s| s.to_string()), edit)?;

    // Apply unified I/O using IoOptions
    io_options.apply_unified_io(storage, &result, "Composed prompt result", start)?;

    // Show composition summary if not quiet
    if !io_options.quiet {
        println!(
            "🔗 {} {} ({}ms)",
            "Composed".green(),
            prompts.bold(),
            start.elapsed().as_millis()
        );
    }

    Ok(())
}
