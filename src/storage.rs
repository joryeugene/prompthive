//! Local Markdown storage. Reads do not migrate files or consult remote settings.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::persistence::{ConflictError, Directory};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PromptMetadata {
    pub id: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_version: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub vars: BTreeMap<String, Option<String>>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_yaml::Value>,
}

#[derive(Debug, Clone)]
pub struct Storage {
    base_dir: PathBuf,
}

impl Storage {
    pub fn new() -> Result<Self> {
        let base = match std::env::var("PROMPTHIVE_BASE_DIR") {
            Ok(value) if value.is_empty() => bail!("PROMPTHIVE_BASE_DIR must not be empty"),
            Ok(value) => PathBuf::from(value),
            Err(std::env::VarError::NotPresent) => dirs::home_dir()
                .context("could not find home directory")?
                .join(".prompthive"),
            Err(error) => return Err(error.into()),
        };
        Self::new_with_base(base)
    }

    pub fn new_with_base(base_dir: PathBuf) -> Result<Self> {
        let absolute = if base_dir.is_absolute() {
            base_dir
        } else {
            std::env::current_dir()?.join(base_dir)
        };
        let mut nearest = absolute.as_path();
        let mut missing = Vec::new();
        while !nearest.exists() {
            missing.push(
                nearest
                    .file_name()
                    .context("invalid store path")?
                    .to_os_string(),
            );
            nearest = nearest.parent().context("invalid store path")?;
        }
        let mut base_dir = fs::canonicalize(nearest).context("cannot resolve selected store")?;
        for name in missing.into_iter().rev() {
            if name == "." || name == ".." {
                bail!("invalid store path component");
            }
            base_dir.push(name);
        }
        Ok(Self { base_dir })
    }

    pub fn base_dir(&self) -> &PathBuf {
        &self.base_dir
    }
    pub fn prompts_dir(&self) -> PathBuf {
        self.base_dir.join("prompts")
    }
    pub fn teams_dir(&self) -> PathBuf {
        self.base_dir.join("teams")
    }

    /// Seed only a genuinely new store. Existing empty stores stay empty.
    pub fn init(&self) -> Result<()> {
        if self.base_dir.exists() {
            Directory::open(&self.base_dir, false)?;
            return Ok(());
        }
        let parent = Directory::open(
            self.base_dir
                .parent()
                .context("store root cannot be filesystem root")?,
            true,
        )?;
        let Some(root) = parent.create_child(
            self.base_dir
                .file_name()
                .context("store has no directory name")?,
        )?
        else {
            return Ok(());
        };
        let _lock = root.lock_file()?;
        for (name, content) in STARTERS {
            let relative = Self::relative_prompt_path(name)?;
            let parent = root.descend(relative.parent().unwrap_or(Path::new("")), true)?;
            parent.write_atomic(
                relative.file_name().unwrap(),
                content.as_bytes(),
                false,
                None,
            )?;
        }
        Ok(())
    }

    pub fn validate_name(name: &str) -> Result<()> {
        Self::relative_prompt_path(name).map(|_| ())
    }

    pub fn validate_component(component: &str) -> Result<()> {
        if component.is_empty() || component == "." || component == ".." || component.len() > 255 {
            bail!(
                "invalid name component {component:?}: use 1–255 UTF-8 bytes, excluding . and .."
            );
        }
        if !component
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
        {
            bail!("invalid name component {component:?}: use letters, numbers, -, _, or .");
        }
        #[cfg(windows)]
        {
            let upper = component
                .split('.')
                .next()
                .unwrap_or(component)
                .to_ascii_uppercase();
            if matches!(
                upper.as_str(),
                "CON"
                    | "PRN"
                    | "AUX"
                    | "NUL"
                    | "COM1"
                    | "COM2"
                    | "COM3"
                    | "COM4"
                    | "COM5"
                    | "COM6"
                    | "COM7"
                    | "COM8"
                    | "COM9"
                    | "LPT1"
                    | "LPT2"
                    | "LPT3"
                    | "LPT4"
                    | "LPT5"
                    | "LPT6"
                    | "LPT7"
                    | "LPT8"
                    | "LPT9"
            ) || component.ends_with('.')
            {
                bail!("name component is reserved on Windows: {component}");
            }
        }
        Ok(())
    }

    fn relative_prompt_path(name: &str) -> Result<PathBuf> {
        if name.is_empty() || name.starts_with('/') || name.contains('\\') || name.contains(':') {
            bail!("invalid prompt name {name:?}");
        }
        let (team, text) = if let Some(text) = name.strip_prefix('@') {
            (true, text)
        } else {
            (false, name)
        };
        let parts: Vec<_> = text.split('/').collect();
        if team && parts.len() < 2 {
            bail!("team prompt must be @team/name");
        }
        for part in &parts {
            Self::validate_component(part)?;
        }
        if parts[..parts.len() - 1]
            .iter()
            .any(|part| part.ends_with(".versions"))
        {
            bail!("a snapshot directory cannot be a prompt namespace");
        }
        if parts.last().unwrap().len() + 3 > 255 {
            bail!("prompt filename is too long");
        }
        let mut relative = PathBuf::from(if team {
            "teams"
        } else if parts.len() > 1 {
            "banks"
        } else {
            "prompts"
        });
        for part in &parts[..parts.len() - 1] {
            relative.push(part);
        }
        relative.push(format!("{}.md", parts.last().unwrap()));
        Ok(relative)
    }

    pub fn prompt_path(&self, name: &str) -> Result<PathBuf> {
        self.validate_path(&self.base_dir.join(Self::relative_prompt_path(name)?))
    }

