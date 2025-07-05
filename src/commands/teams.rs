use anyhow::{Context, Result};
use clap::Subcommand;
use colored::*;
use std::time::Instant;
// use toml;
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};

use super::common;
use crate::{RegistryClient, Storage};

#[derive(Subcommand)]
pub enum TeamsCommands {
    /// List your teams
    #[command(alias = "ls")]
    List,
    /// Create a new team
    Create {
        /// Team name
        name: String,
        /// Team description
        #[arg(short = 'd', long = "description")]
        description: Option<String>,
    },
    /// Invite user to team
    Invite {
        /// Email address to invite
        email: String,
        /// Team name or ID
        team: String,
        /// Role for the invited user (member or admin)
        #[arg(short = 'r', long = "role", default_value = "member")]
        role: String,
    },
    /// Accept team invitation
    Accept {
        /// Invitation token from email
        token: String,
    },
    /// Share prompt with team
    Share {
        /// Prompt name to share
        prompt: String,
        /// Team name or ID
        team: String,
        /// Permissions (read or write)
        #[arg(short = 'p', long = "permissions", default_value = "read")]
        permissions: String,
    },
    /// Remove prompt from team sharing
    Unshare {
        /// Prompt name to unshare
        prompt: String,
        /// Team name or ID
        team: String,
    },
    /// List team prompts
    Prompts {
        /// Team name or ID
        team: String,
    },
}

// Helper functions moved to common module

// Temporary function - will be moved to common module later
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

                for prompt_name in &bank_prompts {
                    if let Some(score) = fuzzy.fuzzy_match(prompt_name, prompt) {
                        if score > best_score {
                            best_score = score;
                            best_match = Some(format!("{}/{}", bank, prompt_name));
                        }
                    }
                }

                if let Some(matched) = best_match {
                    return Ok(matched);
                }
            }
        }
    }

    // Regular prompt matching (no bank syntax)
    if storage.prompt_exists(query) {
        return Ok(query.to_string());
    }

    // Try fuzzy matching
    let prompts = storage.list_prompts()?;
    let fuzzy = SkimMatcherV2::default();
    let mut best_match = None;
    let mut best_score = 0;

    for prompt in &prompts {
        if let Some(score) = fuzzy.fuzzy_match(prompt, query) {
            if score > best_score {
                best_score = score;
                best_match = Some(prompt.clone());
            }
        }
    }

    if let Some(matched) = best_match {
        Ok(matched)
    } else {
        Err(anyhow::anyhow!("No prompt found matching '{}'", query))
    }
}

pub async fn handle_teams(
    storage: &Storage,
    action: &Option<TeamsCommands>,
    start: Instant,
) -> Result<()> {
    // Check if user has API key
    let api_key = common::require_api_key("Teams")?;

    let registry_url = super::configuration::get_registry_url();
    let client = RegistryClient::new(registry_url).with_api_key(api_key);

    match action {
        Some(TeamsCommands::List) => handle_teams_list(&client, start).await,
        Some(TeamsCommands::Create { name, description }) => {
            handle_teams_create(&client, name, description.as_deref(), start).await
        }
        Some(TeamsCommands::Invite { email, team, role }) => {
            handle_teams_invite(&client, email, team, role, start).await
        }
        Some(TeamsCommands::Accept { token }) => handle_teams_accept(&client, token, start).await,
        Some(TeamsCommands::Share {
            prompt,
            team,
            permissions,
        }) => handle_teams_share(storage, &client, prompt, team, permissions, start).await,
        Some(TeamsCommands::Unshare { prompt, team }) => {
            handle_teams_unshare(&client, prompt, team, start).await
        }
        Some(TeamsCommands::Prompts { team }) => handle_teams_prompts(&client, team, start).await,
        None => handle_teams_list(&client, start).await,
    }
}

