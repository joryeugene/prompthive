use super::common;
use crate::{RegistryClient, Storage};
use anyhow::Result;
use clap::Subcommand;
use colored::*;
use std::time::Instant;
use urlencoding;
// use std::fs;
// use toml;
// use anyhow::Context;

// Helper functions moved to common module

#[derive(Subcommand)]
pub enum UserCommands {
    /// Show current user account information
    Me,
    /// Search for users by username
    Search { query: String },
}

pub async fn handle_users(
    _storage: &Storage,
    action: &Option<UserCommands>,
    start: Instant,
) -> Result<()> {
    // Check if user has API key
    let api_key = common::require_api_key("User commands")?;

    let registry_url = super::configuration::get_registry_url();
    let client = RegistryClient::new(registry_url).with_api_key(api_key);

    match action {
        Some(UserCommands::Me) => handle_users_me(&client, start).await,
        Some(UserCommands::Search { query }) => handle_users_search(&client, query, start).await,
        None => handle_users_me(&client, start).await,
    }
}

async fn handle_users_me(client: &RegistryClient, start: Instant) -> Result<()> {
    println!("👤 Getting your account information...");

    match client.get("/api/users/me").await {
        Ok(response) => {
            if let Ok(data) = response.json::<serde_json::Value>().await {
                let email = data["email"].as_str().unwrap_or("Unknown");
                let is_pro = data["is_pro"].as_bool().unwrap_or(false);
                let prompt_count = data["prompt_count"].as_u64().unwrap_or(0);
                let bank_count = data["bank_count"].as_u64().unwrap_or(0);

                let created_at = data["created_at"]
                    .as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| {
                        eprintln!("⚠️  Backend API missing created_at field");
                        "Not available".to_string()
                    });

                println!("\n📋 Your Account:");
                println!("  📧 Email: {}", email);
                println!(
                    "  💎 Plan: {}",
                    if is_pro {
                        "Pro ($5/month)".green()
                    } else {
                        "Free".yellow()
                    }
                );
                println!("  📅 Member since: {}", created_at);
                println!("  📝 Prompts: {}", prompt_count);
                println!("  🏦 Banks: {}", bank_count);

                if !is_pro {
                    println!("\n💡 Upgrade to Pro for:");
                    println!("  • Private prompt banks");
                    println!("  • Cloud sync & backup");
                    println!("  • Team collaboration");
                    println!("  • Priority support");
                }
            } else {
                println!("❌ Failed to parse account information");
            }
        }
        Err(e) => {
            eprintln!("{}: Failed to get account info: {}", "Error".red(), e);
            std::process::exit(1);
        }
    }

    println!(
        "\n⏱️  Account info retrieved ({}ms)",
        start.elapsed().as_millis()
    );
    Ok(())
}

async fn handle_users_search(client: &RegistryClient, query: &str, start: Instant) -> Result<()> {
    println!("🔍 Searching for users matching '{}'...", query);

    let url = format!("/api/users/search?q={}", urlencoding::encode(query));

    match client.get(&url).await {
        Ok(response) => {
            if let Ok(data) = response.json::<serde_json::Value>().await {
                if let Some(users) = data["users"].as_array() {
                    if users.is_empty() {
                        println!("📝 No users found matching '{}'", query);
                    } else {
                        println!("\n👥 Found {} user(s):", users.len());
                        for user in users {
                            let email = user["email"].as_str().unwrap_or("Unknown");
                            let display_name = user["display_name"].as_str().unwrap_or("");
                            let is_pro = user["is_pro"].as_bool().unwrap_or(false);

                            let plan_badge = if is_pro { "💎" } else { "⭐" };

                            if display_name.is_empty() {
                                println!("  {} {}", plan_badge, email);
                            } else {
                                println!("  {} {} ({})", plan_badge, display_name, email);
                            }
                        }
                    }
                } else {
                    println!("❌ Invalid response format");
                }
            } else {
                println!("❌ Failed to parse search results");
            }
        }
        Err(e) => {
            eprintln!("{}: Failed to search users: {}", "Error".red(), e);
            std::process::exit(1);
        }
    }

    println!(
        "\n⏱️  User search completed ({}ms)",
        start.elapsed().as_millis()
    );
    Ok(())
}
