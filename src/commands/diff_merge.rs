// Diff and merge commands - extracted from main.rs

use anyhow::Result;
use colored::*;
use std::fs;
use std::time::Instant;

use super::common::resolve_prompt_name;
use crate::Storage;

pub fn handle_diff(
    storage: &Storage,
    prompt1: &str,
    prompt2: &str,
    format: &str,
    context: usize,
    output: Option<&str>,
    start: Instant,
) -> Result<()> {
    // Resolve prompt names with fuzzy matching
    let resolved_prompt1 = resolve_prompt_name(storage, prompt1)?;
    let resolved_prompt2 = resolve_prompt_name(storage, prompt2)?;

    // Read prompt contents
    let (_, content1) = storage.read_prompt(&resolved_prompt1)?;
    let (_, content2) = storage.read_prompt(&resolved_prompt2)?;

    // Generate diff based on format
    let diff_output = match format {
        "unified" => generate_unified_diff(
            &resolved_prompt1,
            &content1,
            &resolved_prompt2,
            &content2,
            context,
        ),
        "side-by-side" | "side" => {
            generate_side_by_side_diff(&resolved_prompt1, &content1, &resolved_prompt2, &content2)
        }
        "brief" => generate_brief_diff(&resolved_prompt1, &content1, &resolved_prompt2, &content2),
        _ => {
            eprintln!(
                "Error: Unsupported diff format '{}'. Supported: unified, side-by-side, brief",
                format
            );
            std::process::exit(1);
        }
    };

    // Output result
    if let Some(output_file) = output {
        fs::write(output_file, &diff_output)?;
        println!(
            "✓ Diff written to {} ({}ms)",
            output_file,
            start.elapsed().as_millis()
        );
    } else {
        println!("{}", diff_output);
        println!("⏱️  Diff completed ({}ms)", start.elapsed().as_millis());
    }

    Ok(())
}

pub fn handle_merge(
    storage: &Storage,
    source_prompt: &str,
    target_prompt: &str,
    backup: bool,
    preview: bool,
    interactive: bool,
    start: Instant,
) -> Result<()> {
    // Resolve prompt names
    let resolved_source = resolve_prompt_name(storage, source_prompt)?;
    let resolved_target = resolve_prompt_name(storage, target_prompt)?;

    // Read source content
    let (source_metadata, source_content) = storage.read_prompt(&resolved_source)?;
    let (target_metadata, target_content) = storage.read_prompt(&resolved_target)?;

    if preview {
        // Show what would be merged
        println!("📋 Merge Preview:");
        println!(
            "Source: {} -> Target: {}",
            resolved_source.bold(),
            resolved_target.bold()
        );
        println!("\nTarget content would be replaced with:");
        println!("{}", source_content.dimmed());
        println!("⏱️  Preview completed ({}ms)", start.elapsed().as_millis());
        return Ok(());
    }

    if interactive {
        eprintln!("Error: Interactive merge not yet implemented");
        std::process::exit(1);
    }

    // Create backup if requested
    if backup {
        let backup_name = format!("{}.backup", resolved_target);
        storage.write_prompt(&backup_name, &target_metadata, &target_content)?;
        println!("💾 Backup created: {}", backup_name.dimmed());
    }

    // Perform merge (simple content replacement for now)
    storage.write_prompt(&resolved_target, &source_metadata, &source_content)?;

    println!(
        "✓ Merged {} into {} ({}ms)",
        resolved_source.green(),
        resolved_target.green(),
        start.elapsed().as_millis()
    );

    Ok(())
}

