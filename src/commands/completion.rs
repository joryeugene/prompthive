use anyhow::{bail, Context, Result};
use clap::CommandFactory;
use clap_complete::{generate, Shell};
use std::io::Write;

pub fn handle_completion(shell: &str) -> Result<()> {
    let shell = match shell {
        "bash" => Shell::Bash,
        "zsh" => Shell::Zsh,
        "fish" => Shell::Fish,
        _ => bail!("unsupported shell; use bash, zsh, or fish"),
    };
    let mut generated = Vec::new();
    generate(shell, &mut crate::Cli::command(), "ph", &mut generated);
    let generated = String::from_utf8(generated).context("completion script is not UTF-8")?;
    // Preserve clap's command/flag grammar; query the live library only at selector positions.
    let (generated, dynamic) = match shell {
        Shell::Bash => (generated.replace("_ph", "_ph_generated"), BASH_NAMES),
        Shell::Zsh => (generated.replace("_ph", "_ph_generated"), ZSH_NAMES),
        Shell::Fish => (generated, FISH_NAMES),
        _ => unreachable!("validated above"),
    };
    let mut output = std::io::stdout().lock();
    output.write_all(generated.as_bytes())?;
    output.write_all(dynamic.as_bytes())?;
    Ok(())
}

const BASH_NAMES: &str = r#"
# Saved names are read at completion time, so later additions need no regeneration.
_ph_selector_position() {
    local action=${COMP_WORDS[1]-} word expecting='' positional=0 i
    for ((i=2; i<COMP_CWORD; i++)); do
        word=${COMP_WORDS[i]}
        if [[ -n $expecting ]]; then expecting=''; continue; fi
        case "$word" in
            -s|-a|-f|--with|--format|--context|--output|-m|--message) expecting=$word ;;
            -v) case "$action" in u|use|c|compose|chain) expecting=$word ;; esac ;;
            -c) [[ $action == diff ]] && expecting=$word ;;
            -*) ;;
            *) ((positional+=1)) ;;
        esac
    done
    if [[ -n $expecting ]]; then
        case "$action:$expecting" in
            u:-a|use:-a|s:-a|show:-a|n:-a|new:-a|c:-a|compose:-a|chain:-a|x:-a|clean:-a|l:-a|ls:-a|list:-a|f:-a|find:-a) printf name; return 0 ;;
            *) return 1 ;;
        esac
    fi
    case "$action" in
        u|use|s|show|e|edit|d|delete|rm|r|rename|mv|f|find|t|tui|version|versions|rollback)
            ((positional == 0)) || return 1; printf name ;;
        c|compose|chain) ((positional == 0)) || return 1; printf compose ;;
        diff|merge) ((positional < 2)) || return 1; printf name ;;
        *) return 1 ;;
    esac
}
_ph() {
    local current=${COMP_WORDS[COMP_CWORD]-} mode prefix='' fragment candidate
    if [[ $current != -* ]] && mode=$(_ph_selector_position); then
        fragment=$current
        if [[ $mode == compose && $current == *,* ]]; then
            prefix=${current%,*},
            fragment=${current##*,}
        fi
        COMPREPLY=()
        while IFS= read -r candidate; do
            [[ $candidate == "$fragment"* ]] && COMPREPLY+=("$prefix$candidate")
        done < <(command ph l 2>/dev/null)
        compopt +o default +o bashdefault 2>/dev/null || :
        return 0
    fi
    _ph_generated "$@"
}
complete -o bashdefault -o default -F _ph ph
"#;

const ZSH_NAMES: &str = r#"
_ph_selector_position() {
    local action=${words[2]-} word expecting=''
    local -i positional=0 i
    for ((i=3; i<CURRENT; i++)); do
        word=${words[i]}
        if [[ -n $expecting ]]; then expecting=''; continue; fi
        case "$word" in
            -s|-a|-f|--with|--format|--context|--output|-m|--message) expecting=$word ;;
            -v) case "$action" in u|use|c|compose|chain) expecting=$word ;; esac ;;
            -c) [[ $action == diff ]] && expecting=$word ;;
            -*) ;;
            *) ((positional+=1)) ;;
        esac
    done
    if [[ -n $expecting ]]; then
        case "$action:$expecting" in
            u:-a|use:-a|s:-a|show:-a|n:-a|new:-a|c:-a|compose:-a|chain:-a|x:-a|clean:-a|l:-a|ls:-a|list:-a|f:-a|find:-a) print -r -- name; return 0 ;;
            *) return 1 ;;
        esac
    fi
    case "$action" in
        u|use|s|show|e|edit|d|delete|rm|r|rename|mv|f|find|t|tui|version|versions|rollback)
            ((positional == 0)) || return 1; print -r -- name ;;
        c|compose|chain) ((positional == 0)) || return 1; print -r -- compose ;;
        diff|merge) ((positional < 2)) || return 1; print -r -- name ;;
        *) return 1 ;;
    esac
}
_ph() {
    local mode
    local -a names
    if [[ ${words[CURRENT]-} != -* ]] && mode=$(_ph_selector_position); then
        names=("${(@f)$(command ph l 2>/dev/null)}")
        [[ $mode == compose ]] && compset -P '*,'
        compadd -a names
        return
    fi
    _ph_generated "$@"
}
compdef _ph ph
"#;

const FISH_NAMES: &str = r#"
function __ph_selector_position
    set -l words (commandline -opc)
    test (count $words) -ge 2; or return 1
    set -l current (commandline -ct)
    if test (count $current) -gt 0
        string match -q -- '-*' "$current"; and return 1
    end
    set -l action $words[2]
    set -l expecting ''
    set -l positional 0
    set -l arguments $words
    set -e arguments[1..2]
    for word in $arguments
        if test -n "$expecting"
            set expecting ''
            continue
        end
        switch $word
            case -s -a -f --with --format --context --output -m --message
                set expecting $word
            case -v
                contains -- $action u use c compose chain; and set expecting $word
            case -c
                test "$action" = diff; and set expecting $word
            case '-*'
            case '*'
                set positional (math $positional + 1)
        end
    end
    if test -n "$expecting"
        contains -- $action u use s show n new c compose chain x clean l ls list f find; and test "$expecting" = -a; and return 0
        return 1
    end
    switch $action
        case u use s show e edit d delete rm r rename mv f find t tui version versions rollback
            test $positional -eq 0
        case c compose chain
            test $positional -eq 0
        case diff merge
            test $positional -lt 2
        case '*'
            return 1
    end
end
function __ph_prompt_names
    set -l words (commandline -opc)
    set -l current (commandline -ct)
    set -l prefix ''
    if contains -- $words[2] c compose chain; and test (count $current) -gt 0
        if string match -q -- '*,*' "$current"
            set prefix (string replace -r '[^,]*$' '' -- "$current")
        end
    end
    for name in (command ph l 2>/dev/null)
        printf '%s\n' "$prefix$name"
    end
end
complete -c ph -f -n '__ph_selector_position' -a '(__ph_prompt_names)'
"#;
