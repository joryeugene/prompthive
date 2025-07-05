//! Command-line interface definitions and parsing
//!
//! Defines the CLI structure using Clap, including all commands, subcommands,
//! and their associated arguments and options.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "ph")]
#[command(version)]
#[command(arg_required_else_help = false)]
#[command(about = "PromptHive - Lightning-fast prompt manager for AI workflows")]
#[command(help_template = "{about}

Usage: {usage}

Options:
  -h, --help     Print help
  -V, --version  Print version

{after-help}")]
#[command(after_help = "COMMANDS BY CATEGORY:

CORE OPERATIONS:
  use, u          Execute a prompt with optional input
  show, s         Display prompt content
  new, n          Create a new prompt
  edit, e         Edit a prompt in your editor
  delete, d, rm   Delete a prompt
  ls, l, list     List all prompts

SEARCH & DISCOVERY:
  find, f         Search prompts with fuzzy matching
  tui, t          Launch interactive TUI

COMPOSITION & UTILITIES:
  compose, c      Compose multiple prompts together
  clean, x        Clean TUI artifacts and formatting
  diff            Compare two prompts
  merge           Merge changes between prompts
  import          Import prompts from files

VERSIONING:
  version         Create a version tag
  versions        Show version history
  rollback        Rollback to previous version
  rename, r, mv   Rename a prompt

REGISTRY & SHARING:
  search          Search the community registry
  install         Install prompts from registry
  publish         Publish prompts to registry
  unpublish       Unpublish registry prompts
  share           Share prompts with others
  login           Login to registry
  logout          Logout from registry

CLOUD & TEAMS:
  sync            Synchronize prompts with cloud
  teams           Team collaboration features
  bank            Manage prompt banks
  users           User account management

SYSTEM & CONFIG:
  config          Configure settings
  security        Security scanning
  perf            Measure performance
  health          System health monitoring
  completion      Generate shell completions
  vars            Manage template variables
  values, v       Access core directive files

EXAMPLES:
  ph use debug \"error log\"           # Use with input
  ph new api-spec \"REST endpoints\"   # Create prompt
  echo \"code\" | ph use review        # Pipe input
  ph ls | grep test                  # List and filter

