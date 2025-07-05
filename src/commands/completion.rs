// Shell completion generation commands - extracted from main.rs

use anyhow::Result;
use colored::*;
use std::time::Instant;

pub fn handle_completion(shell: &str, start: Instant) -> Result<()> {
    let completion_script = match shell.to_lowercase().as_str() {
        "bash" => generate_bash_completion(),
        "zsh" => generate_zsh_completion(),
        "fish" => generate_fish_completion(),
        _ => {
            eprintln!(
                "Error: Unsupported shell '{}'. Supported: bash, zsh, fish",
                shell
            );
            std::process::exit(1);
        }
    };

    println!("{}", completion_script);

    if !completion_script.is_empty() {
        println!(
            "\n# {} completion script generated ({}ms)",
            shell.green(),
            start.elapsed().as_millis()
        );
        println!("# Add this to your shell configuration file:");

        match shell.to_lowercase().as_str() {
            "bash" => {
                println!("# For bash, add to ~/.bashrc:");
                println!("# eval \"$(ph completion bash)\"");
            }
            "zsh" => {
                println!("# For zsh, add to ~/.zshrc:");
                println!("# eval \"$(ph completion zsh)\"");
            }
            "fish" => {
                println!("# For fish, save to ~/.config/fish/completions/ph.fish:");
                println!("# ph completion fish > ~/.config/fish/completions/ph.fish");
            }
            _ => {}
        }
    }

    Ok(())
}

fn generate_bash_completion() -> String {
    r#"#!/bin/bash

_ph_completion() {
    local cur prev opts
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"

    # Main commands
    opts="use new edit show delete ls find rename import compose diff merge batch clean vars web perf values version versions rollback history completion search install publish browse login logout tui share suggestions banks teams sync users help"

    # Subcommands and flags  
    case "${prev}" in
        ph)
            COMPREPLY=( $(compgen -W "${opts}" -- ${cur}) )
            return 0
            ;;
        use|show|edit|delete|rename)
            # Complete with prompt names (would need actual prompt list in real implementation)
            COMPREPLY=( $(compgen -W "$(ph ls 2>/dev/null | awk '{print $1}' 2>/dev/null || echo '')" -- ${cur}) )
            return 0
            ;;
        completion)
            COMPREPLY=( $(compgen -W "bash zsh fish" -- ${cur}) )
            return 0
            ;;
        web)
            COMPREPLY=( $(compgen -W "--port --no-browser" -- ${cur}) )
            return 0
            ;;
        tui)
            COMPREPLY=( $(compgen -W "$(ph ls 2>/dev/null | awk '{print $1}' 2>/dev/null || echo '')" -- ${cur}) )
            return 0
            ;;
        *)
            ;;
    esac

    COMPREPLY=( $(compgen -W "${opts}" -- ${cur}) )
}

complete -F _ph_completion ph
"#.to_string()
}

fn generate_zsh_completion() -> String {
    r#"#compdef ph

_ph() {
    local context curcontext="$curcontext" state line
    typeset -A opt_args

    _arguments -C \
        '1: :_ph_commands' \
        '*::arg:->args'

    case $state in
        args)
            case $line[1] in
                use|show|edit|delete|rename)
                    _arguments \
                        '*:prompt:_ph_prompts'
                    ;;
                completion)
                    _arguments \
                        '1:shell:(bash zsh fish)'
                    ;;
                web)
                    _arguments \
                        '--port[Port number]:port:' \
                        '--no-browser[Do not open browser]'
                    ;;
                tui)
                    _arguments \
                        '*:prompt:_ph_prompts'
                    ;;
            esac
            ;;
    esac
}

_ph_commands() {
    local commands
    commands=(
        'use:Use a prompt with input'
        'new:Create a new prompt'
        'edit:Edit an existing prompt'
        'show:Display prompt content'
        'delete:Delete a prompt'
        'ls:List all prompts'
        'find:Search prompts'
        'rename:Rename a prompt'
        'import:Import prompts from files'
        'compose:Compose multiple prompts'
        'diff:Show differences between prompts'
        'merge:Merge prompt changes'
        'batch:Run batch operations'
        'clean:Clean TUI artifacts from text'
        'vars:Manage template variables'
        'web:Start web dashboard'
        'perf:Performance verification'
        'values:Show prompt values'
        'version:Create prompt version'
        'versions:List prompt versions'
        'rollback:Rollback to previous version'
        'history:Show command history'
        'completion:Generate shell completion'
        'search:Search registry'
        'install:Install package'
        'publish:Publish prompt'
        'browse:Browse registry'
        'login:Authenticate with registry'
        'logout:Remove authentication'
        'tui:Interactive prompt selector'
        'share:Share prompts'
        'suggestions:Manage suggestions'
        'banks:Manage prompt banks'
        'teams:Team collaboration'
        'sync:Sync prompts'
        'users:User management'
        'help:Show help'
    )
    _describe 'commands' commands
}

