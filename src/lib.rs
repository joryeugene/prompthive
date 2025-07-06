//! PromptHive - A powerful CLI tool for managing and organizing prompt libraries
//!
//! PromptHive provides a comprehensive solution for storing, organizing, and retrieving
//! prompts for AI models. It supports features like fuzzy matching, template processing,
//! version control integration, and team collaboration.
//!
//! # Quick Start
//!
//! ```no_run
//! use prompthive::{Storage, Matcher};
//!
//! // Initialize storage
//! let storage = Storage::new()?;
//! storage.init()?;
//!
//! // Create a prompt matcher
//! let prompts = storage.list_prompts()?;
//! let matcher = Matcher::new(vec![]); // Load actual prompts as needed
//!
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Features
//!
//! - **Storage**: Persistent prompt storage with metadata
//! - **Matching**: Fuzzy search and exact matching for prompts
//! - **Templates**: Template processing with variable substitution
//! - **Compose**: Chain prompts together for complex workflows
//! - **Registry**: Share prompts via remote registries
//! - **TUI**: Terminal user interface for interactive browsing
//!
//! # Modules
//!
//! - [`storage`]: Core storage functionality for prompts and metadata
//! - [`matching`]: Fuzzy matching and search capabilities
//! - [`template`]: Template processing and variable substitution
//! - [`compose`]: Prompt composition and chaining (feature-gated)
//! - [`registry`]: Remote prompt registry integration (feature-gated)
//! - [`tui`]: Terminal user interface components (feature-gated)

pub mod cache;
pub mod clean;
pub mod cli;
pub mod clipboard;
pub mod commands;
pub mod common;
pub mod edit;
pub mod error_help;
pub mod health;
pub mod history;
pub mod io_options;
pub mod logging;
pub mod matching;
pub mod perf_verify;
pub mod signals;
pub mod storage;
pub mod security;
pub mod sync_manager;
pub mod telemetry;
pub mod template;

#[cfg(feature = "compose")]
pub mod compose;
#[cfg(feature = "import")]
pub mod import;
#[cfg(feature = "registry")]
pub mod registry;
#[cfg(feature = "registry")]
pub mod registry_tui;
#[cfg(feature = "tui")]
pub mod tui;

pub use cache::{DirectoryCache, PromptCache};
pub use cli::{Cli, Commands, VarsCommands};
pub use clipboard::Clipboard;
pub use commands::configuration::{get_editor_command_for_file, load_editor_config, EditorConfig};
pub use health::{ComponentHealth, HealthMonitor, HealthReport, HealthStatus, SystemMetrics};
pub use history::{HistoryEntry, HistoryTracker};
pub use io_options::{IoOptions, CommandCategory};
pub use logging::{init_logging, log_command_execution, log_security_event, log_storage_operation, LogConfig};
pub use matching::{MatchResult, Matcher, Prompt};
pub use perf_verify::{PerformanceReport, PerformanceVerifier};
pub use signals::{
    is_shutdown_requested, request_shutdown, ShutdownAware, ShutdownConfig, ShutdownHandler,
};
pub use storage::{PromptMetadata, Storage};
pub use security::{
    SecurityAuditReport, SecurityConfig, SecurityIssue, SecurityIssueType, SecurityScanResult, 
    SecurityScanner, SecuritySeverity, SecurityStatus,
};
pub use sync_manager::{SyncManager, SyncMetadata, SyncRegistry, SyncStatus, SyncStatusType};
pub use telemetry::{
    format_time_saved, generate_contribution_graph_html, init_telemetry, record_command_metric,
    record_performance_metric, TelemetryCollector,
};
pub use template::TemplateProcessor;

#[cfg(feature = "compose")]
pub use compose::{parse_prompt_list, Composer};
#[cfg(feature = "import")]
pub use import::{ImportResult, Importer};
#[cfg(feature = "registry")]
pub use registry::{
    default_registry_url, InstallResult, PackagePrompt, PublishRequest, RegistryClient,
    ShareResponse, SuggestionResponse, PublicShareRequest, InviteShareRequest,
};
