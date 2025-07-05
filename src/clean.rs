//! Text cleaning and normalization utilities
//!
//! Provides functionality to clean text content by removing ANSI escape sequences,
//! box drawing characters, and other formatting artifacts to produce clean,
//! readable text suitable for processing and storage.

use once_cell::sync::Lazy;
use regex::Regex;

// Box drawing characters to remove
static BOX_CHARS: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"[│┤╡╢╖╕╣║╗╝╜╛┐└┴┬├─┼╞╟╚╔╩╦╠═╬╧╨╤╥╙╘╒╓╫╪┘┌╯╰╱╲╳╭╮]").unwrap());

// ANSI escape sequences
static ANSI_ESCAPES: Lazy<Regex> = Lazy::new(|| Regex::new(r"\x1b\[[0-9;]*[mGKHF]").unwrap());

// Common TUI artifacts
static TUI_ARTIFACTS: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*[│┃┆┊]\s*|\s*[│┃┆┊]\s*$").unwrap());

/// Clean text by removing TUI artifacts, box drawing characters, and ANSI escapes
pub fn clean_text(input: &str) -> String {
    let mut cleaned = input.to_string();

    // Remove ANSI escape sequences first
    cleaned = ANSI_ESCAPES.replace_all(&cleaned, "").to_string();

    // Remove box drawing characters
    cleaned = BOX_CHARS.replace_all(&cleaned, "").to_string();

    // Clean up line-by-line TUI artifacts
    cleaned = cleaned
        .lines()
        .map(|line| {
            // Remove leading/trailing TUI borders
            let line = TUI_ARTIFACTS.replace_all(line, "");
            line.trim().to_string()
        })
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    // Clean up excessive whitespace
    cleaned = cleaned.trim().to_string();

    // Replace multiple blank lines with single blank line
    let multi_newline = Regex::new(r"\n{3,}").unwrap();
    cleaned = multi_newline.replace_all(&cleaned, "\n\n").to_string();

    cleaned
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_box_chars() {
        let input = "│ Hello World │";
        assert_eq!(clean_text(input), "Hello World");
    }

    #[test]
    fn test_clean_ansi() {
        let input = "\x1b[1mBold Text\x1b[0m";
        assert_eq!(clean_text(input), "Bold Text");
    }

    #[test]
    fn test_clean_complex() {
        let input = r#"╭─────────────────╮
│ Test Message    │
│ With multiple   │
│ lines          │
╰─────────────────╯"#;
        let expected = "Test Message\nWith multiple\nlines";
        assert_eq!(clean_text(input), expected);
    }
}
