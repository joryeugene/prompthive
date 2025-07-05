//! Security and audit capabilities for PromptHive
//!
//! This module provides comprehensive security features including:
//! - Content scanning for sensitive information
//! - Vulnerability detection in prompts
//! - Audit trail logging
//! - Permission validation
//! - Encryption/decryption utilities
//! - Integrity verification

use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tracing::{debug, info, warn};
use crate::log_security_event;

/// Security policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Enable content scanning for sensitive information
    pub scan_content: bool,
    /// Enable vulnerability detection
    pub vulnerability_scanning: bool,
    /// Enable audit logging
    pub audit_logging: bool,
    /// Maximum file size for scanning (bytes)
    pub max_scan_size: usize,
    /// Sensitive patterns to detect
    pub sensitive_patterns: Vec<String>,
    /// Blocked content patterns
    pub blocked_patterns: Vec<String>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            scan_content: true,
            vulnerability_scanning: true,
            audit_logging: true,
            max_scan_size: 10 * 1024 * 1024, // 10MB
            sensitive_patterns: vec![
                // API keys and tokens
                r"(?i)(api[_-]?key|token|secret)[_\s=:]*['\x22]?([a-z0-9_-]{16,})".to_string(),
                // AWS credentials
                r"(?i)(aws[_-]?(access[_-]?)?key[_-]?id)[_\s=:]*['\x22]?([a-z0-9]{20})".to_string(),
                r"(?i)(aws[_-]?secret[_-]?access[_-]?key)[_\s=:]*['\x22]?([a-z0-9/+=]{40})".to_string(),
                // Database credentials
                r"(?i)(password|pwd)[_\s=:]*['\x22]?([a-z0-9!@#$%^&*()-_+=]{8,})".to_string(),
                // JWT tokens
                r"eyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+".to_string(),
                // Email addresses (potentially PII)
                r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b".to_string(),
                // Credit card numbers
                r"\b(?:\d{4}[-\s]?){3}\d{4}\b".to_string(),
                // SSH private keys
                r"-----BEGIN [A-Z]+ PRIVATE KEY-----".to_string(),
            ],
            blocked_patterns: vec![
                // Malicious commands
                r"(?i)(rm\s+-rf|del\s+/s|format\s+c:)".to_string(),
                // SQL injection patterns
                r"(?i)(union\s+select|drop\s+table|delete\s+from)".to_string(),
                // XSS patterns
                r"(?i)(<script>|javascript:|on\w+\s*=)".to_string(),
            ],
        }
    }
}

/// Security scan result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityScanResult {
    /// Overall security status
    pub status: SecurityStatus,
    /// Detected security issues
    pub issues: Vec<SecurityIssue>,
    /// Scan duration in milliseconds
    pub scan_duration_ms: u64,
    /// Scanned file path
    pub file_path: String,
    /// File size in bytes
    pub file_size: usize,
}

/// Security status levels
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SecurityStatus {
    /// No security issues detected
    Clean,
    /// Minor issues that should be reviewed
    Warning,
    /// Serious issues requiring immediate attention
    Critical,
    /// Blocked content that should not be stored
    Blocked,
}

/// Individual security issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityIssue {
    /// Type of security issue
    pub issue_type: SecurityIssueType,
    /// Severity level
    pub severity: SecuritySeverity,
    /// Human-readable description
    pub description: String,
    /// Line number where issue was found
    pub line_number: Option<usize>,
    /// Character position in line
    pub position: Option<usize>,
    /// Matched content (redacted for sensitive data)
    pub matched_content: Option<String>,
    /// Suggested remediation
    pub remediation: Option<String>,
}

/// Types of security issues
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SecurityIssueType {
    /// Sensitive information detected
    SensitiveInformation,
    /// Potentially malicious content
    MaliciousContent,
    /// Vulnerable patterns
    Vulnerability,
    /// Privacy concern
    PrivacyConcern,
    /// Compliance violation
    ComplianceViolation,
}

/// Security severity levels
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SecuritySeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Main security scanner
pub struct SecurityScanner {
    config: SecurityConfig,
    sensitive_regexes: Vec<Regex>,
    blocked_regexes: Vec<Regex>,
}

impl SecurityScanner {
    /// Create a new security scanner with default configuration
    pub fn new() -> Result<Self> {
        let config = SecurityConfig::default();
        Self::new_with_config(config)
    }

