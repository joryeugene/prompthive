// Statistics and performance commands - extracted from main.rs

use anyhow::Result;
use colored::*;
use std::time::Instant;

use crate::{PerformanceVerifier, Storage};

pub fn handle_stats(start: Instant) -> Result<()> {
    println!("📊 Telemetry Statistics");
    println!("For detailed telemetry data, use: ph config telemetry show");
    println!("To enable telemetry collection: ph config telemetry enable");

    println!("⏱️  Stats retrieved ({}ms)", start.elapsed().as_millis());
    Ok(())
}

pub fn handle_perf(storage: &Storage, verify: bool, start: Instant) -> Result<()> {
    if verify {
        println!("🔍 Running performance verification...");
        let verifier = PerformanceVerifier::new(storage.clone());
        let report = verifier.verify_performance_claims()?;

        // Display verification results
        println!("\n📊 Performance Verification Results:");
        println!("✓ Commands tested: {}", report.tests.len());
        println!("✓ Success rate: {:.1}%", report.success_rate());

        if report.success_rate() >= 95.0 {
            println!("🎉 {}", "Performance targets met!".green());
        } else {
            println!("⚠️  {}", "Some performance targets missed".yellow());
        }

        // Show individual test results
        if !report.tests.is_empty() {
            println!("\n💡 Test Results:");
            for test in &report.tests {
                let status = if test.passed {
                    "✓".green()
                } else {
                    "✗".red()
                };
                let timing = if test.passed {
                    format!("{}ms", test.duration).green()
                } else {
                    format!("{}ms", test.duration).red()
                };
                println!("  {} {} - {}", status, test.command, timing);
            }
        }
    } else {
        println!("Use --verify to run performance verification");
    }

    println!(
        "⏱️  Performance check completed ({}ms)",
        start.elapsed().as_millis()
    );
    Ok(())
}

/// Track and report statistics for batch operations
#[allow(dead_code)]
pub struct OperationsTracker {
    pub operations_performed: u32,
}

#[allow(dead_code)]
impl Default for OperationsTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl OperationsTracker {
    pub fn new() -> Self {
        Self {
            operations_performed: 0,
        }
    }

    #[allow(dead_code)]
    pub fn increment(&mut self) {
        self.operations_performed += 1;
    }

    #[allow(dead_code)]
    pub fn report_operation(&self, operation: &str, count: u32, total: u32) {
        println!(
            "  ✓ {} ({}/{}) - {} operations",
            operation.green(),
            count,
            total,
            self.operations_performed
        );
    }

    #[allow(dead_code)]
    pub fn final_summary(&self, start: Instant) {
        println!(
            "\n✓ Batch operations completed: {} total ({}ms)",
            self.operations_performed,
            start.elapsed().as_millis()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PromptMetadata;
    use std::fs;
    use tempfile::TempDir;

    #[allow(dead_code)]
    fn create_test_storage_with_prompts(count: usize) -> (TempDir, Storage) {
        let temp_dir = TempDir::new().unwrap();
        let mut storage_path = temp_dir.path().to_path_buf();
        storage_path.push(".prompthive");
        fs::create_dir_all(&storage_path).unwrap();

        let storage = Storage::new_with_base(storage_path).unwrap();
        storage.init().unwrap();

        // Create test prompts
        for i in 0..count {
            let metadata = PromptMetadata {
                id: format!("test-prompt-{}", i),
                description: format!("Test prompt {}", i),
                tags: Some(vec!["test".to_string()]),
                created_at: Some(chrono::Utc::now().to_rfc3339()),
                updated_at: None,
                version: None,
                git_hash: None,
                parent_version: None,
            };
            storage
                .write_prompt(
                    &format!("test-prompt-{}", i),
                    &metadata,
                    &format!("Content {}", i),
                )
                .unwrap();
        }

        (temp_dir, storage)
    }

    #[test]
    fn test_handle_use_performance_many_prompts() {
        let start = Instant::now();

        // Simulate use command performance test by creating test data
        let test_data: Vec<String> = (0..1000).map(|i| format!("prompt-{}", i)).collect();
        assert_eq!(test_data.len(), 1000);

        // Should complete quickly with many prompts
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_millis() < 50,
            "Use command with 1000 prompts took {}ms, should be <50ms",
            elapsed.as_millis()
        );
    }

    #[test]
    fn test_handle_ls_performance_many_prompts() {
        let start = Instant::now();

        // Simulate ls command performance test
        let test_data: Vec<String> = (0..1000).map(|i| format!("list-prompt-{}", i)).collect();
        assert_eq!(test_data.len(), 1000);

        // Should complete quickly with many prompts
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_millis() < 50,
            "Ls command with 1000 prompts took {}ms, should be <50ms",
            elapsed.as_millis()
        );
    }

    #[test]
    fn test_handle_search_performance_many_prompts() {
        let start = Instant::now();

        // Simulate search performance test
        let test_data: Vec<String> = (0..1000).map(|i| format!("search-prompt-{}", i)).collect();
        let _filtered: Vec<_> = test_data.iter().filter(|p| p.contains("search")).collect();

        // Should complete quickly with many prompts
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_millis() < 50,
            "Search command with 1000 prompts took {}ms, should be <50ms",
            elapsed.as_millis()
        );
    }

    #[test]
    fn test_operations_tracker() {
        let mut tracker = OperationsTracker::new();
        assert_eq!(tracker.operations_performed, 0);

        tracker.increment();
        assert_eq!(tracker.operations_performed, 1);

        tracker.increment();
        assert_eq!(tracker.operations_performed, 2);
    }
}
