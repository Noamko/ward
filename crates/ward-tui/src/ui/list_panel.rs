use crate::app::{AppMode, AppState, ListEditField, Panel};
use crate::ui::theme;
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};
use ward_core::model::Item;

pub fn render(frame: &mut Frame, app: &AppState, area: ratatui::layout::Rect) {
    let is_active = app.active_panel == Panel::Lists;
    let editing = matches!(
        app.mode,
        AppMode::NewList | AppMode::EditList | AppMode::NewNote | AppMode::EditNote
    );

    let border_style = if is_active {
        theme::active_border()
    } else {
        theme::inactive_border()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" Lists & Notes ")
        .title_style(if is_active { theme::accent() } else { theme::dim() })
        .style(Style::default().bg(theme::BG));

    if editing {
        render_edit_form(frame, app, area, block);
        return;
    }

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let flat = app.flat_sidebar();

    let items: Vec<ListItem> = flat
        .iter()
        .enumerate()
        .map(|(flat_idx, fi)| {
            let item = match crate::app::item_at_path_pub(&app.store.items, &fi.path) {
                Some(i) => i,
                None => return ListItem::new(Line::from("")),
            };

            let indent = "  ".repeat(fi.depth);
            let is_selected = flat_idx == app.selected_item;

            let (type_prefix, suffix) = match item {
                Item::List(list) => {
                    let count = list.pending_count();
                    let suf = if count > 0 { format!(" ({})", count) } else { String::new() };
                    ("", suf)
                }
                Item::Note(_) => ("", String::new()),
                Item::Folder(f) => {
                    let arrow = if f.collapsed { "▶ " } else { "▼ " };
                    (arrow, String::new())
                }
            };

            let sel_arrow = if is_selected && is_active { "▶ " } else { "  " };

            let style = if is_selected {
                if is_active { theme::selected() }
                else { Style::default().fg(theme::TEXT).add_modifier(Modifier::BOLD) }
            } else if item.is_folder() {
                theme::accent()
            } else {
                theme::base()
            };

            let line = Line::from(vec![
                Span::raw(format!("{}{}", indent, sel_arrow)),
                Span::styled(type_prefix, style),
                Span::styled(item.display_name(), style),
                Span::styled(suffix, theme::dim()),
            ]);
            ListItem::new(line)
        })
        .collect();

    let mut list_state = ListState::default();
    if !flat.is_empty() {
        list_state.select(Some(app.selected_item.min(flat.len().saturating_sub(1))));
    }

    frame.render_stateful_widget(
        List::new(items).style(theme::base()),
        inner,
        &mut list_state,
    );
}

fn render_edit_form(
    frame: &mut Frame,
    app: &AppState,
    area: ratatui::layout::Rect,
    block: Block,
) {
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(le) = &app.list_edit else { return };

    let type_label = if le.is_note { "New Note" } else { "New List" };
    let name_label = if le.is_note { "Title:" } else { "Name:" };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // type indicator
            Constraint::Length(1), // spacer
            Constraint::Length(1), // name label
            Constraint::Length(3), // name input
            Constraint::Length(1), // icon label
            Constraint::Length(3), // icon input
            Constraint::Min(0),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(type_label).style(theme::accent()),
        chunks[0],
    );

    let name_active = le.focused_field == ListEditField::Name;
    let icon_active = le.focused_field == ListEditField::Icon;

    frame.render_widget(
        Paragraph::new(name_label)
            .style(if name_active { theme::accent() } else { theme::dim() }),
        chunks[2],
    );
    let name_block = Block::default()
        .borders(Borders::ALL)
        .border_style(if name_active {
            theme::active_border()
        } else {
            theme::inactive_border()
        })
        .style(Style::default().bg(theme::SURFACE));
    let name_inner = name_block.inner(chunks[3]);
    frame.render_widget(name_block, chunks[3]);
    frame.render_widget(
        Paragraph::new(le.name.value()).style(theme::base()),
        name_inner,
    );
    if name_active {
        frame.set_cursor_position((
            name_inner.x + le.name.visual_cursor() as u16,
            name_inner.y,
        ));
    }

    frame.render_widget(
        Paragraph::new("Icon (emoji):").style(if icon_active {
            theme::accent()
        } else {
            theme::dim()
        }),
        chunks[4],
    );
    let icon_block = Block::default()
        .borders(Borders::ALL)
        .border_style(if icon_active {
            theme::active_border()
        } else {
            theme::inactive_border()
        })
        .style(Style::default().bg(theme::SURFACE));
    let icon_inner = icon_block.inner(chunks[5]);
    frame.render_widget(icon_block, chunks[5]);
    frame.render_widget(
        Paragraph::new(le.icon.value()).style(theme::base()),
        icon_inner,
    );
    if icon_active {
        frame.set_cursor_position((
            icon_inner.x + le.icon.visual_cursor() as u16,
            icon_inner.y,
        ));
    }
}
