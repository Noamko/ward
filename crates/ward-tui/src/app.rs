use anyhow::Result;
use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use ward_core::{
    model::{Folder, Item, Note, Priority, Recurrence, Reminder, ReminderList, Store, Subtask},
    notify,
    paths::save_last_dir,
    store::{load_dir, save_all_to_disk, save_item_to_disk},
};
use std::path::{Path, PathBuf};
use tui_input::Input;

// ── Panels ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Lists,
    Reminders,
    Detail,
}

// ── Modes ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Browse,
    EditReminder,
    NewReminder,
    EditList,
    NewList,
    EditNote,
    NewNote,
    ConfirmDelete,
    Help,
    Search,
    MoveReminder,
}

// ── Sort ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortMode {
    #[default]
    Default,
    DueDate,
    Priority,
    Title,
    CreatedAt,
}

impl SortMode {
    pub fn label(self) -> &'static str {
        match self {
            SortMode::Default => "Default",
            SortMode::DueDate => "Due Date",
            SortMode::Priority => "Priority",
            SortMode::Title => "Title",
            SortMode::CreatedAt => "Created",
        }
    }

    pub fn next(self) -> Self {
        match self {
            SortMode::Default => SortMode::DueDate,
            SortMode::DueDate => SortMode::Priority,
            SortMode::Priority => SortMode::Title,
            SortMode::Title => SortMode::CreatedAt,
            SortMode::CreatedAt => SortMode::Default,
        }
    }
}

// ── Undo ──────────────────────────────────────────────────────────────────────

pub enum UndoItem {
    Reminder {
        list_id: String,
        reminder: Reminder,
        index: usize,
    },
    SidebarItem {
        item: Item,
        path: Vec<usize>,
    },
}

// ── Search results ────────────────────────────────────────────────────────────

#[derive(Clone)]
pub enum SearchJump {
    Note { path: Vec<usize> },
    Reminder { path: Vec<usize>, reminder_id: String },
}

pub struct SearchResult {
    pub source_label: String,
    pub snippet: String,
    pub match_start: usize,
    pub match_end: usize,
    pub jump: SearchJump,
}

// ── Edit states ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditField {
    Title,
    DueAt,
    Priority,
    Tags,
    Recurrence,
}

pub struct EditState {
    pub title: Input,
    pub due_input: Input,
    pub priority: Priority,
    pub tags_input: Input,
    pub recurrence: Option<Recurrence>,
    #[allow(dead_code)]
    pub due_error: Option<String>,
    pub focused_field: EditField,
    pub editing_reminder_id: Option<String>,
    pub subtasks: Vec<Subtask>,
}

impl EditState {
    pub fn new() -> Self {
        Self {
            title: Input::default(),
            due_input: Input::default(),
            priority: Priority::Medium,
            tags_input: Input::default(),
            recurrence: None,
            due_error: None,
            focused_field: EditField::Title,
            editing_reminder_id: None,
            subtasks: vec![],
        }
    }

    pub fn from_reminder(r: &Reminder) -> Self {
        let due_str = r
            .due_at
            .map(|d| d.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_default();
        Self {
            title: Input::new(r.title.clone()),
            due_input: Input::new(due_str),
            priority: r.priority.clone(),
            tags_input: Input::new(r.tags.join(", ")),
            recurrence: r.recurrence.clone(),
            due_error: None,
            focused_field: EditField::Title,
            editing_reminder_id: Some(r.id.clone()),
            subtasks: r.subtasks.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListEditField {
    Name,
    Icon,
}

pub struct ListEditState {
    pub name: Input,
    pub icon: Input,
    pub editing_id: Option<String>,
    pub focused_field: ListEditField,
    pub is_note: bool,
    pub is_folder: bool,
}

impl ListEditState {
    pub fn new_list() -> Self {
        Self {
            name: Input::default(),
            icon: Input::default(),
            editing_id: None,
            focused_field: ListEditField::Name,
            is_note: false,
            is_folder: false,
        }
    }

    pub fn new_note() -> Self {
        Self { is_note: true, is_folder: false, ..Self::new_list() }
    }

    pub fn new_folder() -> Self {
        Self { is_note: false, is_folder: true, ..Self::new_list() }
    }

    pub fn from_list(l: &ReminderList) -> Self {
        Self {
            name: Input::new(l.name.clone()),
            icon: Input::new(l.icon.clone().unwrap_or_default()),
            editing_id: Some(l.id.clone()),
            focused_field: ListEditField::Name,
            is_note: false,
            is_folder: false,
        }
    }

    pub fn from_note(n: &Note) -> Self {
        Self {
            name: Input::new(n.title.clone()),
            icon: Input::new(n.icon.clone().unwrap_or_default()),
            editing_id: Some(n.id.clone()),
            focused_field: ListEditField::Name,
            is_note: true,
            is_folder: false,
        }
    }

    pub fn from_folder(f: &Folder) -> Self {
        Self {
            name: Input::new(f.name.clone()),
            icon: Input::new(f.icon.clone().unwrap_or_default()),
            editing_id: Some(f.id.clone()),
            focused_field: ListEditField::Name,
            is_note: false,
            is_folder: true,
        }
    }
}

// ── Flat sidebar view ─────────────────────────────────────────────────────────

/// One entry in the flattened visible sidebar list.
pub struct FlatItem {
    pub depth: usize,
    /// Path of indices to reach this item (e.g. [2, 1] = items[2].children[1]).
    pub path: Vec<usize>,
}

fn flatten_items(items: &[Item], prefix: &[usize], depth: usize, result: &mut Vec<FlatItem>) {
    for (i, item) in items.iter().enumerate() {
        let mut path = prefix.to_vec();
        path.push(i);
        result.push(FlatItem { depth, path: path.clone() });
        if let Item::Folder(f) = item {
            if !f.collapsed {
                flatten_items(&f.children, &path, depth + 1, result);
            }
        }
    }
}

/// Public re-export for use in UI rendering.
pub fn item_at_path_pub<'a>(items: &'a [Item], path: &[usize]) -> Option<&'a Item> {
    item_at_path(items, path)
}

fn item_at_path<'a>(items: &'a [Item], path: &[usize]) -> Option<&'a Item> {
    if path.is_empty() { return None; }
    let item = items.get(path[0])?;
    if path.len() == 1 { Some(item) } else {
        item.as_folder().and_then(|f| item_at_path(&f.children, &path[1..]))
    }
}

