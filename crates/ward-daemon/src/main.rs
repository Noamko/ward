use anyhow::Result;
use chrono::Utc;
use ward_core::{
    model::Item,
    notify,
    paths::load_last_dir,
    store::{load_dir, save_item_to_disk},
};
use std::time::Duration;

fn main() -> Result<()> {
    loop {
        if let Some(dir) = load_last_dir() {
            if let Ok(mut s) = load_dir(&dir) {
                let now = Utc::now();
                let changed_items = fire_due_notifications(&mut s.items, now);
                for item in changed_items {
                    save_item_to_disk(&item).ok();
                }
            }
        }
        std::thread::sleep(Duration::from_secs(60));
    }
}

fn fire_due_notifications(items: &mut Vec<Item>, now: chrono::DateTime<Utc>) -> Vec<Item> {
    let mut changed = Vec::new();
    for item in items.iter_mut() {
        match item {
            Item::List(list) => {
                let mut list_changed = false;
                for r in &mut list.reminders {
                    if r.done || r.notified { continue; }
                    if let Some(due) = r.due_at {
                        if due <= now {
                            let body = r.notes.as_deref().unwrap_or("Due now.");
                            notify::fire(&r.title, body).ok();
                            if !r.advance_recurrence() { r.notified = true; }
                            list_changed = true;
                        }
                    }
                }
                if list_changed {
                    changed.push(item.clone());
                }
            }
            Item::Folder(f) => {
                let sub = fire_due_notifications(&mut f.children, now);
                changed.extend(sub);
            }
            Item::Note(_) => {}
        }
    }
    changed
}
