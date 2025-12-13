use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use anyhow::{anyhow, Context, Result};

#[derive(Debug, Clone)]
pub struct EntryInfo {
    pub name: OsString,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

pub fn list_dir(dir: &Path) -> Result<Vec<EntryInfo>> {
    let mut entries = Vec::new();

    for item in fs::read_dir(dir).with_context(|| format!("read_dir failed: {}", dir.display()))? {
        let item = item?;
        let path = item.path();
        let meta = item.metadata()?;

        entries.push(EntryInfo {
            name: item.file_name(),
            is_dir: meta.is_dir(),
            size: if meta.is_file() { meta.len() } else { 0 },
            modified: meta.modified().ok(),
            path,
        });
    }

    // directories first, then name
    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a
            .name
            .to_string_lossy()
            .to_lowercase()
            .cmp(&b.name.to_string_lossy().to_lowercase()),
    });

    Ok(entries)
}

pub fn read_file_limited(path: &Path, max_bytes: usize) -> Result<Vec<u8>> {
    let f = fs::File::open(path).with_context(|| format!("open failed: {}", path.display()))?;
    let mut buf = Vec::new();
    use std::io::Read;
    f.take(max_bytes as u64)
        .read_to_end(&mut buf)
        .with_context(|| format!("read failed: {}", path.display()))?;
    Ok(buf)
}

pub fn canonicalize_root(root: &Path) -> Result<PathBuf> {
    let canon = fs::canonicalize(root)
        .with_context(|| format!("Failed to canonicalize root: {}", root.display()))?;
    if !canon.is_dir() {
        return Err(anyhow!("Root is not a directory: {}", canon.display()));
    }
    Ok(canon)
}

pub fn resolve_under_root(root: &Path, rel: &str) -> Result<PathBuf> {
    // rel is treated as path relative to root; disallow absolute paths.
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() {
        return Err(anyhow!("Absolute paths are not allowed"));
    }

    let joined = root.join(rel_path);
    let canon = fs::canonicalize(&joined)
        .with_context(|| format!("Failed to canonicalize path: {}", joined.display()))?;

    if !canon.starts_with(root) {
        return Err(anyhow!("Path is outside root"));
    }

    Ok(canon)
}

pub fn to_rel_string(root: &Path, full: &Path) -> String {
    match full.strip_prefix(root) {
        Ok(p) => p.to_string_lossy().replace('\\', "/"),
        Err(_) => "".to_string(),
    }
}
