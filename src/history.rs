//! Command history tracking and management
//!
//! Provides functionality to track command usage history, enabling features
//! like usage analytics, frequently used prompts, and command replay.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HistoryEntry {
    pub timestamp: DateTime<Utc>,
    pub command: String,
    pub input_preview: String,
    pub output_preview: String,
    pub success: bool,
    pub duration_ms: u128,
}

pub struct HistoryTracker {
    history_file: PathBuf,
    max_entries: usize,
}

impl HistoryTracker {
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            history_file: base_dir.join("history.json"),
            max_entries: 1000,
        }
    }

    pub fn record(&self, entry: HistoryEntry) -> Result<()> {
        let mut history = self.load_history().unwrap_or_default();

        // Add new entry at the beginning
        history.insert(0, entry);

        // Keep only the most recent entries
        if history.len() > self.max_entries {
            history.truncate(self.max_entries);
        }

        self.save_history(&history)?;
        Ok(())
    }

    pub fn get_recent(&self, limit: usize) -> Result<Vec<HistoryEntry>> {
        let history = self.load_history().unwrap_or_default();
        Ok(history.into_iter().take(limit).collect())
    }

    pub fn get_last(&self) -> Result<Option<HistoryEntry>> {
        let history = self.load_history().unwrap_or_default();
        Ok(history.into_iter().next())
    }

    pub fn search(&self, query: &str) -> Result<Vec<HistoryEntry>> {
        let history = self.load_history().unwrap_or_default();
        Ok(history
            .into_iter()
            .filter(|entry| {
                entry.command.to_lowercase().contains(&query.to_lowercase())
                    || entry
                        .input_preview
                        .to_lowercase()
                        .contains(&query.to_lowercase())
                    || entry
                        .output_preview
                        .to_lowercase()
                        .contains(&query.to_lowercase())
            })
            .collect())
    }

    pub fn get_successful_only(&self, limit: usize) -> Result<Vec<HistoryEntry>> {
        let history = self.load_history().unwrap_or_default();
        Ok(history
            .into_iter()
            .filter(|entry| entry.success)
            .take(limit)
            .collect())
    }

    fn load_history(&self) -> Result<Vec<HistoryEntry>> {
        if !self.history_file.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&self.history_file)
            .with_context(|| format!("Failed to read history file: {:?}", self.history_file))?;

        let history: Vec<HistoryEntry> =
            serde_json::from_str(&content).with_context(|| "Failed to parse history file")?;

        Ok(history)
    }

    fn save_history(&self, history: &[HistoryEntry]) -> Result<()> {
        // Ensure directory exists
        if let Some(parent) = self.history_file.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {:?}", parent))?;
        }

        let content =
            serde_json::to_string_pretty(history).with_context(|| "Failed to serialize history")?;

        fs::write(&self.history_file, content)
            .with_context(|| format!("Failed to write history file: {:?}", self.history_file))?;

        Ok(())
    }
}

impl HistoryEntry {
    pub fn new(
        command: String,
        input: &str,
        output: &str,
        success: bool,
        duration_ms: u128,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            command,
            input_preview: Self::truncate_preview(input),
            output_preview: Self::truncate_preview(output),
            success,
            duration_ms,
        }
    }

    fn truncate_preview(text: &str) -> String {
        const MAX_PREVIEW: usize = 100;
        if text.len() <= MAX_PREVIEW {
            text.to_string()
        } else {
            format!("{}...", &text[..MAX_PREVIEW])
        }
    }

    pub fn display(&self) {
        use colored::*;

        let status = if self.success {
            "✓".green()
        } else {
            "✗".red()
        };
        let timestamp = self.timestamp.format("%Y-%m-%d %H:%M:%S");

        println!(
            "{} {} {} ({}ms)",
            status,
            timestamp.to_string().dimmed(),
            self.command.bold(),
            self.duration_ms
        );

        if !self.input_preview.is_empty() {
            println!("  Input:  {}", self.input_preview.dimmed());
        }

        if !self.output_preview.is_empty() {
            println!("  Output: {}", self.output_preview.dimmed());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_history_tracking() {
        let temp_dir = TempDir::new().unwrap();
        let tracker = HistoryTracker::new(temp_dir.path().to_path_buf());

        let entry = HistoryEntry::new(
            "use api-design".to_string(),
            "test input",
            "test output",
            true,
            100,
        );

        tracker.record(entry.clone()).unwrap();

        let history = tracker.get_recent(10).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].command, "use api-design");
    }

    #[test]
    fn test_history_search() {
        let temp_dir = TempDir::new().unwrap();
        let tracker = HistoryTracker::new(temp_dir.path().to_path_buf());

        let entry1 = HistoryEntry::new("use api-design".to_string(), "", "", true, 100);
        let entry2 = HistoryEntry::new("use code-review".to_string(), "", "", true, 150);

        tracker.record(entry1).unwrap();
        tracker.record(entry2).unwrap();

        let results = tracker.search("api").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].command, "use api-design");
    }
}
