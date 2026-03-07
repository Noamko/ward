use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Store {
    pub version: u8,
    pub items: Vec<Item>,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            version: 1,
            items: vec![Item::List(ReminderList::new("Personal", Some("📝")))],
        }
    }
}

/// A top-level item in the sidebar — a reminder list, a markdown note, or a folder.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Item {
    List(ReminderList),
    Note(Note),
    Folder(Folder),
}

impl Item {
    pub fn id(&self) -> &str {
        match self {
            Item::List(l) => &l.id,
            Item::Note(n) => &n.id,
            Item::Folder(f) => &f.id,
        }
    }

    pub fn display_name(&self) -> String {
        match self {
            Item::List(l) => l.display_name(),
            Item::Note(n) => n.display_name(),
            Item::Folder(f) => f.display_name(),
        }
    }

    pub fn as_list(&self) -> Option<&ReminderList> {
        if let Item::List(l) = self { Some(l) } else { None }
    }

    pub fn as_list_mut(&mut self) -> Option<&mut ReminderList> {
        if let Item::List(l) = self { Some(l) } else { None }
    }

    pub fn as_note(&self) -> Option<&Note> {
        if let Item::Note(n) = self { Some(n) } else { None }
    }

    pub fn as_note_mut(&mut self) -> Option<&mut Note> {
        if let Item::Note(n) = self { Some(n) } else { None }
    }

    pub fn as_folder(&self) -> Option<&Folder> {
        if let Item::Folder(f) = self { Some(f) } else { None }
    }

    pub fn as_folder_mut(&mut self) -> Option<&mut Folder> {
        if let Item::Folder(f) = self { Some(f) } else { None }
    }

    pub fn is_note(&self) -> bool { matches!(self, Item::Note(_)) }
    pub fn is_folder(&self) -> bool { matches!(self, Item::Folder(_)) }
}

/// A collapsible group that can contain notes, lists, and nested folders.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Folder {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub children: Vec<Item>,
    #[serde(default)]
    pub collapsed: bool,
    /// Path to the backing directory on disk. Not persisted.
    #[serde(skip, default)]
    pub dir_path: Option<PathBuf>,
}

impl Folder {
    pub fn new(name: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            icon: None,
            children: Vec::new(),
            collapsed: false,
            dir_path: None,
        }
    }

    pub fn display_name(&self) -> String {
        let icon = self.icon.as_deref().unwrap_or("📁");
        format!("{} {}", icon, self.name)
    }
}

/// A standalone markdown note.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Path to the backing `.md` file on disk. Not persisted.
    #[serde(skip, default)]
    pub file_path: Option<PathBuf>,
}

impl Note {
    pub fn new(title: &str) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.to_string(),
            icon: None,
            content: String::new(),
            created_at: now,
            updated_at: now,
            file_path: None,
        }
    }

    pub fn display_name(&self) -> String {
        let icon = self.icon.as_deref().unwrap_or("📝");
        format!("{} {}", icon, self.title)
    }

    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReminderList {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub reminders: Vec<Reminder>,
    pub created_at: DateTime<Utc>,
    /// Path to the backing `.json` file on disk. Not persisted.
    #[serde(skip, default)]
    pub file_path: Option<PathBuf>,
}

impl ReminderList {
    pub fn new(name: &str, icon: Option<&str>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            icon: icon.map(str::to_string),
            reminders: vec![],
            created_at: Utc::now(),
            file_path: None,
        }
    }

    pub fn pending_count(&self) -> usize {
        self.reminders.iter().filter(|r| !r.done).count()
    }

    pub fn display_name(&self) -> String {
        match &self.icon {
            Some(icon) => format!("{} {}", icon, self.name),
            None => format!("☑ {}", self.name),
        }
    }
}

/// A checklist item nested inside a reminder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subtask {
    pub id: String,
    pub title: String,
    pub done: bool,
}

impl Subtask {
    pub fn new(title: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.to_string(),
            done: false,
        }
    }
}

/// How often a reminder repeats after it fires.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Recurrence {
    Daily,
    Weekly,
    Monthly,
    Yearly,
    EveryNDays(u32),
}

