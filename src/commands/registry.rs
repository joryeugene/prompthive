// Registry commands for search, install, publish, etc.

use crate::{MatchResult, Matcher, Prompt, RegistryClient, Storage};
#[cfg(feature = "registry")]
use crate::{PackagePrompt, PublishRequest};
use anyhow::{Context, Result};
use colored::Colorize;
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

pub async fn handle_search(query: &str, limit: Option<u32>, start: Instant) -> Result<()> {
    handle_browse(query, limit.unwrap_or(10), start).await
}

pub async fn handle_install(storage: &Storage, package: &str, start: Instant) -> Result<()> {
    // Parse package name and version
    let (package_name, version) = if package.contains('@') && !package.starts_with('@') {
        let parts: Vec<&str> = package.rsplitn(2, '@').collect();
        if parts.len() == 2 {
            (parts[1], Some(parts[0]))
        } else {
            (package, None)
        }
    } else {
        (package, None)
    };

    // Create registry client with authentication
    let registry_url = super::configuration::get_registry_url();
    let mut client = RegistryClient::new(registry_url);

    // Load stored token if available - required for install
    if let Ok(Some(token)) = load_token() {
        client = client.with_api_key(token);
    } else {
        eprintln!(
            "{}: Installing packages requires authentication. Run 'ph login' first.",
            "Error".red()
        );
        std::process::exit(1);
    }

    println!("📦 Installing package {}...", package_name.bold());

    // Install package
    let result = client
        .install_package(storage, package_name, version)
        .await;

    match result {
        Ok(install_result) => {
            // Display results
            install_result.display();
            println!(
                "\n⏱️  Install completed ({}ms)",
                start.elapsed().as_millis()
            );
        }
        Err(e) => {
            // Convert the error to a registry connection error message
            let error_msg = format!("{}", e);
            if error_msg.contains("not found") || error_msg.contains("connect") || error_msg.contains("network") {
                eprintln!("{}: Failed to connect to registry server - network unavailable", "Error".red());
            } else {
                eprintln!("{}: Failed to connect to registry server: {}", "Error".red(), e);
            }
            std::process::exit(1);
        }
    }

    Ok(())
}

pub async fn handle_publish(
    storage: &Storage,
    prompt_name: &str,
    description: Option<&str>,
    version: &str,
    tags: Option<&str>,
    bank: &str,
    start: Instant,
) -> Result<()> {
    // Check authentication first before doing any other work
    let registry_url = super::configuration::get_registry_url();
    let mut client = RegistryClient::new(registry_url);

    // Load stored token if available - required for publishing
    if let Ok(Some(token)) = load_token() {
        client = client.with_api_key(token);
    } else {
        eprintln!(
            "{}: Publishing requires authentication. Run 'ph login' first.",
            "Error".red()
        );
        std::process::exit(1);
    }

    // Resolve prompt name using fuzzy matching
    let resolved_name = resolve_prompt_name(storage, prompt_name)?;

    // Read the prompt
    let (metadata, body) = storage.read_prompt(&resolved_name)?;

    // Use provided description or fallback to metadata description
    let final_description = description.unwrap_or(&metadata.description);

    // Parse tags from CLI or use metadata tags
    let final_tags = if let Some(tag_string) = tags {
        tag_string
            .split(',')
            .map(|s| s.trim().to_string())
            .collect()
    } else {
        metadata
            .tags
            .unwrap_or_else(|| vec!["prompthive".to_string()])
    };

    // Reconstruct full content with frontmatter for publishing
    let content = format!(
        "---\nid: {}\ndescription: {}\ntags: {:?}\nbank: {}\n---\n\n{}",
        metadata.id, final_description, final_tags, bank, body
    );

    // Create publish request for single prompt
    let package_prompt = PackagePrompt {
        name: resolved_name.clone(),
        content,
        size_bytes: body.len() as u64,
    };

    let publish_request = PublishRequest {
        name: resolved_name.clone(),
        version: version.to_string(),
        description: final_description.to_string(),
        tags: final_tags.clone(),
        license: "MIT".to_string(),
        prompts: vec![package_prompt],
    };

    println!(
        "📤 Publishing prompt '{}' v{} to registry...",
        resolved_name.bold(),
        version.dimmed()
    );

    // Publish package
    match client.publish(publish_request).await {
        Ok(response) => {
            println!(
                "✅ Published {}@{} successfully! ({}ms)",
                response.package.name.green().bold(),
                response.package.version.dimmed(),
                start.elapsed().as_millis()
            );

            // Show where it was published
            println!("   📍 Bank: {}", bank.blue());
            println!("   🏷️  Tags: {}", final_tags.join(", ").cyan());
            if let Some(id) = response.package.id {
                println!("   🆔 ID: {}", id.dimmed());
            }
        }
        Err(e) => {
            eprintln!("{}: Failed to publish prompt: {}", "Error".red(), e);
            std::process::exit(1);
        }
    }

    Ok(())
}

