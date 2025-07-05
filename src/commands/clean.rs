use crate::{clean::clean_text, IoOptions, Storage};
use anyhow::Result;
use std::io::{self, Read};
use std::time::Instant;

/// Handle the clean command - remove TUI artifacts and formatting from text
pub fn handle_clean(
    text: Option<&str>,
    io_options: &IoOptions,
    start: Instant,
) -> Result<()> {
    // Get input text - either from argument or stdin
    let input = if let Some(text) = text {
        text.to_string()
    } else {
        // Read from stdin
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer)?;
        buffer
    };

    // Clean the text using the existing clean_text function
    let cleaned = clean_text(&input);

    // Initialize storage for I/O operations
    let storage = Storage::new()?;

    // Handle output according to I/O options
    io_options.apply_unified_io(
        &storage,
        &cleaned,
        "Cleaned text",
        start,
    )?;

    Ok(())
}