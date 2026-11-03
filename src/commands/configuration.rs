//! Read only the retained editor setting. Never alter legacy account/config data.
use crate::Storage;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub preset: Option<String>,
}
pub fn load_editor_config() -> Result<EditorConfig> {
    if let Ok(editor) = std::env::var("EDITOR") {
        let words = shlex::split(&editor).context("EDITOR contains unmatched quotes")?;
        let (command, args) = words.split_first().context("EDITOR must name a program")?;
        return Ok(EditorConfig {
            command: command.clone(),
            args: args.to_vec(),
            preset: None,
        });
    }
    let path = Storage::new()?.base_dir().join("config.toml");
    if path.exists() {
        let config: toml::Value = std::fs::read_to_string(path)?.parse().map_err(|_| {
            anyhow::anyhow!(
                "invalid local config.toml; fix its TOML syntax (the file was unchanged)"
            )
        })?;
        if let Some(editor) = config.get("editor") {
            return editor.clone().try_into().map_err(|_| {
                anyhow::anyhow!(
                    "invalid editor configuration; check command and args (the file was unchanged)"
                )
            });
        }
    }
    Ok(EditorConfig {
        command: if cfg!(windows) { "notepad" } else { "vi" }.into(),
        args: vec![],
        preset: None,
    })
}
pub fn get_editor_command_for_file(path: &std::path::Path) -> Result<std::process::Command> {
    let config = load_editor_config()?;
    let mut command = std::process::Command::new(config.command);
    command.args(config.args).arg(path);
    Ok(command)
}
