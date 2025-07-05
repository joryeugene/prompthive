use anyhow::{Context, Result};
use clap::Subcommand;
use colored::*;
use regex::Regex;
use std::time::Instant;
// use toml;

use super::common;
use crate::{RegistryClient, Storage};

// Helper functions moved to common module

#[derive(Subcommand)]
pub enum SuggestionsCommands {
    /// List all suggestions for your shared prompts
    #[command(alias = "ls")]
    List {
        /// Show suggestions for specific shared prompt only
        #[arg(short = 's', long = "share")]
        share_id: Option<String>,
        /// Show only pending suggestions (default shows all)
        #[arg(long = "pending")]
        pending: bool,
    },
    /// View detailed information about a specific suggestion
    #[command(alias = "show")]
    View {
        /// Suggestion ID to view
        suggestion_id: String,
    },
    /// Accept a suggestion and apply the improvement
    Accept {
        /// Suggestion ID to accept
        suggestion_id: String,
        /// Apply suggestion to the original prompt
        #[arg(long = "apply")]
        apply: bool,
    },
    /// Reject a suggestion with optional reason
    Reject {
        /// Suggestion ID to reject
        suggestion_id: String,
        /// Optional reason for rejection
        #[arg(short = 'r', long = "reason")]
        reason: Option<String>,
    },
}

pub async fn handle_share(
    storage: &Storage,
    prompt: &str,
    public: bool,
    invite: Option<&str>,
    allow_suggestions: bool,
    expires: Option<u32>,
    start: Instant,
) -> Result<()> {
    // Check if user has API key (Pro feature)
    let api_key = common::require_api_key("Viral sharing")?;

    let registry_url = super::configuration::get_registry_url();
    let client = RegistryClient::new(registry_url).with_api_key(api_key);

    // Validate that either public or invite is specified
    if !public && invite.is_none() {
        eprintln!(
            "{}: Must specify either --public or --invite <emails>",
            "Error".red()
        );
        std::process::exit(1);
    }

    if public {
        handle_share_public(storage, &client, prompt, allow_suggestions, expires, start).await
    } else if let Some(emails) = invite {
        handle_share_invite(
            storage,
            &client,
            prompt,
            emails,
            allow_suggestions,
            expires,
            start,
        )
        .await
    } else {
        unreachable!()
    }
}

async fn handle_share_public(
    storage: &Storage,
    client: &RegistryClient,
    prompt: &str,
    allow_suggestions: bool,
    expires: Option<u32>,
    start: Instant,
) -> Result<()> {
    // Resolve prompt name using fuzzy matching
    let resolved_name = common::resolve_prompt_name(storage, prompt)?;

    // Read the prompt
    let (metadata, body) = storage.read_prompt(&resolved_name)?;

    println!("🌐 {}", "Creating public sharing link...".blue());

    let response = client
        .create_public_share(
            &resolved_name,
            &metadata.description,
            &body,
            allow_suggestions,
            expires,
        )
        .await
        .context("Failed to create public share")?;

    println!(
        "✅ {} created for '{}'",
        "Public link".green(),
        resolved_name.bold()
    );
    println!();
    println!("🔗 {}: {}", "Share URL".cyan(), response.share_url.bold());
    println!("🆔 {}: {}", "Share ID".bright_black(), response.share_id);

    if allow_suggestions {
        println!(
            "💡 {}: Viewers can suggest improvements",
            "Suggestions".yellow()
        );
    } else {
        println!("📖 {}: View-only (no suggestions)", "Mode".bright_black());
    }

    if let Some(exp) = expires {
        println!("⏰ {}: {} hours", "Expires in".yellow(), exp);
    } else {
        println!("♾️  {}: Never expires", "Duration".bright_black());
    }

    println!();
    println!(
        "💡 {}: Anyone with this link can view your prompt",
        "Note".bright_blue()
    );
    println!(
        "📊 {}: Track views and suggestions at the registry",
        "Analytics".bright_blue()
    );

    println!(
        "\n⏱️  {} ({}ms)",
        "Public share created".green(),
        start.elapsed().as_millis()
    );

    Ok(())
}

