//! Ephemeral value entry followed by a frozen, safely displayed output buffer.
use super::{
    display_destination, poll_event, safe_text, tail_to_width, wrap_text, Screen, SMALL_TERMINAL,
};
use crate::{limits, template::Variables};
use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

struct Field {
    name: String,
    text: String,
    cursor: usize,
    answered: bool,
}

struct Form {
    fields: Vec<Field>,
    active: usize,
    visible: bool,
    message: Option<String>,
    preview: Option<String>,
    scroll: usize,
    page: usize,
    more: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum Action {
    Continue,
    Cancel,
    Render,
    Accept,
}

impl Form {
    fn new(names: Vec<String>) -> Self {
        Self {
            fields: names
                .into_iter()
                .map(|name| Field {
                    name,
                    text: String::new(),
                    cursor: 0,
                    answered: false,
                })
                .collect(),
            active: 0,
            visible: false,
            message: None,
            preview: None,
            scroll: 0,
            page: 1,
            more: false,
        }
    }

    fn insert(&mut self, text: &str) {
        let total = self
            .fields
            .iter()
            .map(|f| f.text.len())
            .try_fold(text.len(), usize::checked_add);
        if total.is_none_or(|size| size > limits::TEXT_BYTES) {
            self.message = Some("values are too large; nothing was truncated.".into());
            return;
        }
        let field = &mut self.fields[self.active];
        field.text.insert_str(field.cursor, text);
        field.cursor += text.len();
        if !text.is_empty() {
            field.answered = true;
        }
        self.message = None;
    }

    fn key(&mut self, key: KeyEvent) -> Action {
        if key.kind == KeyEventKind::Release {
            return Action::Continue;
        }
        if key.code == KeyCode::Esc
            || (key.modifiers.contains(KeyModifiers::CONTROL)
                && matches!(key.code, KeyCode::Char('c' | 'd')))
        {
            return Action::Cancel;
        }
        if self.preview.is_some() {
            match key.code {
                KeyCode::Enter if self.visible => return Action::Accept,
                KeyCode::PageDown if self.more => {
                    self.scroll = self.scroll.saturating_add(self.page)
                }
                KeyCode::Down if self.more => self.scroll = self.scroll.saturating_add(1),
                KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(self.page),
                KeyCode::Up => self.scroll = self.scroll.saturating_sub(1),
                KeyCode::Home => self.scroll = 0,
                _ => {}
            }
            return Action::Continue;
        }
        if self.fields.is_empty() {
            return Action::Continue;
        }
        match key.code {
            KeyCode::Enter if !self.visible => self.message = Some(SMALL_TERMINAL.into()),
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::ALT) => self.insert("\n"),
            KeyCode::Enter => {
                self.fields[self.active].answered = true; // Enter explicitly permits an empty value.
                if self.fields.iter().all(|f| f.answered) {
                    return Action::Render;
                }
                self.active = (1..=self.fields.len())
                    .map(|step| (self.active + step) % self.fields.len())
                    .find(|&i| !self.fields[i].answered)
                    .unwrap();
            }
            KeyCode::Tab => self.active = (self.active + 1) % self.fields.len(),
            KeyCode::BackTab => {
                self.active = (self.active + self.fields.len() - 1) % self.fields.len()
            }
            KeyCode::Left => {
                let f = &mut self.fields[self.active];
                if let Some((index, _)) = f.text[..f.cursor].char_indices().next_back() {
                    f.cursor = index;
                }
            }
            KeyCode::Right => {
                let f = &mut self.fields[self.active];
                if let Some(c) = f.text[f.cursor..].chars().next() {
                    f.cursor += c.len_utf8();
                }
            }
            KeyCode::Home => self.fields[self.active].cursor = 0,
            KeyCode::End => {
                let f = &mut self.fields[self.active];
                f.cursor = f.text.len();
            }
            KeyCode::Backspace => {
                let f = &mut self.fields[self.active];
                if let Some((index, _)) = f.text[..f.cursor].char_indices().next_back() {
                    f.text.drain(index..f.cursor);
                    f.cursor = index;
                    f.answered = true;
                }
            }
            KeyCode::Delete => {
                let f = &mut self.fields[self.active];
                if let Some(c) = f.text[f.cursor..].chars().next() {
                    f.text.drain(f.cursor..f.cursor + c.len_utf8());
                    f.answered = true;
                }
            }
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                    && !c.is_control() =>
            {
                self.insert(&c.to_string())
            }
            _ => {}
        }
        Action::Continue
    }

