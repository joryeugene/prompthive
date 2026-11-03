# Prompthive local comeback specification

Draft for Jory's scope review. Proposed release line: **0.3.0**, because removing remote commands and making ambiguous shorthand fail changes compatibility. No release is approved by this draft.

Prompthive should make a saved prompt available in as few keys as possible, from any terminal, for any AI tool. The product is a local Markdown library with familiar single-letter actions, a useful search-and-preview start screen, template rendering, and reliable Unix output. It does not run models or depend on a service.

## The decision

Keep developing the personal utility. Judge its value by whether Jory retrieves, adapts, and reuses his own prompts more easily than a folder plus shell history. A marketplace or hosted platform is unnecessary to that outcome. Ship the dependable local core before adding integrations.

The first comeback release keeps `ph`, its existing single-letter verbs, Markdown prompts, import, editing, composition, version snapshots, clipboard use, and piping. Bare `ph` helps people find and use their saved text. It removes cloud sync, remote registry, accounts, teams, publishing, subscriptions, network telemetry, and their dashboards. It adds no required prompt-alias vocabulary or bare prompt-selector grammar. Existing files and remote data are preserved.

## Website and unarchive readiness

The original website URL is **https://prompthive.sh/**, with the second p before thive. The GitHub homepage, Cargo homepage/documentation fields, and retained README agree. On 2 November 2026, Google and Cloudflare DNS both returned NXDOMAIN, so this is a verified historical URL, not a currently working website. The available repository is https://github.com/joryeugene/prompthive. Domain registration or DNS recovery is separate work; this check does not establish why resolution stopped or who currently controls the domain.

The same check found repository ID 1013846131 still archived, not disabled, with admin access and zero GitHub Actions workflows. Its main commit remains 1ab5930bb23e9b6c67c7f05fff7e54184a54b559. **Unarchiving can proceed independently now under the user's existing revival request.** No code, prompt data, credentials, tags, assets, or releases need to change for that action. The parent can change only archived to false and read back the exact repository identity and state. No unarchive mutation has been performed in this specification work. Unarchiving does not repair the website or make the old CLI release ready; the implementation and release gates below remain open.

## What works today and where shorthand breaks

The audited public source is `joryeugene/prompthive`, baseline `1ab5930bb23e9b6c67c7f05fff7e54184a54b559`, version 0.2.8. It is a public package snapshot, including a Cargo-normalized manifest whose original VCS commit is absent from public history. No older checkout or accessible separate authoring repository was found on this machine. The historical archival reason remains unknown.

Current verb aliases include `u` use, `s` show, `n` new, `e` edit, `d`/`rm` delete, `l`/`list` ls, `f` find, `c`/`chain` compose, `x` clean, `r`/`mv` rename, and `t` tui. Bare `ph er` is currently an unknown command, not default prompt use.

Prompt resolution is split. `show`, edit, and rename use `Matcher`; use and composition first use storage's highest-scoring fuzzy result. Generated codes are rebuilt from the sorted prompt list, split names on `-` and `_`, and add extra letters or numbers for collisions. They are not persistent identities.

Fresh synthetic CLI observations:

| Input | Observed 0.2.8 behavior |
| --- | --- |
| `ph s es` | Shows the starter debug prompt. |
| `ph u es` | Renders the starter commit prompt. |
| `ph u au sample` with auth-basic and auth-bearer | Silently picks auth-basic. |
| `ph s au` with the same prompts | Fails with multiple matches. |
| `ph s e` before and after adding email/foo | Changes from commit to email/foo. |

Preserve the single-letter verbs and intentional existing bindings. The rejected alias-first draft treated prompt mnemonics as new command names; this revision keeps the action and prompt name separate. Migration must expose command-dependent legacy codes before preserving any compatibility binding. Silent redirection is not a feature.

## Proposed command model

This is a revised proposal for scope review, not implemented or finalized grammar. Keep one small rule: **ph, then an action, then a real prompt name.** Bare `ph` is the helpful start when the name is not remembered. A prompt name is ordinary saved text identity, not another command to memorize.