    /// Validate a managed path without following any symlink beneath the root.
    pub fn validate_path(&self, path: &Path) -> Result<PathBuf> {
        let relative = path
            .strip_prefix(&self.base_dir)
            .context("path is outside the selected store")?;
        if relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        {
            bail!("unsafe path: {}", path.display());
        }
        if !self.base_dir.exists() {
            return Ok(path.to_path_buf());
        }
        let mut directory = Directory::open(&self.base_dir, false)?;
        let parts: Vec<_> = relative.components().collect();
        for (index, part) in parts.iter().enumerate() {
            let name = part.as_os_str();
            // Filesystems may resolve a differently cased path to an existing
            // file. Canonical selectors must still use the stored spelling.
            if !directory.entries()?.iter().any(|entry| entry == name) {
                match directory.metadata(name) {
                    Ok(_) => {
                        return Err(io::Error::new(
                            io::ErrorKind::NotFound,
                            format!("no exact stored spelling for {}", path.display()),
                        )
                        .into())
                    }
                    Err(error) if is_missing(&error) => {}
                    Err(error) => return Err(error),
                }
            }
            if index + 1 == parts.len() {
                match directory.metadata(name) {
                    Ok(meta) if !meta.is_file() => bail!("not a regular file: {}", path.display()),
                    Ok(_) => {}
                    Err(error) if is_missing(&error) => {}
                    Err(error) => return Err(error),
                }
            } else {
                match directory.child(name, false) {
                    Ok(next) => directory = next,
                    Err(error) if is_missing(&error) => break,
                    Err(error) => return Err(error),
                }
            }
        }
        Ok(path.to_path_buf())
    }

    fn parent(&self, relative: &Path, create: bool) -> Result<Directory> {
        Directory::open(&self.base_dir, create)?
            .descend(relative.parent().unwrap_or(Path::new("")), create)
    }

    pub fn read_safe_path(&self, path: &Path) -> Result<Vec<u8>> {
        let path = self.validate_path(path)?;
        let relative = path.strip_prefix(&self.base_dir)?;
        let parent = self.parent(relative, false)?;
        Ok(parent
            .read(
                relative
                    .file_name()
                    .context("managed path has no filename")?,
            )?
            .0)
    }

    pub fn read_raw(&self, name: &str) -> Result<Vec<u8>> {
        self.read_safe_path(&self.prompt_path(name)?)
    }

    pub fn read_prompt(&self, name: &str) -> Result<(PromptMetadata, String)> {
        let bytes = self.read_raw(name)?;
        let text = std::str::from_utf8(&bytes)
            .with_context(|| format!("prompt {name} is not valid UTF-8"))?;
        let (mut metadata, body) = self
            .parse_prompt_content(text)
            .with_context(|| format!("invalid prompt {name}"))?;
        if metadata.id.is_empty() {
            metadata.id = name.to_string();
        }
        if metadata.description.is_empty() {
            metadata.description = body
                .lines()
                .find(|line| !line.is_empty())
                .unwrap_or("No description")
                .chars()
                .take(80)
                .collect();
        }
        Ok((metadata, body))
    }

    pub fn parse_prompt_content(&self, content: &str) -> Result<(PromptMetadata, String)> {
        let Some(first_end) = content.find('\n') else {
            return Ok((PromptMetadata::default(), content.to_string()));
        };
        if content[..first_end].trim_end_matches('\r') != "---" {
            return Ok((PromptMetadata::default(), content.to_string()));
        }
        let mut position = first_end + 1;
        while position <= content.len() {
            let end = content[position..]
                .find('\n')
                .map(|index| position + index)
                .unwrap_or(content.len());
            if content[position..end].trim_end_matches('\r') == "---" {
                let yaml = &content[first_end + 1..position];
                let value: serde_yaml::Value =
                    serde_yaml::from_str(yaml).context("invalid YAML frontmatter")?;
                if !value.is_mapping() {
                    bail!("frontmatter must be a YAML mapping");
                }
                if let Some(vars) = value.get("vars") {
                    let vars = vars
                        .as_mapping()
                        .context("vars must be a mapping of names to string or null")?;
                    for (name, default) in vars {
                        if !matches!(name, serde_yaml::Value::String(_))
                            || !matches!(
                                default,
                                serde_yaml::Value::Null | serde_yaml::Value::String(_)
                            )
                        {
                            bail!("vars names must be strings and defaults must be string or null");
                        }
                    }
                }
                // Serde's flatten buffer cannot carry YAML enum/tag values.
                // Deserialize the known fields separately, retaining unknown YAML
                // nodes intact instead of passing them through that buffer.
                let mut fields = value.as_mapping().expect("mapping checked above").clone();
                let mut known = serde_yaml::Mapping::new();
                for name in [
                    "id",
                    "description",
                    "tags",
                    "created_at",
                    "updated_at",
                    "version",
                    "git_hash",
                    "parent_version",
                    "vars",
                ] {
                    let key = serde_yaml::Value::String(name.into());
                    if let Some(value) = fields.remove(&key) {
                        known.insert(key, value);
                    }
                }
                let mut metadata: PromptMetadata =
                    serde_yaml::from_value(serde_yaml::Value::Mapping(known))
                        .context("invalid prompt metadata")?;
                for (key, value) in fields {
                    let serde_yaml::Value::String(name) = key else {
                        bail!("frontmatter field names must be untagged strings");
                    };
                    metadata.extra.insert(name, value);
                }
                crate::template::validate_declarations(&metadata.vars)?;
                let mut start = if end < content.len() { end + 1 } else { end };
                // One optional conventional blank line separates frontmatter/body.
                if content[start..].starts_with("\r\n") {
                    start += 2;
                } else if content[start..].starts_with('\n') {
                    start += 1;
                }
                return Ok((metadata, content[start..].to_string()));
            }
            if end == content.len() {
                break;
            }
            position = end + 1;
        }
        bail!("frontmatter is missing its closing --- line")
    }

    pub fn serialize_prompt(metadata: &PromptMetadata, body: &str) -> Result<Vec<u8>> {
        crate::template::validate_declarations(&metadata.vars)?;
        Ok(format!("---\n{}---\n\n{}", serde_yaml::to_string(metadata)?, body).into_bytes())
    }

