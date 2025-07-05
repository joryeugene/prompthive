//! Bidirectional file sync manager for PromptHive
//!
//! This module implements the core bidirectional sync functionality that allows
//! prompts to exist simultaneously in project directories (AI-accessible) and
//! PromptHive storage (command-accessible).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use chrono::{DateTime, Utc};

/// Metadata for a single sync relationship
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncMetadata {
    /// Local file path (in project directory)
    pub local_path: PathBuf,
    /// PromptHive prompt name
    pub prompt_name: String,
    /// SHA-256 hash of last synced content
    pub content_hash: String,
    /// Timestamp of last successful sync
    pub last_sync: DateTime<Utc>,
    /// Timestamp when sync relationship was created
    pub created: DateTime<Utc>,
    /// Whether sync is currently active
    pub active: bool,
}

/// Global sync registry
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncRegistry {
    /// Map of prompt names to sync metadata
    pub syncs: HashMap<String, SyncMetadata>,
    /// Reverse map of local paths to prompt names for fast lookup
    #[serde(skip)]
    pub path_index: HashMap<PathBuf, String>,
}

impl SyncRegistry {
    /// Load sync registry from storage
    pub fn load(storage: &crate::Storage) -> Result<Self> {
        let sync_path = storage.base_dir().join(".sync").join("registry.json");
        
        if sync_path.exists() {
            let content = fs::read_to_string(&sync_path)
                .with_context(|| format!("Failed to read sync registry from {:?}", sync_path))?;
            
            let mut registry: SyncRegistry = serde_json::from_str(&content)
                .with_context(|| "Failed to parse sync registry JSON")?;
            
            // Rebuild path index
            registry.rebuild_path_index();
            
            Ok(registry)
        } else {
            Ok(SyncRegistry::default())
        }
    }
    
    /// Save sync registry to storage
    pub fn save(&self, storage: &crate::Storage) -> Result<()> {
        let sync_dir = storage.base_dir().join(".sync");
        fs::create_dir_all(&sync_dir)
            .with_context(|| format!("Failed to create sync directory {:?}", sync_dir))?;
        
        let sync_path = sync_dir.join("registry.json");
        let content = serde_json::to_string_pretty(self)
            .with_context(|| "Failed to serialize sync registry")?;
        
        fs::write(&sync_path, content)
            .with_context(|| format!("Failed to write sync registry to {:?}", sync_path))?;
        
        Ok(())
    }
    
    /// Rebuild the path index for fast lookups
    fn rebuild_path_index(&mut self) {
        self.path_index.clear();
        for (prompt_name, sync_meta) in &self.syncs {
            self.path_index.insert(sync_meta.local_path.clone(), prompt_name.clone());
        }
    }
    
    /// Add or update a sync relationship
    pub fn add_sync(&mut self, prompt_name: String, local_path: PathBuf, content_hash: String) {
        let sync_meta = SyncMetadata {
            local_path: local_path.clone(),
            prompt_name: prompt_name.clone(),
            content_hash,
            last_sync: Utc::now(),
            created: Utc::now(),
            active: true,
        };
        
        // Remove old path index entry if it exists
        if let Some(old_sync) = self.syncs.get(&prompt_name) {
            self.path_index.remove(&old_sync.local_path);
        }
        
        // Add new entries
        self.syncs.insert(prompt_name.clone(), sync_meta);
        self.path_index.insert(local_path, prompt_name);
    }
    
    /// Remove a sync relationship
    pub fn remove_sync(&mut self, prompt_name: &str) -> Option<SyncMetadata> {
        if let Some(sync_meta) = self.syncs.remove(prompt_name) {
            self.path_index.remove(&sync_meta.local_path);
            Some(sync_meta)
        } else {
            None
        }
    }
    
    /// Check if a prompt is synced
    pub fn is_synced(&self, prompt_name: &str) -> bool {
        self.syncs.contains_key(prompt_name)
    }
    
    /// Get sync metadata for a prompt
    pub fn get_sync(&self, prompt_name: &str) -> Option<&SyncMetadata> {
        self.syncs.get(prompt_name)
    }
    
    /// Get prompt name for a local path
    pub fn get_prompt_for_path(&self, path: &Path) -> Option<&str> {
        self.path_index.get(path).map(|s| s.as_str())
    }
    
    /// Get all synced prompts
    pub fn get_all_synced(&self) -> Vec<&str> {
        self.syncs.keys().map(|s| s.as_str()).collect()
    }
    
    /// Update sync metadata after successful sync
    pub fn update_sync(&mut self, prompt_name: &str, content_hash: String) {
        if let Some(sync_meta) = self.syncs.get_mut(prompt_name) {
            sync_meta.content_hash = content_hash;
            sync_meta.last_sync = Utc::now();
        }
    }
}