fn item_at_path_mut<'a>(items: &'a mut [Item], path: &[usize]) -> Option<&'a mut Item> {
    if path.is_empty() { return None; }
    let item = items.get_mut(path[0])?;
    if path.len() == 1 { Some(item) } else {
        item.as_folder_mut().and_then(|f| item_at_path_mut(&mut f.children, &path[1..]))
    }
}

fn children_at_path_mut<'a>(items: &'a mut Vec<Item>, path: &[usize]) -> Option<&'a mut Vec<Item>> {
    if path.is_empty() { return Some(items); }
    match items.get_mut(path[0])? {
        Item::Folder(f) => children_at_path_mut(&mut f.children, &path[1..]),
        _ => None,
    }
}

fn remove_at_path(items: &mut Vec<Item>, path: &[usize]) -> Option<Item> {
    if path.is_empty() { return None; }
    if path.len() == 1 {
        if path[0] < items.len() { Some(items.remove(path[0])) } else { None }
    } else {
        match items.get_mut(path[0])? {
            Item::Folder(f) => remove_at_path(&mut f.children, &path[1..]),
            _ => None,
        }
    }
}

fn insert_at_path(items: &mut Vec<Item>, path: &[usize], item: Item) {
    if path.is_empty() { return; }
    if path.len() == 1 {
        items.insert(path[0].min(items.len()), item);
    } else if let Some(Item::Folder(f)) = items.get_mut(path[0]) {
        insert_at_path(&mut f.children, &path[1..], item);
    }
}

fn find_item_mut_by_id<'a>(items: &'a mut Vec<Item>, id: &str) -> Option<&'a mut Item> {
    for item in items.iter_mut() {
        if item.id() == id { return Some(item); }
        if let Item::Folder(f) = item {
            if let Some(found) = find_item_mut_by_id(&mut f.children, id) {
                return Some(found);
            }
        }
    }
    None
}

fn visible_subtree_size(item: &Item) -> usize {
    1 + match item {
        Item::Folder(f) if !f.collapsed => f.children.iter().map(visible_subtree_size).sum(),
        _ => 0,
    }
}

fn search_items_recursive(
    items: &[Item],
    path_prefix: &[usize],
    q: &str,
    results: &mut Vec<SearchResult>,
) {
    for (i, item) in items.iter().enumerate() {
        let mut path = path_prefix.to_vec();
        path.push(i);
        match item {
            Item::Note(n) => {
                let title_lower = n.title.to_lowercase();
                if let Some(pos) = title_lower.find(q) {
                    results.push(SearchResult {
                        source_label: format!("Note: {}", n.display_name()),
                        snippet: n.title.clone(),
                        match_start: pos,
                        match_end: pos + q.len(),
                        jump: SearchJump::Note { path: path.clone() },
                    });
                }
                let mut content_hits = 0;
                for line in n.content.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() { continue; }
                    if let Some(pos) = trimmed.to_lowercase().find(q) {
                        results.push(SearchResult {
                            source_label: format!("Note: {}", n.display_name()),
                            snippet: trimmed.to_string(),
                            match_start: pos,
                            match_end: pos + q.len(),
                            jump: SearchJump::Note { path: path.clone() },
                        });
                        content_hits += 1;
                        if content_hits >= 5 { break; }
                    }
                }
            }
            Item::List(l) => {
                for r in &l.reminders {
                    if let Some(pos) = r.title.to_lowercase().find(q) {
                        results.push(SearchResult {
                            source_label: format!("{} > {}", l.display_name(), r.title),
                            snippet: r.title.clone(),
                            match_start: pos,
                            match_end: pos + q.len(),
                            jump: SearchJump::Reminder { path: path.clone(), reminder_id: r.id.clone() },
                        });
                    }
                    for tag in &r.tags {
                        if let Some(pos) = tag.to_lowercase().find(q) {
                            results.push(SearchResult {
                                source_label: format!("{} > {}", l.display_name(), r.title),
                                snippet: format!("#{}", tag),
                                match_start: pos + 1,
                                match_end: pos + 1 + q.len(),
                                jump: SearchJump::Reminder { path: path.clone(), reminder_id: r.id.clone() },
                            });
                        }
                    }
                    for s in &r.subtasks {
                        let prefix = "  > ";
                        if let Some(pos) = s.title.to_lowercase().find(q) {
                            results.push(SearchResult {
                                source_label: format!("{} > {}", l.display_name(), r.title),
                                snippet: format!("{}{}", prefix, s.title),
                                match_start: prefix.len() + pos,
                                match_end: prefix.len() + pos + q.len(),
                                jump: SearchJump::Reminder { path: path.clone(), reminder_id: r.id.clone() },
                            });
                        }
                    }
                }
            }
            Item::Folder(f) => {
                search_items_recursive(&f.children, &path, q, results);
            }
        }
    }
}

