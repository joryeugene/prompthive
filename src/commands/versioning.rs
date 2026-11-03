//! Local raw snapshots. A snapshot is data, never a second prompt frontmatter layer.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::{persistence, Storage};

#[derive(Debug, Default, Serialize, Deserialize)]
struct VersionNote {
    message: String,
    created_at: String,
}

pub fn handle_version(
    storage: &Storage,
    name: &str,
    tag: &str,
    message: Option<&str>,
    _start: Instant,
) -> Result<()> {
    if storage
        .snapshot_paths(name)?
        .iter()
        .any(|(existing, _)| existing == tag)
    {
        bail!("version '{tag}' already exists for '{name}'");
    }
    let snapshot = storage.snapshot(name, tag)?;
    if let Some(message) = message {
        let note = VersionNote {
            message: message.to_owned(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        let bytes = serde_json::to_vec_pretty(&note)?;
        persistence::atomic_write(&snapshot.with_extension("json"), &bytes, false, false)
            .with_context(|| {
                format!(
                    "snapshot was saved at '{}', but its message could not be saved",
                    snapshot.display()
                )
            })?;
    }
    eprintln!("Created version {tag} for {name}.");
    Ok(())
}

pub fn handle_versions(
    storage: &Storage,
    name: &str,
    verbose: bool,
    _start: Instant,
) -> Result<()> {
    let paths = storage.snapshot_paths(name)?;
    if paths.is_empty() {
        writeln!(io::stdout().lock(), "No versions for {name}.")?;
        return Ok(());
    }
    let mut versions = BTreeMap::<String, (PathBuf, Vec<u8>)>::new();
    for (tag, path) in paths {
        let bytes = snapshot_bytes(storage, name, &path)?;
        if let Some((_, previous)) = versions.get(&tag) {
            if previous != &bytes {
                bail!("version '{tag}' has conflicting snapshots; inspect the local version files before restoring");
            }
        } else {
            versions.insert(tag, (path, bytes));
        }
    }
    let mut versions: Vec<_> = versions.into_iter().collect();
    versions.sort_by(|a, b| {
        let modified = |p: &Path| fs::metadata(p).and_then(|m| m.modified()).ok();
        modified(&b.1 .0)
            .cmp(&modified(&a.1 .0))
            .then_with(|| a.0.cmp(&b.0))
    });
    let mut output = String::new();
    for (tag, (path, bytes)) in versions {
        let hash = format!("{:x}", Sha256::digest(&bytes));
        if verbose {
            let note_path = path.with_extension("json");
            let note = if note_path.exists() {
                serde_json::from_slice::<VersionNote>(&storage.read_safe_path(&note_path)?)
                    .with_context(|| {
                        format!(
                            "invalid version message '{}'; snapshot content is intact",
                            note_path.display()
                        )
                    })?
            } else {
                VersionNote::default()
            };
            output.push_str(&format!("{tag}\t{}", &hash[..12]));
            if !note.created_at.is_empty() {
                output.push_str(&format!("\t{}", note.created_at));
            }
            if !note.message.is_empty() {
                output.push_str(&format!("\t{}", note.message.replace('\n', " ")));
            }
            output.push('\n');
        } else {
            output.push_str(&format!("{tag}\n"));
        }
    }
    let mut stdout = io::stdout().lock();
    stdout.write_all(output.as_bytes())?;
    stdout.flush()?;
    Ok(())
}

pub fn handle_rollback(
    storage: &Storage,
    name: &str,
    version: &str,
    _backup: bool,
    _start: Instant,
) -> Result<()> {
    let mut selected: Option<Vec<u8>> = None;
    for (tag, path) in storage.snapshot_paths(name)? {
        if tag == version {
            let bytes = snapshot_bytes(storage, name, &path)?;
            if let Some(previous) = &selected {
                if previous != &bytes {
                    bail!("version '{version}' has conflicting snapshots; choose its local source explicitly");
                }
            } else {
                selected = Some(bytes);
            }
        }
    }
    let bytes = selected.with_context(|| format!("version '{version}' not found for '{name}'"))?;
    // The shared writer snapshots any existing target before replacing it.
    storage.write_raw(name, &bytes, true)?;
    eprintln!("Restored {name} from {version}; any previous content was backed up.");
    Ok(())
}

fn snapshot_bytes(storage: &Storage, name: &str, path: &Path) -> Result<Vec<u8>> {
    let bytes = storage.read_safe_path(path)?;
    if path.starts_with(storage.versions_dir(name)?) {
        return Ok(bytes);
    }
    unwrap_legacy_snapshot(&bytes)
}

fn unwrap_legacy_snapshot(bytes: &[u8]) -> Result<Vec<u8>> {
    let text = std::str::from_utf8(bytes).context("legacy snapshot is not valid UTF-8")?;
    let Some(rest) = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    else {
        return Ok(bytes.to_vec());
    };
    let header_start = text.len() - rest.len();
    let mut offset = header_start;
    for line in rest.split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            let header = &text[header_start..offset];
            let has = |key: &str| header.lines().any(|line| line.starts_with(key));
            if has("version:")
                && has("git_hash:")
                && has("created_at:")
                && !has("id:")
                && !has("description:")
            {
                let body = &text[offset + line.len()..];
                let original = body
                    .strip_prefix("\r\n")
                    .or_else(|| body.strip_prefix('\n'))
                    .unwrap_or(body);
                return Ok(original.as_bytes().to_vec());
            }
            break;
        }
        offset += line.len();
    }
    Ok(bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_wrapper_is_removed_once_and_original_frontmatter_and_whitespace_survive() {
        let original = "---\nid: daily/review\ndescription: Review\ncustom: preserve\n---\n\n  Review {input}\n\n";
        let wrapper = format!("---\nversion: v1\ngit_hash: 1234\ncreated_at: 2025-07-09\nmessage: source: notes\n---\n\n{original}");
        assert_eq!(
            unwrap_legacy_snapshot(wrapper.as_bytes()).unwrap(),
            original.as_bytes()
        );
        assert_eq!(
            unwrap_legacy_snapshot(original.as_bytes()).unwrap(),
            original.as_bytes()
        );
    }

    #[test]
    fn raw_version_round_trip_keeps_metadata_and_body_after_deletion() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new_with_base(temp.path().join("store")).unwrap();
        storage.init().unwrap();
        let original = b"---\nid: notes/idea\ndescription: Note\nunfamiliar:\n  preserve: true\n---\n\n  A note with {braces}\n\n";
        storage.write_raw("notes/idea", original, false).unwrap();
        handle_version(
            &storage,
            "notes/idea",
            "v1",
            Some("Before edit"),
            Instant::now(),
        )
        .unwrap();
        storage.write_raw("notes/idea", b"Changed", true).unwrap();
        storage.delete_prompt("notes/idea").unwrap();
        handle_versions(&storage, "notes/idea", false, Instant::now()).unwrap();
        handle_rollback(&storage, "notes/idea", "v1", false, Instant::now()).unwrap();
        assert_eq!(storage.read_raw("notes/idea").unwrap(), original);
    }

    #[test]
    fn version_tag_cannot_escape_storage() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new_with_base(temp.path().join("store")).unwrap();
        storage.init().unwrap();
        storage.write_raw("note", b"A note", false).unwrap();
        assert!(handle_version(&storage, "note", "../../escape", None, Instant::now()).is_err());
        assert!(!temp.path().join("escape.md").exists());
    }

    #[test]
    fn rollback_keeps_the_complete_current_file_as_a_recovery_snapshot() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new_with_base(temp.path().join("store")).unwrap();
        storage.init().unwrap();
        let original = b"Original\n\n";
        let current = b"---\nid: note\nunknown: [keep, me]\n---\n\nEdited current\n\n";
        storage.write_raw("note", original, false).unwrap();
        handle_version(&storage, "note", "v1", None, Instant::now()).unwrap();
        storage.write_raw("note", current, true).unwrap();
        handle_rollback(&storage, "note", "v1", false, Instant::now()).unwrap();
        assert_eq!(storage.read_raw("note").unwrap(), original);
        assert!(storage
            .snapshot_paths("note")
            .unwrap()
            .iter()
            .any(|(_, path)| storage.read_safe_path(path).unwrap() == current));
    }

    #[test]
    fn old_banked_wrapper_restores_one_prompt_layer_and_rejects_conflicting_tags() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new_with_base(temp.path().join("store")).unwrap();
        storage.init().unwrap();
        let original =
            b"---\nid: notes/idea\ndescription: Note\ncustom: keep\n---\n\n  Original body\n\n";
        let legacy = storage.base_dir().join("banks/notes/notes/idea.versions");
        fs::create_dir_all(&legacy).unwrap();
        let mut wrapped =
            b"---\nversion: v1\ngit_hash: abcdef\ncreated_at: 2025-07-09\n---\n\n".to_vec();
        wrapped.extend_from_slice(original);
        fs::write(legacy.join("v1.md"), wrapped).unwrap();
        handle_rollback(&storage, "notes/idea", "v1", false, Instant::now()).unwrap();
        assert_eq!(storage.read_raw("notes/idea").unwrap(), original);
        let newer = storage.versions_dir("notes/idea").unwrap();
        fs::create_dir_all(&newer).unwrap();
        fs::write(newer.join("v1.md"), "Conflicting snapshot").unwrap();
        assert!(handle_rollback(&storage, "notes/idea", "v1", false, Instant::now()).is_err());
        assert_eq!(storage.read_raw("notes/idea").unwrap(), original);
    }
}
