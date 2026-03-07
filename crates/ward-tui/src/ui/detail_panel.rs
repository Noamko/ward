use crate::app::{AppMode, AppState, EditField, Panel};
use crate::ui::theme;
use chrono::Local;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub fn render(frame: &mut Frame, app: &AppState, area: Rect) {
    let is_active = app.active_panel == Panel::Detail;
    let editing = matches!(app.mode, AppMode::EditReminder | AppMode::NewReminder);

    let border_style = if is_active {
        theme::active_border()
    } else {
        theme::inactive_border()
    };

    // Note view — render markdown content directly
    if !editing && app.current_note().is_some() {
        render_note_view(frame, app, area, border_style);
        return;
    }

    let title = match app.mode {
        AppMode::NewReminder => " New Reminder ",
        AppMode::EditReminder => " Edit Reminder ",
        _ => " Detail ",
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(title)
        .title_style(if is_active { theme::accent() } else { theme::dim() })
        .style(Style::default().bg(theme::BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if editing {
        render_edit_form(frame, app, inner);
    } else {
        render_reminder_detail(frame, app, inner);
    }
}

fn render_note_view(
    frame: &mut Frame,
    app: &AppState,
    area: Rect,
    border_style: ratatui::style::Style,
) {
    let note = app.current_note().unwrap();
    let is_active = app.active_panel == Panel::Detail;

    let title = format!(" {} ", note.display_name());
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(title)
        .title_style(if is_active { theme::accent() } else { theme::dim() })
        .style(Style::default().bg(theme::BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if note.content.is_empty() {
        frame.render_widget(
            Paragraph::new(
                "\n  This note is empty.\n\n  Press [Enter] in the sidebar to open it in $EDITOR.",
            )
            .style(theme::dim()),
            inner,
        );
    } else {
        let md_lines = render_markdown(&note.content);
        frame.render_widget(
            Paragraph::new(md_lines).scroll((app.note_scroll as u16, 0)),
            inner,
        );
    }
}

fn render_reminder_detail(frame: &mut Frame, app: &AppState, area: Rect) {
    let Some(r) = app.selected_reminder_ref() else {
        frame.render_widget(
            Paragraph::new(
                "\n  Select a reminder to view details.\n  Press [n] to create one.",
            )
            .style(theme::dim()),
            area,
        );
        return;
    };

    let due_str = r.due_at.map(|d| {
        d.with_timezone(&Local)
            .format("%Y-%m-%d %H:%M")
            .to_string()
    });

    let title_style = if r.done {
        theme::done()
    } else {
        theme::base().add_modifier(Modifier::BOLD)
    };

    let due_style = if r.done {
        theme::done()
    } else if r.is_overdue() {
        theme::overdue()
    } else if r.is_due_today() {
        theme::due_today()
    } else {
        theme::base()
    };

    let priority_style = match r.priority {
        ward_core::model::Priority::High => theme::priority_high(),
        ward_core::model::Priority::Medium => theme::priority_medium(),
        ward_core::model::Priority::Low => theme::priority_low(),
    };

    let mut lines: Vec<Line<'static>> = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  ", theme::dim()),
            Span::styled(r.title.clone(), title_style),
        ]),
        Line::from(""),
    ];

    let status = if r.done { "✓ Done" } else { "● Pending" };
    let status_style = if r.done {
        theme::done()
    } else {
        Style::default().fg(theme::GREEN)
    };
    lines.push(Line::from(vec![
        Span::styled("  Status:   ", theme::dim()),
        Span::styled(status, status_style),
    ]));

    lines.push(Line::from(vec![
        Span::styled("  Priority: ", theme::dim()),
        Span::styled(r.priority.label(), priority_style),
    ]));

    if let Some(due) = &due_str {
        let due_label = if r.is_overdue() {
            "  Due:      ⚠ "
        } else {
            "  Due:        "
        };
        lines.push(Line::from(vec![
            Span::styled(due_label, theme::dim()),
            Span::styled(due.clone(), due_style),
        ]));
    }

    // Tags
    if !r.tags.is_empty() {
        let tag_spans: Vec<Span<'static>> = std::iter::once(Span::styled("  Tags:     ", theme::dim()))
            .chain(
                r.tags
                    .iter()
                    .enumerate()
                    .flat_map(|(i, t)| {
                        let sep = if i > 0 { vec![Span::styled("  ", theme::dim())] } else { vec![] };
                        let tag = vec![Span::styled(
                            format!("#{}", t),
                            Style::default().fg(Color::Cyan),
                        )];
                        sep.into_iter().chain(tag)
                    }),
            )
            .collect();
        lines.push(Line::from(tag_spans));
    }

    // Recurrence
    if let Some(rec) = &r.recurrence {
        lines.push(Line::from(vec![
            Span::styled("  Repeat:   ", theme::dim()),
            Span::styled(rec.label(), Style::default().fg(Color::Magenta)),
        ]));
    }

    // Subtasks
    if !r.subtasks.is_empty() {
        lines.push(Line::from(""));
        let done = r.subtasks.iter().filter(|s| s.done).count();
        lines.push(Line::from(vec![
            Span::styled("  Subtasks: ", theme::dim()),
            Span::styled(
                format!("{}/{}", done, r.subtasks.len()),
                theme::dim(),
            ),
        ]));
        for s in &r.subtasks {
            let (check, style) = if s.done {
                ("[✓]", theme::done())
            } else {
                ("[ ]", theme::base())
            };
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(check, style),
                Span::raw(" "),
                Span::styled(s.title.clone(), style),
            ]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  Created: ", theme::dim()),
        Span::styled(
            r.created_at
                .with_timezone(&Local)
                .format("%Y-%m-%d %H:%M")
                .to_string(),
            theme::dim(),
        ),
    ]));

    frame.render_widget(Paragraph::new(lines).style(theme::base()), area);
}

