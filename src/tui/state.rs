use super::clipboard::ClipboardItem;
use std::collections::{HashMap, HashSet};

/// Main TUI state management
#[derive(Debug, Clone)]
pub struct TuiState {
    pub current_path: Vec<String>,
    pub expanded_folders: HashSet<String>,
    pub selected_items: HashSet<String>,
    pub cursor_position: usize,
    pub view_mode: ViewMode,
    pub search_query: Option<String>,
    pub compose_queue: Vec<String>,
    pub clipboard: Option<ClipboardItem>,
    pub undo_stack: Vec<UndoOperation>,
    pub favorites: HashSet<String>,
    pub usage_stats: HashMap<String, u32>,
    pub sort_mode: SortMode,
    
    // Enhanced productivity state
    pub bookmarks: HashMap<char, String>, // character -> item path
    pub tags: HashMap<String, HashSet<String>>, // item -> tags
    pub workspaces: HashMap<String, WorkspaceState>,
    pub current_workspace: Option<String>,
    pub recent_items: Vec<String>, // LRU cache of recent items
    pub quick_jump_items: Vec<Option<String>>, // 1-9 quick jump slots
    pub command_palette_visible: bool,
    pub fuzzy_search_mode: bool,
    pub bulk_operation_mode: bool,
    
    // Cursor position tracking per view mode
    pub view_cursor_positions: HashMap<ViewMode, usize>,
}

/// Different view modes for the TUI
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ViewMode {
    LocalBanks, // Default bank browser
    Registry,   // Registry discovery mode
    Search,     // Search results
    BankDetail, // Individual bank expanded
    Installing, // Installation progress
    Favorites,  // Favorites-only view
    Recent,     // Recent prompts view
    Compose,    // Compose queue management
    FileFocus,  // File focus view for reading/scrolling prompts
}

/// Sorting modes for prompts/banks
#[derive(Debug, Clone, PartialEq)]
pub enum SortMode {
    Alphabetical,
    Usage,
    Modified,
    Created,
}

/// Workspace state for saving/restoring TUI configurations
#[derive(Debug, Clone)]
pub struct WorkspaceState {
    pub expanded_folders: HashSet<String>,
    pub view_mode: ViewMode,
    pub sort_mode: SortMode,
    pub current_path: Vec<String>,
    pub favorites: HashSet<String>,
    pub compose_queue: Vec<String>,
    pub tags: HashMap<String, HashSet<String>>,
}

/// Operations that can be undone
#[derive(Debug, Clone)]
pub enum UndoOperation {
    DeletePrompt {
        name: String,
        bank: Option<String>,
        content: String,
        metadata: crate::storage::PromptMetadata,
    },
    MovePrompt {
        name: String,
        from_bank: Option<String>,
        to_bank: Option<String>,
    },
    RenamePrompt {
        old_name: String,
        new_name: String,
        bank: Option<String>,
    },
    DeleteBank {
        name: String,
        prompts: Vec<(String, String, crate::storage::PromptMetadata)>, // name, content, metadata
    },
    CreatePrompt {
        name: String,
        bank: Option<String>,
    },
    CreateBank {
        name: String,
    },
    InstallBank {
        bank_name: String,
        prompts: Vec<String>, // List of installed prompt names
        created_bank: bool,
    },
}

impl TuiState {
    pub fn new() -> Self {
        Self {
            current_path: Vec::new(),
            expanded_folders: HashSet::new(),
            selected_items: HashSet::new(),
            cursor_position: 0,
            view_mode: ViewMode::LocalBanks,
            search_query: None,
            compose_queue: Vec::new(),
            clipboard: None,
            undo_stack: Vec::new(),
            favorites: HashSet::new(),
            usage_stats: HashMap::new(),
            sort_mode: SortMode::Alphabetical,
            
            // Enhanced productivity state
            bookmarks: HashMap::new(),
            tags: HashMap::new(),
            workspaces: HashMap::new(),
            current_workspace: None,
            recent_items: Vec::new(),
            quick_jump_items: vec![None; 9],
            command_palette_visible: false,
            fuzzy_search_mode: false,
            bulk_operation_mode: false,
            
            // Cursor position tracking per view mode
            view_cursor_positions: HashMap::new(),
        }
    }

    /// Check if currently in a specific bank
    pub fn current_bank(&self) -> Option<&String> {
        self.current_path.first()
    }

    /// Check if a bank/folder is expanded
    pub fn is_expanded(&self, name: &str) -> bool {
        self.expanded_folders.contains(name)
    }

    /// Toggle expansion state of a bank/folder
    pub fn toggle_expanded(&mut self, name: &str) {
        if self.expanded_folders.contains(name) {
            self.expanded_folders.remove(name);
        } else {
            self.expanded_folders.insert(name.to_string());
        }
    }

