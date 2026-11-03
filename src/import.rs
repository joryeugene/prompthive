//! Explicit local imports, preserving source bytes and keeping conflict decisions visible.

use anyhow::{bail, Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use walkdir::WalkDir;

use crate::persistence::Directory;
use crate::portability::{self, VerifiedCompatibility, COMPATIBILITY_NAME};
use crate::Storage;

pub struct Importer {
    storage: Storage,
}

struct ImportEntry {
    id: String,
    source: PathBuf,
    bytes: Vec<u8>,
}

impl Importer {
    pub fn new(storage: Storage) -> Self {
        Self { storage }
    }

    pub fn import_from_path(&self, path: &str) -> Result<ImportResult> {
        self.import_with_options(path, None, false, false, false, false, false)
    }

    pub fn import_from_path_enhanced(
        &self,
        path: &str,
        name: Option<&str>,
        force: bool,
        version: bool,
        skip: bool,
        update: bool,
    ) -> Result<ImportResult> {
        self.import_with_options(path, name, force, version, skip, update, false)
    }

    // Retain the existing library handler signature for callers.
    #[allow(clippy::too_many_arguments)]
    pub fn import_with_options(
        &self,
        path: &str,
        name: Option<&str>,
        force: bool,
        version: bool,
        skip: bool,
        update: bool,
        dry_run: bool,
    ) -> Result<ImportResult> {
        if [force, version, skip, update]
            .into_iter()
            .filter(|flag| *flag)
            .count()
            > 1
        {
            bail!("choose one import conflict policy: --force, --version, --skip, or --update");
        }
        let path = Path::new(path);
        let info = fs::symlink_metadata(path)
            .with_context(|| format!("cannot read import source '{}'", path.display()))?;
        if info.file_type().is_symlink() {
            bail!("import source is a symlink; select its actual local directory or file");
        }
        let manifest = if info.is_dir() {
            portability::load_manifest(path, &self.storage)?
        } else {
            None
        };
        if manifest.is_some() && name.is_some() {
            bail!("a portable store already contains canonical names; a custom NAME is only for loose files or directories: ph import PATH NAME");
        }
        if info.is_dir() && is_legacy_store(path) && name.is_some() {
            bail!("a complete local store already contains canonical names; a custom NAME is only for a loose file, bank, or directory: ph import PATH NAME");
        }
        let mut compatibility_warning = None;
        let compatibility = if let Some(manifest) = &manifest {
            manifest.compatibility.clone()
        } else if info.is_dir() && is_legacy_store(path) {
            match portability::read_verified_compatibility(path) {
                Ok(bindings) => bindings,
                Err(error) => {
                    compatibility_warning =
                        Some(format!("Source bindings were not adopted: {error:#}"));
                    BTreeMap::new()
                }
            }
        } else {
            BTreeMap::new()
        };
        let mut entries = Vec::new();
        let mut budget = crate::limits::Budget::new(crate::limits::IMPORT_BYTES, "import batch");
        let mut recovery_sources = Vec::new();
        if let Some(manifest) = &manifest {
            for prompt in &manifest.prompts {
                let source = portability::checked_source_path(path, Path::new(&prompt.path))?;
                let bytes = portability::read_source_bytes(path, Path::new(&prompt.path))?;
                budget.add(bytes.len())?;
                if portability::digest(&bytes) != prompt.sha256.to_lowercase() {
                    bail!(
                        "checksum mismatch for '{}' in export; no import was started",
                        prompt.id
                    );
                }
                entries.push(ImportEntry {
                    id: prompt.id.clone(),
                    source,
                    bytes,
                });
            }
        } else if info.is_file() {
            let id = match name {
                Some(name) => name.to_owned(),
                None => file_stem(path)?,
            };
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            let file = path.file_name().context("import source has no filename")?;
            let bytes = portability::read_source_bytes(parent, Path::new(file))?;
            budget.add(bytes.len())?;
            entries.push(ImportEntry {
                id,
                source: path.to_path_buf(),
                bytes,
            });
        } else if info.is_dir() {
            self.collect_directory(path, name, &mut entries, &mut recovery_sources, &mut budget)?;
        } else {
            bail!("import source is not a regular file or directory");
        }
        let mut seen = BTreeSet::new();
        for entry in &entries {
            self.storage.prompt_path(&entry.id)?;
            if !seen.insert(entry.id.clone()) {
                bail!("import contains duplicate canonical name '{}'", entry.id);
            }
            let text = std::str::from_utf8(&entry.bytes)
                .with_context(|| format!("'{}' is not valid UTF-8", entry.source.display()))?;
            self.storage.parse_prompt_content(text).with_context(|| {
                format!("cannot parse import source '{}'", entry.source.display())
            })?;
        }
        entries.sort_by(|a, b| a.id.cmp(&b.id));
        let mut result = ImportResult {
            dry_run,
            ..Default::default()
        };
        if !recovery_sources.is_empty() {
            result.migration_report.push(format!("This import copies live prompts only. Recovery snapshots remain untouched in '{}'; copy the whole source directory separately to retain that rollback history.", path.display()));
            result.migration_report.extend(
                recovery_sources
                    .into_iter()
                    .map(|source| format!("Retained source recovery directory: {source}")),
            );
        }
        if let Some(warning) = compatibility_warning {
            result
                .compatibility_skipped
                .push((COMPATIBILITY_NAME.into(), warning));
        }
        if dry_run && info.is_dir() && is_legacy_store(path) {
            let old = Storage::new_with_base(path.to_path_buf())?;
            result
                .migration_report
                .extend(crate::matching::legacy_code_report(&old)?);
        }
        let mut available_targets = BTreeMap::new();
        let mut planned_targets = BTreeMap::new();
        for entry in entries {
            let original_name = entry.id.clone();
            let outcome = (|| -> Result<()> {
                let target = self.storage.prompt_path(&entry.id)?;
                let existing = if let Some(bytes) = planned_targets.get(&entry.id) {
                    Some(Vec::clone(bytes))
                } else if target.try_exists()? {
                    Some(self.storage.read_raw(&entry.id)?)
                } else {
                    None
                };
                let has_history = !self.storage.snapshot_paths(&entry.id)?.is_empty();
                let id = if version && (existing.is_some() || has_history) {
                    self.next_version_name(&entry.id, &planned_targets)?
                } else {
                    entry.id.clone()
                };
                let renamed = id != entry.id;
                if existing.is_none() && has_history && !renamed && !force && !update {
                    if skip {
                        result
                            .skipped
                            .push((id, "Recovery history exists; skipped".into()));
                        return Ok(());
                    }
                    bail!("prompt has recovery history; restore it or explicitly choose --force or --version");
                }
                if let Some(existing) = &existing {
                    if !renamed && *existing == entry.bytes {
                        result
                            .skipped
                            .push((id.clone(), "Already identical".into()));
                        available_targets.insert(original_name.clone(), entry.bytes.clone());
                        return Ok(());
                    }
                    if !renamed && !force && !update {
                        result.skipped.push((
                            id.clone(),
                            "Already exists; choose --force, --version, --skip, or --update".into(),
                        ));
                        return Ok(());
                    }
                    if !renamed && update {
                        let source_time = modified(&entry.source)?;
                        let target_time = modified(&target)?;
                        if source_time <= target_time {
                            result.skipped.push((
                                id.clone(),
                                "Target is newer or has the same modification time".into(),
                            ));
                            return Ok(());
                        }
                    }
                }
                if dry_run {
                    planned_targets.insert(id.clone(), entry.bytes.clone());
                    result.planned.push(id.clone());
                    if !renamed {
                        available_targets.insert(original_name.clone(), entry.bytes.clone());
                    }
                    return Ok(());
                }
                self.storage
                    .write_raw(&id, &entry.bytes, !renamed && (force || update))?;
                result.imported.push(id);
                if !renamed {
                    available_targets.insert(original_name.clone(), entry.bytes.clone());
                }
                Ok(())
            })();
            if let Err(error) = outcome {
                result.errors.push((original_name, format!("{error:#}")));
            }
        }
        if let Err(error) =
            self.import_bindings(&compatibility, &available_targets, dry_run, &mut result)
        {
            result.errors.push((COMPATIBILITY_NAME.into(), format!("Prompt processing completed, but compatibility bindings were not committed: {error:#}")));
        }
        Ok(result)
    }

    fn collect_directory(
        &self,
        root: &Path,
        name: Option<&str>,
        entries: &mut Vec<ImportEntry>,
        recovery_sources: &mut Vec<String>,
        budget: &mut crate::limits::Budget,
    ) -> Result<()> {
        let legacy = is_legacy_store(root);
        let bank = root.join("bank.yaml").is_file()
            || root
                .parent()
                .is_some_and(|parent| parent.ends_with("banks"));
        let bank_prefix = if bank {
            Some(match name {
                Some(name) => name.to_owned(),
                None => root
                    .file_name()
                    .and_then(OsStr::to_str)
                    .context("bank directory must have a Unicode name")?
                    .to_owned(),
            })
        } else {
            name.map(str::to_owned)
        };
        let walker = WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| {
                if entry.depth() == 0 {
                    return true;
                }
                let Some(name) = entry.file_name().to_str() else {
                    return true;
                };
                if (legacy || bank) && entry.file_type().is_dir() && name.ends_with(".versions") {
                    recovery_sources.push(entry.path().display().to_string());
                    return false;
                }
                if name.starts_with('.') {
                    return false;
                }
                true
            });
        for entry in walker {
            let entry = entry
                .with_context(|| format!("cannot enumerate import source '{}'", root.display()))?;
            if entry.file_type().is_symlink() {
                bail!("import contains a symlink: '{}'", entry.path().display());
            }
            if !entry.file_type().is_file() {
                continue;
            }
            let relative = entry.path().strip_prefix(root)?;
            if !is_text_extension(entry.path()) {
                continue;
            }
            let id = if legacy {
                let parts: Vec<_> = relative
                    .components()
                    .map(|p| p.as_os_str().to_string_lossy().into_owned())
                    .collect();
                if parts.first().map(String::as_str) == Some("prompts") && parts.len() == 2 {
                    file_stem(entry.path())?
                } else if parts.first().map(String::as_str) == Some("prompts") {
                    bail!("nested plain prompt '{}' has no unambiguous canonical name; import that file explicitly with a custom NAME: ph import PATH NAME", entry.path().display());
                } else if parts.first().map(String::as_str) == Some("banks") && parts.len() >= 3 {
                    if entry
                        .file_name()
                        .to_str()
                        .is_some_and(|name| name.eq_ignore_ascii_case("README.md"))
                    {
                        continue;
                    }
                    without_extension(Path::new(&parts[1..].join("/")))?
                } else if parts.first().map(String::as_str) == Some("teams") && parts.len() >= 3 {
                    format!("@{}", without_extension(Path::new(&parts[1..].join("/")))?)
                } else {
                    continue;
                }
            } else {
                if bank
                    && entry
                        .file_name()
                        .to_str()
                        .is_some_and(|name| name.eq_ignore_ascii_case("README.md"))
                {
                    continue;
                }
                let local = without_extension(relative)?;
                match &bank_prefix {
                    Some(prefix) => format!("{prefix}/{local}"),
                    None => local,
                }
            };
            let source = portability::checked_source_path(root, relative)?;
            let bytes = portability::read_source_bytes(root, relative)?;
            budget.add(bytes.len())?;
            entries.push(ImportEntry { id, bytes, source });
        }
        Ok(())
    }

    fn next_version_name(&self, id: &str, planned: &BTreeMap<String, Vec<u8>>) -> Result<String> {
        for number in 2..=10000 {
            let candidate = format!("{id}-v{number}");
            if !planned.contains_key(&candidate)
                && !self.storage.prompt_path(&candidate)?.try_exists()?
                && self.storage.snapshot_paths(&candidate)?.is_empty()
            {
                return Ok(candidate);
            }
        }
        bail!("no available import version name for '{id}'");
    }

    fn import_bindings(
        &self,
        bindings: &BTreeMap<String, String>,
        available: &BTreeMap<String, Vec<u8>>,
        dry_run: bool,
        result: &mut ImportResult,
    ) -> Result<()> {
        if bindings.is_empty() {
            return Ok(());
        }
        let root = if dry_run && !self.storage.base_dir().try_exists()? {
            None
        } else {
            Some(Directory::open(self.storage.base_dir(), false)?)
        };
        let _lock = if dry_run {
            None
        } else {
            root.as_ref().map(Directory::lock_file).transpose()?
        };
        let previous = match root
            .as_ref()
            .map(|root| root.read(OsStr::new(COMPATIBILITY_NAME)))
        {
            Some(Ok((bytes, _))) => Some(bytes),
            Some(Err(error))
                if error.chain().any(|cause| {
                    cause
                        .downcast_ref::<io::Error>()
                        .is_some_and(|error| error.kind() == io::ErrorKind::NotFound)
                }) =>
            {
                None
            }
            Some(Err(error)) => return Err(error),
            None => None,
        };
        let mut existing = if let Some(bytes) = &previous {
            let config: VerifiedCompatibility = match toml::from_str(std::str::from_utf8(bytes)?) {
                Ok(config) => config,
                Err(_) => {
                    for code in bindings.keys() {
                        result.compatibility_skipped.push((
                            code.clone(),
                            "Existing compatibility file is invalid; it was preserved".into(),
                        ));
                    }
                    return Ok(());
                }
            };
            if config.version != 1 || !config.verified {
                for code in bindings.keys() {
                    result.compatibility_skipped.push((
                        code.clone(),
                        "Existing compatibility file is unverified; it was preserved".into(),
                    ));
                }
                return Ok(());
            }
            config.bindings
        } else {
            BTreeMap::new()
        };
        let mut pending = Vec::new();
        for (code, target) in bindings {
            let reason = if portability::validate_code(code).is_err()
                || self.storage.prompt_path(target).is_err()
            {
                Some("Invalid compatibility identity or target")
            } else if !available.contains_key(target) {
                Some("Target was not imported under that exact name or verified identical")
            } else if !dry_run
                && !self
                    .storage
                    .read_raw(target)
                    .is_ok_and(|bytes| Some(&bytes) == available.get(target))
            {
                Some("Target changed or became unavailable after import; choose the binding explicitly")
            } else if existing.get(code).is_some_and(|old| old != target) {
                Some("Existing compatibility target differs; choose the binding explicitly")
            } else if code != target
                && (available.contains_key(code)
                    || self
                        .storage
                        .prompt_path(code)
                        .is_ok_and(|path| path.exists()))
            {
                Some("An exact canonical prompt already owns that name")
            } else {
                None
            };
            if let Some(reason) = reason {
                result
                    .compatibility_skipped
                    .push((code.clone(), reason.into()));
            } else if existing.get(code) != Some(target) {
                existing.insert(code.clone(), target.clone());
                pending.push(code.clone());
            }
        }
        if pending.is_empty() {
            return Ok(());
        }
        if dry_run {
            result.compatibility_planned.extend(pending);
            return Ok(());
        }
        let config = VerifiedCompatibility {
            version: 1,
            verified: true,
            bindings: existing,
        };
        let bytes = toml::to_string_pretty(&config)?.into_bytes();
        let root =
            root.context("imported prompt store is unavailable for compatibility bindings")?;
        let commit = (|| -> Result<()> {
            if let Some(previous) = &previous {
                let backups = root.child(OsStr::new(".metadata-backups"), true)?;
                backups.write_atomic(
                    OsStr::new(&format!("{}-compatibility.toml", uuid::Uuid::new_v4())),
                    previous,
                    false,
                    None,
                )?;
            }
            root.write_atomic(
                OsStr::new(COMPATIBILITY_NAME),
                &bytes,
                previous.is_some(),
                previous.as_deref(),
            )
        })();
        match commit {
            Ok(()) => result.compatibility_imported.extend(pending),
            Err(error) => result.errors.push((
                COMPATIBILITY_NAME.into(),
                format!(
                    "Prompts imported, but compatibility bindings were not committed: {error:#}"
                ),
            )),
        }
        Ok(())
    }
}

