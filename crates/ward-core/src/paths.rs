use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub fn data_dir() -> Result<PathBuf> {
    let base = dirs::data_dir().context("could not determine data directory")?;
    let dir = base.join("ward");
    std::fs::create_dir_all(&dir).context("could not create data directory")?;
    Ok(dir)
}

pub fn store_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("data.json"))
}

pub fn last_dir_file() -> Result<PathBuf> {
    Ok(data_dir()?.join("last_dir"))
}

pub fn load_last_dir() -> Option<PathBuf> {
    last_dir_file()
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| PathBuf::from(s.trim().to_string()))
        .filter(|p| p.is_dir())
}

pub fn save_last_dir(dir: &Path) -> Result<()> {
    let f = last_dir_file()?;
    std::fs::write(f, dir.to_string_lossy().as_bytes())
        .context("failed to save last dir")?;
    Ok(())
}