    /// Check if a directory within a bank is expanded
    pub fn is_directory_expanded(&self, bank_name: &str, dir_path: &str) -> bool {
        let full_path = format!("{}/{}", bank_name, dir_path);
        self.expanded_folders.contains(&full_path)
    }

    /// Toggle expansion state of a directory within a bank
    pub fn toggle_directory_expanded(&mut self, bank_name: &str, dir_path: &str) {
        let full_path = format!("{}/{}", bank_name, dir_path);
        if self.expanded_folders.contains(&full_path) {
            self.expanded_folders.remove(&full_path);
        } else {
            self.expanded_folders.insert(full_path);
        }
    }

    /// Check if an item is selected
    pub fn is_selected(&self, name: &str) -> bool {
        self.selected_items.contains(name)
    }

    /// Toggle selection of an item
    pub fn toggle_selection(&mut self, name: &str) {
        if self.selected_items.contains(name) {
            self.selected_items.remove(name);
        } else {
            self.selected_items.insert(name.to_string());
        }
    }

    /// Clear all selections
    pub fn clear_selections(&mut self) {
        self.selected_items.clear();
    }

    /// Check if an item is favorited
    pub fn is_favorite(&self, name: &str) -> bool {
        self.favorites.contains(name)
    }

    /// Toggle favorite status of an item
    pub fn toggle_favorite(&mut self, name: &str) {
        if self.favorites.contains(name) {
            self.favorites.remove(name);
        } else {
            self.favorites.insert(name.to_string());
        }
    }

    /// Get usage count for an item
    pub fn usage_count(&self, name: &str) -> u32 {
        self.usage_stats.get(name).copied().unwrap_or(0)
    }

    /// Increment usage count for an item
    pub fn increment_usage(&mut self, name: &str) {
        let count = self.usage_stats.get(name).copied().unwrap_or(0);
        self.usage_stats.insert(name.to_string(), count + 1);
    }

    /// Navigate into a bank/folder
    pub fn navigate_into(&mut self, name: &str) {
        self.current_path.push(name.to_string());
        self.cursor_position = 0;
    }

    /// Navigate back up one level
    pub fn navigate_back(&mut self) {
        if !self.current_path.is_empty() {
            self.current_path.pop();
            self.cursor_position = 0;
        }
    }

    /// Navigate to root
    pub fn navigate_to_root(&mut self) {
        self.current_path.clear();
        self.cursor_position = 0;
    }

    /// Get current path as string
    pub fn current_path_string(&self) -> String {
        if self.current_path.is_empty() {
            "PromptHive".to_string()
        } else {
            format!("PromptHive / {}", self.current_path.join(" / "))
        }
    }

    /// Add item to compose queue
    pub fn add_to_compose_queue(&mut self, name: &str) {
        if !self.compose_queue.contains(&name.to_string()) {
            self.compose_queue.push(name.to_string());
        }
    }

    /// Remove item from compose queue
    pub fn remove_from_compose_queue(&mut self, name: &str) {
        self.compose_queue.retain(|item| item != name);
    }

    /// Clear compose queue
    pub fn clear_compose_queue(&mut self) {
        self.compose_queue.clear();
    }

    /// Check if item is in compose queue
    pub fn is_in_compose_queue(&self, name: &str) -> bool {
        self.compose_queue.contains(&name.to_string())
    }

    /// Add operation to undo stack (max 10 operations)
    pub fn add_undo_operation(&mut self, operation: UndoOperation) {
        self.undo_stack.push(operation);
        if self.undo_stack.len() > 10 {
            self.undo_stack.remove(0);
        }
    }

    /// Pop last operation from undo stack
    pub fn pop_undo_operation(&mut self) -> Option<UndoOperation> {
        self.undo_stack.pop()
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Get display string for current view mode
    pub fn view_mode_display(&self) -> &'static str {
        match self.view_mode {
            ViewMode::LocalBanks => "Local Banks",
            ViewMode::Registry => "Registry",
            ViewMode::Search => "Search",
            ViewMode::BankDetail => "Bank Detail",
            ViewMode::Installing => "Installing",
            ViewMode::Favorites => "Favorites",
            ViewMode::Recent => "Recent",
            ViewMode::Compose => "Compose Queue",
            ViewMode::FileFocus => "File Focus",
        }
    }

    /// Get search query display string
    pub fn search_display(&self) -> String {
        match &self.search_query {
            Some(query) => {
                if self.view_mode == ViewMode::Search {
                    format!(" [/{}]", query)
                } else {
                    format!(" [{}]", query)
                }
            }
            None => String::new(),
        }
    }

    /// Get compose queue display string
    pub fn compose_queue_display(&self) -> String {
        if self.compose_queue.is_empty() {
            String::new()
        } else {
            format!(" 🔗{}", self.compose_queue.len())
        }
    }