fn check_due_recursive(items: &mut [Item], now: DateTime<Utc>) -> bool {
    let mut changed = false;
    for item in items.iter_mut() {
        match item {
            Item::List(list) => {
                for reminder in &mut list.reminders {
                    if reminder.done || reminder.notified { continue; }
                    if let Some(due) = reminder.due_at {
                        if due <= now {
                            let body = reminder.notes.as_deref().unwrap_or("Due now.");
                            notify::fire(&reminder.title, body).ok();
                            if !reminder.advance_recurrence() {
                                reminder.notified = true;
                            }
                            changed = true;
                        }
                    }
                }
            }
            Item::Folder(f) => {
                changed |= check_due_recursive(&mut f.children, now);
            }
            Item::Note(_) => {}
        }
    }
    changed
}

// ── AppState ──────────────────────────────────────────────────────────────────

pub struct AppState {
    pub store: Store,
    pub opened_dir: PathBuf,
    pub mode: AppMode,
    pub active_panel: Panel,
    /// Index into the flattened sidebar view (respects folder expand/collapse).
    pub selected_item: usize,
    pub selected_reminder: usize,
    pub edit: Option<EditState>,
    pub list_edit: Option<ListEditState>,
    pub delete_label: String,
    pub show_done: bool,
    // Sort
    pub sort_mode: SortMode,
    // Search
    pub search_input: Input,
    pub search_results: Vec<SearchResult>,
    pub search_cursor: usize,
    // Note scrolling
    pub note_scroll: usize,
    // Undo
    pub undo_stack: Vec<UndoItem>,
    // Move reminder
    pub move_src_id: Option<String>,
    pub move_list_cursor: usize,
    // Status message (shown briefly in status bar)
    pub status_message: Option<String>,
    pub status_ticks: u8,
}

impl AppState {
    pub fn open(dir: &Path) -> Result<Self> {
        let dir = std::fs::canonicalize(dir)
            .unwrap_or_else(|_| dir.to_path_buf());
        let store = load_dir(&dir)?;
        save_last_dir(&dir).ok();
        Ok(Self {
            store,
            opened_dir: dir,
            mode: AppMode::Browse,
            active_panel: Panel::Lists,
            selected_item: 0,
            selected_reminder: 0,
            edit: None,
            list_edit: None,
            delete_label: String::new(),
            show_done: true,
            sort_mode: SortMode::default(),
            search_input: Input::default(),
            search_results: Vec::new(),
            search_cursor: 0,
            note_scroll: 0,
            undo_stack: Vec::new(),
            move_src_id: None,
            move_list_cursor: 0,
            status_message: None,
            status_ticks: 0,
        })
    }

    pub fn save(&self) -> Result<()> {
        save_all_to_disk(&self.store.items)
    }

    /// Save a single item (e.g. after editing).
    pub fn save_item_at(&self, flat_idx: usize) -> Result<()> {
        let path = self.flat_sidebar().get(flat_idx).map(|fi| fi.path.clone());
        if let Some(path) = path {
            if let Some(item) = item_at_path(&self.store.items, &path) {
                save_item_to_disk(item)?;
            }
        }
        Ok(())
    }

    /// Return the directory where new items should be created.
    pub fn current_dir(&self) -> PathBuf {
        let flat = self.flat_sidebar();
        if let Some(fi) = flat.get(self.selected_item) {
            if let Some(item) = item_at_path(&self.store.items, &fi.path) {
                match item {
                    Item::Folder(f) => {
                        if let Some(d) = &f.dir_path { return d.clone(); }
                    }
                    Item::Note(n) => {
                        if let Some(p) = &n.file_path {
                            if let Some(parent) = p.parent() { return parent.to_path_buf(); }
                        }
                    }
                    Item::List(l) => {
                        if let Some(p) = &l.file_path {
                            if let Some(parent) = p.parent() { return parent.to_path_buf(); }
                        }
                    }
                }
            }
        }
        self.opened_dir.clone()
    }

    /// Tick-driven: decrement status message counter and clear when expired.
    pub fn tick(&mut self) {
        if self.status_message.is_some() {
            if self.status_ticks == 0 {
                self.status_message = None;
            } else {
                self.status_ticks -= 1;
            }
        }
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_message = Some(msg.into());
        self.status_ticks = 6;
    }

    // ── Flat sidebar ──────────────────────────────────────────────────────────

    pub fn flat_sidebar(&self) -> Vec<FlatItem> {
        let mut result = Vec::new();
        flatten_items(&self.store.items, &[], 0, &mut result);
        result
    }

    pub fn flat_sidebar_len(&self) -> usize {
        self.flat_sidebar().len()
    }

    // ── Selectors ─────────────────────────────────────────────────────────────

    pub fn current_item(&self) -> Option<&Item> {
        let flat = self.flat_sidebar();
        let fi = flat.get(self.selected_item)?;
        item_at_path(&self.store.items, &fi.path)
    }

    pub fn current_item_mut(&mut self) -> Option<&mut Item> {
        let path = self.flat_sidebar().get(self.selected_item)?.path.clone();
        item_at_path_mut(&mut self.store.items, &path)
    }

