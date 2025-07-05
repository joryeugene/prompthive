//! Structured logging configuration and utilities for PromptHive
//!
//! This module provides comprehensive logging infrastructure with:
//! - Environment-based log level configuration
//! - Structured JSON logging for production
//! - Human-readable console logging for development
//! - File rotation and archival
//! - Performance-aware log filtering

use anyhow::Result;
use std::env;
use std::path::PathBuf;
use tracing::{debug, error, info, trace, warn};
use tracing_subscriber::{fmt::format::FmtSpan, EnvFilter};

/// Logging configuration for different environments
#[derive(Debug, Clone)]
pub struct LogConfig {
    /// Log level (trace, debug, info, warn, error)
    pub level: String,
    /// Output format (json, pretty, compact)
    pub format: LogFormat,
    /// Log file directory (None for stdout only)
    pub file_dir: Option<PathBuf>,
    /// Enable colored output
    pub colored: bool,
    /// Enable source location logging
    pub with_location: bool,
    /// Enable span timing
    pub with_spans: bool,
}

#[derive(Debug, Clone)]
pub enum LogFormat {
    /// JSON structured logging for production
    Json,
    /// Pretty human-readable for development  
    Pretty,
    /// Compact single-line format
    Compact,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: "error".to_string(),
            format: LogFormat::Pretty,
            file_dir: None,
            colored: is_terminal::IsTerminal::is_terminal(&std::io::stderr()),
            with_location: false,
            with_spans: false,
        }
    }
}

impl LogConfig {
    /// Create logging configuration from environment variables
    pub fn from_env() -> Self {
        let level = env::var("PROMPTHIVE_LOG_LEVEL")
            .or_else(|_| env::var("LOG_LEVEL"))
            .unwrap_or_else(|_| "error".to_string());

        let format = match env::var("PROMPTHIVE_LOG_FORMAT").as_deref() {
            Ok("json") => LogFormat::Json,
            Ok("compact") => LogFormat::Compact,
            _ => LogFormat::Pretty,
        };

        let file_dir = env::var("PROMPTHIVE_LOG_DIR").ok().map(PathBuf::from);

        let colored = env::var("PROMPTHIVE_LOG_COLOR")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or_else(|_| is_terminal::IsTerminal::is_terminal(&std::io::stderr()));

        let with_location = env::var("PROMPTHIVE_LOG_LOCATION")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false);

        let with_spans = env::var("PROMPTHIVE_LOG_SPANS")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false);

        Self {
            level,
            format,
            file_dir,
            colored,
            with_location,
            with_spans,
        }
    }
}

/// Initialize the global tracing subscriber
pub fn init_logging(config: LogConfig) -> Result<()> {
    // Create environment filter
    let env_filter = EnvFilter::try_new(&config.level)
        .or_else(|_| EnvFilter::try_new("error"))
        .unwrap_or_else(|_| EnvFilter::new("error"));

    // Simple console-only logging for now
    let span_events = if config.with_spans {
        FmtSpan::NEW | FmtSpan::CLOSE
    } else {
        FmtSpan::NONE
    };

    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(std::io::stderr)
        .with_ansi(config.colored)
        .with_span_events(span_events)
        .with_file(config.with_location)
        .with_line_number(config.with_location);

    match config.format {
        LogFormat::Json => subscriber
            .json()
            .try_init()
            .map_err(|e| anyhow::anyhow!("Failed to initialize JSON logging: {}", e))?,
        LogFormat::Pretty => subscriber
            .pretty()
            .try_init()
            .map_err(|e| anyhow::anyhow!("Failed to initialize pretty logging: {}", e))?,
        LogFormat::Compact => subscriber
            .compact()
            .try_init()
            .map_err(|e| anyhow::anyhow!("Failed to initialize compact logging: {}", e))?,
    }

    info!(
        level = %config.level,
        format = ?config.format,
        colored = config.colored,
        "Logging initialized"
    );

    Ok(())
}

