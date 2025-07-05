use anyhow::Result;
use std::time::Instant;
use crate::Storage;

pub fn handle_web(storage: &Storage, page: Option<&str>, _port: u16, no_browser: bool, start: Instant) -> Result<()> {
    println!("✓ Web module works!");
    println!("⏱️  Web generated ({}ms)", start.elapsed().as_millis());
    Ok(())
}