| Main action | Existing short and long forms | Example |
| --- | --- | --- |
| Use or copy a rendered prompt | `u`, `use` | `git diff | ph u essentials/review` |
| Find saved prompts | `f`, `find` | `ph f review` |
| Save a new prompt | `n`, `new` | `ph n review "Review this: {input}"` |
| Edit a prompt | `e`, `edit` | `ph e review` |
| Combine templates | `c`, `compose` | `git diff | ph c work/rules,review` |

The final two examples assume actual local prompts named review and work/rules, not aliases. Creating review with n makes `ph u review` an exact name. Existing starter IDs such as essentials/review remain unchanged. People need not type or remember those IDs to use the picker; it displays them and supplies the exact command for later reuse.

Keep secondary existing actions under `ph --help`: `s` show the original text, `l` list, `r` rename, `d` delete, `t` open the local picker, and `x` clean. Keep the existing import, diff, merge, version, versions, and rollback names; proposed local export remains a rare descriptive command. Do not force every rare operation onto a new single-letter abbreviation. For notes, use n to save plain text, s to read it, and e to edit it; no new notes application or separate database is added.

### Conflicts reviewed before changing grammar

| Existing token | Meaning that must survive | Conflict to avoid |
| --- | --- | --- |
| `u` | Use | Not a prompt shortcut at command position. |
| `f` | Find | On use/show/new/compose, -f is a file destination; diff -f still means format. |
| `n` | New/save a prompt | No new competing save command is needed. |
| `e` | Edit | Generated prompt code e must not replace the edit command. |
| `c` | Compose | c stays compose. Existing -c copies on use/show/new; compose keeps --clipboard; diff -c means context. |
| `s` | Show original text | `-s NAME` saves output; never silently rename s to save. |
| `r` and `d` | Rename and delete | Not review and debug. |
| `t` and `x` | TUI and clean | Preserve working existing uses. |
| `v`, `-v`, `-V` | v is a retired values command; top-level version flags remain | Do not reuse v as a new verb; contextual variable/history flags stay explicit. |

Preserve `ph -v`, `ph -V`, and `ph --version`. Template `-v NAME=value` is contextual to use/compose, while `ph versions NAME -v` remains verbose history. There is no bare `ph SELECTOR` fallback and no new alias-management command in this release proposal. An unknown command returns a concise command correction and help; it cannot accidentally render a prompt. Removed cloud command names return a local migration explanation, never a network request. An exact prompt named sync or e is still reachable after an explicit u, such as `ph u sync`.

### One resolver, explicit selection

Use one resolver across use/show/compose/edit/rename/delete:

1. Exact, case-sensitive canonical IDs resolve without a picker. Full bank-qualified IDs remain available everywhere.
2. Retain an existing short code only through a verified compatibility binding whose old target agrees across the applicable verbs. No new code is generated from sort order. Exact canonical IDs always take precedence: if preserved e points to essentials/commit but a real root prompt e exists, disable and report that compatibility binding, while `ph u e` remains the exact root prompt. Never make a real canonical ID unreachable. Compatibility codes are not an automation contract; scripts use canonical IDs. Migration previews these collisions, creation checks them before writing, and externally introduced collisions produce a clear binding-disabled notice without rewriting stored metadata during a render.
3. A query or prefix is discovery. On an interactive read/use path it opens a preview picker seeded with the query; even a single fuzzy candidate requires Enter. In a script it fails with candidate IDs on stderr and no stdout. It never silently picks the highest score.
4. Mutation uses an exact target. Discovery may help a person select that full target, but an unconfirmed fuzzy result cannot edit, rename, or delete it. Destructive selection shows the full ID and intended recovery action before confirmation; create the snapshot only after confirmation and before mutation.
5. Existing command-dependent codes such as es are migration conflicts, not silently chosen new bindings. A migration preview records use/show/compose targets and shows full canonical alternatives. No ordinary render automatically rewrites prompts or compatibility metadata.

Preserving a compatibility code is optional and does not create a new primary interface. Users can always use an exact name or bare ph. Unverified old fuzzy shortcuts intentionally fail in 0.3 rather than change targets. No mandatory alias setup, mnemonic table, popularity ranking, model-generated command, or hidden project override is introduced.

