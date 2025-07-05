//! High-performance caching layer for PromptHive
//!
//! Provides memory and I/O optimization through intelligent caching of prompt
//! listings and metadata, reducing filesystem operations and improving performance.

use crate::storage::PromptMetadata;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

type CacheData = Arc<RwLock<Option<(Vec<String>, Instant)>>>;

#[derive(Clone)]
pub struct CacheEntry {
    pub metadata: PromptMetadata,
    pub content: String,
    pub last_accessed: Instant,
    pub access_count: u32,
}

pub struct PromptCache {
    entries: Arc<RwLock<HashMap<String, CacheEntry>>>,
    max_entries: usize,
    ttl: Duration,
}

impl PromptCache {
    pub fn new(max_entries: usize, ttl_seconds: u64) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            max_entries,
            ttl: Duration::from_secs(ttl_seconds),
        }
    }

    pub fn get(&self, prompt_name: &str) -> Option<CacheEntry> {
        let mut entries = self.entries.write().ok()?;

        if let Some(entry) = entries.get_mut(prompt_name) {
            // Check if entry is still valid
            if entry.last_accessed.elapsed() < self.ttl {
                entry.last_accessed = Instant::now();
                entry.access_count += 1;
                return Some(entry.clone());
            } else {
                // Entry expired, remove it
                entries.remove(prompt_name);
            }
        }

        None
    }

    pub fn put(&self, prompt_name: String, metadata: PromptMetadata, content: String) {
        let mut entries = self.entries.write().ok().unwrap();

        // If at capacity, remove least recently used entry
        if entries.len() >= self.max_entries {
            self.evict_lru(&mut entries);
        }

        let entry = CacheEntry {
            metadata,
            content,
            last_accessed: Instant::now(),
            access_count: 1,
        };

        entries.insert(prompt_name, entry);
    }

    fn evict_lru(&self, entries: &mut HashMap<String, CacheEntry>) {
        if let Some((lru_key, _)) = entries
            .iter()
            .min_by_key(|(_, entry)| (entry.last_accessed, entry.access_count))
            .map(|(k, v)| (k.clone(), v.clone()))
        {
            entries.remove(&lru_key);
        }
    }

    pub fn clear(&self) {
        if let Ok(mut entries) = self.entries.write() {
            entries.clear();
        }
    }

    pub fn size(&self) -> usize {
        self.entries.read().map(|e| e.len()).unwrap_or(0)
    }

    pub fn hit_rate(&self) -> f64 {
        let entries = match self.entries.read() {
            Ok(entries) => entries,
            Err(_) => return 0.0,
        };

        let total_accesses: u32 = entries.values().map(|e| e.access_count).sum();
        let cache_hits = entries.len() as u32;

        if total_accesses > 0 {
            cache_hits as f64 / total_accesses as f64
        } else {
            0.0
        }
    }
}

// Directory listing cache for faster ls operations
pub struct DirectoryCache {
    cache: CacheData,
    ttl: Duration,
}

impl DirectoryCache {
    pub fn new(ttl_seconds: u64) -> Self {
        Self {
            cache: Arc::new(RwLock::new(None)),
            ttl: Duration::from_secs(ttl_seconds),
        }
    }

    pub fn get(&self) -> Option<Vec<String>> {
        let cache = self.cache.read().ok()?;

        if let Some((entries, timestamp)) = cache.as_ref() {
            if timestamp.elapsed() < self.ttl {
                return Some(entries.clone());
            }
        }

        None
    }

    pub fn put(&self, entries: Vec<String>) {
        if let Ok(mut cache) = self.cache.write() {
            *cache = Some((entries, Instant::now()));
        }
    }

    pub fn invalidate(&self) {
        if let Ok(mut cache) = self.cache.write() {
            *cache = None;
        }
    }
}