fn modified(path: &Path) -> Result<SystemTime> {
    fs::metadata(path)?
        .modified()
        .with_context(|| format!("cannot read modification time for '{}'", path.display()))
}
fn file_stem(path: &Path) -> Result<String> {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(str::to_owned)
        .context("source filename must be valid Unicode")
}
fn without_extension(path: &Path) -> Result<String> {
    let mut value = path.to_path_buf();
    value.set_extension("");
    value
        .to_str()
        .map(|s| s.replace('\\', "/"))
        .context("source filename must be valid Unicode")
}
fn is_text_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|extension| {
            ["md", "txt", "prompt", "tmpl", "template"]
                .iter()
                .any(|expected| extension.eq_ignore_ascii_case(expected))
        })
}
fn is_legacy_store(path: &Path) -> bool {
    ["prompts", "banks", "teams"]
        .iter()
        .any(|name| path.join(name).is_dir())
}

#[derive(Debug, Default)]
pub struct ImportResult {
    pub imported: Vec<String>,
    pub planned: Vec<String>,
    pub skipped: Vec<(String, String)>,
    pub errors: Vec<(String, String)>,
    pub compatibility_imported: Vec<String>,
    pub compatibility_planned: Vec<String>,
    pub compatibility_skipped: Vec<(String, String)>,
    pub migration_report: Vec<String>,
    pub dry_run: bool,
}

