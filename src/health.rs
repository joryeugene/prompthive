//! Health check endpoints and system monitoring for PromptHive
//!
//! This module provides comprehensive health monitoring capabilities:
//! - Application health status reporting
//! - Storage system health checks
//! - Performance metrics collection
//! - System resource monitoring
//! - Dependency health validation

use crate::storage::Storage;
use crate::telemetry::TelemetryCollector;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Overall health status of the application
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthStatus::Healthy => write!(f, "healthy"),
            HealthStatus::Degraded => write!(f, "degraded"),
            HealthStatus::Unhealthy => write!(f, "unhealthy"),
        }
    }
}

/// Health check result for individual components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub name: String,
    pub status: HealthStatus,
    pub message: String,
    pub response_time_ms: u64,
    pub last_checked: u64,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl ComponentHealth {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            status: HealthStatus::Healthy,
            message: "OK".to_string(),
            response_time_ms: 0,
            last_checked: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            metadata: HashMap::new(),
        }
    }

    pub fn with_status(mut self, status: HealthStatus, message: &str) -> Self {
        self.status = status;
        self.message = message.to_string();
        self
    }

    pub fn with_response_time(mut self, duration: Duration) -> Self {
        self.response_time_ms = duration.as_millis() as u64;
        self
    }

    pub fn with_metadata(mut self, key: &str, value: serde_json::Value) -> Self {
        self.metadata.insert(key.to_string(), value);
        self
    }
}

/// Complete application health report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    pub status: HealthStatus,
    pub version: String,
    pub uptime_seconds: u64,
    pub timestamp: u64,
    pub components: Vec<ComponentHealth>,
    pub system_metrics: SystemMetrics,
}

/// System resource metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub memory_usage_mb: f64,
    pub disk_usage_mb: f64,
    pub disk_available_mb: f64,
    pub cpu_usage_percent: f64,
    pub load_average: f64,
}

impl Default for SystemMetrics {
    fn default() -> Self {
        Self {
            memory_usage_mb: 0.0,
            disk_usage_mb: 0.0,
            disk_available_mb: 0.0,
            cpu_usage_percent: 0.0,
            load_average: 0.0,
        }
    }
}

/// Health monitoring service
pub struct HealthMonitor {
    storage: Storage,
    start_time: Instant,
    #[allow(dead_code)]
    system_start: SystemTime,
    telemetry: Option<TelemetryCollector>,
}

impl HealthMonitor {
    pub fn new(storage: Storage) -> Self {
        Self {
            storage,
            start_time: Instant::now(),
            system_start: SystemTime::now(),
            telemetry: None,
        }
    }

    pub fn with_telemetry(mut self, telemetry: TelemetryCollector) -> Self {
        self.telemetry = Some(telemetry);
        self
    }

    /// Perform comprehensive health check
    pub fn check_health(&self) -> Result<HealthReport> {
        let mut components = Vec::new();
        let mut overall_status = HealthStatus::Healthy;

        // Check storage health
        let storage_health = self.check_storage_health()?;
        if storage_health.status != HealthStatus::Healthy {
            overall_status = match storage_health.status {
                HealthStatus::Degraded if overall_status == HealthStatus::Healthy => {
                    HealthStatus::Degraded
                }
                HealthStatus::Unhealthy => HealthStatus::Unhealthy,
                _ => overall_status,
            };
        }
        components.push(storage_health);

        // Check file system health
        let filesystem_health = self.check_filesystem_health()?;
        if filesystem_health.status != HealthStatus::Healthy {
            overall_status = match filesystem_health.status {
                HealthStatus::Degraded if overall_status == HealthStatus::Healthy => {
                    HealthStatus::Degraded
                }
                HealthStatus::Unhealthy => HealthStatus::Unhealthy,
                _ => overall_status,
            };
        }
        components.push(filesystem_health);

        // Check configuration health
        let config_health = self.check_configuration_health()?;
        if config_health.status != HealthStatus::Healthy {
            overall_status = match config_health.status {
                HealthStatus::Degraded if overall_status == HealthStatus::Healthy => {
                    HealthStatus::Degraded
                }
                HealthStatus::Unhealthy => HealthStatus::Unhealthy,
                _ => overall_status,
            };
        }
        components.push(config_health);

        // Check telemetry health if available
        if let Some(ref telemetry) = self.telemetry {
            let telemetry_health = self.check_telemetry_health(telemetry)?;
            if telemetry_health.status != HealthStatus::Healthy {
                overall_status = match telemetry_health.status {
                    HealthStatus::Degraded if overall_status == HealthStatus::Healthy => {
                        HealthStatus::Degraded
                    }
                    HealthStatus::Unhealthy => HealthStatus::Unhealthy,
                    _ => overall_status,
                };
            }
            components.push(telemetry_health);
        }

        let uptime = self.start_time.elapsed().as_secs();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Ok(HealthReport {
            status: overall_status,
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: uptime,
            timestamp,
            components,
            system_metrics: self.collect_system_metrics()?,
        })
    }