    /// Create a new security scanner with custom configuration
    pub fn new_with_config(config: SecurityConfig) -> Result<Self> {
        let mut sensitive_regexes = Vec::new();
        for pattern in &config.sensitive_patterns {
            match Regex::new(pattern) {
                Ok(regex) => sensitive_regexes.push(regex),
                Err(e) => {
                    warn!("Invalid sensitive pattern regex '{}': {}", pattern, e);
                }
            }
        }

        let mut blocked_regexes = Vec::new();
        for pattern in &config.blocked_patterns {
            match Regex::new(pattern) {
                Ok(regex) => blocked_regexes.push(regex),
                Err(e) => {
                    warn!("Invalid blocked pattern regex '{}': {}", pattern, e);
                }
            }
        }

        Ok(Self {
            config,
            sensitive_regexes,
            blocked_regexes,
        })
    }

    /// Scan file content for security issues
    pub fn scan_file<P: AsRef<Path>>(&self, file_path: P) -> Result<SecurityScanResult> {
        let start_time = std::time::Instant::now();
        let file_path_str = file_path.as_ref().to_string_lossy().to_string();

        debug!("Starting security scan of file: {}", file_path_str);

        // Check file size
        let metadata = fs::metadata(&file_path)?;
        let file_size = metadata.len() as usize;
        
        if file_size > self.config.max_scan_size {
            warn!("File {} exceeds maximum scan size ({} bytes)", file_path_str, file_size);
            return Ok(SecurityScanResult {
                status: SecurityStatus::Warning,
                issues: vec![SecurityIssue {
                    issue_type: SecurityIssueType::ComplianceViolation,
                    severity: SecuritySeverity::Medium,
                    description: format!("File size ({} bytes) exceeds scan limit ({} bytes)", 
                                       file_size, self.config.max_scan_size),
                    line_number: None,
                    position: None,
                    matched_content: None,
                    remediation: Some("Consider reducing file size or adjusting scan limits".to_string()),
                }],
                scan_duration_ms: start_time.elapsed().as_millis() as u64,
                file_path: file_path_str,
                file_size,
            });
        }

        // Read file content
        let content = fs::read_to_string(&file_path)
            .context(format!("Failed to read file: {}", file_path_str))?;

        let result = self.scan_content(&content, &file_path_str);
        
        let scan_duration = start_time.elapsed().as_millis() as u64;
        
        // Log security scan result
        match &result.status {
            SecurityStatus::Critical | SecurityStatus::Blocked => {
                log_security_event("file_scan", "high", 
                    Some(&format!("Critical issues found in {}: {} issues", 
                                file_path_str, result.issues.len())));
            }
            SecurityStatus::Warning => {
                log_security_event("file_scan", "medium", 
                    Some(&format!("Warnings found in {}: {} issues", 
                                file_path_str, result.issues.len())));
            }
            SecurityStatus::Clean => {
                log_security_event("file_scan", "low", 
                    Some(&format!("Clean scan result for {}", file_path_str)));
            }
        }

        Ok(SecurityScanResult {
            status: result.status,
            issues: result.issues,
            scan_duration_ms: scan_duration,
            file_path: file_path_str,
            file_size,
        })
    }

    /// Scan text content for security issues
    pub fn scan_content(&self, content: &str, source: &str) -> SecurityScanResult {
        let mut issues = Vec::new();
        let start_time = std::time::Instant::now();

        // Check for blocked patterns first
        for (line_no, line) in content.lines().enumerate() {
            for regex in &self.blocked_regexes {
                if let Some(captures) = regex.captures(line) {
                    let matched = captures.get(0).map(|m| m.as_str()).unwrap_or("");
                    issues.push(SecurityIssue {
                        issue_type: SecurityIssueType::MaliciousContent,
                        severity: SecuritySeverity::Critical,
                        description: "Blocked malicious pattern detected".to_string(),
                        line_number: Some(line_no + 1),
                        position: captures.get(0).map(|m| m.start()),
                        matched_content: Some(matched.chars().take(50).collect()),
                        remediation: Some("Remove or modify the detected pattern".to_string()),
                    });
                }
            }
        }

        // If blocked content found, return immediately
        if !issues.is_empty() {
            return SecurityScanResult {
                status: SecurityStatus::Blocked,
                issues,
                scan_duration_ms: start_time.elapsed().as_millis() as u64,
                file_path: source.to_string(),
                file_size: content.len(),
            };
        }

        // Check for sensitive patterns
        for (line_no, line) in content.lines().enumerate() {
            for regex in &self.sensitive_regexes {
                if let Some(captures) = regex.captures(line) {
                    let matched = captures.get(0).map(|m| m.as_str()).unwrap_or("");
                    
                    // Determine issue type and severity based on pattern
                    let (issue_type, severity, description, remediation) = 
                        self.classify_sensitive_pattern(matched);

                    issues.push(SecurityIssue {
                        issue_type,
                        severity,
                        description,
                        line_number: Some(line_no + 1),
                        position: captures.get(0).map(|m| m.start()),
                        matched_content: Some(self.redact_sensitive_content(matched)),
                        remediation,
                    });
                }
            }
        }

        // Determine overall status
        let status = if issues.iter().any(|i| i.severity == SecuritySeverity::Critical) {
            SecurityStatus::Critical
        } else if issues.iter().any(|i| i.severity == SecuritySeverity::High) {
            SecurityStatus::Warning
        } else if !issues.is_empty() {
            SecurityStatus::Warning
        } else {
            SecurityStatus::Clean
        };

        SecurityScanResult {
            status,
            issues,
            scan_duration_ms: start_time.elapsed().as_millis() as u64,
            file_path: source.to_string(),
            file_size: content.len(),
        }
    }

