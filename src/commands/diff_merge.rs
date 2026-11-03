//! Local comparison and explicit source-to-target replacement.

use anyhow::{bail, Context, Result};
use std::io::{self, Write};
use std::path::Path;
use std::time::Instant;

use crate::{persistence, Storage};

pub fn handle_diff(
    storage: &Storage,
    prompt1: &str,
    prompt2: &str,
    format: &str,
    context: usize,
    output: Option<&str>,
    start: Instant,
) -> Result<()> {
    handle_diff_with_force(
        storage, prompt1, prompt2, format, context, output, false, start,
    )
}

// Retain the existing library handler signature for callers.
#[allow(clippy::too_many_arguments)]
pub fn handle_diff_with_force(
    storage: &Storage,
    prompt1: &str,
    prompt2: &str,
    format: &str,
    context: usize,
    output: Option<&str>,
    force: bool,
    _start: Instant,
) -> Result<()> {
    let (_, content1) = storage.read_prompt(prompt1)?;
    let (_, content2) = storage.read_prompt(prompt2)?;
    let diff = match format {
        "unified" => generate_unified_diff(prompt1, &content1, prompt2, &content2, context)?,
        "side-by-side" | "side" => {
            generate_side_by_side_diff(prompt1, &content1, prompt2, &content2)?
        }
        "brief" => generate_brief_diff(prompt1, &content1, prompt2, &content2),
        _ => bail!("unsupported diff format '{format}'; use unified, side-by-side, or brief"),
    };
    if let Some(path) = output {
        persistence::atomic_write(Path::new(path), diff.as_bytes(), force, force)
            .with_context(|| format!("could not write diff to '{path}'"))?;
        eprintln!("Wrote diff to {path}.");
    } else {
        let mut stdout = io::stdout().lock();
        stdout.write_all(diff.as_bytes())?;
        stdout.flush()?;
    }
    Ok(())
}

pub fn handle_merge(
    storage: &Storage,
    source_prompt: &str,
    target_prompt: &str,
    _backup: bool,
    preview: bool,
    interactive: bool,
    _start: Instant,
) -> Result<()> {
    if interactive {
        bail!(
            "interactive merge is not supported; use --preview, then an explicit source and target"
        );
    }
    if source_prompt == target_prompt {
        bail!("merge source and target are the same prompt");
    }
    let (source_metadata, source_body) = storage.read_prompt(source_prompt)?;
    let target_raw = storage.read_raw(target_prompt)?;
    let target_text = std::str::from_utf8(&target_raw).context("target is not valid UTF-8")?;
    let (mut target_metadata, _) = storage.parse_prompt_content(target_text)?;
    target_metadata.id = target_prompt.to_owned();
    target_metadata.vars = source_metadata.vars;
    if !source_metadata.description.is_empty() && source_metadata.description != "No description" {
        target_metadata.description = source_metadata.description;
    }
    target_metadata.updated_at = Some(chrono::Utc::now().to_rfc3339());
    if preview {
        let mut text = format!("Replace {target_prompt} with the body of {source_prompt}:\n\n");
        crate::limits::push_str(&mut text, &source_body, "merge preview")?;
        return super::common::write_stdout(&text);
    }
    let needs_frontmatter = target_text.starts_with("---\n")
        || target_text.starts_with("---\r\n")
        || !target_metadata.vars.is_empty();
    let replacement = if needs_frontmatter {
        Storage::serialize_prompt(&target_metadata, &source_body)?
    } else {
        source_body.into_bytes()
    };
    storage.replace_raw_if_unchanged(target_prompt, &replacement, &target_raw)?;
    eprintln!("Replaced {target_prompt} from {source_prompt}; previous content was backed up.");
    Ok(())
}

fn diff_line(prefix: char, line: &str, result: &mut String) -> Result<()> {
    crate::limits::check_len(
        result.len().saturating_add(prefix.len_utf8()),
        crate::limits::TEXT_BYTES,
        "diff output",
    )?;
    result.push(prefix);
    crate::limits::push_str(result, line, "diff output")?;
    if !line.ends_with('\n') {
        crate::limits::push_str(result, "\n\\ No newline at end of file\n", "diff output")?;
    }
    Ok(())
}

fn unified_range(start: usize, count: usize) -> String {
    if count == 0 {
        format!("{start},0")
    } else if count == 1 {
        (start + 1).to_string()
    } else {
        format!("{},{}", start + 1, count)
    }
}