    fn validate_raw(&self, bytes: &[u8]) -> Result<()> {
        let text = std::str::from_utf8(bytes).context("prompt is not valid UTF-8")?;
        self.parse_prompt_content(text)?;
        Ok(())
    }

    pub fn create_prompt(&self, name: &str, metadata: &PromptMetadata, body: &str) -> Result<()> {
        self.write_raw(name, &Self::serialize_prompt(metadata, body)?, false)
    }

    pub fn write_prompt(&self, name: &str, metadata: &PromptMetadata, body: &str) -> Result<()> {
        self.write_raw(name, &Self::serialize_prompt(metadata, body)?, true)
    }

    pub fn write_raw(&self, name: &str, bytes: &[u8], overwrite: bool) -> Result<()> {
        Self::validate_name(name)?;
        self.validate_raw(bytes)?;
        let root = Directory::open(&self.base_dir, true)?;
        let _lock = root.lock_file()?;
        self.write_raw_locked(&root, name, bytes, overwrite, None)
    }

    fn write_raw_locked(
        &self,
        root: &Directory,
        name: &str,
        bytes: &[u8],
        overwrite: bool,
        expected: Option<&[u8]>,
    ) -> Result<()> {
        let relative = Self::relative_prompt_path(name)?;
        let parent = root.descend(relative.parent().unwrap_or(Path::new("")), true)?;
        let file = relative.file_name().unwrap();
        let existing = match parent.read(file) {
            Ok(value) => Some(value.0),
            Err(error) if is_missing(&error) => None,
            Err(error) => return Err(error),
        };
        if let Some(expected) = expected {
            if existing.as_deref() != Some(expected) {
                return self.save_conflict(root, name, bytes);
            }
        }
        if existing.is_some() && !overwrite {
            bail!("prompt {name:?} already exists");
        }
        if !overwrite && existing.is_none() && self.has_snapshot_locked(root, name)? {
            bail!("prompt {name:?} has recovery history; restore it or explicitly choose a different name");
        }
        if let Some(original) = &existing {
            self.snapshot_bytes_locked(root, name, &automatic_tag(), original)?;
        }
        match parent.write_atomic(file, bytes, overwrite, existing.as_deref()) {
            Err(error) if error.downcast_ref::<ConflictError>().is_some() => {
                self.save_conflict(root, name, bytes)
            }
            result => result,
        }
    }

    fn save_conflict(&self, root: &Directory, name: &str, bytes: &[u8]) -> Result<()> {
        let relative = Self::relative_prompt_path(name)?;
        let directory = root.descend(
            &PathBuf::from(".conflicts").join(relative.with_extension("")),
            true,
        )?;
        let file = format!("{}.md", uuid::Uuid::new_v4());
        directory
            .write_atomic(OsStr::new(&file), bytes, false, None)
            .context("edit conflict; could not preserve the proposed content")?;
        Err(ConflictError {
            path: self.base_dir.join(relative),
            recovery: Some(directory.path().join(file)),
        }
        .into())
    }

    pub fn replace_raw_if_unchanged(
        &self,
        name: &str,
        bytes: &[u8],
        expected: &[u8],
    ) -> Result<()> {
        Self::validate_name(name)?;
        self.validate_raw(bytes)?;
        let root = Directory::open(&self.base_dir, false)?;
        let _lock = root.lock_file()?;
        self.write_raw_locked(&root, name, bytes, true, Some(expected))
    }

    pub fn replace_if_unchanged(
        &self,
        name: &str,
        metadata: &PromptMetadata,
        body: &str,
        expected: &[u8],
    ) -> Result<()> {
        let bytes = if std::str::from_utf8(expected)?.starts_with("---\n")
            || std::str::from_utf8(expected)?.starts_with("---\r\n")
        {
            Self::serialize_prompt(metadata, body)?
        } else {
            body.as_bytes().to_vec()
        };
        self.replace_raw_if_unchanged(name, &bytes, expected)
    }

    pub fn read_prompt_metadata(&self, name: &str) -> Result<PromptMetadata> {
        Ok(self.read_prompt(name)?.0)
    }
    pub fn write_prompt_metadata(&self, name: &str, metadata: &PromptMetadata) -> Result<()> {
        let raw = self.read_raw(name)?;
        let (_, body) = self.parse_prompt_content(std::str::from_utf8(&raw)?)?;
        self.replace_raw_if_unchanged(name, &Self::serialize_prompt(metadata, &body)?, &raw)
    }

    pub fn versions_dir(&self, name: &str) -> Result<PathBuf> {
        let relative = Self::relative_prompt_path(name)?.with_extension("");
        self.checked_directory_path(&PathBuf::from(".versions").join(relative))
    }

    fn checked_directory_path(&self, relative: &Path) -> Result<PathBuf> {
        if relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        {
            bail!("unsafe managed directory");
        }
        if self.base_dir.exists() {
            let mut directory = Directory::open(&self.base_dir, false)?;
            for part in relative.components() {
                match directory.child(part.as_os_str(), false) {
                    Ok(next) => directory = next,
                    Err(error) if is_missing(&error) => break,
                    Err(error) => return Err(error),
                }
            }
        }
        Ok(self.base_dir.join(relative))
    }

    fn snapshot_bytes_locked(
        &self,
        root: &Directory,
        name: &str,
        tag: &str,
        bytes: &[u8],
    ) -> Result<PathBuf> {
        Self::validate_component(tag)?;
        if tag.len() + 3 > 255 {
            bail!("version tag is too long");
        }
        let relative =
            PathBuf::from(".versions").join(Self::relative_prompt_path(name)?.with_extension(""));
        let directory = root.descend(&relative, true)?;
        let filename = format!("{tag}.md");
        directory
            .write_atomic(OsStr::new(&filename), bytes, false, None)
            .context("cannot create required recovery snapshot; current prompt is unchanged")?;
        Ok(directory.path().join(filename))
    }

    fn has_snapshot_locked(&self, _root: &Directory, name: &str) -> Result<bool> {
        Ok(!self.snapshot_paths(name)?.is_empty())
    }