/// Core bidirectional sync manager
pub struct SyncManager {
    storage: crate::Storage,
    registry: SyncRegistry,
}

impl SyncManager {
    /// Create a new sync manager
    pub fn new(storage: crate::Storage) -> Result<Self> {
        let registry = SyncRegistry::load(&storage)?;
        
        Ok(Self {
            storage,
            registry,
        })
    }
    
    /// Create a bidirectional sync for a prompt
    pub fn sync_prompt(&mut self, prompt_name: &str, local_path: Option<PathBuf>) -> Result<PathBuf> {
        // Check if prompt exists
        if !self.storage.prompt_exists(prompt_name) {
            return Err(anyhow::anyhow!("Prompt '{}' does not exist", prompt_name));
        }
        
        // Resolve local path using smart defaults
        let resolved_path = match local_path {
            Some(path) => {
                if path.is_absolute() {
                    path
                } else {
                    std::env::current_dir()?.join(path)
                }
            }
            None => {
                // Smart default: ./prompt-name.md (with team prefix stripping)
                let file_name = if prompt_name.starts_with('@') {
                    // Strip team prefix: @team/api-design -> api-design.md
                    prompt_name.split('/').last().unwrap_or(prompt_name)
                } else {
                    prompt_name
                };
                
                // Convert to kebab-case for filename
                let kebab_name = file_name.replace('_', "-").to_lowercase();
                std::env::current_dir()?.join(format!("{}.md", kebab_name))
            }
        };
        
        // Check for conflicts
        if resolved_path.exists() {
            return Err(anyhow::anyhow!(
                "File already exists at {:?}. Use --force to overwrite or choose a different path.",
                resolved_path
            ));
        }
        
        // Check if already synced
        if self.registry.is_synced(prompt_name) {
            return Err(anyhow::anyhow!(
                "Prompt '{}' is already synced. Use 'ph unsync {}' first.",
                prompt_name, prompt_name
            ));
        }
        
        // Read prompt content
        let (metadata, content) = self.storage.read_prompt(prompt_name)?;
        
        // Create file content with frontmatter
        let file_content = format!(
            "---\nprompt: {}\ndescription: {}\ntags: {}\n---\n\n{}",
            prompt_name,
            metadata.description,
            metadata.tags.unwrap_or_default().join(", "),
            content
        );
        
        // Create directory if needed
        if let Some(parent) = resolved_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {:?}", parent))?;
        }
        
        // Write file
        fs::write(&resolved_path, &file_content)
            .with_context(|| format!("Failed to write file to {:?}", resolved_path))?;
        
        // Calculate content hash
        let content_hash = calculate_content_hash(&file_content);
        
        // Add to sync registry
        self.registry.add_sync(prompt_name.to_string(), resolved_path.clone(), content_hash);
        
        // Save registry
        self.registry.save(&self.storage)?;
        