    fn values(&self, supplied: &Variables) -> Variables {
        let mut result = supplied.clone();
        for field in &self.fields {
            if field.answered {
                result.insert(field.name.clone(), field.text.clone());
            }
        }
        result
    }

    fn draw(&mut self, frame: &mut Frame<'_>, name: &str, destination: &str) {
        self.visible = false;
        let outer = frame.area();
        if outer.width < 32 || outer.height < 14 {
            frame.render_widget(
                Paragraph::new(preview_lines(
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
            x: outer.x + 1,
            y: outer.y + 1,
            width: outer.width - 2,
            height: outer.height - 2,
        };
        let preview = self.preview.is_some();
        let mut heading = vec![Line::from(if preview {
            "preview rendered prompt"
        } else {
            "fill required values"
        })
        .style(Style::default().add_modifier(Modifier::BOLD))];
        heading.extend(wrap_text(
            &format!("prompt · {}", safe_text(name)),
            area.width,
        ));
        heading.extend(wrap_text(
            &format!(
                "destination · {}",
                safe_text(&display_destination(destination))
            ),
            area.width,
        ));
        if heading.len() + 7 > area.height as usize {
            frame.render_widget(
                Paragraph::new(preview_lines(
                    SMALL_TERMINAL,
                    area.width,
                    0,
                    area.height as usize,
                )),
                area,
            );
            return;
        }
        let head_height = heading.len() as u16;
        frame.render_widget(
            Paragraph::new(heading),
            Rect {
                height: head_height,
                ..area
            },
        );
        let content = Rect {
            y: area.y + head_height + 1,
            height: area.height - head_height - 4,
            ..area
        };
        let footer = Rect {
            y: area.y + area.height - 2,
            height: 2,
            ..area
        };
        self.visible = true;
        if let Some(text) = &self.preview {
            let (mut lines, mut more) =
                preview_window(text, content.width, self.scroll, content.height as usize);
            if lines.is_empty() && self.scroll > 0 {
                self.scroll = 0;
                (lines, more) = preview_window(text, content.width, 0, content.height as usize);
            }
            self.more = more;
            self.page = content.height as usize;
            let footer_y = if more {
                footer.y
            } else {
                (content.y + lines.len().min(content.height as usize) as u16 + 1).min(footer.y)
            };
            frame.render_widget(Paragraph::new(lines), content);
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(if area.width < 34 {
                        "enter use  esc cancel"
                    } else {
                        "enter use  esc cancel  pgup/pgdn"
                    }),
                    Line::from(if area.width < 34 {
                        "pgup/pgdn; controls escaped"
                    } else {
                        "controls escaped; output unchanged."
                    }),
                ]),
                Rect {
                    y: footer_y,
                    ..footer
                },
            );
            return;
        }
        let field = &self.fields[self.active];
        let label = format!(
            "> {} ({}/{})",
            field.name,
            self.active + 1,
            self.fields.len()
        );
        let label = wrap_text(&safe_text(&label), content.width);
        if label.len() + 2 > content.height as usize {
            self.visible = false;
            frame.render_widget(
                Paragraph::new(preview_lines(
                    SMALL_TERMINAL,
                    content.width,
                    0,
                    content.height as usize,
                )),
                content,
            );
            return;
        }
        let label_height = label.len() as u16;
        frame.render_widget(
            Paragraph::new(label).style(Style::default().add_modifier(Modifier::BOLD)),
            Rect {
                height: label_height,
                ..content
            },
        );
        // Only format the nearby caret window, not an entire pasted value.
        let before: String = field.text[..field.cursor]
            .chars()
            .rev()
            .take(usize::from(content.width) * 4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let before = tail_to_width(&inline_text(&before), content.width.saturating_sub(1));
        let after: String = field.text[field.cursor..]
            .chars()
            .take(usize::from(content.width) * 2)
            .collect();
        let after = inline_text(&after);
        let value = before.clone() + &after;
        frame.render_widget(
            Paragraph::new(value),
            Rect {
                y: content.y + label_height,
                height: 1,
                ..content
            },
        );
        frame.set_cursor_position((
            content.x + Line::from(before).width() as u16,
            content.y + label_height,
        ));
        let message = self.message.as_deref().unwrap_or(if field.answered {
            "value set. paste keeps newlines."
        } else {
            "unset. enter accepts even an empty value."
        });
        let message_y = content.y + label_height + 2;
        let message_lines = preview_lines(
            message,
            content.width,
            0,
            content.height.saturating_sub(label_height + 2) as usize,
        );
        let footer_y =
            (message_y + message_lines.len().min(content.height as usize) as u16 + 1).min(footer.y);
        frame.render_widget(
            Paragraph::new(message_lines),
            Rect {
                y: message_y,
                height: footer_y.saturating_sub(message_y),
                ..content
            },
        );
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(if area.width < 34 {
                    "tab field  esc cancel"
                } else {
                    "tab/shift-tab field  esc cancel"
                }),
                Line::from(if area.width < 34 {
                    "shift-tab back  enter preview"
                } else {
                    "enter preview/next  alt-enter newline"
                }),
            ]),
            Rect {
                y: footer_y,
                ..footer
            },
        );
    }
}