### Bare ph should answer what can I do here

When a terminal is available, bare `ph` opens the local prompt picker with search focused. The first screen shows useful saved prompts immediately, their full canonical names, a readable preview, and one short command hint row. It does not start with accounts, sync status, statistics, onboarding questions, or a wall of commands. A fresh store can use the bundled local starter prompts; an existing empty store is not silently reseeded after deliberate deletion.

A proposed ordinary 80-column view is:

```text
Prompthive
Find a prompt: review|

> essentials/review   Review code
  work/review         Review a change request

Preview: essentials/review
Review the provided code focusing on critical issues ...

Type to find  Up/Down select  Enter use  Esc cancel
CLI: u use  f find  n new  e edit  c compose
```

The vertical bar above depicts the input cursor, not a key the user types. The CLI row is a reminder of shell commands after leaving the picker, not live bare-letter shortcuts: typing e or n in the search field always inserts that letter. Only the explicitly shown navigation/accept/cancel keys act on the picker. Selection and preview always refer to the same full ID. No result is copied just because it became the first match. Enter renders the selected template and performs the normal use output action. Escape cancels without changing clipboard, files, or stored prompts. This replaces the initial bank-navigation burden; optional advanced browsing must not obscure the simple path.

If there are no saved prompts, show a specific empty state with one copyable creation example and the available n/new action. If a query has no matches, keep the query visible, show no matches, and let the user clear it or cancel. Show permission or malformed-file errors as actual failures with a path and next step, not as an apparently empty library. Ordinary Markdown notes remain discoverable without a separate mode.

Interactive discovery requires terminal stdout and a usable keyboard terminal. Nonterminal stdout always uses script behavior, even if the process also has a controlling terminal.

| Invocation state | Bare ph behavior |
| --- | --- |
| Terminal input and output | Open the focused search picker. |
| Piped context, terminal output, controlling terminal available | Open the same picker using the controlling terminal for keys; preserve stdin exclusively as selected prompt context. |
| No nonempty input and nonterminal output, including CI stdin at EOF | Print compact plain help, exit 0, and do not touch clipboard. This is help output, not rendered prompt payload. |
| Nonempty piped input and nonterminal output | Fail with exit 2, empty stdout, and an explicit `ph u NAME` or `ph c A,B` example on stderr. Never hang on an invisible picker. |
| Picker needed but no keyboard terminal | Explain how to use an exact name, fail clearly, and leave all destinations unchanged. |

`ph u` without a name opens that same use picker when its interaction can be visible; in a script it is a missing-name error. `ph u QUERY` pre-fills discovery when no exact target exists. No separate picker language has to be learned. `ph t QUERY` must honor its existing search argument instead of discarding it.

### Keystrokes and acceptance journeys

This revision reduces the concepts people must memorize before claiming minimum physical keys. Explicit paths keep the same length as before. Bare ph already exists in 0.2.8; the proposed improvement is focused discovery and clear feedback, not a new two-character command. Remove the previous alias-setup savings claim.

| Command text | ASCII characters before Enter | Interpretation |
| --- | --- | --- |
| `ph` | 2 | Search and choose; query/selection keys must also be counted. |
| `ph u` | 4 | Explicit use picker where interactive. |
| `ph f review` | 11 | Readable query, no mnemonic prompt alias. |
| `ph u review` | 11 | Exact prompt review must actually exist. |
| `ph e review` | 11 | Same name, different existing action. |
| `ph u essentials/review` | 22 | Existing explicit starter path, unchanged. |

Replay these five complete journeys against 0.2.8 and the candidate with the same synthetic library and terminal starting state:

