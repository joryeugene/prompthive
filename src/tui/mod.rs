use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use std::{
    collections::HashSet,
    io,
    time::{Duration, Instant},
};

use crate::{Clipboard, Storage};

pub mod banks;
pub mod clipboard;
pub mod state;

#[cfg(test)]
mod test_install;

pub use banks::*;
pub use clipboard::*;
pub use state::*;

/// Install preview dialog state
#[derive(Debug, Clone)]
pub struct InstallPreview {
    pub bank: Bank,
    pub selected_prompts: HashSet<String>,
    pub install_options: InstallOptions,
    pub stage: InstallStage,
    pub cursor_index: usize, // Track selected prompt in dialog
}

#[derive(Debug, Clone, PartialEq)]
pub enum InstallStage {
    Preview,
    Confirm,
    Installing,
    Complete,
}

#[derive(Debug, Clone)]
pub struct InstallOptions {
    pub install_all: bool,
    pub create_bank: bool,
    pub merge_with_existing: bool,
}

/// Main TUI interface for PromptHive with complete Banks System
pub struct PromptTui {
    pub state: TuiState,
    banks: Vec<Bank>,
    local_prompts: Vec<Prompt>,
    pub filtered_items: Vec<TreeItem>,
    pub list_state: ListState,
    status_message: Option<(String, Instant)>,
    preview_scroll: u16,
    help_visible: bool,
    install_preview: Option<InstallPreview>,
    pub delete_confirmation: Option<DeleteConfirmation>,
    focused_prompt: Option<Prompt>,
    file_content_scroll: u16,
    force_redraw: bool,
    pub rename_input: Option<RenameInput>,
    pub new_prompt_input: Option<NewPromptInput>,
    pub new_bank_input: Option<NewBankInput>,
    waiting_for_bookmark_key: Option<String>, // Path to bookmark when key is pressed
    waiting_for_goto_key: bool, // Waiting for key to jump to bookmark
}

#[derive(Debug, Clone)]
pub struct RenameInput {
    pub original_name: String,
    pub new_name: String,
    pub item_type: TreeItem,
}

