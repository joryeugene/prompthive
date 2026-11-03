//! Ephemeral value entry followed by a frozen, safely displayed output buffer.
use super::{poll_event, safe_text, tail_to_width, wrap_text, Screen, SMALL_TERMINAL};
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
            self.message = Some("Values are too large; nothing was truncated.".into());
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
                Paragraph::new(wrap_text(SMALL_TERMINAL, outer.width)),
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
            "Preview rendered prompt"
        } else {
            "Fill required values"
        })
        .style(Style::default().add_modifier(Modifier::BOLD))];
        heading.extend(wrap_text(
            &format!("Prompt: {}", safe_text(name)),
            area.width,
        ));
        heading.extend(wrap_text(
            &format!("Destination: {}", safe_text(destination)),
            area.width,
        ));
        if heading.len() + 7 > area.height as usize {
            frame.render_widget(Paragraph::new(wrap_text(SMALL_TERMINAL, area.width)), area);
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
            frame.render_widget(Paragraph::new(lines), content);
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(if area.width < 34 {
                        "Enter use  Esc cancel"
                    } else {
                        "Enter use  Esc cancel  PgUp/PgDn"
                    }),
                    Line::from(if area.width < 34 {
                        "PgUp/PgDn; controls escaped"
                    } else {
                        "Controls escaped; output unchanged."
                    }),
                ]),
                footer,
            );
            return;
        }
        let field = &self.fields[self.active];
        let label = format!("{} ({}/{})", field.name, self.active + 1, self.fields.len());
        let label = wrap_text(&safe_text(&label), content.width);
        if label.len() + 2 > content.height as usize {
            self.visible = false;
            frame.render_widget(
                Paragraph::new(wrap_text(SMALL_TERMINAL, content.width)),
                content,
            );
            return;
        }
        let label_height = label.len() as u16;
        frame.render_widget(
            Paragraph::new(label),
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
            "Value set. Paste keeps newlines."
        } else {
            "Unset. Enter accepts even an empty value."
        });
        frame.render_widget(
            Paragraph::new(wrap_text(message, content.width)),
            Rect {
                y: content.y + label_height + 2,
                height: content.height.saturating_sub(label_height + 2),
                ..content
            },
        );
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(if area.width < 34 {
                    "Tab field  Esc cancel"
                } else {
                    "Tab/Shift-Tab field  Esc cancel"
                }),
                Line::from(if area.width < 34 {
                    "Shift-Tab back  Enter preview"
                } else {
                    "Enter preview/next  Alt-Enter newline"
                }),
            ]),
            footer,
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
    let mut lines = Vec::new();
    let mut row = 0;
    let mut line = String::new();
    let mut used = 0;
    for logical in text.split('\n') {
        // Span deliberately filters control graphemes; separate them first so
        // the preview visibly represents every byte rather than dropping tabs/CR.
        for segment in logical.split_inclusive(char::is_control) {
            let control = segment.chars().next_back().filter(|c| c.is_control());
            let normal = &segment[..segment.len() - control.map_or(0, char::len_utf8)];
            let span = Span::raw(normal);
            for grapheme in span.styled_graphemes(Style::default()) {
                if preview_token(
                    grapheme.symbol,
                    width,
                    skip,
                    count,
                    &mut lines,
                    &mut row,
                    &mut line,
                    &mut used,
                ) {
                    return (lines, true);
                }
            }
            if let Some(control) = control {
                let escaped = inline_text(&control.to_string());
                for character in escaped.chars() {
                    if preview_token(
                        &character.to_string(),
                        width,
                        skip,
                        count,
                        &mut lines,
                        &mut row,
                        &mut line,
                        &mut used,
                    ) {
                        return (lines, true);
                    }
                }
            }
        }
        if row >= skip {
            lines.push(Line::from(std::mem::take(&mut line)));
        } else {
            line.clear();
        }
        row += 1;
        used = 0;
        if lines.len() > count {
            lines.pop();
            return (lines, true);
        }
    }
    (lines, false)
}

#[allow(clippy::too_many_arguments)]
fn preview_token(
    symbol: &str,
    width: u16,
    skip: usize,
    count: usize,
    lines: &mut Vec<Line<'static>>,
    row: &mut usize,
    line: &mut String,
    used: &mut usize,
) -> bool {
    let columns = Span::raw(symbol).width();
    if *used + columns > width as usize && !line.is_empty() {
        if *row >= skip {
            lines.push(Line::from(std::mem::take(line)));
        } else {
            line.clear();
        }
        *row += 1;
        *used = 0;
        if lines.len() > count {
            lines.pop();
            return true;
        }
    }
    line.push_str(symbol);
    *used += columns;
    false
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
        assert!(paint(&mut f, 20, 6).contains("Terminal too small"));
        f.insert("yes");
        assert_eq!(f.key(key(KeyCode::Enter)), Action::Continue);
        for (w, h) in [(40, 18), (80, 24), (120, 40)] {
            let s = paint(&mut f, w, h);
            assert!(f.visible);
            assert!(s.contains("notes/explain"));
            assert!(s.contains("Esc cancel"));
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
}
