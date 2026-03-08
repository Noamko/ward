mod app;
mod ui;

use anyhow::Result;
use app::{AppMode, AppState, EditField, Panel, parse_due, parse_tags};
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
            let path = args.get(2).ok_or_else(|| anyhow::anyhow!("Usage: ward import <file.md>"))?;
            return run_import(path);
        }
        Some("ls") => return run_ls(&args[2..]),
        Some("add") => return run_add(&args[2..]),
        Some("done") => return run_done_cmd(&args[2..]),
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

    // No argument: use last opened dir or default ~/ward
    if let Some(last) = load_last_dir() {
        return Ok(last);
    }

    let default = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ward");
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
    let path = std::env::temp_dir().join("ward_note.md");
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
    // Ctrl+P opens the command palette from any mode
    if key.code == KeyCode::Char('p') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.open_command_palette();
        return Ok(Action::Continue);
    }

    match app.mode {
        AppMode::Browse => handle_browse(app, key),
        AppMode::NewReminder | AppMode::EditReminder => handle_edit_reminder(app, key),
        AppMode::NewList | AppMode::EditList | AppMode::NewNote | AppMode::EditNote => {
            handle_edit_list(app, key)
        }
        AppMode::ConfirmDelete => handle_confirm_delete(app, key),
        AppMode::Search => handle_search(app, key),
        AppMode::MoveReminder => handle_move_reminder(app, key),
        AppMode::BulkSelect => handle_bulk(app, key),
        AppMode::MoveItem => handle_move_item(app, key),
        AppMode::CommandPalette => handle_command_palette(app, key),
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

        // Bulk select reminders
        KeyCode::Char('V') => {
            if matches!(app.active_panel, Panel::Reminders | Panel::Detail) {
                app.begin_bulk_select();
            }
        }

        // Move sidebar item to a group/folder
        KeyCode::Char('g') => {
            if app.active_panel == Panel::Lists {
                app.begin_move_item();
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
                EditField::Recurrence => EditField::NotifyBefore,
                EditField::NotifyBefore => EditField::Title,
            };
        }
        KeyCode::BackTab => {
            let es = app.edit.as_mut().unwrap();
            es.focused_field = match es.focused_field {
                EditField::Title => EditField::NotifyBefore,
                EditField::DueAt => EditField::Title,
                EditField::Priority => EditField::DueAt,
                EditField::Tags => EditField::Priority,
                EditField::Recurrence => EditField::Tags,
                EditField::NotifyBefore => EditField::Recurrence,
            };
        }
        KeyCode::Left | KeyCode::Right => {
            let es = app.edit.as_mut().unwrap();
            let forward = key.code == KeyCode::Right;
            match es.focused_field {
                EditField::Priority => { es.priority = es.priority.next(); }
                EditField::Recurrence => { es.recurrence = cycle_recurrence(&es.recurrence, forward); }
                EditField::NotifyBefore => {
                    es.notify_before_mins = app::cycle_notify_before(es.notify_before_mins, forward);
                }
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
        EditField::Priority | EditField::Recurrence | EditField::NotifyBefore => {}
    }
}

fn cycle_recurrence(current: &Option<Recurrence>, forward: bool) -> Option<Recurrence> {
    if forward {
        match current {
            None => Some(Recurrence::Daily),
            Some(r) => r.next_variant(),
        }
    } else {
        match current {
            None => Some(Recurrence::Yearly),
            Some(Recurrence::Daily) => None,
            Some(Recurrence::Weekly) => Some(Recurrence::Daily),
            Some(Recurrence::Monthly) => Some(Recurrence::Weekly),
            Some(Recurrence::Yearly) => Some(Recurrence::Monthly),
            Some(Recurrence::EveryNDays(_)) => None,
        }
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

fn handle_bulk(app: &mut AppState, key: KeyEvent) -> Result<Action> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => { app.cancel_bulk(); }
        KeyCode::Up | KeyCode::Char('k') => {
            if app.selected_reminder > 0 { app.selected_reminder -= 1; }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let max = app.visible_reminders().len().saturating_sub(1);
            if app.selected_reminder < max { app.selected_reminder += 1; }
        }
        KeyCode::Char(' ') => { app.bulk_toggle_current(); }
        KeyCode::Enter => { app.bulk_mark_done()?; }
        KeyCode::Char('d') | KeyCode::Delete => { app.begin_bulk_delete(); }
        KeyCode::Char('m') => { app.begin_bulk_move(); }
        _ => {}
    }
    Ok(Action::Continue)
}

fn handle_move_item(app: &mut AppState, key: KeyEvent) -> Result<Action> {
    let folder_count = app.flat_folders().len();
    match key.code {
        KeyCode::Esc => { app.cancel_move_item(); }
        KeyCode::Enter => { app.commit_move_item()?; }
        KeyCode::Up | KeyCode::Char('k') => {
            if app.move_list_cursor > 0 { app.move_list_cursor -= 1; }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            // 0 = root, 1..=N = folders
            if app.move_list_cursor <= folder_count { app.move_list_cursor += 1; }
        }
        _ => {}
    }
    Ok(Action::Continue)
}

fn handle_command_palette(app: &mut AppState, key: KeyEvent) -> Result<Action> {
    use tui_input::backend::crossterm::EventHandler;
    match key.code {
        KeyCode::Esc => { app.cancel_command_palette(); }
        KeyCode::Enter => { app.execute_command()?; }
        KeyCode::Up | KeyCode::Char('k') => {
            if app.command_cursor > 0 { app.command_cursor -= 1; }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let max = app.filtered_commands().len().saturating_sub(1);
            if app.command_cursor < max { app.command_cursor += 1; }
        }
        _ => {
            app.command_input.handle_event(&Event::Key(key));
            app.command_cursor = 0;
        }
    }
    Ok(Action::Continue)
}

// ── CLI helpers ───────────────────────────────────────────────────────────────

fn cli_load_dir() -> std::path::PathBuf {
    use ward_core::paths::load_last_dir;
    load_last_dir().unwrap_or_else(|| {
        dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from(".")).join("ward")
    })
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())
}

