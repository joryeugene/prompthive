//! Focused, local prompt discovery. Selection never renders or copies a prompt.

use anyhow::{bail, Context, Result};
use crossterm::{
    cursor::{Hide, Show},
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use std::{
    collections::HashMap,
    fs::OpenOptions,
    io::{self, IsTerminal, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    matching::{ranked_prompts, Prompt},
    template::Variables,
    Storage,
};

mod guided;
type Screen = Terminal<CrosstermBackend<Box<dyn Write>>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerIntent {
    Use,
    Edit,
    Browse,
}

const SMALL_TERMINAL: &str =
    "terminal too small for the full selection. resize, or esc and use an exact name.";

/// A keyboard is separate from piped prompt context.
pub fn keyboard_available() -> bool {
    if io::stdin().is_terminal() {
        return true;
    }
    #[cfg(unix)]
    {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")
            .is_ok_and(|terminal| terminal.is_terminal())
    }
    #[cfg(not(unix))]
    false
}

/// A plain terminal receives a clear CLI alternative instead of escape sequences.
pub fn interactive_supported() -> bool {
    io::stdout().is_terminal()
        && keyboard_available()
        && display_supported(std::env::var("TERM").ok().as_deref())
}

fn display_supported(term: Option<&str>) -> bool {
    term.is_some_and(|term| !term.is_empty() && term != "dumb")
}

/// Return a full canonical ID only after Enter. Escape, Ctrl-C and Unix
/// termination signals cancel; opening and cancelling have no storage effects.
pub fn pick_prompt(storage: &Storage, query: &str) -> Result<Option<String>> {
    pick_prompt_for(storage, query, PickerIntent::Use)
}

pub fn pick_prompt_for(
    storage: &Storage,
    query: &str,
    intent: PickerIntent,
) -> Result<Option<String>> {
    pick_with(
        storage,
        query,
        intent,
        |_| None,
        |name, _, _, _| Ok(Some(name.to_owned())),
    )
}

/// Selection, missing values, and final preview share one terminal session.
/// The returned buffer is the exact reviewed output; callers must not render it again.
pub fn pick_rendered<M, R, D>(
    storage: &Storage,
    query: &str,
    supplied: &Variables,
    destination: D,
    missing: M,
    render: R,
) -> Result<Option<(String, String)>>
where
    M: Fn(&str, &Variables) -> Result<Vec<String>>,
    R: Fn(&str, &Variables) -> Result<String>,
    D: Fn(&str) -> String,
{
    pick_with(
        storage,
        query,
        PickerIntent::Use,
        |name| Some(destination(name)),
        |name, _, terminal, interrupted| {
            let required = missing(name, supplied)?;
            if required.is_empty() {
                return Ok(Some((name.to_owned(), render(name, supplied)?)));
            }
            guided::complete(
                terminal,
                interrupted,
                name,
                &destination(name),
                required,
                supplied,
                |values| render(name, values),
            )
            .map(|output| output.map(|text| (name.to_owned(), text)))
        },
    )
}

fn pick_with<T>(
    storage: &Storage,
    query: &str,
    intent: PickerIntent,
    destination: impl Fn(&str) -> Option<String>,
    selected: impl FnOnce(&str, Action, &mut Screen, &Arc<AtomicBool>) -> Result<Option<T>>,
) -> Result<Option<T>> {
    if !interactive_supported() {
        bail!("Interactive preview is unavailable here. Use ph f QUERY, then ph u FULL_NAME.");
    }
    crate::limits::check_text(query, "search query")?;
    let entries = load_entries(storage)?;
    let mut state = PickerState::new(entries, query);
    state.intent = intent;
    let (writer, restore) = terminal_writers()?;
    let (_session, interrupted) = TerminalSession::enter(restore)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(writer))
        .context("Could not initialize the prompt picker terminal")?;
    loop {
        if interrupted.load(Ordering::Relaxed) {
            return Ok(None);
        }
        state.destination = state
            .selected()
            .and_then(|entry| destination(&entry.prompt.name));
        terminal.draw(|frame| state.draw(frame))?;
        match poll_event(&interrupted)? {
            Some(Event::Key(key)) => match state.key(key) {
                Decision::Use(name) => {
                    return selected(&name, state.action, &mut terminal, &interrupted)
                }
                Decision::Cancel => return Ok(None),
                Decision::Continue => {}
            },
            Some(Event::Paste(text)) => {
                let total = state.query.len().checked_add(text.len()).ok_or_else(|| {
                    crate::limits::exceeded("search query", crate::limits::TEXT_BYTES)
                })?;
                crate::limits::check_len(total, crate::limits::TEXT_BYTES, "search query")?;
                let text: String = text.chars().filter(|c| !c.is_control()).collect();
                state.query.insert_str(state.cursor, &text);
                state.cursor += text.len();
                state.align_cursor();
                state.action_focus = false;
                state.action = Action::Use;
                state.refilter();
            }
            _ => {}
        }
    }
}

fn poll_event(interrupted: &Arc<AtomicBool>) -> Result<Option<Event>> {
    let ready = match event::poll(Duration::from_millis(100)) {
        Ok(ready) => ready,
        Err(_) if interrupted.load(Ordering::Relaxed) => return Ok(None),
        Err(error) => return Err(error).context("Could not read picker input"),
    };
    if !ready {
        return Ok(None);
    }
    match event::read() {
        Ok(input) => Ok(Some(input)),
        Err(_) if interrupted.load(Ordering::Relaxed) => Ok(None),
        Err(error) => Err(error).context("Could not read picker input"),
    }
}

fn terminal_writers() -> Result<(Box<dyn Write>, Box<dyn Write>)> {
    #[cfg(unix)]
    if let Ok(terminal) = OpenOptions::new().read(true).write(true).open("/dev/tty") {
        let restore = terminal.try_clone()?;
        return Ok((Box::new(terminal), Box::new(restore)));
    }
    #[cfg(windows)]
    if let Ok(terminal) = OpenOptions::new().read(true).write(true).open("CONOUT$") {
        let restore = terminal.try_clone()?;
        return Ok((Box::new(terminal), Box::new(restore)));
    }
    if io::stderr().is_terminal() {
        return Ok((Box::new(io::stderr()), Box::new(io::stderr())));
    }
    bail!("No screen terminal is available. Use ph f QUERY, then ph u FULL_NAME.")
}

struct TerminalSession {
    restore: Box<dyn Write>,
    raw: bool,
    alternate: bool,
    #[cfg(unix)]
    signals: Vec<signal_hook::SigId>,
}

impl TerminalSession {
    fn enter(restore: Box<dyn Write>) -> Result<(Self, Arc<AtomicBool>)> {
        let interrupted = Arc::new(AtomicBool::new(false));
        let mut session = Self {
            restore,
            raw: false,
            alternate: false,
            #[cfg(unix)]
            signals: Vec::new(),
        };
        #[cfg(unix)]
        for signal in [
            signal_hook::consts::SIGINT,
            signal_hook::consts::SIGTERM,
            signal_hook::consts::SIGHUP,
            signal_hook::consts::SIGQUIT,
        ] {
            session.signals.push(signal_hook::flag::register(
                signal,
                Arc::clone(&interrupted),
            )?);
        }
        // Set teardown intent before an operation that can partially succeed.
        session.raw = true;
        enable_raw_mode().context("Could not enable picker keyboard input")?;
        session.alternate = true;
        execute!(
            session.restore,
            EnterAlternateScreen,
            EnableBracketedPaste,
            Hide
        )
        .context("Could not open the picker screen")?;
        Ok((session, interrupted))
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        if self.raw {
            let _ = disable_raw_mode();
        }
        if self.alternate {
            let _ = execute!(
                self.restore,
                DisableBracketedPaste,
                Show,
                LeaveAlternateScreen
            );
            let _ = self.restore.flush();
        }
        #[cfg(unix)]
        for signal in self.signals.drain(..) {
            signal_hook::low_level::unregister(signal);
        }
    }
}

#[derive(Debug, Clone)]
struct Entry {
    prompt: Prompt,
    body: String,
    error: Option<String>,
    editable: bool,
}

fn load_entries(storage: &Storage) -> Result<Vec<Entry>> {
    load_entries_with_budget(storage, crate::limits::IMPORT_BYTES)
}

fn load_entries_with_budget(storage: &Storage, limit: usize) -> Result<Vec<Entry>> {
    let mut budget = crate::limits::Budget::new(limit, "picker library");
    let mut entries = Vec::new();
    for name in storage.list_prompts()? {
        let entry = match storage.read_prompt(&name) {
            Ok((metadata, body)) => Entry {
                prompt: Prompt {
                    name,
                    short_code: String::new(),
                    description: metadata.description,
                    // These fields are never displayed or matched. Keep the
                    // original metadata on disk, outside the discovery index.
                    version: None,
                    created_at: None,
                    updated_at: None,
                    git_hash: None,
                },
                body,
                error: None,
                editable: true,
            },
            Err(error) => {
                let permissions = error.chain().any(|cause| {
                    cause
                        .downcast_ref::<io::Error>()
                        .is_some_and(|error| error.kind() == io::ErrorKind::PermissionDenied)
                });
                let limit = error
                    .chain()
                    .find_map(|cause| cause.downcast_ref::<crate::limits::LimitError>());
                let problem = if permissions {
                    "read permission is denied.".to_owned()
                } else if let Some(limit) = limit {
                    format!("preview unavailable: {limit}. the stored file is unchanged.")
                } else {
                    "the prompt could not be read or its frontmatter is malformed.".to_owned()
                };
                let editable = storage
                    .read_raw(&name)
                    .ok()
                    .is_some_and(|bytes| std::str::from_utf8(&bytes).is_ok());
                let location = storage
                    .prompt_path(&name)
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|_| storage.base_dir().display().to_string());
                let recovery = if editable {
                    format!(
                        "edit with: ph e {}\ncheck its frontmatter and file permissions.",
                        shell_selector(&name)
                    )
                } else {
                    "check permissions or repair the file outside ph.".to_owned()
                };
                Entry {
                    prompt: Prompt {
                        name: name.clone(),
                        short_code: String::new(),
                        description: "cannot read this prompt".into(),
                        version: None,
                        created_at: None,
                        updated_at: None,
                        git_hash: None,
                    },
                    body: String::new(),
                    editable,
                    error: Some(format!("{problem}\n{recovery}\npath: {location}")),
                }
            }
        };
        budget.add(std::mem::size_of::<Entry>())?;
        budget.add(entry.prompt.name.len())?;
        budget.add(entry.prompt.description.len())?;
        budget.add(entry.body.len())?;
        budget.add(entry.error.as_ref().map_or(0, String::len))?;
        entries.push(entry);
    }
    Ok(entries)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Use,
    Show,
    Edit,
}

