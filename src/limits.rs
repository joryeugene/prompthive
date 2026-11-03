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
    unit: &'static str,
}
impl fmt::Display for LimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} exceeds the {}-{} safety limit; use smaller input (nothing was truncated)",
            self.label, self.limit, self.unit
        )
    }
}
impl std::error::Error for LimitError {}

pub fn exceeded(label: &str, limit: usize) -> anyhow::Error {
    LimitError {
        label: label.into(),
        limit,
        unit: "byte",
    }
    .into()
}
pub(crate) fn check_parts(count: usize) -> Result<()> {
    if count > TEMPLATE_PARTS {
        return Err(LimitError {
            label: "compiled template".into(),
            limit: TEMPLATE_PARTS,
            unit: "segment",
        }
        .into());
    }
    Ok(())
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
        let count = match reader.read(&mut chunk[..available]) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if count == 0 {
            return Ok(bytes);
        }
        let capacity = bytes
            .capacity()
            .saturating_mul(2)
            .max(8192)
            .max(bytes.len() + count)
            .min(limit);
        bytes
            .try_reserve_exact(capacity - bytes.len())
            .context("cannot allocate bounded input buffer")?;
        bytes.extend_from_slice(&chunk[..count]);
    }
    let extra = loop {
        match reader.read(&mut chunk[..1]) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => break result?,
        }
    };
    if extra != 0 {
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
    let capacity = target
        .capacity()
        .saturating_mul(2)
        .max(size)
        .min(TEXT_BYTES);
    target
        .try_reserve_exact(capacity - target.len())
        .context("cannot allocate bounded output buffer")?;
    target.push_str(text);
    Ok(())
}

pub struct Budget {
    used: usize,
    limit: usize,
    label: &'static str,
}

/// Serialize into a bounded buffer rather than allocating an unchecked YAML/JSON string.
pub(crate) struct Writer {
    bytes: Vec<u8>,
    limit: usize,
    label: &'static str,
    exceeded: bool,
}
impl Writer {
    pub(crate) fn new(limit: usize, label: &'static str) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
            label,
            exceeded: false,
        }
    }
    pub(crate) fn finish<E: Into<anyhow::Error>>(
        self,
        result: std::result::Result<(), E>,
    ) -> Result<Vec<u8>> {
        if self.exceeded {
            return Err(exceeded(self.label, self.limit));
        }
        result.map_err(Into::into)?;
        Ok(self.bytes)
    }
}
impl std::io::Write for Writer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|size| size > self.limit)
        {
            self.exceeded = true;
            return Err(std::io::Error::other("serialization exceeds safety limit"));
        }
        let capacity = self
            .bytes
            .capacity()
            .saturating_mul(2)
            .max(8192)
            .max(self.bytes.len() + bytes.len())
            .min(self.limit);
        self.bytes
            .try_reserve_exact(capacity - self.bytes.len())
            .map_err(std::io::Error::other)?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
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