fn cli_find_list_id(items: &[ward_core::model::Item], name: Option<&str>) -> Option<String> {
    use ward_core::model::Item;
    for item in items {
        match item {
            Item::List(l) => {
                let matches = name.map(|n| l.name.to_lowercase().contains(&n.to_lowercase())).unwrap_or(true);
                if matches { return Some(l.id.clone()); }
            }
            Item::Folder(f) => {
                if let Some(id) = cli_find_list_id(&f.children, name) { return Some(id); }
            }
            Item::Note(_) => {}
        }
    }
    None
}

fn cli_find_list_mut<'a>(items: &'a mut Vec<ward_core::model::Item>, id: &str) -> Option<&'a mut ward_core::model::ReminderList> {
    use ward_core::model::Item;
    for item in items.iter_mut() {
        match item {
            Item::List(l) if l.id == id => return Some(l),
            Item::Folder(f) => {
                if let Some(found) = cli_find_list_mut(&mut f.children, id) { return Some(found); }
            }
            _ => {}
        }
    }
    None
}

fn run_ls(args: &[String]) -> Result<()> {
    use ward_core::store::load_dir;
    use ward_core::model::Item;
    use chrono::Local;

    let dir = cli_load_dir();
    let store = load_dir(&dir)?;

    let filter_list = flag_value(args, "--list");
    let only_today   = args.iter().any(|a| a == "--today");
    let only_overdue = args.iter().any(|a| a == "--overdue");
    let only_pending = args.iter().any(|a| a == "--pending");

    fn print_item(item: &Item, filter: Option<&str>, today: bool, overdue: bool, pending: bool, found: &mut bool) {
        use ward_core::model::Item;
        match item {
            Item::List(list) => {
                if let Some(name) = filter {
                    if !list.name.to_lowercase().contains(&name.to_lowercase()) { return; }
                }
                for r in &list.reminders {
                    if pending && r.done { continue; }
                    if today && !r.is_due_today() { continue; }
                    if overdue && !r.is_overdue() { continue; }
                    *found = true;
                    let check = if r.done { "✓" } else { "○" };
                    let due = r.due_at.map(|d| {
                        format!("  due {}", d.with_timezone(&Local).format("%d/%m/%y %H:%M"))
                    }).unwrap_or_default();
                    let warn = if r.is_overdue() { "  ⚠" } else { "" };
                    println!("[{}] {}  [{}]{}{}", check, r.title, list.name, due, warn);
                }
            }
            Item::Folder(f) => {
                for child in &f.children { print_item(child, filter, today, overdue, pending, found); }
            }
            Item::Note(_) => {}
        }
    }

    let mut found = false;
    for item in &store.items {
        print_item(item, filter_list.as_deref(), only_today, only_overdue, only_pending, &mut found);
    }
    if !found { println!("No reminders found."); }
    Ok(())
}

