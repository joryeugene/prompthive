//! Sync types and functionality for command modules
//! 
//! This module provides a simple interface to sync functionality
//! to avoid complex module import issues.

use anyhow::Result;
use std::path::PathBuf;
use crate::Storage;

/// Simple sync manager interface for commands
pub struct SimpleSyncManager {
    storage: Storage,
}

impl SimpleSyncManager {
    /// Create a new sync manager
    pub fn new(storage: Storage) -> Result<Self> {
        Ok(Self { storage })
    }
    
    /// Create a bidirectional sync for a prompt
    pub fn sync_prompt(&self, prompt_name: &str, local_path: Option<PathBuf>) -> Result<PathBuf> {
        // Smart default path resolution
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
            std::fs::create_dir_all(parent)?;
        }
        
        // Write file
        std::fs::write(&resolved_path, &file_content)?;
        
        Ok(resolved_path)
    }
}