    pub fn current_list(&self) -> Option<&ReminderList> {
        self.current_item()?.as_list()
    }

    pub fn current_list_mut(&mut self) -> Option<&mut ReminderList> {
        self.current_item_mut()?.as_list_mut()
    }

    pub fn current_note(&self) -> Option<&Note> {
        self.current_item()?.as_note()
    }

    pub fn current_note_mut(&mut self) -> Option<&mut Note> {
        self.current_item_mut()?.as_note_mut()
    }

    pub fn visible_reminders(&self) -> Vec<&Reminder> {
        let Some(list) = self.current_list() else { return vec![]; };
        let mut items: Vec<&Reminder> = list
            .reminders
            .iter()
            .filter(|r| self.show_done || !r.done)
            .collect();

        match self.sort_mode {
            SortMode::Default => {
                items.sort_by(|a, b| match (a.done, b.done) {
                    (false, true) => std::cmp::Ordering::Less,
                    (true, false) => std::cmp::Ordering::Greater,
                    _ => match (a.due_at, b.due_at) {
                        (Some(da), Some(db)) => da.cmp(&db),
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (None, None) => b.priority.cmp(&a.priority),
                    },
                });
            }
            SortMode::DueDate => {
                items.sort_by(|a, b| match (a.due_at, b.due_at) {
                    (Some(da), Some(db)) => da.cmp(&db),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
                });
            }
            SortMode::Priority => {
                items.sort_by(|a, b| b.priority.cmp(&a.priority));
            }
            SortMode::Title => {
                items.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
            }
            SortMode::CreatedAt => {
                items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            }
        }
        items
    }

    pub fn selected_reminder_ref(&self) -> Option<&Reminder> {
        self.visible_reminders().get(self.selected_reminder).copied()
    }

    pub fn clamp_selection(&mut self) {
        let flat_len = self.flat_sidebar_len();
        self.selected_item = if flat_len == 0 { 0 } else { self.selected_item.min(flat_len - 1) };
        let rem_len = self.visible_reminders().len();
        self.selected_reminder = if rem_len == 0 { 0 } else { self.selected_reminder.min(rem_len - 1) };
    }

    pub fn check_due_notifications(&mut self) -> Result<()> {
        let now = Utc::now();
        if check_due_recursive(&mut self.store.items, now) {
            self.save()?;
        }
        Ok(())
    }

    // ── Note scroll ───────────────────────────────────────────────────────────

    pub fn scroll_note_down(&mut self) {
        self.note_scroll = self.note_scroll.saturating_add(3);
    }

    pub fn scroll_note_up(&mut self) {
        self.note_scroll = self.note_scroll.saturating_sub(3);
    }

    pub fn reset_note_scroll(&mut self) {
        self.note_scroll = 0;
    }

    // ── Search ────────────────────────────────────────────────────────────────

    pub fn begin_search(&mut self) {
        self.search_input = Input::default();
        self.search_results.clear();
        self.search_cursor = 0;
        self.mode = AppMode::Search;
    }

    pub fn cancel_search(&mut self) {
        self.search_results.clear();
        self.search_cursor = 0;
        self.mode = AppMode::Browse;
    }

    pub fn update_search_results(&mut self) {
        let q = self.search_input.value().trim().to_lowercase();
        self.search_results.clear();
        self.search_cursor = 0;
        if q.is_empty() { return; }
        let mut results = Vec::new();
        search_items_recursive(&self.store.items, &[], &q, &mut results);
        self.search_results = results;
    }

    pub fn jump_to_search_result(&mut self) {
        if self.search_results.is_empty() {
            self.mode = AppMode::Browse;
            return;
        }
        let cursor = self.search_cursor.min(self.search_results.len() - 1);
        let jump = self.search_results[cursor].jump.clone();

        let target_path = match &jump {
            SearchJump::Note { path } => path.clone(),
            SearchJump::Reminder { path, .. } => path.clone(),
        };

        // Expand all ancestor folders so the item is visible
        self.expand_path_ancestors(&target_path);

        let flat = self.flat_sidebar();
        if let Some(flat_idx) = flat.iter().position(|fi| fi.path == target_path) {
            self.selected_item = flat_idx;
        }

        match jump {
            SearchJump::Note { .. } => {
                self.active_panel = Panel::Lists;
                self.reset_note_scroll();
            }
            SearchJump::Reminder { reminder_id, .. } => {
                let idx = self.visible_reminders().iter().position(|r| r.id == reminder_id).unwrap_or(0);
                self.selected_reminder = idx;
                self.active_panel = Panel::Reminders;
            }
        }

        self.search_results.clear();
        self.search_cursor = 0;
        self.mode = AppMode::Browse;
    }

    fn expand_path_ancestors(&mut self, path: &[usize]) {
        for len in 1..path.len() {
            let prefix = path[..len].to_vec();
            if let Some(item) = item_at_path_mut(&mut self.store.items, &prefix) {
                if let Item::Folder(f) = item {
                    f.collapsed = false;
                }
            }
        }
    }

    // ── Folder operations ─────────────────────────────────────────────────────

    pub fn toggle_folder_collapsed(&mut self) {
        let path = self.flat_sidebar().get(self.selected_item).map(|fi| fi.path.clone());
        if let Some(path) = path {
            if let Some(Item::Folder(f)) = item_at_path_mut(&mut self.store.items, &path) {
                f.collapsed = !f.collapsed;
            }
        }
    }

    // ── Sort ──────────────────────────────────────────────────────────────────

