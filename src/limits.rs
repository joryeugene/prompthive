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
        if bytes.len() + count > bytes.capacity() {
            let capacity = bytes
                .capacity()
                .saturating_mul(2)
                .max(8192)
                .max(bytes.len() + count)
                .min(limit);
            bytes
                .try_reserve_exact(capacity - bytes.len())
                .context("cannot allocate bounded input buffer")?;
        }
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
    if size > target.capacity() {
        let capacity = target
            .capacity()
            .saturating_mul(2)
            .max(size)
            .min(TEXT_BYTES);
        target
            .try_reserve_exact(capacity - target.len())
            .context("cannot allocate bounded output buffer")?;
    }
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
        if self.bytes.len() + bytes.len() > self.bytes.capacity() {
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
        }
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

/// Walk expanded scalars and containers without constructing Values. YAML aliases
/// can repeat a large scalar many times while their source remains below 1 MiB.
pub(crate) fn validate_yaml_expansion(yaml: &str) -> Result<()> {
    use serde::de::{DeserializeSeed, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor};
    struct State {
        budget: Budget,
        exceeded: bool,
    }
    impl State {
        fn charge<E: serde::de::Error>(&mut self, bytes: usize) -> std::result::Result<(), E> {
            if self.budget.add(bytes).is_err() {
                self.exceeded = true;
                return Err(E::custom("expanded YAML data exceeds the safety budget"));
            }
            Ok(())
        }
    }
    struct Counter<'a>(&'a mut State);
    impl<'de> DeserializeSeed<'de> for Counter<'_> {
        type Value = ();
        fn deserialize<D: serde::Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> std::result::Result<(), D::Error> {
            // Reserve for Values, map nodes and the metadata copies used by the
            // existing parser. This is a conservative data budget, not an allocator sandbox.
            self.0
                .charge::<D::Error>(4 * std::mem::size_of::<serde_yaml::Value>())?;
            deserializer.deserialize_any(self)
        }
    }
    impl<'de> Visitor<'de> for Counter<'_> {
        type Value = ();
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("bounded YAML data")
        }
        fn visit_bool<E: serde::de::Error>(self, _: bool) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_i64<E: serde::de::Error>(self, _: i64) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_u64<E: serde::de::Error>(self, _: u64) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_f64<E: serde::de::Error>(self, _: f64) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_none<E: serde::de::Error>(self) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_some<D: serde::Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> std::result::Result<(), D::Error> {
            self.deserialize(deserializer)
        }
        fn visit_str<E: serde::de::Error>(self, value: &str) -> std::result::Result<(), E> {
            self.0.charge(value.len().saturating_mul(4))
        }
        fn visit_seq<A: SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> std::result::Result<(), A::Error> {
            while sequence.next_element_seed(Counter(self.0))?.is_some() {}
            Ok(())
        }
        fn visit_map<A: MapAccess<'de>>(self, mut mapping: A) -> std::result::Result<(), A::Error> {
            while mapping.next_key_seed(Counter(self.0))?.is_some() {
                mapping.next_value_seed(Counter(self.0))?;
            }
            Ok(())
        }
        fn visit_enum<A: EnumAccess<'de>>(self, data: A) -> std::result::Result<(), A::Error> {
            let ((), value) = data.variant_seed(Counter(self.0))?;
            value.newtype_variant_seed(Counter(self.0))
        }
    }
    let mut state = State {
        budget: Budget::new(TEXT_BYTES, "expanded YAML data"),
        exceeded: false,
    };
    let result = Counter(&mut state).deserialize(serde_yaml::Deserializer::from_str(yaml));
    if state.exceeded {
        return Err(exceeded("expanded YAML data", TEXT_BYTES));
    }
    result.context("invalid YAML frontmatter")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn yaml_preflight_accepts_tags_and_small_aliases_but_bounds_expanded_scalars() {
        validate_yaml_expansion("base: &a !custom {text: hello}\ncopy: *a\n").unwrap();
        let scalar = "x".repeat(512 * 1024);
        validate_yaml_expansion(&format!(
            "base: &large {scalar}\ncopies: [{}]\n",
            vec!["*large"; 14].join(", ")
        ))
        .unwrap();
        assert!(validate_yaml_expansion(&format!(
            "base: &large {scalar}\ncopies: [{}]\n",
            vec!["*large"; 15].join(", ")
        ))
        .unwrap_err()
        .downcast_ref::<LimitError>()
        .is_some());
        let huge = format!(
            "base: &large {}\ncopies: [{}]\n",
            "x".repeat(512 * 1024),
            vec!["*large"; 1000].join(", ")
        );
        assert!(validate_yaml_expansion(&huge)
            .unwrap_err()
            .downcast_ref::<LimitError>()
            .is_some());
    }
    #[test]
    fn repeated_small_chunks_do_not_reserve_the_whole_budget() {
        let bytes = read(
            std::io::Cursor::new(vec![b'x'; 128 * 1024]),
            TEXT_BYTES,
            "chunked input",
        )
        .unwrap();
        assert!(bytes.capacity() <= 256 * 1024, "{}", bytes.capacity());
        let mut text = String::new();
        for _ in 0..128 {
            push_str(&mut text, "x", "typed input").unwrap();
        }
        assert!(text.capacity() <= 256, "{}", text.capacity());
        let mut writer = Writer::new(FRONTMATTER_BYTES, "metadata");
        for _ in 0..128 {
            std::io::Write::write_all(&mut writer, b"x").unwrap();
        }
        assert!(
            writer.bytes.capacity() <= 8192,
            "{}",
            writer.bytes.capacity()
        );
    }
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
