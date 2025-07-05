use anyhow::Result;
use colored::*;
use std::fs;
use std::time::Instant;

use crate::{init_telemetry, record_command_metric, Storage};

pub fn handle_version(
    storage: &Storage,
    name: &str,
    tag: &str,
    message: Option<&str>,
    start: Instant,
) -> Result<()> {
    let mut telemetry = Some(init_telemetry(storage.base_dir().clone()).ok()).flatten();

    // Find the prompt
    let prompt_path = storage.prompt_path(name);
    if !prompt_path.exists() {
        eprintln!("{}", "❌ Prompt not found".red());
        std::process::exit(1);
    }

    // Read current content
    let content = fs::read_to_string(&prompt_path)?;

    // Create version directory if it doesn't exist
    let versions_dir = prompt_path
        .parent()
        .unwrap()
        .join(format!("{}.versions", name));
    fs::create_dir_all(&versions_dir)?;

    // Generate Git hash of current content
    let git_hash = format!("{:x}", md5::compute(&content));

    // Check if this exact version already exists
    let version_file = versions_dir.join(format!("{}.md", tag));
    if version_file.exists() {
        eprintln!("{}", format!("❌ Version '{}' already exists", tag).red());
        std::process::exit(1);
    }

    // Create version metadata
    let timestamp = chrono::Utc::now().to_rfc3339();
    let version_content = format!(
        "---\nversion: {}\ngit_hash: {}\ncreated_at: {}\nmessage: {}\n---\n\n{}",
        tag,
        git_hash,
        timestamp,
        message.unwrap_or(""),
        content
    );

    // Write version file
    fs::write(&version_file, version_content)?;

    // Update original prompt metadata with version info
    if let Ok(mut metadata) = storage.read_prompt_metadata(name) {
        metadata.version = Some(tag.to_string());
        metadata.git_hash = Some(git_hash.clone());
        metadata.updated_at = Some(timestamp);
        let _ = storage.write_prompt_metadata(name, &metadata);
    }

    println!(
        "Created version '{}' for '{}' ({})",
        tag,
        name,
        &git_hash[..8]
    );
    if let Some(msg) = message {
        println!("Message: {}", msg);
    }

    record_command_metric(&mut telemetry, "version", start.elapsed(), true, None, None);
    Ok(())
}

pub fn handle_versions(storage: &Storage, name: &str, verbose: bool, start: Instant) -> Result<()> {
    let mut telemetry = Some(init_telemetry(storage.base_dir().clone()).ok()).flatten();

    // Check if prompt exists
    let prompt_path = storage.prompt_path(name);
    if !prompt_path.exists() {
        eprintln!("{}", "❌ Prompt not found".red());
        std::process::exit(1);
    }

    // Check for versions directory
    let versions_dir = prompt_path
        .parent()
        .unwrap()
        .join(format!("{}.versions", name));
    if !versions_dir.exists() {
        println!(
            "{}",
            format!("📝 No versions found for '{}'", name).yellow()
        );
        return Ok(());
    }

    // List version files
    let mut versions = Vec::new();
    for entry in fs::read_dir(&versions_dir)? {
        let entry = entry?;
        if let Some(name) = entry.file_name().to_str() {
            if name.ends_with(".md") {
                let version_name = name.trim_end_matches(".md");
                let content = fs::read_to_string(entry.path())?;

                // Parse metadata
                if let Some(metadata_end) = content.find("\n---\n") {
                    let metadata_section = &content[4..metadata_end]; // Skip initial ---
                    let mut version_info = std::collections::HashMap::<String, String>::new();

                    for line in metadata_section.lines() {
                        if let Some((key, value)) = line.split_once(": ") {
                            version_info.insert(key.trim().to_string(), value.trim().to_string());
                        }
                    }

                    versions.push((version_name.to_string(), version_info));
                }
            }
        }
    }

    if versions.is_empty() {
        println!(
            "{}",
            format!("📝 No versions found for '{}'", name).yellow()
        );
        return Ok(());
    }

    // Sort by creation date (newest first)
    versions.sort_by(|a, b| {
        let a_date = a.1.get("created_at").map(|s| s.as_str()).unwrap_or("");
        let b_date = b.1.get("created_at").map(|s| s.as_str()).unwrap_or("");
        b_date.cmp(a_date)
    });

    println!("{}", format!("📚 Version history for '{}'", name).bold());
    println!();

    for (version, info) in &versions {
        let hash = info
            .get("git_hash")
            .map(|s| s.as_str())
            .unwrap_or("unknown");
        let date = info
            .get("created_at")
            .map(|s| s.as_str())
            .unwrap_or("unknown");
        let message = info.get("message").map(|s| s.as_str()).unwrap_or("");

        if verbose {
            println!("{} {} ({})", "📌".cyan(), version.bold(), &hash[..8]);
            println!("   📅 {}", date.dimmed());
            if !message.is_empty() {
                println!("   💬 {}", message);
            }
            println!();
        } else {
            let msg_display = if message.is_empty() {
                String::new()
            } else {
                format!(" - {}", message)
            };
            println!(
                "{} {} ({}){}",
                "📌".cyan(),
                version.bold(),
                &hash[..8],
                msg_display
            );
        }
    }

    record_command_metric(
        &mut telemetry,
        "versions",
        start.elapsed(),
        true,
        None,
        None,
    );
    Ok(())
}

pub fn handle_rollback(
    storage: &Storage,
    name: &str,
    version: &str,
    backup: bool,
    start: Instant,
) -> Result<()> {
    let mut telemetry = Some(init_telemetry(storage.base_dir().clone()).ok()).flatten();

    // Check if prompt exists
    let prompt_path = storage.prompt_path(name);
    if !prompt_path.exists() {
        eprintln!("{}", "❌ Prompt not found".red());
        std::process::exit(1);
    }

    // Check if version exists
    let versions_dir = prompt_path
        .parent()
        .unwrap()
        .join(format!("{}.versions", name));
    let version_file = versions_dir.join(format!("{}.md", version));
    if !version_file.exists() {
        eprintln!("{}", format!("❌ Version '{}' not found", version).red());
        std::process::exit(1);
    }

    // Create backup if requested
    if backup {
        let backup_tag = format!("backup-{}", chrono::Utc::now().timestamp());
        handle_version(
            storage,
            name,
            &backup_tag,
            Some("Pre-rollback backup"),
            start,
        )?;
        println!(
            "{}",
            format!("💾 Created backup version '{}'", backup_tag).blue()
        );
    }

    // Read version content
    let version_content = fs::read_to_string(&version_file)?;

    // Extract the content (after metadata)
    if let Some(content_start) = version_content.find("\n---\n") {
        let content = &version_content[content_start + 5..]; // Skip \n---\n

        // Write to main prompt file
        fs::write(&prompt_path, content)?;

        // Update metadata
        if let Ok(mut metadata) = storage.read_prompt_metadata(name) {
            metadata.version = Some(version.to_string());
            metadata.updated_at = Some(chrono::Utc::now().to_rfc3339());
            let _ = storage.write_prompt_metadata(name, &metadata);
        }

        println!("Rolled back '{}' to version '{}'", name, version);
    } else {
        eprintln!("Invalid version file format");
        std::process::exit(1);
    }

    record_command_metric(
        &mut telemetry,
        "rollback",
        start.elapsed(),
        true,
        None,
        None,
    );
    Ok(())
}
