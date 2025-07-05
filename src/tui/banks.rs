use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;

use crate::Storage;

/// Represents a prompt bank with its metadata and prompts
#[derive(Debug, Clone)]
pub struct Bank {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub author: String,
    pub version: String,
    pub tags: Vec<String>,
    pub prompts: Vec<Prompt>,
    pub bank_type: BankType,
    pub is_expanded: bool,
}

/// Types of banks supported
#[derive(Debug, Clone, PartialEq)]
pub enum BankType {
    Local,     // Local prompts in prompts/ directory
    LocalBank, // Local bank in banks/ directory
    Registry,  // Remote bank from registry
    GitHub,    // GitHub repository bank
}

/// Represents an individual prompt
#[derive(Debug, Clone)]
pub struct Prompt {
    pub name: String,
    pub description: String,
    pub content: String,
    pub bank_name: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub tags: Vec<String>,
    pub is_favorite: bool,
    pub usage_count: u32,
}

/// Bank metadata from bank.yaml file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BankMetadata {
    pub name: String,
    pub description: String,
    pub author: String,
    pub version: String,
    pub tags: Vec<String>,
    pub prompts: Vec<PromptMetadata>,
}

/// Prompt metadata within bank.yaml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptMetadata {
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
}

/// Tree item for hierarchical display
#[derive(Debug, Clone)]
pub enum TreeItem {
    Bank { bank: Bank, depth: usize },
    Prompt { prompt: Prompt, depth: usize },
}

impl TreeItem {
    pub fn depth(&self) -> usize {
        match self {
            TreeItem::Bank { depth, .. } => *depth,
            TreeItem::Prompt { depth, .. } => *depth,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            TreeItem::Bank { bank, .. } => &bank.name,
            TreeItem::Prompt { prompt, .. } => &prompt.name,
        }
    }

    pub fn description(&self) -> &str {
        match self {
            TreeItem::Bank { bank, .. } => &bank.description,
            TreeItem::Prompt { prompt, .. } => &prompt.description,
        }
    }

    pub fn is_bank(&self) -> bool {
        matches!(self, TreeItem::Bank { .. })
    }

    pub fn is_prompt(&self) -> bool {
        matches!(self, TreeItem::Prompt { .. })
    }
}

