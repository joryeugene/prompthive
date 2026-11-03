use crate::Storage;
use anyhow::{bail, Result};
use std::io::Write;

/// Edit a staged document. An unsuccessful editor never changes the store.
pub fn edit_content(content: &str) -> Result<String> {
    let mut staged = tempfile::Builder::new()
        .prefix("ph-edit-")
        .suffix(".md")
        .tempfile()?;
    staged.write_all(content.as_bytes())?;
    staged.flush()?;
    let status =
        crate::commands::configuration::get_editor_command_for_file(staged.path())?.status()?;
    if !status.success() {
        if status.code() == Some(130) {
            return Err(crate::commands::common::fail(
                130,
                "Editor cancelled; nothing was saved.",
            ));
        }
        bail!("editor exited unsuccessfully; the stored prompt was unchanged");
    }
    Ok(String::from_utf8(crate::limits::read_file(
        staged.path(),
        "edited document",
    )?)?)
}
pub fn edit_prompt(storage: &Storage, name: &str) -> Result<()> {
    let original = storage.read_raw(name)?;
    let text = std::str::from_utf8(&original)?;
    let edited = edit_content(text)?;
    storage.replace_raw_if_unchanged(name, edited.as_bytes(), &original)
}