    pub fn snapshot(&self, name: &str, tag: &str) -> Result<PathBuf> {
        Self::validate_component(tag)?;
        let root = Directory::open(&self.base_dir, false)?;
        let _lock = root.lock_file()?;
        if self
            .snapshot_paths(name)?
            .iter()
            .any(|(existing, _)| existing == tag)
        {
            bail!("version {tag:?} already exists for {name}");
        }
        let relative = Self::relative_prompt_path(name)?;
        let parent = root.descend(relative.parent().unwrap_or(Path::new("")), false)?;
        let bytes = parent.read(relative.file_name().unwrap())?.0;
        self.snapshot_bytes_locked(&root, name, tag, &bytes)
    }

    pub fn snapshot_paths(&self, name: &str) -> Result<Vec<(String, PathBuf)>> {
        let relative = Self::relative_prompt_path(name)?;
        let mut directories = vec![self.versions_dir(name)?];
        let prompt = self.base_dir.join(&relative);
        let parent = prompt.parent().unwrap();
        directories.push(parent.join(format!("{name}.versions")));
        directories.push(prompt.with_extension("versions"));
        directories.sort();
        directories.dedup();
        let mut result = Vec::new();
        for path in directories {
            let relative = path
                .strip_prefix(&self.base_dir)
                .context("legacy snapshot directory escaped storage")?;
            self.checked_directory_path(relative)?;
            let directory = match Directory::open(&self.base_dir, false)
                .and_then(|root| root.descend(relative, false))
            {
                Ok(directory) => directory,
                Err(error) if is_missing(&error) => continue,
                Err(error) => return Err(error),
            };
            for entry in directory.entries()? {
                if Path::new(&entry).extension() != Some(OsStr::new("md")) {
                    continue;
                }
                directory.open_file(&entry)?;
                let tag = Path::new(&entry)
                    .file_stem()
                    .and_then(OsStr::to_str)
                    .context("snapshot tag is not UTF-8")?;
                Self::validate_component(tag)?;
                result.push((tag.to_string(), directory.path().join(entry)));
            }
        }
        result.sort();
        Ok(result)
    }

    pub fn delete_prompt(&self, name: &str) -> Result<()> {
        let relative = Self::relative_prompt_path(name)?;
        let root = Directory::open(&self.base_dir, false)?;
        let _lock = root.lock_file()?;
        let parent = root.descend(relative.parent().unwrap_or(Path::new("")), false)?;
        let file = relative.file_name().unwrap();
        let original = parent.read(file)?.0;
        self.snapshot_bytes_locked(&root, name, &automatic_tag(), &original)?;
        if parent.read(file)?.0 != original {
            return Err(ConflictError {
                path: self.base_dir.join(relative),
                recovery: None,
            }
            .into());
        }
        parent.remove(file)
    }

    pub fn rename_prompt(&self, old: &str, new: &str) -> Result<()> {
        let old_relative = Self::relative_prompt_path(old)?;
        let new_relative = Self::relative_prompt_path(new)?;
        if old_relative == new_relative {
            bail!("source and destination prompt names are the same");
        }
        let root = Directory::open(&self.base_dir, false)?;
        let _lock = root.lock_file()?;
        let source = root.descend(old_relative.parent().unwrap_or(Path::new("")), false)?;
        let (original, metadata) = source.read(old_relative.file_name().unwrap())?;
        if metadata.permissions().readonly() {
            bail!("permission denied: source prompt is read-only");
        }
        let destination = root.descend(new_relative.parent().unwrap_or(Path::new("")), true)?;
        match destination.open_file(new_relative.file_name().unwrap()) {
            Ok(_) => bail!("prompt {new:?} already exists"),
            Err(error) if is_missing(&error) => {}
            Err(error) => return Err(error),
        }
        if self.has_snapshot_locked(&root, new)? {
            bail!("prompt {new:?} has recovery history; choose a different destination");
        }
        let compatibility = self.renamed_compatibility(&root, old, new)?;
        self.snapshot_bytes_locked(&root, old, &automatic_tag(), &original)?;
        if let Some((old_metadata, _)) = &compatibility {
            let backups = root.child(OsStr::new(".metadata-backups"), true)?;
            backups.write_atomic(
                OsStr::new(&format!("{}-compatibility.toml", uuid::Uuid::new_v4())),
                old_metadata,
                false,
                None,
            )?;
        }
        let finish = || -> Result<()> {
            destination.write_atomic(new_relative.file_name().unwrap(), &original, false, None)?;
            if let Some((previous, updated)) = compatibility {
                root.write_atomic(
                    OsStr::new("compatibility.toml"),
                    &updated,
                    true,
                    Some(&previous),
                )?;
            }
            if source.read(old_relative.file_name().unwrap())?.0 != original {
                return Err(ConflictError {
                    path: self.base_dir.join(old_relative.clone()),
                    recovery: Some(self.base_dir.join(&new_relative)),
                }
                .into());
            }
            source.remove(old_relative.file_name().unwrap())
        };
        finish().with_context(|| format!("rename incomplete; complete copies remain at {} and {} when present; do not delete either until inspected", self.base_dir.join(old_relative).display(), self.base_dir.join(new_relative).display()))
    }

    fn renamed_compatibility(
        &self,
        root: &Directory,
        old: &str,
        new: &str,
    ) -> Result<Option<(Vec<u8>, Vec<u8>)>> {
        let bytes = match root.read(OsStr::new("compatibility.toml")) {
            Ok((bytes, _)) => bytes,
            Err(error) if is_missing(&error) => return Ok(None),
            Err(error) => return Err(error),
        };
        let mut value: toml::Value = toml::from_str(std::str::from_utf8(&bytes)?)?;
        if value.get("version").and_then(toml::Value::as_integer) != Some(1)
            || value.get("verified").and_then(toml::Value::as_bool) != Some(true)
        {
            return Ok(None);
        }
        let mut changed = false;
        if let Some(bindings) = value.get_mut("bindings") {
            let bindings = bindings
                .as_table_mut()
                .context("verified compatibility bindings must be a table")?;
            for (_, binding) in bindings.iter_mut() {
                let target = binding
                    .as_str()
                    .context("verified compatibility targets must be strings")?;
                if target == old {
                    *binding = toml::Value::String(new.to_string());
                    changed = true;
                }
            }
        }
        Ok(if changed {
            Some((bytes, toml::to_string(&value)?.into_bytes()))
        } else {
            None
        })
    }