    /// Classify sensitive pattern matches
    fn classify_sensitive_pattern(&self, matched_content: &str) -> (SecurityIssueType, SecuritySeverity, String, Option<String>) {
        let lower = matched_content.to_lowercase();
        
        if lower.contains("api") || lower.contains("key") || lower.contains("token") {
            (
                SecurityIssueType::SensitiveInformation,
                SecuritySeverity::High,
                "Potential API key or authentication token detected".to_string(),
                Some("Remove or replace with placeholder (e.g., ${API_KEY})".to_string()),
            )
        } else if lower.contains("password") || lower.contains("pwd") {
            (
                SecurityIssueType::SensitiveInformation,
                SecuritySeverity::High,
                "Potential password detected".to_string(),
                Some("Remove or replace with placeholder (e.g., ${PASSWORD})".to_string()),
            )
        } else if lower.contains("@") && lower.contains(".") {
            (
                SecurityIssueType::PrivacyConcern,
                SecuritySeverity::Medium,
                "Email address detected (potential PII)".to_string(),
                Some("Consider redacting or using example email".to_string()),
            )
        } else if matched_content.starts_with("eyJ") {
            (
                SecurityIssueType::SensitiveInformation,
                SecuritySeverity::Critical,
                "JWT token detected".to_string(),
                Some("Remove token and use placeholder".to_string()),
            )
        } else if matched_content.contains("BEGIN") && matched_content.contains("PRIVATE KEY") {
            (
                SecurityIssueType::SensitiveInformation,
                SecuritySeverity::Critical,
                "Private key detected".to_string(),
                Some("Remove private key immediately".to_string()),
            )
        } else {
            (
                SecurityIssueType::SensitiveInformation,
                SecuritySeverity::Medium,
                "Potentially sensitive information detected".to_string(),
                Some("Review and redact if necessary".to_string()),
            )
        }
    }

    /// Redact sensitive content for safe logging
    fn redact_sensitive_content(&self, content: &str) -> String {
        if content.len() <= 8 {
            "*".repeat(content.len())
        } else {
            format!("{}...{}", &content[..4], "*".repeat(content.len() - 4))
        }
    }

    /// Perform comprehensive security audit of a directory
    pub fn audit_directory<P: AsRef<Path>>(&self, dir_path: P) -> Result<SecurityAuditReport> {
        let start_time = std::time::Instant::now();
        let dir_path_str = dir_path.as_ref().to_string_lossy().to_string();
        
        info!("Starting security audit of directory: {}", dir_path_str);

        let mut scan_results = Vec::new();
        let mut total_files = 0;
        let mut clean_files = 0;
        let mut warning_files = 0;
        let mut critical_files = 0;
        let mut blocked_files = 0;

        // Recursively scan all files
        self.scan_directory_recursive(&dir_path, &mut scan_results, &mut total_files)?;

        // Analyze results
        for result in &scan_results {
            match result.status {
                SecurityStatus::Clean => clean_files += 1,
                SecurityStatus::Warning => warning_files += 1,
                SecurityStatus::Critical => critical_files += 1,
                SecurityStatus::Blocked => blocked_files += 1,
            }
        }

        let audit_duration = start_time.elapsed().as_millis() as u64;
        
        let report = SecurityAuditReport {
            directory: dir_path_str,
            scan_results,
            summary: SecurityAuditSummary {
                total_files,
                clean_files,
                warning_files,
                critical_files,
                blocked_files,
                audit_duration_ms: audit_duration,
            },
        };

        // Log audit completion
        log_security_event("directory_audit", "low", 
            Some(&format!("Completed audit of {}: {} files, {} issues", 
                        report.directory, total_files, 
                        warning_files + critical_files + blocked_files)));

        Ok(report)
    }

    /// Recursively scan directory for files
    fn scan_directory_recursive<P: AsRef<Path>>(
        &self,
        dir_path: P,
        results: &mut Vec<SecurityScanResult>,
        file_count: &mut usize,
    ) -> Result<()> {
        let entries = fs::read_dir(dir_path)?;
        
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_dir() {
                // Skip hidden directories
                if let Some(name) = path.file_name() {
                    if name.to_string_lossy().starts_with('.') {
                        continue;
                    }
                }
                self.scan_directory_recursive(&path, results, file_count)?;
            } else if path.is_file() {
                // Only scan text files
                if self.is_text_file(&path) {
                    *file_count += 1;
                    match self.scan_file(&path) {
                        Ok(result) => results.push(result),
                        Err(e) => {
                            warn!("Failed to scan file {}: {}", path.display(), e);
                        }
                    }
                }
            }
        }
        
