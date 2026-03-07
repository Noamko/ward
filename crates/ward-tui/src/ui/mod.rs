mod detail_panel;
mod list_panel;
mod reminder_panel;
pub mod theme;

use crate::app::{AppMode, AppState, Panel};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

pub fn render(frame: &mut Frame, app: &AppState) {
    let area = frame.area();
    frame.render_widget(Block::default().style(Style::default().bg(theme::BG)), area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    render_titlebar(frame, chunks[0]);
    render_main(frame, app, chunks[1]);
    render_statusbar(frame, app, chunks[2]);

    // Overlays (drawn last, on top)
    match app.mode {
        AppMode::ConfirmDelete => render_confirm_delete(frame, app, area),
        AppMode::Help => render_help(frame, area),
        AppMode::Search => render_search_overlay(frame, app, area),
        AppMode::MoveReminder => render_move_overlay(frame, app, area),
        _ => {}
    }
}

fn render_titlebar(frame: &mut Frame, area: Rect) {
    let title = Line::from(vec![
        Span::styled(" rmdr ", theme::accent()),
        Span::styled("─ Terminal Reminders & Notes", theme::dim()),
    ]);
    frame.render_widget(Paragraph::new(title).style(theme::base()), area);
}

fn render_main(frame: &mut Frame, app: &AppState, area: Rect) {
    if app.current_note().is_some() && !matches!(app.mode, AppMode::EditReminder | AppMode::NewReminder) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(22), Constraint::Min(0)])
            .split(area);
        list_panel::render(frame, app, chunks[0]);
        detail_panel::render(frame, app, chunks[1]);
    } else {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(22),
                Constraint::Percentage(40),
                Constraint::Percentage(38),
            ])
            .split(area);
        list_panel::render(frame, app, chunks[0]);
        reminder_panel::render(frame, app, chunks[1]);
        detail_panel::render(frame, app, chunks[2]);
    }
}

fn render_statusbar(frame: &mut Frame, app: &AppState, area: Rect) {
    // Show status message if set
    if let Some(msg) = &app.status_message {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(" "),
                Span::styled(msg.as_str(), Style::default().fg(Color::Green)),
            ]))
            .style(Style::default().bg(theme::SURFACE)),
            area,
        );
        return;
    }

    let is_note = app.current_note().is_some();

    let hints: Vec<(&str, &str)> = match app.mode {
        AppMode::Browse => match app.active_panel {
            Panel::Lists => {
                if is_note {
                    vec![
                        ("↑↓", "navigate"), ("Shift+↑↓", "reorder"),
                        ("e", "edit note"), ("d", "delete"),
                        ("N", "new note"), ("n", "new list"), ("x", "export"), ("q", "quit"),
                    ]
                } else {
                    vec![
                        ("↑↓", "navigate"), ("Shift+↑↓", "reorder"),
                        ("n", "new list"), ("N", "new note"), ("e", "rename"),
                        ("d", "delete"), ("u", "undo"), ("x", "export"), ("q", "quit"), ("?", "help"),
                    ]
                }
            }
            Panel::Reminders => vec![
                ("↑↓", "navigate"), ("n", "new"), ("e", "edit"), ("Space", "done"),
                ("d", "delete"), ("m", "move"), ("s", "sort"), ("/", "search"),
                ("u", "undo"), ("h", "toggle done"), ("q", "quit"),
            ],
            Panel::Detail => {
                if is_note {
                    vec![("↑↓", "scroll"), ("e", "edit in $EDITOR"), ("x", "export"), ("q", "quit")]
                } else {
                    vec![
                        ("e", "edit"), ("Space", "done"), ("d", "delete"),
                        ("m", "move"), ("u", "undo"), ("q", "quit"),
                    ]
                }
            }
        },
        AppMode::EditReminder | AppMode::NewReminder => vec![
            ("Tab", "next field"), ("Enter", "save"), ("Esc", "cancel"), ("←→", "toggle"),
        ],
        AppMode::EditList | AppMode::NewList | AppMode::EditNote | AppMode::NewNote => vec![
            ("Tab", "next field"), ("Enter", "save"), ("Esc", "cancel"),
        ],
        AppMode::ConfirmDelete => vec![("y", "confirm"), ("n/Esc", "cancel")],
        AppMode::Search => vec![("type", "filter"), ("Enter", "confirm"), ("Esc", "cancel")],
        AppMode::MoveReminder => vec![("↑↓", "pick list"), ("Enter", "move here"), ("Esc", "cancel")],
        AppMode::Help => vec![("any key", "close")],
    };

    let mut spans: Vec<Span> = vec![Span::raw(" ")];
    for (i, (key, desc)) in hints.iter().enumerate() {
        if i > 0 { spans.push(Span::styled("  ", theme::dim())); }
        spans.push(Span::styled(format!("[{}]", key), theme::key_hint()));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(*desc, theme::key_desc()));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(theme::SURFACE)),
        area,
    );
}