impl Recurrence {
    pub fn label(&self) -> String {
        match self {
            Recurrence::Daily => "Daily".into(),
            Recurrence::Weekly => "Weekly".into(),
            Recurrence::Monthly => "Monthly".into(),
            Recurrence::Yearly => "Yearly".into(),
            Recurrence::EveryNDays(n) => format!("Every {} days", n),
        }
    }

    /// Advance a due date by one recurrence interval.
    pub fn advance(&self, from: DateTime<Utc>) -> DateTime<Utc> {
        use chrono::Months;
        match self {
            Recurrence::Daily => from + chrono::Duration::days(1),
            Recurrence::Weekly => from + chrono::Duration::weeks(1),
            Recurrence::Monthly => from + Months::new(1),
            Recurrence::Yearly => from + Months::new(12),
            Recurrence::EveryNDays(n) => from + chrono::Duration::days(*n as i64),
        }
    }

    /// All variants in display order, for cycling through in the UI.
    pub fn variants() -> &'static [Recurrence] {
        &[
            Recurrence::Daily,
            Recurrence::Weekly,
            Recurrence::Monthly,
            Recurrence::Yearly,
        ]
    }

    pub fn next_variant(&self) -> Option<Recurrence> {
        match self {
            Recurrence::Daily => Some(Recurrence::Weekly),
            Recurrence::Weekly => Some(Recurrence::Monthly),
            Recurrence::Monthly => Some(Recurrence::Yearly),
            Recurrence::Yearly => None,
            Recurrence::EveryNDays(_) => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reminder {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub due_at: Option<DateTime<Utc>>,
    pub priority: Priority,
    pub done: bool,
    pub notified: bool,
    /// Minutes before due_at to fire the notification (0 = at due time).
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub notify_before_mins: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Free-form tags, e.g. ["work", "urgent"].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// If set, the reminder reschedules itself after firing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recurrence: Option<Recurrence>,
    /// Optional checklist nested inside this reminder.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subtasks: Vec<Subtask>,
}

fn is_zero_u32(v: &u32) -> bool { *v == 0 }

impl Reminder {
    pub fn new(title: &str) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.to_string(),
            notes: None,
            due_at: None,
            priority: Priority::Medium,
            done: false,
            notified: false,
            notify_before_mins: 0,
            created_at: now,
            updated_at: now,
            tags: vec![],
            recurrence: None,
            subtasks: vec![],
        }
    }

    /// Returns the UTC time at which the notification should fire.
    pub fn notify_at(&self) -> Option<DateTime<Utc>> {
        self.due_at.map(|due| due - chrono::Duration::minutes(self.notify_before_mins as i64))
    }

    pub fn is_overdue(&self) -> bool {
        match self.due_at {
            Some(due) => !self.done && due < Utc::now(),
            None => false,
        }
    }

    pub fn is_due_today(&self) -> bool {
        use chrono::Local;
        match self.due_at {
            Some(due) => {
                let due_local = due.with_timezone(&Local);
                let today = Local::now().date_naive();
                !self.done && due_local.date_naive() == today
            }
            None => false,
        }
    }

    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
        self.notified = false;
    }

    pub fn subtask_summary(&self) -> Option<String> {
        if self.subtasks.is_empty() {
            return None;
        }
        let done = self.subtasks.iter().filter(|s| s.done).count();
        Some(format!("{}/{}", done, self.subtasks.len()))
    }

    /// Advance due_at by one recurrence interval and reset notification state.
    /// Returns true if the due date was advanced.
    pub fn advance_recurrence(&mut self) -> bool {
        if let (Some(rec), Some(due)) = (&self.recurrence.clone(), self.due_at) {
            self.due_at = Some(rec.advance(due));
            self.done = false;
            self.notified = false;
            self.updated_at = Utc::now();
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Priority {
    Low,
    #[default]
    Medium,
    High,
}

impl Priority {
    pub fn label(&self) -> &'static str {
        match self {
            Priority::Low => "Low",
            Priority::Medium => "Medium",
            Priority::High => "High",
        }
    }

    pub fn indicator(&self) -> &'static str {
        match self {
            Priority::Low => "○  ",
            Priority::Medium => "●  ",
            Priority::High => "!! ",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Priority::Low => Priority::Medium,
            Priority::Medium => Priority::High,
            Priority::High => Priority::Low,
        }
    }
}
