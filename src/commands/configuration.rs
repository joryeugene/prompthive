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
        let words = editor_words(&editor)?;
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

#[cfg(not(windows))]
fn editor_words(editor: &str) -> Result<Vec<String>> {
    shlex::split(editor).context("EDITOR contains unmatched quotes")
}

#[cfg(windows)]
fn editor_words(editor: &str) -> Result<Vec<String>> {
    use std::ffi::{c_void, OsStr, OsString};
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn CommandLineToArgvW(command: *const u16, count: *mut i32) -> *mut *mut u16;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
    }
    let editor = editor.trim();
    anyhow::ensure!(!editor.is_empty(), "EDITOR must name a program");
    if std::path::Path::new(editor).is_file() {
        return Ok(vec![editor.to_owned()]);
    }
    let mut wide: Vec<u16> = OsStr::new(editor).encode_wide().collect();
    anyhow::ensure!(!wide.contains(&0), "EDITOR contains a NUL character");
    wide.push(0);
    let mut count = 0;
    // Windows owns the quoting rules and allocates one block for the argv array.
    // Copy every NUL-terminated argument before releasing that block exactly once.
    unsafe {
        let argv = CommandLineToArgvW(wide.as_ptr(), &mut count);
        if argv.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut words = Vec::new();
        for index in 0..count {
            let value = *argv.add(index as usize);
            let mut length = 0;
            while *value.add(length) != 0 {
                length += 1;
            }
            words.push(
                OsString::from_wide(std::slice::from_raw_parts(value, length))
                    .to_string_lossy()
                    .into_owned(),
            );
        }
        LocalFree(argv.cast());
        Ok(words)
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::editor_words;
    #[test]
    fn native_editor_paths_and_quoted_arguments_keep_backslashes() {
        assert_eq!(
            editor_words(r"C:\Windows\System32\notepad.exe").unwrap(),
            vec![r"C:\Windows\System32\notepad.exe"]
        );
        assert_eq!(
            editor_words(r#""C:\Program Files\Editor\editor.exe" --wait "C:\notes\draft.md""#)
                .unwrap(),
            vec![
                r"C:\Program Files\Editor\editor.exe",
                "--wait",
                r"C:\notes\draft.md"
            ]
        );
        assert!(editor_words(" ").is_err());
    }
}
pub fn get_editor_command_for_file(path: &std::path::Path) -> Result<std::process::Command> {
    let config = load_editor_config()?;
    let mut command = std::process::Command::new(config.command);
    command.args(config.args).arg(path);
    Ok(command)
}
