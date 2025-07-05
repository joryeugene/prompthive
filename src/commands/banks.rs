use anyhow::{Context, Result};
use clap::Subcommand;
use colored::*;
use std::time::Instant;
// use toml;
use urlencoding;

use super::common;
use crate::{RegistryClient, Storage};

#[derive(Subcommand)]
pub enum BankCommands {
    /// Create a new private prompt bank
    Create {
        /// Bank name
        name: String,
        /// Make bank private (Pro feature)
        #[arg(long = "private")]
        private: bool,
        /// Bank description
        #[arg(short = 'd', long = "description")]
        description: Option<String>,
    },
    /// Share private bank with user
    Share {
        /// Bank name to share
        bank: String,
        /// Email address to share with
        email: String,
        /// Permissions (read or write)
        #[arg(short = 'p', long = "permissions", default_value = "read")]
        permissions: String,
    },
    /// Remove user access from private bank
    Unshare {
        /// Bank name
        bank: String,
        /// Email address to remove
        email: String,
    },
    /// List bank members
    Members {
        /// Bank name
        bank: String,
    },
    /// Delete private bank
    Delete {
        /// Bank name to delete
        bank: String,
        /// Force deletion without confirmation
        #[arg(short = 'f', long = "force")]
        force: bool,
    },
    /// List all banks
    #[command(alias = "ls")]
    List {
        /// Show private banks only
        #[arg(long = "private")]
        private: bool,
    },
}

// Helper functions moved to common module

pub async fn handle_banks(
    _storage: &Storage,
    action: &Option<BankCommands>,
    start: Instant,
) -> Result<()> {
    // Check if user has API key (Pro feature required for private banks)
    let api_key = common::require_api_key("Private banks")?;

    let registry_url = super::configuration::get_registry_url();
    let client = RegistryClient::new(registry_url).with_api_key(api_key);

    match action {
        Some(BankCommands::Create {
            name,
            private,
            description,
        }) => handle_bank_create(&client, name, *private, description.as_deref(), start).await,
        Some(BankCommands::Share {
            bank,
            email,
            permissions,
        }) => handle_bank_share(&client, bank, email, permissions, start).await,
        Some(BankCommands::Unshare { bank, email }) => {
            handle_bank_unshare(&client, bank, email, start).await
        }
        Some(BankCommands::Members { bank }) => handle_bank_members(&client, bank, start).await,
        Some(BankCommands::Delete { bank, force }) => {
            handle_bank_delete(&client, bank, *force, start).await
        }
        Some(BankCommands::List { private }) => handle_bank_list(&client, *private, start).await,
        None => handle_bank_list(&client, false, start).await,
    }
}

async fn handle_bank_create(
    client: &RegistryClient,
    name: &str,
    private: bool,
    description: Option<&str>,
    start: Instant,
) -> Result<()> {
    if private {
        println!("🏦 Creating private bank '{}'...", name);
    } else {
        println!("🏦 Creating public bank '{}'...", name);
    }

    let payload = serde_json::json!({
        "name": name,
        "private": private,
        "description": description.unwrap_or("")
    });

    match client.post("/api/banks", &payload).await {
        Ok(response) => {
            if let Ok(data) = response.json::<serde_json::Value>().await {
                let bank_id = data
                    .get("bank_id")
                    .and_then(|id| id.as_str())
                    .unwrap_or("unknown");
                let bank_name = data.get("name").and_then(|n| n.as_str()).unwrap_or(name);
                let is_private = data
                    .get("private")
                    .and_then(|p| p.as_bool())
                    .unwrap_or(false);

                println!("✅ Bank created successfully!");
                println!("   📛 Name: {}", bank_name.bright_blue().bold());
                println!("   🆔 Bank ID: {}", bank_id);
                if is_private {
                    println!("   🔒 Type: Private (Pro feature)");
                } else {
                    println!("   🌍 Type: Public");
                }
                if let Some(desc) = description {
                    println!("   📝 Description: {}", desc);
                }

                println!();
                println!("Next steps:");
                if is_private {
                    println!(
                        "   • Share with team: {} share {} <email>",
                        "ph banks".dimmed(),
                        bank_name
                    );
                    println!(
                        "   • Add members: {} members {}",
                        "ph banks".dimmed(),
                        bank_name
                    );
                }
                println!(
                    "   • Create prompts: {} new {}/my-prompt",
                    "ph".dimmed(),
                    bank_name
                );
                println!("   • List banks: {} list", "ph banks".dimmed());
            } else {
                println!("❌ Invalid response format");
            }
        }
        Err(e) => {
            eprintln!("{}: Failed to create bank: {}", "Error".red(), e);
            std::process::exit(1);
        }
    }

    println!(
        "⏱️  Bank creation completed ({}ms)",
        start.elapsed().as_millis()
    );
    Ok(())
}

