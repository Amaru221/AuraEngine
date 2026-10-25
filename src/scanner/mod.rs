//! Library scanning.

pub mod thumbs;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crossbeam_channel::Sender;
use walkdir::WalkDir;

use crate::config::{is_supported_media, MediaKind};
use crate::util;

#[derive(Clone, Debug)]
pub struct MediaItem {
    pub path: PathBuf,
    pub name: String,
    pub kind: MediaKind,
    pub size: u64,
    pub modified_ms: u128,
    pub folder: String,
    /// Cached thumbnail cache key (hashing it on every frame was wasteful).
    pub thumb_key: String,
    /// Lower-cased `name\nfolder`, ready for large-library filtering.
    pub search_key: String,
}

impl MediaItem {
    fn new(
        path: PathBuf,
        name: String,
        kind: MediaKind,
        size: u64,
        modified_ms: u128,
        folder: String,
        thumb_key: String,
    ) -> Self {
        let search_key = format!(
            "{}\n{}",
            name.to_lowercase(),
            folder.to_lowercase()
        );
        Self {
            path,
            name,
            kind,
            size,
            modified_ms,
            folder,
            thumb_key,
            search_key,
        }
    }
}

#[derive(Clone, Debug)]
pub enum ScanEvent {
    Progress,
    Done(Vec<MediaItem>),
    Failed(String),
}

const MAX_ITEMS: usize = 25_000;
const MAX_DEPTH: usize = 8;

pub fn scan_async(dirs: Vec<PathBuf>, tx: Sender<ScanEvent>) {
    std::thread::Builder::new()
        .name("aura-scan".into())
        .spawn(move || {
            let result = scan(&dirs, &tx);
            match result {
                Ok(items) => {
                    let _ = tx.send(ScanEvent::Done(items));
                }
                Err(err) => {
                    let _ = tx.send(ScanEvent::Failed(format!("{err:#}")));
                }
            }
        })
        .ok();
}

fn scan(dirs: &[PathBuf], tx: &Sender<ScanEvent>) -> anyhow::Result<Vec<MediaItem>> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut items: Vec<MediaItem> = Vec::new();
    let mut visited = 0usize;

    for dir in dirs {
        if !dir.is_dir() {
            continue;
        }
        for entry in WalkDir::new(dir)
            .max_depth(MAX_DEPTH)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                !(e.file_type().is_dir()
                    && (name.starts_with('.')
                        || name.eq_ignore_ascii_case("node_modules")
                        || name.eq_ignore_ascii_case("$RECYCLE.BIN")
                        || name.eq_ignore_ascii_case("System Volume Information")))
            })
            .filter_map(|e| e.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if !is_supported_media(path) {
                continue;
            }
            let key = path.to_string_lossy().to_lowercase();
            if !seen.insert(key) {
                continue;
            }
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            if metadata.len() == 0 {
                continue;
            }
            let modified_ms = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis())
                .unwrap_or(0);
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string());
            let folder = path
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            let thumb_key = util::file_cache_key(path, metadata.len(), modified_ms);
            items.push(MediaItem::new(
                path.to_path_buf(),
                name,
                MediaKind::from_path(path),
                metadata.len(),
                modified_ms,
                folder,
                thumb_key,
            ));

            visited += 1;
            if visited % 250 == 0 {
                let _ = tx.send(ScanEvent::Progress);
            }
            if items.len() >= MAX_ITEMS {
                break;
            }
        }
        if items.len() >= MAX_ITEMS {
            break;
        }
    }

    items.sort_by(|a, b| {
        b.modified_ms
            .cmp(&a.modified_ms)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(items)
}

pub fn item_for(path: &Path) -> Option<MediaItem> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified_ms = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string());
    let folder = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    Some(MediaItem::new(
        path.to_path_buf(),
        name,
        MediaKind::from_path(path),
        metadata.len(),
        modified_ms,
        folder,
        util::file_cache_key(path, metadata.len(), modified_ms),
    ))
}

