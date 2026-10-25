//! Persistent user configuration.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::util;

pub const CURRENT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Lang {
    #[default]
    En,
    Es,
    Fr,
}

impl Lang {
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Es => "Espanol",
            Lang::Fr => "Francais",
        }
    }

    pub fn detect() -> Self {
        // Prefer the Windows UI language, then the LANG environment variable.
        unsafe {
            use windows::Win32::Globalization::GetUserDefaultUILanguage;
            let id = GetUserDefaultUILanguage() as u32;
            match id & 0x3FF {
                0x0A => return Lang::Es,
                0x0C => return Lang::Fr,
                _ => {}
            }
        }
        if let Ok(lang) = std::env::var("LANG") {
            let lower = lang.to_lowercase();
            if lower.starts_with("es") {
                return Lang::Es;
            }
            if lower.starts_with("fr") {
                return Lang::Fr;
            }
        }
        Lang::En
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Scaling {
    Fit,
    #[default]
    Fill,
    Stretch,
    Center,
}

impl Scaling {
    pub fn all() -> [Scaling; 4] {
        [Scaling::Fit, Scaling::Fill, Scaling::Stretch, Scaling::Center]
    }

    /// mpv properties used to reproduce this scaling mode.
    pub fn mpv_properties(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Scaling::Fit => &[
                ("keepaspect", "yes"),
                ("panscan", "0.0"),
                ("video-unscaled", "no"),
            ],
            Scaling::Fill => &[
                ("keepaspect", "yes"),
                ("panscan", "1.0"),
                ("video-unscaled", "no"),
            ],
            Scaling::Stretch => &[
                ("keepaspect", "no"),
                ("panscan", "0.0"),
                ("video-unscaled", "no"),
            ],
            Scaling::Center => &[
                ("keepaspect", "yes"),
                ("panscan", "0.0"),
                ("video-unscaled", "yes"),
            ],
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum HudStyle {
    #[default]
    Grid,
    Cinematic,
    Dock,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreset {
    #[default]
    Dark,
    Midnight,
    Light,
    Custom,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ParentMode {
    #[default]
    Auto,
    WorkerW,
    Progman,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    #[default]
    Image,
    Video,
}

impl MediaKind {
    pub fn is_video_path(path: &Path) -> bool {
        video_extensions()
            .iter()
            .any(|ext| path.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext)))
    }

    pub fn from_path(path: &Path) -> Self {
        if Self::is_video_path(path) {
            MediaKind::Video
        } else {
            MediaKind::Image
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            MediaKind::Image => "IMAGE",
            MediaKind::Video => "LIVE",
        }
    }
}

pub fn video_extensions() -> &'static [&'static str] {
    &[
        "mp4", "webm", "mkv", "mov", "avi", "m4v", "wmv", "flv", "mpg", "mpeg", "ts", "m2ts",
        "mts", "ogv", "gif", "webp",
    ]
}

pub fn image_extensions() -> &'static [&'static str] {
    &[
        "jpg", "jpeg", "png", "bmp", "webp", "gif", "tif", "tiff", "ico", "jfif",
    ]
}

pub fn is_supported_media(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    let ext = ext.to_ascii_lowercase();
    video_extensions().contains(&ext.as_str()) || image_extensions().contains(&ext.as_str())
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Assignment {
    pub path: PathBuf,
    pub kind: MediaKind,
    #[serde(default)]
    pub scaling: Scaling,
    /// Cached thumbnail key, so the Displays page does not stat/hash files
    /// on every frame.
    #[serde(default)]
    pub thumb_key: String,
}

impl Assignment {
    pub fn new(path: PathBuf, scaling: Scaling) -> Self {
        let kind = MediaKind::from_path(&path);
        Self {
            path,
            kind,
            scaling,
            thumb_key: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    pub language: Lang,

    pub library_dirs: Vec<PathBuf>,
    pub favorites: Vec<PathBuf>,

    /// Monitor device name (e.g. `\\.\DISPLAY1`) or `*` for every monitor.
    pub assignments: BTreeMap<String, Assignment>,
    /// Static wallpapers captured before Aura replaced them, for restoration.
    pub originals: BTreeMap<String, String>,

    pub volume: u8,
    pub muted: bool,

    pub pause_on_fullscreen: bool,
    pub pause_on_occlusion: bool,
    pub pause_on_battery: bool,

    pub start_with_windows: bool,
    pub start_hidden: bool,
    pub close_to_tray: bool,

    pub accent_from_wallpaper: bool,
    pub apply_windows_accent: bool,
    pub accent_color: Option<[u8; 3]>,
    pub theme_preset: ThemePreset,
    pub theme_bg: Option<[u8; 3]>,
    pub window_opacity: u8,
    pub hud_style: HudStyle,
    pub parent_mode: ParentMode,

    pub mpv_path: Option<PathBuf>,
    pub wallhaven_api_key: String,
    pub pexels_api_key: String,

    pub restore_on_exit: bool,
    pub last_applied: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let pictures = home.join("Pictures").join("Wallpapers");
        let videos = home.join("Videos").join("Wallpapers");
        let _ = std::fs::create_dir_all(&pictures);
        let _ = std::fs::create_dir_all(&videos);

        Self {
            version: CURRENT_VERSION,
            language: Lang::detect(),
            library_dirs: vec![util::online_dir(), pictures, videos],
            favorites: Vec::new(),
            assignments: BTreeMap::new(),
            originals: BTreeMap::new(),
            volume: 60,
            muted: true,
            pause_on_fullscreen: true,
            pause_on_occlusion: true,
            pause_on_battery: false,
            start_with_windows: false,
            start_hidden: false,
            close_to_tray: true,
            accent_from_wallpaper: true,
            apply_windows_accent: false,
            accent_color: None,
            theme_preset: ThemePreset::Dark,
            theme_bg: None,
            window_opacity: 92,
            hud_style: HudStyle::Grid,
            parent_mode: ParentMode::Auto,
            mpv_path: None,
            wallhaven_api_key: String::new(),
            pexels_api_key: String::new(),
            restore_on_exit: false,
            last_applied: None,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let path = util::config_path();
        let raw = match std::fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(_) => return Self::default(),
        };
        match serde_json::from_str::<Config>(&raw) {
            Ok(mut cfg) => {
                if cfg.version < CURRENT_VERSION {
                    cfg.version = CURRENT_VERSION;
                }
                cfg.normalize();
                cfg
            }
            Err(err) => {
                log::warn!("invalid config ({err}); backing up and starting fresh");
                let backup = util::config_root().join(format!(
                    "config.corrupt-{}.json",
                    util::now_ms()
                ));
                let _ = std::fs::rename(&path, backup);
                Self::default()
            }
        }
    }

    fn normalize(&mut self) {
        self.volume = self.volume.min(100);
        self.window_opacity = self.window_opacity.clamp(20, 100);
        self.library_dirs.retain(|p| !p.as_os_str().is_empty());
        if self.library_dirs.is_empty() {
            let defaults = Config::default();
            self.library_dirs = defaults.library_dirs;
        }
        self.favorites.sort();
        self.favorites.dedup();
        self.assignments.retain(|_, a| !a.path.as_os_str().is_empty());
    }

    pub fn save(&self) -> Result<()> {
        std::fs::create_dir_all(util::config_root()).ok();
        let json = serde_json::to_string_pretty(self).context("serialize config")?;
        let path = util::config_path();
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json).context("write config")?;
        std::fs::rename(&tmp, &path).context("replace config")?;
        Ok(())
    }

    pub fn assignment_for(&self, monitor_key: &str) -> Option<&Assignment> {
        self.assignments
            .get(monitor_key)
            .or_else(|| self.assignments.get("*"))
    }

    pub fn set_assignment(&mut self, monitor_key: &str, assignment: Assignment) {
        self.last_applied = Some(assignment.path.clone());
        self.assignments.insert(monitor_key.to_string(), assignment);
    }

    pub fn clear_assignment(&mut self, monitor_key: &str) {
        self.assignments.remove(monitor_key);
    }

    pub fn is_favorite(&self, path: &Path) -> bool {
        self.favorites.iter().any(|f| f == path)
    }

    pub fn toggle_favorite(&mut self, path: &Path) {
        if let Some(pos) = self.favorites.iter().position(|f| f == path) {
            self.favorites.remove(pos);
        } else {
            self.favorites.push(path.to_path_buf());
            self.favorites.sort();
        }
    }

    pub fn add_library_dir(&mut self, dir: PathBuf) {
        if !self.library_dirs.iter().any(|d| d == &dir) {
            self.library_dirs.push(dir);
        }
    }

    pub fn remove_library_dir(&mut self, dir: &Path) {
        self.library_dirs.retain(|d| d != dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaling_maps_to_mpv() {
        assert!(Scaling::Fill
            .mpv_properties()
            .iter()
            .any(|(k, v)| *k == "panscan" && *v == "1.0"));
        assert!(Scaling::Stretch
            .mpv_properties()
            .iter()
            .any(|(k, v)| *k == "keepaspect" && *v == "no"));
    }

    #[test]
    fn detects_kind() {
        assert_eq!(MediaKind::from_path(Path::new("a.mp4")), MediaKind::Video);
        assert_eq!(MediaKind::from_path(Path::new("a.WEBM")), MediaKind::Video);
        assert_eq!(MediaKind::from_path(Path::new("a.jpg")), MediaKind::Image);
        assert_eq!(MediaKind::from_path(Path::new("a.txt")), MediaKind::Image);
        assert!(is_supported_media(Path::new("a.PNG")));
        assert!(!is_supported_media(Path::new("a.exe")));
    }

    #[test]
    fn wildcard_assignment_is_a_fallback() {
        let mut cfg = Config::default();
        cfg.set_assignment(
            "*",
            Assignment::new(PathBuf::from("C:/a.mp4"), Scaling::Fill),
        );
        assert!(cfg.assignment_for("\\\\.\\DISPLAY9").is_some());
        cfg.set_assignment(
            "\\\\.\\DISPLAY1",
            Assignment::new(PathBuf::from("C:/b.mp4"), Scaling::Fit),
        );
        assert_eq!(
            cfg.assignment_for("\\\\.\\DISPLAY1").unwrap().path,
            PathBuf::from("C:/b.mp4")
        );
    }

    #[test]
    fn favorites_toggle() {
        let mut cfg = Config::default();
        let p = PathBuf::from("C:/x.mp4");
        cfg.toggle_favorite(&p);
        assert!(cfg.is_favorite(&p));
        cfg.toggle_favorite(&p);
        assert!(!cfg.is_favorite(&p));
    }
}