async fn handle_teams_list(client: &RegistryClient, start: Instant) -> Result<()> {
    println!("👥 Listing your teams...");

    // Get teams from API
    let response = client
        .get("/api/teams")
        .await
        .context("Failed to get teams from server")?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "Teams list failed with status {}: {}",
            status,
            error_text
        ));
    }

    let result: serde_json::Value = response
        .json()
        .await
        .context("Failed to parse teams response")?;

    if let Some(teams) = result.get("teams").and_then(|t| t.as_array()) {
        if teams.is_empty() {
            println!("📋 No teams found");
            println!("   Create your first team with: ph teams create <name>");
        } else {
            println!("📋 Your Teams ({}):", teams.len());
            println!();

            for team in teams {
                let name = team
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("Unknown");
                let description = team
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("");
                let role = team
                    .get("role")
                    .and_then(|r| r.as_str())
                    .unwrap_or("member");
                let member_count = team
                    .get("member_count")
                    .and_then(|c| c.as_u64())
                    .unwrap_or(0);
                let created_at = team
                    .get("created_at")
                    .and_then(|c| c.as_str())
                    .unwrap_or("");

                let role_icon = match role {
                    "admin" => "👑",
                    "owner" => "🏆",
                    _ => "👤",
                };

                println!("  {} {} ({})", role_icon, name.bright_blue().bold(), role);
                if !description.is_empty() {
                    println!("     {}", description.dimmed());
                }
                println!("     {} members • Created {}", member_count, created_at);
                println!();
            }

            println!(
                "💡 Use {} to share prompts with teams",
                "ph teams share <prompt> <team>".dimmed()
            );
        }
    } else {
        println!("⚠️  Unexpected response format from server");
    }

    println!(
        "⏱️  Teams list completed ({}ms)",
        start.elapsed().as_millis()
    );
    Ok(())
}

async fn handle_teams_create(
    client: &RegistryClient,
    name: &str,
    description: Option<&str>,
    start: Instant,
) -> Result<()> {
    println!("👥 Creating team '{}'...", name);

    // Validate team name
    if name.trim().is_empty() {
        return Err(anyhow::anyhow!("Team name cannot be empty"));
    }

    if name.len() > 50 {
        return Err(anyhow::anyhow!(
            "Team name cannot be longer than 50 characters"
        ));
    }

    // Build create team payload
    let mut payload = serde_json::json!({
        "name": name.trim()
    });

    if let Some(desc) = description {
        payload["description"] = serde_json::Value::String(desc.to_string());
    }

    // Send create team request
    let response = client
        .post("/api/teams", &payload)
        .await
        .context("Failed to create team")?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "Team creation failed with status {}: {}",
            status,
            error_text
        ));
    }

    let result: serde_json::Value = response
        .json()
        .await
        .context("Failed to parse team creation response")?;

    // Extract team information from response
    let team_id = result
        .get("team_id")
        .and_then(|id| id.as_str())
        .unwrap_or("unknown");
    let team_name = result.get("name").and_then(|n| n.as_str()).unwrap_or(name);
    let invite_code = result.get("invite_code").and_then(|c| c.as_str());

    println!("✅ Team created successfully!");
    println!("   📛 Name: {}", team_name.bright_blue().bold());
    if let Some(desc) = description {
        println!("   📝 Description: {}", desc);
    }
    println!("   🆔 Team ID: {}", team_id);

    if let Some(code) = invite_code {
        println!("   🔗 Invite Code: {}", code.bright_green());
        println!("   💡 Share this code with team members to invite them");
    }

    println!();
    println!("Next steps:");
    println!(
        "   • Invite members: {} invite <email> {}",
        "ph teams".dimmed(),
        team_name
    );
    println!(
        "   • Share prompts: {} share <prompt> {}",
        "ph teams".dimmed(),
        team_name
    );
    println!(
        "   • List team prompts: {} prompts {}",
        "ph teams".dimmed(),
        team_name
    );

    println!(
        "⏱️  Team creation completed ({}ms)",
        start.elapsed().as_millis()
    );
    Ok(())
}

async fn handle_teams_invite(
    client: &RegistryClient,
    email: &str,
    team: &str,
    role: &str,
    start: Instant,
) -> Result<()> {
    println!("📧 Inviting {} to team '{}'...", email, team);

    // Validate email format (basic check)
    if !email.contains('@') || email.trim().is_empty() {
        return Err(anyhow::anyhow!("Invalid email address format"));
    }

    // Validate role
    if !["member", "admin"].contains(&role) {
        return Err(anyhow::anyhow!("Invalid role. Must be 'member' or 'admin'"));
    }

    // Build invite payload
    let payload = serde_json::json!({
        "email": email.trim(),
        "team_name": team,
        "role": role
    });

    // Send invite request
    let response = client
        .post("/api/teams/invite", &payload)
        .await
        .context("Failed to send team invitation")?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "Team invitation failed with status {}: {}",
            status,
            error_text
        ));
    }

    let result: serde_json::Value = response
        .json()
        .await
        .context("Failed to parse team invitation response")?;

    // Extract invitation information
    let invitation_id = result
        .get("invitation_id")
        .and_then(|id| id.as_str())
        .unwrap_or("unknown");
    let team_name = result
        .get("team_name")
        .and_then(|n| n.as_str())
        .unwrap_or(team);
    let invite_token = result.get("invitation_token").and_then(|t| t.as_str());
    let expires_at = result.get("expires_at").and_then(|e| e.as_str());

    println!("✅ Invitation sent successfully!");
    println!("   📧 Email: {}", email.bright_blue());
    println!("   👥 Team: {}", team_name.bright_blue().bold());
    println!("   👤 Role: {}", role);
    println!("   🆔 Invitation ID: {}", invitation_id);

    if let Some(token) = invite_token {
        println!("   🔗 Token: {}...", &token[..8.min(token.len())]);
    }

    if let Some(expiry) = expires_at {
        println!("   ⏰ Expires: {}", expiry);
    }

    println!();
    println!(
        "📬 {} will receive an email with instructions to join the team",
        email
    );
    println!(
        "   They can also use: {} accept <invitation-token>",
        "ph teams".dimmed()
    );

    println!(
        "⏱️  Team invitation completed ({}ms)",
        start.elapsed().as_millis()
    );
    Ok(())
}

