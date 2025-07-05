use anyhow::{Context, Result};
use colored::*;
use std::time::Instant;
use clap::Subcommand;

use super::common;
use crate::{RegistryClient, Storage};

#[derive(Subcommand)]
pub enum ImprovementCommands {
    /// Submit prompt for community or AI improvement
    Submit {
        /// Name of the prompt to improve
        prompt: String,
        /// Submit to community for crowdsourced improvements
        #[arg(long = "crowd")]
        crowd: bool,
        /// Get AI-powered enhancement suggestions
        #[arg(long = "ai")]
        ai: bool,
        /// Custom instructions for the improvement
        #[arg(short = 'i', long = "instructions")]
        instructions: Option<String>,
        /// Priority level for the improvement request (low, medium, high)
        #[arg(short = 'p', long = "priority", default_value = "medium")]
        priority: String,
    },
    /// Check status of an improvement request
    Status {
        /// Improvement request ID to check
        improvement_id: String,
    },
}

pub async fn handle_improvement_commands(
    storage: &Storage,
    action: &Option<ImprovementCommands>,
    start: Instant,
) -> Result<()> {
    match action {
        Some(ImprovementCommands::Submit {
            prompt,
            crowd,
            ai,
            instructions,
            priority,
        }) => {
            handle_improve(
                storage,
                prompt,
                *crowd,
                *ai,
                instructions.as_deref(),
                priority,
                start,
            )
            .await
        }
        Some(ImprovementCommands::Status { improvement_id }) => {
            handle_improvement_status(storage, improvement_id, start).await
        }
        None => {
            eprintln!("{}: Must specify a subcommand", "Error".red());
            eprintln!();
            eprintln!("Available subcommands:");
            eprintln!("  {} submit <prompt> --crowd|--ai  Submit prompt for improvement", "ph improve".green());
            eprintln!("  {} status <id>                  Check improvement status", "ph improve".green());
            eprintln!();
            eprintln!("Examples:");
            eprintln!("  {} improve submit api-design --ai --instructions \"Add error handling\"", "ph".green());
            eprintln!("  {} improve submit code-review --crowd --priority high", "ph".green());
            eprintln!("  {} improve status imp_abc123def456", "ph".green());
            std::process::exit(1);
        }
    }
}

pub async fn handle_improve(
    storage: &Storage,
    prompt: &str,
    crowd: bool,
    ai: bool,
    instructions: Option<&str>,
    priority: &str,
    start: Instant,
) -> Result<()> {
    // Check if user has API key
    let api_key = common::require_api_key("Prompt improvement")?;

    let registry_url = super::configuration::get_registry_url();
    let client = RegistryClient::new(registry_url).with_api_key(api_key);

    // Validate that either crowd or ai is specified
    if !crowd && !ai {
        eprintln!(
            "{}: Must specify either --crowd or --ai improvement method",
            "Error".red()
        );
        std::process::exit(1);
    }

    // Validate priority
    let valid_priorities = ["low", "medium", "high"];
    if !valid_priorities.contains(&priority) {
        eprintln!(
            "{}: Priority must be one of: {}",
            "Error".red(),
            valid_priorities.join(", ")
        );
        std::process::exit(1);
    }

    // Resolve prompt name using fuzzy matching
    let resolved_name = common::resolve_prompt_name(storage, prompt)?;

    // Read the prompt
    let (metadata, body) = storage.read_prompt(&resolved_name)?;

    if crowd {
        handle_crowd_improvement(storage, &client, &resolved_name, &metadata.description, &body, instructions, priority, start).await
    } else if ai {
        handle_ai_improvement(storage, &client, &resolved_name, &metadata.description, &body, instructions, priority, start).await
    } else {
        unreachable!()
    }
}

async fn handle_crowd_improvement(
    _storage: &Storage,
    client: &RegistryClient,
    prompt_name: &str,
    description: &str,
    content: &str,
    instructions: Option<&str>,
    priority: &str,
    start: Instant,
) -> Result<()> {
    println!("🌍 {}", "Submitting to community for improvement...".blue());

    let response = client
        .submit_crowd_improvement(
            prompt_name,
            description,
            content,
            instructions,
            priority,
        )
        .await
        .context("Failed to submit crowd improvement request")?;

    println!(
        "✅ {} submitted for community improvement",
        prompt_name.green().bold()
    );
    println!();
    println!("🆔 {}: {}", "Request ID".cyan(), response.request_id.bold());
    println!("📊 {}: {}", "Status".cyan(), response.status.yellow());
    println!("⚡ {}: {}", "Priority".cyan(), priority.bold());

    if let Some(instructions_text) = instructions {
        println!("📝 {}: {}", "Instructions".cyan(), instructions_text);
    }

    if let Some(eta) = &response.estimated_completion {
        println!("⏰ {}: {}", "Estimated completion".yellow(), eta);
    }

    println!();
    println!("💡 {} Phase 2B Features:", "Community Enhancement Engine".bright_blue().bold());
    println!("   • Community experts will review and improve your prompt");
    println!("   • Multiple improvement suggestions will be generated");
    println!("   • You'll receive notifications when improvements are ready");
    println!("   • Attribution and credit system for community contributors");

    println!();
    println!(
        "📊 {}: Check status with {} improve status {}",
        "Track Progress".bright_blue(),
        "ph".green(),
        response.id
    );
    println!(
        "🔔 {}: You'll be notified via email when improvements are ready",
        "Notifications".bright_blue()
    );

    println!(
        "\n⏱️  {} ({}ms)",
        "Community improvement request submitted".green(),
        start.elapsed().as_millis()
    );

    Ok(())
}

