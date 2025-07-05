use super::common;
use crate::{PromptMetadata, RegistryClient, Storage};
use super::SimpleSyncManager;
use anyhow::{Context, Result};
use chrono;
use clap::Subcommand;
use colored::Colorize;
use serde_json;
use std::time::Instant;
use urlencoding;

// Helper functions moved to common module

fn resolve_prompt_name(_storage: &Storage, query: &str) -> Result<String> {
    // Simplified implementation for now
    Ok(query.to_string())
}

#[derive(Subcommand)]
pub enum SyncCommands {
    /// Push local prompts to cloud storage
    Push {
        /// Specific prompt to push (all if not specified)
        prompt: Option<String>,
        /// Force push even if conflicts exist
        #[arg(short = 'f', long = "force")]
        force: bool,
    },
    /// Pull cloud prompts to local storage
    Pull {
        /// Specific prompt to pull (all if not specified)
        prompt: Option<String>,
        /// Overwrite local changes without confirmation
        #[arg(short = 'f', long = "force")]
        force: bool,
    },
    /// Show sync status and conflicts
    Status {
        /// Show detailed status for each prompt
        #[arg(short = 'v', long = "verbose")]
        verbose: bool,
    },
    /// Resolve sync conflicts
    Resolve {
        /// Prompt name with conflict to resolve
        prompt: String,
        /// Resolution strategy: local, cloud, or manual
        #[arg(short = 'r', long = "resolution", value_parser = ["local", "cloud", "manual"])]
        resolution: String,
    },
    /// Verify sync integrity by checking actual database state
    Verify {
        /// Specific prompt to verify (all if not specified)
        prompt: Option<String>,
        /// Show detailed verification information
        #[arg(short = 'v', long = "verbose")]
        verbose: bool,
    },
    /// Create bidirectional file sync for a prompt
    SyncFile {
        /// Local file path to sync with
        path: String,
        /// Prompt name (defaults to filename without extension)
        #[arg(short = 'n', long = "name")]
        name: Option<String>,
        /// Force overwrite if file exists
        #[arg(short = 'f', long = "force")]
        force: bool,
    },
    /// Sync entire directory of prompts
    SyncDir {
        /// Directory containing markdown files
        directory: String,
        /// Pattern to match files (default: "*.md")
        #[arg(short = 'p', long = "pattern")]
        pattern: Option<String>,
        /// Force sync even if files exist
        #[arg(short = 'f', long = "force")]
        force: bool,
    },
    /// Remove bidirectional sync relationship
    Unsync {
        /// Prompt name to unsync
        prompt: String,
        /// Keep the synced file (don't delete)
        #[arg(short = 'k', long = "keep-file")]
        keep_file: bool,
    },
    /// Show detailed sync status for file sync
    FileStatus {
        /// Specific prompt to check (all if not specified)
        prompt: Option<String>,
        /// Show file paths and timestamps
        #[arg(short = 'v', long = "verbose")]
        verbose: bool,
    },
    /// Repair broken sync relationships
    Repair {
        /// Specific prompt to repair (all if not specified)
        prompt: Option<String>,
        /// Recreate missing files from PromptHive content
        #[arg(short = 'r', long = "recreate")]
        recreate: bool,
    },
    /// List all sync conflicts
    Conflicts {
        /// Show detailed conflict information
        #[arg(short = 'v', long = "verbose")]
        verbose: bool,
    },
    /// Watch files for changes and auto-sync
    Watch {
        /// Directory to watch (default: current directory)
        directory: Option<String>,
        /// Debounce delay in milliseconds (default: 100)
        #[arg(short = 'd', long = "delay")]
        delay: Option<u64>,
    },
}

pub async fn handle_sync(
    storage: &Storage,
    action: &Option<SyncCommands>,
    start: Instant,
) -> Result<()> {
    // Check if user has API key
    let api_key = common::require_api_key("Sync")?;

    let registry_url = super::configuration::get_registry_url();
    let client = RegistryClient::new(registry_url).with_api_key(api_key);

    match action {
        Some(SyncCommands::Push { prompt, force }) => {
            handle_sync_push(storage, &client, prompt.as_deref(), *force, start).await
        }
        Some(SyncCommands::Pull { prompt, force }) => {
            handle_sync_pull(storage, &client, prompt.as_deref(), *force, start).await
        }
        Some(SyncCommands::Status { verbose }) => {
            handle_sync_status(storage, &client, *verbose, start).await
        }
        Some(SyncCommands::Resolve { prompt, resolution }) => {
            handle_sync_resolve(storage, &client, prompt, resolution, start).await
        }
        Some(SyncCommands::Verify { prompt, verbose }) => {
            handle_sync_verify(storage, &client, prompt.as_deref(), *verbose, start).await
        }
        Some(SyncCommands::SyncFile { path, name, force }) => {
            handle_sync_file(storage, path, name.as_deref(), *force, start).await
        }
        Some(SyncCommands::SyncDir { directory: _, pattern: _, force: _ }) => {
            Err(anyhow::anyhow!("SyncDir functionality temporarily disabled - coming soon"))
        }
        Some(SyncCommands::Unsync { prompt: _, keep_file: _ }) => {
            Err(anyhow::anyhow!("Unsync functionality temporarily disabled - coming soon"))
        }
        Some(SyncCommands::FileStatus { prompt: _, verbose: _ }) => {
            Err(anyhow::anyhow!("FileStatus functionality temporarily disabled - coming soon"))
        }
        Some(SyncCommands::Repair { prompt: _, recreate: _ }) => {
            Err(anyhow::anyhow!("Repair functionality temporarily disabled - coming soon"))
        }
        Some(SyncCommands::Conflicts { verbose: _ }) => {
            Err(anyhow::anyhow!("Conflicts functionality temporarily disabled - coming soon"))
        }
        Some(SyncCommands::Watch { directory: _, delay: _ }) => {
            Err(anyhow::anyhow!("Watch functionality temporarily disabled - coming soon"))
        }
        None => {
            // Default sync (bidirectional)
            handle_sync_bidirectional(storage, &client, start).await
        }
    }
}