1. Known prompt: invoke by exact name, verify clipboard bytes, and paste into an inert sink; separately use bare ph discovery and count every selection key.
2. Forgotten prompt: type ph, search, inspect matching full ID/preview, choose, and copy. Include no result, equal leaf names in different banks, first-use, empty store, cancel, resize, and return to the terminal.
3. Two-variable template: provide two declared values and a missing-value case. Verify spaces, equals signs, braces, Unicode, and command-looking data remain literal; no partial output is allowed on validation failure.
4. Two templates plus context: pipe context into `ph c A,B`, verify specified nesting and exact whitespace, and pass bytes to an inert consumer. Also test `git diff | ph` with visible terminal selection and both stdin/stdout piped with no selector.
5. Edit and reuse: use e, save through the editor, then use the same exact name; verify the next invocation sees the edit without a daemon. Cover cancellation, failure, and concurrent external edits.

Record physical key-down events on a declared keyboard layout, Enter/Tab/arrows/cancel/backtracking, logical actions, correctness, and elapsed time. Include setup only when actually required; this design needs no alias setup. Set speed budgets after measurements. The old every-command-under-15ms and adoption claims must leave the README unless independently supported.

## Rendering and composition

Keep existing `{input}` templates. Stdin is UTF-8 text and retains leading/trailing whitespace exactly. An explicit positional input and nonempty piped input together are an error, rather than silently dropping either source. Empty `{input}` is allowed for compatibility. Invalid UTF-8 reports an error before output.

Named placeholders are declared explicitly in optional frontmatter, for example `vars: {lang: null, tone: brief}`. A null declares a required value; a string declares its default, including an empty string. Other value types and reserved token names are rejected. Only declared `{name}` placeholders are substituted. Undeclared code such as Rust `{foo}` and JSON braces remain literal; `{{name}}` escapes a declared or reserved placeholder. `-v name=value` supplies one value and may repeat for distinct names; duplicate or undeclared supplied names fail. Values may contain `=` after the first separator. Precedence is explicit CLI value, then the prompt's own default. There is no hidden project or environment default for ordinary names. Migration previews which old custom-variable config keys would become declarations/defaults; it does not rewrite templates implicitly.

Reserved tokens are the four input slots below; date, time, datetime, timestamp, iso_date, user, hostname, uuid, cwd, pwd, git_branch, git_status, and git_hash; plus the explicit `{env:NAME}` form. Retain their existing supported meanings and document the UTC/local-time distinction. Clipboard is not a template token in this release. Missing requested Git/environment context fails instead of silently disappearing. `{env:NAME}` reads only when written in the template itself. Remove implicit `.env` loading. Input, variable values, environment values, and earlier composition output are opaque text: inserting `{env:NAME}`, `{date}`, or shell syntax inside data never triggers a second expansion. Compute each explicit time/context value once per invocation.

`ph c a,b INPUT` retains the existing **left-to-right template chain**, not an AI workflow. Render a with INPUT; render b with the complete result. For a=`A {input}` and b=`B {input}`, the result is `B A INPUT`. Recognized input slots are `{input}`, `{INPUT}`, `{content}`, and `{CONTENT}`, preserving existing spellings. A stage without any input slot appends its incoming nonempty text after exactly two newline characters. This preserves simple legacy stages; it intentionally repairs the old behavior that dropped input whenever an unrelated brace token such as `{date}` appeared. This can put later template text before earlier text; the help must demonstrate it. Do not silently change `c` into flat concatenation or add a second composition mode in this release. Conflicting named defaults in separate stages remain stage-local; an explicit supplied value overrides that name in every declaring stage, and undeclared values fail against the complete composition declaration set.

Resolve all stages, validate required variables, and render the complete result before writing any payload, file, or clipboard. Missing stages or variables leave every destination unchanged. User values are literal in every stage. Preserve multiline templates and Unicode. Cancel returns without a clipboard or file change.

Retain local composition files and `use --with` directives with an explicit distinction: `./`, `../`, `/`, or `~` starts a file path; all other tokens are prompt selectors, including IDs containing dots. File sources are local only, never URLs. Read their exact text, apply the same declared template rules, and report missing/unreadable required sources before output. Relative paths are resolved from the invocation directory. This replaces the old guess that any dot implies a file. The existing comma-separated list grammar cannot represent a filename containing a comma; use a local imported prompt ID for that case rather than introduce another escaping language.

## CLI output is part of the product

