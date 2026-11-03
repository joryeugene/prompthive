#!/usr/bin/env bash
# Local-only demonstration. All prompt data stays in a disposable directory.
set -euo pipefail

ph_binary=${PROMPTHIVE_BIN:-ph}
if ! command -v "$ph_binary" >/dev/null 2>&1; then
  printf 'Cannot find ph. Set PROMPTHIVE_BIN to the candidate binary.\n' >&2
  exit 1
fi

demo_dir=$(mktemp -d "${TMPDIR:-/tmp}/ph-examples.XXXXXXXX")
cleanup() {
  rm -rf -- "$demo_dir"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
export PROMPTHIVE_BASE_DIR="$demo_dir/store"
export XDG_CONFIG_HOME="$demo_dir/config"
mkdir -p "$PROMPTHIVE_BASE_DIR" "$XDG_CONFIG_HOME"

printf 'Prompthive local examples (temporary library)\n\n'

"$ph_binary" n study/explain 'Explain this in plain language: {input}' -q >/dev/null
"$ph_binary" n work/rules 'Use concrete examples. Keep the answer brief.' -q >/dev/null
"$ph_binary" n work/review 'Review this material for concrete defects: {input}' -q >/dev/null
printf '%s\n' 'Design meeting: preserve the original notes.' |
  "$ph_binary" n notes/meeting -q >/dev/null

printf '1. Find saved text by its full name\n'
"$ph_binary" f work
printf '\n2. Reuse a template with literal input\n'
"$ph_binary" u study/explain 'Rust ownership' -q
printf '\n\n3. Chain two prompts with piped context\n'
printf '%s' 'A small code change' | "$ph_binary" c work/rules,work/review -q
printf '\n\n4. Read notes and save a separate copy\n'
"$ph_binary" s notes/meeting -s notes/meeting-copy -q >/dev/null
"$ph_binary" r notes/meeting-copy notes/meeting-archive >/dev/null
"$ph_binary" s notes/meeting-archive -q

# Optional YAML declares variables. This fixture is an ordinary Markdown file.
mkdir -p "$PROMPTHIVE_BASE_DIR/banks/work"
cat > "$PROMPTHIVE_BASE_DIR/banks/work/explain.md" <<'PROMPT'
---
id: work/explain
description: Explain a change
vars: {language: null, tone: brief}
---

Explain this {language} change in a {tone} tone: {input}
PROMPT
printf '\n5. Supply declared variables\n'
printf '%s' 'A parser correction' |
  "$ph_binary" u work/explain -v language=Rust -v tone=direct -q

printf '\n6. Take a local version and restore it\n'
"$ph_binary" version notes/meeting before-append -m 'Original note' >/dev/null
"$ph_binary" s notes/meeting-archive -a notes/meeting -q >/dev/null
"$ph_binary" rollback notes/meeting before-append >/dev/null
"$ph_binary" s notes/meeting -q > "$demo_dir/original.txt"
printf '%s\n' 'Design meeting: preserve the original notes.' > "$demo_dir/expected.txt"
cmp "$demo_dir/expected.txt" "$demo_dir/original.txt"

printf '\n7. Export, preview an import, and verify a restored copy\n'
"$ph_binary" export "$demo_dir/export" >/dev/null
mkdir "$demo_dir/restored"
PROMPTHIVE_BASE_DIR="$demo_dir/restored" "$ph_binary" import "$demo_dir/export" --dry-run >/dev/null
PROMPTHIVE_BASE_DIR="$demo_dir/restored" "$ph_binary" import "$demo_dir/export" >/dev/null
PROMPTHIVE_BASE_DIR="$demo_dir/restored" "$ph_binary" s notes/meeting -q > "$demo_dir/restored.txt"
cmp "$demo_dir/original.txt" "$demo_dir/restored.txt"
printf 'The restored note matches the original bytes.\n'
printf '\nDone. The temporary library and export are removed on exit.\n'