        Ok(())
    }

    /// Check if a file is likely a text file
    fn is_text_file<P: AsRef<Path>>(&self, path: P) -> bool {
        if let Some(extension) = path.as_ref().extension() {
            let ext = extension.to_string_lossy().to_lowercase();
            matches!(ext.as_str(), "txt" | "md" | "json" | "yaml" | "yml" | "toml" | "rs" | "py" | "js" | "ts" | "go" | "java" | "c" | "cpp" | "h" | "hpp")
        } else {
            // Try to read first few bytes to detect text
            if let Ok(bytes) = fs::read(path.as_ref()) {
                bytes.iter().take(1024).all(|&b| b.is_ascii() || b == b'\n' || b == b'\r' || b == b'\t')
            } else {
                false
            }
        }
    }
}

/// Security audit report for a directory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityAuditReport {
    /// Directory that was audited
    pub directory: String,
    /// Individual file scan results
    pub scan_results: Vec<SecurityScanResult>,
    /// Summary statistics
    pub summary: SecurityAuditSummary,
}

/// Summary statistics for security audit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityAuditSummary {
    /// Total number of files scanned
    pub total_files: usize,
    /// Number of clean files
    pub clean_files: usize,
    /// Number of files with warnings
    pub warning_files: usize,
    /// Number of files with critical issues
    pub critical_files: usize,
    /// Number of blocked files
    pub blocked_files: usize,
    /// Total audit duration in milliseconds
    pub audit_duration_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_security_scanner_creation() {
        let scanner = SecurityScanner::new().unwrap();
        assert!(scanner.config.scan_content);
        assert!(scanner.config.vulnerability_scanning);
    }

    #[test]
    fn test_api_key_detection() {
        let scanner = SecurityScanner::new().unwrap();
        let content = "API_KEY=sk-1234567890abcdef";
        let result = scanner.scan_content(content, "test");
        
        assert_eq!(result.status, SecurityStatus::Warning);
        assert!(!result.issues.is_empty());
        assert_eq!(result.issues[0].issue_type, SecurityIssueType::SensitiveInformation);
    }

    #[test]
    fn test_malicious_content_detection() {
        let scanner = SecurityScanner::new().unwrap();
        let content = "rm -rf /";
        let result = scanner.scan_content(content, "test");
        
        assert_eq!(result.status, SecurityStatus::Blocked);
        assert!(!result.issues.is_empty());
        assert_eq!(result.issues[0].issue_type, SecurityIssueType::MaliciousContent);
    }

    #[test]
    fn test_clean_content() {
        let scanner = SecurityScanner::new().unwrap();
        let content = "This is a safe prompt for AI interaction.";
        let result = scanner.scan_content(content, "test");
        
        assert_eq!(result.status, SecurityStatus::Clean);
        assert!(result.issues.is_empty());
    }

    #[test]
    fn test_file_scanning() {
        let scanner = SecurityScanner::new().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        
        let mut file = fs::File::create(&file_path).unwrap();
        writeln!(file, "API_KEY=secret123456789abcdef").unwrap();
        
        let result = scanner.scan_file(&file_path).unwrap();
        assert_eq!(result.status, SecurityStatus::Warning);
        assert!(!result.issues.is_empty());
    }

    #[test]
    fn test_directory_audit() {
        let scanner = SecurityScanner::new().unwrap();
        let temp_dir = TempDir::new().unwrap();
        
        // Create test files
        let safe_file = temp_dir.path().join("safe.txt");
        let mut file = fs::File::create(&safe_file).unwrap();
        writeln!(file, "This is safe content").unwrap();
        
        let unsafe_file = temp_dir.path().join("unsafe.txt");
        let mut file = fs::File::create(&unsafe_file).unwrap();
        writeln!(file, "password=secret123").unwrap();
        
        let report = scanner.audit_directory(temp_dir.path()).unwrap();
        assert_eq!(report.summary.total_files, 2);
        assert_eq!(report.summary.clean_files, 1);
        assert_eq!(report.summary.warning_files, 1);
    }
}

// Implement Display for SecurityStatus
impl std::fmt::Display for SecurityStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecurityStatus::Clean => write!(f, "Clean"),
            SecurityStatus::Warning => write!(f, "Warning"),
            SecurityStatus::Critical => write!(f, "Critical"),
            SecurityStatus::Blocked => write!(f, "Blocked"),
        }
    }
}