The screen should answer three questions in order: what is selected, what text will I get, and what happened. Keep human help short; keep payload output exact. Use plain words, whitespace, consistent indentation, and one restrained selection accent. No emoji, timing badges, progress animation, box borders, arbitrary IDs without names, or repeated success banners on routine commands. Errors must not require a backtrace to understand.

Interactive use after successful automatic copying:

```text
$ ph u essentials/review
Copied essentials/review to clipboard.
```

That status is on stderr; stdout is empty after successful terminal copying. When the clipboard is unavailable, show the rendered text on stdout and say `Clipboard unavailable; showing the text instead.` on stderr. A short command must not pretend copying succeeded. `ph u essentials/review -q` always prints raw text without automatic copying.

A correct pipe needs no success chatter:

```text
$ git diff | ph u essentials/review | another-tool
```

Only rendered review text reaches another-tool. No clipboard attempt, title, ANSI escape, timing, summary, or blank decoration is added. Compose has the same contract without requiring q to repair current stdout pollution:

```text
$ git diff | ph c work/rules,review | another-tool
```

These example IDs must exist; there is no fuzzy fallback inside a pipeline. The user supplies their own AI or text consumer, and Prompthive never runs it itself.

A missing exact name in noninteractive use should explain the decision:

```text
error: no exact prompt named "review".
Matches:
  essentials/review
  work/review
Use a full name, or run ph to choose with a preview.
```

The error is stderr, exit 2; stdout stays empty. Do not show suggestion lists for a failed filesystem permission check, or hide malformed prompt files as no matches. A failed new/save/export reports the path and whether the prior file is intact or durability is uncertain. It never prints secret values or a prompt body as diagnostic context.

Compact headless help starts with the five primary actions and two real examples, followed by `ph --help` and `ph u --help` for detail. Full help includes retained secondary actions and precise flag meanings; no removed cloud commands are advertised as available. Find/list show names and descriptions to a terminal; piped find/list emit canonical IDs one per line, with no header. That intentional 0.3 machine-output change must be documented. The picker supplies the exact reusable command alongside a selected prompt, so command names and bank paths need not be memorized first.

| Context | Required behavior |
| --- | --- |
| Text-producing command with piped/file stdout and no explicit destination | Exact payload bytes only; never initialize a clipboard or draw a TUI on stdout. |
| Use/show with terminal stdout | Automatic copy, followed by one truthful status on stderr. |
| Automatic copy unavailable | Raw text on stdout plus one explanatory stderr line. |
| Explicit copy (-c where already supported; --clipboard on compose) | Overrides automatic pipe/TTY selection: copy only, leave stdout empty, and report actual success on stderr unless quiet. Failure is exit 4 with an actionable error. |
| `-q` with no explicit destination | Raw payload on stdout, no automatic clipboard or routine status; real errors remain. |
| `-f PATH` | One exact-content local export; no sync. Existing files require explicit force and a recoverable preimage. |
| Explicit TUI without a usable terminal | Explain the exact-name alternative, exit nonzero, and do not hang. |

Explicit destination wins before automatic TTY/pipe behavior. For `ph u ID -c | consumer`, the consumer receives zero bytes whether copying succeeds or fails; copy failure returns exit 4, and q only suppresses success chatter. For `ph u ID -f PATH | consumer`, the consumer receives zero bytes; success creates the exact file and optional stderr status, failure preserves the prior tool-owned file and returns a nonzero error. The same tests include q and unavailable clipboard.

Allow one explicit destination among each command's supported save, append, file, and clipboard options. Retain the actual spellings: use/show/new/clean use -c for copy, while compose uses --clipboard. Reject combinations before effects; do not promise a transaction across disk and clipboard. s remains show at command position; -s saves a new prompt and refuses an existing name. -a appends with a snapshot and exactly two separating newlines. Show stores its unrendered body, while use/compose store rendered text. An explicit destination suppresses automatic copying and payload stdout. q suppresses routine messages but does not override that destination. These are reviewed 0.3 compatibility changes, not existing implementation claims. Ordinary shell redirection is controlled by the shell and is outside the tool-owned file transaction guarantee.