Run 'ph COMMAND --help' for more information on a command.")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    // === CORE OPERATIONS ===
    /// Execute a prompt with optional input
    #[command(alias = "u")]
    Use {
        /// Name of the prompt to use
        name: String,
        /// Optional input to replace {input} placeholder
        input: Option<String>,
        /// Open editor before outputting
        #[arg(short = 'e', long = "edit")]
        edit: bool,
        /// Save executed result as a new prompt
        #[arg(short = 's', value_name = "NAME")]
        save: Option<String>,
        /// Append executed result to an existing prompt
        #[arg(short = 'a', value_name = "NAME")]
        append: Option<String>,
        /// Copy to clipboard (explicit override)
        #[arg(short = 'c')]
        clipboard: bool,
        /// Write to file (smart filename if no path provided)
        #[arg(short = 'f', value_name = "PATH", num_args = 0..=1, default_missing_value = "")]
        file: Option<String>,
        /// Quiet mode - suppress output messages
        #[arg(short = 'q')]
        quiet: bool,
        /// Auto-import and prepend directive files (comma-separated list)
        #[arg(long = "with", value_name = "FILES")]
        with_directives: Option<String>,
    },
    /// Display prompt content
    #[command(alias = "s")]
    Show {
        /// Name of the prompt to show
        name: String,
        /// Open in editor before outputting
        #[arg(short = 'e', long = "edit")]
        edit: bool,
        /// Save shown prompt as a new prompt
        #[arg(short = 's', value_name = "NAME")]
        save: Option<String>,
        /// Append shown prompt to an existing prompt
        #[arg(short = 'a', value_name = "NAME")]
        append: Option<String>,
        /// Copy to clipboard (explicit override)
        #[arg(short = 'c')]
        clipboard: bool,
        /// Write to file (smart filename if no path provided)
        #[arg(short = 'f', value_name = "PATH", num_args = 0..=1, default_missing_value = "")]
        file: Option<String>,
        /// Quiet mode - suppress output messages
        #[arg(short = 'q')]
        quiet: bool,
    },
    /// Create a new prompt
    #[command(alias = "n")]
    New {
        /// Name or content for the new prompt
        name: String,
        /// Optional: explicit name when first arg is content
        explicit_name: Option<String>,
        /// Clean TUI artifacts from input
        #[arg(long = "clean")]
        clean: bool,
        /// Save new prompt under a different name
        #[arg(short = 's', value_name = "NAME")]
        save: Option<String>,
        /// Append to existing prompt
        #[arg(short = 'a', value_name = "NAME")]
        append: Option<String>,
        /// Copy to clipboard (explicit override)
        #[arg(short = 'c')]
        clipboard: bool,
        /// Write to file (bidirectional sync by default)
        #[arg(short = 'f', value_name = "PATH")]
        file: Option<String>,
        /// Quiet mode - suppress output messages
        #[arg(short = 'q')]
        quiet: bool,
        /// Open in editor after creation
        #[arg(short = 'e', long = "edit")]
        edit: bool,
        /// Create sync with local file (deprecated, use -f)
        #[arg(long = "sync", value_name = "PATH", hide = true)]
        sync: Option<String>,
    },
    /// Edit a prompt in your editor
    #[command(alias = "e")]
    Edit {
        /// Name of the prompt to edit
        name: String,
    },
    /// Delete a prompt
    #[command(alias = "d", alias = "rm")]
    Delete {
        /// Name of the prompt to delete
        name: String,
    },

    // === DISCOVERY & SEARCH ===
    /// List all prompts
    #[command(alias = "l", alias = "list")]
    Ls {
        /// Save list as a new prompt
        #[arg(short = 's', value_name = "NAME")]
        save: Option<String>,
        /// Append list to an existing prompt
        #[arg(short = 'a', value_name = "NAME")]
        append: Option<String>,
        /// Copy to clipboard (explicit override)
        #[arg(short = 'c')]
        clipboard: bool,
        /// Write to file (smart filename if no path provided)
        #[arg(short = 'f', value_name = "PATH", num_args = 0..=1, default_missing_value = "")]
        file: Option<String>,
        /// Quiet mode - suppress output messages
        #[arg(short = 'q')]
        quiet: bool,
    },
    /// Search prompts with fuzzy matching
    #[command(alias = "f")]
    Find {
        /// Search query
        query: String,
        /// Save results as a new prompt
        #[arg(short = 's', value_name = "NAME")]
        save: Option<String>,
        /// Append results to an existing prompt
        #[arg(short = 'a', value_name = "NAME")]
        append: Option<String>,
        /// Copy to clipboard (explicit override)
        #[arg(short = 'c')]
        clipboard: bool,
        /// Write to file (smart filename if no path provided)
        #[arg(short = 'f', value_name = "PATH", num_args = 0..=1, default_missing_value = "")]
        file: Option<String>,
        /// Quiet mode - suppress output messages
        #[arg(short = 'q')]
        quiet: bool,
    },

    // === COMPOSITION & UTILITIES ===
    /// Compose multiple prompts together
    #[command(alias = "c", alias = "chain")]
    Compose {
        /// Comma-separated prompt names (e.g. a,b,c)
        prompts: String,
        /// Optional input for first prompt in chain
        input: Option<String>,
        /// Open editor before outputting
        #[arg(short = 'e', long = "edit")]
        edit: bool,
        /// Save composition result as a new prompt
        #[arg(short = 's', value_name = "NAME")]
        save: Option<String>,
        /// Append composition result to an existing prompt
        #[arg(short = 'a', value_name = "NAME")]
        append: Option<String>,
        /// Copy to clipboard (explicit override)
        #[arg(long = "clipboard")]
        clipboard: bool,
        /// Write to file (smart filename if no path provided)
        #[arg(short = 'f', value_name = "PATH", num_args = 0..=1, default_missing_value = "")]
        file: Option<String>,
        /// Quiet mode - suppress output messages
        #[arg(short = 'q')]
        quiet: bool,
    },
    /// Import prompts from files or directories
    #[cfg(feature = "import")]
    Import {
        /// Path to import from
        path: String,
        /// Optional custom name for the imported prompt
        name: Option<String>,
        /// Force overwrite existing prompts
        #[arg(short, long)]
        force: bool,
        /// Create versioned copies instead of overwriting (e.g., file-v2, file-v3)
        #[arg(short, long)]
        version: bool,
        /// Skip existing prompts without prompting
        #[arg(short, long)]
        skip: bool,
        /// Update existing prompts only if source is newer
        #[arg(short, long)]
        update: bool,
    },

    /// Clean TUI artifacts and formatting
    #[command(alias = "x")]
    Clean {
        /// Text to clean (optional - uses stdin if not provided)
        text: Option<String>,
        /// Save cleaned text as a new prompt (auto-clipboard unless -q)
        #[arg(short = 's', value_name = "NAME")]
        save: Option<String>,
        /// Append cleaned text to an existing prompt (auto-clipboard unless -q)
        #[arg(short = 'a', value_name = "NAME")]
        append: Option<String>,
        /// Copy to clipboard (explicit override)
        #[arg(short = 'c')]
        clipboard: bool,
        /// Write to file (smart filename if no path provided)
        #[arg(short = 'f', value_name = "PATH", num_args = 0..=1, default_missing_value = "")]
        file: Option<String>,
        /// Quiet mode - suppress clipboard and output messages
        #[arg(short = 'q')]
        quiet: bool,
    },
    /// Compare two prompts and show differences
    Diff {
        /// First prompt to compare
        prompt1: String,
        /// Second prompt to compare
        prompt2: String,
        /// Output format (unified, side-by-side, or brief)
        #[arg(short = 'f', long = "format", default_value = "unified")]
        format: String,
        /// Context lines around differences
        #[arg(short = 'c', long = "context", default_value = "3")]
        context: usize,
        /// Save diff output to file
        #[arg(long = "output")]
        output: Option<String>,
    },
    /// Merge changes from one prompt into another
    Merge {
        /// Source prompt (to copy from)
        source: String,
        /// Target prompt (to merge into)
        target: String,
        /// Create backup before merging
        #[arg(short = 'b', long = "backup")]
        backup: bool,
        /// Preview merge without applying changes
        #[arg(short = 'p', long = "preview")]
        preview: bool,
        /// Interactive merge mode
        #[arg(short = 'i', long = "interactive")]
        interactive: bool,
    },
    // === VERSIONING ===
    /// Create a version tag for prompt
    Version {
        /// Name of the prompt to version
        name: String,
        /// Version tag (e.g. v1.0, stable, working)
        tag: String,
        /// Optional description for this version
        #[arg(short = 'm', long = "message")]
        message: Option<String>,
    },
    /// Show version history for a prompt
    Versions {
        /// Name of the prompt to show versions for
        name: String,
        /// Show detailed information
        #[arg(short = 'v', long = "verbose")]
        verbose: bool,
    },
    /// Rollback prompt to a previous version
    Rollback {
        /// Name of the prompt to rollback
        name: String,
        /// Version tag to rollback to
        version: String,
        /// Create backup before rollback
        #[arg(short = 'b', long = "backup")]
        backup: bool,
    },
    /// Rename a prompt
    #[command(alias = "r", alias = "mv")]
    Rename {
        /// Current name of the prompt
        old_name: String,
        /// New name for the prompt
        new_name: String,
    },
    /// Show usage statistics
    Stats,
    /// Show command history
    History {
        /// Number of recent entries to show
        #[arg(short = 'n', long = "limit", default_value = "10")]
        limit: usize,
        /// Search query to filter history
        #[arg(short = 's', long = "search")]
        search: Option<String>,
        /// Show only successful commands
        #[arg(long = "success-only")]
        success_only: bool,
        /// Replay the last command
        #[arg(long = "last")]
        last: bool,
    },
    /// Batch operations on multiple prompts
    Batch {
        /// Query to find prompts (e.g. "find test", "bank/old", "recent 10")
        query: String,
        /// Add tags to matching prompts
        #[arg(long = "add-tag")]
        add_tag: Option<String>,
        /// Remove tags from matching prompts
        #[arg(long = "remove-tag")]
        remove_tag: Option<String>,
        /// Move prompts to a different bank/location
        #[arg(long = "move-to")]
        move_to: Option<String>,
        /// Export prompts to directory
        #[arg(long = "export")]
        export: Option<String>,
        /// Delete matching prompts (requires confirmation)
        #[arg(long = "delete")]
        delete: bool,
        /// Copy results to clipboard
        #[arg(short = 'c')]
        clipboard: bool,
        /// Write results to file
        #[arg(short = 'f', value_name = "PATH")]
        file: Option<String>,
        /// Quiet mode - suppress output messages
        #[arg(short = 'q')]
        quiet: bool,
    },

    // === REGISTRY & SHARING ===
    /// Search the community registry
    #[cfg(feature = "registry")]
    Search {
        /// Search query
        query: String,
        /// Maximum results to show
        #[arg(short, long, default_value = "10")]
        limit: u32,
    },
    /// Install prompts from registry
    Install {
        /// Package to install (e.g. @user/package)
        package: String,
    },
    /// Publish prompts to registry
    Publish {
        /// Name of the prompt to publish
        name: String,
        /// Description for the published prompt (optional)
        #[arg(short = 'd', long = "description")]
        description: Option<String>,
        /// Version for the published prompt (default: 1.0.0)
        #[arg(short = 'v', long = "version", default_value = "1.0.0")]
        version: String,
        /// Tags for the prompt (comma-separated)
        #[arg(short = 't', long = "tags")]
        tags: Option<String>,
        /// Bank/category for the prompt (default: personal)
        #[arg(short = 'b', long = "bank", default_value = "personal")]
        bank: String,
    },
    /// Unpublish registry prompts
    Unpublish {
        /// Package name to unpublish (e.g. user/package or user/package@version)
        package: String,
    },
    /// Browse registry packages
    #[cfg(feature = "registry")]
    Browse {
        /// Search query
        query: String,
        /// Maximum results to show
        #[arg(short, long, default_value = "10")]
        limit: u32,
    },
    /// Login to registry
    Login {
        /// Email address for magic link authentication
        #[arg(short = 'e', long = "email")]
        email: Option<String>,
        /// API key (if you already have one)
        #[arg(long = "api-key")]
        api_key: Option<String>,
    },
    /// Logout from registry
    Logout,

    /// Synchronize prompts with cloud
    Sync {
        #[command(subcommand)]
        action: Option<crate::commands::sync::SyncCommands>,
    },
    /// Team collaboration features
    Teams {
        #[command(subcommand)]
        action: Option<crate::commands::teams::TeamsCommands>,
    },
    /// Share prompts with others
    Share {
        /// Name of the prompt to share
        prompt: String,
        /// Create public sharing link (anyone with link can view)
        #[arg(long = "public")]
        public: bool,
        /// Share with specific email addresses (comma-separated)
        #[arg(long = "invite")]
        invite: Option<String>,
        /// Allow suggestions and improvements from viewers
        #[arg(long = "allow-suggestions", default_value = "true")]
        allow_suggestions: bool,
        /// Set expiration time in hours (default: never expires)
        #[arg(long = "expires")]
        expires: Option<u32>,
    },
    /// Manage prompt suggestions
    Suggestions {
        #[command(subcommand)]
        action: Option<crate::commands::sharing::SuggestionsCommands>,
    },
    /// Submit prompts for improvement
    #[cfg(feature = "registry")]
    Improve {
        #[command(subcommand)]
        action: Option<crate::commands::improvement::ImprovementCommands>,
    },

    /// Manage prompt banks
    Bank {
        #[command(subcommand)]
        action: Option<crate::commands::banks::BankCommands>,
    },
    /// User account management
    Users {
        #[command(subcommand)]
        action: Option<crate::commands::users::UserCommands>,
    },
    /// Account status and usage
    Subscription {
        #[command(subcommand)]
        action: Option<crate::commands::subscription::SubscriptionCommands>,
    },

    // === SYSTEM & CONFIG ===
    /// Launch interactive TUI
    #[cfg(feature = "tui")]
    #[command(alias = "t")]
    Tui {
        /// Optional search query to filter/select on open
        search: Option<String>,
    },
    /// Generate shell completion scripts
    Completion {
        /// Shell to generate completion for (bash, zsh, fish)
        shell: String,
    },
    /// Configure telemetry settings
    Config {
        /// Configuration category
        category: String,
        /// Configuration action
        action: String,
    },
    /// Security scanning and audit
    Security {
        /// Scan all prompts for security issues
        #[arg(long, short = 'a')]
        all: bool,
        /// Output format (table, json)
        #[arg(long, short = 'f', default_value = "table")]
        format: String,
        /// Show only summary
        #[arg(long)]
        summary: bool,
    },

    /// Manage template variables
    Vars {
        #[command(subcommand)]
        action: Option<VarsCommands>,
    },

    /// Open web dashboard
    Web {
        /// Page to open (stats, prompts, etc.)
        page: Option<String>,
        /// Port for local server (default: 8080)
        #[arg(short = 'p', long = "port", default_value = "8080")]
        port: u16,
        /// Don't open browser automatically
        #[arg(long = "no-browser")]
        no_browser: bool,
    },

    /// Measure performance metrics
    Perf {
        /// Run full performance verification suite
        #[arg(long = "verify")]
        verify: bool,
    },
    /// System health monitoring
    Health {
        /// Output format (json, pretty, compact)
        #[arg(long = "format", default_value = "pretty")]
        format: String,
        /// Show only basic health status
        #[arg(long = "simple")]
        simple: bool,
        /// Watch mode - continuous health monitoring
        #[arg(long = "watch")]
        watch: bool,
    },
    /// Access core directive files and values
    #[command(alias = "v")]
    Values {
        /// Optional specific value to retrieve (claude, values, etc.)
        name: Option<String>,
        /// Save result as a new prompt
        #[arg(short = 's', value_name = "NAME")]
        save: Option<String>,
        /// Append result to an existing prompt
        #[arg(short = 'a', value_name = "NAME")]
        append: Option<String>,
        /// Copy to clipboard (explicit override)
        #[arg(short = 'c')]
        clipboard: bool,
        /// Write to file (smart filename if no path provided)
        #[arg(short = 'f', value_name = "PATH", num_args = 0..=1, default_missing_value = "")]
        file: Option<String>,
        /// Quiet mode - suppress output messages
        #[arg(short = 'q')]
        quiet: bool,
    },
}

#[derive(Subcommand)]
pub enum VarsCommands {
    /// List all available template variables
    #[command(alias = "ls")]
    List,
    /// Set a custom template variable
    Set {
        /// Variable name
        name: String,
        /// Variable value
        value: String,
    },
    /// Remove a custom template variable
    #[command(alias = "rm")]
    Remove {
        /// Variable name to remove
        name: String,
    },
    /// Show example usage of template variables
    Examples,
}