pub enum BareSelection {
    Use(String, String),
    Show(String),
    Edit(String),
}

pub fn pick_bare(storage: &Storage, input: &str) -> Result<Option<BareSelection>> {
    let composer = crate::Composer::new(storage.clone());
    let supplied = Variables::default();
    let destination = |name: &str| crate::IoOptions::default().confirmation(name);
    pick_with(
        storage,
        "",
        PickerIntent::Browse,
        |name| Some(destination(name)),
        |name, action, terminal, interrupted| match action {
            Action::Show => Ok(Some(BareSelection::Show(name.to_owned()))),
            Action::Edit => Ok(Some(BareSelection::Edit(name.to_owned()))),
            Action::Use => {
                let names = vec![name.to_owned()];
                let required = composer
                    .required_variables(&names, &supplied)
                    .map_err(|error| crate::commands::common::fail(2, format!("{error:#}")))?;
                let render = |values: &Variables| {
                    composer
                        .render(&names, input, values)
                        .map_err(|error| crate::commands::common::fail(2, format!("{error:#}")))
                };
                if required.is_empty() {
                    return Ok(Some(BareSelection::Use(
                        name.to_owned(),
                        render(&supplied)?,
                    )));
                }
                guided::complete(
                    terminal,
                    interrupted,
                    name,
                    &destination(name),
                    required,
                    &supplied,
                    render,
                )
                .map(|result| result.map(|text| BareSelection::Use(name.to_owned(), text)))
            }
        },
    )
}

#[derive(Debug, PartialEq, Eq)]
enum Decision {
    Continue,
    Use(String),
    Cancel,
}

struct PickerState {
    intent: PickerIntent,
    destination: Option<String>,
    entries: Vec<Entry>,
    filtered: Vec<usize>,
    query: String,
    cursor: usize,
    action_focus: bool,
    action: Action,
    selection: ListState,
    preview_scroll: usize,
    max_scroll: usize,
    page_height: usize,
    preview_layout: Option<(usize, u16, usize)>,
    identity_visible: bool,
    message: Option<String>,
}

impl PickerState {
    fn align_cursor(&mut self) {
        self.cursor = self
            .query
            .grapheme_indices(true)
            .map(|(index, _)| index)
            .chain(std::iter::once(self.query.len()))
            .find(|&index| index >= self.cursor)
            .unwrap_or(self.query.len());
    }
    fn new(entries: Vec<Entry>, query: &str) -> Self {
        let mut state = Self {
            intent: PickerIntent::Use,
            destination: None,
            entries,
            filtered: Vec::new(),
            query: query.chars().filter(|c| !c.is_control()).collect(),
            cursor: 0,
            action_focus: false,
            action: Action::Use,
            selection: ListState::default(),
            preview_scroll: 0,
            max_scroll: 0,
            page_height: 1,
            preview_layout: None,
            identity_visible: false,
            message: None,
        };
        state.cursor = state.query.len();
        state.refilter();
        state
    }

