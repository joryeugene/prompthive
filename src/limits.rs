//! Byte budgets for local text ingress and rendering; oversized data is never truncated.
use anyhow::{Context, Result};
use std::fmt;
use std::io::Read;
use std::path::Path;

pub const TEXT_BYTES: usize = 32 * 1024 * 1024;
pub const FRONTMATTER_BYTES: usize = 1024 * 1024;
pub const IMPORT_BYTES: usize = 128 * 1024 * 1024;
pub(crate) const TEMPLATE_PARTS: usize = 65_536;

#[derive(Debug)]
pub struct LimitError {
    label: String,
    limit: usize,
}
impl fmt::Display for LimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} exceeds the {}-byte safety limit; use smaller input (nothing was truncated)",
            self.label, self.limit
        )
    }
}
impl std::error::Error for LimitError {}

pub fn exceeded(label: &str, limit: usize) -> anyhow::Error {
    LimitError {
        label: label.into(),
        limit,
    }
    .into()
}
pub fn check_len(length: usize, limit: usize, label: &str) -> Result<()> {
    if length > limit {
        return Err(exceeded(label, limit));
    }
    Ok(())
}
pub fn check_text(text: &str, label: &str) -> Result<()> {
    check_len(text.len(), TEXT_BYTES, label)
}

/// Read only up to the budget, then probe one extra byte without allocating it.
/// This also enforces the limit when a regular file grows after its metadata check.
pub fn read(reader: impl Read, limit: usize, label: &str) -> Result<Vec<u8>> {
    let mut reader = reader;
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 8192];
    while bytes.len() < limit {
        let available = chunk.len().min(limit - bytes.len());
        let count = reader.read(&mut chunk[..available])?;
        if count == 0 {
            return Ok(bytes);
        }
        bytes
            .try_reserve_exact(count)
            .context("cannot allocate bounded input buffer")?;
        bytes.extend_from_slice(&chunk[..count]);
    }
    if reader.read(&mut chunk[..1])? != 0 {
        return Err(exceeded(label, limit));
    }
    Ok(bytes)
}

pub fn read_file(path: impl AsRef<Path>, label: &str) -> Result<Vec<u8>> {
    let path = path.as_ref();
    let label = format!("{label} {}", path.display());
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .with_context(|| format!("cannot open {label}"))?;
    let metadata = file.metadata()?;
    anyhow::ensure!(metadata.is_file(), "not a regular file: {}", path.display());
    if metadata.len() > TEXT_BYTES as u64 {
        return Err(exceeded(&label, TEXT_BYTES));
    }
    read(file, TEXT_BYTES, &label)
}

pub fn push_str(target: &mut String, text: &str, label: &str) -> Result<()> {
    let size = target
        .len()
        .checked_add(text.len())
        .ok_or_else(|| exceeded(label, TEXT_BYTES))?;
    check_len(size, TEXT_BYTES, label)?;
    target
        .try_reserve(text.len())
        .context("cannot allocate bounded output buffer")?;
    target.push_str(text);
    Ok(())
}

pub struct Budget {
    used: usize,
    limit: usize,
    label: &'static str,
}
impl Budget {
    pub fn new(limit: usize, label: &'static str) -> Self {
        Self {
            used: 0,
            limit,
            label,
        }
    }
    pub fn add(&mut self, bytes: usize) -> Result<()> {
        let size = self
            .used
            .checked_add(bytes)
            .ok_or_else(|| exceeded(self.label, self.limit))?;
        check_len(size, self.limit, self.label)?;
        self.used = size;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn streaming_boundaries_accept_exact_bytes_and_reject_an_extra_byte() {
        for size in [0, 3, 4] {
            assert_eq!(
                read(std::io::Cursor::new(vec![b'x'; size]), 4, "fixture")
                    .unwrap()
                    .len(),
                size
            );
        }
        let error = read(std::io::Cursor::new(b"12345"), 4, "growing fixture").unwrap_err();
        assert!(error.downcast_ref::<LimitError>().is_some());
    }
    #[test]
    fn aggregate_budget_checks_before_consuming_and_catches_overflow() {
        let mut budget = Budget::new(4, "batch");
        budget.add(3).unwrap();
        assert!(budget.add(2).is_err());
        budget.add(1).unwrap();
        let mut budget = Budget::new(usize::MAX, "batch");
        budget.add(usize::MAX).unwrap();
        assert!(budget.add(1).is_err());
    }
    #[test]
    fn output_rejection_preserves_the_prior_buffer() {
        let mut value = "before".to_string();
        assert!(push_str(&mut value, &"x".repeat(TEXT_BYTES), "rendered output").is_err());
        assert_eq!(value, "before");
    }
}
