# Local examples

These examples target the unreleased 0.3.0 candidate. Build the checkout first, then run:

```sh
cargo build --locked
PROMPTHIVE_BIN="$PWD/target/debug/ph" bash examples/creative_patterns.sh
```

If `ph` already points to the candidate, run `bash examples/creative_patterns.sh` directly. A build using `--no-default-features` also supports this demo.

The script creates its own prompt store and configuration directory with `mktemp`, overrides `PROMPTHIVE_BASE_DIR` and `XDG_CONFIG_HOME` for its child commands, and removes only that directory on exit. It does not change `HOME`, use your existing prompts, access the clipboard, contact a service, or run an AI tool. It requires Bash and common local utilities (`mktemp`, `cmp`, `mkdir`, and `rm`).

The examples demonstrate:

1. Finding exact, bank-qualified names.
2. Rendering a prompt with literal context.
3. Chaining templates from left to right.
4. Reading notes, saving a copy, and renaming it.
5. Declaring required variables and defaults in Markdown.
6. Restoring a local version without leaking frontmatter into the body.
7. Exporting, previewing import, and checking that restored note bytes match.

Every rendered example uses `-q`, so it works in a headless terminal without copying anything. The demonstration library is discarded; the examples do not install templates into your own store. For an interactive picker, use `ph` with your chosen library after reviewing the [main guide](../README.md).