impl ImportResult {
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
    pub fn summary(&self) -> String {
        if self.dry_run {
            format!(
                "Would import: {}, Skipped: {}, Errors: {}",
                self.planned.len(),
                self.skipped.len(),
                self.errors.len()
            )
        } else {
            format!(
                "Imported: {}, Skipped: {}, Errors: {}",
                self.imported.len(),
                self.skipped.len(),
                self.errors.len()
            )
        }
    }
    pub fn display(&self) {
        eprintln!("{}", self.summary());
        for name in &self.planned {
            eprintln!("  Would import {name}");
        }
        for name in &self.imported {
            eprintln!("  Imported {name}");
        }
        for (name, reason) in &self.skipped {
            eprintln!("  Skipped {name}: {reason}");
        }
        for (name, reason) in &self.errors {
            eprintln!("  Failed {name}: {reason}");
        }
        for name in &self.compatibility_planned {
            eprintln!("  Would retain compatibility code {name}");
        }
        for name in &self.compatibility_imported {
            eprintln!("  Retained compatibility code {name}");
        }
        for (name, reason) in &self.compatibility_skipped {
            eprintln!("  Skipped compatibility code {name}: {reason}");
        }
        for line in &self.migration_report {
            eprintln!("  {line}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn storage(temp: &tempfile::TempDir, name: &str) -> Storage {
        let storage = Storage::new_with_base(temp.path().join(name)).unwrap();
        storage.init().unwrap();
        storage
    }

    #[test]
    fn versioned_dry_run_reserves_destinations_and_matches_real_bindings() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        fs::create_dir_all(source.join("prompts")).unwrap();
        fs::write(source.join("prompts/a.md"), "First source").unwrap();
        fs::write(source.join("prompts/a-v2.md"), "Second source").unwrap();
        fs::write(
            source.join("compatibility.toml"),
            "version=1\nverified=true\n[bindings]\nv=\"a-v2\"\n",
        )
        .unwrap();
        let destination = storage(&temp, "destination");
        destination
            .write_raw("a", b"Existing target", false)
            .unwrap();
        let importer = Importer::new(destination.clone());
        let plan = importer
            .import_with_options(
                source.to_str().unwrap(),
                None,
                false,
                true,
                false,
                false,
                true,
            )
            .unwrap();
        assert_eq!(plan.planned, vec!["a-v2", "a-v2-v2"]);
        assert!(plan.compatibility_planned.is_empty());
        assert!(!destination.prompt_path("a-v2").unwrap().exists());
        let actual = importer
            .import_with_options(
                source.to_str().unwrap(),
                None,
                false,
                true,
                false,
                false,
                false,
            )
            .unwrap();
        assert_eq!(actual.imported, plan.planned);
        assert_eq!(actual.compatibility_imported, plan.compatibility_planned);
        assert_eq!(destination.read_raw("a").unwrap(), b"Existing target");
        assert_eq!(destination.read_raw("a-v2").unwrap(), b"First source");
        assert_eq!(destination.read_raw("a-v2-v2").unwrap(), b"Second source");
    }

    #[test]
    fn portable_note_round_trip_and_dry_run_preserve_all_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let source = storage(&temp, "source");
        let note = b"---\nid: notes/idea\ndescription: Note\nunfamiliar: [one, two]\n---\n\n  Keep {input} literal here\n\n";
        source.write_raw("notes/idea", note, false).unwrap();
        let export = temp.path().join("export");
        portability::export_store(&source, &export).unwrap();
        let destination = storage(&temp, "destination");
        let importer = Importer::new(destination.clone());
        let plan = importer
            .import_with_options(
                export.to_str().unwrap(),
                None,
                false,
                false,
                false,
                false,
                true,
            )
            .unwrap();
        assert!(plan.planned.contains(&"notes/idea".to_string()));
        assert!(!destination.prompt_path("notes/idea").unwrap().exists());
        let result = importer.import_from_path(export.to_str().unwrap()).unwrap();
        assert!(!result.has_errors());
        assert_eq!(destination.read_raw("notes/idea").unwrap(), note);
    }

