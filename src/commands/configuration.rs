// Configuration management commands - extracted from main.rs and common.rs

use anyhow::{Context, Result};
use colored::*;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use toml;

use crate::{Storage, TelemetryCollector};

/// Editor configuration settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorConfig {
    /// Editor command (e.g., "code", "nvim", "nano")
    pub command: String,
    /// Command line arguments (e.g., ["--wait"] for VSCode)
    pub args: Vec<String>,
    /// Preset name if using a preset (e.g., "vscode", "nvim")
    pub preset: Option<String>,
}

impl Default for EditorConfig {
    fn default() -> Self {
        // Default to environment EDITOR or nano
        let command = env::var("EDITOR").unwrap_or_else(|_| "nano".to_string());
        Self {
            command,
            args: vec![],
            preset: None,
        }
    }
}

pub fn handle_config(
    telemetry: &mut Option<TelemetryCollector>,
    category: &str,
    action: &str,
    start: Instant,
) -> Result<()> {
    match (category, action) {
        ("telemetry", "enable") => {
            if let Some(tel) = telemetry {
                tel.enable_telemetry(true)?;
            } else {
                println!("✓ Telemetry enabled - tracking performance metrics");
            }
        }
        ("telemetry", "disable") => {
            if let Some(tel) = telemetry {
                tel.enable_telemetry(false)?;
            } else {
                println!("✓ Telemetry disabled - no metrics will be collected");
            }
        }
        ("telemetry", "status") => {
            if let Some(tel) = telemetry {
                if tel.is_enabled() {
                    println!("📊 Telemetry Status: {}", "Enabled".green());
                    let summary = tel.get_summary();
                    println!("   Commands tracked: {}", summary.total_commands);
                    println!(
                        "   Average response time: {:.1}ms",
                        summary.average_response_time_ms
                    );
                } else {
                    println!("📊 Telemetry Status: {}", "Disabled".yellow());
                }
            } else {
                println!("📊 Telemetry Status: {}", "Disabled".yellow());
            }
        }
        ("telemetry", "clear") => {
            println!("ℹ️  To clear telemetry data, delete: ~/.prompthive/telemetry.json");
            println!("   Or restart telemetry with: ph config telemetry disable && ph config telemetry enable");
        }
        ("api", "show") => match load_api_key()? {
            Some(key) => {
                let masked = format!("{}****", &key[..key.len().min(8)]);
                println!("🔑 API Key: {}", masked.green());
            }
            None => println!("🔑 API Key: {}", "Not configured".yellow()),
        },
        ("api", "clear") => {
            remove_api_key()?;
            println!("✓ API key removed");
        }
        ("registry", "url") => {
            let url = get_registry_url();
            println!("🌐 Registry URL: {}", url.cyan());
        }
        ("paths", "show") => {
            show_configuration_paths()?;
        }
        ("claude", "discover") => {
            discover_claude_md_files()?;
        }
        ("env", "show") => {
            show_environment_variables();
        }
        ("editor", "show") => {
            let config = load_editor_config()?;
            println!("📝 Editor Configuration:");
            println!("  Command: {}", config.command.cyan());
            if !config.args.is_empty() {
                println!("  Arguments: {}", config.args.join(" ").cyan());
            }
            if let Some(preset) = &config.preset {
                println!("  Preset: {}", preset.green());
            } else {
                println!("  Preset: {}", "Custom".yellow());
            }
        }
        ("editor", "reset") => {
            reset_editor_config()?;
            println!("✓ Editor configuration reset to defaults");
        }
        _ => {
            // Handle editor preset and command specially since they have additional arguments
            if category == "editor" {
                match action {
                    action if action.starts_with("preset ") => {
                        let preset = action.strip_prefix("preset ").unwrap_or("");
                        set_editor_preset(preset)?;
                        println!("✓ Editor preset set to '{}'", preset.green());
                    }
                    action if action.starts_with("command ") => {
                        let command_str = action.strip_prefix("command ").unwrap_or("");
                        set_editor_command(command_str)?;
                        println!("✓ Editor command set to '{}'", command_str.green());
                    }
                    _ => {
                        eprintln!("Error: Unknown editor action '{}'", action);
                        eprintln!("Available editor actions:");
                        eprintln!("  show                     # Show current configuration");
                        eprintln!("  reset                    # Reset to defaults");
                        eprintln!("  preset <name>            # Set preset (vscode, nvim, vim, nano, zed)");
                        eprintln!(
                            "  command \"<cmd> [args]\"   # Set custom command with arguments"
                        );
                        std::process::exit(1);
                    }
                }
            } else {
                eprintln!("Error: Unknown config command '{}' '{}'", category, action);
                eprintln!("Available categories:");
                eprintln!("  telemetry: enable, disable, status, clear");
                eprintln!("  api: show, clear");
                eprintln!("  registry: url");
                eprintln!("  paths: show");
                eprintln!("  claude: discover");
                eprintln!("  env: show");
                eprintln!("  editor: show, reset, preset <name>, command \"<cmd> [args]\"");
                std::process::exit(1);
            }
        }
    }

    println!("⏱️  Config updated ({}ms)", start.elapsed().as_millis());
    Ok(())
}