fn generate_unified_diff(
    name1: &str,
    content1: &str,
    name2: &str,
    content2: &str,
    context: usize,
) -> Result<String> {
    if content1 == content2 {
        return Ok(String::new());
    }
    // Bound line indexes as well as payload bytes before allocating their vectors.
    for text in [content1, content2] {
        crate::limits::check_len(text.lines().count(), 65_536, "diff line count")?;
    }
    let before: Vec<_> = content1.split_inclusive('\n').collect();
    let after: Vec<_> = content2.split_inclusive('\n').collect();
    let common_prefix = before
        .iter()
        .zip(&after)
        .take_while(|(a, b)| a == b)
        .count();
    let mut common_suffix = 0;
    while common_suffix < before.len().saturating_sub(common_prefix)
        && common_suffix < after.len().saturating_sub(common_prefix)
        && before[before.len() - common_suffix - 1] == after[after.len() - common_suffix - 1]
    {
        common_suffix += 1;
    }
    let start = common_prefix.saturating_sub(context);
    let trailing = common_suffix.min(context);
    let old_change_end = before.len() - common_suffix;
    let new_change_end = after.len() - common_suffix;
    let old_end = old_change_end + trailing;
    let new_end = new_change_end + trailing;
    let mut result = format!(
        "--- {name1}\n+++ {name2}\n@@ -{} +{} @@\n",
        unified_range(start, old_end - start),
        unified_range(start, new_end - start)
    );
    for line in &before[start..common_prefix] {
        diff_line(' ', line, &mut result)?;
    }
    for line in &before[common_prefix..old_change_end] {
        diff_line('-', line, &mut result)?;
    }
    for line in &after[common_prefix..new_change_end] {
        diff_line('+', line, &mut result)?;
    }
    for line in &before[old_change_end..old_end] {
        diff_line(' ', line, &mut result)?;
    }
    Ok(result)
}

fn display_fragment(line: &str) -> String {
    if line.chars().count() > 38 {
        format!("{}..", line.chars().take(36).collect::<String>())
    } else {
        line.to_owned()
    }
}

fn generate_side_by_side_diff(
    name1: &str,
    content1: &str,
    name2: &str,
    content2: &str,
) -> Result<String> {
    for text in [content1, content2] {
        crate::limits::check_len(text.lines().count(), 65_536, "diff line count")?;
    }
    let before: Vec<_> = content1.lines().collect();
    let after: Vec<_> = content2.lines().collect();
    let mut result = format!("{name1:<40} | {name2}\n");
    for index in 0..before.len().max(after.len()) {
        let left = display_fragment(before.get(index).copied().unwrap_or(""));
        let right = display_fragment(after.get(index).copied().unwrap_or(""));
        crate::limits::push_str(
            &mut result,
            &format!("{left:<40} | {right}\n"),
            "diff output",
        )?;
    }
    Ok(result)
}