    pub fn cycle_sort(&mut self) {
        self.sort_mode = self.sort_mode.next();
        self.clamp_selection();
        self.set_status(format!("Sort: {}", self.sort_mode.label()));
    }

    // ── Sidebar list operations ───────────────────────────────────────────────

    pub fn begin_new_list(&mut self) {
        self.list_edit = Some(ListEditState::new_list());
        self.mode = AppMode::NewList;
    }

    pub fn begin_new_note(&mut self) {
        self.list_edit = Some(ListEditState::new_note());
        self.mode = AppMode::NewNote;
    }

    pub fn begin_new_folder(&mut self) {
        self.list_edit = Some(ListEditState::new_folder());
        self.mode = AppMode::NewList; // reuse edit form
    }

    pub fn begin_edit_item_metadata(&mut self) {
        match self.current_item() {
            Some(Item::List(list)) => {
                self.list_edit = Some(ListEditState::from_list(list));
                self.mode = AppMode::EditList;
            }
            Some(Item::Note(note)) => {
                self.list_edit = Some(ListEditState::from_note(note));
                self.mode = AppMode::EditNote;
            }
            Some(Item::Folder(folder)) => {
                self.list_edit = Some(ListEditState::from_folder(folder));
                self.mode = AppMode::EditList;
            }
            None => {}
        }
    }

    pub fn commit_list_edit(&mut self) -> Result<()> {
        let Some(le) = self.list_edit.take() else { return Ok(()); };
        let name = le.name.value().trim().to_string();
        if name.is_empty() { self.mode = AppMode::Browse; return Ok(()); }
        let icon_str = le.icon.value().trim().to_string();
        let icon = if icon_str.is_empty() { None } else { Some(icon_str) };

        if let Some(id) = le.editing_id {
            if let Some(item) = find_item_mut_by_id(&mut self.store.items, &id) {
                match item {
                    Item::List(l) => {
                        if l.name != name {
                            if let Some(old_path) = l.file_path.clone() {
                                let new_path = unique_file_path(
                                    old_path.parent().unwrap_or(Path::new(".")),
                                    &name, "json",
                                );
                                if std::fs::rename(&old_path, &new_path).is_ok() {
                                    l.file_path = Some(new_path);
                                }
                            }
                        }
                        l.name = name; l.icon = icon;
                    }
                    Item::Note(n) => {
                        if n.title != name {
                            if let Some(old_path) = n.file_path.clone() {
                                let new_path = unique_file_path(
                                    old_path.parent().unwrap_or(Path::new(".")),
                                    &name, "md",
                                );
                                if std::fs::rename(&old_path, &new_path).is_ok() {
                                    n.file_path = Some(new_path);
                                }
                            }
                        }
                        n.title = name; n.icon = icon; n.touch();
                    }
                    Item::Folder(f) => {
                        if f.name != name {
                            if let Some(old_dir) = f.dir_path.clone() {
                                let new_dir = unique_dir_path(
                                    old_dir.parent().unwrap_or(Path::new(".")),
                                    &name,
                                );
                                if std::fs::rename(&old_dir, &new_dir).is_ok() {
                                    f.dir_path = Some(new_dir);
                                }
                            }
                        }
                        f.name = name; f.icon = icon;
                    }
                }
            }
        } else if le.is_note {
            let target_dir = self.current_dir();
            let file_path = unique_file_path(&target_dir, &name, "md");
            std::fs::write(&file_path, "").ok();
            let mut note = Note::new(&name);
            note.icon = icon;
            note.file_path = Some(file_path);
            self.store.items.push(Item::Note(note));
            self.selected_item = self.flat_sidebar_len().saturating_sub(1);
        } else if le.is_folder {
            let target_dir = self.current_dir();
            let dir_path = unique_dir_path(&target_dir, &name);
            std::fs::create_dir_all(&dir_path).ok();
            let mut folder = Folder::new(&name);
            folder.icon = icon;
            folder.dir_path = Some(dir_path);
            self.store.items.push(Item::Folder(folder));
            self.selected_item = self.flat_sidebar_len().saturating_sub(1);
        } else {
            let target_dir = self.current_dir();
            let file_path = unique_file_path(&target_dir, &name, "json");
            let mut new_list = ReminderList::new(&name, icon.as_deref());
            new_list.file_path = Some(file_path.clone());
            let json = serde_json::to_string_pretty(&new_list).unwrap_or_default();
            std::fs::write(&file_path, json).ok();
            self.store.items.push(Item::List(new_list));
            self.selected_item = self.flat_sidebar_len().saturating_sub(1);
        }
        self.mode = AppMode::Browse;
        self.save()
    }

    pub fn begin_delete_item(&mut self) {
        if let Some(item) = self.current_item() {
            self.delete_label = match item {
                Item::List(l) => format!("list \"{}\"", l.name),
                Item::Note(n) => format!("note \"{}\"", n.title),
                Item::Folder(f) => format!("folder \"{}\"", f.name),
            };
            self.mode = AppMode::ConfirmDelete;
        }
    }