// ── Overlays ──────────────────────────────────────────────────────────────────

fn render_confirm_delete(frame: &mut Frame, app: &AppState, area: Rect) {
    let popup_area = centered_rect(50, 5, area);
    frame.render_widget(Clear, popup_area);
    let text = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("  Delete "),
            Span::styled(&app.delete_label, Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("?"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  [y] ", theme::key_hint()),
            Span::styled("Yes, delete  ", theme::key_desc()),
            Span::styled("[n] ", theme::key_hint()),
            Span::styled("Cancel", theme::key_desc()),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::RED))
                .title(" Confirm Delete ")
                .title_style(Style::default().fg(theme::RED).add_modifier(Modifier::BOLD))
                .style(Style::default().bg(theme::SURFACE)),
        ),
        popup_area,
    );
}

fn render_search_overlay(frame: &mut Frame, app: &AppState, area: Rect) {
    use crate::app::SearchJump;
    use ratatui::layout::{Constraint, Direction, Layout};

    let results = &app.search_results;
    let popup_height = (results.len() as u16 * 2 + 5).clamp(7, area.height.saturating_sub(4));
    let popup_area = centered_rect(72, popup_height, area);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::active_border())
        .title(" Search ")
        .title_style(theme::accent())
        .style(Style::default().bg(theme::SURFACE));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Split inner: 1 row for input, rest for results
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);

    // Input row
    let query = app.search_input.value();
    let input_line = Line::from(vec![
        Span::styled("> ", theme::accent()),
        Span::styled(query, theme::base()),
    ]);
    frame.render_widget(Paragraph::new(input_line), sections[0]);
    frame.set_cursor_position((
        sections[0].x + 2 + app.search_input.visual_cursor() as u16,
        sections[0].y,
    ));

    // Results area
    let results_area = sections[1];
    if results.is_empty() {
        let hint = if !query.is_empty() {
            " No results."
        } else {
            " Type to search across all notes and reminders."
        };
        frame.render_widget(Paragraph::new(hint).style(theme::dim()), results_area);
        return;
    }

    // Scroll so selected item is always visible (2 rows per result)
    let visible_results = (results_area.height as usize / 2).max(1);
    let scroll_offset = if app.search_cursor >= visible_results {
        app.search_cursor - visible_results + 1
    } else {
        0
    };

    let mut y = results_area.y;
    for (i, result) in results.iter().enumerate().skip(scroll_offset) {
        if y + 1 >= results_area.y + results_area.height {
            break;
        }
        let is_selected = i == app.search_cursor;
        let sel_marker = if is_selected { ">" } else { " " };
        let source_icon = match &result.jump {
            SearchJump::Note { .. } => "\u{1f4dd} ", // 📝
            SearchJump::Reminder { .. } => "\u{2610} ", // ☐
        };
        let source_style = if is_selected { theme::accent() } else { theme::dim() };

        // Source label row
        let source_line = Line::from(vec![
            Span::styled(format!("{} ", sel_marker), source_style),
            Span::styled(source_icon, source_style),
            Span::styled(result.source_label.clone(), source_style),
        ]);
        frame.render_widget(
            Paragraph::new(source_line),
            Rect::new(results_area.x, y, results_area.width, 1),
        );
        y += 1;
        if y >= results_area.y + results_area.height {
            break;
        }

        // Snippet row with highlighted match
        let snippet = &result.snippet;
        let ms = result.match_start.min(snippet.len());
        let me = result.match_end.min(snippet.len());
        let before = &snippet[..ms];
        let matched = &snippet[ms..me];
        let after = &snippet[me..];

        let snip_base = if is_selected { theme::selected() } else { theme::base() };
        let snip_hl = Style::default()
            .fg(ratatui::style::Color::Yellow)
            .add_modifier(Modifier::BOLD);

        let snippet_line = Line::from(vec![
            Span::raw("    "),
            Span::styled(before.to_string(), snip_base),
            Span::styled(matched.to_string(), snip_hl),
            Span::styled(after.to_string(), snip_base),
        ]);
        frame.render_widget(
            Paragraph::new(snippet_line),
            Rect::new(results_area.x, y, results_area.width, 1),
        );
        y += 1;
    }
}

