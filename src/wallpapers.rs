//! Wallpaper discovery and resolution on disk.

use crate::prelude::*;

// ───────────────────────────── wallpapers on disk ─────────────────────────────

/// Folder scanned for user wallpapers: `./wallpapers` unless overridden.
pub(crate) fn wallpaper_dir() -> PathBuf {
    std::env::var_os("HORN_WALLPAPERS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("wallpapers"))
}

pub(crate) fn is_image_file(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase());
    matches!(
        ext.as_deref(),
        Some("png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif" | "tif" | "tiff")
    )
}

/// Wallpaper images found in the wallpapers folder, sorted by path.
pub(crate) fn scan_wallpapers() -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(wallpaper_dir()) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_image_file(path))
        .collect();
    files.sort();
    files
}

/// Resolve a stored wallpaper path, accepting a full path or a bare file name
/// inside the wallpapers folder.
pub(crate) fn resolve_wallpaper(path: &str) -> Option<PathBuf> {
    let direct = PathBuf::from(path);
    if direct.is_file() {
        return Some(direct);
    }
    let in_dir = wallpaper_dir().join(path);
    in_dir.is_file().then_some(in_dir)
}

/// A short label for a wallpaper file (`sunset.jpg` → `sunset`).
pub(crate) fn wallpaper_label(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("wallpaper")
        .to_string()
}
