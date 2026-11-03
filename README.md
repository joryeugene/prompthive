# Prompthive

A local-first prompt toolkit for your terminal. Find, compose, and reuse prompts with fewer keystrokes.

`ph` keeps your prompts and notes in Markdown files. Find one, add context, and copy it or pipe the text to another tool. Prompthive does not run models or require an account.

## Install this candidate

This checkout contains the **unreleased 0.3.0 comeback candidate**. Installing `prompthive` from crates.io still selects a published release, which has different behavior.

With a Rust toolchain installed, run from this checkout:

```sh
cargo install --path . --locked
ph --version
```

The default build includes the terminal picker. For a smaller build without it, add `--no-default-features`; exact-name commands and pipelines remain available.

On Linux, clipboard copying needs `wl-copy` on Wayland, or `xclip` or `xsel` on X11, plus a working desktop session. Prompthive starts no clipboard daemon of its own. Use a pipe or `-q` without a clipboard. An explicit copy request fails clearly when copying is unavailable.

## Five actions to remember

Start with `ph` in a terminal. Type to search, inspect the full name and preview, press Enter to use, or Escape to cancel. No selection is copied before you accept it. With no saved prompts, the picker offers a creation example.

| Action | Short | Long | Example |
| --- | --- | --- | --- |
| Use a prompt | `u` | `use` | `ph u review` |
| Find prompts | `f` | `find` | `ph f review` |
| Save new text | `n` | `new` | `ph n review 'Review this: {input}'` |
| Edit saved text | `e` | `edit` | `ph e review` |
| Compose prompts | `c` | `compose` | `git diff \| ph c work/rules,review` |

Create the names in these examples first. `ph` also includes local starter prompts when creating a fresh store. An existing empty store stays empty.

```sh
ph n review 'Review this code. Explain concrete defects: {input}'
git diff | ph u review | less
printf '%s\n' 'Notes from the design meeting' | ph n notes/meeting
ph e notes/meeting
```

Names are case-sensitive. `work/review` and `personal/review` are different prompts. Use exact, complete names in scripts. A prefix or fuzzy query requires a visible preview and confirmation; in a pipe it fails with candidate names on stderr. Adding another prompt never silently changes the meaning of an exact name. Retained short codes require a verified compatibility map; they are optional, and a real canonical name always wins.

Other actions are `s`/`show`, `l`/`ls`, `r`/`rename`, `d`/`delete`, `t`/`tui`, and `x`/`clean`. Run `ph --help` or `ph ACTION --help` for options. The action `s` shows original text; the flag `-s NAME` saves output:

```sh
ph s review                         # Original text, including {input}
ph s review -s notes/review-template # Save the original body under a new name
git diff | ph u review -s notes/run  # Save rendered text under a new name
```

Generate shell completions with `ph completion bash`, `ph completion zsh`, or `ph completion fish`, and load the output through your shell's completion setup. Saved-name suggestions read the current library, so new prompts need no script regeneration.

## Terminal, pipes, and destinations

In a terminal, use renders the selected prompt and attempts to copy it. If automatic copying is unavailable, it shows the text and explains the clipboard problem. `-q` prints only raw text and disables automatic copying.

In a pipeline, stdout contains only the exact payload, with no status banners or invented trailing newline. Errors go to stderr and return a nonzero exit code. Bare `ph` with piped context and terminal output can use the controlling terminal for selection. With both streams piped, choose a name explicitly. Bare `ph` with empty input and nonterminal output prints compact help.

Choose at most one explicit destination. It overrides automatic pipe output and leaves stdout empty. On `n`, an explicit destination replaces the default save, so a failed file or copy operation cannot leave an extra prompt behind:

```sh
git diff | ph u review -f review.txt # New file; existing paths need --force
git diff | ph u review -s notes/run # New saved prompt; refuses an existing name
git diff | ph u review -a notes/run # Append with two newlines; keep target metadata
git diff | ph u review -c           # Explicit clipboard copy
git diff | ph c work/rules,review --clipboard
```

`-c` means copy on use/show/new/clean and the retained list/find destinations. Compose uses `--clipboard`; diff keeps `-c` for context and `-f` for format. `-q` never suppresses errors. Exit codes distinguish validation or lookup errors (`2`), an editor conflict (`3`), explicit clipboard failure (`4`), and cancellation (`130`).