fn generate_brief_diff(name1: &str, content1: &str, name2: &str, content2: &str) -> String {
    if content1 == content2 {
        format!("Prompts {name1} and {name2} are identical\n")
    } else {
        format!("Prompts {name1} and {name2} differ\n{name1}: {} lines, {} characters\n{name2}: {} lines, {} characters\n",
            content1.lines().count(), content1.chars().count(), content2.lines().count(), content2.chars().count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[allow(dead_code)]
    fn create_test_storage() -> (TempDir, Storage) {
        let temp_dir = TempDir::new().unwrap();
        let storage = Storage::new_with_base(temp_dir.path().to_path_buf()).unwrap();
        storage.init().unwrap();
        (temp_dir, storage)
    }

    #[test]
    fn test_generate_brief_diff_identical() {
        let result = generate_brief_diff("prompt1", "same content", "prompt2", "same content");
        assert!(result.contains("identical"));
    }

    #[test]
    fn test_generate_brief_diff_different() {
        let result = generate_brief_diff("prompt1", "content1", "prompt2", "content2");
        assert!(result.contains("differ"));
        assert!(result.contains("1 lines"));
    }

    #[test]
    fn test_generate_side_by_side_diff() {
        let result =
            generate_side_by_side_diff("p1", "line1\nline2", "p2", "line1\nchanged").unwrap();
        assert!(result.contains("p1"));
        assert!(result.contains("p2"));
        assert!(result.contains("line1"));
    }

    #[test]
    fn test_generate_unified_diff() {
        let result =
            generate_unified_diff("p1", "line1\nline2", "p2", "line1\nchanged", 1).unwrap();
        assert!(result.contains("---"));
        assert!(result.contains("+++"));
        assert!(result.contains("@@"));
    }

    #[test]
    fn unified_diff_has_valid_ranges_for_insertions_deletions_and_missing_newline() {
        assert_eq!(
            generate_unified_diff("a", "", "b", "one\n", 0).unwrap(),
            "--- a\n+++ b\n@@ -0,0 +1 @@\n+one\n"
        );
        assert_eq!(
            generate_unified_diff("a", "one\n", "b", "", 0).unwrap(),
            "--- a\n+++ b\n@@ -1 +0,0 @@\n-one\n"
        );
        assert_eq!(
            generate_unified_diff("a", "same\nold", "b", "same\nnew\n", 0).unwrap(),
            "--- a\n+++ b\n@@ -2 +2 @@\n-old\n\\ No newline at end of file\n+new\n"
        );
        assert!(generate_unified_diff("a", "same\n", "b", "same\n", 1)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn side_by_side_handles_long_unicode_lines_without_slicing_bytes() {
        let text = "🦀".repeat(80);
        let result = generate_side_by_side_diff("a", &text, "b", &text).unwrap();
        assert!(result.contains(&format!("{}..", "🦀".repeat(36))));
        assert!(!result.contains('\u{1b}'));
    }

    #[test]
    fn excessive_diff_structure_or_output_is_rejected_before_emission() {
        let lines = "x\n".repeat(65_537);
        assert!(generate_unified_diff("a", &lines, "b", "changed", 3).is_err());
        assert!(generate_side_by_side_diff("a", &lines, "b", "changed").is_err());
        let boundary = "x".repeat(crate::limits::TEXT_BYTES);
        assert!(generate_unified_diff("a", &boundary, "b", "changed", 0).is_err());
        assert!(generate_unified_diff("a", "below\n", "b", "changed\n", 0).is_ok());
    }

    #[test]
    fn merge_preserves_target_identity_unknown_fields_and_raw_preimage() {
        let (_temp, storage) = create_test_storage();
        let target =
            b"---\nid: target\ndescription: Old\ncustom: target-value\n---\n\nTarget body\n\n";
        let source = b"---\nid: source\ndescription: New\nvars:\n  language: Rust\ncustom: source-value\n---\n\n  Review in {language}\n\n";
        storage.write_raw("source", source, false).unwrap();
        storage.write_raw("target", target, false).unwrap();
        handle_merge(
            &storage,
            "source",
            "target",
            false,
            false,
            false,
            Instant::now(),
        )
        .unwrap();
        let (metadata, body) = storage.read_prompt("target").unwrap();
        assert_eq!(metadata.id, "target");
        assert_eq!(metadata.description, "New");
        assert_eq!(metadata.extra["custom"].as_str(), Some("target-value"));
        assert_eq!(metadata.vars.get("language"), Some(&Some("Rust".into())));
        assert_eq!(body, "  Review in {language}\n\n");
        assert!(storage
            .snapshot_paths("target")
            .unwrap()
            .iter()
            .any(|(_, path)| storage.read_safe_path(path).unwrap() == target));
    }

    #[test]
    fn merge_preserves_declared_variables_into_a_plain_target_and_preview_never_writes() {
        let (_temp, storage) = create_test_storage();
        storage
            .write_raw(
                "source",
                b"---\nvars:\n  language: Rust\n---\n\n{language}",
                false,
            )
            .unwrap();
        storage
            .write_raw("target", b"Plain original", false)
            .unwrap();
        let before = storage.read_raw("target").unwrap();
        handle_merge(
            &storage,
            "source",
            "target",
            false,
            true,
            false,
            Instant::now(),
        )
        .unwrap();
        assert_eq!(storage.read_raw("target").unwrap(), before);
        assert!(storage.snapshot_paths("target").unwrap().is_empty());
        assert!(handle_merge(
            &storage,
            "source",
            "target",
            false,
            false,
            true,
            Instant::now()
        )
        .is_err());
        assert_eq!(storage.read_raw("target").unwrap(), before);
        handle_merge(
            &storage,
            "source",
            "target",
            false,
            false,
            false,
            Instant::now(),
        )
        .unwrap();
        let (metadata, body) = storage.read_prompt("target").unwrap();
        assert_eq!(metadata.id, "target");
        assert_eq!(metadata.vars.get("language"), Some(&Some("Rust".into())));
        assert_eq!(body, "{language}");
    }

    #[test]
    fn diff_output_refuses_overwrite_without_force_and_snapshots_with_force() {
        let (temp, storage) = create_test_storage();
        storage.write_raw("left", b"Before\n", false).unwrap();
        storage.write_raw("right", b"After\n", false).unwrap();
        let path = temp.path().join("diff.patch");
        std::fs::write(&path, b"Existing file").unwrap();
        let output = path.to_str().unwrap();
        assert!(handle_diff(
            &storage,
            "left",
            "right",
            "unified",
            2,
            Some(output),
            Instant::now()
        )
        .is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"Existing file");
        handle_diff_with_force(
            &storage,
            "left",
            "right",
            "unified",
            2,
            Some(output),
            true,
            Instant::now(),
        )
        .unwrap();
        assert!(std::fs::read_to_string(&path)
            .unwrap()
            .starts_with("--- left\n+++ right\n"));
        let backups: Vec<_> = std::fs::read_dir(temp.path().join(".ph-backups/diff.patch"))
            .unwrap()
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(
            std::fs::read(backups[0].as_ref().unwrap().path()).unwrap(),
            b"Existing file"
        );
    }
}