fn inline_text(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\n' => "\\n".into(),
            '\t' => "\\t".into(),
            c if c.is_control() => format!("\\u{{{:04x}}}", c as u32),
            c => c.to_string(),
        })
        .collect()
}

/// Stream only the requested page. Control escaping never materializes an expanded document.
fn preview_window(text: &str, width: u16, skip: usize, count: usize) -> (Vec<Line<'static>>, bool) {
    let page = preview_page(text, width, skip, count, false);
    (page.lines, page.row > skip.saturating_add(count))
}

pub(super) fn counted_preview(
    text: &str,
    width: u16,
    skip: usize,
    count: usize,
) -> (Vec<Line<'static>>, usize) {
    let page = preview_page(text, width, skip, count, true);
    (page.lines, page.row)
}

pub(super) fn preview_lines(
    text: &str,
    width: u16,
    skip: usize,
    count: usize,
) -> Vec<Line<'static>> {
    preview_page(text, width, skip, count, false).lines
}

/// Page a description and body without copying either complete string.
pub(super) fn detail_preview(
    description: &str,
    body: &str,
    width: u16,
    skip: usize,
    count: usize,
    scan_all: bool,
) -> (Vec<Line<'static>>, usize) {
    let lines = description
        .split('\n')
        .map(|line| (line, true))
        .chain([("", false), ("preview", false)])
        .chain(body.split('\n').map(|line| (line, false)));
    let page = preview_logical_lines(lines, width, skip, count, scan_all);
    (page.lines, page.row)
}

pub(super) fn description_row_count(description: &str, width: u16) -> usize {
    preview_logical_lines(
        description.split('\n').map(|line| (line, true)),
        width,
        0,
        0,
        true,
    )
    .row
}

struct Page {
    width: usize,
    skip: usize,
    count: usize,
    scan_all: bool,
    row: usize,
    used: usize,
    has_token: bool,
    line: String,
    lines: Vec<Line<'static>>,
}
impl Page {
    fn flush(&mut self) -> bool {
        if self.row >= self.skip && self.lines.len() < self.count {
            self.lines.push(Line::from(std::mem::take(&mut self.line)));
        } else {
            self.line.clear();
        }
        self.row += 1;
        self.used = 0;
        self.has_token = false;
        !self.scan_all && self.row > self.skip.saturating_add(self.count)
    }
    fn push(&mut self, symbol: &str) -> bool {
        let columns = Span::raw(symbol).width();
        if self.has_token && self.used + columns > self.width && self.flush() {
            return true;
        }
        if self.row >= self.skip && self.lines.len() < self.count {
            self.line.push_str(symbol);
        }
        self.used += columns;
        self.has_token = true;
        false
    }
}

