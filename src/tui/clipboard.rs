use crate::storage::PromptMetadata;
use std::time::{SystemTime, UNIX_EPOCH};

/// Internal clipboard for TUI operations
#[derive(Debug, Clone)]
pub struct ClipboardItem {
    pub items: Vec<ClipboardEntry>,
    pub operation: ClipboardOperation,
    pub timestamp: u64,
}

/// Individual item in clipboard
#[derive(Debug, Clone)]
pub struct ClipboardEntry {
    pub name: String,
    pub content: String,
    pub metadata: PromptMetadata,
    pub source_bank: Option<String>,
    pub item_type: ClipboardItemType,
}

/// Type of clipboard operation
#[derive(Debug, Clone, PartialEq)]
pub enum ClipboardOperation {
    Cut,  // Move operation (source will be deleted)
    Copy, // Copy operation (source remains)
}

/// Type of item in clipboard
#[derive(Debug, Clone, PartialEq)]
pub enum ClipboardItemType {
    Prompt,
    Bank,
}

impl ClipboardItem {
    /// Create new clipboard item for cut operation
    pub fn new_cut(items: Vec<ClipboardEntry>) -> Self {
        Self {
            items,
            operation: ClipboardOperation::Cut,
            timestamp: current_timestamp(),
        }
    }

    /// Create new clipboard item for copy operation
    pub fn new_copy(items: Vec<ClipboardEntry>) -> Self {
        Self {
            items,
            operation: ClipboardOperation::Copy,
            timestamp: current_timestamp(),
        }
    }

    /// Check if this is a cut operation
    pub fn is_cut(&self) -> bool {
        self.operation == ClipboardOperation::Cut
    }

    /// Check if this is a copy operation
    pub fn is_copy(&self) -> bool {
        self.operation == ClipboardOperation::Copy
    }

    /// Get number of items in clipboard
    pub fn count(&self) -> usize {
        self.items.len()
    }

    /// Check if clipboard contains only prompts
    pub fn is_prompts_only(&self) -> bool {
        self.items
            .iter()
            .all(|item| item.item_type == ClipboardItemType::Prompt)
    }

    /// Check if clipboard contains only banks
    pub fn is_banks_only(&self) -> bool {
        self.items
            .iter()
            .all(|item| item.item_type == ClipboardItemType::Bank)
    }

    /// Check if clipboard has mixed content types
    pub fn is_mixed_content(&self) -> bool {
        !self.is_prompts_only() && !self.is_banks_only()
    }

    /// Get display string for clipboard content
    pub fn display(&self) -> String {
        let op_symbol = match self.operation {
            ClipboardOperation::Cut => "✂",
            ClipboardOperation::Copy => "📋",
        };

        if self.items.len() == 1 {
            format!("{} {}", op_symbol, self.items[0].name)
        } else {
            format!("{} {} items", op_symbol, self.items.len())
        }
    }

    /// Get detailed display for status
    pub fn detailed_display(&self) -> String {
        let operation = match self.operation {
            ClipboardOperation::Cut => "cut",
            ClipboardOperation::Copy => "copied",
        };

        if self.items.len() == 1 {
            let item = &self.items[0];
            let type_str = match item.item_type {
                ClipboardItemType::Prompt => "prompt",
                ClipboardItemType::Bank => "bank",
            };
            format!("{} {} '{}'", operation, type_str, item.name)
        } else {
            let prompt_count = self
                .items
                .iter()
                .filter(|i| i.item_type == ClipboardItemType::Prompt)
                .count();
            let bank_count = self
                .items
                .iter()
                .filter(|i| i.item_type == ClipboardItemType::Bank)
                .count();

            match (prompt_count, bank_count) {
                (p, 0) => format!("{} {} prompts", operation, p),
                (0, b) => format!("{} {} banks", operation, b),
                (p, b) => format!("{} {} prompts and {} banks", operation, p, b),
            }
        }
    }

    /// Check if clipboard has expired (older than 1 hour)
    pub fn is_expired(&self) -> bool {
        let current = current_timestamp();
        current.saturating_sub(self.timestamp) > 3600 // 1 hour
    }

    /// Get age of clipboard content in seconds
    pub fn age_seconds(&self) -> u64 {
        let current = current_timestamp();
        current.saturating_sub(self.timestamp)
    }

    /// Get prompts from clipboard
    pub fn prompts(&self) -> Vec<&ClipboardEntry> {
        self.items
            .iter()
            .filter(|item| item.item_type == ClipboardItemType::Prompt)
            .collect()
    }

    /// Get banks from clipboard
    pub fn banks(&self) -> Vec<&ClipboardEntry> {
        self.items
            .iter()
            .filter(|item| item.item_type == ClipboardItemType::Bank)
            .collect()
    }

    /// Check if clipboard can be pasted into the given bank
    pub fn can_paste_into(&self, target_bank: Option<&str>) -> (bool, Option<String>) {
        // Check for naming conflicts
        for item in &self.items {
            if item.source_bank.as_deref() == target_bank {
                // Can't paste into same location for cut operations
                if self.is_cut() {
                    return (
                        false,
                        Some("Cannot move items to the same location".to_string()),
                    );
                }
                // For copy operations, would create duplicate names
                return (
                    false,
                    Some(format!(
                        "Item '{}' already exists in target location",
                        item.name
                    )),
                );
            }
        }

        // All good
        (true, None)
    }
}

impl ClipboardEntry {
    /// Create new prompt clipboard entry
    pub fn new_prompt(
        name: String,
        content: String,
        metadata: PromptMetadata,
        source_bank: Option<String>,
    ) -> Self {
        Self {
            name,
            content,
            metadata,
            source_bank,
            item_type: ClipboardItemType::Prompt,
        }
    }

    /// Create new bank clipboard entry
    pub fn new_bank(name: String, source_bank: Option<String>) -> Self {
        let metadata = PromptMetadata {
            id: name.clone(),
            description: format!("Bank: {}", name),
            tags: None,
            created_at: None,
            updated_at: None,
            version: None,
            git_hash: None,
            parent_version: None,
        };

        Self {
            name,
            content: String::new(), // Banks don't have content
            metadata,
            source_bank,
            item_type: ClipboardItemType::Bank,
        }
    }

    /// Get full name including bank prefix
    pub fn full_name(&self) -> String {
        if let Some(bank) = &self.source_bank {
            format!("{}/{}", bank, self.name)
        } else {
            self.name.clone()
        }
    }

    /// Get display name for UI
    pub fn display_name(&self) -> String {
        match self.item_type {
            ClipboardItemType::Prompt => {
                if let Some(bank) = &self.source_bank {
                    format!("{}/{}", bank, self.name)
                } else {
                    self.name.clone()
                }
            }
            ClipboardItemType::Bank => self.name.clone(),
        }
    }
}

/// Get current timestamp in seconds
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
