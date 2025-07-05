use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use std::{
    io,
    time::{Duration, Instant},
};

use crate::{Clipboard, Storage};

#[derive(Debug, Clone)]
pub struct Prompt {
    pub name: String,
    pub description: String,
    pub content: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub tags: Vec<String>,
}

pub struct PromptTui {
    prompts: Vec<Prompt>,
    filtered: Vec<usize>,
    list_state: ListState,
    search_query: String,
    mode: Mode,
    preview_scroll: u16,
    status_message: Option<(String, Instant)>,
    fuzzy_matcher: SkimMatcherV2,
    pending_delete: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Normal,
    Search,
}

impl PromptTui {
    pub fn new(storage: &Storage) -> Result<Self> {
        let prompts = load_prompts(storage)?;
        let filtered: Vec<usize> = (0..prompts.len()).collect();
        
        let mut list_state = ListState::default();
        if !prompts.is_empty() {
            list_state.select(Some(0));
        }

        Ok(Self {
            prompts,
            filtered,
            list_state,
            search_query: String::new(),
            mode: Mode::Normal,
            preview_scroll: 0,
            status_message: None,
            fuzzy_matcher: SkimMatcherV2::default(),
            pending_delete: None,
        })
    }

    pub fn set_initial_search(&mut self, query: &str) {
        self.search_query = query.to_string();
        self.mode = Mode::Search;
        self.update_filter();
        
        // Select first match if any
        if !self.filtered.is_empty() {
            self.list_state.select(Some(0));
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
            terminal.draw(|f| self.ui(f))?;

            if let Event::Key(key) = event::read()? {
                match self.mode {
                    Mode::Normal => match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Char('/') => {
                            self.mode = Mode::Search;
                            self.search_query.clear();
                        }
                        KeyCode::Char('j') | KeyCode::Down => self.next(),
                        KeyCode::Char('k') | KeyCode::Up => self.previous(),
                        KeyCode::Char('g') => self.first(),
                        KeyCode::Char('G') => self.last(),
                        KeyCode::Enter | KeyCode::Char('u') | KeyCode::Char('l') => self.use_prompt()?,
                        KeyCode::Char('s') => self.show_prompt()?,
                        KeyCode::Char('e') => self.edit_prompt(storage)?,
                        KeyCode::Char('d') => {
                            if self.pending_delete.is_some() {
                                self.confirm_delete(storage)?
                            } else {
                                self.delete_prompt(storage)?
                            }
                        }
                        KeyCode::Char('n') => self.new_prompt(storage)?,
                        KeyCode::Char('h') => {
                            // For now, h just shows a message. Later it can navigate back from detail view
                            self.status_message = Some((
                                "Press 'q' to quit, '/' to search".to_string(),
                                Instant::now(),
                            ));
                        }
                        KeyCode::Char('?') => self.show_help(),
                        KeyCode::PageDown => self.scroll_preview_down(),
                        KeyCode::PageUp => self.scroll_preview_up(),
                        _ => {
                            // Cancel delete if any other key pressed
                            if self.pending_delete.is_some() {
                                self.pending_delete = None;
                                self.status_message = Some((
                                    "Delete cancelled".to_string(),
                                    Instant::now(),
                                ));
                            }
                        }
                    },
                    Mode::Search => match key.code {
                        KeyCode::Esc => {
                            self.mode = Mode::Normal;
                            self.search_query.clear();
                            self.update_filter();
                        }
                        KeyCode::Enter => {
                            self.mode = Mode::Normal;
                            self.update_filter();
                        }
                        KeyCode::Char(c) => {
                            self.search_query.push(c);
                            self.update_filter();
                        }
                        KeyCode::Backspace => {
                            self.search_query.pop();
                            self.update_filter();
                        }
                        _ => {}
                    },
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

    fn ui(&mut self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)].as_ref())
            .split(f.area());

        self.draw_prompt_list(f, chunks[0]);
        self.draw_preview(f, chunks[1]);
        self.draw_status_bar(f);
    }

