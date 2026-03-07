use crate::model::{Folder, Item, Note, ReminderList, Store};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

// ── Legacy JSON store (used by daemon fallback) ───────────────────────────────

pub fn load(path: &Path) -> Result<Store> {
    if !path.exists() {
        return Ok(Store::default());
    }
    let bytes = std::fs::read(path).context("failed to read store")?;

    if let Ok(store) = serde_json::from_slice::<Store>(&bytes) {
        return Ok(store);
    }

    #[derive(Deserialize)]
    struct OldStore {
        #[serde(default)]
        lists: Vec<ReminderList>,
    }
    if let Ok(old) = serde_json::from_slice::<OldStore>(&bytes) {
        if !old.lists.is_empty() {
            return Ok(Store {
                version: 1,
                items: old.lists.into_iter().map(Item::List).collect(),
            });
        }
    }

    Ok(Store::default())
}

pub fn save(path: &Path, store: &Store) -> Result<()> {
    let tmp = path.with_extension("tmp");
    let json = serde_json::to_vec_pretty(store).context("failed to serialize store")?;
    std::fs::write(&tmp, json).context("failed to write temp store")?;
    std::fs::rename(&tmp, path).context("failed to commit store")?;
    Ok(())
}

// ── Directory-based store ─────────────────────────────────────────────────────

/// Scan a directory and build a Store from its contents.
/// - `.md` files become Notes
/// - `.json` files that parse as ReminderList become Lists
/// - subdirectories become Folders (recursed)
pub fn load_dir(dir: &Path) -> Result<Store> {
    let mut items = Vec::new();
    scan_directory(dir, &mut items)?;
    Ok(Store { version: 1, items })
}

fn scan_directory(dir: &Path, items: &mut Vec<Item>) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("failed to read directory {}", dir.display()))?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        if name.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            let mut folder = Folder::new(&name);
            folder.dir_path = Some(path.clone());
            scan_directory(&path, &mut folder.children)?;
            items.push(Item::Folder(folder));
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&name)
                .to_string();
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            let mut note = Note::new(&stem);
            note.content = content;
            note.file_path = Some(path);
            items.push(Item::Note(note));
        } else if path.extension().and_then(|e| e.to_str()) == Some("json") {
            if let Ok(bytes) = std::fs::read(&path) {
                if let Ok(mut list) = serde_json::from_slice::<ReminderList>(&bytes) {
                    list.file_path = Some(path);
                    items.push(Item::List(list));
                }
            }
        }
    }
    Ok(())
}

/// Write a single item (and its children) back to disk.
pub fn save_item_to_disk(item: &Item) -> Result<()> {
    match item {
        Item::Note(n) => {
            if let Some(path) = &n.file_path {
                std::fs::write(path, &n.content)
                    .with_context(|| format!("failed to write note {}", path.display()))?;
            }
        }
        Item::List(l) => {
            if let Some(path) = &l.file_path {
                let json =
                    serde_json::to_string_pretty(l).context("failed to serialize list")?;
                let tmp = path.with_extension("tmp");
                std::fs::write(&tmp, json)
                    .with_context(|| format!("failed to write list {}", path.display()))?;
                std::fs::rename(&tmp, path).context("failed to commit list")?;
            }
        }
        Item::Folder(f) => {
            for child in &f.children {
                save_item_to_disk(child)?;
            }
        }
    }
    Ok(())
}

/// Write all items in the store back to disk.
pub fn save_all_to_disk(items: &[Item]) -> Result<()> {
    for item in items {
        save_item_to_disk(item)?;
    }
    Ok(())
}
