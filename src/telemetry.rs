// Production Telemetry - Opt-in Usage Metrics
// Iteration 13-8: Privacy-respecting analytics for PromptHive improvement

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    pub enabled: bool,
    pub anonymous_id: String,
    pub collect_performance: bool,
    pub collect_errors: bool,
    pub collect_usage: bool,
    pub upload_enabled: bool,
    pub last_upload: Option<u64>,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Opt-in by default
            anonymous_id: generate_anonymous_id(),
            collect_performance: true,
            collect_errors: true,
            collect_usage: true,
            upload_enabled: false,
            last_upload: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageMetric {
    pub timestamp: u64,
    pub command: String,
    pub duration_ms: u64,
    pub success: bool,
    pub prompt_count: Option<usize>,
    pub error_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetric {
    pub timestamp: u64,
    pub operation: String,
    pub duration_ms: u64,
    pub file_size_bytes: Option<u64>,
    pub prompt_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMetric {
    pub timestamp: u64,
    pub command: String,
    pub error_type: String,
    pub error_category: String, // e.g., "file_not_found", "permission_denied"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryData {
    pub config: TelemetryConfig,
    pub usage_metrics: Vec<UsageMetric>,
    pub performance_metrics: Vec<PerformanceMetric>,
    pub error_metrics: Vec<ErrorMetric>,
    pub summary: TelemetrySummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetrySummary {
    pub total_commands: u64,
    pub total_prompts_created: u64,
    pub total_prompts_used: u64,
    pub average_response_time_ms: f64,
    pub most_used_commands: HashMap<String, u64>,
    pub error_rate_percent: f64,
    pub session_count: u64,
    pub first_use: Option<u64>,
    pub last_use: Option<u64>,
    pub daily_activity: HashMap<String, DailyActivity>, // Date string -> activity
    pub current_streak: u32,
    pub longest_streak: u32,
    pub total_time_saved_seconds: u64,
    pub achievements: Vec<Achievement>,
    pub daily_challenges: Vec<DailyChallenge>,
    pub milestones: Vec<DailyMilestone>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyActivity {
    pub date: String, // YYYY-MM-DD format
    pub command_count: u32,
    pub prompts_created: u32,
    pub prompts_used: u32,
    pub time_saved_seconds: u32,
    pub errors: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyChallenge {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub target: u32,
    pub current: u32,
    pub completed: bool,
    pub reward_xp: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyMilestone {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub target: u32,
    pub current: u32,
    pub completed: bool,
    pub completion_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Achievement {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub unlocked_at: Option<u64>,
    pub progress: u32,
    pub target: u32,
}

impl Default for TelemetrySummary {
    fn default() -> Self {
        Self {
            total_commands: 0,
            total_prompts_created: 0,
            total_prompts_used: 0,
            average_response_time_ms: 0.0,
            most_used_commands: HashMap::new(),
            error_rate_percent: 0.0,
            session_count: 0,
            first_use: None,
            last_use: None,
            daily_activity: HashMap::new(),
            current_streak: 0,
            longest_streak: 0,
            total_time_saved_seconds: 0,
            achievements: Self::initialize_achievements(),
            daily_challenges: Self::initialize_daily_challenges(),
            milestones: Self::initialize_milestones(),
        }
    }
}

impl TelemetrySummary {
    fn initialize_achievements() -> Vec<Achievement> {
        vec![
            Achievement {
                id: "first_prompt".to_string(),
                name: "First Steps".to_string(),
                description: "Create your first prompt".to_string(),
                icon: "🌱".to_string(),
                unlocked_at: None,
                progress: 0,
                target: 1,
            },
            Achievement {
                id: "speed_demon".to_string(),
                name: "Speed Demon".to_string(),
                description: "All commands under 50ms average".to_string(),
                icon: "⚡".to_string(),
                unlocked_at: None,
                progress: 0,
                target: 1,
            },
            Achievement {
                id: "prompt_creator".to_string(),
                name: "Prompt Creator".to_string(),
                description: "Create 10 prompts".to_string(),
                icon: "✨".to_string(),
                unlocked_at: None,
                progress: 0,
                target: 10,
            },
            Achievement {
                id: "heavy_user".to_string(),
                name: "Heavy User".to_string(),
                description: "Use prompts 50 times".to_string(),
                icon: "🔥".to_string(),
                unlocked_at: None,
                progress: 0,
                target: 50,
            },
            Achievement {
                id: "command_master".to_string(),
                name: "Command Master".to_string(),
                description: "Execute 100 commands".to_string(),
                icon: "👑".to_string(),
                unlocked_at: None,
                progress: 0,
                target: 100,
            },
            Achievement {
                id: "week_warrior".to_string(),
                name: "Week Warrior".to_string(),
                description: "7 day streak".to_string(),
                icon: "🗓️".to_string(),
                unlocked_at: None,
                progress: 0,
                target: 7,
            },
            Achievement {
                id: "month_master".to_string(),
                name: "Month Master".to_string(),
                description: "30 day streak".to_string(),
                icon: "📅".to_string(),
                unlocked_at: None,
                progress: 0,
                target: 30,
            },
            Achievement {
                id: "compose_wizard".to_string(),
                name: "Compose Wizard".to_string(),
                description: "Chain 50 prompts with compose".to_string(),
                icon: "🔗".to_string(),
                unlocked_at: None,
                progress: 0,
                target: 50,
            },
            Achievement {
                id: "time_saver".to_string(),
                name: "Time Saver".to_string(),
                description: "Save 1 hour of time".to_string(),
                icon: "⏰".to_string(),
                unlocked_at: None,
                progress: 0,
                target: 3600,
            },
            Achievement {
                id: "productivity_guru".to_string(),
                name: "Productivity Guru".to_string(),
                description: "Save 10 hours of time".to_string(),
                icon: "🚀".to_string(),
                unlocked_at: None,
                progress: 0,
                target: 36000,
            },
        ]
    }

    fn initialize_daily_challenges() -> Vec<DailyChallenge> {
        vec![
            DailyChallenge {
                id: "daily_explorer".to_string(),
                name: "Daily Explorer".to_string(),
                description: "Use 3 different prompts today".to_string(),
                icon: "🧭".to_string(),
                target: 3,
                current: 0,
                completed: false,
                reward_xp: 50,
            },
            DailyChallenge {
                id: "efficiency_master".to_string(),
                name: "Efficiency Master".to_string(),
                description: "Save 2 minutes of time today".to_string(),
                icon: "⚡".to_string(),
                target: 120, // 2 minutes in seconds
                current: 0,
                completed: false,
                reward_xp: 75,
            },
            DailyChallenge {
                id: "command_runner".to_string(),
                name: "Command Runner".to_string(),
                description: "Execute 10 commands today".to_string(),
                icon: "🏃".to_string(),
                target: 10,
                current: 0,
                completed: false,
                reward_xp: 40,
            },
        ]
    }

    fn initialize_milestones() -> Vec<DailyMilestone> {
        vec![
            DailyMilestone {
                id: "first_week".to_string(),
                name: "First Week".to_string(),
                description: "Complete 7 days of activity".to_string(),
                icon: "📅".to_string(),
                target: 7,
                current: 0,
                completed: false,
                completion_date: None,
            },
            DailyMilestone {
                id: "power_user".to_string(),
                name: "Power User".to_string(),
                description: "Reach 100 total commands".to_string(),
                icon: "💪".to_string(),
                target: 100,
                current: 0,
                completed: false,
                completion_date: None,
            },
            DailyMilestone {
                id: "time_master".to_string(),
                name: "Time Master".to_string(),
                description: "Save 1 hour total".to_string(),
                icon: "⏰".to_string(),
                target: 3600, // 1 hour in seconds
                current: 0,
                completed: false,
                completion_date: None,
            },
        ]
    }
}

pub struct TelemetryCollector {
    data_path: PathBuf,
    data: TelemetryData,
}

impl TelemetryCollector {
    pub fn new(base_dir: PathBuf) -> Result<Self> {
        let data_path = base_dir.join("telemetry.json");
        let data = if data_path.exists() {
            Self::load_data(&data_path)?
        } else {
            TelemetryData {
                config: TelemetryConfig::default(),
                usage_metrics: Vec::new(),
                performance_metrics: Vec::new(),
                error_metrics: Vec::new(),
                summary: TelemetrySummary::default(),
            }
        };

        Ok(Self { data_path, data })
    }

    pub fn is_enabled(&self) -> bool {
        self.data.config.enabled
    }

    pub fn enable_telemetry(&mut self, enable: bool) -> Result<()> {
        self.data.config.enabled = enable;
        if enable {
            println!("✅ Telemetry enabled - helping improve PromptHive");
            println!("   Anonymous ID: {}", &self.data.config.anonymous_id[..8]);
            println!("   Data stored locally at: {}", self.data_path.display());
            println!("   Use 'ph config telemetry disable' to turn off");
        } else {
            println!("🔒 Telemetry disabled - no data will be collected");
        }
        self.save_data()
    }

    pub fn record_command(
        &mut self,
        command: &str,
        duration: Duration,
        success: bool,
        prompt_count: Option<usize>,
        error_type: Option<String>,
    ) -> Result<()> {
        if !self.is_enabled() {
            return Ok(());
        }

        let timestamp = current_timestamp();
        let duration_ms = duration.as_millis() as u64;

        // Exclude blocking operations from performance metrics
        let is_blocking_operation = matches!(command, "new" | "edit" | "delete" | "tui");
        let performance_duration = if is_blocking_operation {
            None
        } else {
            Some(duration_ms)
        };

        // Record usage metric
        if self.data.config.collect_usage {
            self.data.usage_metrics.push(UsageMetric {
                timestamp,
                command: command.to_string(),
                duration_ms,
                success,
                prompt_count,
                error_type: error_type.clone(),
            });
        }

        // Record error metric if there was an error
        if let Some(error_type) = error_type {
            if self.data.config.collect_errors {
                self.data.error_metrics.push(ErrorMetric {
                    timestamp,
                    command: command.to_string(),
                    error_type: error_type.clone(),
                    error_category: categorize_error(&error_type),
                });
            }
        }

        // Update summary (use performance duration for non-blocking operations)
        self.update_summary(
            command,
            performance_duration.unwrap_or(duration_ms),
            success,
            prompt_count,
            is_blocking_operation,
        );

        // Clean old data periodically (keep last 30 days)
        self.cleanup_old_data()?;

        self.save_data()
    }

    pub fn record_performance(
        &mut self,
        operation: &str,
        duration: Duration,
        file_size: Option<u64>,
        prompt_count: Option<usize>,
    ) -> Result<()> {
        if !self.is_enabled() || !self.data.config.collect_performance {
            return Ok(());
        }

        self.data.performance_metrics.push(PerformanceMetric {
            timestamp: current_timestamp(),
            operation: operation.to_string(),
            duration_ms: duration.as_millis() as u64,
            file_size_bytes: file_size,
            prompt_count,
        });

        self.save_data()
    }

    pub fn get_summary(&self) -> &TelemetrySummary {
        &self.data.summary
    }

    pub fn show_stats(&self) {
        if !self.is_enabled() {
            println!("✗ Enable telemetry to unlock stats");
            println!("  ph config telemetry enable");
            return;
        }

        let summary = &self.data.summary;

        if summary.total_commands == 0 {
            println!("Welcome to PromptHive");
            println!("Try: ph use commit \"fix: add cool feature\"");
            return;
        }

        // Calculate metrics matching web dashboard format
        let commands_today = self.calculate_commands_today();
        let time_saved_minutes = (commands_today as f64 * 0.5).round() as u32;
        let time_saved_str = if time_saved_minutes >= 60 {
            format!("{:.1} hours", time_saved_minutes as f64 / 60.0)
        } else {
            format!("{} minutes", time_saved_minutes)
        };
        let streak_days = std::cmp::min(self.calculate_streak_days(), 7);
        let (level, _title, xp, _) = self.calculate_level_and_xp();

        // Linear-style single line metrics (matching web dashboard exactly)
        println!(
            "{} commands today • {} saved • {} day streak • Level {}",
            commands_today, time_saved_str, streak_days, level
        );
        println!();

        // Progress indicators with labels (matching SPEC format)
        let streak_progress = self.create_minimal_progress_bar(streak_days, 7);
        let level_progress = self.create_minimal_progress_bar(xp % 500, 500);
        let (activity_symbol, activity_level) = if commands_today > 20 {
            ("●●●", "High")
        } else if commands_today > 5 {
            ("●●○", "Medium")
        } else {
            ("●○○", "Low")
        };

        println!(
            "Streak progress    {} {}% to 7-day goal",
            streak_progress,
            (streak_days * 100 / 7)
        );
        println!(
            "Level progress     {} {}% to Level {}",
            level_progress,
            ((xp % 500) * 100 / 500),
            level + 1
        );
        println!(
            "Activity level     {}                   {}",
            activity_symbol, activity_level
        );
        println!();

        // 7-day contribution graph
        self.show_minimal_contribution_graph();

        // Linear-style footer with quick actions
        println!();
        println!("Quick actions: ph tui • ph web • ph --help");
    }

    #[allow(dead_code)]
    fn show_daily_challenges(&self) {
        let summary = &self.data.summary;

        println!();
        println!("🎯 Daily Challenges:");

        let completed_count = summary
            .daily_challenges
            .iter()
            .filter(|c| c.completed)
            .count();
        let total_count = summary.daily_challenges.len();

        if completed_count == total_count {
            println!("  🎉 All challenges completed! Come back tomorrow for new ones.");
        } else {
            for challenge in &summary.daily_challenges {
                let status = if challenge.completed {
                    "✅".to_string()
                } else {
                    format!("{}/{}", challenge.current, challenge.target)
                };

                let progress_bar = if challenge.completed {
                    "[==========] 100%".to_string()
                } else {
                    self.create_progress_bar(challenge.current, challenge.target)
                };

                println!(
                    "  {} {} - {} {}",
                    challenge.icon, challenge.name, challenge.description, status
                );

                if !challenge.completed {
                    println!("    {}", progress_bar);
                }
            }
        }
    }

    #[allow(dead_code)]
    fn show_milestones(&self) {
        let summary = &self.data.summary;

        println!();
        println!("🏆 Milestones:");

        let completed_milestones: Vec<_> =
            summary.milestones.iter().filter(|m| m.completed).collect();

        if !completed_milestones.is_empty() {
            println!("  Achieved:");
            for milestone in completed_milestones.iter().take(3) {
                println!(
                    "    {} {} - {}",
                    milestone.icon, milestone.name, milestone.description
                );
            }
            println!();
        }

        // Show next milestone to achieve
        if let Some(next) = summary
            .milestones
            .iter()
            .filter(|m| !m.completed)
            .max_by_key(|m| (m.current as f32 / m.target as f32 * 100.0) as u32)
        {
            let percent = (next.current as f32 / next.target as f32 * 100.0) as u32;
            println!("  Next: {} {} ({}%)", next.icon, next.name, percent);
            let progress_bar = self.create_progress_bar(next.current, next.target);
            println!("        {}", progress_bar);
        }
    }

    fn calculate_streak_days(&self) -> u32 {
        self.data.summary.current_streak
    }

    fn calculate_commands_today(&self) -> u32 {
        let today = get_today_date_string();
        self.data
            .summary
            .daily_activity
            .get(&today)
            .map(|activity| activity.command_count)
            .unwrap_or(0)
    }

    fn calculate_level_and_xp(&self) -> (u32, &str, u32, u32) {
        let total_xp =
            self.data.summary.total_commands * 10 + self.data.summary.total_prompts_created * 25;
        let level = total_xp / 500 + 1;

        let title = match level {
            1 => "Prompt Newbie",
            2 => "Command Explorer",
            3 => "Workflow Builder",
            4 => "Automation Ace",
            5 => "Prompt Master",
            6 => "CLI Wizard",
            7 => "Productivity Guru",
            8 => "Efficiency Expert",
            9 => "Prompt Ninja",
            _ => "PromptHive Legend",
        };

        (level as u32, title, total_xp as u32, 500)
    }

    #[allow(dead_code)]
    fn create_progress_bar(&self, current: u32, max: u32) -> String {
        let progress = (current as f32 / max as f32 * 10.0) as usize;
        let filled = "=".repeat(progress);
        let empty = "-".repeat(10 - progress);
        format!("[{}{}]", filled, empty)
    }

    fn create_minimal_progress_bar(&self, current: u32, max: u32) -> String {
        let percentage = if max > 0 {
            current as f64 / max as f64
        } else {
            0.0
        };
        let filled = (percentage * 20.0).round() as usize;
        let empty = 20 - filled;
        format!("{}{}", "█".repeat(filled), "░".repeat(empty))
    }

    #[allow(dead_code)]
    fn show_mini_contribution_graph(&self) {
        let mut dates = Vec::new();
        let today = chrono::Local::now().date_naive();

        // Get last 7 days of activity
        for i in 0..7 {
            let date = today - chrono::Duration::days(6 - i);
            let date_str = date.format("%Y-%m-%d").to_string();
            dates.push(date_str);
        }

        print!("  ");
        for date in &dates {
            let activity = self.data.summary.daily_activity.get(date);
            let level = match activity {
                Some(a) if a.command_count >= 20 => "🟩", // Dark green
                Some(a) if a.command_count >= 10 => "🟨", // Green
                Some(a) if a.command_count >= 5 => "🟧",  // Yellow
                Some(a) if a.command_count >= 1 => "🟦",  // Light blue
                _ => "⬜",                                // White/empty
            };
            print!("{} ", level);
        }
        println!();
    }

    fn show_minimal_contribution_graph(&self) {
        let mut dates = Vec::new();
        let today = chrono::Local::now().date_naive();

        // Get last 7 days of activity
        for i in 0..7 {
            let date = today - chrono::Duration::days(6 - i);
            let date_str = date.format("%Y-%m-%d").to_string();
            dates.push(date_str);
        }

        for date in &dates {
            let activity = self.data.summary.daily_activity.get(date);
            let level = match activity {
                Some(a) if a.command_count >= 20 => "●", // High activity (>20 commands)
                Some(a) if a.command_count >= 5 => "○",  // Some activity (5-20 commands)
                Some(a) if a.command_count >= 1 => "·",  // Low activity (1-5 commands)
                _ => "·",                                // No activity
            };
            print!("{} ", level);
        }
        println!();
    }

    pub fn export_data(&self, output_path: Option<PathBuf>) -> Result<()> {
        if !self.is_enabled() {
            return Err(anyhow::anyhow!("Telemetry disabled - no data to export"));
        }

        let export_path = output_path.unwrap_or_else(|| {
            PathBuf::from(format!("prompthive-telemetry-{}.json", current_timestamp()))
        });

        let export_data = serde_json::to_string_pretty(&self.data)?;
        fs::write(&export_path, export_data)?;

        println!("📤 Telemetry data exported to: {}", export_path.display());
        Ok(())
    }

    fn update_summary(
        &mut self,
        command: &str,
        duration_ms: u64,
        success: bool,
        _prompt_count: Option<usize>,
        is_blocking: bool,
    ) {
        // Calculate counts before mutable borrow
        let blocking_count = self.count_blocking_commands();

        let summary = &mut self.data.summary;

        summary.total_commands += 1;
        summary.session_count += 1;

        let timestamp = current_timestamp();
        if summary.first_use.is_none() {
            summary.first_use = Some(timestamp);
        }
        summary.last_use = Some(timestamp);

        // Update command frequency
        *summary
            .most_used_commands
            .entry(command.to_string())
            .or_insert(0) += 1;

        // Update average response time (exclude blocking operations)
        if !is_blocking {
            // Count non-blocking commands including the one we're about to add
            let non_blocking_count = summary.total_commands - blocking_count;
            if non_blocking_count > 0 {
                // For the first non-blocking command, average is just the duration
                if non_blocking_count == 1 {
                    summary.average_response_time_ms = duration_ms as f64;
                } else {
                    let total_time =
                        summary.average_response_time_ms * (non_blocking_count - 1) as f64;
                    summary.average_response_time_ms =
                        (total_time + duration_ms as f64) / non_blocking_count as f64;
                }
            }
        }

        // Update error rate
        let error_count = self.data.error_metrics.len() as f64;
        summary.error_rate_percent = (error_count / summary.total_commands as f64) * 100.0;

        // Update daily activity
        let today = get_today_date_string();
        let daily = summary
            .daily_activity
            .entry(today.clone())
            .or_insert(DailyActivity {
                date: today,
                command_count: 0,
                prompts_created: 0,
                prompts_used: 0,
                time_saved_seconds: 0,
                errors: 0,
            });

        daily.command_count += 1;
        if !success {
            daily.errors += 1;
        }

        // Update prompt counters
        match command {
            "new" => {
                summary.total_prompts_created += 1;
                daily.prompts_created += 1;
            }
            "use" => {
                summary.total_prompts_used += 1;
                daily.prompts_used += 1;
            }
            _ => {}
        }

        // Update time saved based on command type
        let time_saved = estimate_time_saved(command);
        daily.time_saved_seconds += time_saved;
        summary.total_time_saved_seconds += time_saved as u64;

        // Update streaks
        self.update_streaks();

        // Update achievements
        self.update_achievements();

        // Update daily challenges
        self.update_daily_challenges();

        // Update milestones
        self.update_milestones();
    }

    fn update_streaks(&mut self) {
        let today = get_today_date_string();
        let yesterday = chrono::Local::now()
            .date_naive()
            .pred_opt()
            .unwrap_or(chrono::Local::now().date_naive())
            .format("%Y-%m-%d")
            .to_string();

        let summary = &mut self.data.summary;

        // Check if we have activity today
        let has_activity_today = summary.daily_activity.contains_key(&today);
        let had_activity_yesterday = summary.daily_activity.contains_key(&yesterday);

        if has_activity_today {
            if had_activity_yesterday || summary.current_streak == 0 {
                // Continue or start streak
                summary.current_streak = if had_activity_yesterday {
                    summary.current_streak + 1
                } else {
                    1
                };
            }
            // Update longest streak if needed
            if summary.current_streak > summary.longest_streak {
                summary.longest_streak = summary.current_streak;
            }
        } else if !had_activity_yesterday && summary.current_streak > 0 {
            // Streak broken (no activity yesterday or today)
            summary.current_streak = 0;
        }
    }

    fn update_achievements(&mut self) {
        // Calculate counts before mutable borrow
        let blocking_count = self.count_blocking_commands();

        let summary = &mut self.data.summary;
        let timestamp = current_timestamp();

        // Update achievement progress
        for achievement in &mut summary.achievements {
            if achievement.unlocked_at.is_some() {
                continue; // Already unlocked
            }

            let (progress, should_unlock) = match achievement.id.as_str() {
                "first_prompt" => {
                    let progress = summary.total_prompts_created.min(1) as u32;
                    (progress, progress >= achievement.target)
                }
                "speed_demon" => {
                    let non_blocking_count = summary.total_commands - blocking_count;
                    let is_fast =
                        summary.average_response_time_ms < 50.0 && non_blocking_count >= 10;
                    (if is_fast { 1 } else { 0 }, is_fast)
                }
                "prompt_creator" => {
                    let progress = summary.total_prompts_created as u32;
                    (progress, progress >= achievement.target)
                }
                "heavy_user" => {
                    let progress = summary.total_prompts_used as u32;
                    (progress, progress >= achievement.target)
                }
                "command_master" => {
                    let progress = summary.total_commands as u32;
                    (progress, progress >= achievement.target)
                }
                "week_warrior" => {
                    let progress = summary.current_streak;
                    (progress, progress >= achievement.target)
                }
                "month_master" => {
                    let progress = summary.current_streak;
                    (progress, progress >= achievement.target)
                }
                "compose_wizard" => {
                    let compose_count = summary.most_used_commands.get("compose").unwrap_or(&0);
                    let progress = *compose_count as u32;
                    (progress, progress >= achievement.target)
                }
                "time_saver" => {
                    let progress = summary.total_time_saved_seconds as u32;
                    (progress, progress >= achievement.target)
                }
                "productivity_guru" => {
                    let progress = summary.total_time_saved_seconds as u32;
                    (progress, progress >= achievement.target)
                }
                _ => (0, false),
            };

            achievement.progress = progress;
            if should_unlock {
                achievement.unlocked_at = Some(timestamp);
            }
        }
    }

    fn update_daily_challenges(&mut self) {
        let today = get_today_date_string();
        let summary = &mut self.data.summary;

        // Reset daily challenges if it's a new day
        let today_activity = summary.daily_activity.get(&today);
        if let Some(activity) = today_activity {
            for challenge in &mut summary.daily_challenges {
                if !challenge.completed {
                    challenge.current = match challenge.id.as_str() {
                        "daily_explorer" => {
                            // Count unique prompts used today (simplified - just count use commands)
                            activity.prompts_used
                        }
                        "efficiency_master" => activity.time_saved_seconds,
                        "command_runner" => activity.command_count,
                        _ => challenge.current,
                    };

                    // Check if challenge is completed
                    if challenge.current >= challenge.target {
                        challenge.completed = true;
                        // TODO: Award XP bonus for completing challenge
                    }
                }
            }
        }
    }

    fn update_milestones(&mut self) {
        let summary = &mut self.data.summary;
        let today = get_today_date_string();

        for milestone in &mut summary.milestones {
            if !milestone.completed {
                milestone.current = match milestone.id.as_str() {
                    "first_week" => summary.current_streak.min(7),
                    "power_user" => summary.total_commands as u32,
                    "time_master" => summary.total_time_saved_seconds as u32,
                    _ => milestone.current,
                };

                // Check if milestone is completed
                if milestone.current >= milestone.target {
                    milestone.completed = true;
                    milestone.completion_date = Some(today.clone());
                }
            }
        }
    }

    fn count_blocking_commands(&self) -> u64 {
        self.data
            .usage_metrics
            .iter()
            .filter(|m| matches!(m.command.as_str(), "new" | "edit" | "delete" | "tui"))
            .count() as u64
    }

    fn cleanup_old_data(&mut self) -> Result<()> {
        let cutoff = current_timestamp() - (30 * 24 * 60 * 60); // 30 days ago

        self.data.usage_metrics.retain(|m| m.timestamp > cutoff);
        self.data
            .performance_metrics
            .retain(|m| m.timestamp > cutoff);
        self.data.error_metrics.retain(|m| m.timestamp > cutoff);

        Ok(())
    }

    fn load_data(path: &PathBuf) -> Result<TelemetryData> {
        let content = fs::read_to_string(path)?;
        let data: TelemetryData = serde_json::from_str(&content)?;
        Ok(data)
    }

    fn save_data(&self) -> Result<()> {
        if let Some(parent) = self.data_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(&self.data)?;
        fs::write(&self.data_path, content)?;

        // Set secure permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&self.data_path)?.permissions();
            perms.set_mode(0o600); // Only owner can read/write
            fs::set_permissions(&self.data_path, perms)?;
        }

        Ok(())
    }
}

// Helper functions

fn generate_anonymous_id() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    current_timestamp().hash(&mut hasher);
    std::env::var("USER")
        .unwrap_or_else(|_| "anonymous".to_string())
        .hash(&mut hasher);

    // Add process ID for uniqueness within same millisecond
    std::process::id().hash(&mut hasher);

    // Add a random component
    use std::time::SystemTime;
    SystemTime::now()
        .elapsed()
        .unwrap_or_default()
        .as_nanos()
        .hash(&mut hasher);

    format!("{:x}", hasher.finish())
}

fn get_today_date_string() -> String {
    chrono::Local::now()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string()
}

pub fn format_time_saved(seconds: u64) -> String {
    if seconds < 60 {
        format!("{} seconds", seconds)
    } else if seconds < 3600 {
        format!("{} minutes", seconds / 60)
    } else if seconds < 86400 {
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        if minutes > 0 {
            format!("{} hours {} minutes", hours, minutes)
        } else {
            format!("{} hours", hours)
        }
    } else {
        let days = seconds / 86400;
        let hours = (seconds % 86400) / 3600;
        if hours > 0 {
            format!("{} days {} hours", days, hours)
        } else {
            format!("{} days", days)
        }
    }
}

fn estimate_time_saved(command: &str) -> u32 {
    // Estimate time saved in seconds based on command type
    match command {
        "use" => 30,     // Using a prompt saves ~30 seconds vs typing
        "compose" => 60, // Composing saves ~1 minute vs manual chaining
        "new" => 20,     // Creating a prompt saves ~20 seconds for future use
        "clean" => 45,   // Cleaning text saves ~45 seconds of manual editing
        "batch" => 120,  // Batch operations save ~2 minutes
        "merge" => 40,   // Merging prompts saves ~40 seconds
        "import" => 90,  // Importing saves ~1.5 minutes of manual entry
        _ => 10,         // Default: 10 seconds saved
    }
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn categorize_error(error_type: &str) -> String {
    let error_lower = error_type.to_lowercase();

    if error_lower.contains("not found") || error_lower.contains("missing") {
        "resource_not_found".to_string()
    } else if error_lower.contains("permission") || error_lower.contains("denied") {
        "permission_error".to_string()
    } else if error_lower.contains("network") || error_lower.contains("connection") {
        "network_error".to_string()
    } else if error_lower.contains("parse") || error_lower.contains("invalid") {
        "parse_error".to_string()
    } else if error_lower.contains("io") || error_lower.contains("file") {
        "io_error".to_string()
    } else {
        "other_error".to_string()
    }
}

fn _format_timestamp(timestamp: u64) -> String {
    use chrono::DateTime;

    if let Some(datetime) = DateTime::from_timestamp(timestamp as i64, 0) {
        datetime.format("%Y-%m-%d %H:%M UTC").to_string()
    } else {
        "Unknown".to_string()
    }
}

// Public convenience functions for main.rs integration

pub fn init_telemetry(base_dir: PathBuf) -> Result<TelemetryCollector> {
    TelemetryCollector::new(base_dir)
}

pub fn record_command_metric(
    collector: &mut Option<TelemetryCollector>,
    command: &str,
    duration: Duration,
    success: bool,
    prompt_count: Option<usize>,
    error_type: Option<String>,
) {
    if let Some(ref mut collector) = collector {
        if let Err(e) =
            collector.record_command(command, duration, success, prompt_count, error_type)
        {
            eprintln!("Warning: Failed to record telemetry: {}", e);
        }
    }
}

pub fn record_performance_metric(
    collector: &mut Option<TelemetryCollector>,
    operation: &str,
    duration: Duration,
    file_size: Option<u64>,
    prompt_count: Option<usize>,
) {
    if let Some(ref mut collector) = collector {
        if let Err(e) = collector.record_performance(operation, duration, file_size, prompt_count) {
            eprintln!("Warning: Failed to record performance metric: {}", e);
        }
    }
}

pub fn generate_contribution_graph_html(summary: &TelemetrySummary) -> String {
    let mut html = String::from(
        r#"<div class="contribution-graph">
        <h3>📈 Contribution Activity</h3>
        <div class="graph-container">
"#,
    );

    // Calculate date range (last 52 weeks)
    let today = chrono::Local::now().date_naive();
    let start_date = today - chrono::Duration::weeks(52);

    // Build week grid
    html.push_str("<div class=\"weeks\">\n");

    let mut current_date = start_date;
    let mut week_html = String::from("<div class=\"week\">\n");
    let mut day_count = 0;

    while current_date <= today {
        let date_str = current_date.format("%Y-%m-%d").to_string();
        let activity = summary.daily_activity.get(&date_str);

        let level = match activity {
            Some(a) if a.command_count >= 20 => 4,
            Some(a) if a.command_count >= 10 => 3,
            Some(a) if a.command_count >= 5 => 2,
            Some(a) if a.command_count >= 1 => 1,
            _ => 0,
        };

        let tooltip = match activity {
            Some(a) => format!("{}: {} commands", date_str, a.command_count),
            None => format!("{}: No activity", date_str),
        };

        week_html.push_str(&format!(
            r#"<div class="day level-{}" title="{}"></div>
"#,
            level, tooltip
        ));

        current_date = current_date.succ_opt().unwrap_or(current_date);
        day_count += 1;

        // Start new week every 7 days
        if day_count % 7 == 0 {
            week_html.push_str("</div>\n");
            html.push_str(&week_html);
            week_html = String::from("<div class=\"week\">\n");
        }
    }

    // Close any remaining week
    if day_count % 7 != 0 {
        week_html.push_str("</div>\n");
        html.push_str(&week_html);
    }

    html.push_str("</div>\n</div>\n");

    // Add CSS for the graph
    html.push_str(
        r#"
<style>
.contribution-graph {
    margin: 20px 0;
    padding: 20px;
    background: #f6f8fa;
    border-radius: 8px;
}

.graph-container {
    overflow-x: auto;
}

.weeks {
    display: flex;
    gap: 3px;
}

.week {
    display: flex;
    flex-direction: column;
    gap: 3px;
}

.day {
    width: 12px;
    height: 12px;
    border-radius: 2px;
    cursor: pointer;
}

.day.level-0 { background: #ebedf0; }
.day.level-1 { background: #9be9a8; }
.day.level-2 { background: #40c463; }
.day.level-3 { background: #30a14e; }
.day.level-4 { background: #216e39; }

.day:hover {
    outline: 1px solid #1b1f23;
    outline-offset: 1px;
}
</style>
"#,
    );

    html.push_str("</div>");
    html
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_telemetry_collector_creation() {
        let temp_dir = TempDir::new().unwrap();
        let collector = TelemetryCollector::new(temp_dir.path().to_path_buf()).unwrap();

        assert!(!collector.is_enabled()); // Should be disabled by default
        assert!(!collector.data.config.anonymous_id.is_empty());
    }

    #[test]
    fn test_telemetry_enable_disable() {
        let temp_dir = TempDir::new().unwrap();
        let mut collector = TelemetryCollector::new(temp_dir.path().to_path_buf()).unwrap();

        // Enable telemetry
        collector.enable_telemetry(true).unwrap();
        assert!(collector.is_enabled());

        // Disable telemetry
        collector.enable_telemetry(false).unwrap();
        assert!(!collector.is_enabled());
    }

    #[test]
    fn test_command_recording() {
        let temp_dir = TempDir::new().unwrap();
        let mut collector = TelemetryCollector::new(temp_dir.path().to_path_buf()).unwrap();

        // Enable telemetry
        collector.enable_telemetry(true).unwrap();

        // Record a successful command
        let duration = Duration::from_millis(25);
        collector
            .record_command("use", duration, true, Some(1), None)
            .unwrap();

        // Check summary was updated
        let summary = collector.get_summary();
        assert_eq!(summary.total_commands, 1);
        assert_eq!(summary.total_prompts_used, 1);
        assert_eq!(summary.average_response_time_ms, 25.0);
    }

    #[test]
    fn test_error_categorization() {
        assert_eq!(categorize_error("File not found"), "resource_not_found");
        assert_eq!(categorize_error("Permission denied"), "permission_error");
        assert_eq!(
            categorize_error("Network connection failed"),
            "network_error"
        );
        assert_eq!(categorize_error("Parse error in YAML"), "parse_error");
        assert_eq!(categorize_error("IO error reading file"), "io_error");
        assert_eq!(categorize_error("Unknown error"), "other_error");
    }

    #[test]
    fn test_performance_recording() {
        let temp_dir = TempDir::new().unwrap();
        let mut collector = TelemetryCollector::new(temp_dir.path().to_path_buf()).unwrap();

        collector.enable_telemetry(true).unwrap();

        let duration = Duration::from_millis(15);
        collector
            .record_performance("file_read", duration, Some(1024), None)
            .unwrap();

        assert_eq!(collector.data.performance_metrics.len(), 1);
        assert_eq!(collector.data.performance_metrics[0].operation, "file_read");
        assert_eq!(collector.data.performance_metrics[0].duration_ms, 15);
        assert_eq!(
            collector.data.performance_metrics[0].file_size_bytes,
            Some(1024)
        );
    }

    #[test]
    fn test_data_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let data_path = temp_dir.path().to_path_buf();

        // Create collector and record data
        {
            let mut collector = TelemetryCollector::new(data_path.clone()).unwrap();
            collector.enable_telemetry(true).unwrap();

            let duration = Duration::from_millis(30);
            collector
                .record_command("new", duration, true, None, None)
                .unwrap();
        }

        // Create new collector from same path
        {
            let collector = TelemetryCollector::new(data_path).unwrap();
            assert!(collector.is_enabled());
            assert_eq!(collector.get_summary().total_commands, 1);
            assert_eq!(collector.get_summary().total_prompts_created, 1);
        }
    }

    #[test]
    fn test_anonymous_id_generation() {
        let id1 = generate_anonymous_id();

        assert!(!id1.is_empty());
        assert!(id1.len() > 8); // Should be reasonably long hex string
        assert!(id1.chars().all(|c| c.is_ascii_hexdigit())); // Should be valid hex

        // Test that function doesn't panic and produces consistent output
        let id2 = generate_anonymous_id();
        assert!(!id2.is_empty());
        assert!(id2.len() > 8);

        // Note: IDs may be the same if called within same millisecond
        // This is acceptable behavior for anonymous ID generation
    }

    #[test]
    fn test_achievement_tracking() {
        let temp_dir = TempDir::new().unwrap();
        let mut collector = TelemetryCollector::new(temp_dir.path().to_path_buf()).unwrap();
        collector.enable_telemetry(true).unwrap();

        // Test first prompt achievement
        collector
            .record_command("new", Duration::from_millis(10), true, None, None)
            .unwrap();
        let summary = collector.get_summary();
        let first_prompt = summary
            .achievements
            .iter()
            .find(|a| a.id == "first_prompt")
            .unwrap();
        assert_eq!(first_prompt.progress, 1);
        assert!(first_prompt.unlocked_at.is_some());

        // Test speed demon achievement (need 10 commands under 50ms)
        for _ in 0..10 {
            collector
                .record_command("use", Duration::from_millis(25), true, None, None)
                .unwrap();
        }
        let summary = collector.get_summary();
        let speed_demon = summary
            .achievements
            .iter()
            .find(|a| a.id == "speed_demon")
            .unwrap();
        assert_eq!(speed_demon.progress, 1);
        assert!(speed_demon.unlocked_at.is_some());
    }

    #[test]
    fn test_daily_activity_tracking() {
        let temp_dir = TempDir::new().unwrap();
        let mut collector = TelemetryCollector::new(temp_dir.path().to_path_buf()).unwrap();
        collector.enable_telemetry(true).unwrap();

        // Record some commands
        collector
            .record_command("use", Duration::from_millis(30), true, None, None)
            .unwrap();
        collector
            .record_command("new", Duration::from_millis(20), true, None, None)
            .unwrap();
        collector
            .record_command("compose", Duration::from_millis(60), true, None, None)
            .unwrap();

        let summary = collector.get_summary();
        let today = get_today_date_string();
        let daily_activity = summary.daily_activity.get(&today).unwrap();

        assert_eq!(daily_activity.command_count, 3);
        assert_eq!(daily_activity.prompts_created, 1);
        assert_eq!(daily_activity.prompts_used, 1);
        assert_eq!(daily_activity.time_saved_seconds, 30 + 20 + 60); // Based on estimate_time_saved
    }

    #[test]
    fn test_streak_calculation() {
        let temp_dir = TempDir::new().unwrap();
        let mut collector = TelemetryCollector::new(temp_dir.path().to_path_buf()).unwrap();
        collector.enable_telemetry(true).unwrap();

        // First command starts streak
        collector
            .record_command("use", Duration::from_millis(30), true, None, None)
            .unwrap();
        let summary = collector.get_summary();
        assert_eq!(summary.current_streak, 1);
        assert_eq!(summary.longest_streak, 1);
    }

    #[test]
    fn test_time_saved_formatting() {
        assert_eq!(format_time_saved(30), "30 seconds");
        assert_eq!(format_time_saved(90), "1 minutes");
        assert_eq!(format_time_saved(3665), "1 hours 1 minutes");
        assert_eq!(format_time_saved(3600), "1 hours");
        assert_eq!(format_time_saved(90000), "1 days 1 hours");
    }

    #[test]
    fn test_old_data_cleanup() {
        let temp_dir = TempDir::new().unwrap();
        let mut collector = TelemetryCollector::new(temp_dir.path().to_path_buf()).unwrap();
        collector.enable_telemetry(true).unwrap();

        // Add old metric (simulate 31 days ago)
        let old_timestamp = current_timestamp().saturating_sub(31 * 24 * 60 * 60);
        collector.data.usage_metrics.push(UsageMetric {
            timestamp: old_timestamp,
            command: "old_command".to_string(),
            duration_ms: 100,
            success: true,
            prompt_count: None,
            error_type: None,
        });

        // Add recent metric
        let duration = Duration::from_millis(50);
        collector
            .record_command("new_command", duration, true, None, None)
            .unwrap();

        // Should have 1 metric (old one cleaned up)
        assert_eq!(collector.data.usage_metrics.len(), 1);
        assert_eq!(collector.data.usage_metrics[0].command, "new_command");
    }
}
