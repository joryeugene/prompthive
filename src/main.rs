use clap::Parser;
fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() == 2 && args[1] == "-v" {
        println!("ph {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if let Some(command) = args.get(1).and_then(|v| v.to_str()) {
        if [
            "login",
            "logout",
            "sync",
            "teams",
            "users",
            "subscription",
            "share",
            "suggestions",
            "improve",
            "publish",
            "unpublish",
            "install",
            "search",
            "browse",
            "web",
            "health",
            "security",
            "history",
            "batch",
            "vars",
            "values",
            "v",
            "bank",
            "banks",
            "stats",
            "perf",
            "config",
        ]
        .contains(&command)
        {
            eprintln!("error: {command} was removed from the local-first CLI. Your existing files are preserved. Use ph f QUERY, ph u NAME, or ph export DIR.");
            std::process::exit(2);
        }
    }
    let cli = prompthive::Cli::parse();
    if let Err(error) = prompthive::commands::core::run(cli.command) {
        let code = prompthive::commands::common::error_code(&error);
        if code != 0 {
            eprintln!("error: {error:#}");
        }
        std::process::exit(code);
    }
}
