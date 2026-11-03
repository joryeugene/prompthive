//! Local, credential-free portable copies of prompt data.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::persistence::Directory;
use crate::Storage;

pub const MANIFEST_NAME: &str = "prompthive-export.json";
pub const COMPATIBILITY_NAME: &str = "compatibility.toml";

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportManifest {
    pub format: String,
    pub version: u32,
    pub prompts: Vec<ExportPrompt>,
    #[serde(default)]
    pub compatibility: BTreeMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportPrompt {
    pub id: String,
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifiedCompatibility {
    pub version: u32,
    pub verified: bool,
    #[serde(default)]
    pub bindings: BTreeMap<String, String>,
}

pub fn read_verified_compatibility(root: &Path) -> Result<BTreeMap<String, String>> {
    let path = root.join(COMPATIBILITY_NAME);
    if !path.try_exists()? {
        return Ok(BTreeMap::new());
    }
    let bytes = read_source_bytes(root, Path::new(COMPATIBILITY_NAME))?;
    let config: VerifiedCompatibility =
        toml::from_str(std::str::from_utf8(&bytes)?).map_err(|_| {
            anyhow::anyhow!(
                "invalid compatibility file '{}'; its content was preserved",
                path.display()
            )
        })?;
    if config.version != 1 || !config.verified {
        return Ok(BTreeMap::new());
    }
    for code in config.bindings.keys() {
        validate_code(code)?;
    }
    Ok(config.bindings)
}

pub fn validate_code(code: &str) -> Result<()> {
    if code.is_empty()
        || code == "."
        || code == ".."
        || code.chars().count() > 64
        || !code
            .chars()
            .all(|c| c.is_alphanumeric() || "-_.@".contains(c))
    {
        bail!("invalid compatibility code");
    }
    Ok(())
}

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// A selected source root may resolve to a real directory; entries beneath it may not be links.
pub(crate) fn checked_source_path(root: &Path, relative: &Path) -> Result<PathBuf> {
    if relative.as_os_str().is_empty()
        || !relative
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        bail!("source path must be a relative path without parent or root components");
    }
    let mut path = open_source_root(root)?.path().to_path_buf();
    for part in relative.components() {
        path.push(part.as_os_str());
        let metadata = fs::symlink_metadata(&path)
            .with_context(|| format!("cannot read source entry '{}'", path.display()))?;
        if metadata.file_type().is_symlink() {
            bail!("source symlink is not allowed: '{}'", path.display());
        }
    }
    Ok(path)
}

/// Open each component through checked directory handles, including the final file.
/// A link swapped in after enumeration must not become an import source.
pub(crate) fn read_source_bytes(root: &Path, relative: &Path) -> Result<Vec<u8>> {
    if relative.as_os_str().is_empty()
        || !relative
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        bail!("source path must be a relative path without parent or root components");
    }
    let directory =
        open_source_root(root)?.descend(relative.parent().unwrap_or(Path::new("")), false)?;
    Ok(directory
        .read(
            relative
                .file_name()
                .context("source path has no filename")?,
        )?
        .0)
}

fn open_source_root(root: &Path) -> Result<Directory> {
    // Resolve the selected parent, then open the final directory without following a link.
    // This also rejects a selected root swapped for a symlink after initial enumeration.
    if let Some(name) = root.file_name() {
        let parent = root
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let parent = parent
            .canonicalize()
            .with_context(|| format!("cannot read source parent '{}'", parent.display()))?;
        return Directory::open(&parent, false)?.child(name, false);
    }
    Directory::open(
        &root
            .canonicalize()
            .with_context(|| format!("cannot read source '{}'", root.display()))?,
        false,
    )
}

pub(crate) fn relative_prompt_path(storage: &Storage, id: &str) -> Result<PathBuf> {
    let path = storage.prompt_path(id)?;
    let relative = path
        .strip_prefix(storage.base_dir())
        .context("prompt path is outside its selected store")?
        .to_path_buf();
    if !relative
        .components()
        .all(|part| matches!(part, Component::Normal(_)))
    {
        bail!("invalid stored prompt path");
    }
    Ok(relative)
}

