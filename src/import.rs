use crate::storage::{PromptMetadata, Storage};
use anyhow::{Context, Result};
use regex::Regex;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

pub struct Importer {
    storage: Storage,
}

impl Importer {
    pub fn new(storage: Storage) -> Self {
        Self { storage }
    }

    /// Import prompts from a directory or file
    pub fn import_from_path(&self, path: &str) -> Result<ImportResult> {
        let path = Path::new(path);

        if !path.exists() {
            return Err(anyhow::anyhow!("Path does not exist: {}", path.display()));
        }

        let mut result = ImportResult::new();

        if path.is_file() {
            self.import_file(path, &mut result)?;
        } else if path.is_dir() {
            self.import_directory(path, &mut result)?;
        }

        Ok(result)
    }

    /// Enhanced import with custom naming, force overwrite, versioning, skip, and update support
    pub fn import_from_path_enhanced(
        &self,
        path: &str,
        custom_name: Option<&str>,
        force: bool,
        version: bool,
        skip: bool,
        update: bool,
    ) -> Result<ImportResult> {
        let path = Path::new(path);

        if !path.exists() {
            return Err(anyhow::anyhow!("Path does not exist: {}", path.display()));
        }

        let mut result = ImportResult::new();

        if path.is_file() {
            self.import_file_enhanced(
                path,
                custom_name,
                force,
                version,
                skip,
                update,
                &mut result,
            )?;
        } else if path.is_dir() {
            self.import_directory_enhanced(
                path,
                custom_name,
                force,
                version,
                skip,
                update,
                &mut result,
            )?;
        }

        Ok(result)
    }

    /// Import from Claude Code session files
    pub fn import_claude_session(&self, session_path: &str) -> Result<ImportResult> {
        let path = Path::new(session_path);
        let mut result = ImportResult::new();

        // Look for .md files that look like prompts
        for entry in WalkDir::new(path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("md"))
        {
            if self.looks_like_prompt(entry.path())? {
                self.import_file(entry.path(), &mut result)?;
            }
        }

        Ok(result)
    }