Retain diff and merge as secondary long commands. Diff keeps -f/--format, -c/--context, and --output; it does not acquire global copy/file meanings for those short flags. With no explicit output file, piped diff contains only the diff, without its old timing summary. Its --output path uses the safe writer and conflict policy. Merge remains the existing source-to-target replacement behavior, not an invented three-way merge: both targets must be exact or explicitly selected, preview must have no effects, and replacement keeps the target identity with a recoverable preimage. Do not silently enable its currently unimplemented interactive mode; retain a clear unsupported-mode error with no writes until separately implemented. Document the replacement semantics plainly.

Inspect stdin, stdout, stderr, and the controlling terminal independently. Selection keys must never consume context bytes from a pipe; picker paint must never enter payload stdout. Use no ANSI controls or color when the relevant output is not a terminal, NO_COLOR is set, or TERM is dumb. Terminal failure and signals restore modes and leave the cursor on a usable command line. At 80 by 24, 120 by 40, narrow widths, and after resizing, the selected full ID, actual preview, and essential hints must be readable; reduce the preview before hiding identity or actions. Esc/cancel leaves the prior clipboard and files unchanged.

Exit statuses remain 0 success, 1 storage/operational failure, 2 usage/not-found/ambiguity/validation, 3 edit conflict, 4 explicit clipboard failure, and 130 cancellation. A downstream closed pipe exits quietly, without panic or backtrace; conventional SIGPIPE behavior is acceptable. Shell completions share the actual command and selector inventory, rather than carry a separate cloud-command list.

## Storage and recovery

Retain the current root and on-disk Markdown/frontmatter formats, including existing `prompts`, `banks`, and locally cached team prompts. `PROMPTHIVE_BASE_DIR` explicitly selects one store; otherwise use `~/.prompthive`. Relative explicit roots resolve from the invocation directory. Do not automatically search ancestor projects or merge personal and project stores. A selected project store cannot silently shadow a personal prompt because no implicit merged namespace exists.

Read all existing prompts without migration. Preserve unfamiliar frontmatter fields and original bodies when editing metadata. Existing team paths remain locally readable until an explicit, previewed export moves copies into ordinary banks. Existing credentials, remote identifiers, sync metadata, histories, and remote objects remain untouched and unused; removal of product code does not authorize deleting them.