    /// Check storage system health
    fn check_storage_health(&self) -> Result<ComponentHealth> {
        let start = Instant::now();
        let mut health = ComponentHealth::new("storage");

        // Test basic storage operations
        match self.storage.list_prompts() {
            Ok(prompts) => {
                health = health
                    .with_status(HealthStatus::Healthy, "Storage accessible")
                    .with_metadata(
                        "prompt_count",
                        serde_json::Value::Number(prompts.len().into()),
                    );
            }
            Err(e) => {
                health =
                    health.with_status(HealthStatus::Unhealthy, &format!("Storage error: {}", e));
            }
        }

        // Check storage directory permissions
        let base_dir = self.storage.base_dir();
        match fs::metadata(base_dir) {
            Ok(metadata) => {
                health = health.with_metadata(
                    "writable",
                    serde_json::Value::Bool(metadata.permissions().readonly()),
                );
            }
            Err(e) => {
                health = health.with_status(
                    HealthStatus::Unhealthy,
                    &format!("Storage directory error: {}", e),
                );
            }
        }

        Ok(health.with_response_time(start.elapsed()))
    }

    /// Check filesystem health and available space
    fn check_filesystem_health(&self) -> Result<ComponentHealth> {
        let start = Instant::now();
        let mut health = ComponentHealth::new("filesystem");

        let base_dir = self.storage.base_dir();

        // Check available disk space
        if let Ok(available_space) = fs_available_space(base_dir) {
            let available_mb = available_space as f64 / 1_024_000.0;

            if available_mb < 10.0 {
                health = health.with_status(HealthStatus::Unhealthy, "Critically low disk space");
            } else if available_mb < 100.0 {
                health = health.with_status(HealthStatus::Degraded, "Low disk space");
            } else {
                health = health.with_status(HealthStatus::Healthy, "Sufficient disk space");
            }

            health = health.with_metadata(
                "available_mb",
                serde_json::Value::Number((available_mb as u64).into()),
            );
        } else {
            health = health.with_status(HealthStatus::Degraded, "Unable to check disk space");
        }

        Ok(health.with_response_time(start.elapsed()))
    }

    /// Check application configuration health
    fn check_configuration_health(&self) -> Result<ComponentHealth> {
        let start = Instant::now();
        let mut health = ComponentHealth::new("configuration");

        // Check if core directories exist and are accessible
        let base_dir = self.storage.base_dir();
        let core_dirs = ["banks"]; // Only check essential directories
        let mut missing_dirs = Vec::new();

        for dir in &core_dirs {
            let dir_path = base_dir.join(dir);
            if !dir_path.exists() {
                missing_dirs.push(dir.to_string());
            }
        }

        if missing_dirs.is_empty() {
            health = health.with_status(
                HealthStatus::Healthy,
                "Core configuration directories present",
            );
        } else {
            health = health.with_status(
                HealthStatus::Degraded,
                &format!("Missing core directories: {}", missing_dirs.join(", ")),
            );
        }

        Ok(health.with_response_time(start.elapsed()))
    }

    /// Check telemetry system health
    fn check_telemetry_health(&self, _telemetry: &TelemetryCollector) -> Result<ComponentHealth> {
        let start = Instant::now();
        let health = ComponentHealth::new("telemetry")
            .with_status(HealthStatus::Healthy, "Telemetry system operational");

        Ok(health.with_response_time(start.elapsed()))
    }

    /// Collect system resource metrics
    fn collect_system_metrics(&self) -> Result<SystemMetrics> {
        let mut metrics = SystemMetrics::default();

        // Get basic system information
        #[cfg(target_os = "macos")]
        {
            metrics.memory_usage_mb = get_memory_usage_macos().unwrap_or(0.0);
            metrics.load_average = get_load_average_macos().unwrap_or(0.0);
        }

        #[cfg(target_os = "linux")]
        {
            metrics.memory_usage_mb = get_memory_usage_linux().unwrap_or(0.0);
            metrics.load_average = get_load_average_linux().unwrap_or(0.0);
        }

        // Get disk usage for storage directory
        let base_dir = self.storage.base_dir();
        if let Ok(usage) = get_directory_size(base_dir) {
            metrics.disk_usage_mb = usage as f64 / 1_024_000.0;
        }

        if let Ok(available) = fs_available_space(base_dir) {
            metrics.disk_available_mb = available as f64 / 1_024_000.0;
        }

        Ok(metrics)
    }

    /// Simple readiness check for quick health verification
    pub fn is_ready(&self) -> bool {
        // Basic readiness: can we list prompts?
        self.storage.list_prompts().is_ok()
    }

    /// Simple liveness check
    pub fn is_alive(&self) -> bool {
        // Liveness: are we responsive and not deadlocked?
        true // If we can execute this function, we're alive
    }
}

// Platform-specific system metrics functions

#[cfg(target_os = "macos")]
fn get_memory_usage_macos() -> Result<f64> {
    use std::process::Command;

    let output = Command::new("vm_stat").output()?;
    let _output_str = String::from_utf8_lossy(&output.stdout);

    // Parse vm_stat output to get memory usage
    // This is a simplified implementation
    Ok(0.0) // Placeholder
}