async fn handle_bank_share(
    client: &RegistryClient,
    bank: &str,
    email: &str,
    permissions: &str,
    start: Instant,
) -> Result<()> {
    println!("🤝 Sharing bank '{}' with {}...", bank, email);

    let payload = serde_json::json!({
        "bank_name": bank,
        "email": email,
        "permissions": permissions
    });

    match client.post("/api/banks/share", &payload).await {
        Ok(response) => {
            if let Ok(data) = response.json::<serde_json::Value>().await {
                println!("✅ Bank shared successfully!");
                println!("   🏦 Bank: {}", bank.bright_blue().bold());
                println!("   📧 Shared with: {}", email.bright_blue());
                println!("   🔐 Permissions: {}", permissions);

                if let Some(share_id) = data.get("share_id").and_then(|id| id.as_str()) {
                    println!("   🆔 Share ID: {}", share_id);
                }
            } else {
                println!("❌ Invalid response format");
            }
        }
        Err(e) => {
            eprintln!("{}: Failed to share bank: {}", "Error".red(), e);
        }
    }

    println!(
        "⏱️  Bank sharing completed ({}ms)",
        start.elapsed().as_millis()
    );
    Ok(())
}

async fn handle_bank_unshare(
    client: &RegistryClient,
    bank: &str,
    email: &str,
    start: Instant,
) -> Result<()> {
    println!("🚫 Removing {} access from bank '{}'...", email, bank);

    let payload = serde_json::json!({
        "bank_name": bank,
        "email": email
    });

    match client.post("/api/banks/unshare", &payload).await {
        Ok(response) => {
            if let Ok(data) = response.json::<serde_json::Value>().await {
                let removed = data
                    .get("removed")
                    .and_then(|r| r.as_bool())
                    .unwrap_or(false);

                if removed {
                    println!("✅ Access removed successfully!");
                    println!("   🏦 Bank: {}", bank.bright_blue().bold());
                    println!("   📧 User: {}", email.bright_blue());
                    println!("   🚫 {} can no longer access this bank", email);
                } else {
                    println!("ℹ️  User was not sharing this bank");
                    println!("   🏦 Bank: {}", bank);
                    println!("   📧 User: {}", email);
                }
            } else {
                println!("❌ Invalid response format");
            }
        }
        Err(e) => {
            eprintln!("{}: Failed to remove bank access: {}", "Error".red(), e);
        }
    }

    println!(
        "⏱️  Bank access removal completed ({}ms)",
        start.elapsed().as_millis()
    );
    Ok(())
}

