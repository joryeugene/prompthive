use anyhow::Result;
use clap::Parser;
use std::env;
use std::time::Instant;
use prompthive::error_help;

#[cfg(feature = "registry")]
use prompthive::RegistryClient;
use prompthive::{
    init_logging, init_telemetry, log_command_execution, record_command_metric, CommandCategory,
    HistoryEntry, HistoryTracker, IoOptions, LogConfig,
    MatchResult, Matcher, PerformanceVerifier, Prompt, ShutdownHandler, Storage,
    TelemetryCollector,
};

#[cfg(feature = "compose")]
use prompthive::{parse_prompt_list, Composer};

#[cfg(feature = "import")]
use prompthive::Importer;

// Re-export for command modules
pub use prompthive::storage::PromptMetadata;
pub use prompthive::{clean, edit};

#[cfg(feature = "registry")]
pub use prompthive::{PackagePrompt, PublishRequest};

mod cli;
mod commands;

use cli::{Cli, Commands};

// Import command handlers
use crate::commands::core::{handle_use, handle_show, handle_edit, handle_ls, handle_delete, handle_rename, handle_import, handle_compose, handle_find};

// CLI command definitions moved to src/cli/mod.rs

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables from .env file
    if let Err(_) = dotenvy::dotenv() {
        // .env file not found or readable - this is fine, use system env vars
    }

    // Handle -v flag manually since clap only supports -V by default
    let args: Vec<String> = env::args().collect();
    if args.len() == 2 && args[1] == "-v" {
        println!("ph {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let start = Instant::now();
    
    // Parse CLI with custom error handling for better suggestions
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            // Check if this is an unknown command error
            if let clap::error::ErrorKind::InvalidSubcommand = e.kind() {
                let args: Vec<String> = env::args().collect();
                if args.len() > 1 {
                    let cmd = &args[1];
                    let available_commands = vec![
                        "use", "u", "show", "s", "new", "n", "edit", "e", "delete", "d", "rm",
                        "ls", "l", "list", "find", "f", "tui", "t", "compose", "c", "clean", "x",
                        "diff", "merge", "import", "version", "versions", "rollback", "rename", "r", "mv",
                        "search", "install", "publish", "sync", "login", "logout", "banks", "init",
                        "completion", "stats", "config"
                    ];
                    
                    use crate::error_help;
                    eprintln!("{}", error_help::format_command_typo(cmd, &available_commands));
                    std::process::exit(1);
                }
            }
            // For other errors, use the default clap error display
            e.exit();
        }
    };

    // Only initialize logging if explicitly requested via env var
    if env::var("PROMPTHIVE_LOG_LEVEL").is_ok() {
        let log_config = LogConfig::from_env();
        init_logging(log_config)?;
    }

    // Set up graceful shutdown handling
    let shutdown_handler = ShutdownHandler::new();
    shutdown_handler.setup_signal_handlers()?;

    // Initialize storage
    let storage = Storage::new()?;
    storage.init()?;

    // Initialize telemetry only if not in performance mode
    let mut telemetry = if env::var("PROMPTHIVE_PERF_MODE").is_ok() {
        None
    } else {
        init_telemetry(storage.base_dir().to_path_buf()).ok()
    };

    // If no command provided, launch TUI if available, otherwise show help
    if cli.command.is_none() {
        #[cfg(feature = "tui")]
        {
            // Check if we have a proper terminal for TUI
            use std::io::IsTerminal;
            let has_tty = std::io::stdout().is_terminal() && 
                         std::env::var("PROMPTHIVE_TEST_MODE").is_err();
            
            if has_tty {
                use prompthive::tui::PromptTui;
                match PromptTui::new(&storage) {
                    Ok(tui) => return tui.run(&storage),
                    Err(_) => {
                        // TUI initialization failed, fall back to help
                        use clap::CommandFactory;
                        Cli::command().print_help()?;
                        return Ok(());
                    }
                }
            } else {
                // No TTY or in test mode, show help
                use clap::CommandFactory;
                Cli::command().print_help()?;
                return Ok(());
            }
        }

        #[cfg(not(feature = "tui"))]
        {
            // Show help if TUI not available
            use clap::CommandFactory;
            Cli::command().print_help()?;
            return Ok(());
        }
    }

    match cli.command.unwrap() {
        Commands::Use {
            name,
            input,
            edit,
            save,
            append,
            clipboard,
            file,
            quiet,
            with_directives,
        } => {
            let io_options = IoOptions::new(
                save.as_deref(),
                append.as_deref(),
                clipboard,
                file.as_deref(),
                quiet,
            ).with_category(CommandCategory::TextTransform);
            let result = handle_use(
                &storage,
                &name,
                input.as_deref(),
                edit,
                &io_options,
                with_directives.as_deref(),
                start,
            );
            let success = result.is_ok();
            let error_type = if let Err(ref e) = result {
                Some(format!("{}", e))
            } else {
                None
            };
            record_command_metric(
                &mut telemetry,
                "use",
                start.elapsed(),
                success,
                None,
                error_type,
            );
            result?;
        }
        Commands::Show {
            name,
            edit,
            save,
            append,
            clipboard,
            file,
            quiet,
        } => {
            let io_options = IoOptions::new(
                save.as_deref(),
                append.as_deref(),
                clipboard,
                file.as_deref(),
                quiet,
            ).with_category(CommandCategory::TextTransform);
            let result = handle_show(&storage, &name, edit, &io_options, start);
            let success = result.is_ok();
            let error_type = if let Err(ref e) = result {
                Some(format!("{}", e))
            } else {
                None
            };
            record_command_metric(
                &mut telemetry,
                "show",
                start.elapsed(),
                success,
                None,
                error_type,
            );
            result?;
        }
        Commands::New {
            name,
            explicit_name,
            clean,
            save,
            append,
            clipboard,
            file,
            quiet,
            edit,
            sync,
        } => {
            let io_options = IoOptions::new(save.as_deref(), append.as_deref(), clipboard, file.as_deref(), quiet)
                .with_category(CommandCategory::Creation);
            crate::commands::core::handle_new(&storage, &name, explicit_name.as_deref(), edit, clean, sync.as_deref(), &io_options, start)?;
        }
        Commands::Edit { name } => {
            handle_edit(&storage, &name, start)?;
        }
        Commands::Find {
            query,
            save,
            append,
            clipboard,
            file,
            quiet,
        } => {
            let io_options = IoOptions::new(save.as_deref(), append.as_deref(), clipboard, file.as_deref(), quiet)
                .with_category(CommandCategory::Query);
            handle_find(&storage, &query, &io_options, start)?;
        }
        Commands::Install { package } => {
            crate::commands::registry::handle_install(&storage, &package, start).await?;
        }
        Commands::Publish {
            name,
            description,
            version,
            tags,
            bank,
        } => {
            crate::commands::registry::handle_publish(
                &storage,
                &name,
                description.as_deref(),
                &version,
                tags.as_deref(),
                &bank,
                start,
            )
            .await?;
        }
        Commands::Unpublish { package } => {
            crate::commands::registry::handle_unpublish(&package, start).await?;
        }
        #[cfg(feature = "registry")]
        Commands::Search { query, limit } => {
            crate::commands::registry::handle_search(&query, Some(limit), start).await?;
        }
        #[cfg(feature = "registry")]
        Commands::Browse { query, limit } => {
            crate::commands::registry::handle_browse(&query, limit, start).await?;
        }
        Commands::Login { email, api_key } => {
            crate::commands::registry::handle_login(email.as_deref(), api_key.as_deref(), start).await?;
        }
        Commands::Logout => {
            crate::commands::registry::handle_logout(start)?;
        }
        #[cfg(feature = "import")]
        Commands::Import {
            path,
            name,
            force,
            version,
            skip,
            update,
        } => {
            handle_import(
                &storage,
                &path,
                name.as_deref(),
                force,
                version,
                skip,
                update,
                start,
            )?;
        }
        Commands::Compose {
            prompts,
            input,
            edit,
            save,
            append,
            clipboard,
            file,
            quiet,
        } => {
            let io_options = IoOptions::new(
                save.as_deref(),
                append.as_deref(),
                clipboard,
                file.as_deref(),
                quiet,
            ).with_category(CommandCategory::Utility);
            handle_compose(&storage, &prompts, input.as_deref(), edit, &io_options, start)?;
        }
        Commands::Ls {
            save,
            append,
            clipboard,
            file,
            quiet,
        } => {
            let io_options = IoOptions::new(save.as_deref(), append.as_deref(), clipboard, file.as_deref(), quiet)
                .with_category(CommandCategory::Query);
            handle_ls(&storage, &io_options, start)?;
        }
        Commands::Delete { name } => {
            handle_delete(&storage, &name, start)?;
        }
        Commands::Rename { old_name, new_name } => {
            handle_rename(&storage, &old_name, &new_name, start)?;
        }
        #[cfg(feature = "tui")]
        Commands::Tui { search: _ } => {
            use prompthive::tui::PromptTui;
            let tui = PromptTui::new(&storage)?;
            return tui.run(&storage);
        }
        Commands::Completion { shell } => {
            commands::handle_completion(&shell, start)?;
        }
        Commands::Config { category, action } => {
            // Initialize telemetry if needed for config command
            let mut telemetry = init_telemetry(storage.base_dir().to_path_buf()).ok();
            commands::handle_config(&mut telemetry, &category, &action, start)?;
        }
        Commands::Security { all: _, format: _, summary: _ } => {
            // TODO: Implement security scanning
            eprintln!("Security scanning not yet implemented");
        }
        Commands::Stats => {
            commands::statistics::handle_stats(start)?;
        }
        Commands::History {
            limit: _,
            search: _,
            success_only: _,
            last: _,
        } => {
            // TODO: Implement history command
            eprintln!("History command not yet implemented");
        }
        Commands::Batch {
            query: _,
            add_tag: _,
            remove_tag: _,
            move_to: _,
            export: _,
            delete: _,
            clipboard,
            file,
            quiet,
        } => {
            let _io_options = IoOptions::new(None, None, clipboard, file.as_deref(), quiet)
                .with_category(CommandCategory::Utility);
            // TODO: Implement batch operations
            eprintln!("Batch operations not yet implemented");
        }
        Commands::Clean {
            text,
            save,
            append,
            clipboard,
            file,
            quiet,
        } => {
            let io_options = IoOptions::new(
                save.as_deref(),
                append.as_deref(),
                clipboard,
                file.as_deref(),
                quiet,
            ).with_category(CommandCategory::TextTransform);
            commands::clean::handle_clean(text.as_deref(), &io_options, start)?;
        }
        Commands::Diff {
            prompt1,
            prompt2,
            format,
            context,
            output,
        } => {
            commands::handle_diff(
                &storage,
                &prompt1,
                &prompt2,
                &format,
                context,
                output.as_deref(),
                start,
            )?;
        }
        Commands::Merge {
            source,
            target,
            backup,
            preview,
            interactive,
        } => {
            commands::handle_merge(
                &storage,
                &source,
                &target,
                backup,
                preview,
                interactive,
                start,
            )?;
        }
        Commands::Vars { action: _ } => {
            // TODO: Implement vars command
            eprintln!("Vars command not yet implemented");
        }
        Commands::Web {
            page,
            port,
            no_browser,
        } => {
            commands::web::handle_web(&storage, page.as_deref(), port, no_browser, start)?;
        }
        Commands::Perf { verify } => {
            commands::statistics::handle_perf(&storage, verify, start)?;
        }
        Commands::Health {
            format: _,
            simple: _,
            watch: _,
        } => {
            // TODO: Implement health command
            eprintln!("Health command not yet implemented");
        }
        Commands::Values {
            name: _,
            save,
            append,
            clipboard,
            file,
            quiet,
        } => {
            let _io_options = IoOptions::new(
                save.as_deref(),
                append.as_deref(),
                clipboard,
                file.as_deref(),
                quiet,
            );
            // TODO: Implement values command
            eprintln!("Values command not yet implemented");
        }
        Commands::Version { name, tag, message } => {
            commands::versioning::handle_version(&storage, &name, &tag, message.as_deref(), start)?;
        }
        Commands::Versions { name, verbose } => {
            commands::versioning::handle_versions(&storage, &name, verbose, start)?;
        }
        Commands::Rollback {
            name,
            version,
            backup,
        } => {
            commands::versioning::handle_rollback(&storage, &name, &version, backup, start)?;
        }
        Commands::Sync { action } => {
            commands::sync::handle_sync(&storage, &action, start).await?;
        }
        Commands::Teams { action } => {
            commands::teams::handle_teams(&storage, &action, start).await?;
        }
        Commands::Share {
            prompt,
            public,
            invite,
            allow_suggestions,
            expires,
        } => {
            commands::sharing::handle_share(
                &storage,
                &prompt,
                public,
                invite.as_deref(),
                allow_suggestions,
                expires,
                start,
            )
            .await?;
        }
        Commands::Suggestions { action } => {
            commands::sharing::handle_suggestions(&storage, &action, start).await?;
        }
        #[cfg(feature = "registry")]
        Commands::Improve { action } => {
            commands::improvement::handle_improvement_commands(&storage, &action, start).await?;
        }
        Commands::Bank { action } => {
            commands::banks::handle_banks(&storage, &action, start).await?;
        }
        Commands::Users { action } => {
            commands::users::handle_users(&storage, &action, start).await?;
        }
        Commands::Subscription { action } => {
            commands::subscription::handle_subscription(&storage, &action, start).await?;
        }
    }

    // Perform graceful shutdown if requested
    if shutdown_handler.is_shutdown_requested() {
        shutdown_handler.shutdown(Some(&storage))?;
    }

    // Log successful main command completion
    let duration = start.elapsed();
    log_command_execution("main", duration.as_millis() as u64, true, &Ok(()));

    Ok(())
}
