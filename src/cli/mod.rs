use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name="ph", version, about="Find, compose, and reuse local prompts", color=clap::ColorChoice::Never,
 after_help="Start with ph to search and preview.\n\n  ph u review                  use a known prompt\n  ph f review                  find by name or description\n  ph n review 'Review {input}' save a prompt\n  ph e review                  edit in $EDITOR\n  ph c tone,review 'context'    compose left to right\n\nUse copies text in a terminal (shows it if copying is unavailable). Pipes print only text. Use exact names in scripts.")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}
#[derive(Args, Clone, Debug, Default)]
pub struct Destinations {
    /// Save the result as a new local prompt
    #[arg(short = 's', value_name = "NAME")]
    pub save: Option<String>,
    /// Append the result to an existing local prompt
    #[arg(short = 'a', value_name = "NAME")]
    pub append: Option<String>,
    /// Write to a file (omit PATH for a suggested filename)
    #[arg(short='f', value_name="PATH", num_args=0..=1, default_missing_value="")]
    pub file: Option<String>,
    /// Allow file replacement after saving a recoverable preimage
    #[arg(long)]
    pub force: bool,
    /// Suppress routine status and automatic clipboard use
    #[arg(short = 'q', long)]
    pub quiet: bool,
}
#[derive(Subcommand)]
pub enum Commands {
    /// Render a prompt; omit NAME to search and preview
    #[command(
        visible_alias = "u",
        after_help = "With terminal output, copy the rendered text; show it if copying is unavailable. Pipes and -q print only text. An explicit destination overrides this default.

Input comes from INPUT or stdin, never both. Omitted input is empty for {input}. The picker can collect missing declared values; exact-name commands use -v NAME=value."
    )]
    Use {
        /// Exact prompt name; omit to choose with a preview
        name: Option<String>,
        /// Literal context; omit to read stdin, or use empty context
        input: Option<String>,
        /// Review rendered text in $EDITOR before output
        #[arg(short = 'e', long)]
        edit: bool,
        /// Supply a declared template variable; repeat for distinct names
        #[arg(short = 'v', value_name = "NAME=VALUE")]
        vars: Vec<String>,
        /// Prefix comma-separated local prompt names or explicit file paths
        #[arg(long = "with", value_name = "FILES")]
        with_directives: Option<String>,
        /// Copy only; report an error if the clipboard is unavailable
        #[arg(short = 'c', long)]
        clipboard: bool,
        #[command(flatten)]
        destination: Destinations,
    },
    /// Show the original, unrendered body
    #[command(visible_alias = "s")]
    Show {
        name: String,
        #[arg(short = 'e', long)]
        edit: bool,
        /// Copy only; report an error if the clipboard is unavailable
        #[arg(short = 'c', long)]
        clipboard: bool,
        #[command(flatten)]
        destination: Destinations,
    },
    /// Save text or stdin; with no text in a terminal, open $EDITOR
    #[command(visible_alias = "n")]
    New {
        name: String,
        content: Option<String>,
        #[arg(short = 'e', long)]
        edit: bool,
        #[arg(long)]
        clean: bool,
        /// Copy only; report an error if the clipboard is unavailable
        #[arg(short = 'c', long)]
        clipboard: bool,
        #[command(flatten)]
        destination: Destinations,
    },
    /// Edit saved Markdown; omit NAME in a terminal to search
    #[command(visible_alias = "e")]
    Edit {
        /// Exact name; omit with terminal stdin/stdout to choose a prompt
        name: Option<String>,
    },
    /// Find prompts by name or description
    #[command(visible_alias = "f")]
    Find {
        query: String,
        /// Copy only; report an error if the clipboard is unavailable
        #[arg(short = 'c', long)]
        clipboard: bool,
        #[command(flatten)]
        destination: Destinations,
    },
    /// List canonical prompt names
    #[command(visible_aliases=["l","list"])]
    Ls {
        /// Copy only; report an error if the clipboard is unavailable
        #[arg(short = 'c', long)]
        clipboard: bool,
        #[command(flatten)]
        destination: Destinations,
    },
    /// Compose prompts left to right
    #[command(visible_aliases=["c","chain"])]
    Compose {
        prompts: String,
        input: Option<String>,
        #[arg(short = 'e', long)]
        edit: bool,
        /// Supply a declared template variable; repeat for distinct names
        #[arg(short = 'v', value_name = "NAME=VALUE")]
        vars: Vec<String>,
        #[arg(long)]
        clipboard: bool,
        #[command(flatten)]
        destination: Destinations,
    },
    /// Rename an exact prompt and its verified compatibility references
    #[command(visible_aliases=["r","mv"])]
    Rename { old_name: String, new_name: String },
    /// Delete an exact prompt after saving a recoverable version
    #[command(visible_aliases=["d","rm"])]
    Delete { name: String },
    /// Clean terminal formatting from text or stdin
    #[command(visible_alias = "x")]
    Clean {
        text: Option<String>,
        /// Copy only; report an error if the clipboard is unavailable
        #[arg(short = 'c', long)]
        clipboard: bool,
        #[command(flatten)]
        destination: Destinations,
    },
    /// Search and preview in the terminal
    #[command(visible_alias = "t")]
    Tui { search: Option<String> },
    /// Export a portable prompt-only directory with checksums
    Export { path: String },
    /// Import local Markdown or a portable directory
    Import {
        path: String,
        name: Option<String>,
        #[arg(short, long)]
        force: bool,
        #[arg(short, long)]
        version: bool,
        #[arg(short, long)]
        skip: bool,
        #[arg(short, long)]
        update: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// Compare exact prompt names
    Diff {
        prompt1: String,
        prompt2: String,
        #[arg(short = 'f', long, default_value = "unified")]
        format: String,
        #[arg(short = 'c', long, default_value = "3")]
        context: usize,
        #[arg(long)]
        output: Option<String>,
        #[arg(long)]
        force: bool,
    },
    /// Replace a target body from a source, preserving a preimage
    Merge {
        source: String,
        target: String,
        #[arg(short = 'b', long)]
        backup: bool,
        #[arg(short = 'p', long)]
        preview: bool,
        #[arg(short = 'i', long)]
        interactive: bool,
    },
    /// Save a named raw version
    Version {
        name: String,
        tag: String,
        #[arg(short = 'm', long)]
        message: Option<String>,
    },
    /// List saved versions, including deleted prompts
    Versions {
        name: String,
        #[arg(short = 'v', long)]
        verbose: bool,
    },
    /// Restore a saved version, preserving current content
    Rollback {
        name: String,
        version: String,
        #[arg(short = 'b', long)]
        backup: bool,
    },
    /// Generate bash, zsh, or fish completions
    Completion { shell: String },
}
