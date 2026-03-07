mod app;
mod ui;

use anyhow::Result;
use app::{AppMode, AppState, EditField, Panel};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use ward_core::model::Recurrence;
use std::{
    io,
    time::{Duration, Instant},
};

enum Action {
    Quit,
    Continue,
    OpenEditorNote,
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // Sub-commands that don't open the TUI
    match args.get(1).map(|s| s.as_str()) {
        Some("daemon") => return run_daemon(),
        Some("import") => {
            let path = args.get(2).ok_or_else(|| anyhow::anyhow!("Usage: rmdr import <file.md>"))?;
            return run_import(path);
        }
        _ => {}
    }

    // Resolve which directory to open
    let dir = resolve_open_dir(args.get(1).map(|s| s.as_str()))?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let result = run_app(&mut terminal, &dir);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn resolve_open_dir(arg: Option<&str>) -> Result<std::path::PathBuf> {
    use ward_core::paths::load_last_dir;
    use std::path::PathBuf;

    if let Some(path_str) = arg {
        let p = PathBuf::from(path_str);
        if p.is_dir() {
            return Ok(p);
        }
        return Err(anyhow::anyhow!("\"{}\" is not a directory", path_str));
    }

    // No argument: use last opened dir or default ~/rmdr
    if let Some(last) = load_last_dir() {
        return Ok(last);
    }

    let default = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rmdr");
    std::fs::create_dir_all(&default)?;
    Ok(default)
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, dir: &std::path::Path) -> Result<()> {
    let mut app = AppState::open(dir)?;
    let mut last_tick = Instant::now();
    let tick_rate = Duration::from_millis(500);

    loop {
        terminal.draw(|f| ui::render(f, &app))?;

        let timeout = tick_rate.checked_sub(last_tick.elapsed()).unwrap_or_default();

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match handle_key(&mut app, key)? {
                    Action::Quit => break,
                    Action::Continue => {}
                    Action::OpenEditorNote => {
                        let content = app.current_note().map(|n| n.content.clone()).unwrap_or_default();
                        let new_content = open_editor(&content, terminal)?;
                        if let Some(note) = app.current_note_mut() {
                            note.content = new_content;
                            note.touch();
                        }
                        app.reset_note_scroll();
                        app.save()?;
                    }
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            app.check_due_notifications()?;
            app.tick();
            last_tick = Instant::now();
        }
    }
    Ok(())
}

fn open_editor(
    content: &str,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<String> {
    let path = std::env::temp_dir().join("rmdr_note.md");
    std::fs::write(&path, content)?;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());
    std::process::Command::new(&editor).arg(&path).status()?;

    enable_raw_mode()?;
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    terminal.clear()?;

    Ok(std::fs::read_to_string(&path)?.trim_end().to_string())
}

fn handle_key(app: &mut AppState, key: KeyEvent) -> Result<Action> {
    match app.mode {
        AppMode::Browse => handle_browse(app, key),
        AppMode::NewReminder | AppMode::EditReminder => handle_edit_reminder(app, key),
        AppMode::NewList | AppMode::EditList | AppMode::NewNote | AppMode::EditNote => {
            handle_edit_list(app, key)
        }
        AppMode::ConfirmDelete => handle_confirm_delete(app, key),
        AppMode::Search => handle_search(app, key),
        AppMode::MoveReminder => handle_move_reminder(app, key),
        AppMode::Help => {
            app.mode = AppMode::Browse;
            Ok(Action::Continue)
        }
    }
}

fn handle_browse(app: &mut AppState, key: KeyEvent) -> Result<Action> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);

    match key.code {
        // Quit
        KeyCode::Char('q') | KeyCode::Char('Q') => return Ok(Action::Quit),
        KeyCode::Char('c') if ctrl => return Ok(Action::Quit),

        // Help
        KeyCode::Char('?') => app.mode = AppMode::Help,

        // Search
        KeyCode::Char('/') => app.begin_search(),

        // Clear search
        KeyCode::Esc => {}

        // Panel switching
        KeyCode::Tab => {
            app.active_panel = match app.active_panel {
                Panel::Lists => {
                    if app.current_note().is_some() { Panel::Detail } else { Panel::Reminders }
                }
                Panel::Reminders => Panel::Detail,
                Panel::Detail => Panel::Lists,
            };
        }
        KeyCode::BackTab => {
            app.active_panel = match app.active_panel {
                Panel::Lists => Panel::Detail,
                Panel::Reminders => Panel::Lists,
                Panel::Detail => {
                    if app.current_note().is_some() { Panel::Lists } else { Panel::Reminders }
                }
            };
        }

        // Reorder sidebar items (must be before plain Up/Down)
        KeyCode::Up if shift => {
            if app.active_panel == Panel::Lists { app.move_item_up()?; }
        }
        KeyCode::Down if shift => {
            if app.active_panel == Panel::Lists { app.move_item_down()?; }
        }

        // Navigation — with note scroll when detail is focused
        KeyCode::Up | KeyCode::Char('k') => match app.active_panel {
            Panel::Lists => {
                if app.selected_item > 0 {
                    app.selected_item -= 1;
                    app.selected_reminder = 0;
                    app.reset_note_scroll();
                }
            }
            Panel::Reminders => {
                if app.selected_reminder > 0 { app.selected_reminder -= 1; }
            }
            Panel::Detail => {
                if app.current_note().is_some() {
                    app.scroll_note_up();
                } else if app.selected_reminder > 0 {
                    app.selected_reminder -= 1;
                }
            }
        },
        KeyCode::Down | KeyCode::Char('j') => match app.active_panel {
            Panel::Lists => {
                if app.selected_item + 1 < app.flat_sidebar_len() {
                    app.selected_item += 1;
                    app.selected_reminder = 0;
                    app.reset_note_scroll();
                }
            }
            Panel::Reminders => {
                let max = app.visible_reminders().len().saturating_sub(1);
                if app.selected_reminder < max { app.selected_reminder += 1; }
            }
            Panel::Detail => {
                if app.current_note().is_some() {
                    app.scroll_note_down();
                } else {
                    let max = app.visible_reminders().len().saturating_sub(1);
                    if app.selected_reminder < max { app.selected_reminder += 1; }
                }
            }
        },

        // Enter / → : expand/collapse folder, or navigate into list
        KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
            if app.active_panel == Panel::Lists {
                use ward_core::model::Item;
                let is_folder = matches!(app.current_item(), Some(Item::Folder(_)));
                let is_list = matches!(app.current_item(), Some(Item::List(_)));
                if is_folder {
                    app.toggle_folder_collapsed();
                } else if is_list {
                    app.active_panel = Panel::Reminders;
                }
            }
        }

        // Left: collapse open folder
        KeyCode::Left if app.active_panel == Panel::Lists => {
            use ward_core::model::Item;
            let is_open_folder = matches!(app.current_item(), Some(Item::Folder(f)) if !f.collapsed);
            if is_open_folder {
                app.toggle_folder_collapsed();
            }
        }

        // New
        KeyCode::Char('n') => match app.active_panel {
            Panel::Lists => app.begin_new_list(),
            Panel::Reminders | Panel::Detail => {
                if app.current_list().is_some() { app.begin_new_reminder(); }
            }
        },
        KeyCode::Char('N') => {
            if app.active_panel == Panel::Lists { app.begin_new_note(); }
        }
        KeyCode::Char('f') => {
            if app.active_panel == Panel::Lists { app.begin_new_folder(); }
        }

        // Edit: open note in $EDITOR, rename list, or edit reminder
        KeyCode::Char('e') => match app.active_panel {
            Panel::Lists => {
                if app.current_note().is_some() {
                    return Ok(Action::OpenEditorNote);
                } else {
                    app.begin_edit_item_metadata();
                }
            }
            Panel::Reminders | Panel::Detail => {
                if app.current_note().is_some() {
                    return Ok(Action::OpenEditorNote);
                } else {
                    app.begin_edit_reminder();
                }
            }
        },

        // Delete
        KeyCode::Char('d') | KeyCode::Delete => match app.active_panel {
            Panel::Lists => app.begin_delete_item(),
            Panel::Reminders | Panel::Detail => app.begin_delete_reminder(),
        },

        // Toggle done
        KeyCode::Char(' ') => {
            if matches!(app.active_panel, Panel::Reminders | Panel::Detail) {
                app.toggle_done()?;
            }
        }

        // Toggle show completed
        KeyCode::Char('h') => {
            app.show_done = !app.show_done;
            app.clamp_selection();
        }

        // Sort
        KeyCode::Char('s') => {
            if matches!(app.active_panel, Panel::Reminders | Panel::Detail) {
                app.cycle_sort();
            }
        }

        // Undo
        KeyCode::Char('u') => app.undo()?,

        // Move reminder to another list
        KeyCode::Char('m') => {
            if matches!(app.active_panel, Panel::Reminders | Panel::Detail) {
                app.begin_move_reminder();
            }
        }

        // Export current note / list
        KeyCode::Char('x') => app.export_current()?,

        _ => {}
    }
    Ok(Action::Continue)
}

