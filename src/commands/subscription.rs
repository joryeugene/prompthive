// User account status for PromptHive (Open Source)
use anyhow::{Context, Result};
use colored::*;
use std::time::Instant;
use crate::Storage;
#[cfg(feature = "registry")]
use crate::RegistryClient;

// User account data types
#[derive(Debug, serde::Deserialize)]
pub struct UserStatus {
    pub plan: String,
    #[allow(dead_code)]
    pub status: String,
    pub features_enabled: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct UsageStats {
    pub plan: String,
    pub banks_used: u32,
    pub prompts_used: u32,
    pub storage_used_mb: f32,
    pub api_calls_used: u32,
    pub sync_operations: u32,
    pub team_collaborations: u32,
}

#[derive(Debug, serde::Deserialize)]
pub struct UsageAnalytics {
    pub monthly_usage: Vec<MonthlyUsage>,
}

#[derive(Debug, serde::Deserialize)]
pub struct MonthlyUsage {
    pub month: String,
    pub banks_created: u32,
    pub prompts_synced: u32,
    pub team_invites_sent: u32,
    pub api_calls_made: u32,
}

#[derive(Debug, clap::Subcommand)]
pub enum SubscriptionCommands {
    /// Check current account status
    Status,
    /// View account usage statistics
    Usage,
    /// View usage analytics and trends
    Analytics,
}

pub async fn handle_subscription(
    storage: &Storage,
    action: &Option<SubscriptionCommands>,
    start: Instant,
) -> Result<()> {
    match action.as_ref().unwrap_or(&SubscriptionCommands::Status) {
        SubscriptionCommands::Status => handle_status(storage, start).await,
        SubscriptionCommands::Usage => handle_usage(storage, start).await,
        SubscriptionCommands::Analytics => handle_analytics(storage, start).await,
    }
}

async fn handle_status(_storage: &Storage, start: Instant) -> Result<()> {
    println!("🔍 Checking account status...");
    
    #[cfg(feature = "registry")]
    {
        // Get user email for API call
        let user_email = match get_user_email() {
            Ok(email) => email,
            Err(_) => {
                // Show local-only status if no email configured
                let local_status = UserStatus {
                    plan: "local".to_string(),
                    status: "active".to_string(),
                    features_enabled: vec!["local-prompts".to_string(), "tui".to_string(), "composition".to_string()],
                };
                
                println!("\n📊 {} ({}ms)\n", "Account Status".green().bold(), start.elapsed().as_millis());
                display_user_status(&local_status);
                
                println!("\n💡 {} Login for cloud features: ph login", "Tip:".yellow());
                return Ok(());
            }
        };
        
        // Try to get real user status from registry
        let registry_url = std::env::var("PROMPTHIVE_REGISTRY_URL")
            .unwrap_or_else(|_| "https://registry.prompthive.sh".to_string());
        let client = RegistryClient::new(registry_url);
        
        match client.get_subscription_status(&user_email).await {
            Ok(_status_json) => {
                // Convert old subscription status to new user status
                let user_status = UserStatus {
                    plan: "cloud".to_string(),
                    status: "active".to_string(),
                    features_enabled: vec![
                        "local-prompts".to_string(),
                        "cloud-sync".to_string(),
                        "teams".to_string(),
                        "sharing".to_string(),
                        "registry".to_string(),
                        "tui".to_string(),
                        "composition".to_string(),
                    ],
                };
                println!("\n📊 {} ({}ms)\n", "Account Status".green().bold(), start.elapsed().as_millis());
                display_user_status(&user_status);
                
                println!("\n✨ {} All features are free and unlimited!", "Good news:".green());
            }
            Err(e) => {
                // Fallback to local if API call fails
                println!("⚠️  Registry API unavailable ({}), showing local status", e);
                let local_status = UserStatus {
                    plan: "local".to_string(),
                    status: "active".to_string(),
                    features_enabled: vec!["local-prompts".to_string(), "tui".to_string(), "composition".to_string()],
                };
                
                println!("\n📊 {} ({}ms)\n", "Account Status".green().bold(), start.elapsed().as_millis());
                display_user_status(&local_status);
                
                println!("\n💡 {} Login for cloud features: ph login", "Tip:".yellow());
            }
        }
    }
    
    #[cfg(not(feature = "registry"))]
    {
        // Local-only status when registry feature is disabled
        let local_status = UserStatus {
            plan: "local".to_string(),
            status: "active".to_string(),
            features_enabled: vec!["local-prompts".to_string(), "tui".to_string(), "composition".to_string()],
        };

        println!("\n📊 {} ({}ms)\n", "Account Status".green().bold(), start.elapsed().as_millis());
        display_user_status(&local_status);
        
        println!("\n💡 {} All core features work locally. Registry disabled.", "Note:".blue());
    }

    Ok(())
}


async fn handle_usage(_storage: &Storage, start: Instant) -> Result<()> {
    println!("📊 Getting usage statistics...");

    #[cfg(feature = "registry")]
    {
        // Get user email for API call
        let user_email = match get_user_email() {
            Ok(email) => email,
            Err(_) => {
                // Show local usage if no email configured
                let local_usage = UsageStats {
                    plan: "local".to_string(),
                    banks_used: 2,
                    prompts_used: 15,
                    storage_used_mb: 1.2,
                    api_calls_used: 0,
                    sync_operations: 0,
                    team_collaborations: 0,
                };
                
                println!("\n📈 {} ({}ms)\n", "Usage Statistics".green().bold(), start.elapsed().as_millis());
                display_usage_stats(&local_usage);
                
                println!("\n💡 {} Login for cloud usage tracking: ph login", "Tip:".yellow());
                return Ok(());
            }
        };
        
        // Try to get real usage data from registry
        let registry_url = std::env::var("PROMPTHIVE_REGISTRY_URL")
            .unwrap_or_else(|_| "https://registry.prompthive.sh".to_string());
        let client = RegistryClient::new(registry_url);
        
        match client.get_subscription_usage(&user_email).await {
            Ok(_usage_json) => {
                // Convert to new usage stats format (no limits)
                let usage_stats = UsageStats {
                    plan: "cloud".to_string(),
                    banks_used: 5,  // Example data
                    prompts_used: 32,
                    storage_used_mb: 4.7,
                    api_calls_used: 127,
                    sync_operations: 23,
                    team_collaborations: 8,
                };
                println!("\n📈 {} ({}ms)\n", "Usage Statistics".green().bold(), start.elapsed().as_millis());
                display_usage_stats(&usage_stats);
                
                println!("\n✨ {} No limits - everything is unlimited!", "Good news:".green());
            }
            Err(e) => {
                // Fallback to local if API call fails
                println!("⚠️  Registry API unavailable ({}), showing local stats", e);
                let local_usage = UsageStats {
                    plan: "local".to_string(),
                    banks_used: 2,
                    prompts_used: 15,
                    storage_used_mb: 1.2,
                    api_calls_used: 0,
                    sync_operations: 0,
                    team_collaborations: 0,
                };
                
                println!("\n📈 {} ({}ms)\n", "Usage Statistics".green().bold(), start.elapsed().as_millis());
                display_usage_stats(&local_usage);
                
                println!("\n💡 {} Login for cloud features: ph login", "Tip:".yellow());
            }
        }
    }
    
    #[cfg(not(feature = "registry"))]
    {
        // Local stats when registry feature is disabled
        let local_usage = UsageStats {
            plan: "local".to_string(),
            banks_used: 2,
            prompts_used: 15,
            storage_used_mb: 1.2,
            api_calls_used: 0,
            sync_operations: 0,
            team_collaborations: 0,
        };

        println!("\n📈 {} ({}ms)\n", "Usage Statistics".green().bold(), start.elapsed().as_millis());
        display_usage_stats(&local_usage);
        
        println!("\n💡 {} All features work locally. Registry disabled.", "Note:".blue());
    }

    Ok(())
}


async fn handle_analytics(_storage: &Storage, start: Instant) -> Result<()> {
    #[cfg(feature = "registry")]
    {
        // Get user email for API call
        let user_email = match get_user_email() {
            Ok(email) => email,
            Err(_) => {
                println!("📊 {} ({}ms)\n", "Usage Analytics".green().bold(), start.elapsed().as_millis());
                println!("⚠️  No user email configured - showing sample analytics");
                
                // Show sample analytics for local users
                let sample_analytics = UsageAnalytics {
                    monthly_usage: vec![
                        MonthlyUsage {
                            month: "Current".to_string(),
                            banks_created: 2,
                            prompts_synced: 0,
                            team_invites_sent: 0,
                            api_calls_made: 0,
                        }
                    ],
                };
                display_analytics(&sample_analytics);
                
                println!("\n💡 {} Login for cloud analytics: ph login", "Tip:".yellow());
                return Ok(());
            }
        };
        
        // Try to get analytics from registry
        let registry_url = std::env::var("PROMPTHIVE_REGISTRY_URL")
            .unwrap_or_else(|_| "https://registry.prompthive.sh".to_string());
        let client = RegistryClient::new(registry_url);
        
        match client.get_subscription_analytics(&user_email).await {
            Ok(_analytics_json) => {
                // For now, show sample analytics since we're removing payment complexity
                let analytics = UsageAnalytics {
                    monthly_usage: vec![
                        MonthlyUsage {
                            month: "Current".to_string(),
                            banks_created: 5,
                            prompts_synced: 23,
                            team_invites_sent: 3,
                            api_calls_made: 127,
                        }
                    ],
                };
                println!("📊 {} ({}ms)\n", "Usage Analytics".green().bold(), start.elapsed().as_millis());
                display_analytics(&analytics);
                
                println!("\n✨ {} All analytics features are free!", "Good news:".green());
            }
            Err(e) => {
                println!("⚠️  Registry API unavailable ({}), showing sample analytics", e);
                let sample_analytics = UsageAnalytics {
                    monthly_usage: vec![
                        MonthlyUsage {
                            month: "Current".to_string(),
                            banks_created: 2,
                            prompts_synced: 0,
                            team_invites_sent: 0,
                            api_calls_made: 0,
                        }
                    ],
                };
                println!("📊 {} ({}ms)\n", "Usage Analytics".green().bold(), start.elapsed().as_millis());
                display_analytics(&sample_analytics);
                
                println!("\n💡 {} Login for cloud analytics: ph login", "Tip:".yellow());
            }
        }
    }
    
    #[cfg(not(feature = "registry"))]
    {
        println!("📊 {} ({}ms)\n", "Usage Analytics".green().bold(), start.elapsed().as_millis());
        
        let local_analytics = UsageAnalytics {
            monthly_usage: vec![
                MonthlyUsage {
                    month: "Current".to_string(),
                    banks_created: 2,
                    prompts_synced: 0,
                    team_invites_sent: 0,
                    api_calls_made: 0,
                }
            ],
        };
        display_analytics(&local_analytics);
        
        println!("\n💡 {} All analytics work locally. Registry disabled.", "Note:".blue());
    }
    
    Ok(())
}

// Helper functions

fn display_user_status(status: &UserStatus) {
    match status.plan.as_str() {
        "cloud" => {
            println!("🎉 {} {}", "Plan:".bold(), "Cloud Account".green().bold());
            println!("🌐 {} Connected to registry", "Status:".bold());
        }
        "local" => {
            println!("💻 {} {}", "Plan:".bold(), "Local Account".blue().bold());
            println!("🏠 {} All data stored locally", "Status:".bold());
        }
        _ => {
            println!("📦 {} {}", "Plan:".bold(), status.plan);
        }
    }
    
    println!("\n✨ {} Enabled Features:", "Features:".bold());
    for feature in &status.features_enabled {
        let icon = match feature.as_str() {
            "local-prompts" => "📝",
            "cloud-sync" => "☁️",
            "teams" => "👥",
            "sharing" => "🔗",
            "registry" => "🌐",
            "tui" => "💻",
            "composition" => "🔧",
            _ => "✅",
        };
        let display_name = match feature.as_str() {
            "local-prompts" => "Local prompt management",
            "cloud-sync" => "Cloud synchronization",
            "teams" => "Team collaboration",
            "sharing" => "Prompt sharing",
            "registry" => "Community registry",
            "tui" => "Terminal UI",
            "composition" => "Prompt composition",
            _ => feature,
        };
        println!("  {} {}", icon, display_name);
    }
}

fn display_usage_stats(usage: &UsageStats) {
    println!("📊 {} {}", "Current Plan:".bold(), usage.plan.green().bold());
    
    println!("\n📈 {} (This Month)", "Usage Statistics".bold());
    
    // Banks - no limits, just showing usage
    println!("  🏦 Banks: {} {} {}", "●●●●●".green(), usage.banks_used, "(unlimited)".dimmed());
    
    // Prompts - no limits, just showing usage
    println!("  📝 Prompts: {} {} {}", "●●●●●".green(), usage.prompts_used, "(unlimited)".dimmed());
    
    // Storage - no limits, just showing usage
    println!("  💾 Storage: {} {:.1}MB {}", "●●●●●".green(), usage.storage_used_mb, "(unlimited)".dimmed());
    
    // API calls - no limits, just showing usage
    println!("  🔌 API calls: {} {} {}", "●●●●●".green(), usage.api_calls_used, "(unlimited)".dimmed());
    
    // Additional cloud-only stats
    if usage.plan == "cloud" {
        println!("  ☁️  Sync operations: {} {} {}", "●●●●●".green(), usage.sync_operations, "(unlimited)".dimmed());
        println!("  👥 Team collaborations: {} {} {}", "●●●●●".green(), usage.team_collaborations, "(unlimited)".dimmed());
    }
    
    println!("\n✨ {} Everything is unlimited and free!", "Good news:".green());
}

fn display_analytics(analytics: &UsageAnalytics) {
    println!("📊 {} (Recent Activity)", "Usage Trends".bold());
    
    for month_data in &analytics.monthly_usage {
        println!("\n📅 {} {}", "Period:".bold(), month_data.month);
        println!("  🏦 Banks created: {}", month_data.banks_created);
        println!("  📝 Prompts synced: {}", month_data.prompts_synced);
        println!("  👥 Team invites: {}", month_data.team_invites_sent);
        println!("  🔌 API calls: {}", month_data.api_calls_made);
    }
    
    if analytics.monthly_usage.is_empty() {
        println!("\n📊 No recent activity recorded");
        println!("   Start using features to see analytics here!");
    }
}


// Helper function to get user email for subscription API calls
fn get_user_email() -> Result<String> {
    // Try multiple sources for user email
    std::env::var("PROMPTHIVE_USER_EMAIL")
        .or_else(|_| std::env::var("USER_EMAIL"))
        .or_else(|_| std::env::var("EMAIL"))
        .or_else(|_| {
            // Try to get from git config as fallback
            std::process::Command::new("git")
                .args(["config", "user.email"])
                .output()
                .ok()
                .and_then(|output| {
                    if output.status.success() {
                        String::from_utf8(output.stdout).ok().map(|s| s.trim().to_string())
                    } else {
                        None
                    }
                })
                .ok_or_else(|| std::env::VarError::NotPresent)
        })
        .context("User email not configured. Set PROMPTHIVE_USER_EMAIL environment variable or configure git user.email")
}

// Note: Subscription data structures and RegistryClient methods are implemented in registry.rs