fn render_edit_form(frame: &mut Frame, app: &AppState, area: Rect) {
    let Some(es) = &app.edit else { return };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title label
            Constraint::Length(3), // title input
            Constraint::Length(1), // due label
            Constraint::Length(3), // due input
            Constraint::Length(1), // due preview/error
            Constraint::Length(1), // priority label
            Constraint::Length(3), // priority selector
            Constraint::Length(1), // tags label
            Constraint::Length(3), // tags input
            Constraint::Length(1), // recurrence label
            Constraint::Length(3), // recurrence selector
            Constraint::Min(0),
        ])
        .split(area);

    // Title
    let title_active = es.focused_field == EditField::Title;
    frame.render_widget(
        Paragraph::new("Title:").style(if title_active { theme::accent() } else { theme::dim() }),
        chunks[0],
    );
    let title_block = Block::default()
        .borders(Borders::ALL)
        .border_style(if title_active {
            theme::active_border()
        } else {
            theme::inactive_border()
        })
        .style(Style::default().bg(theme::SURFACE));
    let title_inner = title_block.inner(chunks[1]);
    frame.render_widget(title_block, chunks[1]);
    frame.render_widget(
        Paragraph::new(es.title.value()).style(theme::base()),
        title_inner,
    );
    if title_active {
        frame.set_cursor_position((
            title_inner.x + es.title.visual_cursor() as u16,
            title_inner.y,
        ));
    }

    // Due date
    let due_active = es.focused_field == EditField::DueAt;
    frame.render_widget(
        Paragraph::new("Due date:").style(if due_active { theme::accent() } else { theme::dim() }),
        chunks[2],
    );
    let due_block = Block::default()
        .borders(Borders::ALL)
        .border_style(if due_active {
            theme::active_border()
        } else {
            theme::inactive_border()
        })
        .style(Style::default().bg(theme::SURFACE));
    let due_inner = due_block.inner(chunks[3]);
    frame.render_widget(due_block, chunks[3]);
    frame.render_widget(
        Paragraph::new(es.due_input.value()).style(theme::base()),
        due_inner,
    );
    if due_active {
        frame.set_cursor_position((
            due_inner.x + es.due_input.visual_cursor() as u16,
            due_inner.y,
        ));
    }

    // Due parse preview / error
    let preview = if es.due_input.value().trim().is_empty() {
        Span::styled("  no due date", theme::dim())
    } else {
        match crate::app::parse_due(es.due_input.value().trim()) {
            Some(dt) => Span::styled(
                format!(
                    "  → {}",
                    dt.with_timezone(&Local).format("%Y-%m-%d %H:%M")
                ),
                Style::default().fg(theme::GREEN),
            ),
            None => Span::styled("  invalid date format", theme::overdue()),
        }
    };
    frame.render_widget(Paragraph::new(Line::from(preview)), chunks[4]);

    // Priority
    let prio_active = es.focused_field == EditField::Priority;
    frame.render_widget(
        Paragraph::new("Priority (←→):").style(if prio_active { theme::accent() } else { theme::dim() }),
        chunks[5],
    );
    let prio_block = Block::default()
        .borders(Borders::ALL)
        .border_style(if prio_active { theme::active_border() } else { theme::inactive_border() })
        .style(Style::default().bg(theme::SURFACE));
    let prio_inner = prio_block.inner(chunks[6]);
    frame.render_widget(prio_block, chunks[6]);
    let (prio_label, prio_style) = match es.priority {
        ward_core::model::Priority::Low => ("  ○  Low", theme::priority_low()),
        ward_core::model::Priority::Medium => ("  ●  Medium", theme::priority_medium()),
        ward_core::model::Priority::High => ("  !! High", theme::priority_high()),
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(prio_label, prio_style))),
        prio_inner,
    );

    // Tags
    let tags_active = es.focused_field == EditField::Tags;
    frame.render_widget(
        Paragraph::new("Tags (comma-separated):").style(if tags_active { theme::accent() } else { theme::dim() }),
        chunks[7],
    );
    let tags_block = Block::default()
        .borders(Borders::ALL)
        .border_style(if tags_active { theme::active_border() } else { theme::inactive_border() })
        .style(Style::default().bg(theme::SURFACE));
    let tags_inner = tags_block.inner(chunks[8]);
    frame.render_widget(tags_block, chunks[8]);
    frame.render_widget(
        Paragraph::new(es.tags_input.value()).style(theme::base()),
        tags_inner,
    );
    if tags_active {
        frame.set_cursor_position((
            tags_inner.x + es.tags_input.visual_cursor() as u16,
            tags_inner.y,
        ));
    }

    // Recurrence
    let rec_active = es.focused_field == EditField::Recurrence;
    frame.render_widget(
        Paragraph::new("Repeat (←→):").style(if rec_active { theme::accent() } else { theme::dim() }),
        chunks[9],
    );
    let rec_block = Block::default()
        .borders(Borders::ALL)
        .border_style(if rec_active { theme::active_border() } else { theme::inactive_border() })
        .style(Style::default().bg(theme::SURFACE));
    let rec_inner = rec_block.inner(chunks[10]);
    frame.render_widget(rec_block, chunks[10]);
    let rec_label = match &es.recurrence {
        None => Span::styled("  None", theme::dim()),
        Some(r) => Span::styled(
            format!("  {}", r.label()),
            Style::default().fg(Color::Magenta),
        ),
    };
    frame.render_widget(Paragraph::new(Line::from(rec_label)), rec_inner);
}