    fn refilter(&mut self) {
        let prompts: Vec<_> = self
            .entries
            .iter()
            .map(|entry| entry.prompt.clone())
            .collect();
        let indices: HashMap<_, _> = self
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.prompt.name.as_str(), index))
            .collect();
        self.filtered = ranked_prompts(&self.query, &prompts)
            .into_iter()
            .filter_map(|prompt| indices.get(prompt.name.as_str()).copied())
            .collect();
        self.selection.select(if self.filtered.is_empty() {
            None
        } else {
            Some(0)
        });
        self.preview_scroll = 0;
        self.message = None;
    }

    fn selected(&self) -> Option<&Entry> {
        self.selection.selected().and_then(|index| {
            self.filtered
                .get(index)
                .and_then(|&index| self.entries.get(index))
        })
    }

    fn key(&mut self, key: KeyEvent) -> Decision {
        if key.kind == KeyEventKind::Release {
            return Decision::Continue;
        }
        if key.code == KeyCode::Esc
            || (key.modifiers.contains(KeyModifiers::CONTROL)
                && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('d')))
        {
            return Decision::Cancel;
        }
        if self.intent == PickerIntent::Browse && key.code == KeyCode::Tab {
            self.action_focus = !self.action_focus;
            if !self.action_focus {
                self.action = Action::Use;
            }
            return Decision::Continue;
        }
        if self.intent == PickerIntent::Browse
            && self.action_focus
            && matches!(key.code, KeyCode::Left | KeyCode::Right)
        {
            self.action = match (self.action, key.code) {
                (Action::Use, KeyCode::Right) | (Action::Edit, KeyCode::Left) => Action::Show,
                (Action::Show, KeyCode::Right) => Action::Edit,
                (Action::Show, KeyCode::Left) => Action::Use,
                (action, _) => action,
            };
            return Decision::Continue;
        }
        match key.code {
            KeyCode::Enter if !self.identity_visible => {
                self.message = Some(SMALL_TERMINAL.into());
            }
            KeyCode::Enter => {
                if let Some(entry) = self.selected() {
                    if entry.error.is_none()
                        || ((self.intent == PickerIntent::Edit
                            || (self.intent == PickerIntent::Browse
                                && self.action == Action::Edit))
                            && entry.editable)
                    {
                        return Decision::Use(entry.prompt.name.clone());
                    }
                    self.message = Some(
                        if self.intent == PickerIntent::Edit
                            || (self.intent == PickerIntent::Browse && self.action == Action::Edit)
                        {
                            "cannot edit this prompt safely. check its bytes and permissions."
                                .into()
                        } else {
                            "cannot use this prompt. check the file shown in the preview.".into()
                        },
                    );
                }
            }
            KeyCode::Up | KeyCode::Down => {
                if !self.filtered.is_empty() {
                    let current = self.selection.selected().unwrap_or(0);
                    let next = if key.code == KeyCode::Up {
                        current.saturating_sub(1)
                    } else {
                        current.saturating_add(1).min(self.filtered.len() - 1)
                    };
                    self.selection.select(Some(next));
                    self.preview_scroll = 0;
                    self.message = None;
                }
            }
            KeyCode::PageDown => {
                self.preview_scroll = self
                    .preview_scroll
                    .saturating_add(self.page_height)
                    .min(self.max_scroll);
                self.message = None;
            }
            KeyCode::PageUp => {
                self.preview_scroll = self.preview_scroll.saturating_sub(self.page_height);
                self.message = None;
            }
            KeyCode::Left => {
                self.cursor = self.query[..self.cursor]
                    .grapheme_indices(true)
                    .next_back()
                    .map_or(0, |(i, _)| i);
            }
            KeyCode::Right => {
                self.cursor += self.query[self.cursor..]
                    .graphemes(true)
                    .next()
                    .map_or(0, str::len);
            }
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.query.len(),
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    let start = self.query[..self.cursor]
                        .grapheme_indices(true)
                        .next_back()
                        .map_or(0, |(i, _)| i);
                    self.query.drain(start..self.cursor);
                    self.cursor = start;
                    self.align_cursor();
                    self.action = Action::Use;
                    self.action_focus = false;
                    self.refilter();
                }
            }
            KeyCode::Delete => {
                if let Some(grapheme) = self.query[self.cursor..].graphemes(true).next() {
                    self.query.drain(self.cursor..self.cursor + grapheme.len());
                    self.align_cursor();
                    self.action = Action::Use;
                    self.action_focus = false;
                    self.refilter();
                }
            }
            KeyCode::Char(character)
                if !key.modifiers.contains(KeyModifiers::CONTROL) && !character.is_control() =>
            {
                if self.query.len().saturating_add(character.len_utf8())
                    <= crate::limits::TEXT_BYTES
                {
                    self.query.insert(self.cursor, character);
                    self.cursor += character.len_utf8();
                    self.align_cursor();
                    self.action = Action::Use;
                    self.action_focus = false;
                    self.refilter();
                } else {
                    self.message = Some("search query is too large.".into());
                }
            }
            _ => {}
        }
        Decision::Continue
    }

    fn draw(&mut self, frame: &mut Frame<'_>) {
        let outer = frame.area();
        self.identity_visible = false;
        if outer.width < 32 || outer.height < 14 {
            frame.render_widget(
                Paragraph::new(guided::preview_lines(
                    SMALL_TERMINAL,
                    outer.width,
                    0,
                    outer.height as usize,
                )),
                outer,
            );
            return;
        }
        let area = Rect {
            height: outer.height.min(12),
            ..outer
        };
        let prefix_text = if self.intent == PickerIntent::Edit {
            "find to edit"
        } else {
            "find"
        };
        let prefix = format!("{prefix_text} · ");
        let no_color = std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty());
        let accent = Style::default().add_modifier(Modifier::BOLD);
        let marker = if no_color {
            Style::default()
        } else {
            Style::default().fg(Color::Cyan)
        };
        let count = if self.filtered.len() > 3 {
            format!(
                "{}{}/{}",
                "  ",
                self.selection.selected().unwrap_or(0) + 1,
                self.filtered.len()
            )
        } else {
            String::new()
        };
        let available = area.width.saturating_sub(
            (Line::from(prefix.clone()).width() + Line::from(count.clone()).width() + 1) as u16,
        );
        let before = safe_text(&self.query[..self.cursor]);
        let after = safe_text(&self.query[self.cursor..]);
        let mut shown_before = tail_to_width(&before, available.saturating_sub(1));
        if shown_before != before && available > 1 {
            shown_before = format!("…{}", tail_to_width(&before, available.saturating_sub(2)));
        }
        let remaining = available.saturating_sub(Line::from(shown_before.clone()).width() as u16);
        let mut shown_after = String::new();
        for ch in after.chars() {
            let mut next = shown_after.clone();
            next.push(ch);
            if Line::from(next.clone()).width() > remaining as usize {
                break;
            }
            shown_after = next;
        }
        let input_line = format!("{prefix}{shown_before}");
        let query = format!("{shown_before}{shown_after}");
        let query_line = Line::from(vec![
            Span::styled(prefix_text, accent),
            Span::styled(" · ", marker),
            Span::raw(query),
            Span::raw(if self.query.is_empty() {
                "type to search"
            } else {
                ""
            }),
            Span::raw(count),
        ]);
        frame.render_widget(Paragraph::new(query_line), Rect { height: 1, ..area });
        if !self.action_focus {
            frame.set_cursor_position((
                area.x
                    + Line::from(input_line)
                        .width()
                        .min(area.width.saturating_sub(1) as usize) as u16,
                area.y,
            ));
        }

        // Display the whole destination before enabling Enter; preserve path case.
        let destination = if self.intent == PickerIntent::Browse && self.action != Action::Use {
            self.selected().map(|entry| match self.action {
                Action::Show => format!("show · raw {}", entry.prompt.name),
                Action::Edit => format!("edit · saved {}", entry.prompt.name),
                Action::Use => unreachable!(),
            })
        } else {
            self.destination.as_deref().map(display_destination)
        };
        let mut destination_lines = destination
            .as_deref()
            .map(|s| wrap_text(&safe_text(s), area.width))
            .unwrap_or_default();
        if let Some(first) = destination_lines.first_mut() {
            let text = first.to_string();
            if let Some((label, rest)) = text.split_once(" · ") {
                *first = Line::from(vec![
                    Span::styled(label.to_owned(), accent),
                    Span::styled(" · ", marker),
                    Span::raw(rest.to_owned()),
                ]);
            }
        }
        let footer_height = destination_lines.len().saturating_add(2);
        let identity_height = self
            .selected()
            .map(|entry| {
                let name = safe_text(&entry.prompt.name);
                if !name.contains('\n')
                    && Line::from(name.clone()).width() <= area.width.saturating_sub(2) as usize
                {
                    0
                } else {
                    wrap_text(&format!("id · {name}"), area.width).len()
                }
            })
            .unwrap_or(1);
        let required_height = footer_height
            .saturating_add(identity_height)
            .saturating_add(6);
        let area = Rect {
            height: outer
                .height
                .min((required_height.min(u16::MAX as usize) as u16).max(12)),
            ..outer
        };
        if footer_height.saturating_add(7) > area.height as usize {
            frame.render_widget(
                Paragraph::new(guided::preview_lines(
                    SMALL_TERMINAL,
                    area.width,
                    0,
                    area.height.saturating_sub(1) as usize,
                )),
                Rect {
                    y: area.y + 1,
                    height: area.height - 1,
                    ..area
                },
            );
            return;
        }
        let footer_y = area.y + area.height - footer_height as u16;
        if !destination_lines.is_empty() {
            frame.render_widget(
                Paragraph::new(destination_lines),
                Rect {
                    y: footer_y,
                    height: (footer_height - 2) as u16,
                    ..area
                },
            );
        }
        let blocked = self.selected().is_some_and(|entry| {
            entry.error.is_some()
                && !((self.intent == PickerIntent::Edit
                    || (self.intent == PickerIntent::Browse && self.action == Action::Edit))
                    && entry.editable)
        });
        let identity_hidden = self.selected().is_some()
            && identity_height.saturating_add(5) > footer_y.saturating_sub(area.y + 1) as usize;
        let action_nav = if self.action_focus {
            "tab search · ←/→ · enter · esc"
        } else {
            "tab actions · enter use · esc"
        };
        let nav = if identity_hidden {
            "resize · esc cancel"
        } else if self.entries.is_empty() {
            "esc cancel"
        } else if self.filtered.is_empty() {
            "backspace · esc cancel"
        } else if blocked
            && (self.intent == PickerIntent::Edit
                || (self.intent == PickerIntent::Browse && self.action == Action::Edit))
        {
            "arrows · esc cancel · no edit"
        } else if blocked
            && self.intent == PickerIntent::Browse
            && self.selected().is_some_and(|entry| entry.editable)
        {
            if self.action_focus {
                "←/→ edit · esc cancel"
            } else {
                "tab actions · esc cancel"
            }
        } else if blocked {
            "arrows · esc cancel · cannot use"
        } else if self.intent == PickerIntent::Edit {
            "arrows · enter edit · esc cancel"
        } else if self.intent == PickerIntent::Browse {
            action_nav
        } else {
            "arrows · enter use · esc cancel"
        };
        frame.render_widget(
            Paragraph::new(nav),
            Rect {
                y: area.y + area.height - 2,
                height: 1,
                ..area
            },
        );
        let actions = if self.intent == PickerIntent::Browse && self.action_focus {
            let selected = Style::default().add_modifier(Modifier::BOLD | Modifier::REVERSED);
            let mut spans = Vec::new();
            for (index, (action, label)) in [
                (Action::Use, "use"),
                (Action::Show, "show"),
                (Action::Edit, "edit"),
            ]
            .into_iter()
            .enumerate()
            {
                if index > 0 {
                    spans.push(Span::raw("  "));
                }
                if self.action == action {
                    spans.push(Span::styled(format!("[{label}]"), selected));
                } else {
                    spans.push(Span::raw(format!(" {label} ")));
                }
            }
            Line::from(spans)
        } else if self.intent == PickerIntent::Browse {
            Line::from("use · show · edit")
        } else {
            Line::from("ph --help")
        };
        frame.render_widget(
            Paragraph::new(actions),
            Rect {
                y: area.y + area.height - 1,
                height: 1,
                ..area
            },
        );
        let content = Rect {
            y: area.y + 1,
            height: footer_y.saturating_sub(area.y + 1),
            ..area
        };
        if self.entries.is_empty() {
            frame.render_widget(Paragraph::new(wrap_text("no saved prompts yet.\ncreate one: ph n review \"Review this: {input}\"\nesc returns to the shell.", content.width)), content);
            self.identity_visible = true;
            return;
        }
        if self.filtered.is_empty() {
            frame.render_widget(
                Paragraph::new(guided::preview_lines(
                    "no prompts match. keep typing, backspace, or esc to cancel.",
                    content.width,
                    0,
                    content.height as usize,
                )),
                content,
            );
            self.identity_visible = true;
            return;
        }
        let Some(index) = self
            .selection
            .selected()
            .and_then(|row| self.filtered.get(row))
            .copied()
        else {
            return;
        };
        let selected = &self.entries[index];
        let name = safe_text(&selected.prompt.name);
        let identity = if !name.contains('\n')
            && Line::from(name.clone()).width() <= content.width.saturating_sub(2) as usize
        {
            Vec::new()
        } else {
            wrap_text(&format!("id · {name}"), content.width)
        };
        let identity_height = identity.len();
        if identity_height.saturating_add(5) > content.height as usize {
            frame.render_widget(
                Paragraph::new(guided::preview_lines(
                    SMALL_TERMINAL,
                    content.width,
                    0,
                    content.height as usize,
                )),
                content,
            );
            return;
        }
        self.identity_visible = true;
        let list_height = (content.height as usize)
            .saturating_sub(identity_height + 4)
            .clamp(1, 3)
            .min(self.filtered.len()) as u16;
        let list_area = Rect {
            height: list_height,
            ..content
        };
        let items = self
            .filtered
            .iter()
            .map(|&index| ListItem::new(safe_text(&self.entries[index].prompt.name)));
        frame.render_stateful_widget(
            List::new(items)
                .highlight_symbol("> ")
                .highlight_style(if self.action_focus {
                    Style::default().add_modifier(Modifier::BOLD)
                } else {
                    Style::default().add_modifier(Modifier::BOLD | Modifier::REVERSED)
                }),
            list_area,
            &mut self.selection,
        );
        let identity_area = Rect {
            y: list_area.y + list_height,
            height: identity_height as u16,
            ..content
        };
        frame.render_widget(Paragraph::new(identity), identity_area);
        let label_area = Rect {
            y: identity_area.y + identity_area.height + 1,
            height: 1,
            x: content.x + 2,
            width: content.width.saturating_sub(2),
        };
        let body_area = Rect {
            y: label_area.y + 1,
            x: content.x + 2,
            width: content.width.saturating_sub(2),
            height: content
                .y
                .saturating_add(content.height)
                .saturating_sub(label_area.y + 1),
        };
        let preview = selected.error.as_deref().unwrap_or(&selected.body);
        self.page_height = (body_area.height as usize).max(1);
        let mut lines = None;
        let total = match self.preview_layout {
            Some((cached_index, width, total))
                if cached_index == index && width == body_area.width =>
            {
                total
            }
            _ => {
                let (page, total) = guided::counted_preview(
                    preview,
                    body_area.width,
                    self.preview_scroll,
                    self.page_height,
                );
                lines = Some(page);
                self.preview_layout = Some((index, body_area.width, total));
                total
            }
        };
        self.max_scroll = total.saturating_sub(body_area.height as usize);
        let blocked = self.message.as_deref().is_some_and(|message| {
            message.starts_with("cannot use this prompt.")
                || message.starts_with("cannot edit this prompt")
        });
        let label = match (
            blocked,
            self.intent,
            selected.error.is_some(),
            self.max_scroll > 0,
        ) {
            (true, PickerIntent::Edit, _, true) => "cannot edit · pgup/pgdn",
            (true, PickerIntent::Edit, _, false) => "cannot edit · see details",
            (true, _, _, true) => "cannot use · pgup/pgdn",
            (true, _, _, false) => "cannot use · see details",
            (false, _, true, true) => "cannot read prompt · pgup/pgdn",
            (false, _, true, false) => "cannot read prompt",
            (false, _, false, true) => "preview · pgup/pgdn",
            (false, _, false, false) => "preview",
        };
        frame.render_widget(Paragraph::new(label).style(accent), label_area);
        let scroll = self.preview_scroll.min(self.max_scroll);
        if scroll != self.preview_scroll {
            lines = None;
        }
        self.preview_scroll = scroll;
        let lines = lines.unwrap_or_else(|| {
            guided::preview_lines(preview, body_area.width, scroll, self.page_height)
        });
        if let Some(message) = self.message.as_ref().filter(|_| !blocked) {
            frame.render_widget(
                Paragraph::new(guided::preview_lines(
                    message,
                    body_area.width,
                    0,
                    body_area.height as usize,
                ))
                .style(Style::default().add_modifier(Modifier::BOLD)),
                body_area,
            );
        } else {
            frame.render_widget(Paragraph::new(lines), body_area);
        }
    }
}