async fn handle_share_invite(
    storage: &Storage,
    client: &RegistryClient,
    prompt: &str,
    emails: &str,
    allow_suggestions: bool,
    expires: Option<u32>,
    start: Instant,
) -> Result<()> {
    // Parse email list
    let email_list: Vec<&str> = emails.split(',').map(|e| e.trim()).collect();
    if email_list.is_empty() {
        eprintln!("{}: No valid emails provided", "Error".red());
        std::process::exit(1);
    }

    // Validate email formats
    let email_regex = Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$").unwrap();
    let invalid_emails: Vec<&str> = email_list
        .iter()
        .filter(|email| !email_regex.is_match(email))
        .cloned()
        .collect();
    if !invalid_emails.is_empty() {
        eprintln!(
            "{}: Invalid email format(s): {}",
            "Error".red(),
            invalid_emails.join(", ")
        );
        std::process::exit(1);
    }

    // Resolve prompt name using fuzzy matching
    let resolved_name = common::resolve_prompt_name(storage, prompt)?;

    // Read the prompt
    let (metadata, body) = storage.read_prompt(&resolved_name)?;

    println!("📧 {}", "Creating private sharing invitations...".blue());

    let response = client
        .create_invite_share(
            &resolved_name,
            &metadata.description,
            &body,
            &email_list,
            allow_suggestions,
            expires,
        )
        .await
        .context("Failed to create invite share")?;

    println!(
        "✅ {} created for '{}'",
        "Private invitations".green(),
        resolved_name.bold()
    );
    println!();
    println!("📧 {}: {}", "Invited".cyan(), email_list.join(", ").bold());
    println!("🆔 {}: {}", "Share ID".bright_black(), response.share_id);

    if allow_suggestions {
        println!(
            "💡 {}: Invitees can suggest improvements",
            "Suggestions".yellow()
        );
    } else {
        println!("📖 {}: View-only (no suggestions)", "Mode".bright_black());
    }

    if let Some(exp) = expires {
        println!("⏰ {}: {} hours", "Expires in".yellow(), exp);
    } else {
        println!("♾️  {}: Never expires", "Duration".bright_black());
    }

    println!();
    println!(
        "💌 {}: Invitation emails sent to all recipients",
        "Status".bright_blue()
    );
    println!(
        "📊 {}: Track engagement at the registry",
        "Analytics".bright_blue()
    );

    println!(
        "\n⏱️  {} ({}ms)",
        "Invitations sent".green(),
        start.elapsed().as_millis()
    );

    Ok(())
}

pub async fn handle_suggestions(
    storage: &Storage,
    action: &Option<SuggestionsCommands>,
    start: Instant,
) -> Result<()> {
    // Check if user has API key
    let api_key = common::require_api_key("Suggestion management")?;

    let registry_url = super::configuration::get_registry_url();
    let client = RegistryClient::new(registry_url).with_api_key(api_key);

    match action {
        Some(SuggestionsCommands::List { share_id, pending }) => {
            handle_suggestions_list(storage, &client, share_id.as_deref(), *pending, start).await
        }
        Some(SuggestionsCommands::View { suggestion_id }) => {
            handle_suggestions_view(storage, &client, suggestion_id, start).await
        }
        Some(SuggestionsCommands::Accept {
            suggestion_id,
            apply,
        }) => handle_suggestions_accept(storage, &client, suggestion_id, *apply, start).await,
        Some(SuggestionsCommands::Reject {
            suggestion_id,
            reason,
        }) => {
            handle_suggestions_reject(storage, &client, suggestion_id, reason.as_deref(), start)
                .await
        }
        None => {
            // Default action - list all suggestions
            handle_suggestions_list(storage, &client, None, false, start).await
        }
    }
}

async fn handle_suggestions_list(
    _storage: &Storage,
    client: &RegistryClient,
    share_id: Option<&str>,
    pending_only: bool,
    start: Instant,
) -> Result<()> {
    println!("📋 {}", "Fetching suggestions...".blue());

    let suggestions = match client.list_suggestions(share_id, pending_only).await {
        Ok(suggestions) => suggestions,
        Err(e) => {
            eprintln!("{}: Failed to fetch suggestions: {}", "Error".red(), e);
            std::process::exit(1);
        }
    };

    if suggestions.is_empty() {
        println!("📭 {}", "No suggestions found".yellow());
        if pending_only {
            println!(
                "💡 Try running {} to see all suggestions",
                "ph suggestions list".green()
            );
        }
        return Ok(());
    }

    println!();
    println!("📝 {} suggestions found:", suggestions.len());
    println!();

    for suggestion in &suggestions {
        let status_display = match suggestion.status.as_str() {
            "pending" => suggestion.status.yellow(),
            "accepted" => suggestion.status.green(),
            "rejected" => suggestion.status.red(),
            _ => suggestion.status.normal(),
        };

        println!(
            "  {} {} | {} | {}",
            "📝".cyan(),
            suggestion.id.bright_blue(),
            status_display,
            suggestion.shared_prompt_name.bold()
        );

        if !suggestion.suggestion_text.is_empty() {
            println!("     {}", suggestion.suggestion_text.dimmed());
        }

        if let Some(email) = &suggestion.suggested_by_email {
            println!("     By: {}", email.bright_black());
        }

        println!();
    }

    println!("💡 Commands:");
    println!(
        "   View suggestion: {} view <suggestion-id>",
        "ph suggestions".green()
    );
    println!(
        "   Accept: {} accept <suggestion-id> --apply",
        "ph suggestions".green()
    );
    println!(
        "   Reject: {} reject <suggestion-id>",
        "ph suggestions".green()
    );

    println!(
        "\n⏱️  {} ({}ms)",
        "Suggestions listed".green(),
        start.elapsed().as_millis()
    );

    Ok(())
}