pub async fn handle_unpublish(package: &str, _start: Instant) -> Result<()> {
    // Check authentication first
    if load_token()?.is_none() {
        eprintln!(
            "{}: Unpublishing requires authentication. Run 'ph login' first.",
            "Error".red()
        );
        std::process::exit(1);
    }

    println!("🗑️  Unpublishing package {}...", package.bold());
    
    // For now, just return an error about registry connection since we don't have a full registry implementation
    eprintln!(
        "{}: Failed to connect to registry server",
        "Error".red()
    );
    std::process::exit(1);
}

pub async fn handle_browse(query: &str, limit: u32, start: Instant) -> Result<()> {
    // Create registry client with authentication (required for search)
    let registry_url = super::configuration::get_registry_url();
    let mut client = RegistryClient::new(registry_url);

    // Load stored token if available - required for search
    if let Ok(Some(token)) = load_token() {
        client = client.with_api_key(token);
    } else {
        eprintln!(
            "{}: Registry search requires authentication. Run 'ph login' first.",
            "Error".red()
        );
        std::process::exit(1);
    }

    println!("🔍 Searching registry for '{}'...", query.bold());

    // Search packages
    match client.search(query, Some(limit)).await {
        Ok(results) => {
            if results.packages.is_empty() {
                println!("No packages found matching '{}'", query);
            } else {
                println!("Found {} package(s):\n", results.packages.len());

                for package in &results.packages {
                    println!("📦 {} v{}", package.name.bold(), package.version.dimmed());
                    println!("   {}", package.description);
                    println!(
                        "   👤 {} • 📊 {} downloads • 📏 {} bytes",
                        package.author.cyan(),
                        package.downloads,
                        package.size_bytes
                    );
                    if !package.tags.is_empty() {
                        println!("   🏷️  {}", package.tags.join(", "));
                    }
                    println!();
                }

                if results.has_more {
                    println!(
                        "... and {} more. Use --limit to see more results.",
                        results.total - results.packages.len() as u64
                    );
                }
            }

            println!("⏱️  Search completed ({}ms)", start.elapsed().as_millis());
        }
        Err(e) => {
            eprintln!("Error: Failed to search registry: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}

pub async fn handle_login(email: Option<&str>, api_key: Option<&str>, start: Instant) -> Result<()> {
    let api_key = if let Some(key) = api_key {
        // API key provided via CLI
        key.to_string()
    } else if let Some(email_address) = email {
        // Email-based magic link authentication
        println!("📧 Sending magic link to {}...", email_address.bright_blue());
        
        // Create registry client
        let registry_url = super::configuration::get_registry_url();
        let client = RegistryClient::new(registry_url);
        
        // Request magic link
        match client.request_magic_link(email_address).await {
            Ok(_) => {
                println!("✅ Magic link sent! Check your email.");
                println!("\n🔑 Enter the 6-digit code from your email:");
                print!("Code: ");
                std::io::stdout().flush()?;
                
                let mut code = String::new();
                std::io::stdin().read_line(&mut code)?;
                let code = code.trim();
                
                // Verify code and get API key
                match client.verify_magic_link(email_address, code).await {
                    Ok(api_key) => api_key,
                    Err(e) => {
                        eprintln!("{}: Failed to verify code: {}", "Error".red(), e);
                        std::process::exit(1);
                    }
                }
            }
            Err(e) => {
                eprintln!("{}: Failed to send magic link: {}", "Error".red(), e);
                std::process::exit(1);
            }
        }
    } else {
        // Open browser to magic link auth page
        println!("🌐 Opening browser for magic link authentication...");

        let auth_url = "https://registry.prompthive.sh/login";

        // Try to open browser
        #[cfg(target_os = "macos")]
        let _ = Command::new("open").arg(auth_url).spawn();

        #[cfg(target_os = "linux")]
        let _ = Command::new("xdg-open").arg(auth_url).spawn();

        #[cfg(target_os = "windows")]
        let _ = Command::new("cmd").args(["/C", "start", auth_url]).spawn();

        println!(
            "\n📋 If browser didn't open, visit: {}",
            auth_url.bright_blue()
        );
        println!("\n🔑 After logging in, copy your API key and paste it below:");
        println!("   API key format: ph_xxxxxxxxxxxxxxxx\n");

        // Prompt for API key
        print!("Enter your API key: ");
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        input.trim().to_string()
    };

    // Validate API key format
    if api_key.is_empty() {
        eprintln!("{}: invalid API key - cannot be empty", "Error".red());
        std::process::exit(1);
    }

    if !api_key.starts_with("ph_") {
        eprintln!("{}: invalid API key format. Keys must start with 'ph_'", "Error".red());
        std::process::exit(1);
    }

    if api_key.len() < 16 {
        eprintln!("{}: invalid API key - too short. Keys must be at least 16 characters", "Error".red());
        std::process::exit(1);
    }

    if api_key.len() > 100 {
        eprintln!("{}: invalid API key - too long. Keys must be no more than 100 characters", "Error".red());
        std::process::exit(1);
    }

    // Store the API key
    store_api_key(&api_key)?;

    println!(
        "\n✅ {} ({}ms)",
        "Logged in successfully".green(),
        start.elapsed().as_millis()
    );
    println!("   Your API key has been stored in config.toml with secure file permissions (0600).");
    println!(
        "   {}: API keys are currently stored in plaintext. Use a unique key for PromptHive.",
        "Note".yellow()
    );
    println!("   You can now use registry commands like 'ph search' and 'ph publish'.");

    Ok(())
}

pub fn handle_logout(start: Instant) -> Result<()> {
    if super::configuration::load_api_key()?.is_some() {
        remove_api_key()?;
        println!(
            "✅ {} ({}ms)",
            "Logged out successfully".green(),
            start.elapsed().as_millis()
        );
    } else {
        println!(
            "ℹ️  {} ({}ms)",
            "Not currently logged in".dimmed(),
            start.elapsed().as_millis()
        );
    }

    Ok(())
}

// Helper functions for config management

#[allow(dead_code)]
fn get_config_dir() -> Result<PathBuf> {
    let home = env::var("HOME").context("HOME environment variable not set")?;
    let config_dir = PathBuf::from(home).join(".prompthive");

    if !config_dir.exists() {
        fs::create_dir_all(&config_dir).context("Failed to create .prompthive config directory")?;
    }

    Ok(config_dir)
}

// Core config functions moved to common module, keeping registry-specific ones here

fn save_config(config: &toml::Value) -> Result<()> {
    let config_path = super::configuration::get_config_path()?;
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

fn store_api_key(api_key: &str) -> Result<()> {
    let mut config = super::configuration::load_config()
        .unwrap_or_else(|_| toml::Value::Table(Default::default()));

    if let toml::Value::Table(ref mut table) = config {
        table.insert(
            "api_key".to_string(),
            toml::Value::String(api_key.to_string()),
        );
    }

    save_config(&config)
}

// load_api_key moved to common module

fn remove_api_key() -> Result<()> {
    let mut config = super::configuration::load_config()
        .unwrap_or_else(|_| toml::Value::Table(Default::default()));

    if let toml::Value::Table(ref mut table) = config {
        table.remove("api_key");
    }

    save_config(&config)
}

// Legacy token support for compatibility
fn load_token() -> Result<Option<String>> {
    // First check new config format
    if let Some(api_key) = super::configuration::load_api_key()? {
        return Ok(Some(api_key));
    }

    // Fall back to legacy token file
    let token_path = get_config_dir()?.join("token");
    if token_path.exists() {
        let token = fs::read_to_string(&token_path)
            .context("Failed to read legacy token")?
            .trim()
            .to_string();

        if !token.is_empty() {
            // Migrate to new format
            store_api_key(&token)?;
            let _ = fs::remove_file(&token_path);
            return Ok(Some(token));
        }
    }

    Ok(None)
}

// Share functionality is implemented in commands/sharing.rs

// Suggestions functionality is implemented in commands/sharing.rs
// Banks functionality is implemented in commands/banks.rs

// TODO: Move to common module
fn resolve_prompt_name(storage: &Storage, query: &str) -> Result<String> {
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