    fn draw_prompt_list(&mut self, f: &mut Frame, area: Rect) {
        let search_indicator = if self.mode == Mode::Search {
            format!(" [/{}]", self.search_query)
        } else if !self.search_query.is_empty() {
            format!(" [/{}]", self.search_query)
        } else {
            String::new()
        };

        let title = format!(" 📋 Prompts ({}){} ", self.filtered.len(), search_indicator);
        
        let items: Vec<ListItem> = self
            .filtered
            .iter()
            .map(|&i| {
                let prompt = &self.prompts[i];
                let content = format!("{:<20} {}", 
                    if prompt.name.len() > 20 {
                        format!("{}...", &prompt.name[..17])
                    } else {
                        prompt.name.clone()
                    },
                    prompt.description
                );
                ListItem::new(content)
            })
            .collect();

        let highlight_style = Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD);

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(title))
            .highlight_style(highlight_style)
            .highlight_symbol("> ");

        f.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn draw_preview(&mut self, f: &mut Frame, area: Rect) {
        if let Some(selected) = self.list_state.selected() {
            if selected < self.filtered.len() {
                let prompt = &self.prompts[self.filtered[selected]];
                
                let mut content = vec![
                    Line::from(vec![Span::styled(
                        format!("# {}", prompt.name),
                        Style::default().add_modifier(Modifier::BOLD),
                    )]),
                    Line::from(""),
                ];

                // Add content lines
                for line in prompt.content.lines() {
                    content.push(Line::from(line.to_string()));
                }

                // Add metadata
                content.push(Line::from(""));
                if let Some(created) = &prompt.created_at {
                    content.push(Line::from(vec![
                        Span::raw("Created: "),
                        Span::styled(created, Style::default().fg(Color::DarkGray)),
                    ]));
                }
                if let Some(updated) = &prompt.updated_at {
                    content.push(Line::from(vec![
                        Span::raw("Updated: "),
                        Span::styled(updated, Style::default().fg(Color::DarkGray)),
                    ]));
                }
                if !prompt.tags.is_empty() {
                    content.push(Line::from(vec![
                        Span::raw("Tags: "),
                        Span::styled(
                            prompt.tags.join(", "),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]));
                }

                let paragraph = Paragraph::new(content)
                    .block(Block::default().borders(Borders::ALL).title(" Preview "))
                    .wrap(Wrap { trim: true })
                    .scroll((self.preview_scroll, 0));

                f.render_widget(paragraph, area);
            }
        } else {
            let paragraph = Paragraph::new("No prompt selected")
                .block(Block::default().borders(Borders::ALL).title(" Preview "))
                .alignment(Alignment::Center);
            f.render_widget(paragraph, area);
        }
    }

    fn draw_status_bar(&mut self, f: &mut Frame) {
        let area = Rect {
            x: 0,
            y: f.area().height - 1,
            width: f.area().width,
            height: 1,
        };

        let help_text = if let Some((msg, _)) = &self.status_message {
            msg.clone()
        } else {
            match self.mode {
                Mode::Normal => {
                    " [↑↓/jk/hl] Navigate  [/] Search  [Enter/u/l] Use  [e] Edit  [n] New  [q] Quit".to_string()
                }
                Mode::Search => {
                    " [Esc] Cancel  [Enter] Confirm  Type to search...".to_string()
                }
            }
        };

        let help = Paragraph::new(help_text)
            .style(Style::default().bg(Color::DarkGray).fg(Color::White));
        
        f.render_widget(help, area);
    }

    fn next(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.filtered.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
        self.preview_scroll = 0;
    }

    fn previous(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.filtered.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
        self.preview_scroll = 0;
    }

    fn first(&mut self) {
        if !self.filtered.is_empty() {
            self.list_state.select(Some(0));
            self.preview_scroll = 0;
        }
    }

    fn last(&mut self) {
        if !self.filtered.is_empty() {
            self.list_state.select(Some(self.filtered.len() - 1));
            self.preview_scroll = 0;
        }
    }

    fn scroll_preview_down(&mut self) {
        self.preview_scroll = self.preview_scroll.saturating_add(5);
    }

    fn scroll_preview_up(&mut self) {
        self.preview_scroll = self.preview_scroll.saturating_sub(5);
    }

    fn update_filter(&mut self) {
        if self.search_query.is_empty() {
            self.filtered = (0..self.prompts.len()).collect();
        } else {
            self.filtered = self
                .prompts
                .iter()
                .enumerate()
                .filter_map(|(i, prompt)| {
                    let search_text = format!("{} {} {}", 
                        prompt.name, 
                        prompt.description, 
                        prompt.content
                    );
                    
                    if self.fuzzy_matcher.fuzzy_match(&search_text, &self.search_query).is_some() {
                        Some(i)
                    } else {
                        None
                    }
                })
                .collect();
        }

        // Reset selection if needed
        if self.filtered.is_empty() {
            self.list_state.select(None);
        } else if self.list_state.selected().map_or(true, |s| s >= self.filtered.len()) {
            self.list_state.select(Some(0));
        }
    }

    fn use_prompt(&mut self) -> Result<()> {
        if let Some(selected) = self.list_state.selected() {
            if selected < self.filtered.len() {
                let prompt = &self.prompts[self.filtered[selected]];
                let mut clipboard = Clipboard::new();
                clipboard.copy_to_clipboard(&prompt.content)?;
                
                self.status_message = Some((
                    format!("✓ Copied '{}' to clipboard", prompt.name),
                    Instant::now(),
                ));
            }
        }
        Ok(())
    }

    fn show_prompt(&mut self) -> Result<()> {
        if let Some(selected) = self.list_state.selected() {
            if selected < self.filtered.len() {
                let prompt = &self.prompts[self.filtered[selected]];
                self.status_message = Some((
                    format!("Showing '{}' (press any key to continue)", prompt.name),
                    Instant::now(),
                ));
            }
        }
        Ok(())
    }

    fn edit_prompt(&mut self, storage: &Storage) -> Result<()> {
        if let Some(selected) = self.list_state.selected() {
            if selected < self.filtered.len() {
                let prompt_name = self.prompts[self.filtered[selected]].name.clone();
                
                // Launch editor
                let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
                let prompt_path = storage.prompt_path(&prompt_name);
                
                // Temporarily leave TUI mode
                disable_raw_mode()?;
                execute!(
                    io::stdout(),
                    LeaveAlternateScreen,
                    DisableMouseCapture
                )?;
                
                // Run editor
                std::process::Command::new(&editor)
                    .arg(&prompt_path)
                    .status()?;
                
                // Restore TUI mode
                enable_raw_mode()?;
                execute!(
                    io::stdout(),
                    EnterAlternateScreen,
                    EnableMouseCapture
                )?;
                
                // Reload prompts
                self.prompts = load_prompts(storage)?;
                self.update_filter();
                
                self.status_message = Some((
                    format!("✓ Edited '{}'", prompt_name),
                    Instant::now(),
                ));
            }
        }
        Ok(())
    }

    fn delete_prompt(&mut self, _storage: &Storage) -> Result<()> {
        if let Some(selected) = self.list_state.selected() {
            if selected < self.filtered.len() {
                let prompt_name = self.prompts[self.filtered[selected]].name.clone();
                
                // Simple confirmation in status
                self.status_message = Some((
                    format!("Press 'd' again to delete '{}' or any other key to cancel", prompt_name),
                    Instant::now(),
                ));
                
                // Set delete pending state
                self.pending_delete = Some(prompt_name);
            }
        }
        Ok(())
    }
    
    fn confirm_delete(&mut self, storage: &Storage) -> Result<()> {
        if let Some(prompt_name) = self.pending_delete.take() {
            // Delete the prompt
            storage.delete_prompt(&prompt_name)?;
            
            // Reload prompts
            self.prompts = load_prompts(storage)?;
            self.update_filter();
            
            self.status_message = Some((
                format!("✓ Deleted '{}'", prompt_name),
                Instant::now(),
            ));
        }
        Ok(())
    }

    fn new_prompt(&mut self, _storage: &Storage) -> Result<()> {
        self.status_message = Some((
            "New prompt creation not yet implemented in TUI".to_string(),
            Instant::now(),
        ));
        Ok(())
    }

    fn show_help(&mut self) {
        self.status_message = Some((
            "PromptHive TUI v1.0 - The fastest way to manage prompts".to_string(),
            Instant::now(),
        ));
    }
}

fn load_prompts(storage: &Storage) -> Result<Vec<Prompt>> {
    let prompt_names = storage.list_prompts()?;
    let mut prompts = Vec::new();

    for name in prompt_names {
        if let Ok((metadata, content)) = storage.read_prompt(&name) {
            prompts.push(Prompt {
                name: name.clone(),
                description: metadata.description,
                content,
                created_at: metadata.created_at,
                updated_at: metadata.updated_at,
                tags: metadata.tags.unwrap_or_default(),
            });
        }
    }

    Ok(prompts)
}