async fn handle_suggestions_view(
    _storage: &Storage,
    client: &RegistryClient,
    suggestion_id: &str,
    start: Instant,
) -> Result<()> {
    println!("🔍 Fetching suggestion details...");

    let suggestion = match client.get_suggestion(suggestion_id).await {
        Ok(suggestion) => suggestion,
        Err(e) => {
            eprintln!("{}: Failed to fetch suggestion: {}", "Error".red(), e);
            std::process::exit(1);
        }
    };

    println!();
    println!(
        "📝 {} {}",
        "Suggestion".blue().bold(),
        suggestion.id.bright_blue()
    );
    println!();

    println!(
        "📋 {}: {}",
        "Status".bold(),
        match suggestion.status.as_str() {
            "pending" => suggestion.status.yellow(),
            "accepted" => suggestion.status.green(),
            "rejected" => suggestion.status.red(),
            _ => suggestion.status.normal(),
        }
    );

    println!(
        "🎯 {}: {}",
        "For prompt".bold(),
        suggestion.shared_prompt_name.bright_blue()
    );

    if let Some(email) = &suggestion.suggested_by_email {
        println!("👤 {}: {}", "Suggested by".bold(), email);
    }

    println!("📅 {}: {}", "Created".bold(), &suggestion.created_at);

    println!();

    if !suggestion.suggestion_text.is_empty() {
        println!("📄 {}:", "Summary".bright_blue().bold());
        println!("{}", suggestion.suggestion_text);
        println!();
    }

    if let Some(content) = &suggestion.improvement_content {
        println!("✨ {}:", "Improved Content".bright_green().bold());
        println!("{}", content);
        println!();
    }

    if !suggestion.suggestion_text.is_empty() {
        println!("💭 {}:", "Reasoning".bright_yellow().bold());
        println!("{}", suggestion.suggestion_text);
        println!();
    }

    if suggestion.status == "pending" {
        println!("🎯 Next Steps:");
        println!(
            "   Accept: {} accept {} --apply",
            "ph suggestions".green(),
            suggestion.id
        );
        println!(
            "   Reject: {} reject {} --reason \"reason\"",
            "ph suggestions".green(),
            suggestion.id
        );
    }

    println!(
        "\n⏱️  {} ({}ms)",
        "Suggestion viewed".green(),
        start.elapsed().as_millis()
    );

    Ok(())
}

async fn handle_suggestions_accept(
    _storage: &Storage,
    client: &RegistryClient,
    suggestion_id: &str,
    apply: bool,
    start: Instant,
) -> Result<()> {
    println!("✅ Accepting suggestion...");

    let result = match client.accept_suggestion(suggestion_id).await {
        Ok(result) => result,
        Err(e) => {
            eprintln!("{}: Failed to accept suggestion: {}", "Error".red(), e);
            std::process::exit(1);
        }
    };

    println!("✅ {}", "Suggestion accepted successfully".green());

    if apply && result.improvement_content.is_some() {
        println!("🔄 Applying improvement to local prompt...");

        // Apply the suggestion to the local prompt
        if let Some(content) = &result.improvement_content {
            let prompt_name = &result.shared_prompt_name;

            // Try to find and update the local prompt
            // Try to update the local prompt - simplified for now
            println!(
                "✅ {} would be updated locally",
                prompt_name.bright_blue().bold()
            );
            println!("💡 Feature: Apply improvement to local prompt");
            println!("📝 Improved content would be:");
            println!("{}", content);
        }
    } else if result.improvement_content.is_some() {
        println!(
            "📝 To apply improvements locally, run: {} accept {} --apply",
            "ph suggestions".green(),
            suggestion_id
        );
    }

    if let Some(email) = &result.suggested_by_email {
        println!(
            "📧 {}: {} will be notified of acceptance",
            "Notification".blue(),
            email
        );
    }

    println!(
        "\n⏱️  {} ({}ms)",
        "Suggestion accepted".green(),
        start.elapsed().as_millis()
    );

    Ok(())
}

async fn handle_suggestions_reject(
    _storage: &Storage,
    client: &RegistryClient,
    suggestion_id: &str,
    reason: Option<&str>,
    start: Instant,
) -> Result<()> {
    println!("❌ Rejecting suggestion...");

    let result = match client.reject_suggestion(suggestion_id, reason).await {
        Ok(result) => result,
        Err(e) => {
            eprintln!("{}: Failed to reject suggestion: {}", "Error".red(), e);
            std::process::exit(1);
        }
    };

    println!("❌ {}", "Suggestion rejected".red());

    if let Some(reason_text) = reason {
        println!("📝 {}: {}", "Reason".bold(), reason_text);
    }

    if let Some(email) = &result.suggested_by_email {
        println!(
            "📧 {}: {} will be notified of rejection",
            "Notification".blue(),
            email
        );
    }

    println!(
        "💡 {}: Constructive feedback helps improve future suggestions",
        "Tip".bright_blue()
    );

    println!(
        "\n⏱️  {} ({}ms)",
        "Suggestion rejected".red(),
        start.elapsed().as_millis()
    );

    Ok(())
}