// Note: The actual sync function implementations will be extracted from main.rs in the next step
// This is a placeholder to establish the module structure

async fn handle_sync_push(
    storage: &Storage,
    client: &RegistryClient,
    prompt: Option<&str>,
    force: bool,
    start: Instant,
) -> Result<()> {
    println!("☁️  Pushing prompts to cloud...");

    let prompts_to_sync = if let Some(prompt_name) = prompt {
        // Push specific prompt
        let resolved_name = resolve_prompt_name(storage, prompt_name)?;
        vec![resolved_name]
    } else {
        // Push all prompts
        storage.list_prompts()?
    };

    if prompts_to_sync.is_empty() {
        println!("No prompts to sync");
        return Ok(());
    }

    // Build prompts payload
    let mut prompts_data = Vec::new();
    for prompt_name in &prompts_to_sync {
        let (metadata, content) = storage
            .read_prompt(prompt_name)
            .with_context(|| format!("Failed to read prompt '{}'", prompt_name))?;

        prompts_data.push(serde_json::json!({
            "name": prompt_name,
            "content": content,
            "description": metadata.description,
            "tags": metadata.tags.unwrap_or_default()
        }));
    }

    let payload = serde_json::json!({
        "prompts": prompts_data,
        "force": force
    });

    // Send sync push request
    let response = client
        .post("/api/sync/push", &payload)
        .await
        .context("Failed to push prompts to cloud")?;

    let status = response.status();
    let response_text = response.text().await.unwrap_or_default();

    if !status.is_success() {
        return Err(anyhow::anyhow!(
            "Sync push failed with status {}: {}",
            status,
            response_text
        ));
    }

    let result: serde_json::Value = serde_json::from_str(&response_text).context(format!(
        "Failed to parse sync push response: {}",
        response_text
    ))?;

    let sync_success = result
        .get("success")
        .and_then(|s| s.as_bool())
        .unwrap_or(false);
    let sync_message = result
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("Sync completed");

    // Process results
    if let Some(results) = result.get("results").and_then(|r| r.as_array()) {
        let mut created = 0;
        let mut updated = 0;
        let mut conflicts = 0;
        let mut errors = 0;

        for result_item in results {
            let name = result_item
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("unknown");
            let status = result_item
                .get("status")
                .and_then(|s| s.as_str())
                .unwrap_or("unknown");

            match status {
                "created" => {
                    created += 1;
                    println!("✅ Created: {}", name);
                }
                "updated" => {
                    updated += 1;
                    println!("🔄 Updated: {}", name);
                }
                "conflict" => {
                    conflicts += 1;
                    println!("⚠️  Conflict: {} (use `ph sync status` to resolve)", name);
                }
                "error" => {
                    errors += 1;
                    let error = result_item
                        .get("error")
                        .and_then(|e| e.as_str())
                        .unwrap_or("unknown error");
                    println!("❌ Error: {} - {}", name, error);
                }
                _ => {
                    println!("❓ Unknown status for {}: {}", name, status);
                }
            }
        }

        // Summary
        println!();
        if sync_success {
            println!(
                "📊 {} {}",
                "Sync Push Summary:".green(),
                sync_message.green()
            );
        } else {
            println!(
                "📊 {} {}",
                "Sync Push Summary:".yellow(),
                sync_message.yellow()
            );
        }

        if created > 0 {
            println!("   ✅ Created: {}", created);
        }
        if updated > 0 {
            println!("   🔄 Updated: {}", updated);
        }
        if conflicts > 0 {
            println!(
                "   ⚠️  Conflicts: {} (resolve with `ph sync status`)",
                conflicts
            );
        }
        if errors > 0 {
            println!("   ❌ Errors: {} (check server logs or try again)", errors);
            if !sync_success {
                return Err(anyhow::anyhow!(
                    "Sync push failed due to {} database errors",
                    errors
                ));
            }
        }

        // Use stats from API if available
        if let Some(stats) = result.get("stats") {
            let api_errors = stats.get("errors").and_then(|e| e.as_u64()).unwrap_or(0);
            if api_errors > 0 && !sync_success {
                return Err(anyhow::anyhow!(
                    "Server reported {} errors during sync push",
                    api_errors
                ));
            }
        }
    } else {
        println!("⚠️  Warning: No detailed results received from server");
        if !sync_success {
            return Err(anyhow::anyhow!("Sync push failed: {}", sync_message));
        }
    }

    println!(
        "⏱️  Sync push completed ({}ms)",
        start.elapsed().as_millis()
    );
    Ok(())
}

