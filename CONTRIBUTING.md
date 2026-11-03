# Contributing to Prompthive

Prompthive is a local Markdown prompt library. Changes should make finding, editing, rendering, and reusing saved text simpler and more dependable. Keep the single-letter actions, exact identities, and clean pipeline output. Discuss changes to storage or command grammar before implementing them.

## Work locally

Use a Rust toolchain and a disposable prompt store. Do not test mutations against your own library or clipboard.

```sh
cargo build --locked
cargo test --locked
cargo test --locked --no-default-features
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
```

The default feature set includes the terminal picker. The build without default features must retain exact-name rendering, composition, notes, file output, import/export, and recovery commands. Linux clipboard tests require a separate desktop-session check with `wl-copy`, `xclip`, or `xsel`; headless tests must never use a real clipboard.

For a manual sandbox:

```sh
PROMPTHIVE_BASE_DIR="$(mktemp -d)" ph n example 'Explain: {input}'
```

That variable applies to one command. Reuse the same explicit directory for later commands if you want to keep the fixture. Do not redefine `HOME` to isolate tests. The integration suite creates disposable stores and configuration directories for every case.

## Test observable behavior

Add a regression that fails for the actual defect and checks the result a caller sees. Favor exact output bytes, exit codes, preserved files, and recoverable preimages over tests that repeat implementation details. Validate a complete render before stdout, a file, a saved prompt, or a clipboard can change.

Run the CLI contracts under both feature configurations:

```sh
cargo test --locked --test cli_contract
cargo test --locked --no-default-features --test cli_contract
```

The suite covers command aliases, exact resolution, literal templates, variables, composition, headless behavior, destinations, legacy notes, version recovery, portable import/export conflicts, and path containment. Unix-specific cases cover symlinks, staged editor failures and conflicts, and a downstream closed pipe. Unit tests provide the lower-level persistence and rendering checks.

Run real Bash pipelines against an installed executable with one command:

```sh
bash scripts/test-pipes "$(command -v ph)"
```

Or give the exact build path, for example `bash scripts/test-pipes ./target/debug/ph`. Add `--headless` only when testing an executable built without the terminal picker. `--report result.json` saves the executable hash, commands, output bytes, exit codes, and explicit platform exceptions. Every case has a timeout and uses disposable prompt/configuration directories; the real store and clipboard are never used. The contract covers Unicode and whitespace, use/composition, defaults and supplied values, literal inserted text, save/redirect destinations, validation failures, non-TTY behavior, a closed downstream pipe, and Unix piped-context cancellation with terminal restoration.

`bash scripts/test-installed` packages the locked source, extracts that actual Cargo archive, installs default and headless executables into temporary roots, checks their versions, and runs the same pipe contract. It respects `CARGO_TARGET_DIR` for packaging and keeps install builds in its `installed-contract` subdirectory. Temporary installed files are removed after verification; `--report installed.json` retains package and executable hashes with the observations. A clean checkout is required by Cargo packaging. Bash, Python 3, and the Rust toolchain are test dependencies, not product runtime dependencies. Native Windows PTY cancellation and physical desktop clipboard behavior need separate evidence.

Automated headless checks do not prove picker readability, keyboard behavior, terminal restoration, a real clipboard paste, or durability across a power interruption. Record those checks separately on the platform being claimed. Compare the five journeys in [the comeback specification](docs/comeback-spec.md) with the same fixtures and terminal state. Count all selection and confirmation keys before making speed claims.

## Preserve the library

`bash scripts/test-interactive /absolute/path/to/ph` exercises guided fields, exact frozen previews, literal multiline paste, defaults and explicit empty values, resized/tiny terminals, searchable editing, and new-note editor success/cancellation/failure in disposable POSIX PTYs. It saves no variable answers and removes desktop display variables. Use `--report FILE --capture-dir DIR` for the exact executable hash, action trace, terminal observations, and decoded screen text. The default installed-package gate runs this same suite; native Windows terminal behavior requires separate acceptance.

Keep old Markdown bodies and unfamiliar frontmatter readable. Back up a mutation before replacing, deleting, or renaming content; a failed required backup must abort the change. Reject paths or symlinks that escape the selected store. Test interrupted writes, permissions, concurrent edits, malformed files, Unicode, and ambiguous names.

Import and export are local file operations. Default conflicts must preserve existing content, and imported compatibility bindings must never point at different content because a prompt was skipped or renamed. Do not add account requirements, silent network requests, telemetry, automatic backup pruning, or migration that deletes credentials or user data.

## Submit a change

Describe the problem, the resulting behavior, and the checks you ran. Include a small before/after command example when output or grammar changes. State any platform or manual checks that remain open. Keep unrelated cleanup separate, and update help, completions, and the README when commands change.

For a bug report, include the operating system, `ph --version`, an isolated reproduction, expected output, and actual output. Use synthetic prompts and remove credentials or private text before sharing.

## Release preparation

Run `cargo fetch --locked`, then `bash scripts/check` for the portable offline suite. It exercises both feature builds through real Bash pipes. Normal release preparation is `bash scripts/release --check`. It invokes `.workflow/check-release --stage local` before Cargo release work; missing or stale workspace acceptance refuses the operation, then the checked source is packaged, installed, and tested in both feature modes. See [.workflow/README.md](.workflow/README.md) for the private evidence location and the safe CI sentinel. CI also runs `bash scripts/test-installed`. Publishing requires reviewed scope approval and the same gate. No release is published by the check mode.

Native macOS terminal tests cancel bare/default-copy journeys before changing a user clipboard. The fresh GitHub-hosted macOS VM explicitly opts into disposable clipboard acceptance with `PROMPTHIVE_QA_DISPOSABLE_CLIPBOARD=1`; the observer also requires GitHub-hosted runner identity and verifies the copied text with `pbpaste`. This is not physical desktop clipboard coverage.