async fn handle_teams_accept(_client: &RegistryClient, token: &str, start: Instant) -> Result<()> {
    println!("✅ Accepting team invitation...");

    // TODO: Implement actual invitation acceptance using the teams API
    // This would call client.post("/api/teams/accept-invite", payload).await

    println!("🔄 Invitation acceptance functionality coming soon!");
    println!("   Token: {}...", &token[..8.min(token.len())]);

    println!(
        "⏱️  Invitation acceptance completed ({}ms)",
        start.elapsed().as_millis()
    );
    Ok(())
}

async fn handle_teams_share(
    storage: &Storage,
    client: &RegistryClient,
    prompt: &str,
    team: &str,
    permissions: &str,
    start: Instant,
) -> Result<()> {
    println!("🤝 Sharing prompt '{}' with team '{}'...", prompt, team);

    // Resolve prompt name
    let resolved_name = resolve_prompt_name(storage, prompt)?;

    // Validate permissions
    if !["read", "write"].contains(&permissions) {
        return Err(anyhow::anyhow!(
            "Invalid permissions. Must be 'read' or 'write'"
        ));
    }

    // Read the prompt to get its content and metadata
    let (metadata, content) = storage
        .read_prompt(&resolved_name)
        .with_context(|| format!("Failed to read prompt '{}'", resolved_name))?;

    // Build share payload
    let payload = serde_json::json!({
        "prompt_name": resolved_name,
        "team_name": team,
        "permissions": permissions,
        "prompt_content": content,
        "description": metadata.description,
        "tags": metadata.tags.unwrap_or_default()
    });

    // Send share request
    let response = client
        .post("/api/teams/share", &payload)
        .await
        .context("Failed to share prompt with team")?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "Prompt sharing failed with status {}: {}",
            status,
            error_text
        ));
    }

    let result: serde_json::Value = response
        .json()
        .await
        .context("Failed to parse prompt sharing response")?;

    // Extract sharing information
    let share_id = result
        .get("share_id")
        .and_then(|id| id.as_str())
        .unwrap_or("unknown");
    let team_name = result
        .get("team_name")
        .and_then(|n| n.as_str())
        .unwrap_or(team);
    let prompt_name = result
        .get("prompt_name")
        .and_then(|p| p.as_str())
        .unwrap_or(&resolved_name);

    println!("✅ Prompt shared successfully!");
    println!("   📝 Prompt: {}", prompt_name.bright_blue().bold());
    println!("   👥 Team: {}", team_name.bright_blue().bold());
    println!("   🔐 Permissions: {}", permissions);
    println!("   🆔 Share ID: {}", share_id);

    println!();
    let permission_icon = if permissions == "write" {
        "✏️"
    } else {
        "👁️"
    };
    println!(
        "{} Team members can now {} this prompt",
        permission_icon,
        if permissions == "write" {
            "read and edit"
        } else {
            "read"
        }
    );

    if permissions == "write" {
        println!("⚠️  Write permissions allow team members to modify the prompt");
    }

    println!(
        "💡 Use {} to see all team prompts",
        format!("ph teams prompts {}", team_name).dimmed()
    );

    println!(
        "⏱️  Prompt sharing completed ({}ms)",
        start.elapsed().as_millis()
    );
    Ok(())
}