fn preview_page(text: &str, width: u16, skip: usize, count: usize, scan_all: bool) -> Page {
    preview_logical_lines(
        text.split('\n').map(|line| (line, false)),
        width,
        skip,
        count,
        scan_all,
    )
}

fn preview_logical_lines<'a>(
    lines: impl Iterator<Item = (&'a str, bool)>,
    width: u16,
    skip: usize,
    count: usize,
    scan_all: bool,
) -> Page {
    let mut page = Page {
        width: usize::from(width),
        skip,
        count,
        scan_all,
        row: 0,
        used: 0,
        has_token: false,
        line: String::new(),
        lines: Vec::new(),
    };
    for (logical, is_description) in lines {
        // Split at words and controls together so a long single line is not
        // scanned to its end before yielding the first visible word.
        for segment in logical.split_inclusive(|c: char| c.is_whitespace() || c.is_control()) {
            let control = segment.chars().next_back().filter(|c| c.is_control());
            let normal = &segment[..segment.len() - control.map_or(0, char::len_utf8)];
            let span = Span::raw(normal);
            if page.has_token && page.used + span.width() > page.width && page.flush() {
                return page;
            }
            for grapheme in span.styled_graphemes(Style::default()) {
                if page.push(grapheme.symbol) {
                    return page;
                }
            }
            if let Some(control) = control {
                let escaped = if is_description && control == '\t' {
                    "    ".to_owned()
                } else if is_description {
                    format!("\\u{{{:04x}}}", control as u32)
                } else {
                    inline_text(&control.to_string())
                };
                for character in escaped.chars() {
                    if page.push(&character.to_string()) {
                        return page;
                    }
                }
            }
        }
        if page.flush() {
            return page;
        }
    }
    page
}