/// Log a command execution with timing and context
pub fn log_command_execution<T>(
    command_name: &str,
    duration_ms: u64,
    success: bool,
    result: &Result<T>,
) {
    let span = tracing::info_span!(
        "command_execution",
        command = command_name,
        duration_ms = duration_ms,
        success = success
    );

    let _enter = span.enter();

    if success {
        info!(
            command = command_name,
            duration_ms = duration_ms,
            "Command completed successfully"
        );
    } else {
        match result {
            Ok(_) => warn!(
                command = command_name,
                duration_ms = duration_ms,
                "Command completed with warnings"
            ),
            Err(e) => error!(
                command = command_name,
                duration_ms = duration_ms,
                error = %e,
                "Command failed"
            ),
        }
    }
}

/// Log storage operations for audit trail
pub fn log_storage_operation(
    operation: &str,
    prompt_name: Option<&str>,
    success: bool,
    duration_ms: Option<u64>,
) {
    let span = tracing::info_span!(
        "storage_operation",
        operation = operation,
        prompt_name = prompt_name,
        success = success,
        duration_ms = duration_ms
    );

    let _enter = span.enter();

    if success {
        debug!(
            operation = operation,
            prompt_name = prompt_name,
            duration_ms = duration_ms,
            "Storage operation completed"
        );
    } else {
        warn!(
            operation = operation,
            prompt_name = prompt_name,
            duration_ms = duration_ms,
            "Storage operation failed"
        );
    }
}

/// Log performance metrics for monitoring
pub fn log_performance_metric(metric_name: &str, value: f64, unit: &str, context: Option<&str>) {
    trace!(
        metric = metric_name,
        value = value,
        unit = unit,
        context = context,
        "Performance metric recorded"
    );
}

/// Log security events for audit
pub fn log_security_event(event_type: &str, severity: &str, details: Option<&str>) {
    match severity {
        "high" | "critical" => error!(
            event_type = event_type,
            severity = severity,
            details = details,
            "Security event detected"
        ),
        "medium" => warn!(
            event_type = event_type,
            severity = severity,
            details = details,
            "Security event detected"
        ),
        _ => info!(
            event_type = event_type,
            severity = severity,
            details = details,
            "Security event detected"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Once;

    static INIT: Once = Once::new();

    fn init_test_logging() {
        INIT.call_once(|| {
            let config = LogConfig {
                level: "debug".to_string(),
                format: LogFormat::Compact,
                file_dir: None,
                colored: false,
                with_location: false,
                with_spans: false,
            };
            let _ = init_logging(config);
        });
    }

    #[test]
    fn test_log_config_from_env() {
        env::set_var("PROMPTHIVE_LOG_LEVEL", "debug");
        env::set_var("PROMPTHIVE_LOG_FORMAT", "json");
        env::set_var("PROMPTHIVE_LOG_COLOR", "false");

        let config = LogConfig::from_env();
        assert_eq!(config.level, "debug");
        matches!(config.format, LogFormat::Json);
        assert!(!config.colored);

        env::remove_var("PROMPTHIVE_LOG_LEVEL");
        env::remove_var("PROMPTHIVE_LOG_FORMAT");
        env::remove_var("PROMPTHIVE_LOG_COLOR");
    }

    #[test]
    fn test_command_execution_logging() {
        init_test_logging();

        let result: Result<()> = Ok(());
        log_command_execution("test_command", 100, true, &result);

        let result: Result<()> = Err(anyhow::anyhow!("test error"));
        log_command_execution("test_command", 200, false, &result);
    }

    #[test]
    fn test_storage_operation_logging() {
        init_test_logging();

        log_storage_operation("read", Some("test-prompt"), true, Some(50));
        log_storage_operation("write", Some("test-prompt"), false, Some(100));
        log_storage_operation("list", None, true, Some(25));
    }

    #[test]
    fn test_performance_metric_logging() {
        init_test_logging();

        log_performance_metric("query_time", 45.2, "ms", Some("fuzzy_search"));
        log_performance_metric("memory_usage", 128.5, "MB", None);
    }

    #[test]
    fn test_security_event_logging() {
        init_test_logging();

        log_security_event("permission_check", "low", Some("file access granted"));
        log_security_event("api_key_validation", "high", Some("invalid key detected"));
    }
}