Writes use a same-directory temporary file, checked serialization, content flush/sync, atomic replacement, and parent-directory sync where the supported platform permits it. Check every error. Atomic replacement alone is not a power-loss durability guarantee: `tempfile.persist` does not sync contents or the directory. Never remove the old file before a replacement is ready. Preserve permissions and restrictive defaults. Do not claim identical durability on untested filesystems or platforms. [tempfile persistence](https://docs.rs/tempfile/latest/tempfile/struct.NamedTempFile.html#method.persist), [Rust file synchronization](https://doc.rust-lang.org/std/fs/struct.File.html#method.sync_all).

Use a process write lock for cooperating Prompthive writers. For an editor round trip, edit a temporary copy and compare the source revision before committing; a detected external change preserves both versions and returns conflict. Noncooperating external writers can race a final comparison, so retain a recoverable preimage and state that limit. A failed directory sync reports uncertain durability, not an invented rollback success.

Create recoverable local snapshots before replacement, rename, deletion, and migration. A failure to create the required backup aborts the mutation. Never prune backups automatically in this release. Rename writes the destination and updates any retained compatibility references before removing the old path; an interrupted multi-file operation can leave two complete copies, with a precise recovery message. Exact identities and any retained compatibility targets never change because of unrelated library additions or ranking changes; deliberate edits to the same canonical file naturally change the returned text.

Reject empty, `.`/`..`, absolute, drive-prefixed, separator-confused, overlong, and colliding canonical path components instead of lossy sanitization. Allow documented Unicode names. Reject symlink escapes on every read/write/import/version/rename/delete path, and test the supported OS behavior. Explicit file-export destinations remain user-chosen paths and are not prompt selectors. Version tags receive the same containment checks. Recovery/rollback must round-trip the body and frontmatter once, without nesting YAML in prompt output.

`ph export DIR` creates a portable local copy of prompts, declared defaults, and any verified legacy compatibility bindings, with a manifest; its destination must be new. `ph import DIR --dry-run` previews restoration and compatibility conflicts without writes. `ph import DIR` imports nonconflicting entries; replacement requires the existing explicit force option and snapshots. Keep this within the retained importer rather than add a second restore engine. Exclude credentials, telemetry, histories containing prompt snippets, and stale remote configuration from the portable export. Existing snapshots remain readable through `ph versions ID`, including after the current prompt is deleted, and rollback restores a complete snapshot. A user can separately copy the whole old directory for binary rollback; Prompthive does not silently upload or destroy it.

Compatibility binding import depends on target import: retain a mapping only if its exact canonical target was successfully imported or its existing bytes match the exported target. When a prompt is skipped for conflict or receives a new versioned name, skip and report dependent bindings until the user chooses a target explicitly. Test an exported legacy selector e pointing to work/review containing A against a destination work/review containing B: default import must not activate e against B. Existing bindings are preserved on disk even when not adopted into the new resolver; no current prompt, note, credential, or old metadata is deleted by this specification.


## Explicit code cuts and retained owners

| Remove from the comeback product | Retain or extract |
| --- | --- |
| Registry client, mock server, registry TUI, remote install/search/publish/unpublish | Local file/directory import and local prompt browsing. |
| Remote sync, sharing/suggestions/improvement APIs, users, login/logout, subscriptions, teams APIs | Plain local prompts already stored in banks or team directories; previewed local export. |
| Sync bookkeeping from `new/use -f` and SimpleSyncManager's fake bidirectional promise | One explicit file export path using the normal writer. |
| Web dashboard/browser launches, telemetry upload and gamification paths | No automatic content history; only minimal opt-in local diagnostics if necessary. |
| Placeholder security/history/batch/vars/health/values command surfaces | Working template values through `-v`; honest errors for removed command names. |
| Dead `tui_old`, unused `core` module, duplicate binary/library command compilation where unnecessary | Existing storage, matching, template, compose, import, edit, clipboard, and local TUI modules, repaired in place. |

Trace callers and feature gates before deleting modules. Remove network-only reqwest/actix/urlencoding/runtime dependencies when no retained path uses them. Do not replace the current TUI, clipboard library, or storage format just to make the diff look new. The default build contains the complete local utility; a documented no-TUI build must also compile. No network activity, daemon, auto-update check, credential fallback, or package download occurs at product runtime.

## Regression and release gates

The untouched baseline default build passed 99 unit tests and one documentation test on Linux with Rust 1.98.0. That did not cover the failures below. The no-default-features build failed with 17 compile errors; formatting also failed. These are internal verification facts, not release marketing.

Independently reproduced baseline failures must become named failing-then-passing cases: parent-path escape; Unicode history panic; composition stdout pollution; ignored show-save; input reinterpreted as environment syntax; successful export's spurious sync warning; and rollback leaking frontmatter. Whitespace preservation, required-variable failure, and shared selector and interactive discovery semantics are proposed contracts rather than claims about old regressions. CLI snapshot listing stayed clean in the fixture; the library's snapshot traversal needs its own test.

Required acceptance groups:

- **Parser/resolver:** every retained one-letter/long verb and flag; s/show versus -s/save, c/compose versus -c/copy, and r/d/v conflicts; no bare-selector fallback; exact identities and compatibility target preservation after insertion/reordering; the e-to-essentials/commit compatibility collision with a real canonical e must disable/report the code while keeping canonical e reachable; bank-qualified IDs; diff/merge retained flags; confirmed interactive discovery versus empty-stdout script failure; no fuzzy mutation; shell completion parity.
- **Renderer:** input/argument conflicts; exact whitespace; Unicode and invalid UTF-8; escaped/code/JSON braces; required/default/duplicate variables; data remains literal; missing final composition stage; explicit chain nesting; no partial destination writes.
- **I/O:** bare-ph empty, no-result, first-use and malformed-file states; all stdin/stdout/controlling-terminal combinations; exact picker preview/selection identity at 80x24 and 120x40 and resize; no keyboard/context mixing; headless operation with clipboard libraries unavailable; real supported desktop copy/paste; copy failure/cancel; explicit-copy and explicit-file destinations with piped stdout, both successful and failed, with and without q; compose --clipboard; diff payload versus --output; file conflicts; quiet output; broken pipe; terminal cleanup after TUI exit or signal.
- **Persistence:** old plain/bank/team/note fixtures; unknown frontmatter; imported conflicts and dependent compatibility bindings; backup/restore and version round trips; exact-source/exact-target merge replacement, target identity retention, preview without writes, snapshot failure, and unsupported interactive mode without effects; external edits on next invocation; path and symlink escapes; disk full, permission failure, interrupted write/rename, failed sync, and concurrent edits. Fault injection must preserve an old or new complete version and report the actual outcome.
- **Removal:** run the whole core suite with network denied; assert no authentication lookup, hidden network call, cloud command dispatch, automatic `.env` load, or sync fallback. Scan source, dependency tree, help, completions, packaged files, and docs for removed infrastructure.
- **Supply chain:** update the retained lockfile and classify advisories by affected runtime path and platform. The 2025 lockfile currently has OSV matches on 19 package versions, including duplicate advisory IDs and two maintenance notices; this does not establish exploitation. Redacted history scanning found two scanner-test fixture matches, not verified live credentials. Keep exact narrow fixture allowlists and a pinned scanner in CI.

Release stages:

1. Review this corrected single-letter, bare-ph, output, compatibility, and cloud-removal scope. Existing shorthand codes are inventoried by their actual old use/show/compose targets before any compatibility binding is retained. Conflicts require an explicit choice; no ordinary render rewrites data. Canonical IDs and existing single-letter actions remain valid. Unarchiving is independently ready as described above and need not wait for this release review. No implementation or release is approved by the specification itself.
2. Keep a draft cleanup PR with one writer, independent review, failing regressions, and preserved fixtures. No rewrite and no competitor code import. Reuse interaction ideas through original implementation; any later code reuse requires license inspection and attribution.
3. Build/test the default and no-TUI configurations, measure the five journeys, and wire the source-bound local release gate into CI. Green local tests are not hosted CI evidence.
4. Verify the exact proposed remote head, then the hosted checks on that same commit. Review tag-first notes, version declarations, package contents, and the full diff. Check all existing releases and drafts before choosing a newer SemVer tag.
5. Only after scope review and the authorized publication sequence: confirm the identified repository is unarchived, land the reviewed candidate, publish through the established crate/release process, download the actual artifacts, and replay installation plus the five consumer journeys against them. Verify published commit, tag, title, body, assets, latest status, and rollback instructions. Never reuse a stale package artifact as proof of the new release.

## Research decisions

These references inform interaction choices; their features are not a reason to grow the scope.

- [Pet](https://github.com/knqyf263/pet) demonstrates short access to stored snippets and editable local configuration. Borrow fast retrieval; omit its sync and shell-execution surface.
- [Prompt Box](https://github.com/nucliweb/prompt-box) offers search with a preview pane, editor access, clipboard, import/export, and shell completion. Keep preview and local portability; preserve Prompthive's existing short action verbs instead of copying its product wholesale.
- [cueCLI](https://github.com/cuecli/cueCLI) separates variable substitution, preview, and clean piping. Adopt explicit payload/status boundaries, using Prompthive's existing syntax where it works.
- [fzf search syntax](https://junegunn.github.io/fzf/search-syntax/) is a reference for predictable discovery. Do not add its full query language or make fzf a mandatory dependency in this release.
- [LLM templates](https://llm.datasette.io/en/stable/templates.html) demonstrate reusable named templates and parameters. Keep Prompthive independent of a model runner.
- [Command Line Interface Guidelines](https://clig.dev/) support useful errors, honest exit status, pipeable payloads, and explicit interactivity. Apply those contracts to every retained command.

The advantage to prove is a dependable personal prompt library that stays easy to use across tools. A smaller codebase and a promising command table do not prove that advantage; the five journeys and Jory's scope review do.