_ph_prompts() {
    local prompts
    prompts=(${(f)"$(ph ls 2>/dev/null | awk '{print $1}' 2>/dev/null)"})
    _describe 'prompts' prompts
}

_ph "$@"
"#
    .to_string()
}

fn generate_fish_completion() -> String {
    r#"# Fish completion for ph command

# Main commands
complete -c ph -f -n "__fish_use_subcommand" -a "use" -d "Use a prompt with input"
complete -c ph -f -n "__fish_use_subcommand" -a "new" -d "Create a new prompt"
complete -c ph -f -n "__fish_use_subcommand" -a "edit" -d "Edit an existing prompt"
complete -c ph -f -n "__fish_use_subcommand" -a "show" -d "Display prompt content"
complete -c ph -f -n "__fish_use_subcommand" -a "delete" -d "Delete a prompt"
complete -c ph -f -n "__fish_use_subcommand" -a "ls" -d "List all prompts"
complete -c ph -f -n "__fish_use_subcommand" -a "find" -d "Search prompts"
complete -c ph -f -n "__fish_use_subcommand" -a "rename" -d "Rename a prompt"
complete -c ph -f -n "__fish_use_subcommand" -a "import" -d "Import prompts from files"
complete -c ph -f -n "__fish_use_subcommand" -a "compose" -d "Compose multiple prompts"
complete -c ph -f -n "__fish_use_subcommand" -a "diff" -d "Show differences between prompts"
complete -c ph -f -n "__fish_use_subcommand" -a "merge" -d "Merge prompt changes"
complete -c ph -f -n "__fish_use_subcommand" -a "batch" -d "Run batch operations"
complete -c ph -f -n "__fish_use_subcommand" -a "clean" -d "Clean TUI artifacts from text"
complete -c ph -f -n "__fish_use_subcommand" -a "vars" -d "Manage template variables"
complete -c ph -f -n "__fish_use_subcommand" -a "web" -d "Start web dashboard"
complete -c ph -f -n "__fish_use_subcommand" -a "perf" -d "Performance verification"
complete -c ph -f -n "__fish_use_subcommand" -a "values" -d "Show prompt values"
complete -c ph -f -n "__fish_use_subcommand" -a "version" -d "Create prompt version"
complete -c ph -f -n "__fish_use_subcommand" -a "versions" -d "List prompt versions"
complete -c ph -f -n "__fish_use_subcommand" -a "rollback" -d "Rollback to previous version"
complete -c ph -f -n "__fish_use_subcommand" -a "history" -d "Show command history"
complete -c ph -f -n "__fish_use_subcommand" -a "completion" -d "Generate shell completion"
complete -c ph -f -n "__fish_use_subcommand" -a "search" -d "Search registry"
complete -c ph -f -n "__fish_use_subcommand" -a "install" -d "Install package"
complete -c ph -f -n "__fish_use_subcommand" -a "publish" -d "Publish prompt"
complete -c ph -f -n "__fish_use_subcommand" -a "browse" -d "Browse registry"
complete -c ph -f -n "__fish_use_subcommand" -a "login" -d "Authenticate with registry"
complete -c ph -f -n "__fish_use_subcommand" -a "logout" -d "Remove authentication"
complete -c ph -f -n "__fish_use_subcommand" -a "tui" -d "Interactive prompt selector"
complete -c ph -f -n "__fish_use_subcommand" -a "share" -d "Share prompts"
complete -c ph -f -n "__fish_use_subcommand" -a "suggestions" -d "Manage suggestions"
complete -c ph -f -n "__fish_use_subcommand" -a "banks" -d "Manage prompt banks"
complete -c ph -f -n "__fish_use_subcommand" -a "teams" -d "Team collaboration"
complete -c ph -f -n "__fish_use_subcommand" -a "sync" -d "Sync prompts"
complete -c ph -f -n "__fish_use_subcommand" -a "users" -d "User management"
complete -c ph -f -n "__fish_use_subcommand" -a "help" -d "Show help"

# Completion shell options
complete -c ph -f -n "__fish_seen_subcommand_from completion" -a "bash zsh fish" -d "Shell type"

# Web command options
complete -c ph -f -n "__fish_seen_subcommand_from web" -l "port" -d "Port number"
complete -c ph -f -n "__fish_seen_subcommand_from web" -l "no-browser" -d "Do not open browser"

# Prompt name completions (dynamic - would need actual prompt list)
function __fish_ph_prompts
    ph ls 2>/dev/null | awk '{print $1}' 2>/dev/null
end

complete -c ph -f -n "__fish_seen_subcommand_from use show edit delete rename tui" -a "(__fish_ph_prompts)" -d "Prompt name"
"#.to_string()
}