    pub fn list_prompts(&self) -> Result<Vec<String>> {
        if !self.base_dir.exists() {
            return Ok(Vec::new());
        }
        let root = Directory::open(&self.base_dir, false)?;
        let mut prompts = Vec::new();
        for (directory, prefix, recursive) in [
            ("prompts", "", false),
            ("banks", "", true),
            ("teams", "@", true),
        ] {
            let directory = match root.child(OsStr::new(directory), false) {
                Ok(value) => value,
                Err(error) if is_missing(&error) => continue,
                Err(error) => return Err(error),
            };
            Self::collect_prompts(&directory, prefix, recursive, &mut prompts)?;
        }
        prompts.sort();
        prompts.dedup();
        Ok(prompts)
    }

    fn collect_prompts(
        directory: &Directory,
        prefix: &str,
        recursive: bool,
        result: &mut Vec<String>,
    ) -> Result<()> {
        for entry in directory.entries()? {
            let text = entry
                .to_str()
                .context("prompt filename is not valid UTF-8")?;
            if text.ends_with(".versions") {
                continue;
            }
            let metadata = directory.metadata(&entry).with_context(|| {
                format!("cannot inspect {}", directory.path().join(&entry).display())
            })?;
            if metadata.is_dir() {
                if recursive {
                    Self::validate_component(text)?;
                    let prefix = if prefix.is_empty() || prefix == "@" {
                        format!("{prefix}{text}")
                    } else {
                        format!("{prefix}/{text}")
                    };
                    Self::collect_prompts(&directory.child(&entry, false)?, &prefix, true, result)?;
                }
            } else if metadata.is_file()
                && Path::new(&entry).extension() == Some(OsStr::new("md"))
                && !(recursive && (prefix.is_empty() || prefix == "@"))
            {
                let stem = Path::new(&entry)
                    .file_stem()
                    .and_then(OsStr::to_str)
                    .context("prompt name is not valid UTF-8")?;
                let name = if prefix.is_empty() {
                    stem.to_string()
                } else {
                    format!("{prefix}/{stem}")
                };
                Self::validate_name(&name)?;
                result.push(name);
            }
        }
        Ok(())
    }

    pub fn prompt_exists(&self, name: &str) -> bool {
        self.read_raw(name).is_ok()
    }
    pub fn resolve_prompt(&self, name: &str) -> Result<String> {
        if self.prompt_exists(name) {
            Ok(name.to_string())
        } else {
            bail!("no exact prompt named {name:?}")
        }
    }
    pub fn resolve_prompt_fast(&self, name: &str) -> Result<String> {
        self.resolve_prompt(name)
    }
    pub fn list_bank_prompts(&self, bank: &str) -> Result<Vec<String>> {
        for part in bank.split('/') {
            Self::validate_component(part)?;
        }
        let prefix = format!("{bank}/");
        Ok(self
            .list_prompts()?
            .into_iter()
            .filter(|name| name.starts_with(&prefix))
            .collect())
    }
    pub fn list_teams(&self) -> Result<Vec<String>> {
        let mut teams: Vec<_> = self
            .list_prompts()?
            .into_iter()
            .filter_map(|name| {
                name.strip_prefix('@')
                    .and_then(|name| name.split('/').next())
                    .map(str::to_string)
            })
            .collect();
        teams.sort();
        teams.dedup();
        Ok(teams)
    }
    pub fn team_dir(&self, team: &str) -> Result<PathBuf> {
        Self::validate_component(team)?;
        self.checked_directory_path(&PathBuf::from("teams").join(team))
    }
    pub fn list_team_prompts(&self, team: &str) -> Result<Vec<String>> {
        Self::validate_component(team)?;
        let prefix = format!("@{team}/");
        Ok(self
            .list_prompts()?
            .into_iter()
            .filter(|name| name.starts_with(&prefix))
            .collect())
    }
    pub fn write_team_prompt(
        &self,
        team: &str,
        name: &str,
        metadata: &PromptMetadata,
        body: &str,
    ) -> Result<()> {
        self.write_prompt(&format!("@{team}/{name}"), metadata, body)
    }
    pub fn read_team_prompt(&self, team: &str, name: &str) -> Result<(PromptMetadata, String)> {
        self.read_prompt(&format!("@{team}/{name}"))
    }
    pub fn delete_team_prompt(&self, team: &str, name: &str) -> Result<()> {
        self.delete_prompt(&format!("@{team}/{name}"))
    }
    pub fn team_prompt_exists(&self, team: &str, name: &str) -> bool {
        self.prompt_exists(&format!("@{team}/{name}"))
    }
}

fn is_missing(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<io::Error>()
            .is_some_and(|error| error.kind() == io::ErrorKind::NotFound)
    })
}
fn automatic_tag() -> String {
    format!(
        "auto-{}-{}",
        chrono::Utc::now().format("%Y%m%dT%H%M%S%f"),
        uuid::Uuid::new_v4()
    )
}