    /// Get clipboard display string  
    pub fn clipboard_display(&self) -> String {
        if let Some(ref clipboard) = self.clipboard {
            format!(" 📋{}", clipboard.display())
        } else {
            String::new()
        }
    }

    /// Get selection display string
    pub fn selection_display(&self) -> String {
        if self.selected_items.is_empty() {
            String::new()
        } else {
            format!(" ✓{}", self.selected_items.len())
        }
    }

    // Enhanced productivity methods

    /// Set bookmark for quick navigation
    pub fn set_bookmark(&mut self, key: char, item_path: String) {
        self.bookmarks.insert(key, item_path);
    }

    /// Get bookmark path
    pub fn get_bookmark(&self, key: char) -> Option<&String> {
        self.bookmarks.get(&key)
    }

    /// Add tag to item
    pub fn add_tag(&mut self, item: &str, tag: &str) {
        self.tags.entry(item.to_string()).or_insert_with(HashSet::new).insert(tag.to_string());
    }

    /// Remove tag from item
    pub fn remove_tag(&mut self, item: &str, tag: &str) {
        if let Some(tags) = self.tags.get_mut(item) {
            tags.remove(tag);
            if tags.is_empty() {
                self.tags.remove(item);
            }
        }
    }

    /// Get tags for item
    pub fn get_tags(&self, item: &str) -> Option<&HashSet<String>> {
        self.tags.get(item)
    }

    /// Save current state as workspace
    pub fn save_workspace(&mut self, name: String) {
        let workspace = WorkspaceState {
            expanded_folders: self.expanded_folders.clone(),
            view_mode: self.view_mode.clone(),
            sort_mode: self.sort_mode.clone(),
            current_path: self.current_path.clone(),
            favorites: self.favorites.clone(),
            compose_queue: self.compose_queue.clone(),
            tags: self.tags.clone(),
        };
        self.workspaces.insert(name.clone(), workspace);
        self.current_workspace = Some(name);
    }

    /// Load workspace state
    pub fn load_workspace(&mut self, name: &str) -> bool {
        if let Some(workspace) = self.workspaces.get(name).cloned() {
            self.expanded_folders = workspace.expanded_folders;
            self.view_mode = workspace.view_mode;
            self.sort_mode = workspace.sort_mode;
            self.current_path = workspace.current_path;
            self.favorites = workspace.favorites;
            self.compose_queue = workspace.compose_queue;
            self.tags = workspace.tags;
            self.current_workspace = Some(name.to_string());
            true
        } else {
            false
        }
    }

    /// Add item to recent items (LRU)
    pub fn add_recent_item(&mut self, item: String) {
        // Remove if already exists to move to front
        self.recent_items.retain(|i| i != &item);
        // Add to front
        self.recent_items.insert(0, item);
        // Keep only last 20 items
        if self.recent_items.len() > 20 {
            self.recent_items.truncate(20);
        }
    }

    /// Set quick jump item (1-9)
    pub fn set_quick_jump(&mut self, index: usize, item: String) {
        if index > 0 && index <= 9 && index - 1 < self.quick_jump_items.len() {
            self.quick_jump_items[index - 1] = Some(item);
        }
    }

    /// Get quick jump item (1-9)
    pub fn get_quick_jump(&self, index: usize) -> Option<&String> {
        if index > 0 && index <= 9 && index - 1 < self.quick_jump_items.len() {
            self.quick_jump_items[index - 1].as_ref()
        } else {
            None
        }
    }

    /// Get workspace display string
    pub fn workspace_display(&self) -> String {
        if let Some(ref workspace) = self.current_workspace {
            format!(" 💾{}", workspace)
        } else {
            String::new()
        }
    }

    /// Get tags display for item
    pub fn tags_display(&self, item: &str) -> String {
        if let Some(tags) = self.tags.get(item) {
            if !tags.is_empty() {
                format!(" #{}", tags.iter().take(2).cloned().collect::<Vec<_>>().join(","))
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    }

    /// Save current cursor position for the current view mode
    pub fn save_cursor_position(&mut self) {
        self.view_cursor_positions.insert(self.view_mode.clone(), self.cursor_position);
    }

    /// Switch to a new view mode while preserving cursor positions
    pub fn switch_view_mode(&mut self, new_mode: ViewMode) {
        // Save current cursor position for current mode
        self.save_cursor_position();
        
        // Switch to new mode
        self.view_mode = new_mode.clone();
        
        // Restore cursor position for new mode (or default to 0)
        self.cursor_position = self.view_cursor_positions.get(&new_mode).copied().unwrap_or(0);
    }

    /// Update cursor position and sync with view mode tracking
    pub fn update_cursor_position(&mut self, position: usize) {
        self.cursor_position = position;
        self.view_cursor_positions.insert(self.view_mode.clone(), position);
    }
}

impl Default for TuiState {
    fn default() -> Self {
        Self::new()
    }
}
