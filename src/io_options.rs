//! Destinations are exclusive and validated before any side effect.
use crate::{
    cli::Destinations,
    commands::common::{fail, usage, write_stdout},
    persistence, Clipboard, PromptMetadata, Storage,
};
use anyhow::Result;
use std::io::IsTerminal;
use std::path::Path;
#[derive(Clone, Debug, Default)]
pub struct IoOptions {
    pub destination: Destinations,
    pub clipboard: bool,
}
impl IoOptions {
    /// Describe the same destination emit will use, without touching it.
    pub fn confirmation(&self, label: &str) -> String {
        let d = &self.destination;
        if let Some(name) = &d.save {
            return format!("Save as {name}");
        }
        if let Some(name) = &d.append {
            return format!("Append to {name}");
        }
        if let Some(path) = &d.file {
            let path = if path.is_empty() {
                format!("{}.md", label.rsplit('/').next().unwrap_or("prompt"))
            } else {
                path.clone()
            };
            return format!("Write {path}");
        }
        if self.clipboard {
            return "Copy to clipboard".into();
        }
        if std::io::stdout().is_terminal() && !d.quiet {
            "Copy to clipboard; show text if unavailable".into()
        } else {
            "Print rendered text".into()
        }
    }

    /// Reject obvious new-document conflicts before launching an editor.
    pub fn preflight_new(&self, storage: &Storage, label: &str) -> Result<()> {
        let d = &self.destination;
        let default_save =
            !self.clipboard && d.save.is_none() && d.append.is_none() && d.file.is_none();
        if let Some(name) = d.save.as_deref().or(default_save.then_some(label)) {
            if storage.prompt_exists(name) {
                return Err(fail(
                    2,
                    format!("prompt {name:?} already exists; use ph e to edit"),
                ));
            }
            if !storage.snapshot_paths(name)?.is_empty() {
                return Err(fail(
                    2,
                    format!(
                        "prompt {name:?} has recovery history; restore it or choose a new name"
                    ),
                ));
            }
        }
        if let Some(name) = &d.append {
            crate::commands::common::resolve_prompt_name(storage, name)?;
        }
        if let Some(path) = &d.file {
            let filename = if path.is_empty() {
                format!("{}.md", label.rsplit('/').next().unwrap_or("prompt"))
            } else {
                path.clone()
            };
            match std::fs::symlink_metadata(&filename) {
                Ok(meta) => {
                    anyhow::ensure!(
                        meta.is_file(),
                        "export target is not a regular file: {filename}"
                    );
                    anyhow::ensure!(
                        d.force,
                        "export file already exists: {filename}; use --force to replace it"
                    );
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
    pub fn new(destination: Destinations, clipboard: bool) -> Result<Self> {
        let count = usize::from(destination.save.is_some())
            + usize::from(destination.append.is_some())
            + usize::from(destination.file.is_some())
            + usize::from(clipboard);
        if count > 1 {
            return Err(fail(
                2,
                "choose one destination: -s NAME, -a NAME, -f PATH, or clipboard",
            ));
        }
        if let Some(name) = &destination.save {
            Storage::validate_name(name).map_err(usage)?;
        }
        if let Some(name) = &destination.append {
            Storage::validate_name(name).map_err(usage)?;
        }
        if destination.force && destination.file.is_none() {
            return Err(fail(
                2,
                "--force applies only to an explicit file destination",
            ));
        }
        Ok(Self {
            destination,
            clipboard,
        })
    }
    pub fn emit(
        &self,
        storage: &Storage,
        content: &str,
        label: &str,
        metadata: Option<&PromptMetadata>,
    ) -> Result<()> {
        crate::limits::check_text(content, "output")?;
        let d = &self.destination;
        if let Some(name) = &d.save {
            if storage.prompt_exists(name) {
                return Err(fail(
                    2,
                    format!("prompt {name:?} already exists; choose a new name"),
                ));
            }
            let mut meta = metadata.cloned().unwrap_or_default();
            meta.id = name.clone();
            storage.create_prompt(name, &meta, content)?;
            self.status(&format!("Saved {name}."));
            return Ok(());
        }
        if let Some(query) = &d.append {
            let name = crate::commands::common::resolve_prompt_name(storage, query)?;
            let original = storage.read_raw(&name)?;
            let (meta, body) = storage.read_prompt(&name)?;
            let joined = if body.is_empty() {
                content.into()
            } else if content.is_empty() {
                body
            } else {
                let mut joined = body;
                crate::limits::push_str(&mut joined, "\n\n", "appended prompt")?;
                crate::limits::push_str(&mut joined, content, "appended prompt")?;
                joined
            };
            storage.replace_if_unchanged(&name, &meta, &joined, &original)?;
            self.status(&format!("Appended to {name}."));
            return Ok(());
        }
        if let Some(path) = &d.file {
            let filename = if path.is_empty() {
                format!("{}.md", label.rsplit('/').next().unwrap_or("prompt"))
            } else {
                path.clone()
            };
            persistence::atomic_write(Path::new(&filename), content.as_bytes(), d.force, d.force)?;
            self.status(&format!("Wrote {filename}."));
            return Ok(());
        }
        if self.clipboard {
            let copied = Clipboard::new().copy_to_clipboard(content).unwrap_or(false);
            if !copied {
                return Err(fail(4,"clipboard unavailable; use -f PATH or redirect stdout without the clipboard flag"));
            }
            self.status(&format!("Copied {label} to clipboard."));
            return Ok(());
        }
        if std::io::stdout().is_terminal() && !d.quiet {
            if Clipboard::new().copy_to_clipboard(content).unwrap_or(false) {
                self.status(&format!("Copied {label} to clipboard."));
                return Ok(());
            }
            eprintln!("Clipboard unavailable; showing the text instead.");
        }
        write_stdout(content)
    }
    pub fn status(&self, message: &str) {
        if !self.destination.quiet {
            eprintln!("{message}");
        }
    }
}