const STARTERS: &[(&str, &str)] = &[
    (
        "essentials/commit",
        r#"---
id: commit
description: Generate conventional commit message from diff
---

Based on the git diff below, generate a concise commit message following conventional commit format.

Format: type(scope): description

Types: feat, fix, docs, style, refactor, test, chore

Keep under 50 characters, use imperative mood.

Git diff:
{input}"#,
    ),
    (
        "essentials/debug",
        r#"---
id: debug
description: Analyze errors and suggest fixes
---

Analyze the provided error and:

1. **Identify the root cause** - What exactly is failing and why
2. **Explain in simple terms** - Make it understandable 
3. **Suggest immediate fixes** - Quick solutions to try
4. **Recommend best practices** - How to prevent this
5. **Provide code examples** - Show the fix, don't just describe

Error details:
{input}"#,
    ),
    (
        "essentials/review",
        r#"---
id: review
description: Comprehensive code review
---

Review the provided code focusing on:

## Critical Issues
- Security vulnerabilities
- Performance problems
- Logic errors

## Code Quality
- Readability and clarity
- Design patterns
- Error handling

## Suggestions
- Specific improvements with examples
- Alternative approaches

Code to review:
{input}"#,
    ),
    (
        "10x/afk-task",
        r#"---
id: afk-task
description: Long-running autonomous task for calmhive
---

You are an autonomous AI agent tasked with completing the following objective independently:

{input}

## Instructions:
1. Break down the task into concrete steps
2. Execute each step systematically
3. Handle errors gracefully and retry if needed
4. Document your progress and decisions
5. Complete the entire task without human intervention

## Success Criteria:
- All subtasks completed
- Output is functional and tested
- Code follows best practices
- Documentation is clear

Begin by analyzing the task and creating an execution plan."#,
    ),
    (
        "10x/fix-tests",
        r#"---
id: fix-tests
description: Systematically fix all failing tests
---

Analyze and fix the failing tests:

{input}

## Approach:
1. Identify why each test is failing
2. Determine if test or implementation needs fixing
3. Make minimal changes to fix
4. Ensure no regressions
5. Run tests to verify

## For each failure:
- Root cause
- Fix applied
- Verification steps

Fix tests one by one, showing your work."#,
    ),
    (
        "10x/refactor",
        r#"---
id: refactor
description: Refactor code for clarity and performance
---

Refactor this code to improve:

{input}

## Goals:
1. **Clarity** - Make intent obvious
2. **Performance** - Optimize hot paths
3. **Maintainability** - Reduce complexity
4. **Testability** - Make it easy to test

## Constraints:
- Maintain all existing functionality
- Keep public API stable
- Add comments for complex logic

Provide the refactored code with explanations."#,
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, Storage) {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new_with_base(temp.path().join("store")).unwrap();
        fs::create_dir(storage.base_dir()).unwrap();
        (temp, storage)
    }

    #[test]
    fn checked_names_reject_traversal_and_preserve_unicode() {
        let (_temp, storage) = store();
        for name in [
            "",
            ".",
            "..",
            "../x",
            "a/../x",
            "a//x",
            "/x",
            "@t",
            "@t/../x",
            "C:x",
            "C:/x",
            "a\\x",
            "bank.versions/x",
        ] {
            assert!(storage.prompt_path(name).is_err(), "accepted {name:?}");
            assert!(storage.write_raw(name, b"payload", false).is_err());
        }
        assert!(storage.prompt_path(&"x".repeat(253)).is_err());
        assert!(storage
            .prompt_path(&format!("{}/x", "x".repeat(256)))
            .is_err());
        storage
            .write_raw("café/αναφορά", b"  note\n\n", false)
            .unwrap();
        assert_eq!(storage.read_raw("café/αναφορά").unwrap(), b"  note\n\n");
    }

    #[test]
    fn existing_empty_store_is_never_seeded_or_given_remote_config() {
        let (_temp, storage) = store();
        storage.init().unwrap();
        assert_eq!(fs::read_dir(storage.base_dir()).unwrap().count(), 0);
        let empty = Storage::new_with_base(storage.base_dir().join("empty")).unwrap();
        fs::create_dir(empty.base_dir()).unwrap();
        empty.init().unwrap();
        assert!(empty.list_prompts().unwrap().is_empty());
    }

    #[test]
    fn new_store_has_only_local_starters_and_no_reseeding() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new_with_base(temp.path().join("new")).unwrap();
        storage.init().unwrap();
        assert_eq!(storage.list_prompts().unwrap().len(), 6);
        let name = storage.list_prompts().unwrap().remove(0);
        storage.delete_prompt(&name).unwrap();
        storage.init().unwrap();
        assert_eq!(storage.list_prompts().unwrap().len(), 5);
        for name in [
            "config.toml",
            "registry.json",
            "credentials.json",
            "history.json",
        ] {
            assert!(!storage.base_dir().join(name).exists());
        }
    }

    #[test]
    fn frontmatter_preserves_unknown_fields_declared_vars_and_body_whitespace() {
        let (_temp, storage) = store();
        let text = "---\r\nid: old\r\ndescription: kept\r\nvars:\r\n  language: null\r\n  style: 'plain'\r\ncustom:\r\n  nested: [1, true, x]\r\n---\r\n\r\n\r\n  leading\r\ntrailing  \r\n\r\n";
        storage.write_raw("note", text.as_bytes(), false).unwrap();
        assert_eq!(storage.read_raw("note").unwrap(), text.as_bytes());
        let (metadata, body) = storage.read_prompt("note").unwrap();
        assert_eq!(body, "\r\n  leading\r\ntrailing  \r\n\r\n");
        assert_eq!(metadata.vars["language"], None);
        assert_eq!(metadata.vars["style"], Some("plain".into()));
        let serialized = Storage::serialize_prompt(&metadata, &body).unwrap();
        let (roundtrip, roundtrip_body) = storage
            .parse_prompt_content(std::str::from_utf8(&serialized).unwrap())
            .unwrap();
        assert_eq!(roundtrip.extra, metadata.extra);
        assert_eq!(roundtrip.vars, metadata.vars);
        assert_eq!(roundtrip_body, body);
    }

    #[test]
    fn unknown_yaml_tags_are_inert_and_survive_metadata_mutations() {
        let (_temp, storage) = store();
        let text = "---\nid: note\ncustom: !include /must-not-be-read\nnested: {object: !arbitrary [one, two]}\n---\n\nBody {input}\n";
        storage.write_raw("note", text.as_bytes(), false).unwrap();
        let (metadata, body) = storage.read_prompt("note").unwrap();
        assert_eq!(body, "Body {input}\n");
        assert!(matches!(
            metadata.extra["custom"],
            serde_yaml::Value::Tagged(_)
        ));
        let serialized = Storage::serialize_prompt(&metadata, &body).unwrap();
        let (roundtrip, roundtrip_body) = storage
            .parse_prompt_content(std::str::from_utf8(&serialized).unwrap())
            .unwrap();
        assert_eq!(roundtrip.extra, metadata.extra);
        assert_eq!(roundtrip_body, body);
        storage.rename_prompt("note", "notes/renamed").unwrap();
        let (renamed, renamed_body) = storage.read_prompt("notes/renamed").unwrap();
        assert_eq!(renamed.extra, metadata.extra);
        assert_eq!(renamed_body, body);
        assert_eq!(renamed.id, metadata.id);
        assert!(storage
            .parse_prompt_content("---\nvars: {language: !custom Rust}\n---\nBody")
            .is_err());
    }

    #[test]
    fn yaml_variables_require_string_or_null_and_valid_nonreserved_identifiers() {
        let (_temp, storage) = store();
        for declaration in [
            "language: true",
            "language: 1",
            "1: null",
            "bad-name: null",
            "input: null",
            "date: null",
        ] {
            let text = format!("---\nvars:\n  {declaration}\n---\n\nbody");
            assert!(
                storage.parse_prompt_content(&text).is_err(),
                "accepted {declaration}"
            );
        }
        assert!(storage
            .parse_prompt_content("---\nvars: []\n---\nbody")
            .is_err());
        assert!(storage
            .parse_prompt_content("---\nvars: {a: null}\nbody")
            .is_err());
        let (_, body) = storage
            .parse_prompt_content("---\nvars: {language: 'true', topic: null}\n---\nbody\n")
            .unwrap();
        assert_eq!(body, "body\n");
    }

    #[test]
    fn plain_bank_team_and_hidden_notes_remain_exact_and_listed() {
        let (_temp, storage) = store();
        let ids = [
            "note",
            ".note",
            "daily/review",
            "daily/sub/note",
            "@local/meeting",
            ".private/note",
        ];
        for id in ids {
            storage
                .write_raw(id, b"  untouched note\n\n", false)
                .unwrap();
        }
        let prompts = storage.list_prompts().unwrap();
        assert_eq!(prompts.len(), ids.len());
        for id in ids {
            assert!(prompts.contains(&id.to_owned()), "missing {id}");
            assert_eq!(storage.read_prompt(id).unwrap().1, "  untouched note\n\n");
        }
        fs::write(
            storage.base_dir().join("banks/README.md"),
            b"namespace docs",
        )
        .unwrap();
        fs::write(
            storage.base_dir().join("teams/README.md"),
            b"namespace docs",
        )
        .unwrap();
        assert_eq!(storage.list_prompts().unwrap(), prompts);
    }

    #[test]
    fn replacement_and_deletion_keep_exact_preimages_for_later_restore() {
        let (_temp, storage) = store();
        let original = b"---\nid: note\ncustom: [1, true]\n---\n\n  original\n\n";
        storage.write_raw("note", original, false).unwrap();
        assert!(storage.write_raw("note", b"collision", false).is_err());
        assert!(storage.snapshot_paths("note").unwrap().is_empty());
        storage.write_raw("note", b"next note\n", true).unwrap();
        let snapshots = storage.snapshot_paths("note").unwrap();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(storage.read_safe_path(&snapshots[0].1).unwrap(), original);
        assert_eq!(storage.list_prompts().unwrap(), vec!["note"]);
        storage.delete_prompt("note").unwrap();
        assert!(storage.list_prompts().unwrap().is_empty());
        assert_eq!(storage.snapshot_paths("note").unwrap().len(), 2);
        assert!(storage
            .write_raw("note", b"accidental new ID", false)
            .is_err());
        storage.write_raw("note", original, true).unwrap();
        assert_eq!(storage.read_raw("note").unwrap(), original);
        assert_eq!(storage.snapshot_paths("note").unwrap().len(), 2);
    }

    #[test]
    fn stale_edit_preserves_proposal_and_keeps_the_external_edit() {
        let (_temp, storage) = store();
        storage.write_raw("note", b"original", false).unwrap();
        let expected = storage.read_raw("note").unwrap();
        fs::write(storage.prompt_path("note").unwrap(), b"external change").unwrap();
        let error = storage
            .replace_raw_if_unchanged("note", b"proposed edit", &expected)
            .unwrap_err();
        let conflict = error
            .downcast_ref::<ConflictError>()
            .expect("typed conflict");
        assert_eq!(
            storage
                .read_safe_path(conflict.recovery.as_ref().unwrap())
                .unwrap(),
            b"proposed edit"
        );
        assert_eq!(storage.read_raw("note").unwrap(), b"external change");
        assert!(storage.snapshot_paths("note").unwrap().is_empty());
    }

    #[test]
    fn concurrent_cooperating_edits_cannot_silently_overwrite_each_other() {
        let (_temp, storage) = store();
        storage.write_raw("note", b"original", false).unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let jobs: Vec<_> = [b"first".as_slice(), b"second".as_slice()]
            .into_iter()
            .map(|proposal| {
                let storage = storage.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    storage.replace_raw_if_unchanged("note", proposal, b"original")
                })
            })
            .collect();
        let results: Vec<_> = jobs.into_iter().map(|job| job.join().unwrap()).collect();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert!(results
            .iter()
            .find_map(|result| result.as_ref().err())
            .unwrap()
            .downcast_ref::<ConflictError>()
            .is_some());
        let current = storage.read_raw("note").unwrap();
        assert!(current == b"first" || current == b"second");
        let snapshots = storage.snapshot_paths("note").unwrap();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(
            storage.read_safe_path(&snapshots[0].1).unwrap(),
            b"original"
        );
    }

    #[test]
    fn blocked_required_preimage_aborts_replace_delete_and_rename() {
        let (_temp, storage) = store();
        storage.write_raw("note", b"original", false).unwrap();
        fs::write(storage.base_dir().join(".versions"), b"block snapshots").unwrap();
        assert!(storage.write_raw("note", b"replacement", true).is_err());
        assert!(storage.delete_prompt("note").is_err());
        assert!(storage.rename_prompt("note", "renamed").is_err());
        assert_eq!(storage.read_raw("note").unwrap(), b"original");
        assert!(!storage.prompt_exists("renamed"));
    }

    #[test]
    fn rename_preserves_raw_bytes_updates_verified_bindings_and_keeps_metadata_preimage() {
        let (_temp, storage) = store();
        let original = b"---\nid: old-authored-id\nunknown: keep\n---\n\n  full body\n\n";
        storage.write_raw("daily/old", original, false).unwrap();
        let compatibility = b"version = 1\nverified = true\ncustom = 'keep'\n[bindings]\no = 'daily/old'\nother = 'unrelated'\n";
        fs::write(storage.base_dir().join("compatibility.toml"), compatibility).unwrap();
        storage.rename_prompt("daily/old", "new").unwrap();
        assert!(!storage.prompt_exists("daily/old"));
        assert_eq!(storage.read_raw("new").unwrap(), original);
        let updated: toml::Value = toml::from_str(
            &fs::read_to_string(storage.base_dir().join("compatibility.toml")).unwrap(),
        )
        .unwrap();
        assert_eq!(updated["bindings"]["o"].as_str(), Some("new"));
        assert_eq!(updated["bindings"]["other"].as_str(), Some("unrelated"));
        assert_eq!(updated["custom"].as_str(), Some("keep"));
        let snapshots = storage.snapshot_paths("daily/old").unwrap();
        assert_eq!(storage.read_safe_path(&snapshots[0].1).unwrap(), original);
        let metadata_backups: Vec<_> = fs::read_dir(storage.base_dir().join(".metadata-backups"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(metadata_backups.len(), 1);
        assert_eq!(fs::read(&metadata_backups[0]).unwrap(), compatibility);
    }

    #[test]
    fn rename_does_not_rewrite_unverified_bindings_or_clobber_a_destination() {
        let (_temp, storage) = store();
        storage.write_raw("old", b"source", false).unwrap();
        storage
            .write_raw("occupied", b"destination", false)
            .unwrap();
        assert!(storage.rename_prompt("old", "occupied").is_err());
        assert_eq!(storage.read_raw("occupied").unwrap(), b"destination");
        let compatibility = b"version = 1\nverified = false\n[bindings]\no = 'old'\n";
        fs::write(storage.base_dir().join("compatibility.toml"), compatibility).unwrap();
        storage.rename_prompt("old", "new").unwrap();
        assert_eq!(
            fs::read(storage.base_dir().join("compatibility.toml")).unwrap(),
            compatibility
        );
    }

    #[test]
    fn old_snapshot_layouts_and_deleted_prompt_history_remain_readable() {
        let (_temp, storage) = store();
        let legacy = storage.base_dir().join("banks/daily/daily/review.versions");
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("v1.md"), b"legacy bytes").unwrap();
        let conventional = storage.base_dir().join("banks/daily/review.versions");
        fs::create_dir_all(&conventional).unwrap();
        fs::write(conventional.join("v2.md"), b"second legacy bytes").unwrap();
        let snapshots = storage.snapshot_paths("daily/review").unwrap();
        assert_eq!(snapshots.len(), 2);
        assert_eq!(
            snapshots
                .iter()
                .map(|(tag, _)| tag.as_str())
                .collect::<Vec<_>>(),
            vec!["v1", "v2"]
        );
        assert_eq!(
            storage.read_safe_path(&snapshots[0].1).unwrap(),
            b"legacy bytes"
        );
        assert!(storage.list_prompts().unwrap().is_empty());
        for tag in ["", ".", "..", "../../x", "/outside", "bad\\tag"] {
            assert!(storage.snapshot("note", tag).is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_is_rejected_for_read_write_snapshot_delete_and_rename() {
        let (_temp, storage) = store();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.md"), b"outside original").unwrap();
        fs::create_dir(storage.base_dir().join("banks")).unwrap();
        std::os::unix::fs::symlink(outside.path(), storage.base_dir().join("banks/escape"))
            .unwrap();
        for action in [
            storage.read_raw("escape/secret").map(|_| ()),
            storage.write_raw("escape/secret", b"bad", true),
            storage.snapshot("escape/secret", "v1").map(|_| ()),
            storage.delete_prompt("escape/secret"),
            storage.rename_prompt("escape/secret", "stolen"),
        ] {
            assert!(action.is_err());
        }
        storage.write_raw("note", b"local", false).unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("secret.md"),
            storage.base_dir().join("prompts/link.md"),
        )
        .unwrap();
        assert!(storage.read_raw("link").is_err());
        assert!(storage.write_raw("link", b"bad", true).is_err());
        assert!(storage.rename_prompt("note", "link").is_err());
        assert_eq!(
            fs::read(outside.path().join("secret.md")).unwrap(),
            b"outside original"
        );
        assert_eq!(storage.read_raw("note").unwrap(), b"local");
    }

    #[cfg(unix)]
    #[test]
    fn read_only_prompts_refuse_mutation_and_keep_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let (_temp, storage) = store();
        storage.write_raw("note", b"original", false).unwrap();
        let path = storage.prompt_path("note").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
        assert!(storage.write_raw("note", b"bad", true).is_err());
        assert!(storage.delete_prompt("note").is_err());
        assert!(storage.rename_prompt("note", "new").is_err());
        assert_eq!(storage.read_raw("note").unwrap(), b"original");
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o400
        );
    }
}