fn generate_unified_diff(
    name1: &str,
    content1: &str,
    name2: &str,
    content2: &str,
    context: usize,
) -> String {
    let mut result = String::new();

    result.push_str(&format!("--- {}\n", name1));
    result.push_str(&format!("+++ {}\n", name2));

    let lines1: Vec<&str> = content1.lines().collect();
    let lines2: Vec<&str> = content2.lines().collect();

    // Simple line-by-line comparison
    let mut i = 0;
    let mut j = 0;
    let mut hunk_start1: usize = 0;
    let mut hunk_start2: usize = 0;
    let mut hunk_lines = Vec::new();

    while i < lines1.len() || j < lines2.len() {
        if i < lines1.len() && j < lines2.len() && lines1[i] == lines2[j] {
            // Lines match
            if !hunk_lines.is_empty() {
                // Add context before
                let context_start = hunk_start1.saturating_sub(context);
                for ctx_i in context_start..hunk_start1 {
                    if ctx_i < lines1.len() {
                        hunk_lines.insert(0, format!(" {}", lines1[ctx_i]));
                    }
                }

                // Add context after
                let mut added_context = 0usize;
                while added_context < context
                    && i + added_context < lines1.len()
                    && j + added_context < lines2.len()
                {
                    if lines1[i + added_context] == lines2[j + added_context] {
                        hunk_lines.push(format!(" {}", lines1[i + added_context]));
                        added_context += 1;
                    } else {
                        break;
                    }
                }

                // Output hunk
                let hunk_len1 = hunk_lines.iter().filter(|l| !l.starts_with('+')).count();
                let hunk_len2 = hunk_lines.iter().filter(|l| !l.starts_with('-')).count();
                result.push_str(&format!(
                    "@@ -{},{} +{},{} @@\n",
                    context_start + 1,
                    hunk_len1,
                    hunk_start2 + 1,
                    hunk_len2
                ));

                for line in &hunk_lines {
                    if line.starts_with('-') {
                        result.push_str(&format!("{}\n", line.red()));
                    } else if line.starts_with('+') {
                        result.push_str(&format!("{}\n", line.green()));
                    } else {
                        result.push_str(&format!("{}\n", line));
                    }
                }

                hunk_lines.clear();
                i += added_context;
                j += added_context;
            }
            i += 1;
            j += 1;
        } else if i < lines1.len() && (j >= lines2.len() || lines1[i] != lines2[j]) {
            // Line removed
            if hunk_lines.is_empty() {
                hunk_start1 = i;
                hunk_start2 = j;
            }
            hunk_lines.push(format!("-{}", lines1[i]));
            i += 1;
        } else if j < lines2.len() {
            // Line added
            if hunk_lines.is_empty() {
                hunk_start1 = i;
                hunk_start2 = j;
            }
            hunk_lines.push(format!("+{}", lines2[j]));
            j += 1;
        }
    }

    // Output final hunk if any
    if !hunk_lines.is_empty() {
        let hunk_len1 = hunk_lines.iter().filter(|l| !l.starts_with('+')).count();
        let hunk_len2 = hunk_lines.iter().filter(|l| !l.starts_with('-')).count();
        result.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            hunk_start1 + 1,
            hunk_len1,
            hunk_start2 + 1,
            hunk_len2
        ));

        for line in &hunk_lines {
            if line.starts_with('-') {
                result.push_str(&format!("{}\n", line.red()));
            } else if line.starts_with('+') {
                result.push_str(&format!("{}\n", line.green()));
            } else {
                result.push_str(&format!("{}\n", line));
            }
        }
    }

    result
}

fn generate_side_by_side_diff(name1: &str, content1: &str, name2: &str, content2: &str) -> String {
    let mut result = String::new();

    result.push_str(&format!("{:<40} | {}\n", name1, name2));
    result.push_str(&format!("{:-<40} | {:-<40}\n", "", ""));

    let lines1: Vec<&str> = content1.lines().collect();
    let lines2: Vec<&str> = content2.lines().collect();

    let max_lines = lines1.len().max(lines2.len());

    for i in 0..max_lines {
        let line1 = if i < lines1.len() {
            let mut l = lines1[i].to_string();
            if l.len() > 38 {
                l.truncate(38);
                l.push_str("..");
            }
            l
        } else {
            String::new()
        };

        let line2 = if i < lines2.len() {
            let mut l = lines2[i].to_string();
            if l.len() > 38 {
                l.truncate(38);
                l.push_str("..");
            }
            l
        } else {
            String::new()
        };

        if line1 != line2 {
            result.push_str(&format!("{:<40} | {}\n", line1.red(), line2.green()));
        } else {
            result.push_str(&format!("{:<40} | {}\n", line1, line2));
        }
    }

    result
}

fn generate_brief_diff(name1: &str, content1: &str, name2: &str, content2: &str) -> String {
    if content1 == content2 {
        format!("Prompts {} and {} are identical", name1, name2)
    } else {
        let lines1 = content1.lines().count();
        let lines2 = content2.lines().count();
        let chars1 = content1.chars().count();
        let chars2 = content2.chars().count();

        format!(
            "Prompts {} and {} differ\n{}: {} lines, {} characters\n{}: {} lines, {} characters",
            name1, name2, name1, lines1, chars1, name2, lines2, chars2
        )
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
        let result = generate_side_by_side_diff("p1", "line1\nline2", "p2", "line1\nchanged");
        assert!(result.contains("p1"));
        assert!(result.contains("p2"));
        assert!(result.contains("line1"));
    }

    #[test]
    fn test_generate_unified_diff() {
        let result = generate_unified_diff("p1", "line1\nline2", "p2", "line1\nchanged", 1);
        assert!(result.contains("---"));
        assert!(result.contains("+++"));
        assert!(result.contains("@@"));
    }
}