fn handle_edit_reminder(app: &mut AppState, key: KeyEvent) -> Result<Action> {
    let Some(_) = &app.edit else { return Ok(Action::Continue); };

    match key.code {
        KeyCode::Esc => {
            app.edit = None;
            app.mode = AppMode::Browse;
            app.active_panel = Panel::Reminders;
        }
        KeyCode::Enter => { app.commit_reminder_edit()?; }
        KeyCode::Tab => {
            let es = app.edit.as_mut().unwrap();
            es.focused_field = match es.focused_field {
                EditField::Title => EditField::DueAt,
                EditField::DueAt => EditField::Priority,
                EditField::Priority => EditField::Tags,
                EditField::Tags => EditField::Recurrence,
                EditField::Recurrence => EditField::Title,
            };
        }
        KeyCode::BackTab => {
            let es = app.edit.as_mut().unwrap();
            es.focused_field = match es.focused_field {
                EditField::Title => EditField::Recurrence,
                EditField::DueAt => EditField::Title,
                EditField::Priority => EditField::DueAt,
                EditField::Tags => EditField::Priority,
                EditField::Recurrence => EditField::Tags,
            };
        }
        KeyCode::Left | KeyCode::Right => {
            let es = app.edit.as_mut().unwrap();
            match es.focused_field {
                EditField::Priority => { es.priority = es.priority.next(); }
                EditField::Recurrence => { es.recurrence = cycle_recurrence(&es.recurrence); }
                _ => dispatch_input(es, key),
            }
        }
        _ => {
            let es = app.edit.as_mut().unwrap();
            dispatch_input(es, key);
        }
    }
    Ok(Action::Continue)
}

