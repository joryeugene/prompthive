pub mod banks;
pub mod clean;
pub mod common;
pub mod completion;
pub mod configuration;
pub mod core;
pub mod diff_merge;
pub mod improvement;
pub mod registry;
pub mod sharing;
pub mod statistics;
pub mod subscription;
pub mod sync;
pub mod sync_types;
pub mod teams;
pub mod users;
pub mod versioning;
pub mod web;

// Export specific functions to avoid ambiguous glob re-exports
pub use completion::handle_completion;
pub use configuration::handle_config;

// Re-export entire modules for internal use
pub use diff_merge::*;

// Re-export simplified sync types for command modules
pub use sync_types::SimpleSyncManager;