fn shell_selector(name: &str) -> String {
    let quoted = if !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-./@".contains(c))
    {
        name.to_owned()
    } else {
        format!("'{}'", name.replace('\'', "'\\''"))
    };
    if name.starts_with('-') {
        format!("-- {quoted}")
    } else {
        quoted
    }
}

fn display_destination(destination: &str) -> String {
    if destination == "Copy to clipboard; show text if unavailable" {
        "use · copy to clipboard; else show".into()
    } else if let Some(path) = destination.strip_prefix("Write ") {
        format!("write · {path}")
    } else if let Some(rest) = destination.strip_prefix("Copy to clipboard") {
        format!("copy · clipboard{rest}")
    } else {
        destination.to_owned()
    }
}

fn safe_text(text: &str) -> String {
    let mut safe = String::new();
    for character in text.chars() {
        match character {
            '\n' => safe.push('\n'),
            '\t' => safe.push_str("    "),
            character if character.is_control() => {
                safe.push_str(&format!("\\u{{{:04x}}}", character as u32));
            }
            character => safe.push(character),
        }
    }
    safe
}

fn wrap_text(text: &str, width: u16) -> Vec<Line<'static>> {
    if width == 0 {
        return Vec::new();
    }
    let mut output = Vec::new();
    for logical in text.split('\n') {
        let span = Span::raw(logical);
        let mut line = String::new();
        let mut used = 0;
        for grapheme in span.styled_graphemes(Style::default()) {
            let columns = Span::raw(grapheme.symbol).width();
            if used + columns > width as usize && !line.is_empty() {
                output.push(Line::from(std::mem::take(&mut line)));
                used = 0;
            }
            line.push_str(grapheme.symbol);
            used += columns;
        }
        output.push(Line::from(line));
    }
    output
}