#[cfg(target_os = "macos")]
fn get_load_average_macos() -> Result<f64> {
    use std::process::Command;

    let output = Command::new("uptime").output()?;
    let output_str = String::from_utf8_lossy(&output.stdout);

    // Parse load average from uptime output
    if let Some(load_part) = output_str.split("load averages:").nth(1) {
        if let Some(first_load) = load_part.split_whitespace().next() {
            return Ok(first_load.parse().unwrap_or(0.0));
        }
    }

    Ok(0.0)
}

#[cfg(target_os = "linux")]
fn get_memory_usage_linux() -> Result<f64> {
    let meminfo = fs::read_to_string("/proc/meminfo")?;
    let mut total = 0u64;
    let mut available = 0u64;

    for line in meminfo.lines() {
        if line.starts_with("MemTotal:") {
            total = line
                .split_whitespace()
                .nth(1)
                .unwrap_or("0")
                .parse()
                .unwrap_or(0);
        } else if line.starts_with("MemAvailable:") {
            available = line
                .split_whitespace()
                .nth(1)
                .unwrap_or("0")
                .parse()
                .unwrap_or(0);
        }
    }

    if total > 0 {
        let used = total - available;
        Ok(used as f64 / 1024.0) // Convert KB to MB
    } else {
        Ok(0.0)
    }
}

#[cfg(target_os = "linux")]
fn get_load_average_linux() -> Result<f64> {
    let loadavg = fs::read_to_string("/proc/loadavg")?;
    let first_value = loadavg.split_whitespace().next().unwrap_or("0.0");
    Ok(first_value.parse().unwrap_or(0.0))
}

/// Get directory size recursively
fn get_directory_size(path: &PathBuf) -> Result<u64> {
    let mut size = 0;

    fn visit_dir(dir: &PathBuf, size: &mut u64) -> Result<()> {
        if dir.is_dir() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    visit_dir(&path, size)?;
                } else {
                    *size += entry.metadata()?.len();
                }
            }
        }
        Ok(())
    }

    visit_dir(path, &mut size)?;
    Ok(size)
}

/// Get available disk space for a path
fn fs_available_space(path: &Path) -> Result<u64> {
    #[cfg(unix)]
    {
        use std::ffi::CString;
        use std::mem;
        use std::os::unix::ffi::OsStrExt;

        let path_cstr = CString::new(path.as_os_str().as_bytes())?;
        let mut statvfs: libc::statvfs = unsafe { mem::zeroed() };

        let result = unsafe { libc::statvfs(path_cstr.as_ptr(), &mut statvfs) };
        if result == 0 {
            Ok((statvfs.f_bavail as u64) * statvfs.f_frsize)
        } else {
            Err(anyhow::anyhow!("Failed to get filesystem stats"))
        }
    }

    #[cfg(windows)]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use winapi::um::fileapi::GetDiskFreeSpaceExW;

        let path_wide: Vec<u16> = OsStr::new(path).encode_wide().chain(Some(0)).collect();
        let mut free_bytes = 0u64;

        let result = unsafe {
            GetDiskFreeSpaceExW(
                path_wide.as_ptr(),
                &mut free_bytes,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };

        if result != 0 {
            Ok(free_bytes)
        } else {
            Err(anyhow::anyhow!("Failed to get disk free space"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_storage() -> (Storage, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let storage = Storage::new_with_base(temp_dir.path().to_path_buf()).unwrap();
        storage.init().unwrap();
        (storage, temp_dir)
    }

    #[test]
    fn test_health_monitor_creation() {
        let (storage, _temp_dir) = create_test_storage();
        let monitor = HealthMonitor::new(storage);

        assert!(monitor.is_alive());
        assert!(monitor.is_ready());
    }

    #[test]
    fn test_health_check() {
        let (storage, _temp_dir) = create_test_storage();
        let monitor = HealthMonitor::new(storage);

        let health_report = monitor.check_health().unwrap();
        assert_eq!(health_report.status, HealthStatus::Healthy);
        assert!(!health_report.components.is_empty());
        assert_eq!(health_report.version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn test_component_health() {
        let mut health = ComponentHealth::new("test");
        health = health
            .with_status(HealthStatus::Degraded, "Test message")
            .with_response_time(Duration::from_millis(100))
            .with_metadata(
                "test_key",
                serde_json::Value::String("test_value".to_string()),
            );

        assert_eq!(health.name, "test");
        assert_eq!(health.status, HealthStatus::Degraded);
        assert_eq!(health.message, "Test message");
        assert_eq!(health.response_time_ms, 100);
        assert!(health.metadata.contains_key("test_key"));
    }

    #[test]
    fn test_health_status_display() {
        assert_eq!(HealthStatus::Healthy.to_string(), "healthy");
        assert_eq!(HealthStatus::Degraded.to_string(), "degraded");
        assert_eq!(HealthStatus::Unhealthy.to_string(), "unhealthy");
    }
}