pub(super) fn complete(
    terminal: &mut Screen,
    interrupted: &Arc<AtomicBool>,
    name: &str,
    destination: &str,
    names: Vec<String>,
    supplied: &Variables,
    render: impl FnOnce(&Variables) -> Result<String>,
) -> Result<Option<String>> {
    let mut form = Form::new(names);
    let mut render = Some(render);
    loop {
        if interrupted.load(Ordering::Relaxed) {
            return Ok(None);
        }
        terminal.draw(|frame| form.draw(frame, name, destination))?;
        match poll_event(interrupted)? {
            Some(Event::Key(key)) => match form.key(key) {
                Action::Cancel => return Ok(None),
                Action::Accept => return Ok(form.preview.take()),
                Action::Render => {
                    let text = render.take().expect("one frozen render")(&form.values(supplied))?;
                    limits::check_text(&text, "rendered preview")?;
                    form.preview = Some(text);
                    form.fields.clear();
                    form.scroll = 0;
                }
                Action::Continue => {}
            },
            Some(Event::Paste(text)) if form.preview.is_none() => form.insert(&text),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    fn paint(form: &mut Form, width: u16, height: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
        t.draw(|f| form.draw(f, "notes/explain", "Copy to clipboard"))
            .unwrap();
        let b = t.backend().buffer();
        (0..height)
            .map(|y| (0..width).map(|x| b[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }
    #[test]
    fn fields_preserve_literal_multiline_unicode_and_distinguish_unanswered_empty() {
        let mut f = Form::new(vec!["lang".into(), "tone".into()]);
        paint(&mut f, 80, 24);
        f.insert("Rust 🦀\n{env:SECRET}\t  ");
        f.key(key(KeyCode::Tab));
        assert!(!f.fields[1].answered);
        assert_eq!(f.key(key(KeyCode::Enter)), Action::Render);
        let values = f.values(&Variables::from([("provided".into(), "default".into())]));
        assert_eq!(values["lang"], "Rust 🦀\n{env:SECRET}\t  ");
        assert_eq!(values["tone"], "");
        assert_eq!(values["provided"], "default");
    }
    #[test]
    fn app_labels_are_lowercase_without_changing_identity_or_path() {
        let mut f = Form::new(vec!["Language".into()]);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| f.draw(frame, "Notes/MyPrompt", "Write Out/Report.TXT"))
            .unwrap();
        let b = terminal.backend().buffer();
        let screen = (0..24)
            .map(|y| (0..80).map(|x| b[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(screen.contains("prompt · Notes/MyPrompt"));
        assert!(screen.contains("destination · write · Out/Report.TXT"));
        assert!(screen.contains("Language"));
    }
    #[test]
    fn narrow_field_hint_keeps_words_intact() {
        let mut f = Form::new(vec!["lang".into()]);
        let screen = paint(&mut f, 40, 18);
        assert!(screen.contains("empty"));
        assert!(screen.contains("value."));
        let rows: Vec<_> = screen.lines().collect();
        let hint = rows
            .iter()
            .position(|line| line.contains("value."))
            .unwrap();
        let keys = rows
            .iter()
            .position(|line| line.contains("tab/shift-tab"))
            .unwrap();
        assert!(keys > hint && keys - hint <= 2);
    }
    #[test]
    fn caret_edits_preserve_utf8_and_never_interpret_paste_as_keys() {
        let mut f = Form::new(vec!["value".into()]);
        f.insert("a🦀é");
        f.key(key(KeyCode::Left));
        f.key(key(KeyCode::Backspace));
        f.insert("中\n");
        assert_eq!(f.fields[0].text, "a中\né");
        assert!(f.fields[0].text.is_char_boundary(f.fields[0].cursor));
        f.insert("\u{1b}[2J");
        assert!(f.fields[0].text.contains('\u{1b}'));
        assert!(!paint(&mut f, 80, 24).contains('\u{1b}'));
    }
    #[test]
    fn invisible_identity_cannot_submit_and_resize_recovers() {
        let mut f = Form::new(vec!["value".into()]);
        assert!(paint(&mut f, 20, 6).contains("terminal too small"));
        f.insert("yes");
        assert_eq!(f.key(key(KeyCode::Enter)), Action::Continue);
        for (w, h) in [(40, 18), (80, 24), (120, 40)] {
            let s = paint(&mut f, w, h);
            assert!(f.visible);
            assert!(s.contains("notes/explain"));
            assert!(s.contains("esc cancel"));
        }
        assert_eq!(f.key(key(KeyCode::Enter)), Action::Render);
    }
    #[test]
    fn preview_is_frozen_safe_pageable_and_accepts_only_when_visible() {
        let original = "value\t\u{1b}[2J\nRésumé 中文 👩‍💻\n".repeat(60);
        let mut f = Form::new(vec![]);
        f.preview = Some(original.clone());
        let s = paint(&mut f, 80, 24);
        assert!(s.contains("\\t\\u{001b}[2J"));
        assert!(s.contains("👩‍💻"));
        assert!(!s.contains('\u{1b}'));
        assert!(f.more);
        f.key(key(KeyCode::PageDown));
        assert!(f.scroll > 0);
        paint(&mut f, 40, 18);
        assert_eq!(f.key(key(KeyCode::Enter)), Action::Accept);
        assert_eq!(f.preview.as_deref(), Some(original.as_str()));
        paint(&mut f, 20, 6);
        assert_eq!(f.key(key(KeyCode::Enter)), Action::Continue);
    }
    #[test]
    fn cancel_is_available_in_fields_preview_and_tiny_terminal() {
        for preview in [false, true] {
            for tiny in [false, true] {
                let mut f = Form::new(vec!["v".into()]);
                if preview {
                    f.preview = Some("frozen".into());
                }
                paint(&mut f, if tiny { 20 } else { 80 }, 24);
                assert_eq!(f.key(key(KeyCode::Esc)), Action::Cancel);
                assert_eq!(
                    f.key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
                    Action::Cancel
                );
            }
        }
    }

    #[test]
    fn oversized_paste_preserves_the_existing_value() {
        let mut f = Form::new(vec!["value".into()]);
        f.insert("keep");
        f.insert(&"x".repeat(limits::TEXT_BYTES));
        assert_eq!(f.fields[0].text, "keep");
        assert!(f.message.as_deref().unwrap().contains("too large"));
    }

    #[test]
    fn large_control_preview_only_materializes_the_visible_page() {
        let source = "\u{1b}".repeat(1_000_000);
        let (page, more) = preview_window(&source, 40, 0, 3);
        assert!(more);
        assert_eq!(page.len(), 3);
        assert!(
            page.iter()
                .map(|line| line.to_string().len())
                .sum::<usize>()
                <= 120
        );
        assert!(!page.iter().any(|line| line.to_string().contains('\u{1b}')));
    }

    #[test]
    fn description_controls_are_safe_and_tabs_display_as_four_spaces() {
        let description = "a\tb\rc\u{1b}d";
        let safe = super::super::safe_text(description);
        let combined = format!("{safe}\n\npreview\nbody");
        for width in [10, 30] {
            let (actual, _) = detail_preview(description, "body", width, 0, 30, true);
            let (expected, _) = counted_preview(&combined, width, 0, 30);
            let text = |lines: Vec<Line<'static>>| {
                lines
                    .into_iter()
                    .map(|line| line.to_string())
                    .collect::<String>()
            };
            let actual = text(actual);
            let expected = text(expected);
            assert!(!actual.contains('\u{1b}'));
            assert_eq!(
                actual
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect::<String>(),
                expected
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect::<String>()
            );
            if width == 30 {
                assert!(actual.contains("a    b"));
            }
        }
    }

    #[test]
    fn multipart_detail_matches_combined_paging_without_copying_the_body() {
        let description = "first line café\nsecond line";
        let body = "long body ".repeat(100_000);
        let combined = format!("{description}\n\npreview\n{body}");
        for width in [38, 50] {
            for skip in [0, 3, 20] {
                let (actual, actual_total) =
                    detail_preview(description, &body, width, skip, 4, true);
                let (expected, expected_total) = counted_preview(&combined, width, skip, 4);
                assert_eq!(actual_total, expected_total);
                assert_eq!(actual, expected);
                assert!(actual.len() <= 4);
            }
        }
    }

    #[test]
    fn counted_picker_preview_tracks_all_rows_but_keeps_only_a_page() {
        let source = "\u{1b}".repeat(100_000);
        let (page, total) = counted_preview(&source, 80, 2, 3);
        assert_eq!(total, 10_000);
        assert_eq!(page.len(), 3);
        assert_eq!(
            page.iter()
                .map(|line| line.to_string().len())
                .sum::<usize>(),
            240
        );
        assert!(!page.iter().any(|line| line.to_string().contains('\u{1b}')));
    }

    #[test]
    fn streamed_preview_keeps_only_the_page_and_escapes_control_bytes() {
        let source = (0..10000).map(|i| format!("line{i}\n")).collect::<String>();
        let (page, more) = preview_window(&source, 40, 100, 3);
        assert_eq!(page.len(), 3);
        assert!(more);
        assert_eq!(page[0].to_string(), "line100");
        let (page, _) = preview_window("a\r\nb\t", 40, 0, 4);
        assert_eq!(page[0].to_string(), "a\\u{000d}");
        assert_eq!(page[1].to_string(), "b\\t");
    }

    #[test]
    fn preview_wraps_at_words_and_pages_without_dropping_content() {
        let source = "Review this change for correctness and maintainability.";
        let (lines, total) = counted_preview(source, 20, 0, 10);
        assert_eq!(
            lines.iter().map(ToString::to_string).collect::<Vec<_>>(),
            [
                "Review this change ",
                "for correctness and ",
                "maintainability."
            ]
        );
        assert_eq!(total, 3);
        let page = preview_lines(source, 20, 1, 1);
        assert_eq!(page[0].to_string(), "for correctness and ");
    }
}