        Ok(resolved_path)
    }
    
    /// Remove sync relationship
    pub fn unsync_prompt(&mut self, prompt_name: &str) -> Result<Option<PathBuf>> {
        if let Some(sync_meta) = self.registry.remove_sync(prompt_name) {
            self.registry.save(&self.storage)?;
            Ok(Some(sync_meta.local_path))
        } else {
            Ok(None)
        }
    }
    
    /// Sync changes from file to PromptHive
    pub fn sync_from_file(&mut self, prompt_name: &str) -> Result<bool> {
        let sync_meta = self.registry.get_sync(prompt_name)
            .ok_or_else(|| anyhow::anyhow!("Prompt '{}' is not synced", prompt_name))?
            .clone();
        
        if !sync_meta.local_path.exists() {
            return Err(anyhow::anyhow!(
                "Synced file does not exist: {:?}",
                sync_meta.local_path
            ));
        }
        
        // Read file content
        let file_content = fs::read_to_string(&sync_meta.local_path)
            .with_context(|| format!("Failed to read file {:?}", sync_meta.local_path))?;
        
        // Calculate current hash
        let current_hash = calculate_content_hash(&file_content);
        
        // Check if changed
        if current_hash == sync_meta.content_hash {
            return Ok(false); // No changes
        }
        
        // Parse frontmatter and content
        let (new_metadata, new_content) = parse_file_content(&file_content)?;
        
        // Update PromptHive
        self.storage.write_prompt(prompt_name, &new_metadata, &new_content)?;
        
        // Update sync registry
        self.registry.update_sync(prompt_name, current_hash);
        self.registry.save(&self.storage)?;
        
        Ok(true)
    }
    
    /// Sync changes from PromptHive to file
    pub fn sync_to_file(&mut self, prompt_name: &str) -> Result<bool> {
        let sync_meta = self.registry.get_sync(prompt_name)
            .ok_or_else(|| anyhow::anyhow!("Prompt '{}' is not synced", prompt_name))?
            .clone();
        
        // Read current PromptHive content
        let (metadata, content) = self.storage.read_prompt(prompt_name)?;
        
        // Create file content
        let file_content = format!(
            "---\nprompt: {}\ndescription: {}\ntags: {}\n---\n\n{}",
            prompt_name,
            metadata.description,
            metadata.tags.unwrap_or_default().join(", "),
            content
        );
        
        // Calculate hash
        let new_hash = calculate_content_hash(&file_content);
        
        // Check if changed
        if new_hash == sync_meta.content_hash {
            return Ok(false); // No changes
        }
        
        // Write to file
        fs::write(&sync_meta.local_path, &file_content)
            .with_context(|| format!("Failed to write to file {:?}", sync_meta.local_path))?;
        
        // Update sync registry
        self.registry.update_sync(prompt_name, new_hash);
        self.registry.save(&self.storage)?;
        
        Ok(true)
    }
    
    /// Get sync status for all or specific prompts
    pub fn get_sync_status(&self, prompt_name: Option<&str>) -> Result<Vec<SyncStatus>> {
        let prompts = if let Some(name) = prompt_name {
            vec![name]
        } else {
            self.registry.get_all_synced()
        };
        
        let mut statuses = Vec::new();
        
        for prompt in prompts {
            let sync_meta = self.registry.get_sync(prompt).unwrap();
            
            let status = if !sync_meta.local_path.exists() {
                SyncStatusType::BrokenSync
            } else {
                // Check if file has changed
                let file_content = fs::read_to_string(&sync_meta.local_path)
                    .unwrap_or_default();
                let current_hash = calculate_content_hash(&file_content);
                
                if current_hash != sync_meta.content_hash {
                    SyncStatusType::OutOfSync
                } else {
                    SyncStatusType::InSync
                }
            };
            
            statuses.push(SyncStatus {
                prompt_name: prompt.to_string(),
                local_path: sync_meta.local_path.clone(),
                status,
                last_sync: sync_meta.last_sync,
            });
        }
        
        Ok(statuses)
    }
    
    /// Get the sync registry
    pub fn registry(&self) -> &SyncRegistry {
        &self.registry
    }
}

/// Status of a sync relationship
#[derive(Debug, Clone)]
pub struct SyncStatus {
    pub prompt_name: String,
    pub local_path: PathBuf,
    pub status: SyncStatusType,
    pub last_sync: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SyncStatusType {
    InSync,
    OutOfSync,
    BrokenSync,
}

/// Calculate SHA-256 hash of content for change detection
fn calculate_content_hash(content: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Parse file content with frontmatter
fn parse_file_content(content: &str) -> Result<(crate::PromptMetadata, String)> {
    let lines: Vec<&str> = content.lines().collect();
    
    if lines.len() < 3 || lines[0] != "---" {
        // No frontmatter, treat entire content as prompt
        let metadata = crate::PromptMetadata {
            id: "unknown".to_string(),
            description: "Imported from file".to_string(),
            tags: None,
            created_at: Some(chrono::Utc::now().to_rfc3339()),
            updated_at: None,
            version: None,
            git_hash: None,
            parent_version: None,
        };
        return Ok((metadata, content.to_string()));
    }
    
    // Find end of frontmatter
    let mut end_index = 1;
    while end_index < lines.len() && lines[end_index] != "---" {
        end_index += 1;
    }
    
    if end_index >= lines.len() {
        return Err(anyhow::anyhow!("Invalid frontmatter: missing closing ---"));
    }
    
    // Parse frontmatter
    let mut prompt_name = "unknown".to_string();
    let mut description = "Imported from file".to_string();
    let mut tags: Vec<String> = Vec::new();
    
    for line in &lines[1..end_index] {
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim();
            let value = value.trim();
            
            match key {
                "prompt" => prompt_name = value.to_string(),
                "description" => description = value.to_string(),
                "tags" => {
                    tags = value.split(',').map(|s| s.trim().to_string()).collect();
                }
                _ => {} // Ignore unknown fields
            }
        }
    }
    
    let metadata = crate::PromptMetadata {
        id: prompt_name,
        description,
        tags: if tags.is_empty() { None } else { Some(tags) },
        created_at: Some(chrono::Utc::now().to_rfc3339()),
        updated_at: None,
        version: None,
        git_hash: None,
        parent_version: None,
    };
    
    // Extract content (everything after frontmatter)
    let content_lines = &lines[end_index + 1..];
    let content = content_lines.join("\n").trim().to_string();
    
    Ok((metadata, content))
}