async fn handle_bank_members(client: &RegistryClient, bank: &str, start: Instant) -> Result<()> {
    println!("👥 Listing members for bank '{}'...", bank);

    let query_string = format!("?bank={}", urlencoding::encode(bank));

    match client
        .get(&format!("/api/banks/members{}", query_string))
        .await
    {
        Ok(response) => {
            if let Ok(data) = response.json::<serde_json::Value>().await {
                let bank_name = data
                    .get("bank_name")
                    .and_then(|n| n.as_str())
                    .unwrap_or(bank);

                if let Some(members) = data.get("members").and_then(|m| m.as_array()) {
                    if members.is_empty() {
                        println!("👥 No members found for bank '{}'", bank_name);
                        println!(
                            "   Share with someone: {} share {} <email>",
                            "ph banks".dimmed(),
                            bank_name
                        );
                    } else {
                        println!(
                            "👥 Bank '{}' Members ({}):",
                            bank_name.bright_blue().bold(),
                            members.len()
                        );
                        println!();

                        for member in members {
                            let email = member
                                .get("email")
                                .and_then(|e| e.as_str())
                                .unwrap_or("Unknown");
                            let permissions = member
                                .get("permissions")
                                .and_then(|p| p.as_str())
                                .unwrap_or("read");
                            let role = member
                                .get("role")
                                .and_then(|r| r.as_str())
                                .unwrap_or("member");
                            let joined_at = member
                                .get("joined_at")
                                .and_then(|j| j.as_str())
                                .unwrap_or("");

                            let role_icon = match role {
                                "owner" => "👑",
                                "admin" => "🔧",
                                _ => "👤",
                            };

                            let permission_icon = match permissions {
                                "write" => "✏️",
                                "read" => "👁️",
                                _ => "❓",
                            };

                            println!(
                                "  {} {} {} ({})",
                                role_icon,
                                email.bright_blue(),
                                permission_icon,
                                permissions
                            );
                            if !joined_at.is_empty() {
                                println!("     Joined: {}", joined_at.dimmed());
                            }
                            println!();
                        }

                        println!("💡 Commands:");
                        println!(
                            "   • Remove member: {} unshare {} <email>",
                            "ph banks".dimmed(),
                            bank_name
                        );
                        println!(
                            "   • Share bank: {} share {} <email>",
                            "ph banks".dimmed(),
                            bank_name
                        );
                    }
                } else {
                    println!("⚠️  Unexpected response format");
                }
            } else {
                println!("❌ Failed to parse response");
            }
        }
        Err(e) => {
            eprintln!("{}: Failed to list bank members: {}", "Error".red(), e);
        }
    }

    println!(
        "⏱️  Bank members listing completed ({}ms)",
        start.elapsed().as_millis()
    );
    Ok(())
}

async fn handle_bank_delete(
    client: &RegistryClient,
    bank: &str,
    force: bool,
    start: Instant,
) -> Result<()> {
    if !force {
        println!(
            "⚠️  This will permanently delete bank '{}' and all its contents!",
            bank.red().bold()
        );
        println!("   Use {} to confirm deletion", "--force".yellow());
        return Ok(());
    }

    println!("🗑️  Deleting bank '{}'...", bank);

    let payload = serde_json::json!({
        "bank_name": bank,
        "confirm": true
    });

    match client.post("/api/banks/delete", &payload).await {
        Ok(response) => {
            if let Ok(data) = response.json::<serde_json::Value>().await {
                let deleted = data
                    .get("deleted")
                    .and_then(|d| d.as_bool())
                    .unwrap_or(false);
                let prompt_count = data
                    .get("prompts_deleted")
                    .and_then(|p| p.as_u64())
                    .unwrap_or(0);

                if deleted {
                    println!("✅ Bank deleted successfully!");
                    println!("   🏦 Bank: {}", bank.bright_blue().bold());
                    if prompt_count > 0 {
                        println!("   📝 Prompts deleted: {}", prompt_count);
                    }
                    println!("   ⚠️  This action cannot be undone");
                } else {
                    println!("❌ Failed to delete bank");
                    if let Some(error) = data.get("error").and_then(|e| e.as_str()) {
                        println!("   Error: {}", error);
                    }
                }
            } else {
                println!("❌ Invalid response format");
            }
        }
        Err(e) => {
            eprintln!("{}: Failed to delete bank: {}", "Error".red(), e);
        }
    }

    println!(
        "⏱️  Bank deletion completed ({}ms)",
        start.elapsed().as_millis()
    );
    Ok(())
}