fn render_move_overlay(frame: &mut Frame, app: &AppState, area: Rect) {
    use crate::app::item_at_path_pub;
    use ward_core::model::Item;

    // Collect all lists from the flat view (including ones inside folders)
    let flat = app.flat_sidebar();
    let lists: Vec<(usize, &str)> = flat
        .iter()
        .enumerate()
        .filter_map(|(flat_idx, fi)| {
            if let Some(Item::List(l)) = item_at_path_pub(&app.store.items, &fi.path) {
                Some((flat_idx, l.name.as_str()))
            } else {
                None
            }
        })
        .collect();

    let height = (lists.len() as u16 + 4).min(area.height);
    let popup_area = centered_rect(50, height, area);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::active_border())
        .title(" Move to list ")
        .title_style(theme::accent())
        .style(Style::default().bg(theme::SURFACE));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let items: Vec<ListItem> = lists
        .iter()
        .map(|(item_idx, name)| {
            let is_current = *item_idx == app.selected_item;
            let is_cursor = *item_idx == app.move_list_cursor;
            let prefix = if is_cursor { "▶ " } else { "  " };
            let style = if is_cursor {
                theme::selected()
            } else if is_current {
                theme::dim()
            } else {
                theme::base()
            };
            let suffix = if is_current { "  (current)" } else { "" };
            ListItem::new(Line::from(vec![
                Span::raw(prefix),
                Span::styled(name.to_string() + suffix, style),
            ]))
        })
        .collect();

    let mut list_state = ListState::default();
    let cursor_pos = lists.iter().position(|(i, _)| *i == app.move_list_cursor);
    list_state.select(cursor_pos);

    frame.render_stateful_widget(List::new(items), inner, &mut list_state);
}

fn render_help(frame: &mut Frame, area: Rect) {
    let popup_area = centered_rect(72, 32, area);
    frame.render_widget(Clear, popup_area);
    let text = vec![
        Line::from(""),
        Line::from(Span::styled("  Navigation", theme::accent())),
        Line::from("  ──────────────────────────────────────────────"),
        Line::from(vec![Span::styled("  Tab / Shift+Tab", theme::key_hint()), Span::styled("     Switch panel", theme::key_desc())]),
        Line::from(vec![Span::styled("  ↑↓ / k j", theme::key_hint()), Span::styled("           Move up / down", theme::key_desc())]),
        Line::from(vec![Span::styled("  Shift+↑↓", theme::key_hint()), Span::styled("           Reorder sidebar items", theme::key_desc())]),
        Line::from(""),
        Line::from(Span::styled("  Lists & Notes (sidebar)", theme::accent())),
        Line::from("  ──────────────────────────────────────────────"),
        Line::from(vec![Span::styled("  n / N", theme::key_hint()), Span::styled("               New list / new note", theme::key_desc())]),
        Line::from(vec![Span::styled("  e", theme::key_hint()), Span::styled("                   Open note in $EDITOR  /  rename list", theme::key_desc())]),
        Line::from(vec![Span::styled("  Enter / →", theme::key_hint()), Span::styled("           Enter list", theme::key_desc())]),
        Line::from(vec![Span::styled("  d", theme::key_hint()), Span::styled("                   Delete  (u to undo)", theme::key_desc())]),
        Line::from(vec![Span::styled("  x", theme::key_hint()), Span::styled("                   Export to ~/name.md", theme::key_desc())]),
        Line::from(""),
        Line::from(Span::styled("  Reminders", theme::accent())),
        Line::from("  ──────────────────────────────────────────────"),
        Line::from(vec![Span::styled("  n", theme::key_hint()), Span::styled("                   New reminder", theme::key_desc())]),
        Line::from(vec![Span::styled("  e", theme::key_hint()), Span::styled("                   Edit reminder", theme::key_desc())]),
        Line::from(vec![Span::styled("  Space", theme::key_hint()), Span::styled("               Toggle done/undone", theme::key_desc())]),
        Line::from(vec![Span::styled("  d", theme::key_hint()), Span::styled("                   Delete  (u to undo)", theme::key_desc())]),
        Line::from(vec![Span::styled("  m", theme::key_hint()), Span::styled("                   Move to another list", theme::key_desc())]),
        Line::from(vec![Span::styled("  s", theme::key_hint()), Span::styled("                   Cycle sort order", theme::key_desc())]),
        Line::from(vec![Span::styled("  /", theme::key_hint()), Span::styled("                   Search / filter", theme::key_desc())]),
        Line::from(vec![Span::styled("  h", theme::key_hint()), Span::styled("                   Toggle show completed", theme::key_desc())]),
        Line::from(vec![Span::styled("  x", theme::key_hint()), Span::styled("                   Export list to ~/name.md", theme::key_desc())]),
        Line::from(""),
        Line::from(Span::styled("  Due date formats", theme::accent())),
        Line::from("  ──────────────────────────────────────────────"),
        Line::from(vec![Span::styled("  today / tomorrow", theme::key_hint()), Span::styled("     [HH:MM]", theme::key_desc())]),
        Line::from(vec![Span::styled("  next monday", theme::key_hint()), Span::styled("         (or any weekday)", theme::key_desc())]),
        Line::from(vec![Span::styled("  in N days  /  YYYY-MM-DD HH:MM", theme::key_hint())]),
    ];
    frame.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme::active_border())
                .title(" Help — rmdr keybindings ")
                .title_style(theme::accent())
                .style(Style::default().bg(theme::SURFACE)),
        ),
        popup_area,
    );
}

pub fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let popup_width = area.width * percent_x / 100;
    let x = area.x + (area.width - popup_width) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, popup_width, height.min(area.height))
}