async fn handle_sync_pull(
    storage: &Storage,
    client: &RegistryClient,
    prompt: Option<&str>,
    force: bool,
    start: Instant,
) -> Result<()> {
    println!("☁️  Pulling prompts from cloud...");

    // Get prompts to pull
    let prompts_to_pull = if let Some(prompt_name) = prompt {
        // Pull specific prompt
        let resolved_name = resolve_prompt_name(storage, prompt_name)?;
        vec![resolved_name]
    } else {
        // Get all cloud prompts
        match client.get("/api/prompts").await {
            Ok(response) => {
                if response.status().is_success() {
                    match response.json::<serde_json::Value>().await {
                        Ok(cloud_data) => {
                            if let Some(cloud_prompts) = cloud_data["prompts"].as_array() {
                                cloud_prompts
                                    .iter()
                                    .filter_map(|p| p["name"].as_str().map(|s| s.to_string()))
                                    .collect()
                            } else {
                                return Err(anyhow::anyhow!("Invalid cloud prompts response format"));
                            }
                        }
                        Err(e) => {
                            return Err(anyhow::anyhow!("Failed to parse cloud prompts: {}", e));
                        }
                    }
                } else {
                    return Err(anyhow::anyhow!(
                        "Failed to fetch cloud prompts: {}",
                        response.status()
                    ));
                }
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Request failed: {}", e));
            }
        }
    };

    if prompts_to_pull.is_empty() {
        println!("No prompts to pull from cloud");
        return Ok(());
    }

    let mut pulled = 0;
    let mut updated = 0;
    let mut conflicts = 0;
    let mut errors = 0;

    for prompt_name in &prompts_to_pull {
        println!("📥 Pulling '{}'...", prompt_name);

        // Fetch cloud prompt
        let url = format!("/api/prompts/{}", urlencoding::encode(prompt_name));
        match client.get(&url).await {
            Ok(response) => {
                if response.status().is_success() {
                    match response.json::<serde_json::Value>().await {
                        Ok(cloud_data) => {
                            let cloud_content = cloud_data["content"].as_str().unwrap_or("");
                            let cloud_description = cloud_data["description"].as_str().unwrap_or("");
                            let cloud_tags: Vec<String> = cloud_data["tags"]
                                .as_array()
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                        .collect()
                                })
                                .unwrap_or_default();

                            // Check if local version exists
                            let local_exists = storage.prompt_exists(prompt_name);

                            if local_exists && !force {
                                // Check for conflicts
                                match storage.read_prompt(prompt_name) {
                                    Ok((local_metadata, local_content)) => {
                                        let content_matches = local_content.trim() == cloud_content.trim();
                                        let description_matches = local_metadata.description == cloud_description;

                                        if !content_matches || !description_matches {
                                            conflicts += 1;
                                            println!("⚠️  Conflict detected for '{}' (use --force to overwrite)", prompt_name);
                                            continue;
                                        } else {
                                            println!("✅ '{}' is already up to date", prompt_name);
                                            continue;
                                        }
                                    }
                                    Err(e) => {
                                        errors += 1;
                                        println!("❌ Failed to read local '{}': {}", prompt_name, e);
                                        continue;
                                    }
                                }
                            }

                            // Create/update local prompt
                            let metadata = PromptMetadata {
                                id: prompt_name.clone(),
                                description: cloud_description.to_string(),
                                tags: if cloud_tags.is_empty() { None } else { Some(cloud_tags) },
                                created_at: Some(chrono::Utc::now().to_rfc3339()),
                                updated_at: None,
                                version: None,
                                git_hash: None,
                                parent_version: None,
                            };

                            match storage.write_prompt(prompt_name, &metadata, cloud_content) {
                                Ok(_) => {
                                    if local_exists {
                                        updated += 1;
                                        println!("🔄 Updated '{}'", prompt_name);
                                    } else {
                                        pulled += 1;
                                        println!("📥 Pulled '{}'", prompt_name);
                                    }
                                }
                                Err(e) => {
                                    errors += 1;
                                    println!("❌ Failed to save '{}': {}", prompt_name, e);
                                }
                            }
                        }
                        Err(e) => {
                            errors += 1;
                            println!("❌ Failed to parse cloud response for '{}': {}", prompt_name, e);
                        }
                    }
                } else if response.status().as_u16() == 404 {
                    println!("⚠️  '{}' not found in cloud", prompt_name);
                } else {
                    errors += 1;
                    println!("❌ Cloud API error for '{}': {}", prompt_name, response.status());
                }
            }
            Err(e) => {
                errors += 1;
                println!("❌ Request failed for '{}': {}", prompt_name, e);
            }
        }
    }

    // Summary
    println!();
    println!("📊 {} Sync Pull Summary", "📥".green().bold());
    
    if pulled > 0 {
        println!("   📥 New prompts: {}", pulled);
    }
    if updated > 0 {
        println!("   🔄 Updated prompts: {}", updated);
    }
    if conflicts > 0 {
        println!("   ⚠️  Conflicts (skipped): {} (use --force to overwrite)", conflicts);
    }
    if errors > 0 {
        println!("   ❌ Errors: {}", errors);
    }

    if pulled == 0 && updated == 0 && conflicts == 0 && errors == 0 {
        println!("   📭 No changes to pull");
    }

    println!(
        "⏱️  Sync pull completed ({}ms)",
        start.elapsed().as_millis()
    );
    Ok(())
}