fn tail_to_width(text: &str, width: u16) -> String {
    let span = Span::raw(text);
    let graphemes: Vec<_> = span.styled_graphemes(Style::default()).collect();
    let mut used = 0;
    let mut start = graphemes.len();
    for (index, grapheme) in graphemes.iter().enumerate().rev() {
        let columns = Span::raw(grapheme.symbol).width();
        if used + columns > width as usize {
            break;
        }
        used += columns;
        start = index;
    }
    graphemes[start..]
        .iter()
        .map(|grapheme| grapheme.symbol)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    fn entry(name: &str, body: &str) -> Entry {
        Entry {
            prompt: Prompt {
                name: name.into(),
                short_code: String::new(),
                description: format!("Description for {name}"),
                version: None,
                created_at: None,
                updated_at: None,
                git_hash: None,
            },
            body: body.into(),
            error: None,
            editable: true,
        }
    }

    fn painted(state: &mut PickerState, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| state.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn loading_a_plain_note_preserves_its_bytes_and_unknown_metadata() {
        let directory = tempfile::tempdir().unwrap();
        let prompts = directory.path().join("prompts");
        std::fs::create_dir(&prompts).unwrap();
        let path = prompts.join("note.md");
        let original = "---\ndescription: A saved note\nmy_extension: retained\n---\n\n  Notes stay literal: {input}\nRésumé\n\n";
        std::fs::write(&path, original).unwrap();
        let storage = Storage::new_with_base(directory.path().to_path_buf()).unwrap();

        let entries = load_entries(&storage).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].prompt.name, "note");
        assert_eq!(entries[0].prompt.description, "A saved note");
        assert_eq!(entries[0].body, "  Notes stay literal: {input}\nRésumé\n\n");
        assert!(entries[0].error.is_none());
        assert_eq!(std::fs::read(&path).unwrap(), original.as_bytes());
    }

    #[test]
    fn malformed_prompt_is_discoverable_without_exposing_parser_source() {
        let directory = tempfile::tempdir().unwrap();
        let prompts = directory.path().join("prompts");
        std::fs::create_dir(&prompts).unwrap();
        let path = prompts.join("broken.md");
        let original = "---\ndescription: [private-malformed-source\n---\nprivate-body\n";
        std::fs::write(&path, original).unwrap();
        let storage = Storage::new_with_base(directory.path().to_path_buf()).unwrap();

        let entries = load_entries(&storage).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].prompt.name, "broken");
        let error = entries[0].error.as_deref().unwrap();
        assert!(error.contains("malformed"));
        assert!(error.contains("ph e broken"));
        assert!(error.find("ph e broken").unwrap() < error.find("path:").unwrap());
        assert!(!error.contains("private-malformed-source"));
        assert!(!error.contains("private-body"));
        assert!(entries[0].body.is_empty());
        assert_eq!(std::fs::read(&path).unwrap(), original.as_bytes());
    }

    #[test]
    fn loading_an_existing_empty_store_creates_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let storage = Storage::new_with_base(directory.path().to_path_buf()).unwrap();
        assert!(load_entries(&storage).unwrap().is_empty());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    }

    #[test]
    fn search_is_focused_and_letter_hints_do_not_execute_commands() {
        let mut state = PickerState::new(vec![entry("notes/example", "Original note")], "");
        painted(&mut state, 80, 24);
        assert_eq!(
            state.key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE)),
            Decision::Continue
        );
        assert_eq!(state.query, "e");
        assert_eq!(
            state.key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE)),
            Decision::Continue
        );
        assert_eq!(state.query, "en");
    }

    #[test]
    fn default_copy_effect_fits_at_forty_columns() {
        let mut state = PickerState::new(vec![entry("review", "Review this change.")], "");
        state.intent = PickerIntent::Browse;
        state.destination = Some("Copy to clipboard; show text if unavailable".into());
        let screen = painted(&mut state, 40, 18);
        assert!(screen
            .lines()
            .any(|line| line.contains("use · copy to clipboard; else show")));
    }

    #[test]
    fn action_focus_highlights_the_action_and_keeps_the_selected_result_marker() {
        let mut state = PickerState::new(vec![entry("review", "Review")], "");
        state.intent = PickerIntent::Browse;
        state.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        state.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        let mut terminal = Terminal::new(TestBackend::new(40, 18)).unwrap();
        terminal.draw(|frame| state.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let mut marker = None;
        let mut bracket = None;
        for y in 0..18 {
            for x in 0..40 {
                let cell = &buffer[(x, y)];
                if cell.symbol() == ">" {
                    marker = Some(cell.modifier);
                }
                if cell.symbol() == "[" {
                    bracket = Some(cell.modifier);
                }
            }
        }
        assert!(marker.is_some_and(|style| !style.contains(Modifier::REVERSED)));
        assert!(bracket.is_some_and(|style| style.contains(Modifier::REVERSED)));
    }

    #[test]
    fn browse_actions_require_focus_and_editing_resets_the_pending_action() {
        let mut state = PickerState::new(vec![entry("review", "Hello")], "rev");
        state.intent = PickerIntent::Browse;
        painted(&mut state, 40, 18);
        state.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        state.key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert_eq!(state.query, "rexv");
        assert_eq!(state.action, Action::Use);
        state.key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        state.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        state.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert_eq!(state.action, Action::Show);
        let screen = painted(&mut state, 40, 18);
        assert!(screen.contains("[show]"));
        assert!(screen.contains("show · raw review"));
        state.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert_eq!(state.action, Action::Edit);
        assert_eq!(
            state.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            Decision::Cancel
        );
        state.key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
        assert_eq!(state.action, Action::Use);
        assert!(!state.action_focus);
    }

    #[test]
    fn search_cursor_moves_and_deletes_whole_graphemes() {
        let mut state = PickerState::new(vec![entry("review", "Hello")], "e\u{301}x");
        state.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        state.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        assert_eq!(state.cursor, 0);
        state.key(KeyEvent::new(KeyCode::Char('A'), KeyModifiers::NONE));
        assert_eq!(state.query, "Ae\u{301}x");
        state.key(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE));
        assert_eq!(state.query, "Ax");
    }

    #[test]
    fn malformed_editable_prompt_keeps_its_action_hint() {
        let mut broken = entry("broken", "");
        broken.error = Some("malformed".into());
        let mut state = PickerState::new(vec![broken], "broken");
        state.intent = PickerIntent::Browse;
        assert!(painted(&mut state, 40, 18).contains("tab actions"));
        state.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        state.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        state.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert_eq!(state.action, Action::Edit);
        painted(&mut state, 40, 18);
        assert_eq!(
            state.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Decision::Use("broken".into())
        );
    }

    #[test]
    fn enter_returns_full_selected_id_only_after_confirmation() {
        let mut state = PickerState::new(
            vec![
                entry("first/review", "FIRST BODY"),
                entry("second/review", "SECOND BODY"),
            ],
            "review",
        );
        let before = painted(&mut state, 80, 24);
        assert!(before.contains("FIRST BODY"));
        assert_eq!(
            state.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
            Decision::Continue
        );
        let after = painted(&mut state, 80, 24);
        assert!(after.contains("> second/review"));
        assert!(after.contains("SECOND BODY"));
        assert!(!after.contains("FIRST BODY"));
        assert_eq!(
            state.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Decision::Use("second/review".into())
        );
    }

    #[test]
    fn escape_and_control_c_cancel_without_selection() {
        let mut state = PickerState::new(vec![entry("review", "Untouched")], "");
        assert_eq!(
            state.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            Decision::Cancel
        );
        assert_eq!(
            state.key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Decision::Cancel
        );
    }

    #[test]
    fn no_matches_keep_query_and_enter_has_no_effect() {
        let mut state = PickerState::new(vec![entry("review", "Untouched")], "zzzz");
        let screen = painted(&mut state, 80, 24);
        assert!(screen.contains("find · zzzz"));
        assert!(screen.contains("no prompts match"));
        assert!(!screen.contains("enter use"));
        assert!(screen.contains("backspace · esc cancel"));
        assert_eq!(
            state.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Decision::Continue
        );
        assert_eq!(state.query, "zzzz");
    }

    #[test]
    fn short_list_signals_more_results_and_updates_position() {
        let entries = ["alpha", "bravo", "charlie", "delta"]
            .into_iter()
            .map(|name| entry(name, "Body"))
            .collect();
        let mut state = PickerState::new(entries, "");
        let screen = painted(&mut state, 40, 18);
        assert!(screen.contains("find · type to search  1/4"));
        state.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        let screen = painted(&mut state, 40, 18);
        assert!(screen.contains("find · type to search  2/4"));
    }

    #[test]
    fn clipped_query_is_signalled_without_changing_its_value() {
        let query = "the-real-query-is-much-longer-than-this-terminal-can-display";
        let mut state = PickerState::new(vec![entry("review", "Body")], query);
        let screen = painted(&mut state, 32, 18);
        assert!(screen.contains("find · …"));
        assert_eq!(state.query, query);
    }

    #[test]
    fn empty_library_shows_one_clear_creation_path() {
        let mut state = PickerState::new(Vec::new(), "");
        let screen = painted(&mut state, 80, 24);
        assert!(screen.contains("no saved prompts yet."));
        assert!(screen.contains("ph n review"));
        assert!(!screen.contains("registry"));
        assert_eq!(
            state.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Decision::Continue
        );
    }

    #[test]
    fn unreadable_item_is_visible_and_cannot_be_accepted() {
        let mut broken = entry("notes/broken", "");
        broken.error = Some("Frontmatter is malformed.\nPath: /synthetic/notes/broken.md".into());
        let mut state = PickerState::new(
            vec![broken, entry("notes/working", "Useful note")],
            "broken",
        );
        let screen = painted(&mut state, 80, 24);
        assert!(screen.contains("cannot read prompt"));
        assert!(screen.contains("/synthetic/notes/broken.md"));
        assert!(!screen.contains("enter use"));
        assert!(screen.contains("cannot use"));
        assert_eq!(
            state.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Decision::Continue
        );
        let blocked = painted(&mut state, 80, 24);
        assert!(blocked.contains("cannot use ·"));
        assert!(blocked.contains("esc"));
        assert!(blocked.contains("ph --help"));
    }

    #[test]
    fn blocked_error_does_not_blend_with_preview_and_paging_recovers_details() {
        let mut broken = entry("broken", "");
        broken.error = Some("The prompt could not be read or its frontmatter is malformed.\nPath: /some/long/example/broken.md\nCheck permissions and repair the source file.".into());
        let mut state = PickerState::new(vec![broken], "broken");
        painted(&mut state, 40, 18);
        assert_eq!(
            state.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Decision::Continue
        );
        let blocked = painted(&mut state, 40, 18);
        assert!(blocked.contains("cannot use ·"));
        assert!(blocked.contains("frontmatter is malformed."));
        state.key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
        let paged = painted(&mut state, 40, 18);
        assert!(!paged.contains("cannot use · pgup/pgdn"));
        assert!(paged.contains("permissions"));
    }

    #[test]
    fn full_identity_and_preview_survive_narrow_and_wide_layouts() {
        let name = format!("notes/{}/review", "long-name-".repeat(12));
        for (width, height) in [(60, 20), (80, 24), (120, 40)] {
            let mut state = PickerState::new(vec![entry(&name, "VISIBLE PREVIEW")], "");
            let screen = painted(&mut state, width, height);
            assert!(state.identity_visible, "{width}x{height}");
            assert!(screen.contains("VISIBLE PREVIEW"), "{width}x{height}");
            let without_padding: String = screen.lines().map(str::trim).collect();
            assert!(without_padding.contains(&name), "{width}x{height}");
            assert!(screen.contains("esc"), "{width}x{height}");
        }
    }

    #[test]
    fn near_identical_long_unicode_names_cannot_be_silently_confused() {
        let first = format!("notes/{}/α", "résumé-".repeat(5));
        let second = format!("notes/{}/β", "résumé-".repeat(5));
        let mut state = PickerState::new(
            vec![entry(&first, "FIRST"), entry(&second, "SECOND")],
            "résumé",
        );
        painted(&mut state, 32, 18);
        state.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        let screen = painted(&mut state, 32, 18);
        if state.identity_visible {
            let compact: String = screen.lines().map(str::trim).collect();
            assert!(compact.contains(&second));
            assert_eq!(
                state.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                Decision::Use(second)
            );
        } else {
            assert!(screen.contains("terminal too small"));
            assert_eq!(
                state.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                Decision::Continue
            );
        }
    }

    #[test]
    fn oversized_identity_recovers_after_terminal_is_enlarged() {
        let name = format!(
            "{}/second",
            ["commonxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"; 8].join("/")
        );
        let mut state = PickerState::new(vec![entry(&name, "SECOND")], "common");
        let narrow = painted(&mut state, 32, 18);
        assert!(!state.identity_visible);
        assert!(narrow.contains("terminal too small"));
        assert!(narrow.contains("use an exact name."));
        assert!(narrow.contains("resize · esc cancel"));
        assert!(!narrow.contains("enter use"));
        assert_eq!(
            state.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Decision::Continue
        );
        let wide = painted(&mut state, 120, 40);
        assert!(state.identity_visible);
        assert!(wide.contains("enter use"));
        assert!(wide
            .lines()
            .map(str::trim)
            .collect::<String>()
            .contains(&name));
        assert_eq!(
            state.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Decision::Use(name)
        );
    }

    #[test]
    fn tiny_terminal_cannot_accept_hidden_identity() {
        let mut state = PickerState::new(vec![entry("notes/review", "Body")], "");
        let screen = painted(&mut state, 20, 6);
        assert!(screen.contains("terminal too small"));
        assert_eq!(
            state.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Decision::Continue
        );
    }

    #[test]
    fn preview_controls_are_printed_as_data_not_terminal_commands() {
        let original = "safe\u{1b}[2J\nvalue\t42\rnext";
        let mut state = PickerState::new(vec![entry("notes/safe", original)], "");
        let screen = painted(&mut state, 80, 24);
        assert!(!screen.contains('\u{1b}'));
        assert!(screen.contains("\\u{001b}[2J"));
        assert!(screen.contains("\\u{000d}next"));
        assert_eq!(state.entries[0].body, original);
    }

    #[test]
    fn unicode_query_and_preview_wrap_without_splitting_characters() {
        let mut state = PickerState::new(vec![entry("notes/café", "Résumé 中文 👩‍💻")], "café");
        let screen = painted(&mut state, 60, 20);
        assert!(screen.contains("café"));
        assert!(screen.contains("Résumé"));
        assert!(screen.contains("中"));
        assert!(screen.contains("文"));
        assert!(screen.contains("👩‍💻"));
        state.key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(state.query, "caf");
    }

    #[test]
    fn preview_paging_is_bounded() {
        let body = (0..80)
            .map(|index| format!("Line {index}\n"))
            .collect::<String>();
        let mut state = PickerState::new(vec![entry("notes/long", &body)], "");
        painted(&mut state, 80, 24);
        for _ in 0..100 {
            state.key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
        }
        assert_eq!(state.preview_scroll, state.max_scroll);
        let screen = painted(&mut state, 80, 24);
        assert!(screen.contains("Line 79"));
    }

    #[test]
    fn terminal_capability_is_independent_of_color_preference() {
        assert!(!display_supported(Some("dumb")));
        assert!(!display_supported(None));
        assert!(display_supported(Some("xterm-256color")));
    }

    #[test]
    fn picker_shows_requested_destination_instead_of_a_copy_default() {
        for (width, height) in [(40, 18), (80, 24), (120, 40)] {
            let mut state = PickerState::new(vec![entry("review", "Body")], "");
            state.destination = Some("Write result.txt".into());
            let screen = painted(&mut state, width, height);
            assert!(screen.contains("write · result.txt"));
            assert!(!screen.contains("Default: copy"));
        }
    }

    #[test]
    fn mixed_case_destination_is_complete_or_selection_is_blocked() {
        let path = format!("Reports/{}/Final.TXT", "LongPart".repeat(10));
        let mut state = PickerState::new(vec![entry("review", "Body")], "");
        state.destination = Some(format!("Write {path}"));
        let screen = painted(&mut state, 40, 18);
        let compact: String = screen.lines().map(str::trim).collect();
        assert!(state.identity_visible && compact.contains(&path));
        state.destination = Some(format!("Write {}", "LongPart".repeat(300)));
        painted(&mut state, 40, 18);
        assert!(!state.identity_visible);
        assert_eq!(
            state.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Decision::Continue
        );
    }

    #[test]
    fn edit_intent_is_visible_and_can_select_a_malformed_note_for_repair() {
        let mut broken = entry("notes/broken", "");
        broken.error = Some("Malformed frontmatter; use an editor to repair it.".into());
        let mut state = PickerState::new(vec![broken], "");
        state.intent = PickerIntent::Edit;
        for (width, height) in [(32, 18), (40, 18), (80, 24), (120, 40)] {
            let screen = painted(&mut state, width, height);
            assert!(screen.contains("enter edit"));
            assert!(screen.contains("esc"));
            assert!(screen
                .lines()
                .map(str::trim)
                .collect::<String>()
                .contains("> notes/broken"));
        }
        assert_eq!(
            state.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Decision::Use("notes/broken".into())
        );
        state.key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert_eq!(state.query, "n");
    }

    #[test]
    fn edit_repairability_uses_checked_raw_utf8_and_excludes_unreadable_entries() {
        let directory = tempfile::tempdir().unwrap();
        let prompts = directory.path().join("prompts");
        std::fs::create_dir(&prompts).unwrap();
        std::fs::write(
            prompts.join("broken.md"),
            "---\ndescription: [broken\n---\nPrivate raw note",
        )
        .unwrap();
        std::fs::write(prompts.join("binary.md"), [0xff, 0xfe]).unwrap();
        let storage = Storage::new_with_base(directory.path().to_path_buf()).unwrap();
        let entries = load_entries(&storage).unwrap();
        assert!(
            entries
                .iter()
                .find(|e| e.prompt.name == "broken")
                .unwrap()
                .editable
        );
        assert!(
            !entries
                .iter()
                .find(|e| e.prompt.name == "binary")
                .unwrap()
                .editable
        );
        let binary_error = entries
            .iter()
            .find(|e| e.prompt.name == "binary")
            .unwrap()
            .error
            .as_deref()
            .unwrap();
        assert!(binary_error.contains("repair the file outside ph"));
        assert!(!binary_error.contains("ph e binary"));
        let mut state = PickerState::new(entries, "binary");
        state.intent = PickerIntent::Edit;
        painted(&mut state, 80, 24);
        assert_eq!(
            state.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Decision::Continue
        );
        let mut unreadable = entry("unreadable", "");
        unreadable.error = Some("Read permission denied".into());
        unreadable.editable = false;
        let mut state = PickerState::new(vec![unreadable], "");
        state.intent = PickerIntent::Edit;
        painted(&mut state, 80, 24);
        assert_eq!(
            state.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Decision::Continue
        );
    }

    #[test]
    fn aggregate_library_limit_rejects_without_changing_notes() {
        let directory = tempfile::tempdir().unwrap();
        let prompts = directory.path().join("prompts");
        std::fs::create_dir(&prompts).unwrap();
        std::fs::write(prompts.join("a.md"), "First note").unwrap();
        let storage = Storage::new_with_base(directory.path().to_path_buf()).unwrap();
        let entries = load_entries(&storage).unwrap();
        let entry = &entries[0];
        let one = std::mem::size_of::<Entry>()
            + entry.prompt.name.len()
            + entry.prompt.description.len()
            + entry.body.len();
        assert_eq!(load_entries_with_budget(&storage, one).unwrap().len(), 1);
        assert!(load_entries_with_budget(&storage, one - 1).is_err());
        std::fs::write(prompts.join("b.md"), "Second note").unwrap();
        assert!(load_entries_with_budget(&storage, one).is_err());
        assert_eq!(
            std::fs::read_to_string(prompts.join("a.md")).unwrap(),
            "First note"
        );
        assert_eq!(
            std::fs::read_to_string(prompts.join("b.md")).unwrap(),
            "Second note"
        );
    }

    #[test]
    fn displayed_reuse_commands_quote_shell_metacharacters() {
        assert_eq!(shell_selector("notes/review"), "notes/review");
        assert_eq!(shell_selector("notes/a'b"), "'notes/a'\\''b'");
        assert_eq!(shell_selector("$(command)"), "'$(command)'");
        assert_eq!(shell_selector("-review"), "-- -review");
    }
}