    pub fn confirm_delete(&mut self) -> Result<()> {
        match self.active_panel {
            Panel::Lists => {
                let path = self.flat_sidebar().get(self.selected_item).map(|fi| fi.path.clone());
                if let Some(path) = path {
                    if let Some(item) = remove_at_path(&mut self.store.items, &path) {
                        // Delete backing file/dir from disk
                        match &item {
                            Item::Note(n) => {
                                if let Some(fp) = &n.file_path { std::fs::remove_file(fp).ok(); }
                                self.push_undo(UndoItem::SidebarItem { item, path });
                            }
                            Item::List(l) => {
                                if let Some(fp) = &l.file_path { std::fs::remove_file(fp).ok(); }
                                self.push_undo(UndoItem::SidebarItem { item, path });
                            }
                            Item::Folder(f) => {
                                if let Some(dp) = &f.dir_path { std::fs::remove_dir_all(dp).ok(); }
                                // No undo for folder deletion (recursive disk delete)
                                drop(item);
                            }
                        }
                        self.clamp_selection();
                        self.mode = AppMode::Browse;
                        return Ok(());
                    }
                }
            }
            Panel::Reminders | Panel::Detail => {
                let visible_ids: Vec<String> = self
                    .visible_reminders()
                    .iter()
                    .map(|r| r.id.clone())
                    .collect();
                if let Some(id) = visible_ids.get(self.selected_reminder) {
                    let id = id.clone();
                    let list_id = self.current_list().map(|l| l.id.clone()).unwrap_or_default();
                    if let Some(list) = self.current_list_mut() {
                        if let Some(pos) = list.reminders.iter().position(|r| r.id == id) {
                            let reminder = list.reminders.remove(pos);
                            self.push_undo(UndoItem::Reminder { list_id, reminder, index: pos });
                        }
                    }
                    self.clamp_selection();
                    self.mode = AppMode::Browse;
                    return self.save();
                }
            }
        }
        self.mode = AppMode::Browse;
        Ok(())
    }

    pub fn cancel_delete(&mut self) {
        self.mode = AppMode::Browse;
    }

    fn push_undo(&mut self, item: UndoItem) {
        self.undo_stack.push(item);
        if self.undo_stack.len() > 20 {
            self.undo_stack.remove(0);
        }
    }

    pub fn undo(&mut self) -> Result<()> {
        let Some(entry) = self.undo_stack.pop() else {
            self.set_status("Nothing to undo.");
            return Ok(());
        };
        match entry {
            UndoItem::Reminder { list_id, reminder, index } => {
                if let Some(item) = find_item_mut_by_id(&mut self.store.items, &list_id) {
                    if let Some(list) = item.as_list_mut() {
                        let insert_at = index.min(list.reminders.len());
                        list.reminders.insert(insert_at, reminder);
                    }
                }
            }
            UndoItem::SidebarItem { item, path } => {
                // Recreate the backing file before re-inserting
                save_item_to_disk(&item).ok();
                insert_at_path(&mut self.store.items, &path, item);
            }
        }
        self.set_status("Undo.");
        self.save()
    }

    // ── Sidebar reorder ───────────────────────────────────────────────────────

    pub fn move_item_up(&mut self) -> Result<()> {
        let flat = self.flat_sidebar();
        let Some(fi) = flat.get(self.selected_item) else { return Ok(()); };
        let path = fi.path.clone();
        let last_idx = *path.last().unwrap_or(&0);
        if last_idx == 0 { return Ok(()); }

        // How many flat rows does the item above us take up?
        let mut above_path = path.clone();
        *above_path.last_mut().unwrap() -= 1;
        let above_size = item_at_path(&self.store.items, &above_path)
            .map(visible_subtree_size)
            .unwrap_or(1);

        let parent_path = path[..path.len() - 1].to_vec();
        if let Some(children) = children_at_path_mut(&mut self.store.items, &parent_path) {
            children.swap(last_idx, last_idx - 1);
        }
        self.selected_item = self.selected_item.saturating_sub(above_size);
        self.save()
    }

    pub fn move_item_down(&mut self) -> Result<()> {
        let flat = self.flat_sidebar();
        let Some(fi) = flat.get(self.selected_item) else { return Ok(()); };
        let path = fi.path.clone();
        let last_idx = *path.last().unwrap_or(&0);

        let parent_path = path[..path.len() - 1].to_vec();
        let parent_len = children_at_path_mut(&mut self.store.items, &parent_path.clone())
            .map(|c| c.len())
            .unwrap_or(0);
        if last_idx + 1 >= parent_len { return Ok(()); }

        // How many flat rows does our current item take?
        let cur_size = item_at_path(&self.store.items, &path)
            .map(visible_subtree_size)
            .unwrap_or(1);

        if let Some(children) = children_at_path_mut(&mut self.store.items, &parent_path) {
            children.swap(last_idx, last_idx + 1);
        }
        self.selected_item += cur_size;
        self.save()
    }

    // ── Move reminder ─────────────────────────────────────────────────────────

    pub fn begin_move_reminder(&mut self) {
        if let Some(r) = self.selected_reminder_ref() {
            self.move_src_id = Some(r.id.clone());
            self.move_list_cursor = self.selected_item;
            self.mode = AppMode::MoveReminder;
        }
    }