fn run_add(args: &[String]) -> Result<()> {
    use ward_core::{store::save_all_to_disk, model::{Reminder, Priority}};

    if args.is_empty() || args[0].starts_with("--") {
        eprintln!("Usage: ward add <title> [--list <name>] [--due <date>] [--priority low|medium|high] [--tags tag1,tag2]");
        return Ok(());
    }

    let title = args[0].clone();
    let list_name  = flag_value(args, "--list");
    let due_str    = flag_value(args, "--due");
    let prio_str   = flag_value(args, "--priority");
    let tags_str   = flag_value(args, "--tags");

    let due_at   = due_str.as_deref().and_then(|s| parse_due(s));
    let tags     = tags_str.as_deref().map(|s| parse_tags(s)).unwrap_or_default();
    let priority = match prio_str.as_deref() {
        Some("high") => Priority::High,
        Some("low")  => Priority::Low,
        _            => Priority::Medium,
    };

    let dir = cli_load_dir();
    let mut store = ward_core::store::load_dir(&dir)?;

    let list_id = cli_find_list_id(&store.items, list_name.as_deref());
    let Some(list_id) = list_id else {
        let name = list_name.as_deref().unwrap_or("(any)");
        eprintln!("No list matching \"{}\" found. Open ward to create one.", name);
        return Ok(());
    };

    let list_name_display = cli_find_list_mut(&mut store.items, &list_id)
        .map(|l| l.name.clone()).unwrap_or_default();

    if let Some(list) = cli_find_list_mut(&mut store.items, &list_id) {
        let mut r = Reminder::new(&title);
        r.due_at   = due_at;
        r.priority = priority;
        r.tags     = tags;
        list.reminders.push(r);
    }
    save_all_to_disk(&store.items)?;
    println!("Added \"{}\" to \"{}\".", title, list_name_display);
    Ok(())
}

fn run_done_cmd(args: &[String]) -> Result<()> {
    use ward_core::{store::{load_dir, save_all_to_disk}, model::Item};
    use chrono::Utc;

    if args.is_empty() {
        eprintln!("Usage: ward done <title-substring>");
        return Ok(());
    }
    let query = args[0].to_lowercase();
    let dir = cli_load_dir();
    let mut store = load_dir(&dir)?;

    fn mark_done_in(items: &mut Vec<Item>, q: &str, count: &mut usize) {
        for item in items.iter_mut() {
            match item {
                Item::List(l) => {
                    for r in l.reminders.iter_mut() {
                        if !r.done && r.title.to_lowercase().contains(q) {
                            r.done = true;
                            r.notified = true;
                            r.updated_at = Utc::now();
                            *count += 1;
                            println!("  ✓  {} [{}]", r.title, l.name);
                        }
                    }
                }
                Item::Folder(f) => mark_done_in(&mut f.children, q, count),
                Item::Note(_)   => {}
            }
        }
    }

    let mut count = 0;
    mark_done_in(&mut store.items, &query, &mut count);
    if count == 0 {
        println!("No pending reminders matching \"{}\".", args[0]);
    } else {
        save_all_to_disk(&store.items)?;
        println!("{} reminder{} marked done.", count, if count == 1 { "" } else { "s" });
    }
    Ok(())
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
                if let Some(notify_at) = r.notify_at() {
                    if notify_at <= now {
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
