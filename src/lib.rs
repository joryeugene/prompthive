//! Local Markdown prompt storage, literal rendering, and terminal discovery.
pub mod clean;
pub mod cli;
pub mod clipboard;
pub mod commands;
pub mod compose;
pub mod edit;
pub mod import;
pub mod io_options;
pub mod limits;
pub mod matching;
pub mod persistence;
pub mod portability;
pub mod storage;
pub mod template;
#[cfg(feature = "tui")]
pub mod tui;
pub use cli::{Cli, Commands};
pub use clipboard::Clipboard;
pub use compose::{parse_prompt_list, Composer};
pub use import::{ImportResult, Importer};
pub use io_options::IoOptions;
pub use matching::{MatchResult, Matcher, Prompt};
pub use storage::{PromptMetadata, Storage};
pub use template::TemplateProcessor;