    pub fn commit_move_reminder(&mut self) -> Result<()> {
        let Some(src_id) = self.move_src_id.take() else {
            self.mode = AppMode::Browse;
            return Ok(());
        };

        let source_flat_idx = self.selected_item;
        let target_flat_idx = self.move_list_cursor;
        if target_flat_idx == source_flat_idx {
            self.mode = AppMode::Browse;
            return Ok(());
        }

        let flat = self.flat_sidebar();
        let source_path = flat.get(source_flat_idx).map(|fi| fi.path.clone());
        let target_path = flat.get(target_flat_idx).map(|fi| fi.path.clone());
        let (Some(source_path), Some(target_path)) = (source_path, target_path) else {
            self.mode = AppMode::Browse;
            return Ok(());
        };

        // Target must be a list
        if item_at_path(&self.store.items, &target_path).and_then(|i| i.as_list()).is_none() {
            self.mode = AppMode::Browse;
            return Ok(());
        }

        // Find and remove from source list
        let src_list_id = item_at_path(&self.store.items, &source_path)
            .and_then(|i| i.as_list())
            .map(|l| l.id.clone());
        let Some(src_list_id) = src_list_id else {
            self.mode = AppMode::Browse;
            return Ok(());
        };

        let reminder = find_item_mut_by_id(&mut self.store.items, &src_list_id)
            .and_then(|i| i.as_list_mut())
            .and_then(|l| {
                l.reminders.iter().position(|r| r.id == src_id).map(|p| l.reminders.remove(p))
            });

        // Insert into target list
        if let Some(reminder) = reminder {
            let target_list_id = item_at_path(&self.store.items, &target_path)
                .and_then(|i| i.as_list())
                .map(|l| l.id.clone());
            if let Some(tid) = target_list_id {
                if let Some(target) = find_item_mut_by_id(&mut self.store.items, &tid)
                    .and_then(|i| i.as_list_mut())
                {
                    target.reminders.push(reminder);
                }
            }
        }

        self.mode = AppMode::Browse;
        self.clamp_selection();
        self.save()
    }

    pub fn cancel_move(&mut self) {
        self.move_src_id = None;
        self.mode = AppMode::Browse;
    }

    // ── Reminder operations ───────────────────────────────────────────────────

    pub fn begin_new_reminder(&mut self) {
        self.edit = Some(EditState::new());
        self.mode = AppMode::NewReminder;
        self.active_panel = Panel::Detail;
    }

    pub fn begin_edit_reminder(&mut self) {
        if let Some(r) = self.selected_reminder_ref() {
            self.edit = Some(EditState::from_reminder(r));
            self.mode = AppMode::EditReminder;
            self.active_panel = Panel::Detail;
        }
    }

    pub fn begin_delete_reminder(&mut self) {
        if let Some(r) = self.selected_reminder_ref() {
            self.delete_label = format!("reminder \"{}\"", r.title);
            self.mode = AppMode::ConfirmDelete;
        }
    }

    pub fn commit_reminder_edit(&mut self) -> Result<()> {
        let Some(es) = self.edit.take() else { return Ok(()); };
        let title = es.title.value().trim().to_string();
        if title.is_empty() { self.mode = AppMode::Browse; return Ok(()); }
        let due_at = parse_due(es.due_input.value().trim());
        let tags = parse_tags(es.tags_input.value());
        let recurrence = es.recurrence;
        let subtasks = es.subtasks;

        if let Some(id) = es.editing_reminder_id {
            if let Some(list) = self.current_list_mut() {
                if let Some(r) = list.reminders.iter_mut().find(|r| r.id == id) {
                    r.title = title;
                    r.due_at = due_at;
                    r.priority = es.priority;
                    r.tags = tags;
                    r.recurrence = recurrence;
                    r.subtasks = subtasks;
                    r.touch();
                }
            }
        } else {
            let mut r = Reminder::new(&title);
            r.due_at = due_at;
            r.priority = es.priority;
            r.tags = tags;
            r.recurrence = recurrence;
            r.subtasks = subtasks;
            if let Some(list) = self.current_list_mut() {
                list.reminders.push(r);
            }
        }
        self.mode = AppMode::Browse;
        self.active_panel = Panel::Reminders;
        self.clamp_selection();
        self.save()
    }

    pub fn toggle_done(&mut self) -> Result<()> {
        let visible_ids: Vec<String> =
            self.visible_reminders().iter().map(|r| r.id.clone()).collect();
        if let Some(id) = visible_ids.get(self.selected_reminder) {
            let id = id.clone();
            if let Some(list) = self.current_list_mut() {
                if let Some(r) = list.reminders.iter_mut().find(|r| r.id == id) {
                    r.done = !r.done;
                    if r.done { r.notified = true; }
                    r.updated_at = Utc::now();
                }
            }
        }
        self.save()
    }

    // ── Subtask operations ────────────────────────────────────────────────────

    #[allow(dead_code)]
    pub fn add_subtask(&mut self, title: &str) -> Result<()> {
        let id = self.visible_reminders().get(self.selected_reminder).map(|r| r.id.clone());
        if let Some(id) = id {
            if let Some(list) = self.current_list_mut() {
                if let Some(r) = list.reminders.iter_mut().find(|r| r.id == id) {
                    r.subtasks.push(Subtask::new(title));
                    r.touch();
                }
            }
        }
        self.save()
    }

    #[allow(dead_code)]
    pub fn toggle_subtask(&mut self, idx: usize) -> Result<()> {
        let id = self.visible_reminders().get(self.selected_reminder).map(|r| r.id.clone());
        if let Some(id) = id {
            if let Some(list) = self.current_list_mut() {
                if let Some(r) = list.reminders.iter_mut().find(|r| r.id == id) {
                    if let Some(s) = r.subtasks.get_mut(idx) {
                        s.done = !s.done;
                        r.updated_at = Utc::now();
                    }
                }
            }
        }
        self.save()
    }

    #[allow(dead_code)]
    pub fn delete_subtask(&mut self, idx: usize) -> Result<()> {
        let id = self.visible_reminders().get(self.selected_reminder).map(|r| r.id.clone());
        if let Some(id) = id {
            if let Some(list) = self.current_list_mut() {
                if let Some(r) = list.reminders.iter_mut().find(|r| r.id == id) {
                    if idx < r.subtasks.len() {
                        r.subtasks.remove(idx);
                        r.touch();
                    }
                }
            }
        }
        self.save()
    }