async fn handle_teams_unshare(
    client: &RegistryClient,
    prompt: &str,
    team: &str,
    start: Instant,
) -> Result<()> {
    println!("🚫 Removing prompt '{}' from team '{}'...", prompt, team);

    // Build unshare payload
    let payload = serde_json::json!({
        "prompt_name": prompt,
        "team_name": team
    });

    // Send unshare request
    let response = client
        .post("/api/teams/unshare", &payload)
        .await
        .context("Failed to unshare prompt from team")?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "Prompt unsharing failed with status {}: {}",
            status,
            error_text
        ));
    }

    let result: serde_json::Value = response
        .json()
        .await
        .context("Failed to parse prompt unsharing response")?;

    // Extract unsharing information
    let team_name = result
        .get("team_name")
        .and_then(|n| n.as_str())
        .unwrap_or(team);
    let prompt_name = result
        .get("prompt_name")
        .and_then(|p| p.as_str())
        .unwrap_or(prompt);
    let removed = result
        .get("removed")
        .and_then(|r| r.as_bool())
        .unwrap_or(false);

    if removed {
        println!("✅ Prompt unshared successfully!");
        println!("   📝 Prompt: {}", prompt_name.bright_blue().bold());
        println!("   👥 Team: {}", team_name.bright_blue().bold());
        println!("   🚫 Team members can no longer access this prompt");
    } else {
        println!("ℹ️  Prompt was not shared with this team");
        println!("   📝 Prompt: {}", prompt_name);
        println!("   👥 Team: {}", team_name);
    }

    println!(
        "⏱️  Prompt unsharing completed ({}ms)",
        start.elapsed().as_millis()
    );
    Ok(())
}

async fn handle_teams_prompts(client: &RegistryClient, team: &str, start: Instant) -> Result<()> {
    println!("📋 Listing prompts for team '{}'...", team);

    // Build query parameters with team name
    let query_string = format!("?team={}", urlencoding::encode(team));

    // Send request to get team prompts
    let response = client
        .get(&format!("/api/teams/prompts{}", query_string))
        .await
        .context("Failed to get team prompts from server")?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "Team prompts listing failed with status {}: {}",
            status,
            error_text
        ));
    }

    let result: serde_json::Value = response
        .json()
        .await
        .context("Failed to parse team prompts response")?;

    let team_name = result
        .get("team_name")
        .and_then(|n| n.as_str())
        .unwrap_or(team);

    if let Some(prompts) = result.get("prompts").and_then(|p| p.as_array()) {
        if prompts.is_empty() {
            println!("📋 No prompts shared with team '{}'", team_name);
            println!(
                "   Share a prompt with: {} share <prompt> {}",
                "ph teams".dimmed(),
                team_name
            );
        } else {
            println!(
                "📋 Team '{}' Prompts ({}):",
                team_name.bright_blue().bold(),
                prompts.len()
            );
            println!();

            for prompt in prompts {
                let name = prompt
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("Unknown");
                let description = prompt
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("");
                let permissions = prompt
                    .get("permissions")
                    .and_then(|p| p.as_str())
                    .unwrap_or("read");
                let shared_by = prompt
                    .get("shared_by")
                    .and_then(|s| s.as_str())
                    .unwrap_or("unknown");
                let shared_at = prompt
                    .get("shared_at")
                    .and_then(|s| s.as_str())
                    .unwrap_or("");
                let tags = prompt
                    .get("tags")
                    .and_then(|t| t.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
                    .unwrap_or_default();

                let permission_icon = match permissions {
                    "write" => "✏️",
                    "read" => "👁️",
                    _ => "❓",
                };

                println!(
                    "  {} {} ({})",
                    permission_icon,
                    name.bright_blue().bold(),
                    permissions
                );
                if !description.is_empty() {
                    println!("     {}", description.dimmed());
                }

                if !tags.is_empty() {
                    let tag_str = tags
                        .iter()
                        .map(|t| format!("#{}", t))
                        .collect::<Vec<_>>()
                        .join(" ");
                    println!("     {}", tag_str.bright_black());
                }

                println!("     Shared by {} • {}", shared_by, shared_at);
                println!();
            }

            println!("💡 Commands:");
            println!(
                "   • Install prompt locally: {} install <prompt>",
                "ph".dimmed()
            );
            println!(
                "   • Share new prompt: {} share <prompt> {}",
                "ph teams".dimmed(),
                team_name
            );
            println!(
                "   • Remove shared prompt: {} unshare <prompt> {}",
                "ph teams".dimmed(),
                team_name
            );
        }
    } else {
        println!("⚠️  Unexpected response format from server");
    }

    println!(
        "⏱️  Team prompts listing completed ({}ms)",
        start.elapsed().as_millis()
    );
    Ok(())
}
