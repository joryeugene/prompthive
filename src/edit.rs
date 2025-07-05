use anyhow::Result;
use std::io::Write;
use tempfile::NamedTempFile;

use crate::commands::configuration::load_editor_config;

/// Open content in $EDITOR and return edited content
pub fn edit_content(content: &str) -> Result<String> {
    // Create a temporary file
    let mut temp_file = NamedTempFile::new()?;
    temp_file.write_all(content.as_bytes())?;
    temp_file.flush()?;

    // Get editor configuration
    let editor_config = load_editor_config().unwrap_or_else(|_| {
        // Fallback to environment or defaults
        use std::env;
        let command = env::var("EDITOR").unwrap_or_else(|_| {
            if cfg!(windows) {
                "notepad".to_string()
            } else {
                "vi".to_string()
            }
        });
        crate::commands::configuration::EditorConfig {
            command,
            args: vec![],
            preset: None,
        }
    });

    // Build command with args
    let mut cmd = std::process::Command::new(&editor_config.command);
    for arg in &editor_config.args {
        cmd.arg(arg);
    }
    cmd.arg(temp_file.path());

    // Open editor
    let status = cmd.status()?;

    if !status.success() {
        anyhow::bail!(
            "Editor '{}' exited with non-zero status",
            editor_config.command
        );
    }

    // Read edited content
    let edited = std::fs::read_to_string(temp_file.path())?;

    Ok(edited)
}