    // ── Export ────────────────────────────────────────────────────────────────

    pub fn export_current(&mut self) -> Result<()> {
        let path = if let Some(note) = self.current_note() {
            let filename = sanitize_filename(&note.title);
            let p = home_dir().join(format!("{}.md", filename));
            std::fs::write(&p, &note.content)?;
            p
        } else if let Some(list) = self.current_list() {
            let filename = sanitize_filename(&list.name);
            let p = home_dir().join(format!("{}.md", filename));
            let mut out = format!("# {}\n\n", list.display_name());
            for r in &list.reminders {
                let check = if r.done { "[x]" } else { "[ ]" };
                out.push_str(&format!("- {} {}\n", check, r.title));
                if let Some(due) = r.due_at {
                    out.push_str(&format!(
                        "  - Due: {}\n",
                        due.with_timezone(&Local).format("%Y-%m-%d %H:%M")
                    ));
                }
                for s in &r.subtasks {
                    let sc = if s.done { "[x]" } else { "[ ]" };
                    out.push_str(&format!("  - {} {}\n", sc, s.title));
                }
            }
            std::fs::write(&p, &out)?;
            p
        } else {
            self.set_status("Nothing to export.");
            return Ok(());
        };
        self.set_status(format!("Exported → {}", path.display()));
        Ok(())
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Sanitize a string for use as a filename (keeps spaces and unicode, strips shell-unsafe chars).
pub fn sanitize_for_filename(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' | '\n' | '\r' => '_',
            _ => c,
        })
        .collect()
}

/// Find a path that doesn't exist yet by appending _1, _2, … as needed.
pub fn unique_file_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let s = sanitize_for_filename(stem);
    let p = dir.join(format!("{}.{}", s, ext));
    if !p.exists() { return p; }
    for i in 1..=999 {
        let p = dir.join(format!("{}_{}.{}", s, i, ext));
        if !p.exists() { return p; }
    }
    dir.join(format!("{}.{}", s, ext))
}

pub fn unique_dir_path(parent: &Path, name: &str) -> PathBuf {
    let s = sanitize_for_filename(name);
    let p = parent.join(&s);
    if !p.exists() { return p; }
    for i in 1..=999 {
        let p = parent.join(format!("{}_{}", s, i));
        if !p.exists() { return p; }
    }
    parent.join(s)
}

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect::<String>()
        .to_lowercase()
}

/// Parse a comma-separated tag string into a Vec<String>.
pub fn parse_tags(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect()
}

/// Parse a human-readable due date string into a UTC DateTime.
pub fn parse_due(input: &str) -> Option<DateTime<Utc>> {
    if input.is_empty() { return None; }
    let lower = input.to_lowercase();
    let now = Local::now();
    let today = now.date_naive();

    let (date_part, time_part) = split_time(&lower);
    let time = parse_time(time_part).unwrap_or(NaiveTime::from_hms_opt(9, 0, 0).unwrap());

    let date: Option<NaiveDate> = if date_part == "today" {
        Some(today)
    } else if date_part == "tomorrow" {
        Some(today + chrono::Duration::days(1))
    } else if let Some(rest) = date_part.strip_prefix("next ") {
        weekday_from_str(rest).map(|wd| next_weekday(today, wd))
    } else if let Some(rest) = date_part.strip_prefix("in ") {
        let rest = rest.trim_end_matches(" days").trim_end_matches(" day");
        rest.parse::<i64>().ok().map(|n| today + chrono::Duration::days(n))
    } else {
        NaiveDate::parse_from_str(date_part.trim(), "%Y-%m-%d").ok()
    };

    let naive = NaiveDateTime::new(date?, time);
    Local.from_local_datetime(&naive).single().map(|dt| dt.with_timezone(&Utc))
}

fn split_time(s: &str) -> (&str, &str) {
    let s = s.trim();
    if s.len() >= 5 {
        let tail = &s[s.len() - 5..];
        if tail.chars().nth(2) == Some(':')
            && tail[..2].chars().all(|c| c.is_ascii_digit())
            && tail[3..].chars().all(|c| c.is_ascii_digit())
        {
            return (s[..s.len() - 5].trim(), tail);
        }
    }
    (s, "")
}

fn parse_time(s: &str) -> Option<NaiveTime> {
    if s.is_empty() { return None; }
    NaiveTime::parse_from_str(s, "%H:%M").ok()
}

fn weekday_from_str(s: &str) -> Option<chrono::Weekday> {
    use chrono::Weekday::*;
    match s.trim() {
        "monday" | "mon" => Some(Mon),
        "tuesday" | "tue" => Some(Tue),
        "wednesday" | "wed" => Some(Wed),
        "thursday" | "thu" => Some(Thu),
        "friday" | "fri" => Some(Fri),
        "saturday" | "sat" => Some(Sat),
        "sunday" | "sun" => Some(Sun),
        _ => None,
    }
}

fn next_weekday(from: NaiveDate, target: chrono::Weekday) -> NaiveDate {
    use chrono::Datelike;
    let days_ahead =
        (target.num_days_from_monday() + 7 - from.weekday().num_days_from_monday()) % 7;
    from + chrono::Duration::days(if days_ahead == 0 { 7 } else { days_ahead } as i64)
}