fn dispatch_input(es: &mut app::EditState, key: KeyEvent) {
    use tui_input::backend::crossterm::EventHandler;
    match es.focused_field {
        EditField::Title => { es.title.handle_event(&Event::Key(key)); }
        EditField::DueAt => { es.due_input.handle_event(&Event::Key(key)); }
        EditField::Tags => { es.tags_input.handle_event(&Event::Key(key)); }
        EditField::Priority | EditField::Recurrence => {}
    }
}

fn cycle_recurrence(current: &Option<Recurrence>) -> Option<Recurrence> {
    match current {
        None => Some(Recurrence::Daily),
        Some(r) => r.next_variant(),
    }
}

fn handle_edit_list(app: &mut AppState, key: KeyEvent) -> Result<Action> {
    let Some(_) = &app.list_edit else { return Ok(Action::Continue); };

    match key.code {
        KeyCode::Esc => { app.list_edit = None; app.mode = AppMode::Browse; }
        KeyCode::Enter => { app.commit_list_edit()?; }
        KeyCode::Tab | KeyCode::Down => {
            use app::ListEditField;
            let le = app.list_edit.as_mut().unwrap();
            le.focused_field = match le.focused_field {
                ListEditField::Name => ListEditField::Icon,
                ListEditField::Icon => ListEditField::Name,
            };
        }
        KeyCode::BackTab | KeyCode::Up => {
            use app::ListEditField;
            let le = app.list_edit.as_mut().unwrap();
            le.focused_field = match le.focused_field {
                ListEditField::Name => ListEditField::Icon,
                ListEditField::Icon => ListEditField::Name,
            };
        }
        _ => {
            use app::ListEditField;
            use tui_input::backend::crossterm::EventHandler;
            let le = app.list_edit.as_mut().unwrap();
            match le.focused_field {
                ListEditField::Name => le.name.handle_event(&Event::Key(key)),
                ListEditField::Icon => le.icon.handle_event(&Event::Key(key)),
            };
        }
    }
    Ok(Action::Continue)
}

fn handle_confirm_delete(app: &mut AppState, key: KeyEvent) -> Result<Action> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => { app.confirm_delete()?; }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => { app.cancel_delete(); }
        _ => {}
    }
    Ok(Action::Continue)
}

fn handle_search(app: &mut AppState, key: KeyEvent) -> Result<Action> {
    use tui_input::backend::crossterm::EventHandler;
    match key.code {
        KeyCode::Enter => { app.jump_to_search_result(); }
        KeyCode::Esc => { app.cancel_search(); }
        KeyCode::Up | KeyCode::Char('k') => {
            if app.search_cursor > 0 { app.search_cursor -= 1; }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let max = app.search_results.len().saturating_sub(1);
            if app.search_cursor < max { app.search_cursor += 1; }
        }
        _ => {
            app.search_input.handle_event(&Event::Key(key));
            app.update_search_results();
        }
    }
    Ok(Action::Continue)
}