/// Export all eligible prompt files as exact bytes. No config, credentials, history, or logs are copied.
pub fn export_store(storage: &Storage, destination: &Path) -> Result<usize> {
    if fs::symlink_metadata(destination).is_ok() {
        bail!(
            "export destination '{}' already exists; choose a new directory",
            destination.display()
        );
    }
    let mut payloads = Vec::new();
    let mut entries = Vec::new();
    for id in storage.list_prompts()? {
        let bytes = storage.read_raw(&id)?;
        let relative = relative_prompt_path(storage, &id)?;
        entries.push(ExportPrompt {
            id,
            path: relative.to_string_lossy().replace('\\', "/"),
            sha256: digest(&bytes),
        });
        payloads.push((relative, bytes));
    }
    let compatibility = match read_verified_compatibility(storage.base_dir()) {
        Ok(bindings) => bindings,
        Err(error) => {
            eprintln!(
                "Compatibility bindings were omitted: {error:#}. Prompt files are still exported."
            );
            BTreeMap::new()
        }
    };
    let manifest = ExportManifest {
        format: "prompthive".to_owned(),
        version: 1,
        prompts: entries,
        compatibility,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    // Reserve the new directory atomically: never replace even an empty existing directory.
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = parent.canonicalize()?;
    let root = parent.join(
        destination
            .file_name()
            .context("export destination has no directory name")?,
    );
    fs::create_dir(&root).with_context(|| {
        format!(
            "cannot create new export directory '{}'",
            destination.display()
        )
    })?;
    let directory = Directory::open(&root, false)?;
    Directory::open(&parent, false)?.sync().with_context(|| format!("export directory '{}' was created but its parent could not be synced; durability is uncertain and original prompt data is unchanged", destination.display()))?;
    directory.write_atomic(
        std::ffi::OsStr::new("INCOMPLETE"),
        b"Export is incomplete until prompthive-export.json is committed.\n",
        false,
        None,
    )?;
    let outcome = (|| -> Result<()> {
        for (relative, bytes) in &payloads {
            let parent = directory.descend(
                relative.parent().context("export path has no parent")?,
                true,
            )?;
            parent.write_atomic(
                relative
                    .file_name()
                    .context("export path has no filename")?,
                bytes,
                false,
                None,
            )?;
        }
        directory.write_atomic(
            std::ffi::OsStr::new(MANIFEST_NAME),
            &manifest_bytes,
            false,
            None,
        )?;
        directory.remove(std::ffi::OsStr::new("INCOMPLETE"))?;
        directory.sync()?;
        Ok(())
    })();
    outcome.with_context(|| format!("export at '{}' did not finish cleanly; inspect its manifest and INCOMPLETE marker before recovery. Original prompt data is unchanged", destination.display()))?;
    Ok(payloads.len())
}

pub(crate) fn load_manifest(root: &Path, storage: &Storage) -> Result<Option<ExportManifest>> {
    if root.join("INCOMPLETE").try_exists()? {
        bail!(
            "'{}' contains an incomplete export; original source files were not changed",
            root.display()
        );
    }
    if !root.join(MANIFEST_NAME).try_exists()? {
        return Ok(None);
    }
    let manifest: ExportManifest =
        serde_json::from_slice(&read_source_bytes(root, Path::new(MANIFEST_NAME))?)
            .context("invalid Prompthive export manifest")?;
    if manifest.format != "prompthive" || manifest.version != 1 {
        bail!("unsupported Prompthive export format/version");
    }
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for prompt in &manifest.prompts {
        if !ids.insert(&prompt.id) || !paths.insert(&prompt.path) {
            bail!("export manifest repeats a prompt identity or source path");
        }
        let expected = relative_prompt_path(storage, &prompt.id)?;
        if Path::new(&prompt.path) != expected {
            bail!(
                "export path does not match canonical prompt '{}'",
                prompt.id
            );
        }
        if prompt.sha256.len() != 64 || !prompt.sha256.bytes().all(|c| c.is_ascii_hexdigit()) {
            bail!("invalid checksum for '{}'", prompt.id);
        }
    }
    Ok(Some(manifest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_preserves_note_bytes_and_omits_credentials_and_history() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new_with_base(temp.path().join("store")).unwrap();
        storage.init().unwrap();
        let note = b"---\nid: notes/idea\ndescription: Note\nextra:\n  preserve: yes\n---\n\n  Literal {thing}\n\n";
        storage.write_raw("notes/idea", note, false).unwrap();
        fs::write(
            storage.base_dir().join("config.toml"),
            "token = 'do-not-copy'",
        )
        .unwrap();
        fs::write(storage.base_dir().join("history.json"), "do-not-copy").unwrap();
        let dest = temp.path().join("export");
        export_store(&storage, &dest).unwrap();
        assert_eq!(fs::read(dest.join("banks/notes/idea.md")).unwrap(), note);
        assert!(!dest.join("config.toml").exists());
        assert!(!dest.join("history.json").exists());
        let manifest = load_manifest(&dest, &storage).unwrap().unwrap();
        assert!(manifest
            .prompts
            .iter()
            .any(|entry| entry.id == "notes/idea" && entry.sha256 == digest(note)));
        assert!(export_store(&storage, &dest).is_err());
        assert_eq!(storage.read_raw("notes/idea").unwrap(), note);
    }

    #[test]
    fn manifest_rejects_parent_paths_before_any_target_writes() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new_with_base(temp.path().join("store")).unwrap();
        storage.init().unwrap();
        let source = temp.path().join("source");
        fs::create_dir(&source).unwrap();
        let manifest = ExportManifest {
            format: "prompthive".into(),
            version: 1,
            prompts: vec![ExportPrompt {
                id: "note".into(),
                path: "../private.md".into(),
                sha256: "0".repeat(64),
            }],
            compatibility: BTreeMap::new(),
        };
        fs::write(
            source.join(MANIFEST_NAME),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        assert!(load_manifest(&source, &storage).is_err());
        assert!(!storage.prompt_path("note").unwrap().exists());
    }
}