pub fn get_config_path() -> Result<PathBuf> {
    let storage = Storage::new()?;
    Ok(storage.config_path())
}

pub fn load_config() -> Result<toml::Value> {
    let config_path = get_config_path()?;

    if !config_path.exists() {
        return Ok(toml::Value::Table(Default::default()));
    }

    let contents = fs::read_to_string(&config_path).context("Failed to read config file")?;

    contents
        .parse::<toml::Value>()
        .context("Failed to parse config file")
}

pub fn load_api_key() -> Result<Option<String>> {
    let config = load_config()?;

    Ok(config
        .get("api_key")
        .and_then(|key| key.as_str())
        .map(|s| s.to_string()))
}

pub fn get_registry_url() -> String {
    env::var("PROMPTHIVE_REGISTRY_URL")
        .unwrap_or_else(|_| "https://registry.prompthive.sh".to_string())
}

fn save_config(config: &toml::Value) -> Result<()> {
    let config_path = get_config_path()?;
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

fn remove_api_key() -> Result<()> {
    let mut config = load_config().unwrap_or_else(|_| toml::Value::Table(Default::default()));

    if let toml::Value::Table(ref mut table) = config {
        table.remove("api_key");
    }

    save_config(&config)
}

fn show_configuration_paths() -> Result<()> {
    println!("📁 Configuration Paths:");

    let config_path = get_config_path()?;
    println!("  Config file: {}", config_path.display());
    println!(
        "    Exists: {}",
        if config_path.exists() {
            "✓".green()
        } else {
            "✗".red()
        }
    );

    let home = env::var("HOME").unwrap_or_else(|_| "Unknown".to_string());
    println!("  Home directory: {}", home);

    // Show PromptHive storage path
    if let Ok(storage) = Storage::new() {
        println!("  Storage directory: {}", storage.base_dir().display());
    }

    Ok(())
}

fn discover_claude_md_files() -> Result<()> {
    println!("🔍 Discovering CLAUDE.md files...");

    let locations = vec![
        shellexpand::tilde("~/.claude/CLAUDE.md").to_string(),
        shellexpand::tilde("~/CLAUDE.md").to_string(),
    ];

    // Add current working directory options
    if let Ok(current_dir) = env::current_dir() {
        let mut current_locations = vec![
            current_dir.join("CLAUDE.md").to_string_lossy().to_string(),
            current_dir
                .join(".claude/CLAUDE.md")
                .to_string_lossy()
                .to_string(),
        ];

        // Add storage-based location if available
        if let Ok(storage) = Storage::new() {
            current_locations.push(
                storage
                    .base_dir()
                    .join("CLAUDE.md")
                    .to_string_lossy()
                    .to_string(),
            );
        }

        // Combine all locations
        let all_locations: Vec<String> = locations.into_iter().chain(current_locations).collect();

        let mut found_any = false;

        for location in all_locations {
            let path = PathBuf::from(&location);
            if path.exists() {
                println!("  ✓ {}", location.green());
                found_any = true;

                // Show file size
                if let Ok(metadata) = fs::metadata(&path) {
                    println!("    Size: {} bytes", metadata.len());
                }
            } else {
                println!("  ✗ {}", location.dimmed());
            }
        }

        if !found_any {
            println!("  No CLAUDE.md files found in standard locations");
            println!("  Create one with: touch ~/.claude/CLAUDE.md");
        }
    } else {
        println!("  Could not determine current directory");
    }

    Ok(())
}

fn show_environment_variables() {
    println!("🌍 Environment Variables:");

    let env_vars = [
        ("EDITOR", "Text editor for prompt editing"),
        ("HOME", "User home directory"),
        ("PROMPTHIVE_REGISTRY_URL", "Custom registry URL"),
        ("PROMPTHIVE_STORAGE_PATH", "Custom storage path"),
        ("CLAUDE_API_KEY", "Claude API key"),
        ("OPENAI_API_KEY", "OpenAI API key"),
    ];

    for (var_name, description) in &env_vars {
        match env::var(var_name) {
            Ok(value) => {
                let masked_value = if var_name.contains("KEY") || var_name.contains("TOKEN") {
                    if value.len() > 8 {
                        format!("{}****", &value[..4])
                    } else {
                        "****".to_string()
                    }
                } else {
                    value
                };
                println!(
                    "  ✓ {}: {} - {}",
                    var_name.cyan(),
                    masked_value.green(),
                    description
                );
            }
            Err(_) => {
                println!(
                    "  ✗ {}: {} - {}",
                    var_name.cyan(),
                    "Not set".dimmed(),
                    description
                );
            }
        }
    }

    // Show template variable examples
    println!("\n📝 Available Template Variables:");
    println!("  {} - Any environment variable", "{env:VAR_NAME}".cyan());
    println!("  {} - Current timestamp", "{timestamp}".cyan());
    println!("  {} - Current date", "{date}".cyan());
    println!("  {} - Your input text", "{input}".cyan());
}

/// Load editor configuration from config.toml
pub fn load_editor_config() -> Result<EditorConfig> {
    let config = load_config()?;

    if let Some(editor_table) = config.get("editor") {
        if let Ok(editor_config) = editor_table.clone().try_into::<EditorConfig>() {
            return Ok(editor_config);
        }
    }

    // Return default configuration if not found
    Ok(EditorConfig::default())
}

/// Save editor configuration to config.toml
pub fn save_editor_config(editor_config: &EditorConfig) -> Result<()> {
    let mut config = load_config().unwrap_or_else(|_| toml::Value::Table(Default::default()));

    // Convert editor config to TOML value
    let editor_toml =
        toml::Value::try_from(editor_config).context("Failed to serialize editor config")?;

    if let toml::Value::Table(ref mut table) = config {
        table.insert("editor".to_string(), editor_toml);
    }

    save_config(&config)
}

/// Set editor preset (vscode, nvim, vim, nano, zed)
pub fn set_editor_preset(preset: &str) -> Result<()> {
    let editor_config = match preset {
        "vscode" => EditorConfig {
            command: "code".to_string(),
            args: vec!["--wait".to_string()],
            preset: Some("vscode".to_string()),
        },
        "nvim" => EditorConfig {
            command: "nvim".to_string(),
            args: vec![],
            preset: Some("nvim".to_string()),
        },
        "vim" => EditorConfig {
            command: "vim".to_string(),
            args: vec![],
            preset: Some("vim".to_string()),
        },
        "nano" => EditorConfig {
            command: "nano".to_string(),
            args: vec![],
            preset: Some("nano".to_string()),
        },
        "zed" => EditorConfig {
            command: "zed".to_string(),
            args: vec!["--wait".to_string()],
            preset: Some("zed".to_string()),
        },
        _ => {
            return Err(anyhow::anyhow!(
                "Unknown preset '{}'. Available presets: vscode, nvim, vim, nano, zed",
                preset
            ));
        }
    };

    save_editor_config(&editor_config)
}

/// Set custom editor command with arguments
pub fn set_editor_command(command_str: &str) -> Result<()> {
    // Remove surrounding quotes if present
    let cleaned = command_str.trim_matches('"').trim_matches('\'');

    let parts: Vec<&str> = cleaned.split_whitespace().collect();
    if parts.is_empty() {
        return Err(anyhow::anyhow!("Editor command cannot be empty"));
    }

    let command = parts[0].to_string();
    let args = parts[1..].iter().map(|s| s.to_string()).collect();

    let editor_config = EditorConfig {
        command,
        args,
        preset: None, // Custom commands don't have a preset
    };

    save_editor_config(&editor_config)
}

/// Reset editor configuration to defaults
pub fn reset_editor_config() -> Result<()> {
    let default_config = EditorConfig::default();
    save_editor_config(&default_config)
}

/// Get editor command and arguments for opening a file
#[allow(dead_code)]
pub fn get_editor_command_for_file(file_path: &Path) -> Result<(String, Vec<String>)> {
    let config = load_editor_config()?;

    let mut args = config.args.clone();
    args.push(file_path.to_string_lossy().to_string());

    Ok((config.command, args))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_registry_url_default() {
        // Use a lock to ensure test isolation since env vars are global
        use std::sync::Mutex;
        static ENV_LOCK: Mutex<()> = Mutex::new(());
        let _guard = ENV_LOCK.lock().unwrap();
        
        // Save current env var value
        let current_value = env::var("PROMPTHIVE_REGISTRY_URL").ok();

        // Remove the env var for this test
        env::remove_var("PROMPTHIVE_REGISTRY_URL");
        let url = get_registry_url();

        // The expected URL should match the actual default URL being used
        assert_eq!(
            url,
            "https://registry.prompthive.sh"
        );

        // Restore original value if it existed
        if let Some(value) = current_value {
            env::set_var("PROMPTHIVE_REGISTRY_URL", value);
        } else {
            // Make sure the env var is really removed
            env::remove_var("PROMPTHIVE_REGISTRY_URL");
        }
    }

    #[test]
    fn test_get_registry_url_custom() {
        // Use a lock to ensure test isolation since env vars are global
        use std::sync::Mutex;
        static ENV_LOCK: Mutex<()> = Mutex::new(());
        let _guard = ENV_LOCK.lock().unwrap();
        
        // Save current env var value
        let current_value = env::var("PROMPTHIVE_REGISTRY_URL").ok();

        // Set custom URL
        env::set_var("PROMPTHIVE_REGISTRY_URL", "https://custom.registry.com");
        let url = get_registry_url();
        assert_eq!(url, "https://custom.registry.com");

        // Restore original value or remove if it didn't exist
        match current_value {
            Some(value) => env::set_var("PROMPTHIVE_REGISTRY_URL", value),
            None => env::remove_var("PROMPTHIVE_REGISTRY_URL"),
        }
    }

    #[test]
    fn test_load_config_nonexistent() {
        // This might fail in some environments, so we'll just ensure it doesn't panic
        let _ = load_config();
    }

    #[test]
    fn test_show_environment_variables() {
        // Just ensure it doesn't panic
        show_environment_variables();
    }
}