async fn handle_sync_status(
    storage: &Storage,
    client: &RegistryClient,
    verbose: bool,
    start: Instant,
) -> Result<()> {
    println!("🔄 Checking sync status...");

    // Get all local prompts
    let local_prompts = storage.list_prompts()?;

    if local_prompts.is_empty() {
        println!("No local prompts to sync");
        return Ok(());
    }

    let mut synced = 0;
    let mut pending_push = 0;
    let mut pending_pull = 0;
    let mut conflicts = 0;
    let mut errors = 0;

    // Check each prompt's sync status
    for prompt_name in &local_prompts {
        if verbose {
            println!("🔍 Checking '{}'...", prompt_name);
        }

        // Read local prompt
        let (local_metadata, local_content) = match storage.read_prompt(prompt_name) {
            Ok((metadata, content)) => (metadata, content),
            Err(e) => {
                if verbose {
                    println!("❌ Local read error for '{}': {}", prompt_name, e);
                }
                errors += 1;
                continue;
            }
        };

        // Check cloud version
        let url = format!("/api/prompts/{}", urlencoding::encode(prompt_name));
        match client.get(&url).await {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    // Parse cloud response
                    match response.json::<serde_json::Value>().await {
                        Ok(cloud_data) => {
                            let cloud_content = cloud_data["content"].as_str().unwrap_or("");
                            let cloud_description =
                                cloud_data["description"].as_str().unwrap_or("");
                            let cloud_updated = cloud_data["updated_at"].as_str().unwrap_or("");

                            // Compare content and metadata
                            let content_matches = local_content.trim() == cloud_content.trim();
                            let description_matches =
                                local_metadata.description == cloud_description;

                            if content_matches && description_matches {
                                synced += 1;
                                if verbose {
                                    println!(
                                        "✅ '{}' - Synced (updated: {})",
                                        prompt_name, cloud_updated
                                    );
                                }
                            } else {
                                conflicts += 1;
                                if verbose {
                                    println!("⚠️  '{}' - Conflict detected", prompt_name);
                                    if !content_matches {
                                        println!("   📝 Content differs");
                                    }
                                    if !description_matches {
                                        println!("   📄 Description differs");
                                    }
                                    println!("   💡 Use `ph sync resolve {}` to fix", prompt_name);
                                }
                            }
                        }
                        Err(e) => {
                            errors += 1;
                            if verbose {
                                println!(
                                    "❌ '{}' - Failed to parse cloud response: {}",
                                    prompt_name, e
                                );
                            }
                        }
                    }
                } else if status.as_u16() == 404 {
                    pending_push += 1;
                    if verbose {
                        println!("📤 '{}' - Needs push (not in cloud)", prompt_name);
                    }
                } else {
                    errors += 1;
                    if verbose {
                        println!("❌ '{}' - Cloud API error: {}", prompt_name, status);
                    }
                }
            }
            Err(e) => {
                errors += 1;
                if verbose {
                    println!("❌ '{}' - Request failed: {}", prompt_name, e);
                }
            }
        }
    }

    // Check for cloud-only prompts (need pull)
    match client.get("/api/prompts").await {
        Ok(response) => {
            if response.status().is_success() {
                match response.json::<serde_json::Value>().await {
                    Ok(cloud_data) => {
                        if let Some(cloud_prompts) = cloud_data["prompts"].as_array() {
                            for cloud_prompt in cloud_prompts {
                                if let Some(cloud_name) = cloud_prompt["name"].as_str() {
                                    if !local_prompts.contains(&cloud_name.to_string()) {
                                        pending_pull += 1;
                                        if verbose {
                                            println!(
                                                "📥 '{}' - Available for pull (cloud only)",
                                                cloud_name
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(_) => {
                        if verbose {
                            println!("⚠️  Could not parse cloud prompts list");
                        }
                    }
                }
            }
        }
        Err(_) => {
            if verbose {
                println!("⚠️  Could not fetch cloud prompts list");
            }
        }
    }

    // Summary
    println!();
    println!("📊 {} Sync Status Summary", "🔄".green().bold());

    if synced > 0 {
        println!("   ✅ Synced: {} prompt(s)", synced);
    }
    if pending_push > 0 {
        println!("   📤 Pending push: {} prompt(s)", pending_push);
    }
    if pending_pull > 0 {
        println!("   📥 Pending pull: {} prompt(s)", pending_pull);
    }
    if conflicts > 0 {
        println!(
            "   ⚠️  Conflicts: {} prompt(s) (require resolution)",
            conflicts
        );
    }
    if errors > 0 {
        println!("   ❌ Errors: {} prompt(s)", errors);
    }

    let total = synced + pending_push + pending_pull + conflicts + errors;
    if total == 0 {
        println!("   📭 No prompts found");
    } else {
        println!("   📊 Total: {} prompt(s)", total);
    }

    // Suggested actions
    if pending_push > 0 || pending_pull > 0 || conflicts > 0 {
        println!();
        println!("💡 {} Suggested actions:", "Next steps:".bold());

        if pending_push > 0 {
            println!("   📤 Push local changes: {}", "ph sync push".bold());
        }
        if pending_pull > 0 {
            println!("   📥 Pull cloud changes: {}", "ph sync pull".bold());
        }
        if conflicts > 0 {
            println!(
                "   ⚠️  Resolve conflicts: {} <prompt_name> --resolution [local|cloud|manual]",
                "ph sync resolve".bold()
            );
        }
        if pending_push > 0 && pending_pull > 0 {
            println!("   🔄 Bidirectional sync: {}", "ph sync".bold());
        }
    } else if synced == total && total > 0 {
        println!();
        println!("💚 {} All prompts are in sync!", "Perfect!".bold());
    }

    println!(
        "⏱️  Sync status completed ({}ms)",
        start.elapsed().as_millis()
    );
    Ok(())
}

async fn handle_sync_resolve(
    storage: &Storage,
    client: &RegistryClient,
    prompt: &str,
    resolution: &str,
    start: Instant,
) -> Result<()> {
    let resolved_name = resolve_prompt_name(storage, prompt)?;
    println!("🔧 Resolving sync conflict for '{}'...", resolved_name);

    // Validate resolution strategy
    if !["local", "cloud", "manual"].contains(&resolution) {
        return Err(anyhow::anyhow!(
            "Invalid resolution strategy '{}'. Must be: local, cloud, or manual",
            resolution
        ));
    }

    // Check if prompt exists locally
    if !storage.prompt_exists(&resolved_name) {
        return Err(anyhow::anyhow!(
            "Prompt '{}' does not exist locally. Cannot resolve conflict.",
            resolved_name
        ));
    }

    // Read local version
    let (local_metadata, local_content) = storage
        .read_prompt(&resolved_name)
        .with_context(|| format!("Failed to read local prompt '{}'", resolved_name))?;

    // Fetch cloud version
    let url = format!("/api/prompts/{}", urlencoding::encode(&resolved_name));
    let cloud_response = client
        .get(&url)
        .await
        .with_context(|| format!("Failed to fetch cloud prompt '{}'", resolved_name))?;

    if !cloud_response.status().is_success() {
        if cloud_response.status().as_u16() == 404 {
            return Err(anyhow::anyhow!(
                "Prompt '{}' does not exist in cloud. No conflict to resolve.",
                resolved_name
            ));
        } else {
            return Err(anyhow::anyhow!(
                "Failed to fetch cloud prompt '{}': {}",
                resolved_name,
                cloud_response.status()
            ));
        }
    }

    let cloud_data: serde_json::Value = cloud_response
        .json()
        .await
        .with_context(|| format!("Failed to parse cloud response for '{}'", resolved_name))?;

    let cloud_content = cloud_data["content"].as_str().unwrap_or("");
    let cloud_description = cloud_data["description"].as_str().unwrap_or("");
    let cloud_tags: Vec<String> = cloud_data["tags"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // Check if there's actually a conflict
    let content_matches = local_content.trim() == cloud_content.trim();
    let description_matches = local_metadata.description == cloud_description;

    if content_matches && description_matches {
        println!("✅ No conflict detected for '{}' - already in sync", resolved_name);
        return Ok(());
    }

    // Show conflict details
    println!();
    println!("⚠️  {} Conflict Details:", "Sync conflict detected!".yellow().bold());
    
    if !content_matches {
        println!("   📝 Content differs:");
        println!("      Local: {} characters", local_content.len());
        println!("      Cloud: {} characters", cloud_content.len());
    }
    
    if !description_matches {
        println!("   📄 Description differs:");
        println!("      Local: '{}'", local_metadata.description);
        println!("      Cloud: '{}'", cloud_description);
    }

    println!();

    match resolution {
        "local" => {
            println!("🏠 Keeping local version and pushing to cloud...");
            
            // Push local version to cloud
            let payload = serde_json::json!({
                "prompts": [{
                    "name": resolved_name,
                    "content": local_content,
                    "description": local_metadata.description,
                    "tags": local_metadata.tags.unwrap_or_default()
                }],
                "force": true
            });

            let push_response = client
                .post("/api/sync/push", &payload)
                .await
                .context("Failed to push local version to cloud")?;

            if push_response.status().is_success() {
                println!("✅ Local version pushed to cloud successfully");
            } else {
                let error_text = push_response.text().await.unwrap_or_default();
                return Err(anyhow::anyhow!(
                    "Failed to push local version: {}",
                    error_text
                ));
            }
        }
        "cloud" => {
            println!("☁️  Keeping cloud version and updating local...");
            
            // Update local with cloud version
            let metadata = PromptMetadata {
                id: resolved_name.clone(),
                description: cloud_description.to_string(),
                tags: if cloud_tags.is_empty() { None } else { Some(cloud_tags) },
                created_at: local_metadata.created_at,
                updated_at: Some(chrono::Utc::now().to_rfc3339()),
                version: local_metadata.version,
                git_hash: local_metadata.git_hash,
                parent_version: local_metadata.parent_version,
            };

            storage
                .write_prompt(&resolved_name, &metadata, cloud_content)
                .with_context(|| format!("Failed to save cloud version locally for '{}'", resolved_name))?;

            println!("✅ Cloud version saved locally successfully");
        }
        "manual" => {
            println!("🛠  Manual resolution selected...");
            println!();
            println!("Local version:");
            println!("  Description: {}", local_metadata.description);
            println!("  Content (first 100 chars): {}", 
                &local_content.chars().take(100).collect::<String>());
            if local_content.len() > 100 {
                println!("  ... ({} more characters)", local_content.len() - 100);
            }
            
            println!();
            println!("Cloud version:");
            println!("  Description: {}", cloud_description);
            println!("  Content (first 100 chars): {}", 
                &cloud_content.chars().take(100).collect::<String>());
            if cloud_content.len() > 100 {
                println!("  ... ({} more characters)", cloud_content.len() - 100);
            }

            println!();
            println!("💡 To manually resolve this conflict:");
            println!("   1. Edit the prompt: {}", format!("ph edit {}", resolved_name).bold());
            println!("   2. Choose your preferred version or merge content");
            println!("   3. Save and exit your editor");
            println!("   4. Push the resolved version: {}", "ph sync push".bold());
            
            return Ok(());
        }
        _ => unreachable!(),
    }

    println!();
    println!(
        "✅ {} Conflict resolved using '{}' strategy",
        "Success!".green().bold(),
        resolution
    );
    
    println!(
        "⏱️  Sync resolve completed ({}ms)",
        start.elapsed().as_millis()
    );
    Ok(())
}

async fn handle_sync_bidirectional(
    storage: &Storage,
    client: &RegistryClient,
    start: Instant,
) -> Result<()> {
    println!("🔄 Starting bidirectional sync...");
    println!();

    // Phase 1: Check sync status first
    println!("📊 Phase 1: Analyzing sync status...");
    
    let local_prompts = storage.list_prompts()?;
    let mut cloud_prompts = Vec::new();
    
    // Get cloud prompts
    match client.get("/api/prompts").await {
        Ok(response) => {
            if response.status().is_success() {
                match response.json::<serde_json::Value>().await {
                    Ok(cloud_data) => {
                        if let Some(cloud_array) = cloud_data["prompts"].as_array() {
                            cloud_prompts = cloud_array
                                .iter()
                                .filter_map(|p| p["name"].as_str().map(|s| s.to_string()))
                                .collect();
                        }
                    }
                    Err(e) => {
                        return Err(anyhow::anyhow!("Failed to parse cloud prompts: {}", e));
                    }
                }
            } else {
                return Err(anyhow::anyhow!(
                    "Failed to fetch cloud prompts: {}",
                    response.status()
                ));
            }
        }
        Err(e) => {
            return Err(anyhow::anyhow!("Request failed: {}", e));
        }
    }

    let mut pending_push = Vec::new();
    let mut pending_pull = Vec::new();
    let mut conflicts = Vec::new();
    let mut synced = 0;

    // Check each local prompt
    for prompt_name in &local_prompts {
        if cloud_prompts.contains(prompt_name) {
            // Check for conflicts
            let url = format!("/api/prompts/{}", urlencoding::encode(prompt_name));
            match client.get(&url).await {
                Ok(response) => {
                    if response.status().is_success() {
                        match response.json::<serde_json::Value>().await {
                            Ok(cloud_data) => {
                                let (local_metadata, local_content) = storage.read_prompt(prompt_name)?;
                                let cloud_content = cloud_data["content"].as_str().unwrap_or("");
                                let cloud_description = cloud_data["description"].as_str().unwrap_or("");

                                let content_matches = local_content.trim() == cloud_content.trim();
                                let description_matches = local_metadata.description == cloud_description;

                                if content_matches && description_matches {
                                    synced += 1;
                                } else {
                                    conflicts.push(prompt_name.clone());
                                }
                            }
                            Err(_) => conflicts.push(prompt_name.clone()),
                        }
                    } else {
                        conflicts.push(prompt_name.clone());
                    }
                }
                Err(_) => conflicts.push(prompt_name.clone()),
            }
        } else {
            // Local only - needs push
            pending_push.push(prompt_name.clone());
        }
    }

    // Check for cloud-only prompts (need pull)
    for cloud_prompt in &cloud_prompts {
        if !local_prompts.contains(cloud_prompt) {
            pending_pull.push(cloud_prompt.clone());
        }
    }

    // Report status
    println!("   ✅ In sync: {} prompts", synced);
    println!("   📤 Need push: {} prompts", pending_push.len());
    println!("   📥 Need pull: {} prompts", pending_pull.len());
    println!("   ⚠️  Conflicts: {} prompts", conflicts.len());

    // Check if everything is already in sync
    if pending_push.is_empty() && pending_pull.is_empty() && conflicts.is_empty() {
        println!();
        println!("💚 {} All prompts are already in sync!", "Perfect!".green().bold());
        println!(
            "⏱️  Bidirectional sync completed ({}ms)",
            start.elapsed().as_millis()
        );
        return Ok(());
    }

    println!();

    // Phase 2: Handle conflicts first
    if !conflicts.is_empty() {
        println!("⚠️  Phase 2: Conflict resolution required");
        println!("   The following prompts have conflicts:");
        for conflict in &conflicts {
            println!("     - {}", conflict);
        }
        println!();
        println!("💡 {} Resolve conflicts manually:", "Action required:".yellow().bold());
        println!("   For each conflict, run: {}", "ph sync resolve <prompt> --resolution [local|cloud|manual]".bold());
        println!("   Then run {} again to continue sync", "ph sync".bold());
        println!();
        return Ok(());
    }

    // Phase 3: Push local-only prompts
    if !pending_push.is_empty() {
        println!("📤 Phase 3: Pushing {} local prompts to cloud...", pending_push.len());
        
        let mut push_success = 0;
        let mut push_errors = 0;

        for prompt_name in &pending_push {
            print!("   📤 Pushing '{}'... ", prompt_name);
            
            match storage.read_prompt(prompt_name) {
                Ok((metadata, content)) => {
                    let payload = serde_json::json!({
                        "prompts": [{
                            "name": prompt_name,
                            "content": content,
                            "description": metadata.description,
                            "tags": metadata.tags.unwrap_or_default()
                        }],
                        "force": false
                    });

                    match client.post("/api/sync/push", &payload).await {
                        Ok(response) => {
                            if response.status().is_success() {
                                push_success += 1;
                                println!("✅");
                            } else {
                                push_errors += 1;
                                println!("❌ ({})", response.status());
                            }
                        }
                        Err(e) => {
                            push_errors += 1;
                            println!("❌ ({})", e);
                        }
                    }
                }
                Err(e) => {
                    push_errors += 1;
                    println!("❌ ({})", e);
                }
            }
        }

        println!("   📤 Push results: {} success, {} errors", push_success, push_errors);
        println!();
    }

    // Phase 4: Pull cloud-only prompts
    if !pending_pull.is_empty() {
        println!("📥 Phase 4: Pulling {} cloud prompts to local...", pending_pull.len());
        
        let mut pull_success = 0;
        let mut pull_errors = 0;

        for prompt_name in &pending_pull {
            print!("   📥 Pulling '{}'... ", prompt_name);
            
            let url = format!("/api/prompts/{}", urlencoding::encode(prompt_name));
            match client.get(&url).await {
                Ok(response) => {
                    if response.status().is_success() {
                        match response.json::<serde_json::Value>().await {
                            Ok(cloud_data) => {
                                let cloud_content = cloud_data["content"].as_str().unwrap_or("");
                                let cloud_description = cloud_data["description"].as_str().unwrap_or("");
                                let cloud_tags: Vec<String> = cloud_data["tags"]
                                    .as_array()
                                    .map(|arr| {
                                        arr.iter()
                                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                            .collect()
                                    })
                                    .unwrap_or_default();

                                let metadata = PromptMetadata {
                                    id: prompt_name.clone(),
                                    description: cloud_description.to_string(),
                                    tags: if cloud_tags.is_empty() { None } else { Some(cloud_tags) },
                                    created_at: Some(chrono::Utc::now().to_rfc3339()),
                                    updated_at: None,
                                    version: None,
                                    git_hash: None,
                                    parent_version: None,
                                };

                                match storage.write_prompt(prompt_name, &metadata, cloud_content) {
                                    Ok(_) => {
                                        pull_success += 1;
                                        println!("✅");
                                    }
                                    Err(e) => {
                                        pull_errors += 1;
                                        println!("❌ ({})", e);
                                    }
                                }
                            }
                            Err(e) => {
                                pull_errors += 1;
                                println!("❌ ({})", e);
                            }
                        }
                    } else {
                        pull_errors += 1;
                        println!("❌ ({})", response.status());
                    }
                }
                Err(e) => {
                    pull_errors += 1;
                    println!("❌ ({})", e);
                }
            }
        }

        println!("   📥 Pull results: {} success, {} errors", pull_success, pull_errors);
        println!();
    }

    // Final summary
    println!("✅ {} Bidirectional sync completed!", "Success!".green().bold());
    
    let total_operations = pending_push.len() + pending_pull.len();
    if total_operations > 0 {
        println!("   📊 Total operations: {}", total_operations);
        println!("   📤 Pushed: {} prompts", pending_push.len());
        println!("   📥 Pulled: {} prompts", pending_pull.len());
    }
    
    println!(
        "⏱️  Bidirectional sync completed ({}ms)",
        start.elapsed().as_millis()
    );
    Ok(())
}

async fn handle_sync_verify(
    storage: &Storage,
    client: &RegistryClient,
    prompt: Option<&str>,
    verbose: bool,
    start: Instant,
) -> Result<()> {
    println!("🔍 Verifying sync integrity by checking actual database state...");

    let prompts_to_verify = if let Some(prompt_name) = prompt {
        // Verify specific prompt
        let resolved_name = resolve_prompt_name(storage, prompt_name)?;
        vec![resolved_name]
    } else {
        // Verify all prompts
        storage.list_prompts()?
    };

    if prompts_to_verify.is_empty() {
        println!("No prompts to verify");
        return Ok(());
    }

    let mut verified = 0;
    let mut errors = 0;
    let mut missing = 0;
    let mut out_of_sync = 0;

    for prompt_name in &prompts_to_verify {
        if verbose {
            println!("🔍 Verifying '{}'...", prompt_name);
        }

        // Read local prompt
        let local_result = storage.read_prompt(prompt_name);
        let (local_metadata, local_content) = match local_result {
            Ok((metadata, content)) => (metadata, content),
            Err(e) => {
                if verbose {
                    println!("❌ Local read error for '{}': {}", prompt_name, e);
                }
                errors += 1;
                continue;
            }
        };

        // Check if prompt exists in cloud
        let url = format!("/api/prompts/{}", urlencoding::encode(prompt_name));
        match client.get(&url).await {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    // Parse cloud response
                    match response.json::<serde_json::Value>().await {
                        Ok(cloud_data) => {
                            let cloud_content = cloud_data["content"].as_str().unwrap_or("");
                            let cloud_description =
                                cloud_data["description"].as_str().unwrap_or("");

                            // Compare content
                            let content_matches = local_content.trim() == cloud_content.trim();
                            let description_matches =
                                local_metadata.description == cloud_description;

                            if content_matches && description_matches {
                                verified += 1;
                                if verbose {
                                    println!("✅ '{}' - In sync", prompt_name);
                                }
                            } else {
                                out_of_sync += 1;
                                if verbose {
                                    println!("⚠️  '{}' - Out of sync", prompt_name);
                                    if !content_matches {
                                        println!("   📝 Content differs");
                                    }
                                    if !description_matches {
                                        println!("   📄 Description differs");
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            errors += 1;
                            if verbose {
                                println!(
                                    "❌ '{}' - Failed to parse cloud response: {}",
                                    prompt_name, e
                                );
                            }
                        }
                    }
                } else if status.as_u16() == 404 {
                    missing += 1;
                    if verbose {
                        println!("⚠️  '{}' - Not found in cloud", prompt_name);
                    }
                } else {
                    errors += 1;
                    if verbose {
                        println!("❌ '{}' - Cloud API error: {}", prompt_name, status);
                    }
                }
            }
            Err(e) => {
                errors += 1;
                if verbose {
                    println!("❌ '{}' - Request failed: {}", prompt_name, e);
                }
            }
        }
    }

    // Summary
    println!();
    println!(
        "📊 {} {}",
        "Sync Verification Summary:".green().bold(),
        "Database integrity check completed".green()
    );

    if verified > 0 {
        println!("   ✅ In sync: {} prompt(s)", verified);
    }
    if missing > 0 {
        println!(
            "   ⚠️  Missing from cloud: {} prompt(s) (may need to push)",
            missing
        );
    }
    if out_of_sync > 0 {
        println!(
            "   ⚠️  Out of sync: {} prompt(s) (may need to sync)",
            out_of_sync
        );
    }
    if errors > 0 {
        println!("   ❌ Verification errors: {} prompt(s)", errors);
    }

    let total = verified + missing + out_of_sync + errors;
    let success_rate = if total > 0 {
        (verified * 100) / total
    } else {
        100
    };
    println!("   📊 Success rate: {}%", success_rate);

    if success_rate == 100 && missing == 0 && out_of_sync == 0 {
        println!("   💚 All prompts are properly synced!");
    } else if missing > 0 || out_of_sync > 0 {
        println!(
            "   💡 Consider running: {} or {} to resolve differences",
            "ph sync push".bold(),
            "ph sync pull".bold()
        );
    }

    println!(
        "⏱️  Sync verification completed ({}ms)",
        start.elapsed().as_millis()
    );
    Ok(())
}

// =============================================================================
// Bidirectional File Sync Handlers
// =============================================================================

async fn handle_sync_file(
    storage: &Storage,
    path: &str,
    name: Option<&str>,
    force: bool,
    start: Instant,
) -> Result<()> {
    println!("🔄 Creating bidirectional sync for file '{}'...", path);
    
    let sync_manager = SimpleSyncManager::new(storage.clone())?;
    
    // Determine prompt name
    let prompt_name = match name {
        Some(name) => name.to_string(),
        None => {
            // Extract filename without extension
            let path_obj = std::path::Path::new(path);
            let filename = path_obj.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("prompt");
            filename.to_string()
        }
    };
    
    let local_path = if std::path::Path::new(path).is_absolute() {
        std::path::PathBuf::from(path)
    } else {
        std::env::current_dir()?.join(path)
    };
    
    // Handle force flag
    if local_path.exists() && !force {
        return Err(anyhow::anyhow!(
            "File already exists at {:?}. Use --force to overwrite.",
            local_path
        ));
    }
    
    match sync_manager.sync_prompt(&prompt_name, Some(local_path.clone())) {
        Ok(created_path) => {
            println!("✅ Created bidirectional sync:");
            println!("   📁 Local file: {:?}", created_path);
            println!("   📦 PromptHive: {}", prompt_name);
            println!("   🔄 Status: Synced");
        }
        Err(e) => {
            return Err(e);
        }
    }
    
    println!(
        "⏱️  Sync file completed ({}ms)",
        start.elapsed().as_millis()
    );
    Ok(())
}

// =============================================================================
// Advanced sync functions temporarily disabled
// =============================================================================
// 
// The following functions have been temporarily disabled while we establish
// the basic sync functionality. They will be re-enabled once the full
// SyncManager integration is complete:
//
// - handle_sync_dir: Sync entire directory of prompts
// - handle_unsync: Remove bidirectional sync relationship  
// - handle_file_status: Show detailed sync status for file sync
// - handle_sync_repair: Repair broken sync relationships
// - handle_sync_conflicts: List all sync conflicts
// - handle_sync_watch: Watch files for changes and auto-sync
//
// For now, basic file sync is available via the `sync-file` command and
// the `ph new -s` flag for creating prompts with bidirectional sync.