    #[test]
    fn conflicting_and_versioned_targets_do_not_activate_dependent_bindings() {
        let temp = tempfile::tempdir().unwrap();
        let source = storage(&temp, "source");
        source.write_raw("work/review", b"Source A", false).unwrap();
        fs::write(
            source.base_dir().join(COMPATIBILITY_NAME),
            "version=1\nverified=true\n[bindings]\ne=\"work/review\"\n",
        )
        .unwrap();
        let export = temp.path().join("export");
        portability::export_store(&source, &export).unwrap();
        let destination = storage(&temp, "destination");
        destination
            .write_raw("work/review", b"Destination B", false)
            .unwrap();
        let importer = Importer::new(destination.clone());
        let result = importer.import_from_path(export.to_str().unwrap()).unwrap();
        assert!(!result.compatibility_skipped.is_empty());
        assert!(!destination.base_dir().join(COMPATIBILITY_NAME).exists());
        assert_eq!(
            destination.read_raw("work/review").unwrap(),
            b"Destination B"
        );
        let versioned = importer
            .import_with_options(
                export.to_str().unwrap(),
                None,
                false,
                true,
                false,
                false,
                false,
            )
            .unwrap();
        assert!(versioned.imported.contains(&"work/review-v2".into()));
        assert!(!destination.base_dir().join(COMPATIBILITY_NAME).exists());
        let forced = importer
            .import_with_options(
                export.to_str().unwrap(),
                None,
                true,
                false,
                false,
                false,
                false,
            )
            .unwrap();
        assert!(!forced.has_errors());
        assert_eq!(destination.read_raw("work/review").unwrap(), b"Source A");
        assert_eq!(
            portability::read_verified_compatibility(destination.base_dir())
                .unwrap()
                .get("e"),
            Some(&"work/review".into())
        );
        assert!(!destination
            .snapshot_paths("work/review")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn checksum_failure_validates_every_source_before_starting_import() {
        let temp = tempfile::tempdir().unwrap();
        let source = storage(&temp, "source");
        source.write_raw("a-good", b"Good note", false).unwrap();
        source
            .write_raw("z-tampered", b"Original note", false)
            .unwrap();
        let export = temp.path().join("export");
        portability::export_store(&source, &export).unwrap();
        fs::write(export.join("prompts/z-tampered.md"), "Changed after export").unwrap();
        let destination = storage(&temp, "destination");
        let error = Importer::new(destination.clone())
            .import_from_path(export.to_str().unwrap())
            .unwrap_err();
        assert!(error.to_string().contains("checksum mismatch"));
        assert!(!destination.prompt_path("a-good").unwrap().exists());
        assert!(!destination.prompt_path("z-tampered").unwrap().exists());
    }

    #[test]
    fn dry_run_into_absent_store_never_creates_the_store_or_bindings() {
        let temp = tempfile::tempdir().unwrap();
        let source = storage(&temp, "source");
        source.write_raw("work/review", b"Review", false).unwrap();
        fs::write(
            source.base_dir().join(COMPATIBILITY_NAME),
            "version=1\nverified=true\n[bindings]\ne=\"work/review\"\n",
        )
        .unwrap();
        let export = temp.path().join("export");
        portability::export_store(&source, &export).unwrap();
        let destination = Storage::new_with_base(temp.path().join("absent")).unwrap();
        let result = Importer::new(destination.clone())
            .import_with_options(
                export.to_str().unwrap(),
                None,
                false,
                false,
                false,
                false,
                true,
            )
            .unwrap();
        assert_eq!(result.planned.len(), source.list_prompts().unwrap().len());
        assert_eq!(result.compatibility_planned, vec!["e".to_string()]);
        assert!(!destination.base_dir().exists());
    }

    #[test]
    fn legacy_plain_bank_and_cached_team_notes_and_verified_bindings_are_preserved() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("old-store");
        fs::create_dir_all(source.join("prompts")).unwrap();
        fs::create_dir_all(source.join("banks/notes")).unwrap();
        fs::create_dir_all(source.join("teams/shared")).unwrap();
        let plain = b"  Plain note\n\n";
        let bank = b"---\nid: notes/idea\ncustom: keep\n---\n\nBank note\n";
        let team = b"Cached team note\n\n";
        fs::write(source.join("prompts/todo.md"), plain).unwrap();
        fs::write(source.join("banks/notes/idea.md"), bank).unwrap();
        fs::write(source.join("teams/shared/check.md"), team).unwrap();
        fs::write(source.join("config.toml"), "token='keep-private'").unwrap();
        fs::write(
            source.join(COMPATIBILITY_NAME),
            "version=1\nverified=true\n[bindings]\ne=\"notes/idea\"\n",
        )
        .unwrap();
        let destination = storage(&temp, "destination");
        let result = Importer::new(destination.clone())
            .import_from_path(source.to_str().unwrap())
            .unwrap();
        assert!(!result.has_errors());
        assert_eq!(destination.read_raw("todo").unwrap(), plain);
        assert_eq!(destination.read_raw("notes/idea").unwrap(), bank);
        assert_eq!(destination.read_raw("@shared/check").unwrap(), team);
        assert!(!destination.base_dir().join("config.toml").exists());
        assert_eq!(result.compatibility_imported, vec!["e".to_string()]);
        assert_eq!(fs::read(source.join("banks/notes/idea.md")).unwrap(), bank);
    }

    #[test]
    fn identical_target_can_activate_binding_but_unknown_existing_config_is_preserved() {
        let temp = tempfile::tempdir().unwrap();
        let source = storage(&temp, "source");
        source
            .write_raw("work/review", b"Same note", false)
            .unwrap();
        fs::write(
            source.base_dir().join(COMPATIBILITY_NAME),
            "version=1\nverified=true\n[bindings]\ne=\"work/review\"\n",
        )
        .unwrap();
        let export = temp.path().join("export");
        portability::export_store(&source, &export).unwrap();
        let destination = storage(&temp, "destination");
        destination
            .write_raw("work/review", b"Same note", false)
            .unwrap();
        let importer = Importer::new(destination.clone());
        let identical = importer.import_from_path(export.to_str().unwrap()).unwrap();
        assert_eq!(identical.compatibility_imported, vec!["e".to_string()]);
        let config =
            b"version=1\nverified=true\nfuture_field='preserve'\n[bindings]\nother='work/review'\n";
        fs::write(destination.base_dir().join(COMPATIBILITY_NAME), config).unwrap();
        let result = importer.import_from_path(export.to_str().unwrap()).unwrap();
        assert!(result.compatibility_imported.is_empty());
        assert!(!result.compatibility_skipped.is_empty());
        assert_eq!(
            fs::read(destination.base_dir().join(COMPATIBILITY_NAME)).unwrap(),
            config
        );
    }

    #[test]
    fn deleted_identity_requires_explicit_restore_or_a_new_version_in_dry_run_too() {
        let temp = tempfile::tempdir().unwrap();
        let destination = storage(&temp, "destination");
        destination
            .write_raw("note", b"Deleted note", false)
            .unwrap();
        destination.delete_prompt("note").unwrap();
        let source = temp.path().join("note.md");
        fs::write(&source, "Restored note").unwrap();
        let importer = Importer::new(destination.clone());
        let plan = importer
            .import_with_options(
                source.to_str().unwrap(),
                None,
                false,
                false,
                false,
                false,
                true,
            )
            .unwrap();
        assert!(plan.has_errors());
        assert!(!destination.prompt_path("note").unwrap().exists());
        let versioned = importer
            .import_with_options(
                source.to_str().unwrap(),
                None,
                false,
                true,
                false,
                false,
                false,
            )
            .unwrap();
        assert_eq!(versioned.imported, vec!["note-v2".to_string()]);
        let forced = importer
            .import_with_options(
                source.to_str().unwrap(),
                None,
                true,
                false,
                false,
                false,
                false,
            )
            .unwrap();
        assert!(!forced.has_errors());
        assert_eq!(destination.read_raw("note").unwrap(), b"Restored note");
        assert!(!destination.snapshot_paths("note").unwrap().is_empty());
    }

    #[test]
    fn legacy_snapshot_directories_are_not_imported_as_live_prompts_and_source_is_retained() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("old-store");
        fs::create_dir_all(source.join("prompts")).unwrap();
        fs::create_dir_all(source.join("banks/work")).unwrap();
        fs::write(source.join("prompts/note.md"), "Current note").unwrap();
        fs::write(source.join("banks/work/review.md"), "Current review").unwrap();
        let recovery_paths = [
            "prompts/note.versions/v1.md",
            "banks/work/review.versions/v1.md",
            "banks/work/work/review.versions/v0.md",
            ".versions/prompts/note/raw.md",
        ];
        for path in &recovery_paths {
            let file = source.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, format!("Recovery bytes for {path}\n\n")).unwrap();
        }
        let destination = storage(&temp, "destination");
        let importer = Importer::new(destination.clone());
        let plan = importer
            .import_with_options(
                source.to_str().unwrap(),
                None,
                false,
                false,
                false,
                false,
                true,
            )
            .unwrap();
        assert!(!plan.has_errors());
        assert_eq!(
            plan.planned,
            vec!["note".to_string(), "work/review".to_string()]
        );
        assert!(plan
            .migration_report
            .iter()
            .any(|line| line.contains("live prompts only")));
        assert!(!destination.prompt_path("note").unwrap().exists());
        let result = importer.import_from_path(source.to_str().unwrap()).unwrap();
        assert!(!result.has_errors());
        assert_eq!(
            result.imported,
            vec!["note".to_string(), "work/review".to_string()]
        );
        assert!(result
            .migration_report
            .iter()
            .any(|line| line.contains("copy the whole source directory")));
        assert_eq!(destination.read_raw("note").unwrap(), b"Current note");
        assert!(destination.snapshot_paths("note").unwrap().is_empty());
        assert!(destination
            .snapshot_paths("work/review")
            .unwrap()
            .is_empty());
        for path in &recovery_paths {
            assert_eq!(
                fs::read(source.join(path)).unwrap(),
                format!("Recovery bytes for {path}\n\n").as_bytes()
            );
        }
        let bank_destination = storage(&temp, "bank-destination");
        let bank_importer = Importer::new(bank_destination.clone());
        let bank = source.join("banks/work");
        let plan = bank_importer
            .import_with_options(
                bank.to_str().unwrap(),
                None,
                false,
                false,
                false,
                false,
                true,
            )
            .unwrap();
        assert_eq!(plan.planned, vec!["work/review".to_string()]);
        let result = bank_importer
            .import_from_path(bank.to_str().unwrap())
            .unwrap();
        assert!(!result.has_errors());
        assert_eq!(result.imported, vec!["work/review".to_string()]);
        assert_eq!(
            bank_destination.read_raw("work/review").unwrap(),
            b"Current review"
        );
    }

    #[cfg(unix)]
    #[test]
    fn imported_symlink_is_rejected_without_any_prompt_write() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let destination = storage(&temp, "destination");
        let source = temp.path().join("loose");
        fs::create_dir(&source).unwrap();
        fs::write(temp.path().join("external.md"), "Private source").unwrap();
        symlink(temp.path().join("external.md"), source.join("linked.md")).unwrap();
        assert!(Importer::new(destination.clone())
            .import_from_path(source.to_str().unwrap())
            .is_err());
        assert!(!destination.prompt_path("linked").unwrap().exists());
    }
}
