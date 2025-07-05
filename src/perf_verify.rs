// Performance Verification System
// Validates that PromptHive meets its <50ms performance claims

use crate::Storage;
use anyhow::Result;
use colored::*;
use std::process::Command;
use std::time::Instant;

pub struct PerformanceVerifier {
    storage: Storage,
}

impl PerformanceVerifier {
    pub fn new(storage: Storage) -> Self {
        Self { storage }
    }

    /// Run comprehensive performance verification against documented claims
    pub fn verify_performance_claims(&self) -> Result<PerformanceReport> {
        println!("{}", "🔬 Performance Verification".bright_blue().bold());
        println!("Validating <50ms claims from CURRENT.md\n");

        let mut report = PerformanceReport::new();

        // Test core commands that claim <50ms performance
        let test_cases = vec![
            ("ph use", vec!["use", "essentials/commit"]),
            ("ph new", vec!["new", "perf-test-prompt", "test content"]),
            ("ph ls", vec!["ls"]),
            ("ph find", vec!["find", "test"]),
            ("ph show", vec!["show", "essentials/commit"]),
            ("ph clean", vec!["clean", "test text"]),
            (
                "ph diff",
                vec!["diff", "essentials/commit", "essentials/debug"],
            ),
        ];

        for (name, args) in test_cases {
            let elapsed = self.benchmark_command(&args)?;
            let passed = elapsed < 50;

            report.add_test(name, elapsed, passed);

            let status = if passed { "✓".green() } else { "✗".red() };
            let timing = if passed {
                format!("{}ms", elapsed).green()
            } else {
                format!("{}ms", elapsed).red()
            };

            println!("  {} {} - {}", status, name, timing);
        }

        // Clean up test prompt
        let _ = self.benchmark_command(&["delete", "perf-test-prompt"]);

        report.finalize();
        self.print_summary(&report);

        Ok(report)
    }

    fn benchmark_command(&self, args: &[&str]) -> Result<u64> {
        // Use the actual binary instead of cargo run to avoid compilation overhead
        let binary_path = std::env::current_exe()?;

        let start = Instant::now();

        let _output = Command::new(&binary_path)
            .args(args)
            .env("PROMPTHIVE_BASE_DIR", self.storage.base_dir())
            .output()?;

        let elapsed = start.elapsed().as_millis() as u64;

        Ok(elapsed)
    }

    fn print_summary(&self, report: &PerformanceReport) {
        println!("\n{}", "📊 Performance Summary".bright_blue().bold());

        let total_tests = report.tests.len();
        let passed_tests = report.tests.iter().filter(|t| t.passed).count();
        let failed_tests = total_tests - passed_tests;

        println!("  Total tests: {}", total_tests);
        println!("  Passed: {}", format!("{}", passed_tests).green());

        if failed_tests > 0 {
            println!("  Failed: {}", format!("{}", failed_tests).red());
        }

        println!("  Average time: {}ms", report.average_time);

        let performance_grade = if report.all_passed() {
            "🏆 EXCELLENT - All commands meet <50ms target".green()
        } else if passed_tests as f64 / total_tests as f64 > 0.8 {
            "⚠️  GOOD - Most commands meet target".yellow()
        } else {
            "❌ NEEDS IMPROVEMENT - Performance claims not met".red()
        };

        println!("\n{}", performance_grade);

        if !report.all_passed() {
            println!("\n{}", "🔧 Recommendations:".bright_yellow());
            for test in &report.tests {
                if !test.passed {
                    println!(
                        "  • Optimize {} (currently {}ms)",
                        test.command, test.duration
                    );
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct PerformanceReport {
    pub tests: Vec<PerformanceTest>,
    pub average_time: f64,
    pub total_time: u64,
}

#[derive(Debug)]
pub struct PerformanceTest {
    pub command: String,
    pub duration: u64,
    pub passed: bool,
}

impl PerformanceReport {
    fn new() -> Self {
        Self {
            tests: Vec::new(),
            average_time: 0.0,
            total_time: 0,
        }
    }

    fn add_test(&mut self, command: &str, duration: u64, passed: bool) {
        self.tests.push(PerformanceTest {
            command: command.to_string(),
            duration,
            passed,
        });
        self.total_time += duration;
    }

    fn finalize(&mut self) {
        if !self.tests.is_empty() {
            self.average_time = self.total_time as f64 / self.tests.len() as f64;
        }
    }

    pub fn all_passed(&self) -> bool {
        self.tests.iter().all(|t| t.passed)
    }

    pub fn success_rate(&self) -> f64 {
        if self.tests.is_empty() {
            return 0.0;
        }
        let passed = self.tests.iter().filter(|t| t.passed).count();
        passed as f64 / self.tests.len() as f64 * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_performance_report() {
        let mut report = PerformanceReport::new();
        report.add_test("ph use", 30, true);
        report.add_test("ph new", 45, true);
        report.add_test("ph slow", 80, false);
        report.finalize();

        assert_eq!(report.tests.len(), 3);
        assert_eq!(report.average_time, 51.666666666666664);
        assert!(!report.all_passed());
        assert_eq!(report.success_rate(), 66.66666666666666);
    }
}
