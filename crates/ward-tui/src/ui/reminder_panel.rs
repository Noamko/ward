use crate::app::{AppState, Panel, SortMode};
use crate::ui::theme;
use chrono::Local;
use ratatui::{
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

pub fn render(frame: &mut Frame, app: &AppState, area: ratatui::layout::Rect) {
    let is_active = app.active_panel == Panel::Reminders;
    let border_style = if is_active {
        theme::active_border()
    } else {
        theme::inactive_border()
    };

    // When a note is selected, the middle panel just shows a hint.
    if app.current_note().is_some() {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(" Note ")
            .title_style(if is_active { theme::accent() } else { theme::dim() })
            .style(Style::default().bg(theme::BG));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(
            Paragraph::new("\n  📝 Markdown note\n\n  [e]  open in $EDITOR\n  [d]  delete")
                .style(theme::dim()),
            inner,
        );
        return;
    }

    let sort_suffix = if app.sort_mode != SortMode::Default {
        format!(" ↕ {}", app.sort_mode.label())
    } else {
        String::new()
    };
    let list_name = app
        .current_list()
        .map(|l| format!(" {}{} ", l.display_name(), sort_suffix))
        .unwrap_or_else(|| " Reminders ".to_string());

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(list_name)
        .title_style(if is_active { theme::accent() } else { theme::dim() })
        .style(Style::default().bg(theme::BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let reminders = app.visible_reminders();

    if reminders.is_empty() {
        let hint = if app.current_list().is_some() {
            " No reminders. Press [n] to add one."
        } else {
            " No lists. Press [n] to create one."
        };
        frame.render_widget(
            Paragraph::new(hint).style(theme::dim()),
            inner,
        );
        return;
    }

    let max_title = (inner.width as usize).saturating_sub(12);

    let items: Vec<ListItem> = reminders
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let checkbox = if r.done { "[✓]" } else { "[ ]" };
            let priority_ind = r.priority.indicator();

            let title = if r.title.len() > max_title {
                format!("{}…", &r.title[..max_title.saturating_sub(1)])
            } else {
                r.title.clone()
            };

            let due_str = r.due_at.map(|d| {
                let local = d.with_timezone(&Local);
                if r.is_overdue() {
                    format!(" ⚠ {}", local.format("%d/%m %H:%M"))
                } else if r.is_due_today() {
                    format!(" ◷ {}", local.format("%H:%M"))
                } else {
                    format!(" {}", local.format("%d/%m"))
                }
            });

            let is_selected = i == app.selected_reminder && is_active;

            let base_style = if r.done {
                theme::done()
            } else if is_selected {
                theme::selected()
            } else {
                theme::base()
            };

            let priority_style = if r.done {
                theme::done()
            } else {
                match r.priority {
                    ward_core::model::Priority::High => theme::priority_high(),
                    ward_core::model::Priority::Medium => theme::priority_medium(),
                    ward_core::model::Priority::Low => theme::priority_low(),
                }
            };

            let due_style = if r.done {
                theme::done()
            } else if r.is_overdue() {
                theme::overdue()
            } else if r.is_due_today() {
                theme::due_today()
            } else {
                theme::dim()
            };

            let prefix = if is_selected { "▶ " } else { "  " };

            let mut spans = vec![
                Span::raw(prefix),
                Span::styled(checkbox, base_style),
                Span::raw(" "),
                Span::styled(title, base_style),
            ];

            if let Some(due) = due_str {
                spans.push(Span::styled(due, due_style));
            }

            if let Some(summary) = r.subtask_summary() {
                spans.push(Span::styled(
                    format!(" [{}]", summary),
                    theme::dim(),
                ));
            }

            if !r.tags.is_empty() {
                for tag in r.tags.iter().take(2) {
                    spans.push(Span::styled(
                        format!(" #{}", tag),
                        Style::default().fg(ratatui::style::Color::Cyan),
                    ));
                }
            }

            spans.push(Span::raw(" "));
            spans.push(Span::styled(priority_ind, priority_style));

            ListItem::new(Line::from(spans))
        })
        .collect();

    let mut list_state = ListState::default();
    if !reminders.is_empty() {
        list_state.select(Some(app.selected_reminder));
    }

    frame.render_stateful_widget(
        List::new(items).style(Style::default().bg(theme::BG)),
        inner,
        &mut list_state,
    );
}
