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
    text::{Line, Span, Text},
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
}

const NAVIGATION: &str = "Type to find  Up/Down select  Enter use  Esc cancel";
const COMMANDS: &str = "CLI: u use  f find  n new  e edit  c compose";
const SMALL_TERMINAL: &str =
    "Terminal too small for the full selection. Resize, or Esc and use an exact name.";

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
        && display_supported(
            std::env::var("TERM").ok().as_deref(),
            std::env::var_os("NO_COLOR").is_some(),
        )
}

fn display_supported(term: Option<&str>, no_color: bool) -> bool {
    !no_color && term.is_some_and(|term| !term.is_empty() && term != "dumb")
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
    pick_with(storage, query, intent, |name, _, _| {
        Ok(Some(name.to_owned()))
    })
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
        |name, terminal, interrupted| {
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
    selected: impl FnOnce(&str, &mut Screen, &Arc<AtomicBool>) -> Result<Option<T>>,
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
        terminal.draw(|frame| state.draw(frame))?;
        match poll_event(&interrupted)? {
            Some(Event::Key(key)) => match state.key(key) {
                Decision::Use(name) => return selected(&name, &mut terminal, &interrupted),
                Decision::Cancel => return Ok(None),
                Decision::Continue => {}
            },
            Some(Event::Paste(text)) => {
                let total = state.query.len().checked_add(text.len()).ok_or_else(|| {
                    crate::limits::exceeded("search query", crate::limits::TEXT_BYTES)
                })?;
                crate::limits::check_len(total, crate::limits::TEXT_BYTES, "search query")?;
                state.query.extend(text.chars().filter(|c| !c.is_control()));
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
                    version: metadata.version,
                    created_at: metadata.created_at,
                    updated_at: metadata.updated_at,
                    git_hash: metadata.git_hash,
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
                    "Read permission is denied.".to_owned()
                } else if let Some(limit) = limit {
                    format!("Preview unavailable: {limit}. The stored file is unchanged.")
                } else {
                    "The prompt could not be read or its frontmatter is malformed.".to_owned()
                };
                let editable = storage
                    .read_raw(&name)
                    .ok()
                    .is_some_and(|bytes| std::str::from_utf8(&bytes).is_ok());
                let location = storage
                    .prompt_path(&name)
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|_| storage.base_dir().display().to_string());
                Entry {
                        prompt: Prompt {
                            name: name.clone(),
                            short_code: String::new(),
                            description: "Cannot read this prompt".into(),
                            version: None,
                            created_at: None,
                            updated_at: None,
                            git_hash: None,
                        },
                        body: String::new(),
                        editable,
                    error: Some(format!(
                            "{problem}\nPath: {location}\nCheck its frontmatter and file permissions.\nEdit with: ph e {}",
                            shell_selector(&name)
                        )),
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

#[derive(Debug, PartialEq, Eq)]
enum Decision {
    Continue,
    Use(String),
    Cancel,
}

struct PickerState {
    intent: PickerIntent,
    entries: Vec<Entry>,
    filtered: Vec<usize>,
    query: String,
    selection: ListState,
    preview_scroll: usize,
    max_scroll: usize,
    page_height: usize,
    preview_layout: Option<(usize, u16, usize)>,
    identity_visible: bool,
    message: Option<String>,
}

impl PickerState {
    fn new(entries: Vec<Entry>, query: &str) -> Self {
        let mut state = Self {
            intent: PickerIntent::Use,
            entries,
            filtered: Vec::new(),
            query: query.chars().filter(|c| !c.is_control()).collect(),
            selection: ListState::default(),
            preview_scroll: 0,
            max_scroll: 0,
            page_height: 1,
            preview_layout: None,
            identity_visible: false,
            message: None,
        };
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
        match key.code {
            KeyCode::Enter if !self.identity_visible => {
                self.message = Some(SMALL_TERMINAL.into());
            }
            KeyCode::Enter => {
                if let Some(entry) = self.selected() {
                    if entry.error.is_none()
                        || (self.intent == PickerIntent::Edit && entry.editable)
                    {
                        return Decision::Use(entry.prompt.name.clone());
                    }
                    self.message = Some(if self.intent == PickerIntent::Edit {
                        "Cannot edit this prompt safely. Check its bytes and permissions.".into()
                    } else {
                        "Cannot use this prompt. Check the file shown in the preview.".into()
                    });
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
            }
            KeyCode::PageUp => {
                self.preview_scroll = self.preview_scroll.saturating_sub(self.page_height);
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.refilter();
            }
            KeyCode::Char(character)
                if !key.modifiers.contains(KeyModifiers::CONTROL) && !character.is_control() =>
            {
                if self.query.len().saturating_add(character.len_utf8())
                    <= crate::limits::TEXT_BYTES
                {
                    self.query.push(character);
                    self.refilter();
                } else {
                    self.message = Some("Search query is too large.".into());
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
                Paragraph::new(wrap_text(SMALL_TERMINAL, outer.width)),
                outer,
            );
            return;
        }
        let area = Rect {
            x: outer.x + 1,
            y: outer.y + 1,
            width: outer.width.saturating_sub(2),
            height: outer.height.saturating_sub(2),
        };
        frame.render_widget(
            Paragraph::new("Prompthive").style(Style::default().add_modifier(Modifier::BOLD)),
            Rect { height: 1, ..area },
        );
        let prefix = if self.intent == PickerIntent::Edit {
            "Find to edit: "
        } else {
            "Find a prompt: "
        };
        let query = tail_to_width(
            &safe_text(&self.query),
            area.width.saturating_sub(prefix.len() as u16 + 1),
        );
        let query_line = format!("{prefix}{query}");
        frame.render_widget(
            Paragraph::new(query_line.clone()),
            Rect {
                y: area.y + 1,
                height: 1,
                ..area
            },
        );
        frame.set_cursor_position((
            area.x
                + Line::from(query_line)
                    .width()
                    .min(area.width.saturating_sub(1) as usize) as u16,
            area.y + 1,
        ));

        let content = Rect {
            y: area.y + 3,
            height: area.height.saturating_sub(7),
            ..area
        };
        let footer = Rect {
            y: area.y + area.height.saturating_sub(3),
            height: 3,
            ..area
        };
        frame.render_widget(
            Paragraph::new(Text::from(vec![
                Line::from(if self.intent == PickerIntent::Edit && area.width < 36 {
                    "Up/Down Enter edit Esc cancel"
                } else if self.intent == PickerIntent::Edit {
                    "Up/Down select  Enter edit  Esc cancel"
                } else if area.width >= 52 {
                    NAVIGATION
                } else if area.width < 36 {
                    "Up/Down  Enter use  Esc cancel"
                } else {
                    "Up/Down select  Enter use  Esc cancel"
                }),
                Line::from(if self.intent == PickerIntent::Edit {
                    "Enter opens EDITOR; nothing copied"
                } else if area.width >= 52 {
                    "PgUp/PgDn preview  Default: copy; show if unavailable"
                } else {
                    "Default: copy/show; PgUp/PgDn preview"
                }),
                Line::from(if area.width >= 44 {
                    COMMANDS
                } else {
                    "CLI: ph --help"
                })
                .style(Style::default().fg(Color::DarkGray)),
            ])),
            footer,
        );

        if self.entries.is_empty() {
            frame.render_widget(
                Paragraph::new(wrap_text(
                    "No saved prompts yet.\nSave one from the shell:\nph n review \"Review this: {input}\"\nEsc returns to the shell.",
                    content.width,
                )),
                content,
            );
            self.identity_visible = true;
            return;
        }
        if self.filtered.is_empty() {
            frame.render_widget(
                Paragraph::new(wrap_text(
                    "No prompts match. Keep typing, Backspace, or Esc to cancel.",
                    content.width,
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
        let command = if self.intent == PickerIntent::Edit {
            format!("Edit with: ph e {}", shell_selector(&selected.prompt.name))
        } else {
            format!("Use again: ph u {}", shell_selector(&selected.prompt.name))
        };
        let identity = wrap_text(&safe_text(&command), content.width);
        let identity_height = identity.len();
        if identity_height.saturating_add(4) > content.height as usize {
            frame.render_widget(
                Paragraph::new(wrap_text(SMALL_TERMINAL, content.width)),
                content,
            );
            return;
        }
        self.identity_visible = true;
        let list_height = (content.height as usize)
            .saturating_sub(identity_height + 3)
            .clamp(1, 6) as u16;
        let list_area = Rect {
            height: list_height,
            ..content
        };
        let items = self.filtered.iter().map(|&index| {
            let entry = &self.entries[index];
            ListItem::new(format!(
                "{}  {}",
                safe_text(&entry.prompt.name),
                safe_text(&entry.prompt.description.replace('\n', " "))
            ))
        });
        frame.render_stateful_widget(
            List::new(items).highlight_symbol("> ").highlight_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            list_area,
            &mut self.selection,
        );
        let identity_area = Rect {
            y: list_area.y + list_height + 1,
            height: identity_height as u16,
            ..content
        };
        frame.render_widget(Paragraph::new(identity), identity_area);
        let label_area = Rect {
            y: identity_area.y + identity_area.height,
            height: 1,
            ..content
        };
        frame.render_widget(
            Paragraph::new(if selected.error.is_some() {
                "Cannot read prompt"
            } else {
                "Preview"
            })
            .style(Style::default().add_modifier(Modifier::BOLD)),
            label_area,
        );
        let body_area = Rect {
            y: label_area.y + 1,
            height: content
                .y
                .saturating_add(content.height)
                .saturating_sub(label_area.y + 1),
            ..content
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
        let scroll = self.preview_scroll.min(self.max_scroll);
        if scroll != self.preview_scroll {
            lines = None;
        }
        self.preview_scroll = scroll;
        let lines = lines.unwrap_or_else(|| {
            guided::preview_lines(preview, body_area.width, scroll, self.page_height)
        });
        frame.render_widget(Paragraph::new(lines), body_area);
        if let Some(message) = &self.message {
            frame.render_widget(
                Paragraph::new(tail_to_width(message, footer.width))
                    .style(Style::default().fg(Color::Red)),
                Rect {
                    y: footer.y + 1,
                    height: 1,
                    ..footer
                },
            );
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
        assert!(after.contains("Use again: ph u second/review"));
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
        assert!(screen.contains("Find a prompt: zzzz"));
        assert!(screen.contains("No prompts match"));
        assert_eq!(
            state.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Decision::Continue
        );
        assert_eq!(state.query, "zzzz");
    }

    #[test]
    fn empty_library_shows_one_clear_creation_path() {
        let mut state = PickerState::new(Vec::new(), "");
        let screen = painted(&mut state, 80, 24);
        assert!(screen.contains("No saved prompts yet."));
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
        assert!(screen.contains("Cannot read prompt"));
        assert!(screen.contains("/synthetic/notes/broken.md"));
        assert_eq!(
            state.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Decision::Continue
        );
        let blocked = painted(&mut state, 80, 24);
        assert!(blocked.contains("Cannot use this prompt."));
        assert!(blocked.contains("Esc cancel"));
        assert!(blocked.contains(COMMANDS));
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
            assert!(screen.contains("Esc cancel"), "{width}x{height}");
        }
    }

    #[test]
    fn tiny_terminal_cannot_accept_hidden_identity() {
        let mut state = PickerState::new(vec![entry("notes/review", "Body")], "");
        let screen = painted(&mut state, 20, 6);
        assert!(screen.contains("Terminal too small"));
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
    fn plain_terminal_and_no_color_have_an_explicit_non_ansi_path() {
        assert!(!display_supported(Some("dumb"), false));
        assert!(!display_supported(None, false));
        assert!(!display_supported(Some("xterm-256color"), true));
        assert!(display_supported(Some("xterm-256color"), false));
    }

    #[test]
    fn edit_intent_is_visible_and_can_select_a_malformed_note_for_repair() {
        let mut broken = entry("notes/broken", "");
        broken.error = Some("Malformed frontmatter; use an editor to repair it.".into());
        let mut state = PickerState::new(vec![broken], "");
        state.intent = PickerIntent::Edit;
        for (width, height) in [(32, 18), (40, 18), (80, 24), (120, 40)] {
            let screen = painted(&mut state, width, height);
            assert!(screen.contains("Enter edit"));
            assert!(screen.contains("Esc cancel"));
            assert!(screen
                .lines()
                .map(str::trim)
                .collect::<String>()
                .contains("Edit with: ph e notes/broken"));
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