fn handle_move_reminder(app: &mut AppState, key: KeyEvent) -> Result<Action> {
    match key.code {
        KeyCode::Esc => { app.cancel_move(); }
        KeyCode::Enter => { app.commit_move_reminder()?; }
        KeyCode::Up | KeyCode::Char('k') => {
            if app.move_list_cursor > 0 { app.move_list_cursor -= 1; }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.move_list_cursor + 1 < app.flat_sidebar_len() {
                app.move_list_cursor += 1;
            }
        }
        _ => {}
    }
    Ok(Action::Continue)
}

fn run_import(path: &str) -> Result<()> {
    use app::{parse_due, unique_file_path};
    use ward_core::{
        model::{Reminder, ReminderList, Subtask},
        paths::load_last_dir,
        store::save_item_to_disk,
    };
    use ward_core::model::Item;

    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Cannot read \"{}\": {}", path, e))?;

    let filename = std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Imported")
        .to_string();

    let mut list_name = filename.clone();
    let mut reminders: Vec<Reminder> = Vec::new();

    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("# ") {
            list_name = rest.trim().to_string();
            continue;
        }
        if let Some(rest) = line.strip_prefix("- [ ] ").or_else(|| line.strip_prefix("* [ ] ")) {
            reminders.push(Reminder::new(rest.trim()));
            continue;
        }
        if let Some(rest) = line
            .strip_prefix("- [x] ").or_else(|| line.strip_prefix("- [X] "))
            .or_else(|| line.strip_prefix("* [x] ")).or_else(|| line.strip_prefix("* [X] "))
        {
            let mut r = Reminder::new(rest.trim());
            r.done = true; r.notified = true;
            reminders.push(r);
            continue;
        }
        if line.starts_with("  ") {
            let t = line.trim();
            if let Some(due_str) = t.strip_prefix("- Due: ").or_else(|| t.strip_prefix("* Due: ")) {
                if let Some(last) = reminders.last_mut() { last.due_at = parse_due(due_str.trim()); }
            } else if let Some(sub) = t.strip_prefix("- [ ] ").or_else(|| t.strip_prefix("* [ ] ")) {
                if let Some(last) = reminders.last_mut() { last.subtasks.push(Subtask::new(sub.trim())); }
            } else if let Some(sub) = t
                .strip_prefix("- [x] ").or_else(|| t.strip_prefix("- [X] "))
                .or_else(|| t.strip_prefix("* [x] ")).or_else(|| t.strip_prefix("* [X] "))
            {
                if let Some(last) = reminders.last_mut() {
                    let mut s = Subtask::new(sub.trim()); s.done = true; last.subtasks.push(s);
                }
            }
        }
    }

    let target_dir = load_last_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")));
    let file_path = unique_file_path(&target_dir, &list_name, "json");
    let count = reminders.len();
    let mut list = ReminderList::new(&list_name, None);
    list.reminders = reminders;
    list.file_path = Some(file_path);
    save_item_to_disk(&Item::List(list))?;

    println!("Imported {} reminder(s) into \"{}\".", count, target_dir.display());
    Ok(())
}

fn run_daemon() -> Result<()> {
    use ward_core::{notify, paths::load_last_dir, store::{load_dir, save_item_to_disk}};
    use ward_core::model::Item;

    loop {
        if let Some(dir) = load_last_dir() {
            if let Ok(mut s) = load_dir(&dir) {
                let now = chrono::Utc::now();
                for item in &mut s.items {
                    check_item_due(item, now, &notify::fire);
                }
                // Write back any changed lists
                for item in &s.items {
                    save_item_to_disk(item).ok();
                }
            }
        }
        std::thread::sleep(Duration::from_secs(60));
    }
}

fn check_item_due(
    item: &mut ward_core::model::Item,
    now: chrono::DateTime<chrono::Utc>,
    fire: &dyn Fn(&str, &str) -> Result<()>,
) {
    use ward_core::model::Item;
    match item {
        Item::List(list) => {
            for r in &mut list.reminders {
                if r.done || r.notified { continue; }
                if let Some(due) = r.due_at {
                    if due <= now {
                        let body = r.notes.as_deref().unwrap_or("Due now.");
                        fire(&r.title, body).ok();
                        if !r.advance_recurrence() { r.notified = true; }
                    }
                }
            }
        }
        Item::Folder(f) => {
            for child in &mut f.children {
                check_item_due(child, now, fire);
            }
        }
        Item::Note(_) => {}
    }
}