async fn handle_ai_improvement(
    _storage: &Storage,
    client: &RegistryClient,
    prompt_name: &str,
    description: &str,
    content: &str,
    instructions: Option<&str>,
    priority: &str,
    start: Instant,
) -> Result<()> {
    println!("🤖 {}", "Generating AI-powered improvements...".blue());

    let response = client
        .submit_ai_improvement(
            prompt_name,
            description,
            content,
            instructions,
            priority,
        )
        .await
        .context("Failed to submit AI improvement request")?;

    println!(
        "✅ {} enhanced with AI suggestions",
        prompt_name.green().bold()
    );
    println!();
    println!("🆔 {}: {}", "Improvement ID".cyan(), response.improvement_id.bold());
    println!("🎯 {}: {:.1}%", "Confidence".cyan(), response.confidence_score * 100.0);

    if let Some(instructions_text) = instructions {
        println!("📝 {}: {}", "Instructions".cyan(), instructions_text);
    }

    println!();
    println!("✨ {}:", "AI-Enhanced Content".bright_green().bold());
    println!("{}", response.suggested_content);

    println!();
    println!("💡 {}:", "Key Improvements".bright_yellow().bold());
    for (i, improvement) in response.improvements.iter().enumerate() {
        println!("   {}. {}", i + 1, improvement);
    }

    println!();
    println!("🧠 {}:", "AI Reasoning".bright_blue().bold());
    println!("{}", response.reasoning);

    println!();
    println!("🎯 {} Phase 2B Features:", "AI Enhancement Engine".bright_purple().bold());
    println!("   • Advanced prompt engineering techniques applied");
    println!("   • Optimized for clarity, specificity, and effectiveness");
    println!("   • Context-aware improvements based on prompt type");
    println!("   • Performance predictions and optimization suggestions");

    println!();
    println!("💾 Next Steps:");
    println!(
        "   Apply improvements: {} improve apply {}",
        "ph".green(),
        response.improvement_id
    );
    println!(
        "   Save as new prompt: {} new {}-improved \"[improved content]\"",
        "ph".green(),
        prompt_name
    );
    println!(
        "   Compare versions: {} diff {} {}-improved",
        "ph".green(),
        prompt_name,
        prompt_name
    );

    println!(
        "\n⏱️  {} ({}ms)",
        "AI improvement generated".green(),
        start.elapsed().as_millis()
    );

    Ok(())
}

pub async fn handle_improvement_status(
    _storage: &Storage,
    improvement_id: &str,
    start: Instant,
) -> Result<()> {
    // Check if user has API key
    let api_key = common::require_api_key("Improvement status")?;

    let registry_url = super::configuration::get_registry_url();
    let client = RegistryClient::new(registry_url).with_api_key(api_key);

    println!("🔍 Checking improvement status...");

    let status = client
        .get_improvement_status(improvement_id)
        .await
        .context("Failed to get improvement status")?;

    println!();
    println!(
        "📊 {} {}",
        "Improvement Status".blue().bold(),
        improvement_id.bright_blue()
    );
    println!();

    println!(
        "🎯 {}: {}",
        "Prompt".bold(),
        status.prompt_name.bright_blue()
    );

    println!(
        "🔧 {}: {}",
        "Type".bold(),
        match status.improvement_type.as_str() {
            "crowd" => "Community Enhancement".green(),
            "ai" => "AI Enhancement".purple(),
            _ => status.improvement_type.normal(),
        }
    );

    println!(
        "📊 {}: {}",
        "Status".bold(),
        match status.status.as_str() {
            "queued" => status.status.yellow(),
            "processing" => status.status.blue(),
            "completed" => status.status.green(),
            "failed" => status.status.red(),
            _ => status.status.normal(),
        }
    );

    if !status.message.is_empty() {
        println!("💬 {}: {}", "Message".bold(), status.message);
    }

    if let Some(eta) = &status.estimated_completion {
        println!("⏰ {}: {}", "Estimated completion".yellow(), eta);
    }

    println!();

    match status.status.as_str() {
        "completed" => {
            println!("🎉 {}: Your improvement is ready!", "Success".green().bold());
            println!(
                "📥 View results: {} suggestions view {}",
                "ph".green(),
                improvement_id
            );
        }
        "processing" => {
            println!("⚙️  {}: Improvement in progress...", "Processing".blue().bold());
            println!("🔄 Check again in a few minutes for updates");
        }
        "queued" => {
            println!("⏳ {}: Request queued for processing", "Queued".yellow().bold());
            println!("📈 Current queue position determined by priority level");
        }
        "failed" => {
            println!("❌ {}: Improvement request failed", "Error".red().bold());
            println!("💡 Try submitting a new request with different parameters");
        }
        _ => {
            println!("❓ {}: Unknown status", "Unknown".bright_black().bold());
        }
    }

    println!(
        "\n⏱️  {} ({}ms)",
        "Status checked".green(),
        start.elapsed().as_millis()
    );

    Ok(())
}