## Templates and composition

`{input}`, `{INPUT}`, `{content}`, and `{CONTENT}` insert context from stdin or one positional input. Supplying both is an error. Input whitespace is preserved, and inserted text is literal: braces, environment placeholders, and shell-looking text inside a value are never executed or expanded again.

Declare named variables in optional YAML frontmatter. For example, save this as `banks/work/explain.md` inside your store:

```markdown
---
id: work/explain
description: Explain a change
vars: {lang: null, tone: brief}
---

Explain this {lang} change in a {tone} tone: {input}
```

```sh
git diff | ph u work/explain -v lang=Rust -v tone=direct
```

`null` requires a value; a string supplies a default. Unknown, repeated, or missing values fail before output. Undeclared braces remain ordinary text, and `{{lang}}` produces literal `{lang}`. Explicit `{env:NAME}` reads that environment variable or fails if it is absent. Prompthive does not load `.env` files. Date/time context is captured once per invocation: `time` and `datetime` use local time; `date` and `iso_date` use UTC, and `timestamp` is Unix seconds.

Composition is a left-to-right template chain. If `a` contains `A {input}` and `b` contains `B {input}`, `ph c a,b context` produces `B A context`. A stage without an input slot appends incoming text after two newlines. Every stage and required variable is checked before any destination changes. Explicit `-v` values apply to every stage declaring that name; other defaults remain local to each stage.

Composition files start with `./`, `../`, `/`, or `~`, such as `ph c ./rules.md,review context`. A dotted name such as `review.v1` is still a saved prompt name. Sources are local files, never URLs.

## Storage and recovery

The default store is `~/.prompthive`. Set `PROMPTHIVE_BASE_DIR` to choose a different store. Ordinary prompts live in `prompts/`, bank-qualified names in `banks/`, and retained team-path notes in `teams/`. There is no hidden project-store merge. Use `ph e NAME` with your chosen `$EDITOR`, or edit these Markdown files directly; the next invocation reads the change. Unknown frontmatter is retained during supported edits.

Mutations keep local preimages in `.versions/` and do not prune them automatically. Writes use a process lock and atomic replacement. A failed required snapshot aborts the change. Editors work on a staged copy; if the source changed meanwhile, Prompthive reports a conflict and keeps recovery material. External editors do not share Prompthive's lock, so keep your own backups too. Power-loss durability depends on the operating system and filesystem; atomic replacement alone is not a universal guarantee.

```sh
ph version notes/meeting before-edit -m 'Before the rewrite'
ph versions notes/meeting -v
ph rollback notes/meeting before-edit
ph export ./prompt-backup
ph import ./prompt-backup --dry-run
ph import ./prompt-backup
```

Export creates a new directory with raw Markdown, a checksummed manifest, and verified compatibility bindings. It excludes credentials, runtime configuration, and history. Import skips existing-file conflicts by default. `--force` backs up and replaces a conflicting prompt; `--version` keeps both under different names. A deleted name with recovery history remains reserved: use `--force` to restore it, `--version` for a new name, or `--skip` to leave it alone. A skipped or renamed prompt cannot activate an imported binding against unrelated existing content. A portable export is useful for transferring prompts; back up the full store separately to retain recovery history.

## Moving from 0.2.8

Existing prompt and note files remain readable. Legacy credentials, cloud metadata, remote objects, and unknown retained files are left untouched. Old custom-variable configuration paths are not searched or guessed; add template `vars` declarations manually. The local CLI no longer offers accounts, sync, remote registries, publishing, teams administration, subscriptions, or telemetry. Placeholder dashboard commands are removed too. Rendering does not contact a service or rewrite your library.

Unverified generated shorthand no longer silently selects the highest-scoring match. Use an exact name or the picker. There is no bare `ph NAME` fallback: use `ph u NAME`. Version flags remain `ph -v`, `ph -V`, and `ph --version`; template variables use `-v NAME=value` after use or compose.

See [CONTRIBUTING.md](CONTRIBUTING.md) for development and regression checks. Prompthive is [MIT licensed](LICENSE).