#[derive(Debug, Clone)]
pub struct NewPromptInput {
    pub name: String,
    pub target_bank: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewBankInput {
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct DeleteConfirmation {
    item_name: String,
    item_type: TreeItem,
}

impl PromptTui {
    pub fn new(storage: &Storage) -> Result<Self> {
        let mut tui = Self {
            state: TuiState::new(),
            banks: Vec::new(),
            local_prompts: Vec::new(),
            filtered_items: Vec::new(),
            list_state: ListState::default(),
            status_message: None,
            preview_scroll: 0,
            help_visible: false,
            install_preview: None,
            delete_confirmation: None,
            focused_prompt: None,
            file_content_scroll: 0,
            force_redraw: false,
            rename_input: None,
            new_prompt_input: None,
            new_bank_input: None,
            waiting_for_bookmark_key: None,
            waiting_for_goto_key: false,
        };

        tui.load_data(storage)?;
        tui.update_filtered_items();

        // Select first item if available
        if !tui.filtered_items.is_empty() {
            tui.list_state.select(Some(0));
        }

        Ok(tui)
    }

    /// Load banks and prompts from storage
    fn load_data(&mut self, storage: &Storage) -> Result<()> {
        let (banks, local_prompts) = Bank::load_all_banks(storage)?;
        self.banks = banks;
        self.local_prompts = local_prompts;
        Ok(())
    }

    /// Update the filtered items list based on current state
    pub fn update_filtered_items(&mut self) {
        self.filtered_items.clear();

        match self.state.view_mode {
            ViewMode::Search => {
                let query = self.state.search_query.clone().unwrap_or_default();
                self.update_search_results(query);
            }
            ViewMode::Favorites => {
                self.update_favorites_view();
            }
            ViewMode::Recent => {
                self.update_recent_view();
            }
            ViewMode::Registry => {
                self.update_registry_view();
            }
            _ => {
                self.update_tree_view();
            }
        }

        // Apply sorting
        self.sort_items();

        // Sync cursor position and list state
        self.sync_cursor_position();
    }

    /// Synchronize cursor position between TuiState and ListState
    fn sync_cursor_position(&mut self) {
        if self.filtered_items.is_empty() {
            self.list_state.select(None);
            self.state.update_cursor_position(0);
        } else {
            // Use the saved cursor position for current view mode, or default to 0
            let cursor_pos = self.state.cursor_position.min(self.filtered_items.len() - 1);
            self.list_state.select(Some(cursor_pos));
            self.state.update_cursor_position(cursor_pos);
        }
    }

    /// Update tree view (default view)
    fn update_tree_view(&mut self) {
        if self.state.current_path.is_empty() {
            // Root level - show folders/banks hierarchy

            // Add "Local" folder for local prompts if any exist
            if !self.local_prompts.is_empty() {
                let local_bank = Bank {
                    name: "📁 Local".to_string(),
                    display_name: "Local Prompts".to_string(),
                    description: format!("Your personal prompts ({})", self.local_prompts.len()),
                    author: "You".to_string(),
                    version: "".to_string(),
                    tags: vec!["local".to_string()],
                    prompts: self.local_prompts.clone(), // Include actual prompts for count
                    bank_type: BankType::LocalBank,
                    is_expanded: false,
                };

                self.filtered_items.push(TreeItem::Bank {
                    bank: local_bank,
                    depth: 0,
                });

                // If "Local" is expanded, show local prompts
                if self.state.is_expanded("📁 Local") {
                    for prompt in &self.local_prompts {
                        self.filtered_items.push(TreeItem::Prompt {
                            prompt: prompt.clone(),
                            depth: 1,
                        });
                    }
                }
            }

            // Add all banks as folders
            for bank in &self.banks {
                self.filtered_items.push(TreeItem::Bank {
                    bank: bank.clone(),
                    depth: 0,
                });

                // If bank is expanded, show its prompts
                if self.state.is_expanded(&bank.name) {
                    for prompt in &bank.prompts {
                        self.filtered_items.push(TreeItem::Prompt {
                            prompt: prompt.clone(),
                            depth: 1,
                        });
                    }
                }
            }
        } else if self.state.current_path.len() == 1 {
            // Inside a specific bank/folder
            let bank_name = &self.state.current_path[0];

            if bank_name == "📁 Local" {
                // Show local prompts
                for prompt in &self.local_prompts {
                    self.filtered_items.push(TreeItem::Prompt {
                        prompt: prompt.clone(),
                        depth: 0,
                    });
                }
            } else if let Some(bank) = self.banks.iter().find(|b| b.name == *bank_name) {
                // Show bank prompts
                for prompt in &bank.prompts {
                    self.filtered_items.push(TreeItem::Prompt {
                        prompt: prompt.clone(),
                        depth: 0,
                    });
                }
            }
        }
    }

    /// Update search results view
    fn update_search_results(&mut self, query: String) {
        // Handle empty query - show all items
        if query.is_empty() {
            // Add all local prompts
            for prompt in &self.local_prompts {
                self.filtered_items.push(TreeItem::Prompt {
                    prompt: prompt.clone(),
                    depth: 0,
                });
            }

            // Add all bank prompts
            for bank in &self.banks {
                for prompt in &bank.prompts {
                    self.filtered_items.push(TreeItem::Prompt {
                        prompt: prompt.clone(),
                        depth: 0,
                    });
                }
            }
            return;
        }

        // Use fuzzy search for non-empty queries
        use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
        let matcher = SkimMatcherV2::default();
        let mut matches = Vec::new();

        // Search local prompts
        for prompt in &self.local_prompts {
            let search_text = format!("{} {}", prompt.name, prompt.description);
            if let Some(score) = matcher.fuzzy_match(&search_text, &query) {
                matches.push((
                    score,
                    TreeItem::Prompt {
                        prompt: prompt.clone(),
                        depth: 0,
                    },
                ));
            }
        }

        // Search bank prompts
        for bank in &self.banks {
            for prompt in &bank.prompts {
                let search_text = format!("{} {} {}", bank.name, prompt.name, prompt.description);
                if let Some(score) = matcher.fuzzy_match(&search_text, &query) {
                    matches.push((
                        score,
                        TreeItem::Prompt {
                            prompt: prompt.clone(),
                            depth: 0,
                        },
                    ));
                }
            }
        }

        // Sort by score (highest first)
        matches.sort_by(|a, b| b.0.cmp(&a.0));

        // Add to filtered items
        for (_, item) in matches {
            self.filtered_items.push(item);
        }
    }

    /// Update favorites view
    fn update_favorites_view(&mut self) {
        // Add favorited local prompts
        for prompt in &self.local_prompts {
            if self.state.is_favorite(&prompt.name) {
                self.filtered_items.push(TreeItem::Prompt {
                    prompt: prompt.clone(),
                    depth: 0,
                });
            }
        }

        // Add favorited bank prompts
        for bank in &self.banks {
            for prompt in &bank.prompts {
                if self.state.is_favorite(&prompt.full_name()) {
                    self.filtered_items.push(TreeItem::Prompt {
                        prompt: prompt.clone(),
                        depth: 0,
                    });
                }
            }
        }
    }

    /// Update recent view
    fn update_recent_view(&mut self) {
        let mut recent_items = Vec::new();

        // Collect all prompts with usage stats
        for prompt in &self.local_prompts {
            let usage = self.state.usage_count(&prompt.name);
            if usage > 0 {
                recent_items.push((
                    usage,
                    TreeItem::Prompt {
                        prompt: prompt.clone(),
                        depth: 0,
                    },
                ));
            }
        }

        for bank in &self.banks {
            for prompt in &bank.prompts {
                let usage = self.state.usage_count(&prompt.full_name());
                if usage > 0 {
                    recent_items.push((
                        usage,
                        TreeItem::Prompt {
                            prompt: prompt.clone(),
                            depth: 0,
                        },
                    ));
                }
            }
        }

        // Sort by usage (highest first)
        recent_items.sort_by(|a, b| b.0.cmp(&a.0));

        // Take top 20
        for (_, item) in recent_items.into_iter().take(20) {
            self.filtered_items.push(item);
        }
    }

    /// Update registry browser view
    fn update_registry_view(&mut self) {
        // Use the registry client to get banks (falls back to demo data)
        #[cfg(feature = "registry")]
        let registry_banks = {
            let client = crate::registry_tui::TuiRegistryClient::new(None);
            client.get_demo_banks()
        };

        #[cfg(not(feature = "registry"))]
        let registry_banks = vec![
            Bank {
                name: "@featured/ai-assistants".to_string(),
                display_name: "AI Assistants Collection".to_string(),
                description: "Professional AI assistant prompts for various tasks".to_string(),
                author: "PromptHive Team".to_string(),
                version: "2.1.0".to_string(),
                tags: vec![
                    "featured".to_string(),
                    "ai".to_string(),
                    "assistant".to_string(),
                ],
                prompts: vec![
                    Prompt {
                        name: "code-reviewer".to_string(),
                        description: "AI code review assistant".to_string(),
                        content: "You are an expert code reviewer...".to_string(),
                        bank_name: Some("@featured/ai-assistants".to_string()),
                        created_at: Some("2024-01-15T10:00:00Z".to_string()),
                        updated_at: Some("2024-01-20T15:30:00Z".to_string()),
                        tags: vec!["code".to_string(), "review".to_string()],
                        is_favorite: false,
                        usage_count: 0,
                    },
                    Prompt {
                        name: "documentation-writer".to_string(),
                        description: "Technical documentation assistant".to_string(),
                        content: "You are a technical writing expert...".to_string(),
                        bank_name: Some("@featured/ai-assistants".to_string()),
                        created_at: Some("2024-01-15T10:00:00Z".to_string()),
                        updated_at: Some("2024-01-18T12:00:00Z".to_string()),
                        tags: vec!["documentation".to_string(), "writing".to_string()],
                        is_favorite: false,
                        usage_count: 0,
                    },
                ],
                bank_type: BankType::Registry,
                is_expanded: false,
            },
            Bank {
                name: "@trending/productivity".to_string(),
                display_name: "Productivity Boosters".to_string(),
                description: "Popular prompts for enhancing productivity and workflow".to_string(),
                author: "Community".to_string(),
                version: "1.5.2".to_string(),
                tags: vec![
                    "trending".to_string(),
                    "productivity".to_string(),
                    "workflow".to_string(),
                ],
                prompts: vec![
                    Prompt {
                        name: "task-planner".to_string(),
                        description: "Smart task planning assistant".to_string(),
                        content: "Help me plan and organize my tasks...".to_string(),
                        bank_name: Some("@trending/productivity".to_string()),
                        created_at: Some("2024-01-10T09:00:00Z".to_string()),
                        updated_at: Some("2024-01-22T14:15:00Z".to_string()),
                        tags: vec!["planning".to_string(), "tasks".to_string()],
                        is_favorite: false,
                        usage_count: 0,
                    },
                    Prompt {
                        name: "meeting-summarizer".to_string(),
                        description: "Extract key points from meeting notes".to_string(),
                        content: "Summarize the following meeting notes...".to_string(),
                        bank_name: Some("@trending/productivity".to_string()),
                        created_at: Some("2024-01-12T11:30:00Z".to_string()),
                        updated_at: Some("2024-01-19T16:45:00Z".to_string()),
                        tags: vec!["meetings".to_string(), "summary".to_string()],
                        is_favorite: false,
                        usage_count: 0,
                    },
                ],
                bank_type: BankType::Registry,
                is_expanded: false,
            },
            Bank {
                name: "@community/creative-writing".to_string(),
                display_name: "Creative Writing Toolkit".to_string(),
                description: "Inspiring prompts for creative writers and storytellers".to_string(),
                author: "WritersCollective".to_string(),
                version: "3.0.1".to_string(),
                tags: vec![
                    "community".to_string(),
                    "writing".to_string(),
                    "creative".to_string(),
                ],
                prompts: vec![Prompt {
                    name: "story-generator".to_string(),
                    description: "Generate creative story ideas".to_string(),
                    content: "Create a compelling story concept...".to_string(),
                    bank_name: Some("@community/creative-writing".to_string()),
                    created_at: Some("2024-01-08T14:00:00Z".to_string()),
                    updated_at: Some("2024-01-21T10:20:00Z".to_string()),
                    tags: vec!["story".to_string(), "generator".to_string()],
                    is_favorite: false,
                    usage_count: 0,
                }],
                bank_type: BankType::Registry,
                is_expanded: false,
            },
        ];

        // Add registry banks to filtered items
        for bank in &registry_banks {
            self.filtered_items.push(TreeItem::Bank {
                bank: bank.clone(),
                depth: 0,
            });

            // If bank is expanded, show its prompts
            if self.state.is_expanded(&bank.name) {
                for prompt in &bank.prompts {
                    self.filtered_items.push(TreeItem::Prompt {
                        prompt: prompt.clone(),
                        depth: 1,
                    });
                }
            }
        }
    }

    /// Sort filtered items according to current sort mode
    /// Preserves tree hierarchy - only sorts top-level items
    fn sort_items(&mut self) {
        // For tree view, we need to preserve parent-child relationships
        // So we only sort the banks (depth 0), not the individual prompts
        if self.state.view_mode == ViewMode::LocalBanks {
            // Group items by their parent (bank name or "Local")
            let mut grouped_items: Vec<(String, Vec<TreeItem>)> = Vec::new();
            let mut current_group: Option<(String, Vec<TreeItem>)> = None;

            for item in &self.filtered_items {
                match item {
                    TreeItem::Bank { bank, .. } => {
                        // Save previous group if exists
                        if let Some(group) = current_group.take() {
                            grouped_items.push(group);
                        }
                        // Start new group
                        current_group = Some((bank.name.clone(), vec![item.clone()]));
                    }
                    TreeItem::Prompt { .. } => {
                        // Add to current group
                        if let Some((_, ref mut items)) = current_group {
                            items.push(item.clone());
                        }
                    }
                }
            }

            // Save the last group
            if let Some(group) = current_group {
                grouped_items.push(group);
            }

            // Sort groups by bank name
            match self.state.sort_mode {
                SortMode::Alphabetical => {
                    grouped_items.sort_by(|a, b| a.0.cmp(&b.0));
                }
                _ => {
                    // For other sort modes in tree view, just preserve order
                    // to avoid breaking the hierarchy
                }
            }

            // Rebuild filtered_items maintaining tree structure
            self.filtered_items.clear();
            for (_, items) in grouped_items {
                self.filtered_items.extend(items);
            }
        } else {
            // For non-tree views, sort normally
            match self.state.sort_mode {
                SortMode::Alphabetical => {
                    self.filtered_items.sort_by(|a, b| a.name().cmp(b.name()));
                }
                SortMode::Usage => {
                    self.filtered_items.sort_by(|a, b| {
                        let usage_a = match a {
                            TreeItem::Bank { bank, .. } => {
                                // For banks, sum usage of all prompts
                                bank.prompts
                                    .iter()
                                    .map(|p| self.state.usage_count(&p.full_name()))
                                    .sum::<u32>()
                            }
                            TreeItem::Prompt { prompt, .. } => {
                                self.state.usage_count(&prompt.full_name())
                            }
                        };
                        let usage_b = match b {
                            TreeItem::Bank { bank, .. } => bank
                                .prompts
                                .iter()
                                .map(|p| self.state.usage_count(&p.full_name()))
                                .sum::<u32>(),
                            TreeItem::Prompt { prompt, .. } => {
                                self.state.usage_count(&prompt.full_name())
                            }
                        };
                        usage_b.cmp(&usage_a) // Descending order (most used first)
                    });
                }
                SortMode::Modified => {
                    self.filtered_items.sort_by(|a, b| {
                        let date_a = match a {
                            TreeItem::Bank { .. } => None, // Banks don't have modified dates
                            TreeItem::Prompt { prompt, .. } => {
                                prompt.updated_at.as_ref().or(prompt.created_at.as_ref())
                            }
                        };
                        let date_b = match b {
                            TreeItem::Bank { .. } => None,
                            TreeItem::Prompt { prompt, .. } => {
                                prompt.updated_at.as_ref().or(prompt.created_at.as_ref())
                            }
                        };

                        match (date_a, date_b) {
                            (Some(a), Some(b)) => b.cmp(a), // Descending order (newest first)
                            (Some(_), None) => std::cmp::Ordering::Less, // Items with dates come first
                            (None, Some(_)) => std::cmp::Ordering::Greater,
                            (None, None) => a.name().cmp(b.name()), // Fallback to alphabetical
                        }
                    });
                }
                SortMode::Created => {
                    self.filtered_items.sort_by(|a, b| {
                        let date_a = match a {
                            TreeItem::Bank { .. } => None,
                            TreeItem::Prompt { prompt, .. } => prompt.created_at.as_ref(),
                        };
                        let date_b = match b {
                            TreeItem::Bank { .. } => None,
                            TreeItem::Prompt { prompt, .. } => prompt.created_at.as_ref(),
                        };

                        match (date_a, date_b) {
                            (Some(a), Some(b)) => b.cmp(a), // Descending order (newest first)
                            (Some(_), None) => std::cmp::Ordering::Less,
                            (None, Some(_)) => std::cmp::Ordering::Greater,
                            (None, None) => a.name().cmp(b.name()),
                        }
                    });
                }
            }
        }
    }

    pub fn set_initial_search(&mut self, query: &str) {
        self.state.search_query = Some(query.to_string());
        self.state.switch_view_mode(ViewMode::Search);
        self.update_filtered_items();

        if !self.filtered_items.is_empty() {
            self.list_state.select(Some(0));
            self.state.update_cursor_position(0);
        }
    }

    pub fn run(mut self, storage: &Storage) -> Result<()> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Run app
        let res = self.run_app(&mut terminal, storage);

        // Restore terminal
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        if let Err(err) = res {
            eprintln!("Error: {}", err);
        }

        Ok(())
    }

    fn run_app<B: Backend>(&mut self, terminal: &mut Terminal<B>, storage: &Storage) -> Result<()> {
        loop {
            // Force full redraw if needed
            if self.force_redraw {
                terminal.clear()?;
                self.force_redraw = false;
            }

            terminal.draw(|f| self.ui(f))?;

            if let Event::Key(key) = event::read()? {
                if self.help_visible {
                    // Any key closes help
                    self.help_visible = false;
                    continue;
                }

                // Handle bookmark key waiting states
                if let Some(item_path) = self.waiting_for_bookmark_key.take() {
                    match key.code {
                        KeyCode::Char(c) if c.is_ascii_lowercase() => {
                            // Set bookmark at this letter
                            self.state.set_bookmark(c, item_path);
                            self.status_message = Some((
                                format!("Bookmark '{}' set. Press M then '{}' to jump", c, c),
                                Instant::now(),
                            ));
                        }
                        KeyCode::Esc => {
                            self.status_message = Some((
                                "Bookmark cancelled".to_string(),
                                Instant::now(),
                            ));
                        }
                        _ => {
                            self.status_message = Some((
                                "Invalid bookmark key. Use a-z".to_string(),
                                Instant::now(),
                            ));
                        }
                    }
                    continue;
                }

                if self.waiting_for_goto_key {
                    self.waiting_for_goto_key = false;
                    match key.code {
                        KeyCode::Char(c) if c.is_ascii_lowercase() => {
                            if let Some(marked_path) = self.state.get_bookmark(c) {
                                // Navigate to the bookmarked item
                                self.navigate_to_bookmark(marked_path.clone())?;
                                self.status_message = Some((
                                    format!("Jumped to bookmark '{}'", c),
                                    Instant::now(),
                                ));
                            } else {
                                self.status_message = Some((
                                    format!("No bookmark set at '{}'", c),
                                    Instant::now(),
                                ));
                            }
                        }
                        KeyCode::Esc => {
                            self.status_message = Some((
                                "Jump cancelled".to_string(),
                                Instant::now(),
                            ));
                        }
                        _ => {
                            self.status_message = Some((
                                "Invalid bookmark key. Use a-z".to_string(),
                                Instant::now(),
                            ));
                        }
                    }
                    continue;
                }

                match key.code {
                    KeyCode::Char('q') => {
                        // Check if we're in input mode first - 'q' should be allowed in input fields
                        if self.rename_input.is_some()
                            || self.new_prompt_input.is_some()
                            || self.new_bank_input.is_some()
                        {
                            // Let input handlers process the 'q' character
                            self.handle_key_input(key, storage)?;
                        } else if self.delete_confirmation.is_some() {
                            // In delete confirmation, 'q' cancels
                            self.delete_confirmation = None;
                            self.status_message =
                                Some(("Deletion cancelled".to_string(), Instant::now()));
                        } else if self.state.view_mode == ViewMode::FileFocus {
                            // In file focus mode, 'q' exits focus mode, not the app
                            self.state.switch_view_mode(ViewMode::LocalBanks);
                            self.focused_prompt = None;
                            self.file_content_scroll = 0;
                        } else {
                            // Normal mode - quit the application
                            return Ok(());
                        }
                    }
                    KeyCode::Char('?') => {
                        self.help_visible = true;
                    }
                    KeyCode::Char('/') => {
                        self.start_search_mode();
                    }
                    KeyCode::Esc => {
                        // Check if any modal dialog is active first
                        if self.delete_confirmation.is_some() {
                            self.delete_confirmation = None;
                            self.status_message =
                                Some(("Deletion cancelled".to_string(), Instant::now()));
                        } else if self.rename_input.is_some() {
                            self.rename_input = None;
                            self.status_message =
                                Some(("Rename cancelled".to_string(), Instant::now()));
                        } else if self.new_prompt_input.is_some() {
                            self.new_prompt_input = None;
                            self.status_message =
                                Some(("New prompt cancelled".to_string(), Instant::now()));
                        } else if self.new_bank_input.is_some() {
                            self.new_bank_input = None;
                            self.status_message =
                                Some(("New bank cancelled".to_string(), Instant::now()));
                        } else {
                            self.handle_escape();
                        }
                    }
                    _ => {
                        self.handle_key_input(key, storage)?;
                    }
                }
            }

            // Clear status message after 3 seconds
            if let Some((_, time)) = &self.status_message {
                if time.elapsed() > Duration::from_secs(3) {
                    self.status_message = None;
                }
            }
        }
    }

    fn handle_key_input(&mut self, key: event::KeyEvent, storage: &Storage) -> Result<()> {
        // Handle delete confirmation first
        if self.delete_confirmation.is_some() {
            self.handle_delete_confirmation_input(key, storage)?;
            return Ok(());
        }

        // Handle rename input second
        if self.rename_input.is_some() {
            self.handle_rename_input(key, storage)?;
            return Ok(());
        }

        // Handle new prompt input
        if self.new_prompt_input.is_some() {
            self.handle_new_prompt_input(key, storage)?;
            return Ok(());
        }

        // Handle new bank input
        if self.new_bank_input.is_some() {
            self.handle_new_bank_input(key, storage)?;
            return Ok(());
        }

        // Handle install dialog navigation
        if self.install_preview.is_some() {
            self.handle_install_dialog_input(key, storage)?;
            return Ok(());
        }

        match self.state.view_mode {
            ViewMode::Search => {
                self.handle_search_input(key, storage)?;
            }
            _ => {
                self.handle_navigation_input(key, storage)?;
            }
        }
        Ok(())
    }

    fn handle_search_input(&mut self, key: event::KeyEvent, storage: &Storage) -> Result<()> {
        match key.code {
            KeyCode::Char('j') => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    // Ctrl+j for navigation
                    if let Some(selected) = self.list_state.selected() {
                        if selected + 1 < self.filtered_items.len() {
                            self.list_state.select(Some(selected + 1));
                        }
                    } else if !self.filtered_items.is_empty() {
                        self.list_state.select(Some(0));
                    }
                } else {
                    // Normal j - add to search query
                    if let Some(ref mut query) = self.state.search_query {
                        query.push('j');
                    } else {
                        self.state.search_query = Some('j'.to_string());
                    }
                    self.update_filtered_items();
                    self.list_state.select(Some(0));
                }
            }
            KeyCode::Char('k') => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    // Ctrl+k for navigation
                    if let Some(selected) = self.list_state.selected() {
                        if selected > 0 {
                            self.list_state.select(Some(selected - 1));
                        }
                    } else if !self.filtered_items.is_empty() {
                        self.list_state.select(Some(self.filtered_items.len() - 1));
                    }
                } else {
                    // Normal k - add to search query
                    if let Some(ref mut query) = self.state.search_query {
                        query.push('k');
                    } else {
                        self.state.search_query = Some('k'.to_string());
                    }
                    self.update_filtered_items();
                    self.list_state.select(Some(0));
                }
            }
            KeyCode::Char('l') => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    // Ctrl+l for selection
                    self.handle_l_key(storage)?;
                } else {
                    // Normal l - add to search query
                    if let Some(ref mut query) = self.state.search_query {
                        query.push('l');
                    } else {
                        self.state.search_query = Some('l'.to_string());
                    }
                    self.update_filtered_items();
                    self.list_state.select(Some(0));
                }
            }
            KeyCode::Char('w') => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    // Ctrl+w clears entire search query
                    self.state.search_query = Some(String::new());
                    self.update_filtered_items();
                    self.list_state.select(Some(0));
                } else {
                    // Normal w - add to search query
                    if let Some(ref mut query) = self.state.search_query {
                        query.push('w');
                    } else {
                        self.state.search_query = Some('w'.to_string());
                    }
                    self.update_filtered_items();
                    self.list_state.select(Some(0));
                }
            }
            KeyCode::Char('q') => {
                // 'q' exits search mode (like Esc)
                self.state.switch_view_mode(ViewMode::LocalBanks);
                self.state.search_query = None;
                self.update_filtered_items();
            }
            KeyCode::Char(c) => {
                if let Some(ref mut query) = self.state.search_query {
                    query.push(c);
                } else {
                    self.state.search_query = Some(c.to_string());
                }
                self.update_filtered_items();
                self.list_state.select(Some(0));
            }
            KeyCode::Backspace => {
                if let Some(ref mut query) = self.state.search_query {
                    query.pop();
                    if query.is_empty() {
                        self.state.search_query = None;
                    }
                }
                self.update_filtered_items();
            }
            KeyCode::Enter => {
                self.handle_l_key(storage)?;
            }
            KeyCode::Down => {
                if let Some(selected) = self.list_state.selected() {
                    if selected + 1 < self.filtered_items.len() {
                        self.list_state.select(Some(selected + 1));
                    }
                } else if !self.filtered_items.is_empty() {
                    self.list_state.select(Some(0));
                }
            }
            KeyCode::Up => {
                if let Some(selected) = self.list_state.selected() {
                    if selected > 0 {
                        self.list_state.select(Some(selected - 1));
                    }
                } else if !self.filtered_items.is_empty() {
                    self.list_state.select(Some(self.filtered_items.len() - 1));
                }
            }
            KeyCode::Esc => {
                // Escape exits search mode
                self.state.switch_view_mode(ViewMode::LocalBanks);
                self.state.search_query = None;
                self.update_filtered_items();
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_navigation_input(&mut self, key: event::KeyEvent, storage: &Storage) -> Result<()> {
        // Special handling for file focus mode
        if self.state.view_mode == ViewMode::FileFocus {
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.file_content_scroll = self.file_content_scroll.saturating_add(1);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.file_content_scroll = self.file_content_scroll.saturating_sub(1);
                }
                KeyCode::PageDown => {
                    self.file_content_scroll = self.file_content_scroll.saturating_add(10);
                }
                KeyCode::PageUp => {
                    self.file_content_scroll = self.file_content_scroll.saturating_sub(10);
                }
                KeyCode::Char('g') => {
                    self.file_content_scroll = 0;
                }
                KeyCode::Char('G') => {
                    // Scroll to bottom (we'll calculate this in the draw method)
                    self.file_content_scroll = u16::MAX;
                }
                KeyCode::Char('e') => {
                    // Edit from file focus view
                    if let Some(ref prompt) = self.focused_prompt {
                        let full_name = prompt.full_name();
                        let prompt_name = prompt.name.clone();

                        // Get the file path
                        let file_path = storage.prompt_path(&full_name);

                        // Show status message
                        self.status_message = Some((
                            format!("Opening '{}' in editor...", prompt_name),
                            Instant::now(),
                        ));

                        // Launch editor seamlessly
                        if let Err(e) = self.launch_editor_seamlessly(&file_path, storage) {
                            self.status_message =
                                Some((format!("Failed to open editor: {}", e), Instant::now()));
                        } else {
                            self.status_message = Some((
                                format!("✓ Finished editing '{}'", prompt_name),
                                Instant::now(),
                            ));

                            // Reload data and update the focused prompt
                            self.load_data(storage)?;
                            if let Some(reloaded_prompt) = self
                                .local_prompts
                                .iter()
                                .find(|p| p.full_name() == full_name)
                                .or_else(|| {
                                    self.banks
                                        .iter()
                                        .flat_map(|b| &b.prompts)
                                        .find(|p| p.full_name() == full_name)
                                })
                            {
                                self.focused_prompt = Some(reloaded_prompt.clone());
                            }

                            // Force complete redraw
                            self.force_redraw = true;
                        }
                    }
                }
                KeyCode::Char('d') => {
                    // Delete from file focus view
                    if let Some(ref prompt) = self.focused_prompt {
                        let full_name = prompt.full_name();

                        // Set up delete confirmation
                        self.delete_confirmation = Some(DeleteConfirmation {
                            item_name: full_name.clone(),
                            item_type: TreeItem::Prompt {
                                prompt: prompt.clone(),
                                depth: 0,
                            },
                        });
                    }
                }
                KeyCode::Char('D') => {
                    // Force delete from file focus view
                    if let Some(ref prompt) = self.focused_prompt {
                        let full_name = prompt.full_name();

                        // Delete immediately without confirmation
                        match storage.delete_prompt(&full_name) {
                            Ok(_) => {
                                self.status_message = Some((
                                    format!("✓ Force deleted '{}'", prompt.name),
                                    Instant::now(),
                                ));

                                // Exit file focus mode since the prompt is deleted
                                self.state.switch_view_mode(ViewMode::LocalBanks);
                                self.focused_prompt = None;
                                self.file_content_scroll = 0;

                                // Reload data
                                self.load_data(storage)?;
                                self.update_filtered_items();
                            }
                            Err(e) => {
                                self.status_message =
                                    Some((format!("Failed to delete: {}", e), Instant::now()));
                            }
                        }
                    }
                }
                KeyCode::Char('y') | KeyCode::Enter => {
                    // Copy to clipboard from file focus view (Enter or y)
                    if let Some(ref prompt) = self.focused_prompt {
                        let mut clipboard = crate::clipboard::Clipboard::new();
                        match clipboard.copy_to_clipboard(&prompt.content) {
                            Ok(_) => {
                                self.status_message = Some((
                                    format!("✓ Copied '{}' to clipboard", prompt.name),
                                    Instant::now(),
                                ));
                            }
                            Err(e) => {
                                self.status_message =
                                    Some((format!("Failed to copy: {}", e), Instant::now()));
                            }
                        }
                    }
                }
                KeyCode::Char('c') => {
                    // Clean (minify) prompt from file focus view
                    if let Some(ref prompt) = self.focused_prompt {
                        let cleaned = crate::clean::clean_text(&prompt.content);
                        let mut clipboard = crate::clipboard::Clipboard::new();
                        match clipboard.copy_to_clipboard(&cleaned) {
                            Ok(_) => {
                                self.status_message = Some((
                                    format!("✓ Cleaned and copied '{}' to clipboard", prompt.name),
                                    Instant::now(),
                                ));
                            }
                            Err(e) => {
                                self.status_message = Some((
                                    format!("Failed to copy cleaned content: {}", e),
                                    Instant::now(),
                                ));
                            }
                        }
                    }
                }
                KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('h') => {
                    // Exit file focus mode
                    self.state.switch_view_mode(ViewMode::LocalBanks);
                    self.focused_prompt = None;
                    self.file_content_scroll = 0;
                }
                _ => {}
            }
            return Ok(());
        }

        let ctrl_pressed = key.modifiers.contains(KeyModifiers::CONTROL);

        match key.code {
            // Navigation
            KeyCode::Char('j') | KeyCode::Down => self.next(),
            KeyCode::Char('k') | KeyCode::Up => self.previous(),
            KeyCode::Char('g') => {
                if ctrl_pressed {
                    self.last();
                } else {
                    self.first();
                }
            }
            KeyCode::Char('G') => self.last(),
            KeyCode::PageDown => self.page_down(),
            KeyCode::PageUp => self.page_up(),

            // Context-sensitive actions
            KeyCode::Char('l') => self.handle_l_key(storage)?,
            KeyCode::Enter => self.handle_expand_or_use(storage)?,
            KeyCode::Char('h') => self.handle_collapse_or_back(),
            KeyCode::Char('n') => self.handle_new_prompt(storage)?,
            KeyCode::Char('N') => self.handle_new_bank(storage)?,
            KeyCode::Char('e') => self.handle_edit(storage)?,
            KeyCode::Char('d') => self.handle_delete_with_confirmation(storage)?,
            KeyCode::Char('x') => self.handle_cut(storage)?,
            KeyCode::Char('D') => self.handle_delete_force(storage)?,
            KeyCode::Char('y') => self.handle_yank()?,
            KeyCode::Char('p') => self.handle_paste(storage)?,
            KeyCode::Char('v') => self.handle_view_preview(storage)?,
            KeyCode::Char('c') => self.handle_add_to_compose()?,
            KeyCode::Char('r') => self.handle_rename(storage)?,
            KeyCode::Char('f') => self.handle_toggle_favorite()?,
            KeyCode::Char('u') => self.handle_undo()?,
            KeyCode::Char(' ') => self.handle_toggle_selection(),
            KeyCode::Char('V') => self.handle_visual_mode(),
            KeyCode::Char('A') if ctrl_pressed => self.handle_select_all(),
            KeyCode::Char('a') if !ctrl_pressed => self.sort_alphabetically(),
            KeyCode::Char('E') => self.handle_select_none(),

            // Enhanced productivity shortcuts
            KeyCode::Char('m') => self.handle_mark_item()?,
            KeyCode::Char('M') => self.handle_goto_mark()?,
            KeyCode::Char('o') => self.handle_open_external()?,
            KeyCode::Char('O') => self.handle_open_directory()?,
            KeyCode::Char('t') => self.handle_tag_item()?,
            KeyCode::Char('T') => self.handle_filter_by_tag()?,
            KeyCode::Char('w') => self.handle_workspace_save()?,
            KeyCode::Char('W') => self.handle_workspace_load()?,
            KeyCode::Char(';') => self.handle_command_palette()?,
            KeyCode::Char('1'..='9') => self.handle_quick_jump(key.code)?,
            KeyCode::Char('*') => self.handle_fuzzy_search()?,
            KeyCode::Char('#') => self.handle_filter_recent()?,
            KeyCode::Char('@') => self.handle_goto_bank()?,
            KeyCode::Char('&') => self.handle_bulk_operations()?,

            // Mode switching
            KeyCode::Char('R') => self.switch_to_registry_mode(),
            KeyCode::Char('L') => self.switch_to_local_mode(),
            KeyCode::Char('F') => self.switch_to_favorites_mode(),
            KeyCode::Char('H') => self.switch_to_recent_mode(),

            // Sorting
            KeyCode::Char('S') => self.sort_by_usage(),
            KeyCode::Char('Z') => self.sort_by_modified(), // Using Z for time-based sorting

            // Registry actions
            KeyCode::Char('i') => {
                if self.state.view_mode == ViewMode::Registry {
                    self.handle_install_preview()?;
                }
            }

            _ => {}
        }
        Ok(())
    }

    fn handle_install_preview(&mut self) -> Result<()> {
        if let Some(selected) = self.list_state.selected() {
            if let Some(item) = self.filtered_items.get(selected) {
                match item {
                    TreeItem::Bank { bank, .. } => {
                        // Create install preview for the selected bank
                        let preview = InstallPreview {
                            bank: bank.clone(),
                            selected_prompts: bank.prompts.iter().map(|p| p.name.clone()).collect(),
                            install_options: InstallOptions {
                                install_all: true,
                                create_bank: true,
                                merge_with_existing: false,
                            },
                            stage: InstallStage::Preview,
                            cursor_index: 0,
                        };
                        self.install_preview = Some(preview);
                        self.set_status("Press Enter to confirm, Esc to cancel");
                    }
                    _ => {
                        self.set_status("Select a bank to install");
                    }
                }
            }
        }
        Ok(())
    }

    fn ui(&mut self, f: &mut Frame) {
        if self.state.view_mode == ViewMode::FileFocus {
            // File focus mode - minimal sidebar, large content area
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(20), Constraint::Percentage(80)].as_ref())
                .split(f.area());

            self.draw_minimal_sidebar(f, chunks[0]);
            self.draw_file_focus_content(f, chunks[1]);
            self.draw_status_bar(f);

            // CRITICAL: Draw overlays in file focus mode too!
            if self.help_visible {
                self.draw_help_overlay(f);
            }

            if self.delete_confirmation.is_some() {
                self.draw_delete_confirmation(f);
            }

            if self.rename_input.is_some() {
                self.draw_rename_input(f);
            }

            if self.new_prompt_input.is_some() {
                self.draw_new_prompt_input(f);
            }

            if self.new_bank_input.is_some() {
                self.draw_new_bank_input(f);
            }

            if self.install_preview.is_some() {
                self.draw_install_dialog(f);
            }
        } else {
            // Normal mode
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(40), Constraint::Percentage(60)].as_ref())
                .split(f.area());

            self.draw_main_list(f, chunks[0]);
            self.draw_preview_pane(f, chunks[1]);
            self.draw_status_bar(f);

            if self.help_visible {
                self.draw_help_overlay(f);
            }

            if self.delete_confirmation.is_some() {
                self.draw_delete_confirmation(f);
            }

            if self.rename_input.is_some() {
                self.draw_rename_input(f);
            }

            if self.new_prompt_input.is_some() {
                self.draw_new_prompt_input(f);
            }

            if self.new_bank_input.is_some() {
                self.draw_new_bank_input(f);
            }

            if self.install_preview.is_some() {
                self.draw_install_dialog(f);
            }
        }
    }

    /// Draw the main list (banks/prompts tree)
    fn draw_main_list(&mut self, f: &mut Frame, area: Rect) {
        // Linear-style header with single-line metrics
        let title = format!(
            " {} {} • {} items{}{}{}{} ",
            match self.state.view_mode {
                ViewMode::LocalBanks => "📁",
                ViewMode::Registry => "🌐",
                ViewMode::Search => "🔍",
                ViewMode::Favorites => "⭐",
                ViewMode::Recent => "🕒",
                _ => "📋",
            },
            self.state.view_mode_display(),
            self.filtered_items.len(),
            self.state.search_display(),
            self.state.compose_queue_display(),
            self.state.clipboard_display(),
            self.state.selection_display(),
        );

        let items: Vec<ListItem> = self
            .filtered_items
            .iter()
            .map(|item| self.create_list_item(item))
            .collect();

        // Linear-inspired subtle highlight style
        let highlight_style = Style::default()
            .bg(Color::Rgb(45, 50, 59)) // Linear's subtle selection background
            .fg(Color::Rgb(255, 255, 255))
            .add_modifier(Modifier::BOLD);

        // Linear-style clean borders and minimal highlight
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(Style::default().fg(Color::Rgb(60, 66, 78))) // Linear's border color
                    .title_style(Style::default().fg(Color::Rgb(255, 255, 255))),
            )
            .highlight_style(highlight_style)
            .highlight_symbol("▶ "); // Clean Linear-style arrow

        f.render_stateful_widget(list, area, &mut self.list_state);
    }

    /// Create a list item for display
    fn create_list_item(&self, tree_item: &TreeItem) -> ListItem<'static> {
        let indent = "  ".repeat(tree_item.depth());

        match tree_item {
            TreeItem::Bank { bank, .. } => {
                let expanded_symbol = if self.state.is_expanded(&bank.name) {
                    "▼"
                } else {
                    "▶"
                };

                let selected_indicator = if self.state.is_selected(&bank.name) {
                    "✓ "
                } else {
                    ""
                };

                let content = format!(
                    "{}{} {} {} {} - {}",
                    indent,
                    selected_indicator,
                    expanded_symbol,
                    bank.icon(),
                    bank.display_name_with_count(),
                    bank.description
                );

                // Linear-style subtle selection coloring
                let mut style = Style::default().fg(Color::Rgb(200, 205, 215)); // Linear's text color
                if self.state.is_selected(&bank.name) {
                    style = style.fg(Color::Rgb(102, 187, 106)); // Linear's subtle green
                }

                ListItem::new(content).style(style)
            }
            TreeItem::Prompt { prompt, .. } => {
                let selected_indicator = if self.state.is_selected(&prompt.full_name()) {
                    "✓ "
                } else {
                    ""
                };

                let favorite_indicator = if self.state.is_favorite(&prompt.full_name()) {
                    "⭐ "
                } else {
                    ""
                };

                let compose_indicator = if self.state.is_in_compose_queue(&prompt.full_name()) {
                    "🔗 "
                } else {
                    ""
                };

                let cut_indicator = if let Some(clipboard) = &self.state.clipboard {
                    if clipboard.is_cut()
                        && clipboard.items.iter().any(|item| item.name == prompt.name)
                    {
                        "~ " // Grayed out for cut items
                    } else {
                        ""
                    }
                } else {
                    ""
                };

                let bank_prefix = if let Some(bank_name) = &prompt.bank_name {
                    format!("{}/", bank_name)
                } else {
                    String::new()
                };

                let content = format!(
                    "{}{}{}{}{}{}  {} - {}",
                    indent,
                    selected_indicator,
                    favorite_indicator,
                    compose_indicator,
                    cut_indicator,
                    bank_prefix,
                    prompt.name,
                    prompt.description
                );

                // Linear-style prompt styling with refined colors
                let mut style = Style::default().fg(Color::Rgb(200, 205, 215)); // Linear's default text
                if self.state.is_selected(&prompt.full_name()) {
                    style = style.fg(Color::Rgb(102, 187, 106)); // Linear's subtle green
                }
                if cut_indicator == "~ " {
                    style = style.fg(Color::Rgb(120, 130, 140)); // Linear's muted text
                }

                ListItem::new(content).style(style)
            }
        }
    }

    /// Draw the preview pane
    fn draw_preview_pane(&mut self, f: &mut Frame, area: Rect) {
        if let Some(selected) = self.list_state.selected() {
            if selected < self.filtered_items.len() {
                // Clone the item to avoid borrow conflicts
                let item = self.filtered_items[selected].clone();
                match item {
                    TreeItem::Bank { bank, .. } => {
                        self.draw_bank_preview(f, area, &bank);
                    }
                    TreeItem::Prompt { prompt, .. } => {
                        self.draw_prompt_preview(f, area, &prompt);
                    }
                }
            } else {
                self.draw_empty_preview(f, area);
            }
        } else {
            self.draw_empty_preview(f, area);
        }
    }

    /// Draw bank preview
    fn draw_bank_preview(&mut self, f: &mut Frame, area: Rect, bank: &Bank) {
        // Linear-style bank preview with refined colors
        let content = vec![
            Line::from(vec![Span::styled(
                format!("{} {}", bank.icon(), bank.display_name),
                Style::default()
                    .fg(Color::Rgb(255, 255, 255))
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    "Description: ",
                    Style::default().fg(Color::Rgb(140, 150, 160)),
                ),
                Span::styled(
                    &bank.description,
                    Style::default().fg(Color::Rgb(200, 205, 215)),
                ),
            ]),
            Line::from(vec![
                Span::styled("Author: ", Style::default().fg(Color::Rgb(140, 150, 160))),
                Span::styled(&bank.author, Style::default().fg(Color::Rgb(200, 205, 215))),
            ]),
            Line::from(vec![
                Span::styled("Version: ", Style::default().fg(Color::Rgb(140, 150, 160))),
                Span::styled(
                    &bank.version,
                    Style::default().fg(Color::Rgb(200, 205, 215)),
                ),
            ]),
            Line::from(vec![
                Span::styled("Prompts: ", Style::default().fg(Color::Rgb(140, 150, 160))),
                Span::styled(
                    bank.prompts.len().to_string(),
                    Style::default().fg(Color::Rgb(102, 187, 106)), // Linear's accent green
                ),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Tags:",
                Style::default()
                    .fg(Color::Rgb(255, 255, 255))
                    .add_modifier(Modifier::BOLD),
            )]),
        ];

        let mut all_content = content;

        // Add tags
        if bank.tags.is_empty() {
            all_content.push(Line::from("  None"));
        } else {
            for tag in &bank.tags {
                all_content.push(Line::from(format!("  • {}", tag)));
            }
        }

        // Add prompts list
        all_content.push(Line::from(""));
        all_content.push(Line::from(vec![Span::styled(
            "Prompts:",
            Style::default().add_modifier(Modifier::BOLD),
        )]));

        if bank.prompts.is_empty() {
            all_content.push(Line::from("  No prompts"));
        } else {
            for prompt in &bank.prompts {
                all_content.push(Line::from(format!(
                    "  • {} - {}",
                    prompt.name, prompt.description
                )));
            }
        }

        // Linear-style preview block with clean borders
        let paragraph = Paragraph::new(all_content)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Bank Details ")
                    .border_style(Style::default().fg(Color::Rgb(60, 66, 78)))
                    .title_style(Style::default().fg(Color::Rgb(255, 255, 255))),
            )
            .style(Style::default().fg(Color::Rgb(200, 205, 215)))
            .wrap(Wrap { trim: true })
            .scroll((self.preview_scroll, 0));

        f.render_widget(paragraph, area);
    }

    /// Draw prompt preview
    fn draw_prompt_preview(&mut self, f: &mut Frame, area: Rect, prompt: &Prompt) {
        // Linear-style prompt preview with refined colors
        let mut content = vec![
            Line::from(vec![Span::styled(
                &prompt.name,
                Style::default()
                    .fg(Color::Rgb(255, 255, 255))
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    "Description: ",
                    Style::default().fg(Color::Rgb(140, 150, 160)),
                ),
                Span::styled(
                    &prompt.description,
                    Style::default().fg(Color::Rgb(200, 205, 215)),
                ),
            ]),
        ];

        // Add bank info if applicable
        if let Some(bank_name) = &prompt.bank_name {
            content.push(Line::from(vec![
                Span::styled("Bank: ", Style::default().fg(Color::Rgb(140, 150, 160))),
                Span::styled(bank_name, Style::default().fg(Color::Rgb(200, 205, 215))),
            ]));
        }

        // Add metadata
        // Linear-style metadata with consistent muted colors
        if let Some(created) = &prompt.created_at {
            content.push(Line::from(vec![
                Span::styled("Created: ", Style::default().fg(Color::Rgb(140, 150, 160))),
                Span::styled(created, Style::default().fg(Color::Rgb(160, 170, 180))),
            ]));
        }

        if let Some(updated) = &prompt.updated_at {
            content.push(Line::from(vec![
                Span::styled("Updated: ", Style::default().fg(Color::Rgb(140, 150, 160))),
                Span::styled(updated, Style::default().fg(Color::Rgb(160, 170, 180))),
            ]));
        }

        if !prompt.tags.is_empty() {
            content.push(Line::from(vec![
                Span::styled("Tags: ", Style::default().fg(Color::Rgb(140, 150, 160))),
                Span::styled(
                    prompt.tags.join(", "),
                    Style::default().fg(Color::Rgb(160, 170, 180)),
                ),
            ]));
        }

        content.push(Line::from(""));
        content.push(Line::from(vec![Span::styled(
            "Content:",
            Style::default()
                .fg(Color::Rgb(255, 255, 255))
                .add_modifier(Modifier::BOLD),
        )]));
        content.push(Line::from(""));

        // Add prompt content
        for line in prompt.content.lines() {
            content.push(Line::from(line.to_string()));
        }

        // Linear-style prompt preview block
        let paragraph = Paragraph::new(content)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Prompt Preview ")
                    .border_style(Style::default().fg(Color::Rgb(60, 66, 78)))
                    .title_style(Style::default().fg(Color::Rgb(255, 255, 255))),
            )
            .style(Style::default().fg(Color::Rgb(200, 205, 215)))
            .wrap(Wrap { trim: true })
            .scroll((self.preview_scroll, 0));

        f.render_widget(paragraph, area);
    }

    /// Draw empty preview
    fn draw_empty_preview(&self, f: &mut Frame, area: Rect) {
        // Linear-style empty preview with muted styling
        let paragraph = Paragraph::new("No item selected")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Preview ")
                    .border_style(Style::default().fg(Color::Rgb(60, 66, 78)))
                    .title_style(Style::default().fg(Color::Rgb(255, 255, 255))),
            )
            .style(Style::default().fg(Color::Rgb(140, 150, 160)))
            .alignment(Alignment::Center);
        f.render_widget(paragraph, area);
    }

    /// Draw status bar
    fn draw_status_bar(&mut self, f: &mut Frame) {
        let area = Rect {
            x: 0,
            y: f.area().height - 1,
            width: f.area().width,
            height: 1,
        };

        let status_text = if let Some((msg, _)) = &self.status_message {
            msg.clone()
        } else {
            // Linear-style status with bullet separators and clean spacing
            match self.state.view_mode {
                ViewMode::Search => {
                    " [Esc] Exit • [Enter] Confirm • Type to search...".to_string()
                }
                _ => {
                    " [hjkl] Navigate • [l] Use • [n] New • [d] Delete • [x] Cut • [p] Paste • [?] Help • [q] Quit".to_string()
                }
            }
        };

        // Linear-style status bar with refined colors
        let status = Paragraph::new(status_text).style(
            Style::default()
                .bg(Color::Rgb(35, 38, 45)) // Linear's dark background
                .fg(Color::Rgb(160, 170, 180)),
        ); // Linear's muted text

        f.render_widget(status, area);
    }

    fn draw_help_overlay(&self, f: &mut Frame) {
        let area = centered_rect(80, 80, f.area());
        f.render_widget(Clear, area);

        let help_text = vec![
            Line::from(vec![Span::styled(
                "PromptHive Banks TUI - Help",
                Style::default().add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from("Navigation:"),
            Line::from("  j/k, ↑/↓     - Move up/down"),
            Line::from("  h/l, ←/→     - Collapse/expand or back/forward"),
            Line::from("  g/G          - Go to top/bottom"),
            Line::from("  Page Up/Down - Navigate by page"),
            Line::from(""),
            Line::from("Actions:"),
            Line::from("  Enter/l      - Use prompt / expand bank / install from registry"),
            Line::from("  n            - New prompt in current bank"),
            Line::from("  e            - Edit prompt/bank"),
            Line::from("  d            - Delete prompt (with confirmation)"),
            Line::from("  D            - Delete immediately (no confirmation)"),
            Line::from("  x            - Cut item (for moving)"),
            Line::from("  y            - Yank/copy item"),
            Line::from("  p            - Paste from clipboard"),
            Line::from("  v            - View/preview"),
            Line::from("  Space        - Toggle selection"),
            Line::from(""),
            Line::from("Modes:"),
            Line::from("  /            - Search mode"),
            Line::from("  R            - Registry browser (install banks)"),
            Line::from("  L            - Local banks"),
            Line::from("  F            - Favorites only"),
            Line::from("  H            - Recent prompts"),
            Line::from("  i            - Install bank (in Registry mode)"),
            Line::from(""),
            Line::from("Multi-select:"),
            Line::from("  Space        - Toggle selection"),
            Line::from("  V            - Visual select mode"),
            Line::from("  Ctrl+A       - Select all"),
            Line::from("  E            - Clear selections"),
            Line::from(""),
            Line::from("Bookmarks:"),
            Line::from("  m + letter   - Set bookmark at letter (a-z)"),
            Line::from("  M + letter   - Jump to bookmark at letter"),
            Line::from(""),
            Line::from("Other:"),
            Line::from("  q            - Quit"),
            Line::from("  ?            - This help"),
            Line::from("  Esc          - Cancel/back"),
            Line::from(""),
            Line::from("Press any key to close help..."),
        ];

        // Linear-style help overlay with clean design
        let help_block = Paragraph::new(help_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Help ")
                    .border_style(Style::default().fg(Color::Rgb(60, 66, 78)))
                    .title_style(
                        Style::default()
                            .fg(Color::Rgb(255, 255, 255))
                            .add_modifier(Modifier::BOLD),
                    )
                    .style(Style::default().bg(Color::Rgb(25, 28, 35))),
            ) // Linear's modal background
            .style(Style::default().fg(Color::Rgb(200, 205, 215)))
            .wrap(Wrap { trim: true });

        f.render_widget(help_block, area);
    }

    /// Draw minimal sidebar for file focus mode
    fn draw_minimal_sidebar(&mut self, f: &mut Frame, area: Rect) {
        // Show a condensed navigation view
        let items: Vec<ListItem> = self
            .filtered_items
            .iter()
            .map(|item| {
                let (icon, name) = match item {
                    TreeItem::Bank { bank, .. } => {
                        let icon = if self.state.is_expanded(&bank.name) {
                            "▼"
                        } else {
                            "▶"
                        };
                        (icon, bank.name.clone())
                    }
                    TreeItem::Prompt { prompt, .. } => {
                        let icon = if let Some(ref focused) = self.focused_prompt {
                            if focused.name == prompt.name {
                                "→"
                            } else {
                                "  "
                            }
                        } else {
                            "  "
                        };
                        (icon, prompt.name.clone())
                    }
                };

                let content = format!("{} {}", icon, name);
                ListItem::new(content).style(Style::default().fg(Color::DarkGray))
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Navigation ")
                    .style(Style::default().fg(Color::DarkGray)),
            )
            .highlight_style(Style::default().fg(Color::Yellow));

        f.render_stateful_widget(list, area, &mut self.list_state);
    }

    /// Draw file focus content area
    fn draw_file_focus_content(&mut self, f: &mut Frame, area: Rect) {
        if let Some(ref prompt) = self.focused_prompt {
            // Use the prompt content that was loaded when entering file focus mode
            let content = &prompt.content;

            // Split content into lines for scrolling
            let lines: Vec<&str> = content.lines().collect();
            let max_scroll = lines.len().saturating_sub(area.height as usize - 4);

            // Adjust scroll if we scrolled too far
            if self.file_content_scroll as usize > max_scroll {
                self.file_content_scroll = max_scroll as u16;
            }

            // Get visible lines based on scroll position
            let start = self.file_content_scroll as usize;
            let end = (start + area.height as usize - 4).min(lines.len());
            let visible_lines: Vec<Line> = lines[start..end]
                .iter()
                .map(|line| Line::from(*line))
                .collect();

            // Create the content widget
            let title = format!(" {} ", prompt.name);
            let paragraph = Paragraph::new(visible_lines)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(title)
                        .style(Style::default().fg(Color::Cyan)),
                )
                .wrap(Wrap { trim: false });

            f.render_widget(paragraph, area);

            // Draw scroll indicator
            if lines.len() > area.height as usize - 4 {
                let scroll_percentage = if max_scroll > 0 {
                    (self.file_content_scroll as f32 / max_scroll as f32 * 100.0) as u16
                } else {
                    0
                };

                let scroll_info = format!(
                    " Lines {}-{} of {} ({}%) [j/k] scroll [e] edit [q] back ",
                    start + 1,
                    end,
                    lines.len(),
                    scroll_percentage
                );

                let info_area = Rect {
                    x: area.x,
                    y: area.y + area.height - 1,
                    width: area.width,
                    height: 1,
                };

                let info = Paragraph::new(scroll_info)
                    .style(Style::default().bg(Color::DarkGray).fg(Color::White));

                f.render_widget(info, info_area);
            }
        } else {
            // Shouldn't happen, but handle gracefully
            let msg = "No prompt selected for file focus view";
            let paragraph = Paragraph::new(msg)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Error ")
                        .style(Style::default().fg(Color::Red)),
                )
                .alignment(Alignment::Center);

            f.render_widget(paragraph, area);
        }
    }

    // Placeholder implementations for all the handler methods
    fn start_search_mode(&mut self) {
        self.state.switch_view_mode(ViewMode::Search);
        self.state.search_query = Some(String::new());
    }

    fn handle_escape(&mut self) {
        match self.state.view_mode {
            ViewMode::Search => {
                self.state.switch_view_mode(ViewMode::LocalBanks);
                self.state.search_query = None;
                self.update_filtered_items();
            }
            ViewMode::Registry => {
                self.state.switch_view_mode(ViewMode::LocalBanks);
                self.update_filtered_items();
            }
            _ => {
                // Clear selections or go back in navigation
                self.state.selected_items.clear();
                if !self.state.current_path.is_empty() {
                    self.state.current_path.pop();
                    self.update_filtered_items();
                }
            }
        }
    }

    // Navigation methods
    fn next(&mut self) {
        if self.filtered_items.is_empty() {
            return;
        }

        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.filtered_items.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
        self.state.update_cursor_position(i);
        self.preview_scroll = 0;
    }

    fn previous(&mut self) {
        if self.filtered_items.is_empty() {
            return;
        }

        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.filtered_items.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
        self.state.update_cursor_position(i);
        self.preview_scroll = 0;
    }

    fn first(&mut self) {
        if !self.filtered_items.is_empty() {
            self.list_state.select(Some(0));
            self.state.update_cursor_position(0);
            self.preview_scroll = 0;
        }
    }

    fn last(&mut self) {
        if !self.filtered_items.is_empty() {
            let last_pos = self.filtered_items.len() - 1;
            self.list_state.select(Some(last_pos));
            self.state.update_cursor_position(last_pos);
            self.preview_scroll = 0;
        }
    }

    fn page_down(&mut self) {
        if let Some(selected) = self.list_state.selected() {
            let new_pos = (selected + 10).min(self.filtered_items.len() - 1);
            self.list_state.select(Some(new_pos));
            self.state.update_cursor_position(new_pos);
        }
    }

    fn page_up(&mut self) {
        if let Some(selected) = self.list_state.selected() {
            let new_pos = selected.saturating_sub(10);
            self.list_state.select(Some(new_pos));
            self.state.update_cursor_position(new_pos);
        }
    }

    // Action handlers
    fn handle_expand_or_use(&mut self, storage: &Storage) -> Result<()> {
        if let Some(selected) = self.list_state.selected() {
            if selected < self.filtered_items.len() {
                // Clone the item to avoid borrow conflicts
                let item = self.filtered_items[selected].clone();
                match item {
                    TreeItem::Bank { bank, .. } => {
                        if self.state.view_mode == ViewMode::Registry {
                            // In registry mode, Enter/l should open install preview dialog
                            self.handle_install_preview()?;
                        } else {
                            // Normal mode - toggle bank expansion
                            let bank_name = bank.name.clone();
                            self.state.toggle_expanded(&bank_name);
                            self.update_filtered_items();
                            self.status_message = Some((
                                format!(
                                    "Bank '{}' {}",
                                    bank_name,
                                    if self.state.is_expanded(&bank_name) {
                                        "expanded"
                                    } else {
                                        "collapsed"
                                    }
                                ),
                                Instant::now(),
                            ));
                        }
                    }
                    TreeItem::Prompt { prompt, .. } => {
                        // Use prompt - copy to clipboard and exit TUI
                        let full_name = prompt.full_name();
                        let (_, content) = storage.read_prompt(&full_name)?;
                        let mut clipboard = Clipboard::new();
                        clipboard.copy_to_clipboard(&content)?;

                        // Increment usage count
                        self.state.increment_usage(&full_name);

                        self.status_message = Some((
                            format!("✓ Used '{}' - copied to clipboard", prompt.name),
                            Instant::now(),
                        ));

                        // Exit TUI after short delay to show message
                        std::thread::sleep(std::time::Duration::from_millis(500));
                        return Ok(());
                    }
                }
            }
        }
        Ok(())
    }

    pub fn handle_collapse_or_back(&mut self) {
        if let Some(selected) = self.list_state.selected() {
            if selected < self.filtered_items.len() {
                // Clone the item to avoid borrow conflicts
                let item = self.filtered_items[selected].clone();
                match item {
                    TreeItem::Bank { bank, .. } => {
                        let bank_name = bank.name.clone();
                        if self.state.is_expanded(&bank_name) {
                            // Collapse the bank
                            self.state.toggle_expanded(&bank_name);
                            self.update_filtered_items();
                            self.status_message =
                                Some((format!("Bank '{}' collapsed", bank_name), Instant::now()));
                        } else {
                            // Navigate back
                            self.state.navigate_back();
                            self.update_filtered_items();
                        }
                    }
                    TreeItem::Prompt { prompt, .. } => {
                        // When on a prompt, navigate to its parent bank
                        if let Some(bank_name) = &prompt.bank_name {
                            // Find and select the parent bank
                            for (idx, item) in self.filtered_items.iter().enumerate() {
                                if let TreeItem::Bank { bank, .. } = item {
                                    if bank.name == *bank_name {
                                        self.list_state.select(Some(idx));
                                        self.status_message = Some((
                                            format!("Navigated to parent bank '{}'", bank_name),
                                            Instant::now(),
                                        ));
                                        return;
                                    }
                                }
                            }
                        }

                        // If no parent bank found, navigate to root
                        self.state.navigate_to_root();
                        self.update_filtered_items();
                        self.status_message =
                            Some(("Navigated to root".to_string(), Instant::now()));
                    }
                }
            }
        } else {
            // Navigate back
            self.state.navigate_back();
            self.update_filtered_items();
        }
    }

    fn handle_new_bank(&mut self, _storage: &Storage) -> Result<()> {
        // Activate new bank input mode
        self.new_bank_input = Some(NewBankInput {
            name: String::new(),
        });
        self.status_message = Some((
            "Enter bank name, Enter to confirm, Esc to cancel".to_string(),
            Instant::now(),
        ));
        Ok(())
    }

    fn handle_new_prompt(&mut self, _storage: &Storage) -> Result<()> {
        // Determine target bank based on current context
        let target_bank = if let Some(selected) = self.list_state.selected() {
            if selected < self.filtered_items.len() {
                match &self.filtered_items[selected] {
                    TreeItem::Bank { bank, .. } => Some(bank.name.clone()),
                    TreeItem::Prompt { prompt, .. } => prompt.bank_name.clone(),
                }
            } else {
                None
            }
        } else {
            None
        };

        // Activate new prompt input mode
        self.new_prompt_input = Some(NewPromptInput {
            name: String::new(),
            target_bank,
        });
        self.status_message = Some((
            "Enter prompt name, Enter to confirm, Esc to cancel".to_string(),
            Instant::now(),
        ));
        Ok(())
    }

    fn handle_edit(&mut self, storage: &Storage) -> Result<()> {
        if let Some(selected) = self.list_state.selected() {
            if selected < self.filtered_items.len() {
                // Clone the item to avoid borrow conflicts
                let item = self.filtered_items[selected].clone();
                match item {
                    TreeItem::Bank { .. } => {
                        self.status_message = Some((
                            "Banks cannot be edited directly. Edit individual prompts instead."
                                .to_string(),
                            Instant::now(),
                        ));
                    }
                    TreeItem::Prompt { prompt, .. } => {
                        let full_name = prompt.full_name();
                        let prompt_name = prompt.name.clone();

                        // Get the file path
                        let file_path = storage.prompt_path(&full_name);

                        // Show status message
                        self.status_message = Some((
                            format!("Opening '{}' in editor...", prompt_name),
                            Instant::now(),
                        ));

                        // Launch editor seamlessly
                        if let Err(e) = self.launch_editor_seamlessly(&file_path, storage) {
                            self.status_message =
                                Some((format!("Failed to open editor: {}", e), Instant::now()));
                        } else {
                            self.status_message = Some((
                                format!("✓ Finished editing '{}'", prompt_name),
                                Instant::now(),
                            ));
                        }

                        // Reload data after editing
                        self.load_data(storage)?;
                        self.update_filtered_items();

                        // Force complete redraw on next iteration
                        self.force_redraw = true;
                    }
                }
            }
        }
        Ok(())
    }

    /// Launch editor seamlessly - exit TUI, run editor, return to TUI
    fn launch_editor_seamlessly(
        &mut self,
        file_path: &std::path::Path,
        _storage: &Storage,
    ) -> Result<()> {
        use std::process::Command;

        // Temporarily exit TUI mode
        disable_raw_mode()?;
        execute!(std::io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;

        // Get editor from environment or use default
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| {
            // Try common editors in order of preference
            if Command::new("nvim").arg("--version").output().is_ok() {
                "nvim".to_string()
            } else if Command::new("vim").arg("--version").output().is_ok() {
                "vim".to_string()
            } else if Command::new("nano").arg("--version").output().is_ok() {
                "nano".to_string()
            } else {
                "vi".to_string() // Fallback - should exist on all Unix systems
            }
        });

        // Launch editor
        let status = Command::new(&editor).arg(file_path).status()?;

        // Re-enter TUI mode
        enable_raw_mode()?;
        execute!(std::io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;

        if !status.success() {
            return Err(anyhow::anyhow!("Editor exited with non-zero status"));
        }

        Ok(())
    }

    fn handle_cut(&mut self, _storage: &Storage) -> Result<()> {
        let mut clipboard_entries = Vec::new();

        if self.state.selected_items.is_empty() {
            // No multi-selection, cut current item
            if let Some(selected) = self.list_state.selected() {
                if selected < self.filtered_items.len() {
                    let item = self.filtered_items[selected].clone();
                    match item {
                        TreeItem::Bank { bank, .. } => {
                            clipboard_entries.push(ClipboardEntry::new_bank(
                                bank.name.clone(),
                                None, // Banks are at root level
                            ));
                        }
                        TreeItem::Prompt { prompt, .. } => {
                            clipboard_entries.push(ClipboardEntry::new_prompt(
                                prompt.name.clone(),
                                prompt.content.clone(),
                                crate::storage::PromptMetadata {
                                    id: prompt.name.clone(),
                                    description: prompt.description.clone(),
                                    tags: if prompt.tags.is_empty() {
                                        None
                                    } else {
                                        Some(prompt.tags.clone())
                                    },
                                    created_at: prompt.created_at.clone(),
                                    updated_at: prompt.updated_at.clone(),
                                    version: None,
                                    git_hash: None,
                                    parent_version: None,
                                },
                                prompt.bank_name.clone(),
                            ));
                        }
                    }
                }
            }
        } else {
            // Multi-selection, cut all selected items
            for item in &self.filtered_items {
                let item_name = match item {
                    TreeItem::Bank { bank, .. } => bank.name.clone(),
                    TreeItem::Prompt { prompt, .. } => prompt.full_name(),
                };

                if self.state.selected_items.contains(&item_name) {
                    match item {
                        TreeItem::Bank { bank, .. } => {
                            clipboard_entries
                                .push(ClipboardEntry::new_bank(bank.name.clone(), None));
                        }
                        TreeItem::Prompt { prompt, .. } => {
                            clipboard_entries.push(ClipboardEntry::new_prompt(
                                prompt.name.clone(),
                                prompt.content.clone(),
                                crate::storage::PromptMetadata {
                                    id: prompt.name.clone(),
                                    description: prompt.description.clone(),
                                    tags: if prompt.tags.is_empty() {
                                        None
                                    } else {
                                        Some(prompt.tags.clone())
                                    },
                                    created_at: prompt.created_at.clone(),
                                    updated_at: prompt.updated_at.clone(),
                                    version: None,
                                    git_hash: None,
                                    parent_version: None,
                                },
                                prompt.bank_name.clone(),
                            ));
                        }
                    }
                }
            }

            // Clear selections after cutting
            self.state.clear_selections();
        }

        if !clipboard_entries.is_empty() {
            self.state.clipboard = Some(ClipboardItem::new_cut(clipboard_entries));
            self.status_message = Some((
                format!(
                    "Cut {} item(s) to clipboard",
                    self.state.clipboard.as_ref().unwrap().count()
                ),
                Instant::now(),
            ));
        }

        Ok(())
    }

    fn handle_delete_force(&mut self, storage: &Storage) -> Result<()> {
        if let Some(selected) = self.list_state.selected() {
            if selected < self.filtered_items.len() {
                let item = &self.filtered_items[selected];
                match item {
                    TreeItem::Bank { bank, .. } => {
                        self.status_message = Some((
                            format!("Cannot force delete entire bank '{}'. Delete individual prompts instead.", bank.name),
                            Instant::now(),
                        ));
                    }
                    TreeItem::Prompt { prompt, .. } => {
                        let full_name = prompt.full_name();
                        // Delete immediately without confirmation
                        match storage.delete_prompt(&full_name) {
                            Ok(_) => {
                                // Add to undo stack
                                self.state.add_undo_operation(UndoOperation::DeletePrompt {
                                    name: prompt.name.clone(),
                                    bank: prompt.bank_name.clone(),
                                    content: prompt.content.clone(),
                                    metadata: crate::storage::PromptMetadata {
                                        id: prompt.name.clone(),
                                        description: prompt.description.clone(),
                                        tags: if prompt.tags.is_empty() {
                                            None
                                        } else {
                                            Some(prompt.tags.clone())
                                        },
                                        created_at: prompt.created_at.clone(),
                                        updated_at: prompt.updated_at.clone(),
                                        version: None,
                                        git_hash: None,
                                        parent_version: None,
                                    },
                                });

                                // Remember current selection position
                                let current_selection = selected;
                                let _total_items = self.filtered_items.len();

                                // Reload data
                                self.load_data(storage)?;
                                self.update_filtered_items();

                                // Smart selection after delete:
                                // - If we deleted the last item, select the new last item
                                // - Otherwise, stay at the same position (which is now the next item)
                                let new_total = self.filtered_items.len();
                                if new_total > 0 {
                                    if current_selection >= new_total {
                                        // We deleted the last item, select the new last item
                                        self.list_state.select(Some(new_total - 1));
                                    } else {
                                        // Stay at same position (which is now the next item)
                                        self.list_state.select(Some(current_selection));
                                    }
                                } else {
                                    // No items left
                                    self.list_state.select(None);
                                }

                                self.status_message = Some((
                                    format!("✓ Force deleted '{}'", full_name),
                                    Instant::now(),
                                ));
                            }
                            Err(e) => {
                                self.status_message =
                                    Some((format!("Failed to delete: {}", e), Instant::now()));
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn handle_yank(&mut self) -> Result<()> {
        let mut clipboard_entries = Vec::new();

        if self.state.selected_items.is_empty() {
            // No multi-selection, copy current item
            if let Some(selected) = self.list_state.selected() {
                if selected < self.filtered_items.len() {
                    let item = self.filtered_items[selected].clone();
                    match item {
                        TreeItem::Bank { bank, .. } => {
                            clipboard_entries.push(ClipboardEntry::new_bank(
                                bank.name.clone(),
                                None, // Banks are at root level
                            ));
                        }
                        TreeItem::Prompt { prompt, .. } => {
                            clipboard_entries.push(ClipboardEntry::new_prompt(
                                prompt.name.clone(),
                                prompt.content.clone(),
                                crate::storage::PromptMetadata {
                                    id: prompt.name.clone(),
                                    description: prompt.description.clone(),
                                    tags: if prompt.tags.is_empty() {
                                        None
                                    } else {
                                        Some(prompt.tags.clone())
                                    },
                                    created_at: prompt.created_at.clone(),
                                    updated_at: prompt.updated_at.clone(),
                                    version: None,
                                    git_hash: None,
                                    parent_version: None,
                                },
                                prompt.bank_name.clone(),
                            ));
                        }
                    }
                }
            }
        } else {
            // Multi-selection, copy all selected items
            for item in &self.filtered_items {
                let item_name = match item {
                    TreeItem::Bank { bank, .. } => bank.name.clone(),
                    TreeItem::Prompt { prompt, .. } => prompt.full_name(),
                };

                if self.state.selected_items.contains(&item_name) {
                    match item {
                        TreeItem::Bank { bank, .. } => {
                            clipboard_entries
                                .push(ClipboardEntry::new_bank(bank.name.clone(), None));
                        }
                        TreeItem::Prompt { prompt, .. } => {
                            clipboard_entries.push(ClipboardEntry::new_prompt(
                                prompt.name.clone(),
                                prompt.content.clone(),
                                crate::storage::PromptMetadata {
                                    id: prompt.name.clone(),
                                    description: prompt.description.clone(),
                                    tags: if prompt.tags.is_empty() {
                                        None
                                    } else {
                                        Some(prompt.tags.clone())
                                    },
                                    created_at: prompt.created_at.clone(),
                                    updated_at: prompt.updated_at.clone(),
                                    version: None,
                                    git_hash: None,
                                    parent_version: None,
                                },
                                prompt.bank_name.clone(),
                            ));
                        }
                    }
                }
            }
        }

        if !clipboard_entries.is_empty() {
            self.state.clipboard = Some(ClipboardItem::new_copy(clipboard_entries));
            self.status_message = Some((
                format!(
                    "Copied {} item(s) to clipboard",
                    self.state.clipboard.as_ref().unwrap().count()
                ),
                Instant::now(),
            ));
        }

        Ok(())
    }

    fn handle_paste(&mut self, storage: &Storage) -> Result<()> {
        if let Some(ref clipboard) = self.state.clipboard.clone() {
            // Determine target location based on current context
            let target_bank = if let Some(selected) = self.list_state.selected() {
                if selected < self.filtered_items.len() {
                    match &self.filtered_items[selected] {
                        TreeItem::Bank { bank, .. } => Some(bank.name.clone()),
                        TreeItem::Prompt { prompt, .. } => prompt.bank_name.clone(),
                    }
                } else {
                    None
                }
            } else {
                None
            };

            // Check if paste is allowed
            let (can_paste, error_msg) = clipboard.can_paste_into(target_bank.as_deref());
            if !can_paste {
                self.status_message = Some((
                    error_msg.unwrap_or("Cannot paste here".to_string()),
                    Instant::now(),
                ));
                return Ok(());
            }

            // Perform paste operation
            let mut pasted_items = Vec::new();
            for item in &clipboard.items {
                match item.item_type {
                    ClipboardItemType::Prompt => {
                        // Determine target prompt name
                        let target_name = if let Some(ref bank) = target_bank {
                            format!("{}/{}", bank, item.name)
                        } else {
                            item.name.clone()
                        };

                        // Write prompt to target location
                        storage.write_prompt(&target_name, &item.metadata, &item.content)?;
                        pasted_items.push(target_name.clone());

                        // If this was a cut operation, delete the original and add to undo stack
                        if clipboard.is_cut() {
                            // Delete the original prompt
                            let original_name = if let Some(ref source_bank) = item.source_bank {
                                format!("{}/{}", source_bank, item.name)
                            } else {
                                item.name.clone()
                            };

                            // Only delete if source is different from target
                            if original_name != target_name {
                                storage.delete_prompt(&original_name)?;
                            }

                            self.state.add_undo_operation(UndoOperation::MovePrompt {
                                name: item.name.clone(),
                                from_bank: item.source_bank.clone(),
                                to_bank: target_bank.clone(),
                            });
                        }
                    }
                    ClipboardItemType::Bank => {
                        // Bank paste - complex operation requiring directory copying
                        self.status_message = Some((
                            "Bank paste is complex - use 'ph import' for bank copying instead"
                                .to_string(),
                            Instant::now(),
                        ));
                        return Ok(());
                    }
                }
            }

            // Clear clipboard if it was a cut operation
            if clipboard.is_cut() {
                self.state.clipboard = None;
            }

            // Reload data and update view
            self.load_data(storage)?;
            self.update_filtered_items();

            self.status_message = Some((
                format!("Pasted {} item(s)", pasted_items.len()),
                Instant::now(),
            ));
        } else {
            self.status_message = Some(("No items in clipboard".to_string(), Instant::now()));
        }
        Ok(())
    }

    fn handle_view_preview(&mut self, storage: &Storage) -> Result<()> {
        if let Some(selected) = self.list_state.selected() {
            if selected < self.filtered_items.len() {
                let item = &self.filtered_items[selected];
                match item {
                    TreeItem::Prompt { prompt, .. } => {
                        let full_name = prompt.full_name();
                        match storage.read_prompt(&full_name) {
                            Ok((_, content)) => {
                                // Truncate content for preview (first 200 chars)
                                let preview = if content.len() > 200 {
                                    format!("{}...", &content[..200])
                                } else {
                                    content
                                };
                                self.status_message = Some((
                                    format!("Preview: {}", preview.replace('\n', " | ")),
                                    Instant::now(),
                                ));
                            }
                            Err(e) => {
                                self.status_message =
                                    Some((format!("Error reading prompt: {}", e), Instant::now()));
                            }
                        }
                    }
                    TreeItem::Bank { bank, .. } => {
                        self.status_message = Some((
                            format!(
                                "Bank '{}' contains {} prompts",
                                bank.name,
                                bank.prompts.len()
                            ),
                            Instant::now(),
                        ));
                    }
                }
            }
        } else {
            self.status_message =
                Some(("No item selected for preview".to_string(), Instant::now()));
        }
        Ok(())
    }

    fn handle_add_to_compose(&mut self) -> Result<()> {
        if let Some(selected) = self.list_state.selected() {
            if selected < self.filtered_items.len() {
                let item = &self.filtered_items[selected];
                match item {
                    TreeItem::Bank { bank, .. } => {
                        // Add all prompts from bank to compose queue
                        let mut added_count = 0;
                        for prompt in &bank.prompts {
                            let full_name = prompt.full_name();
                            if !self.state.is_in_compose_queue(&full_name) {
                                self.state.add_to_compose_queue(&full_name);
                                added_count += 1;
                            }
                        }
                        self.status_message = Some((
                            format!(
                                "Added {} prompts from bank '{}' to compose queue",
                                added_count, bank.name
                            ),
                            Instant::now(),
                        ));
                    }
                    TreeItem::Prompt { prompt, .. } => {
                        let full_name = prompt.full_name();
                        if self.state.is_in_compose_queue(&full_name) {
                            self.state.remove_from_compose_queue(&full_name);
                            self.status_message = Some((
                                format!("Removed '{}' from compose queue", prompt.name),
                                Instant::now(),
                            ));
                        } else {
                            self.state.add_to_compose_queue(&full_name);
                            self.status_message = Some((
                                format!("Added '{}' to compose queue", prompt.name),
                                Instant::now(),
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn handle_rename(&mut self, _storage: &Storage) -> Result<()> {
        if let Some(selected) = self.list_state.selected() {
            if selected < self.filtered_items.len() {
                let item = self.filtered_items[selected].clone();
                match &item {
                    TreeItem::Prompt { prompt, .. } => {
                        // Activate rename mode
                        self.rename_input = Some(RenameInput {
                            original_name: prompt.full_name(),
                            new_name: prompt.name.clone(),
                            item_type: item,
                        });
                        self.status_message = Some((
                            "Enter new name, Enter to confirm, Esc to cancel".to_string(),
                            Instant::now(),
                        ));
                    }
                    TreeItem::Bank { bank, .. } => {
                        // Activate rename mode for bank
                        self.rename_input = Some(RenameInput {
                            original_name: bank.name.clone(),
                            new_name: bank.name.clone(),
                            item_type: item.clone(),
                        });
                        self.status_message = Some((
                            "Enter new bank name, Enter to confirm, Esc to cancel".to_string(),
                            Instant::now(),
                        ));
                    }
                }
            }
        } else {
            self.status_message = Some(("No item selected for rename".to_string(), Instant::now()));
        }
        Ok(())
    }

    fn handle_rename_input(&mut self, key: event::KeyEvent, storage: &Storage) -> Result<()> {
        if let Some(ref mut rename_input) = self.rename_input {
            match key.code {
                KeyCode::Esc => {
                    // Cancel rename
                    self.rename_input = None;
                    self.status_message = Some(("Rename cancelled".to_string(), Instant::now()));
                }
                KeyCode::Enter => {
                    // Confirm rename
                    if !rename_input.new_name.trim().is_empty() {
                        let new_name = rename_input.new_name.trim().to_string();
                        let original_full_name = rename_input.original_name.clone();

                        // Handle rename based on item type
                        match &rename_input.item_type {
                            TreeItem::Prompt { prompt, .. } => {
                                // Determine new full name
                                let new_full_name = if let Some(ref bank_name) = prompt.bank_name {
                                    format!("{}/{}", bank_name, new_name)
                                } else {
                                    new_name.clone()
                                };

                                // Perform rename by reading, writing, and deleting
                                match storage.read_prompt(&original_full_name) {
                                    Ok((mut metadata, content)) => {
                                        // Update metadata with new name
                                        metadata.id = new_name.clone();

                                        // Write prompt with new name
                                        match storage.write_prompt(
                                            &new_full_name,
                                            &metadata,
                                            &content,
                                        ) {
                                            Ok(_) => {
                                                // Delete old prompt
                                                match storage.delete_prompt(&original_full_name) {
                                                    Ok(_) => {
                                                        self.status_message = Some((
                                                            format!(
                                                                "✓ Renamed prompt '{}' to '{}'",
                                                                original_full_name
                                                                    .split('/')
                                                                    .next_back()
                                                                    .unwrap_or(&original_full_name),
                                                                new_name
                                                            ),
                                                            Instant::now(),
                                                        ));

                                                        // Reload data
                                                        self.load_data(storage)?;
                                                        self.update_filtered_items();
                                                    }
                                                    Err(e) => {
                                                        // Try to clean up the new file if deletion failed
                                                        let _ =
                                                            storage.delete_prompt(&new_full_name);
                                                        self.status_message = Some((
                                                            format!(
                                                                "Failed to delete old prompt: {}",
                                                                e
                                                            ),
                                                            Instant::now(),
                                                        ));
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                self.status_message = Some((
                                                    format!("Failed to create new prompt: {}", e),
                                                    Instant::now(),
                                                ));
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        self.status_message = Some((
                                            format!("Failed to read prompt: {}", e),
                                            Instant::now(),
                                        ));
                                    }
                                }
                            }
                            TreeItem::Bank { .. } => {
                                // Rename bank
                                match storage.rename_bank(&original_full_name, &new_name) {
                                    Ok(_) => {
                                        self.status_message = Some((
                                            format!(
                                                "✓ Renamed bank '{}' to '{}'",
                                                original_full_name, new_name
                                            ),
                                            Instant::now(),
                                        ));

                                        // Reload data
                                        self.load_data(storage)?;
                                        self.update_filtered_items();
                                    }
                                    Err(e) => {
                                        self.status_message = Some((
                                            format!("Failed to rename bank: {}", e),
                                            Instant::now(),
                                        ));
                                    }
                                }
                            }
                        }
                    } else {
                        self.status_message =
                            Some(("Name cannot be empty".to_string(), Instant::now()));
                    }

                    self.rename_input = None;
                }
                KeyCode::Backspace => {
                    rename_input.new_name.pop();
                }
                KeyCode::Char(c) => {
                    rename_input.new_name.push(c);
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn handle_toggle_favorite(&mut self) -> Result<()> {
        if let Some(selected) = self.list_state.selected() {
            if selected < self.filtered_items.len() {
                let item = &self.filtered_items[selected];
                match item {
                    TreeItem::Bank { bank, .. } => {
                        self.state.toggle_favorite(&bank.name);
                        let is_favorited = self.state.is_favorite(&bank.name);
                        self.status_message = Some((
                            format!(
                                "Bank '{}' {}",
                                bank.name,
                                if is_favorited {
                                    "added to favorites"
                                } else {
                                    "removed from favorites"
                                }
                            ),
                            Instant::now(),
                        ));
                    }
                    TreeItem::Prompt { prompt, .. } => {
                        let full_name = prompt.full_name();
                        self.state.toggle_favorite(&full_name);
                        let is_favorited = self.state.is_favorite(&full_name);
                        self.status_message = Some((
                            format!(
                                "Prompt '{}' {}",
                                prompt.name,
                                if is_favorited {
                                    "added to favorites"
                                } else {
                                    "removed from favorites"
                                }
                            ),
                            Instant::now(),
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    fn handle_undo(&mut self) -> Result<()> {
        if let Some(operation) = self.state.pop_undo_operation() {
            match &operation {
                UndoOperation::InstallBank {
                    bank_name,
                    prompts,
                    created_bank: _,
                } => {
                    // Get storage from the calling context - for now just show message
                    self.status_message = Some((
                        format!(
                            "Would undo installation of {} prompts from {}",
                            prompts.len(),
                            bank_name
                        ),
                        Instant::now(),
                    ));
                    // In a real implementation:
                    // - Delete all installed prompts
                    // - Remove bank directory if created_bank is true
                    // - Reload the TUI data
                }
                _ => {
                    self.status_message = Some((
                        format!(
                            "Undid operation: {:?} (Note: Undo execution not fully implemented)",
                            operation
                        ),
                        Instant::now(),
                    ));
                }
            }
        } else {
            self.status_message = Some(("No operations to undo".to_string(), Instant::now()));
        }
        Ok(())
    }

    fn handle_toggle_selection(&mut self) {
        if let Some(selected) = self.list_state.selected() {
            if selected < self.filtered_items.len() {
                let item_name = match &self.filtered_items[selected] {
                    TreeItem::Bank { bank, .. } => bank.name.clone(),
                    TreeItem::Prompt { prompt, .. } => prompt.full_name(),
                };

                self.state.toggle_selection(&item_name);
                let is_selected = self.state.is_selected(&item_name);

                self.status_message = Some((
                    format!(
                        "Item '{}' {}",
                        item_name,
                        if is_selected {
                            "selected"
                        } else {
                            "deselected"
                        }
                    ),
                    Instant::now(),
                ));
            }
        }
    }

    fn handle_visual_mode(&mut self) {
        // Toggle visual mode - select range from current position to next movement
        if let Some(current) = self.list_state.selected() {
            if current < self.filtered_items.len() {
                // Store the visual mode anchor point
                let item_name = match &self.filtered_items[current] {
                    TreeItem::Bank { bank, .. } => bank.name.clone(),
                    TreeItem::Prompt { prompt, .. } => prompt.full_name(),
                };

                // If item is already selected, start deselecting range
                // Otherwise start selecting range
                if self.state.is_selected(&item_name) {
                    // Clear all selections and enter visual deselect mode
                    self.state.clear_selections();
                    self.status_message = Some((
                        "Visual mode: cleared selections".to_string(),
                        Instant::now(),
                    ));
                } else {
                    // Select from current position and enter visual select mode
                    self.select_range(current, current);
                    self.status_message = Some((
                        "Visual mode: select range with j/k".to_string(),
                        Instant::now(),
                    ));
                }
            }
        }
    }

    // Mode switching
    fn switch_to_registry_mode(&mut self) {
        self.state.switch_view_mode(ViewMode::Registry);
        self.update_filtered_items();
        self.status_message = Some((
            "Switched to registry browser (demo mode)".to_string(),
            Instant::now(),
        ));
    }

    fn switch_to_local_mode(&mut self) {
        self.state.switch_view_mode(ViewMode::LocalBanks);
        self.update_filtered_items();
    }

    fn switch_to_favorites_mode(&mut self) {
        self.state.switch_view_mode(ViewMode::Favorites);
        self.update_filtered_items();
        self.status_message = Some(("Switched to favorites view".to_string(), Instant::now()));
    }

    fn switch_to_recent_mode(&mut self) {
        self.state.switch_view_mode(ViewMode::Recent);
        self.update_filtered_items();
        self.status_message = Some((
            "Switched to recent prompts view".to_string(),
            Instant::now(),
        ));
    }

    // Sorting
    fn sort_by_usage(&mut self) {
        self.state.sort_mode = SortMode::Usage;
        self.update_filtered_items();
        self.status_message = Some(("Sorted by usage count".to_string(), Instant::now()));
    }

    fn sort_by_modified(&mut self) {
        self.state.sort_mode = SortMode::Modified;
        self.update_filtered_items();
        self.status_message = Some(("Sorted by modified date".to_string(), Instant::now()));
    }

    fn sort_alphabetically(&mut self) {
        self.state.sort_mode = SortMode::Alphabetical;
        self.update_filtered_items();
        self.status_message = Some(("Sorted alphabetically".to_string(), Instant::now()));
    }

    fn handle_select_all(&mut self) {
        // Select all visible items
        self.state.clear_selections();
        for item in &self.filtered_items {
            let item_name = match item {
                TreeItem::Bank { bank, .. } => bank.name.clone(),
                TreeItem::Prompt { prompt, .. } => prompt.full_name(),
            };
            self.state.selected_items.insert(item_name);
        }

        self.status_message = Some((
            format!("Selected all {} items", self.filtered_items.len()),
            Instant::now(),
        ));
    }

    fn handle_select_none(&mut self) {
        let count = self.state.selected_items.len();
        self.state.clear_selections();

        self.status_message = Some((format!("Cleared {} selections", count), Instant::now()));
    }

    fn select_range(&mut self, start: usize, end: usize) {
        let (start_idx, end_idx) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };

        for i in start_idx..=end_idx {
            if i < self.filtered_items.len() {
                let item_name = match &self.filtered_items[i] {
                    TreeItem::Bank { bank, .. } => bank.name.clone(),
                    TreeItem::Prompt { prompt, .. } => prompt.full_name(),
                };
                self.state.selected_items.insert(item_name);
            }
        }
    }

    fn set_status(&mut self, message: &str) {
        self.status_message = Some((message.to_string(), Instant::now()));
    }

    /// Handle input while install dialog is shown
    fn handle_install_dialog_input(
        &mut self,
        key: event::KeyEvent,
        storage: &Storage,
    ) -> Result<()> {
        if let Some(ref mut preview) = self.install_preview {
            match preview.stage {
                InstallStage::Preview => {
                    match key.code {
                        KeyCode::Esc => {
                            self.install_preview = None;
                            self.set_status("Installation cancelled");
                        }
                        KeyCode::Enter => {
                            preview.stage = InstallStage::Confirm;
                        }
                        KeyCode::Char('a') => {
                            // Toggle install all
                            preview.install_options.install_all =
                                !preview.install_options.install_all;
                            if preview.install_options.install_all {
                                // Select all prompts
                                preview.selected_prompts = preview
                                    .bank
                                    .prompts
                                    .iter()
                                    .map(|p| p.name.clone())
                                    .collect();
                            }
                        }
                        KeyCode::Char(' ') => {
                            // Toggle individual prompt selection
                            if preview.cursor_index < preview.bank.prompts.len() {
                                let prompt_name =
                                    preview.bank.prompts[preview.cursor_index].name.clone();
                                if preview.selected_prompts.contains(&prompt_name) {
                                    preview.selected_prompts.remove(&prompt_name);
                                    // If we deselected a prompt, turn off "install all"
                                    preview.install_options.install_all = false;
                                } else {
                                    preview.selected_prompts.insert(prompt_name);
                                    // Check if all prompts are now selected
                                    if preview.selected_prompts.len() == preview.bank.prompts.len()
                                    {
                                        preview.install_options.install_all = true;
                                    }
                                }
                            }
                        }
                        KeyCode::Char('b') => {
                            // Toggle create bank option
                            preview.install_options.create_bank =
                                !preview.install_options.create_bank;
                        }
                        KeyCode::Char('m') => {
                            // Toggle merge with existing option
                            preview.install_options.merge_with_existing =
                                !preview.install_options.merge_with_existing;
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            // Move cursor down in prompt list
                            if preview.cursor_index < preview.bank.prompts.len() - 1 {
                                preview.cursor_index += 1;
                            }
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            // Move cursor up in prompt list
                            if preview.cursor_index > 0 {
                                preview.cursor_index -= 1;
                            }
                        }
                        _ => {}
                    }
                }
                InstallStage::Confirm => {
                    match key.code {
                        KeyCode::Char('y') | KeyCode::Enter => {
                            preview.stage = InstallStage::Installing;
                            // Start installation
                            self.perform_installation(storage)?;
                        }
                        KeyCode::Char('n') | KeyCode::Esc => {
                            preview.stage = InstallStage::Preview;
                        }
                        _ => {}
                    }
                }
                InstallStage::Installing => {
                    // Installation in progress, no input handling
                }
                InstallStage::Complete => {
                    match key.code {
                        KeyCode::Enter | KeyCode::Esc => {
                            self.install_preview = None;
                            // Reload data to show newly installed content
                            self.load_data(storage)?;
                            self.update_filtered_items();
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }

    /// Perform the actual installation
    fn perform_installation(&mut self, storage: &Storage) -> Result<()> {
        if let Some(ref mut preview) = self.install_preview {
            let mut installed_count = 0;
            let mut skipped_count = 0;
            let mut errors = Vec::new();
            let mut installed_prompts = Vec::new(); // Track for rollback

            // Create bank directory if requested
            let target_bank = if preview.install_options.create_bank {
                Some(preview.bank.name.clone())
            } else {
                None
            };

            // Install each selected prompt
            for prompt in &preview.bank.prompts {
                if preview.selected_prompts.contains(&prompt.name) {
                    let target_name = if let Some(ref bank) = target_bank {
                        format!("{}/{}", bank, prompt.name)
                    } else {
                        prompt.name.clone()
                    };

                    // Check if prompt already exists
                    if storage.prompt_exists(&target_name) {
                        if preview.install_options.merge_with_existing {
                            // Find a unique name by appending a number
                            let mut counter = 1;
                            let mut unique_name = format!("{}-{}", target_name, counter);
                            while storage.prompt_exists(&unique_name) {
                                counter += 1;
                                unique_name = format!("{}-{}", target_name, counter);
                            }

                            // Create metadata with unique name
                            let metadata = crate::storage::PromptMetadata {
                                id: prompt.name.clone(),
                                description: prompt.description.clone(),
                                tags: if prompt.tags.is_empty() {
                                    None
                                } else {
                                    Some(prompt.tags.clone())
                                },
                                created_at: prompt.created_at.clone(),
                                updated_at: prompt.updated_at.clone(),
                                version: None,
                                git_hash: None,
                                parent_version: None,
                            };

                            // Write the prompt with unique name
                            match storage.write_prompt(&unique_name, &metadata, &prompt.content) {
                                Ok(_) => {
                                    installed_count += 1;
                                    installed_prompts.push(unique_name.clone());
                                }
                                Err(e) => {
                                    errors.push(format!("{}: {}", prompt.name, e));
                                    // Rollback on error if requested
                                    if preview.install_options.merge_with_existing {
                                        rollback_installation(storage, &installed_prompts);
                                        preview.stage = InstallStage::Complete;
                                        self.set_status(&format!("Installation failed: {}", e));
                                        return Ok(());
                                    }
                                }
                            }
                        } else {
                            // Skip existing prompts
                            skipped_count += 1;
                        }
                    } else {
                        // Create metadata
                        let metadata = crate::storage::PromptMetadata {
                            id: prompt.name.clone(),
                            description: prompt.description.clone(),
                            tags: if prompt.tags.is_empty() {
                                None
                            } else {
                                Some(prompt.tags.clone())
                            },
                            created_at: prompt.created_at.clone(),
                            updated_at: prompt.updated_at.clone(),
                            version: None,
                            git_hash: None,
                            parent_version: None,
                        };

                        // Write the prompt
                        match storage.write_prompt(&target_name, &metadata, &prompt.content) {
                            Ok(_) => {
                                installed_count += 1;
                                installed_prompts.push(target_name.clone());
                            }
                            Err(e) => {
                                errors.push(format!("{}: {}", prompt.name, e));
                                // Rollback on critical error
                                if errors.len() > 3 {
                                    rollback_installation(storage, &installed_prompts);
                                    preview.stage = InstallStage::Complete;
                                    self.set_status(
                                        "Installation failed: too many errors, rolled back",
                                    );
                                    return Ok(());
                                }
                            }
                        }
                    }
                }
            }

            // Update stage based on results
            preview.stage = InstallStage::Complete;

            // Add to undo history if we installed anything
            if installed_count > 0 {
                self.state.add_undo_operation(UndoOperation::InstallBank {
                    bank_name: preview.bank.name.clone(),
                    prompts: installed_prompts.clone(),
                    created_bank: preview.install_options.create_bank,
                });
            }

            if errors.is_empty() {
                if skipped_count > 0 {
                    self.set_status(&format!(
                        "Installed {} prompts, skipped {} existing",
                        installed_count, skipped_count
                    ));
                } else {
                    self.set_status(&format!(
                        "Successfully installed {} prompts!",
                        installed_count
                    ));
                }
            } else {
                self.set_status(&format!(
                    "Installed {} prompts ({} skipped, {} errors)",
                    installed_count,
                    skipped_count,
                    errors.len()
                ));
            }
        }
        Ok(())
    }

    /// Draw the install preview/confirmation dialog
    fn draw_install_dialog(&mut self, f: &mut Frame) {
        if let Some(ref preview) = self.install_preview {
            let area = centered_rect(70, 70, f.area());
            f.render_widget(Clear, area);

            match preview.stage {
                InstallStage::Preview => self.draw_install_preview_dialog(f, area, preview),
                InstallStage::Confirm => self.draw_install_confirm_dialog(f, area, preview),
                InstallStage::Installing => self.draw_install_progress_dialog(f, area, preview),
                InstallStage::Complete => self.draw_install_complete_dialog(f, area, preview),
            }
        }
    }

    /// Draw the preview stage of install dialog
    fn draw_install_preview_dialog(&self, f: &mut Frame, area: Rect, preview: &InstallPreview) {
        let mut lines = vec![
            Line::from(vec![Span::styled(
                format!("Install {}", preview.bank.display_name),
                Style::default().add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from(vec![
                Span::raw("Description: "),
                Span::styled(&preview.bank.description, Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::raw("Author: "),
                Span::styled(&preview.bank.author, Style::default().fg(Color::Yellow)),
            ]),
            Line::from(vec![
                Span::raw("Version: "),
                Span::styled(&preview.bank.version, Style::default().fg(Color::Green)),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Prompts to install:",
                Style::default().add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
        ];

        // List prompts with selection indicators
        for (idx, prompt) in preview.bank.prompts.iter().enumerate() {
            let selected = preview.selected_prompts.contains(&prompt.name);
            let checkbox = if selected { "[x]" } else { "[ ]" };
            let cursor = if idx == preview.cursor_index {
                ">"
            } else {
                " "
            };

            let line = format!(
                " {} {} {} - {}",
                cursor, checkbox, prompt.name, prompt.description
            );

            let style = if idx == preview.cursor_index {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };

            lines.push(Line::from(Span::styled(line, style)));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "Options:",
            Style::default().add_modifier(Modifier::BOLD),
        )]));
        lines.push(Line::from(format!(
            "  [{}] Install all prompts (a)",
            if preview.install_options.install_all {
                "x"
            } else {
                " "
            }
        )));
        lines.push(Line::from(format!(
            "  [{}] Create bank folder (b)",
            if preview.install_options.create_bank {
                "x"
            } else {
                " "
            }
        )));
        lines.push(Line::from(format!(
            "  [{}] Auto-rename if exists (m)",
            if preview.install_options.merge_with_existing {
                "x"
            } else {
                " "
            }
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "[↑/↓ j/k] Navigate  [Space] Toggle  [a/b/m] Options  [Enter] Continue  [Esc] Cancel",
            Style::default().fg(Color::DarkGray),
        )]));

        let block = Block::default()
            .title(" Install Bank ")
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::Black));

        let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });

        f.render_widget(paragraph, area);
    }

    /// Draw the confirmation dialog
    fn draw_install_confirm_dialog(&self, f: &mut Frame, area: Rect, preview: &InstallPreview) {
        let selected_count = preview.selected_prompts.len();
        let total_count = preview.bank.prompts.len();

        let lines = vec![
            Line::from(vec![Span::styled(
                "Confirm Installation",
                Style::default().add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from(format!("Bank: {}", preview.bank.display_name)),
            Line::from(format!(
                "Prompts: {} of {} selected",
                selected_count, total_count
            )),
            Line::from(""),
            Line::from("This will:"),
            Line::from(format!(
                "  • Install {} prompt{}",
                selected_count,
                if selected_count == 1 { "" } else { "s" }
            )),
            if preview.install_options.create_bank {
                Line::from(format!("  • Create bank folder: {}", preview.bank.name))
            } else {
                Line::from("  • Install to local prompts")
            },
            Line::from(""),
            Line::from(vec![Span::styled(
                "Continue with installation?",
                Style::default().fg(Color::Yellow),
            )]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "[y/Enter] Yes  [n/Esc] No",
                Style::default().fg(Color::DarkGray),
            )]),
        ];

        let block = Block::default()
            .title(" Confirm Installation ")
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::Black));

        let paragraph = Paragraph::new(lines)
            .block(block)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });

        f.render_widget(paragraph, area);
    }

    /// Draw installation progress dialog
    fn draw_install_progress_dialog(&self, f: &mut Frame, area: Rect, preview: &InstallPreview) {
        let lines = vec![
            Line::from(vec![Span::styled(
                "Installing...",
                Style::default().add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from(format!("Installing bank: {}", preview.bank.display_name)),
            Line::from(""),
            Line::from("⣾ Please wait..."),
        ];

        let block = Block::default()
            .title(" Installation Progress ")
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::Black));

        let paragraph = Paragraph::new(lines)
            .block(block)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });

        f.render_widget(paragraph, area);
    }

    /// Draw installation complete dialog
    fn draw_install_complete_dialog(&self, f: &mut Frame, area: Rect, preview: &InstallPreview) {
        let prompt_word = if preview.selected_prompts.len() == 1 {
            "prompt"
        } else {
            "prompts"
        };
        let location = if preview.install_options.create_bank {
            format!("bank '{}'", preview.bank.name)
        } else {
            "local prompts".to_string()
        };

        let lines = vec![
            Line::from(vec![Span::styled(
                "✓ Installation Complete!",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from(format!(
                "Successfully installed {} {} from {}",
                preview.selected_prompts.len(),
                prompt_word,
                preview.bank.display_name
            )),
            Line::from(format!("Location: {}", location)),
            Line::from(""),
            Line::from(vec![Span::styled(
                "[Enter/Esc] Close",
                Style::default().fg(Color::DarkGray),
            )]),
        ];

        let block = Block::default()
            .title(" Success ")
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::Black));

        let paragraph = Paragraph::new(lines)
            .block(block)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });

        f.render_widget(paragraph, area);
    }

    /// Handle delete with confirmation
    fn handle_delete_with_confirmation(&mut self, _storage: &Storage) -> Result<()> {
        if let Some(selected) = self.list_state.selected() {
            if selected < self.filtered_items.len() {
                let item = self.filtered_items[selected].clone();
                match &item {
                    TreeItem::Bank { bank, .. } => {
                        // Check if bank has prompts before allowing deletion
                        if !bank.prompts.is_empty() {
                            self.status_message = Some((
                                format!("Cannot delete bank '{}' - it contains {} prompt(s). Delete all prompts first.", 
                                    bank.name, bank.prompts.len()),
                                Instant::now()
                            ));
                        } else {
                            // Set up delete confirmation for empty bank
                            self.delete_confirmation = Some(DeleteConfirmation {
                                item_name: bank.name.clone(),
                                item_type: item.clone(),
                            });
                            self.status_message = Some((
                                format!("Delete empty bank '{}'? (y/Enter to confirm, any other key to cancel)", bank.name),
                                Instant::now()
                            ));
                        }
                    }
                    TreeItem::Prompt { prompt, .. } => {
                        // Set up delete confirmation
                        self.delete_confirmation = Some(DeleteConfirmation {
                            item_name: prompt.full_name(),
                            item_type: item.clone(),
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// Handle delete confirmation input
    fn handle_delete_confirmation_input(
        &mut self,
        key: event::KeyEvent,
        storage: &Storage,
    ) -> Result<()> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                // Perform deletion
                if let Some(confirmation) = self.delete_confirmation.take() {
                    let deletion_result = match &confirmation.item_type {
                        TreeItem::Prompt { .. } => storage.delete_prompt(&confirmation.item_name),
                        TreeItem::Bank { .. } => storage.delete_bank(&confirmation.item_name),
                    };

                    match deletion_result {
                        Ok(_) => {
                            // Add to undo stack
                            if let TreeItem::Prompt { prompt, .. } = &confirmation.item_type {
                                self.state.add_undo_operation(UndoOperation::DeletePrompt {
                                    name: prompt.name.clone(),
                                    bank: prompt.bank_name.clone(),
                                    content: prompt.content.clone(),
                                    metadata: crate::storage::PromptMetadata {
                                        id: prompt.name.clone(),
                                        description: prompt.description.clone(),
                                        tags: if prompt.tags.is_empty() {
                                            None
                                        } else {
                                            Some(prompt.tags.clone())
                                        },
                                        created_at: prompt.created_at.clone(),
                                        updated_at: prompt.updated_at.clone(),
                                        version: None,
                                        git_hash: None,
                                        parent_version: None,
                                    },
                                });
                            }

                            // If we're in file focus mode and deleted the focused prompt, exit
                            if self.state.view_mode == ViewMode::FileFocus {
                                if let Some(ref focused) = self.focused_prompt {
                                    if focused.full_name() == confirmation.item_name {
                                        self.state.switch_view_mode(ViewMode::LocalBanks);
                                        self.focused_prompt = None;
                                        self.file_content_scroll = 0;
                                    }
                                }
                            }

                            // Remember current selection position
                            let current_selection = self.list_state.selected().unwrap_or(0);
                            let _total_items = self.filtered_items.len();

                            // Reload data
                            self.load_data(storage)?;
                            self.update_filtered_items();

                            // Smart selection after delete:
                            // - If we deleted the last item, select the new last item
                            // - Otherwise, stay at the same position (which is now the next item)
                            let new_total = self.filtered_items.len();
                            if new_total > 0 {
                                if current_selection >= new_total {
                                    // We deleted the last item, select the new last item
                                    self.list_state.select(Some(new_total - 1));
                                } else {
                                    // Stay at same position (which is now the next item)
                                    self.list_state.select(Some(current_selection));
                                }
                            } else {
                                // No items left
                                self.list_state.select(None);
                            }

                            self.status_message = Some((
                                format!("✓ Deleted '{}'", confirmation.item_name),
                                Instant::now(),
                            ));
                        }
                        Err(e) => {
                            self.status_message =
                                Some((format!("Failed to delete: {}", e), Instant::now()));
                        }
                    }
                }
            }
            _ => {
                // ANY other key cancels deletion (better UX)
                self.delete_confirmation = None;
                self.status_message = Some(("Deletion cancelled".to_string(), Instant::now()));
            }
        }
        Ok(())
    }

    /// Draw delete confirmation dialog
    fn draw_delete_confirmation(&self, f: &mut Frame) {
        if let Some(ref confirmation) = self.delete_confirmation {
            let area = centered_rect(60, 25, f.area());
            f.render_widget(Clear, area);

            let lines = vec![
                Line::from(vec![Span::styled(
                    "Confirm Delete",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )]),
                Line::from(""),
                Line::from(vec![Span::styled(
                    format!("Delete '{}'?", confirmation.item_name),
                    Style::default().fg(Color::White),
                )]),
                Line::from(""),
                Line::from(vec![Span::styled(
                    "This action cannot be undone.",
                    Style::default().fg(Color::Gray),
                )]),
                Line::from(""),
                Line::from(vec![
                    Span::styled(
                        "[Enter or y]",
                        Style::default()
                            .fg(Color::LightGreen)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(" Delete  ", Style::default().fg(Color::White)),
                    Span::styled(
                        "[Any other key]",
                        Style::default()
                            .fg(Color::LightYellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(" Cancel", Style::default().fg(Color::White)),
                ]),
            ];

            let block = Block::default()
                .title(" Delete Confirmation ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::White))
                .style(Style::default().bg(Color::DarkGray));

            let paragraph = Paragraph::new(lines)
                .block(block)
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true });

            f.render_widget(paragraph, area);
        }
    }

    /// Draw the rename input dialog
    fn draw_rename_input(&self, f: &mut Frame) {
        if let Some(ref rename_input) = self.rename_input {
            let area = centered_rect(60, 25, f.area());
            f.render_widget(Clear, area);

            let lines = vec![
                Line::from(vec![Span::styled(
                    "Rename Prompt",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )]),
                Line::from(""),
                Line::from(vec![Span::styled(
                    format!(
                        "Original: {}",
                        rename_input
                            .original_name
                            .split('/')
                            .next_back()
                            .unwrap_or(&rename_input.original_name)
                    ),
                    Style::default().fg(Color::Gray),
                )]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("New name: ", Style::default().fg(Color::White)),
                    Span::styled(
                        &rename_input.new_name,
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("█", Style::default().fg(Color::White)), // cursor
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled(
                        "[Enter]",
                        Style::default()
                            .fg(Color::LightGreen)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(" Confirm  ", Style::default().fg(Color::White)),
                    Span::styled(
                        "[Esc]",
                        Style::default()
                            .fg(Color::LightYellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(" Cancel", Style::default().fg(Color::White)),
                ]),
            ];

            let block = Block::default()
                .title(" Rename ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::White))
                .style(Style::default().bg(Color::DarkGray));

            let paragraph = Paragraph::new(lines)
                .block(block)
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true });

            f.render_widget(paragraph, area);
        }
    }

    /// Draw the new prompt input dialog
    fn draw_new_prompt_input(&self, f: &mut Frame) {
        if let Some(ref input) = self.new_prompt_input {
            let area = centered_rect(60, 25, f.area());
            f.render_widget(Clear, area);

            let bank_info = if let Some(ref bank) = input.target_bank {
                format!("Target bank: {}", bank)
            } else {
                "Target: Local prompts".to_string()
            };

            let lines = vec![
                Line::from(vec![Span::styled(
                    "New Prompt",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )]),
                Line::from(""),
                Line::from(vec![Span::styled(
                    bank_info,
                    Style::default().fg(Color::Gray),
                )]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Name: ", Style::default().fg(Color::White)),
                    Span::styled(
                        &input.name,
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("█", Style::default().fg(Color::White)), // cursor
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled(
                        "[Enter]",
                        Style::default()
                            .fg(Color::LightGreen)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(" Create  ", Style::default().fg(Color::White)),
                    Span::styled(
                        "[Esc]",
                        Style::default()
                            .fg(Color::LightYellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(" Cancel", Style::default().fg(Color::White)),
                ]),
            ];

            let block = Block::default()
                .title(" New Prompt ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::White))
                .style(Style::default().bg(Color::DarkGray));

            let paragraph = Paragraph::new(lines)
                .block(block)
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true });

            f.render_widget(paragraph, area);
        }
    }

    /// Draw the new bank input dialog
    fn draw_new_bank_input(&self, f: &mut Frame) {
        if let Some(ref input) = self.new_bank_input {
            let area = centered_rect(60, 25, f.area());
            f.render_widget(Clear, area);

            let lines = vec![
                Line::from(vec![Span::styled(
                    "New Bank",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Name: ", Style::default().fg(Color::White)),
                    Span::styled(
                        &input.name,
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("█", Style::default().fg(Color::White)), // cursor
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled(
                        "[Enter]",
                        Style::default()
                            .fg(Color::LightGreen)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(" Create  ", Style::default().fg(Color::White)),
                    Span::styled(
                        "[Esc]",
                        Style::default()
                            .fg(Color::LightYellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(" Cancel", Style::default().fg(Color::White)),
                ]),
            ];

            let block = Block::default()
                .title(" New Bank ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::White))
                .style(Style::default().bg(Color::DarkGray));

            let paragraph = Paragraph::new(lines)
                .block(block)
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true });

            f.render_widget(paragraph, area);
        }
    }

    fn handle_new_prompt_input(&mut self, key: event::KeyEvent, storage: &Storage) -> Result<()> {
        if let Some(ref mut input) = self.new_prompt_input {
            match key.code {
                KeyCode::Esc => {
                    self.new_prompt_input = None;
                    self.status_message =
                        Some(("Prompt creation cancelled".to_string(), Instant::now()));
                }
                KeyCode::Enter => {
                    if !input.name.trim().is_empty() {
                        let prompt_name = input.name.trim().to_string();
                        let target_bank = input.target_bank.clone();

                        // Create the prompt
                        let metadata = crate::storage::PromptMetadata {
                            id: prompt_name.clone(),
                            description: "Edit this description".to_string(),
                            tags: Some(vec!["new".to_string()]),
                            created_at: Some(chrono::Utc::now().to_rfc3339()),
                            updated_at: None,
                            version: None,
                            git_hash: None,
                            parent_version: None,
                        };

                        let content = format!(
                            "# {}\n\nReplace this with your prompt content.\n\nUse {{input}} for user input placeholder.",
                            prompt_name
                        );

                        let full_name = if let Some(ref bank) = target_bank {
                            format!("{}/{}", bank, prompt_name)
                        } else {
                            prompt_name.clone()
                        };

                        match storage.write_prompt(&full_name, &metadata, &content) {
                            Ok(_) => {
                                self.status_message = Some((
                                    format!("✓ Created prompt '{}'", prompt_name),
                                    Instant::now(),
                                ));

                                // Reload data
                                self.load_data(storage)?;
                                self.update_filtered_items();
                            }
                            Err(e) => {
                                self.status_message = Some((
                                    format!("Failed to create prompt: {}", e),
                                    Instant::now(),
                                ));
                            }
                        }
                    } else {
                        self.status_message =
                            Some(("Name cannot be empty".to_string(), Instant::now()));
                    }

                    self.new_prompt_input = None;
                }
                KeyCode::Backspace => {
                    input.name.pop();
                }
                KeyCode::Char(c) => {
                    input.name.push(c);
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn handle_new_bank_input(&mut self, key: event::KeyEvent, storage: &Storage) -> Result<()> {
        if let Some(ref mut input) = self.new_bank_input {
            match key.code {
                KeyCode::Esc => {
                    self.new_bank_input = None;
                    self.status_message =
                        Some(("Bank creation cancelled".to_string(), Instant::now()));
                }
                KeyCode::Enter => {
                    if !input.name.trim().is_empty() {
                        let bank_name = input.name.trim().to_string();

                        // Create the bank directory
                        let bank_path = storage.base_dir().join("banks").join(&bank_name);
                        match std::fs::create_dir_all(&bank_path) {
                            Ok(_) => {
                                self.status_message = Some((
                                    format!("✓ Created bank '{}'", bank_name),
                                    Instant::now(),
                                ));

                                // Reload data
                                self.load_data(storage)?;
                                self.update_filtered_items();
                            }
                            Err(e) => {
                                self.status_message =
                                    Some((format!("Failed to create bank: {}", e), Instant::now()));
                            }
                        }
                    } else {
                        self.status_message =
                            Some(("Name cannot be empty".to_string(), Instant::now()));
                    }

                    self.new_bank_input = None;
                }
                KeyCode::Backspace => {
                    input.name.pop();
                }
                KeyCode::Char(c) => {
                    input.name.push(c);
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Handle 'l' key - either expand bank or enter file focus mode
    fn handle_l_key(&mut self, storage: &Storage) -> Result<()> {
        if let Some(selected) = self.list_state.selected() {
            if selected < self.filtered_items.len() {
                let item = self.filtered_items[selected].clone();
                match item {
                    TreeItem::Bank { .. } => {
                        // For banks, expand/collapse
                        self.handle_expand_or_use(storage)?;
                    }
                    TreeItem::Prompt { mut prompt, .. } => {
                        // For prompts, read the content and enter file focus mode
                        match storage.read_prompt(&prompt.full_name()) {
                            Ok((_, content)) => {
                                prompt.content = content;
                                self.focused_prompt = Some(prompt);
                                self.state.switch_view_mode(ViewMode::FileFocus);
                                self.file_content_scroll = 0;
                                self.status_message = Some((
                                    "File focus mode - [j/k] scroll, [e] edit info, [q/h] back"
                                        .to_string(),
                                    Instant::now(),
                                ));
                            }
                            Err(e) => {
                                self.status_message =
                                    Some((format!("Error reading prompt: {}", e), Instant::now()));
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    // Enhanced productivity handler methods

    fn handle_mark_item(&mut self) -> Result<()> {
        if let Some(selected) = self.list_state.selected() {
            if selected < self.filtered_items.len() {
                let item = &self.filtered_items[selected];
                let item_path = match item {
                    TreeItem::Prompt { prompt, .. } => prompt.full_name(),
                    TreeItem::Bank { bank, .. } => bank.name.clone(),
                };
                
                // Set waiting state - don't set bookmark yet
                self.waiting_for_bookmark_key = Some(item_path);
                self.status_message = Some((
                    format!("Press a letter (a-z) to set bookmark for '{}'", 
                           match item {
                               TreeItem::Prompt { prompt, .. } => &prompt.name,
                               TreeItem::Bank { bank, .. } => &bank.name,
                           }),
                    Instant::now(),
                ));
            }
        }
        Ok(())
    }

    fn handle_goto_mark(&mut self) -> Result<()> {
        // Set waiting state to get bookmark key
        self.waiting_for_goto_key = true;
        self.status_message = Some((
            "Press a letter (a-z) to jump to bookmark".to_string(),
            Instant::now(),
        ));
        Ok(())
    }

    fn navigate_to_bookmark(&mut self, bookmark_path: String) -> Result<()> {
        // Find the item in the filtered list
        let position = self.filtered_items.iter().position(|item| {
            match item {
                TreeItem::Prompt { prompt, .. } => prompt.full_name() == bookmark_path,
                TreeItem::Bank { bank, .. } => bank.name == bookmark_path,
            }
        });

        if let Some(index) = position {
            // Update the selection
            self.list_state.select(Some(index));
            
            // If it's a bank, make sure it's expanded
            if let TreeItem::Bank { bank, .. } = &self.filtered_items[index] {
                let bank_name = bank.name.clone();
                if !self.state.is_expanded(&bank_name) {
                    self.state.toggle_expanded(&bank_name);
                    self.update_filtered_items();
                    // Re-find the position after updating filtered items
                    if let Some(new_index) = self.filtered_items.iter().position(|item| {
                        match item {
                            TreeItem::Bank { bank: b, .. } => b.name == bank_name,
                            _ => false,
                        }
                    }) {
                        self.list_state.select(Some(new_index));
                    }
                }
            }
        } else {
            self.status_message = Some((
                format!("Bookmarked item '{}' not found", bookmark_path),
                Instant::now(),
            ));
        }
        Ok(())
    }

    fn handle_open_external(&mut self) -> Result<()> {
        if let Some(selected) = self.list_state.selected() {
            if selected < self.filtered_items.len() {
                let item = &self.filtered_items[selected];
                match item {
                    TreeItem::Prompt { prompt, .. } => {
                        self.status_message = Some((
                            format!("Would open '{}' in external editor", prompt.name),
                            Instant::now(),
                        ));
                        // TODO: Implement external editor opening
                    }
                    TreeItem::Bank { bank, .. } => {
                        self.status_message = Some((
                            format!("Would open bank '{}' directory", bank.name),
                            Instant::now(),
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    fn handle_open_directory(&mut self) -> Result<()> {
        self.status_message = Some((
            "Would open current directory in file manager".to_string(),
            Instant::now(),
        ));
        // TODO: Implement directory opening in file manager
        Ok(())
    }

    fn handle_tag_item(&mut self) -> Result<()> {
        if let Some(selected) = self.list_state.selected() {
            if selected < self.filtered_items.len() {
                let item = &self.filtered_items[selected];
                let item_path = match item {
                    TreeItem::Prompt { prompt, .. } => prompt.full_name(),
                    TreeItem::Bank { bank, .. } => bank.name.clone(),
                };
                
                // For now, add a default tag
                self.state.add_tag(&item_path, "tagged");
                self.status_message = Some((
                    format!("Tagged item '{}' with 'tagged'", 
                           match item {
                               TreeItem::Prompt { prompt, .. } => &prompt.name,
                               TreeItem::Bank { bank, .. } => &bank.name,
                           }),
                    Instant::now(),
                ));
            }
        }
        Ok(())
    }

    fn handle_filter_by_tag(&mut self) -> Result<()> {
        self.status_message = Some((
            "Tag filtering mode - showing tagged items only".to_string(),
            Instant::now(),
        ));
        // TODO: Implement tag-based filtering
        Ok(())
    }

    fn handle_workspace_save(&mut self) -> Result<()> {
        let workspace_name = format!("workspace_{}", chrono::Utc::now().format("%Y%m%d_%H%M%S"));
        self.state.save_workspace(workspace_name.clone());
        self.status_message = Some((
            format!("Saved current state as workspace '{}'", workspace_name),
            Instant::now(),
        ));
        Ok(())
    }

    fn handle_workspace_load(&mut self) -> Result<()> {
        // For now, try to load the most recent workspace
        if let Some(workspace_name) = self.state.workspaces.keys().last().cloned() {
            if self.state.load_workspace(&workspace_name) {
                self.status_message = Some((
                    format!("Loaded workspace '{}'", workspace_name),
                    Instant::now(),
                ));
                self.update_filtered_items();
            } else {
                self.status_message = Some((
                    "Failed to load workspace".to_string(),
                    Instant::now(),
                ));
            }
        } else {
            self.status_message = Some((
                "No workspaces saved. Use 'w' to save current state.".to_string(),
                Instant::now(),
            ));
        }
        Ok(())
    }

    fn handle_command_palette(&mut self) -> Result<()> {
        self.state.command_palette_visible = !self.state.command_palette_visible;
        if self.state.command_palette_visible {
            self.status_message = Some((
                "Command palette opened - type to search commands".to_string(),
                Instant::now(),
            ));
        } else {
            self.status_message = Some((
                "Command palette closed".to_string(),
                Instant::now(),
            ));
        }
        Ok(())
    }

    fn handle_quick_jump(&mut self, key_code: KeyCode) -> Result<()> {
        if let KeyCode::Char(c) = key_code {
            if let Some(digit) = c.to_digit(10) {
                let index = digit as usize;
                if let Some(item_path) = self.state.get_quick_jump(index) {
                    self.status_message = Some((
                        format!("Jumping to quick slot {}: {}", index, item_path),
                        Instant::now(),
                    ));
                    // TODO: Implement navigation to quick jump item
                } else {
                    // Set current item as quick jump
                    if let Some(selected) = self.list_state.selected() {
                        if selected < self.filtered_items.len() {
                            let item = &self.filtered_items[selected];
                            let item_path = match item {
                                TreeItem::Prompt { prompt, .. } => prompt.full_name(),
                                TreeItem::Bank { bank, .. } => bank.name.clone(),
                            };
                            self.state.set_quick_jump(index, item_path.clone());
                            self.status_message = Some((
                                format!("Set quick jump slot {} to '{}'", index, item_path),
                                Instant::now(),
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn handle_fuzzy_search(&mut self) -> Result<()> {
        self.state.fuzzy_search_mode = !self.state.fuzzy_search_mode;
        if self.state.fuzzy_search_mode {
            self.status_message = Some((
                "Fuzzy search mode enabled - type to search".to_string(),
                Instant::now(),
            ));
        } else {
            self.status_message = Some((
                "Fuzzy search mode disabled".to_string(),
                Instant::now(),
            ));
        }
        Ok(())
    }

    fn handle_filter_recent(&mut self) -> Result<()> {
        self.state.switch_view_mode(ViewMode::Recent);
        self.update_filtered_items();
        self.status_message = Some((
            "Showing recent items only".to_string(),
            Instant::now(),
        ));
        Ok(())
    }

    fn handle_goto_bank(&mut self) -> Result<()> {
        self.status_message = Some((
            "Bank navigation mode - type bank name to jump".to_string(),
            Instant::now(),
        ));
        // TODO: Implement bank-specific navigation
        Ok(())
    }

    fn handle_bulk_operations(&mut self) -> Result<()> {
        self.state.bulk_operation_mode = !self.state.bulk_operation_mode;
        if self.state.bulk_operation_mode {
            self.status_message = Some((
                "Bulk operations mode - select multiple items with Space".to_string(),
                Instant::now(),
            ));
        } else {
            self.status_message = Some((
                "Exited bulk operations mode".to_string(),
                Instant::now(),
            ));
        }
        Ok(())
    }
}

/// Rollback installation by deleting installed prompts
fn rollback_installation(storage: &Storage, installed_prompts: &[String]) {
    for prompt_name in installed_prompts {
        // Try to delete the prompt file
        if let Err(e) = storage.delete_prompt(prompt_name) {
            eprintln!("Failed to rollback prompt '{}': {}", prompt_name, e);
        }
    }
}

// Helper function for centering rectangles (for overlays)
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