impl Bank {
    /// Load all banks from storage
    pub fn load_all_banks(storage: &Storage) -> Result<(Vec<Bank>, Vec<Prompt>)> {
        let mut banks = Vec::new();

        // Load local prompts (not in banks)
        let local_prompts = Self::load_local_prompts(storage)?;

        // Load bank directories
        let banks_dir = storage.base_dir().join("banks");
        if banks_dir.exists() {
            if let Ok(entries) = fs::read_dir(&banks_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        if let Some(bank_name) = path.file_name().and_then(|n| n.to_str()) {
                            if let Ok(bank) = Self::load_bank(storage, bank_name) {
                                banks.push(bank);
                            }
                        }
                    }
                }
            }
        }

        // Sort banks alphabetically
        banks.sort_by(|a, b| a.name.cmp(&b.name));

        Ok((banks, local_prompts))
    }

    /// Load individual bank from directory
    pub fn load_bank(storage: &Storage, bank_name: &str) -> Result<Bank> {
        let bank_dir = storage.base_dir().join("banks").join(bank_name);

        // Try to load bank.yaml metadata
        let bank_yaml_path = bank_dir.join("bank.yaml");
        let metadata = if bank_yaml_path.exists() {
            let yaml_content = fs::read_to_string(&bank_yaml_path)?;
            serde_yaml::from_str::<BankMetadata>(&yaml_content).unwrap_or_else(|_| {
                // Fallback metadata if parsing fails
                BankMetadata {
                    name: bank_name.to_string(),
                    description: format!("Bank: {}", bank_name),
                    author: "Unknown".to_string(),
                    version: "1.0.0".to_string(),
                    tags: vec![],
                    prompts: vec![],
                }
            })
        } else {
            // Generate default metadata
            BankMetadata {
                name: bank_name.to_string(),
                description: format!("Bank: {}", bank_name),
                author: "Local".to_string(),
                version: "1.0.0".to_string(),
                tags: vec!["local".to_string()],
                prompts: vec![],
            }
        };

        // Load prompts from bank directory
        let prompts = Self::load_bank_prompts(storage, bank_name)?;

        // Determine bank type
        let bank_type = if bank_name.starts_with('@') {
            BankType::Registry
        } else if bank_name.contains('/') {
            BankType::GitHub
        } else {
            BankType::LocalBank
        };

        Ok(Bank {
            name: bank_name.to_string(),
            display_name: metadata.name,
            description: metadata.description,
            author: metadata.author,
            version: metadata.version,
            tags: metadata.tags,
            prompts,
            bank_type,
            is_expanded: false,
        })
    }

    /// Load prompts from a specific bank directory (with subdirectory flattening)
    fn load_bank_prompts(storage: &Storage, bank_name: &str) -> Result<Vec<Prompt>> {
        let mut prompts = Vec::new();
        let bank_dir = storage.base_dir().join("banks").join(bank_name);

        // Helper function to recursively load prompts
        fn load_prompts_recursive(
            storage: &Storage,
            bank_name: &str,
            dir_path: &std::path::Path,
            relative_path: &str,
            prompts: &mut Vec<Prompt>,
        ) -> Result<()> {
            if let Ok(entries) = fs::read_dir(dir_path) {
                for entry in entries.flatten() {
                    let path = entry.path();

                    if path.is_dir() {
                        // Recursively process subdirectory
                        if let Some(subdir_name) = path.file_name().and_then(|s| s.to_str()) {
                            let new_relative_path = if relative_path.is_empty() {
                                subdir_name.to_string()
                            } else {
                                format!("{}/{}", relative_path, subdir_name)
                            };
                            let _ = load_prompts_recursive(
                                storage,
                                bank_name,
                                &path,
                                &new_relative_path,
                                prompts,
                            );
                        }
                    } else if path.extension().and_then(|s| s.to_str()) == Some("md") {
                        if let Some(prompt_name) = path.file_stem().and_then(|s| s.to_str()) {
                            // Skip README files
                            if prompt_name.eq_ignore_ascii_case("readme") {
                                continue;
                            }

                            // Create flattened name for display
                            let display_name = if relative_path.is_empty() {
                                prompt_name.to_string()
                            } else {
                                format!("{}/{}", relative_path, prompt_name)
                            };

                            // Full storage path - this should match what storage expects
                            let full_prompt_name = if relative_path.is_empty() {
                                format!("{}/{}", bank_name, prompt_name)
                            } else {
                                format!("{}/{}", bank_name, display_name)
                            };

                            if let Ok((metadata, content)) = storage.read_prompt(&full_prompt_name)
                            {
                                prompts.push(Prompt {
                                    name: display_name,
                                    description: metadata.description,
                                    content,
                                    bank_name: Some(bank_name.to_string()),
                                    created_at: metadata.created_at,
                                    updated_at: metadata.updated_at,
                                    tags: metadata.tags.unwrap_or_default(),
                                    is_favorite: false, // TODO: Load from user preferences
                                    usage_count: 0,     // TODO: Load from telemetry
                                });
                            }
                        }
                    }
                }
            }
            Ok(())
        }

        // Start recursive loading from bank root
        load_prompts_recursive(storage, bank_name, &bank_dir, "", &mut prompts)?;

        // Sort prompts alphabetically
        prompts.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(prompts)
    }

    /// Load local prompts (not in banks)
    fn load_local_prompts(storage: &Storage) -> Result<Vec<Prompt>> {
        let mut prompts = Vec::new();
        let prompts_dir = storage.prompts_dir();

        if prompts_dir.exists() {
            if let Ok(entries) = fs::read_dir(&prompts_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("md") {
                        if let Some(prompt_name) = path.file_stem().and_then(|s| s.to_str()) {
                            if let Ok((metadata, content)) = storage.read_prompt(prompt_name) {
                                prompts.push(Prompt {
                                    name: prompt_name.to_string(),
                                    description: metadata.description,
                                    content,
                                    bank_name: None,
                                    created_at: metadata.created_at,
                                    updated_at: metadata.updated_at,
                                    tags: metadata.tags.unwrap_or_default(),
                                    is_favorite: false,
                                    usage_count: 0,
                                });
                            }
                        }
                    }
                }
            }
        }

        // Sort prompts alphabetically
        prompts.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(prompts)
    }

    /// Get the icon for this bank type
    pub fn icon(&self) -> &'static str {
        match self.bank_type {
            BankType::Local => "📁",
            BankType::LocalBank => "📁",
            BankType::Registry => "🌐",
            BankType::GitHub => "🐙",
        }
    }

    /// Get full display name with count
    pub fn display_name_with_count(&self) -> String {
        format!("{} ({})", self.display_name, self.prompts.len())
    }

    /// Toggle expanded state
    pub fn toggle_expanded(&mut self) {
        self.is_expanded = !self.is_expanded;
    }
}

impl Prompt {
    /// Get the full prompt name (with bank prefix if applicable)
    pub fn full_name(&self) -> String {
        if let Some(bank) = &self.bank_name {
            format!("{}/{}", bank, self.name)
        } else {
            self.name.clone()
        }
    }

    /// Get display name for the UI
    pub fn display_name(&self) -> String {
        self.name.clone()
    }
}