// ── Markdown renderer ─────────────────────────────────────────────────────────

fn render_markdown(input: &str) -> Vec<Line<'static>> {
    input.lines().map(render_md_line).collect()
}

fn render_md_line(line: &str) -> Line<'static> {
    if line.trim() == "---" || line.trim() == "***" || line.trim() == "___" {
        return Line::from(Span::styled(
            "  ──────────────────────────────",
            theme::dim(),
        ));
    }

    if let Some(rest) = line.strip_prefix("### ") {
        return Line::from(vec![
            Span::styled("  ", theme::dim()),
            Span::styled(
                rest.to_owned(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            ),
        ]);
    }
    if let Some(rest) = line.strip_prefix("## ") {
        return Line::from(vec![
            Span::styled("  ", theme::dim()),
            Span::styled(
                rest.to_owned(),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
        ]);
    }
    if let Some(rest) = line.strip_prefix("# ") {
        return Line::from(vec![
            Span::styled("  ", theme::dim()),
            Span::styled(
                rest.to_owned(),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
        ]);
    }

    if let Some(rest) = line
        .strip_prefix("  - ")
        .or_else(|| line.strip_prefix("  * "))
    {
        let mut spans = vec![Span::styled("    ◦ ", theme::dim())];
        spans.extend(parse_inline(rest));
        return Line::from(spans);
    }

    if let Some(rest) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
        let mut spans = vec![Span::styled("  • ", theme::dim())];
        spans.extend(parse_inline(rest));
        return Line::from(spans);
    }

    if let Some(rest) = line.strip_prefix("> ") {
        let mut spans = vec![Span::styled("  │ ", theme::dim())];
        spans.extend(parse_inline(rest));
        return Line::from(spans);
    }

    if line.trim().is_empty() {
        return Line::from("");
    }

    let mut spans = vec![Span::raw("  ")];
    spans.extend(parse_inline(line));
    Line::from(spans)
}

fn parse_inline(input: &str) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let mut plain = String::new();

    macro_rules! flush {
        () => {
            if !plain.is_empty() {
                spans.push(Span::raw(plain.clone()));
                plain.clear();
            }
        };
    }

    while i < chars.len() {
        // Inline code: `text`
        if chars[i] == '`' {
            flush!();
            i += 1;
            let start = i;
            while i < chars.len() && chars[i] != '`' {
                i += 1;
            }
            let code: String = chars[start..i].iter().collect();
            spans.push(Span::styled(code, Style::default().fg(Color::Yellow)));
            if i < chars.len() {
                i += 1;
            }
            continue;
        }

        // Bold: **text**
        if chars[i] == '*' && i + 1 < chars.len() && chars[i + 1] == '*' {
            flush!();
            i += 2;
            let start = i;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '*') {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            spans.push(Span::styled(text, Style::default().add_modifier(Modifier::BOLD)));
            if i + 1 < chars.len() {
                i += 2;
            }
            continue;
        }

        // Italic: *text* or _text_
        if chars[i] == '*' || chars[i] == '_' {
            let marker = chars[i];
            let has_close = chars[i + 1..].contains(&marker);
            if has_close {
                flush!();
                i += 1;
                let start = i;
                while i < chars.len() && chars[i] != marker {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                spans.push(Span::styled(
                    text,
                    Style::default().add_modifier(Modifier::ITALIC),
                ));
                if i < chars.len() {
                    i += 1;
                }
                continue;
            }
        }

        plain.push(chars[i]);
        i += 1;
    }

    flush!();
    spans
}