async fn handle_bank_list(
    client: &RegistryClient,
    private_only: bool,
    start: Instant,
) -> Result<()> {
    if private_only {
        println!("🏦 Listing your private banks...");
    } else {
        println!("🏦 Listing all your banks...");
    }

    let query_string = if private_only { "?private=true" } else { "" };

    match client.get(&format!("/api/banks{}", query_string)).await {
        Ok(response) => {
            let response_text = response.text().await.context("Failed to read response text")?;
            
            match serde_json::from_str::<serde_json::Value>(&response_text) {
                Ok(data) => {
                    if let Some(banks) = data.get("banks").and_then(|b| b.as_array()) {
                    if banks.is_empty() {
                        if private_only {
                            println!("📋 No private banks found");
                            println!(
                                "   Create your first private bank: {} create my-bank --private",
                                "ph banks".dimmed()
                            );
                        } else {
                            println!("📋 No banks found");
                            println!(
                                "   Create your first bank: {} create my-bank",
                                "ph banks".dimmed()
                            );
                        }
                    } else {
                        let filter_text = if private_only { "Private " } else { "" };
                        println!("📋 Your {}Banks ({}):", filter_text, banks.len());
                        println!();

                        for bank in banks {
                            let name = bank
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or("Unknown");
                            let description = bank
                                .get("description")
                                .and_then(|d| d.as_str())
                                .unwrap_or("");
                            let is_private = bank
                                .get("private")
                                .and_then(|p| p.as_bool())
                                .unwrap_or(false);
                            let prompt_count = bank
                                .get("prompt_count")
                                .and_then(|c| c.as_u64())
                                .unwrap_or(0);
                            let member_count = bank
                                .get("member_count")
                                .and_then(|m| m.as_u64())
                                .unwrap_or(0);
                            let created_at = bank
                                .get("created_at")
                                .and_then(|c| c.as_str())
                                .unwrap_or("");

                            let privacy_icon = if is_private { "🔒" } else { "🌍" };
                            let privacy_text = if is_private { "Private" } else { "Public" };

                            println!(
                                "  {} {} ({})",
                                privacy_icon,
                                name.bright_blue().bold(),
                                privacy_text
                            );
                            if !description.is_empty() {
                                println!("     {}", description.dimmed());
                            }
                            println!(
                                "     {} prompts • {} members • Created {}",
                                prompt_count, member_count, created_at
                            );
                            println!();
                        }

                        println!("💡 Commands:");
                        println!("   • Create bank: {} create <name>", "ph banks".dimmed());
                        if !private_only {
                            println!(
                                "   • Create private: {} create <name> --private",
                                "ph banks".dimmed()
                            );
                        }
                        println!(
                            "   • Share bank: {} share <bank> <email>",
                            "ph banks".dimmed()
                        );
                        println!("   • List members: {} members <bank>", "ph banks".dimmed());
                    }
                } else {
                    // Check if this is a server error response
                    if let Some(error_msg) = data.get("error").and_then(|e| e.as_str()) {
                        println!("❌ Server error: {}", error_msg);
                        println!("The registry server encountered an internal error.");
                        println!("This may be a temporary issue - please try again later.");
                    } else {
                        println!("❌ Invalid response format");
                        println!("Expected response with 'banks' array, but received different structure.");
                    }
                }
                }
                Err(_parse_error) => {
                    println!("❌ Failed to parse server response");
                    println!("The registry server returned an invalid JSON response.");
                }
            }
        }
        Err(e) => {
            eprintln!("{}: Failed to list banks: {}", "Error".red(), e);
            std::process::exit(1);
        }
    }

    println!(
        "\n⏱️  Bank listing completed ({}ms)",
        start.elapsed().as_millis()
    );
    Ok(())
}