    fn import_file(&self, file_path: &Path, result: &mut ImportResult) -> Result<()> {
        let content = fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

        // Extract filename as prompt name
        let name = file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid filename: {}", file_path.display()))?;

        // Clean up name for prompt use
        let clean_name = self.clean_prompt_name(name);

        // Parse content
        let (metadata, body) = self.parse_content(&content, &clean_name, false)?;

        // Check if prompt already exists
        if self.storage.prompt_path(&clean_name).exists() {
            result.add_skipped(&clean_name, "Already exists");
            return Ok(());
        }

        // Write prompt
        self.storage.write_prompt(&clean_name, &metadata, &body)?;
        result.add_imported(&clean_name);

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn import_file_enhanced(
        &self,
        file_path: &Path,
        custom_name: Option<&str>,
        force: bool,
        version: bool,
        skip: bool,
        update: bool,
        result: &mut ImportResult,
    ) -> Result<()> {
        let content = fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

        // Determine prompt name
        let prompt_name = if let Some(name) = custom_name {
            // Use custom name (Gap 1 fix)
            self.clean_prompt_name(name)
        } else {
            // Extract filename as prompt name (original behavior)
            let name = file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| anyhow::anyhow!("Invalid filename: {}", file_path.display()))?;
            self.clean_prompt_name(name)
        };

        // Handle versioning
        let final_name = if version {
            self.find_next_version_name(&prompt_name)
        } else {
            prompt_name
        };

        // Parse content
        let (metadata, body) = self.parse_content(&content, &final_name, version)?;

        // Check if prompt already exists with enhanced conflict resolution
        let prompt_path = self.storage.prompt_path(&final_name);
        if prompt_path.exists() {
            if skip {
                result.add_skipped(&final_name, "Skipped (already exists)");
                return Ok(());
            } else if update {
                // Check if source is newer than target
                let source_modified = fs::metadata(file_path)?.modified()?;
                let target_modified = fs::metadata(&prompt_path)?.modified()?;

                if source_modified <= target_modified {
                    result.add_skipped(&final_name, "Skipped (target is newer or same)");
                    return Ok(());
                }
                // Continue with update since source is newer
            } else if !force {
                result.add_skipped(&final_name, "Already exists");
                return Ok(());
            }
        }

        // Write prompt (will overwrite if force=true)
        self.storage.write_prompt(&final_name, &metadata, &body)?;
        result.add_imported(&final_name);

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn import_directory_enhanced(
        &self,
        dir_path: &Path,
        custom_name: Option<&str>,
        force: bool,
        version: bool,
        skip: bool,
        update: bool,
        result: &mut ImportResult,
    ) -> Result<()> {
        // Check if this is a bank directory (contains bank.yaml or looks like a bank)
        let bank_yaml = dir_path.join("bank.yaml");
        let is_bank = bank_yaml.exists()
            || dir_path.starts_with("banks/")
            || (dir_path.parent().is_some_and(|p| p.ends_with("banks")));

        if is_bank {
            // Import as a bank - preserve structure
            let bank_name = custom_name.unwrap_or_else(|| {
                dir_path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("imported-bank")
            });

            self.import_bank(dir_path, bank_name, force, skip, update, result)?;
        } else {
            // Regular directory import - flatten structure
            for entry in WalkDir::new(dir_path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
            {
                let path = entry.path();

                // Skip hidden files and non-text files
                if path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .is_none_or(|s| s.starts_with('.'))
                {
                    continue;
                }

                // Only import text-like files
                if self.is_text_file(path)? {
                    if let Err(e) =
                        self.import_file_enhanced(path, None, force, version, skip, update, result)
                    {
                        result.add_error(path.to_string_lossy().to_string(), e.to_string());
                    }
                }
            }
        }

        Ok(())
    }

    fn import_directory(&self, dir_path: &Path, result: &mut ImportResult) -> Result<()> {
        for entry in WalkDir::new(dir_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();

            // Skip hidden files and non-text files
            if path
                .file_name()
                .and_then(|s| s.to_str())
                .is_none_or(|s| s.starts_with('.'))
            {
                continue;
            }

            // Only import text-like files
            if self.is_text_file(path)? {
                if let Err(e) = self.import_file(path, result) {
                    result.add_error(path.to_string_lossy().to_string(), e.to_string());
                }
            }
        }

        Ok(())
    }

    fn parse_content(
        &self,
        content: &str,
        name: &str,
        version_flag: bool,
    ) -> Result<(PromptMetadata, String)> {
        // Try to parse existing frontmatter
        if content.starts_with("---") {
            return self.storage.parse_prompt_content(content);
        }

        // Auto-generate metadata for files without frontmatter
        let description = self
            .extract_description(content)
            .unwrap_or_else(|| format!("Imported from {}", name));

        let metadata = PromptMetadata {
            id: name.to_string(),
            description,
            tags: Some(vec!["imported".to_string()]),
            created_at: Some(chrono::Utc::now().to_rfc3339()),
            updated_at: None,
            version: if version_flag {
                Some("v1.0".to_string())
            } else {
                None
            },
            git_hash: None,
            parent_version: None,
        };

        Ok((metadata, content.to_string()))
    }

    fn extract_description(&self, content: &str) -> Option<String> {
        let lines: Vec<&str> = content.lines().collect();

        // Look for first line that looks like a title or description
        for line in lines.iter().take(10) {
            let line = line.trim();

            // Skip empty lines
            if line.is_empty() {
                continue;
            }

            // If it starts with #, use as title
            if line.starts_with('#') {
                return Some(line.trim_start_matches('#').trim().to_string());
            }

            // If it's a reasonable length and looks descriptive
            if line.len() > 10 && line.len() < 100 && !line.starts_with("```") {
                return Some(line.to_string());
            }
        }

        None
    }

    fn clean_prompt_name(&self, name: &str) -> String {
        // Replace spaces and special chars with hyphens
        let re = Regex::new(r"[^a-zA-Z0-9\-_]").unwrap();
        let cleaned = re.replace_all(name, "-");

        // Remove multiple consecutive hyphens
        let re = Regex::new(r"-+").unwrap();
        let cleaned = re.replace_all(&cleaned, "-");

        // Trim hyphens from start/end
        cleaned.trim_matches('-').to_lowercase()
    }

    fn find_next_version_name(&self, base_name: &str) -> String {
        // Check if base name exists
        if !self.storage.prompt_path(base_name).exists() {
            return base_name.to_string();
        }

        // Find next available version (file-v2, file-v3, etc.)
        let mut version = 2;
        loop {
            let versioned_name = format!("{}-v{}", base_name, version);
            if !self.storage.prompt_path(&versioned_name).exists() {
                return versioned_name;
            }
            version += 1;

            // Safety check to prevent infinite loop
            if version > 1000 {
                return format!("{}-v{}", base_name, chrono::Utc::now().timestamp());
            }
        }
    }

    /// Import an entire bank preserving structure
    fn import_bank(
        &self,
        bank_path: &Path,
        bank_name: &str,
        force: bool,
        skip: bool,
        update: bool,
        result: &mut ImportResult,
    ) -> Result<()> {
        // Ensure bank directory exists in storage
        let target_bank_dir = self.storage.base_dir().join("banks").join(bank_name);
        fs::create_dir_all(&target_bank_dir)?;

        // Copy bank.yaml if it exists
        let bank_yaml_src = bank_path.join("bank.yaml");
        if bank_yaml_src.exists() {
            let bank_yaml_dest = target_bank_dir.join("bank.yaml");
            fs::copy(&bank_yaml_src, &bank_yaml_dest)?;
        }

        // Copy README.md if it exists
        let readme_src = bank_path.join("README.md");
        if readme_src.exists() {
            let readme_dest = target_bank_dir.join("README.md");
            fs::copy(&readme_src, &readme_dest)?;
        }

        // Import all markdown files in the bank
        for entry in fs::read_dir(bank_path)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md") {
                if let Some(file_name) = path.file_stem().and_then(|s| s.to_str()) {
                    // Skip README
                    if file_name.eq_ignore_ascii_case("readme") {
                        continue;
                    }

                    let prompt_name = format!("{}/{}", bank_name, file_name);

                    // Check if prompt already exists with enhanced conflict resolution
                    let prompt_path = self.storage.prompt_path(&prompt_name);
                    if prompt_path.exists() {
                        if skip {
                            result.add_skipped(&prompt_name, "Skipped (already exists)");
                            continue;
                        } else if update {
                            // Check if source is newer than target
                            let source_modified = fs::metadata(&path)?.modified()?;
                            let target_modified = fs::metadata(&prompt_path)?.modified()?;

                            if source_modified <= target_modified {
                                result
                                    .add_skipped(&prompt_name, "Skipped (target is newer or same)");
                                continue;
                            }
                            // Continue with update since source is newer
                        } else if !force {
                            result.add_skipped(&prompt_name, "Already exists");
                            continue;
                        }
                    }

                    // Read and parse the prompt
                    let content = fs::read_to_string(&path)?;
                    let (metadata, body) = self.parse_content(&content, file_name, false)?;

                    // Write prompt with bank prefix
                    self.storage.write_prompt(&prompt_name, &metadata, &body)?;
                    result.add_imported(&prompt_name);
                }
            } else if path.is_dir() {
                // Handle subdirectories (like static/ and dynamic/ in snippets bank)
                let subdir_name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("subdir");
                let subdir_target = target_bank_dir.join(subdir_name);
                fs::create_dir_all(&subdir_target)?;

                // Import files from subdirectory
                for subentry in fs::read_dir(&path)? {
                    let subentry = subentry?;
                    let subpath = subentry.path();

                    if subpath.is_file()
                        && subpath.extension().and_then(|s| s.to_str()) == Some("md")
                    {
                        if let Some(file_name) = subpath.file_stem().and_then(|s| s.to_str()) {
                            let prompt_name =
                                format!("{}/{}/{}", bank_name, subdir_name, file_name);

                            let prompt_path = self.storage.prompt_path(&prompt_name);
                            if prompt_path.exists() {
                                if skip {
                                    result.add_skipped(&prompt_name, "Skipped (already exists)");
                                    continue;
                                } else if update {
                                    // Check if source is newer than target
                                    let source_modified = fs::metadata(&subpath)?.modified()?;
                                    let target_modified = fs::metadata(&prompt_path)?.modified()?;

                                    if source_modified <= target_modified {
                                        result.add_skipped(
                                            &prompt_name,
                                            "Skipped (target is newer or same)",
                                        );
                                        continue;
                                    }
                                    // Continue with update since source is newer
                                } else if !force {
                                    result.add_skipped(&prompt_name, "Already exists");
                                    continue;
                                }
                            }

                            let content = fs::read_to_string(&subpath)?;
                            let (metadata, body) =
                                self.parse_content(&content, file_name, false)?;

                            self.storage.write_prompt(&prompt_name, &metadata, &body)?;
                            result.add_imported(&prompt_name);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn looks_like_prompt(&self, path: &Path) -> Result<bool> {
        let content = fs::read_to_string(path)?;

        // Check for prompt indicators
        let indicators = [
            "prompt",
            "instruction",
            "template",
            "system:",
            "user:",
            "assistant:",
            "{", // Has placeholders
        ];

        let content_lower = content.to_lowercase();
        Ok(indicators
            .iter()
            .any(|&indicator| content_lower.contains(indicator)))
    }

    fn is_text_file(&self, path: &Path) -> Result<bool> {
        let extensions = ["md", "txt", "prompt", "tmpl", "template"];

        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            return Ok(extensions.contains(&ext.to_lowercase().as_str()));
        }

        // Check if file looks like text by reading first few bytes
        let mut buffer = [0; 512];
        if let Ok(mut file) = fs::File::open(path) {
            use std::io::Read;
            if let Ok(bytes_read) = file.read(&mut buffer) {
                // Simple heuristic: if most bytes are printable ASCII, it's probably text
                let printable_count = buffer[..bytes_read]
                    .iter()
                    .filter(|&&b| b.is_ascii_graphic() || b.is_ascii_whitespace())
                    .count();

                return Ok(printable_count as f32 / bytes_read as f32 > 0.8);
            }
        }

        Ok(false)
    }
}

#[derive(Debug)]
pub struct ImportResult {
    imported: Vec<String>,
    skipped: Vec<(String, String)>,
    errors: Vec<(String, String)>,
}

impl ImportResult {
    fn new() -> Self {
        Self {
            imported: Vec::new(),
            skipped: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn add_imported(&mut self, name: &str) {
        self.imported.push(name.to_string());
    }

    fn add_skipped(&mut self, name: &str, reason: &str) {
        self.skipped.push((name.to_string(), reason.to_string()));
    }

    fn add_error(&mut self, file: String, error: String) {
        self.errors.push((file, error));
    }

    pub fn display(&self) {
        println!("Import complete");

        if !self.imported.is_empty() {
            println!("  Imported: {}", self.imported.len());
            for name in &self.imported {
                println!("    {}", name);
            }
        }

        if !self.skipped.is_empty() {
            println!("  Skipped: {}", self.skipped.len());
            for (name, reason) in &self.skipped {
                println!("    {} ({})", name, reason);
            }
        }

        if !self.errors.is_empty() {
            println!("  Failed: {}", self.errors.len());
            for (file, error) in &self.errors {
                println!("    {} ({})", file, error);
            }
        }
    }

    pub fn summary(&self) -> String {
        format!(
            "Imported: {}, Skipped: {}, Errors: {}",
            self.imported.len(),
            self.skipped.len(),
            self.errors.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_prompt_name() {
        let storage = Storage::new().unwrap();
        let importer = Importer::new(storage);

        assert_eq!(importer.clean_prompt_name("My Prompt!"), "my-prompt");
        assert_eq!(importer.clean_prompt_name("API Design"), "api-design");
        assert_eq!(importer.clean_prompt_name("test_file.md"), "test_file-md");
    }

    #[test]
    fn test_extract_description() {
        let storage = Storage::new().unwrap();
        let importer = Importer::new(storage);

        let content = "# API Design\n\nThis is a prompt for designing APIs";
        assert_eq!(importer.extract_description(content).unwrap(), "API Design");

        let content = "Design a REST API with the following requirements:";
        assert_eq!(
            importer.extract_description(content).unwrap(),
            "Design a REST API with the following requirements:"
        );
    }
}
