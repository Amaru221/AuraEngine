//! Persistent user configuration.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::platform::win::MonitorInfo;
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
            Lang::Es => "Español",
            Lang::Fr => "Français",
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

    #[allow(dead_code)]
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
    /// Stable identity of the physical panel (`MonitorInfo::id`). Windows
    /// renumbers device names (`\\.\DISPLAY1` -> `\\.\DISPLAY6`) after a GPU
    /// driver update, so the identity is what allows the assignment to follow
    /// the screen. Empty for configurations written before this field existed.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub monitor_id: String,
}

impl Assignment {
    pub fn new(path: PathBuf, scaling: Scaling) -> Self {
        let kind = MediaKind::from_path(&path);
        Self {
            path,
            kind,
            scaling,
            thumb_key: String::new(),
            monitor_id: String::new(),
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
    pub nav_opacity: u8,
    pub title_opacity: u8,
    pub opacity_linked: bool,
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
            nav_opacity: 96,
            title_opacity: 94,
            opacity_linked: false,
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

/// Parses a configuration file, tolerating the UTF-8 BOM that Notepad and
/// Windows PowerShell add by default. Serde rejects it, which used to discard
/// the whole configuration.
fn parse_config(raw: &str) -> std::result::Result<Config, serde_json::Error> {
    serde_json::from_str(raw.strip_prefix('\u{feff}').unwrap_or(raw))
}

impl Config {
    pub fn load() -> Self {
        let path = util::config_path();
        let raw = match std::fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(_) => return Self::default(),
        };
        match parse_config(&raw) {
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
        self.nav_opacity = self.nav_opacity.clamp(20, 100);
        self.title_opacity = self.title_opacity.clamp(20, 100);
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

    /// Re-links per-monitor assignments after Windows changes the display
    /// device names.
    ///
    /// `\\.\DISPLAYn` is assigned by the OS and changes when a GPU driver is
    /// updated/reinstalled, a monitor is moved to another port or a dock is
    /// connected. The physical panel keeps its PnP identity, so assignments
    /// saved with that identity are moved to the current device name instead
    /// of being forgotten. Returns `true` when the configuration changed and
    /// needs to be saved.
    pub fn reconcile_monitors(&mut self, monitors: &[MonitorInfo]) -> bool {
        let mut changed = false;
        let current_keys: BTreeMap<&str, &MonitorInfo> = monitors
            .iter()
            .map(|monitor| (monitor.key.as_str(), monitor))
            .collect();

        // Existing assignments for a connected screen learn its identity.
        for monitor in monitors {
            if monitor.id.is_empty() {
                continue;
            }
            if let Some(assignment) = self.assignments.get_mut(&monitor.key) {
                if assignment.monitor_id.is_empty() {
                    assignment.monitor_id = monitor.id.clone();
                    changed = true;
                }
            }
        }

        // Move an assignment saved under a previous device name to the device
        // name that currently drives the same panel.
        for monitor in monitors {
            if monitor.id.is_empty() || self.assignments.contains_key(&monitor.key) {
                continue;
            }
            let stale = self
                .assignments
                .iter()
                .find(|(key, assignment)| {
                    !current_keys.contains_key(key.as_str())
                        && key.as_str() != "*"
                        && !assignment.monitor_id.is_empty()
                        && assignment.monitor_id.eq_ignore_ascii_case(&monitor.id)
                })
                .map(|(key, _)| key.clone());
            let Some(stale) = stale else {
                continue;
            };
            if let Some(mut assignment) = self.assignments.remove(&stale) {
                assignment.monitor_id = monitor.id.clone();
                log::info!(
                    "assignment {stale} re-linked to {} ({})",
                    monitor.key,
                    monitor.id
                );
                self.assignments.insert(monitor.key.clone(), assignment);
                if let Some(original) = self.originals.remove(&stale) {
                    self.originals.insert(monitor.key.clone(), original);
                }
                changed = true;
            }
        }

        // Drop leftovers for a panel that is already assigned to a connected
        // device name. Two keys for one screen would draw two display numbers
        // and create duplicate hosts.
        let mut live: BTreeMap<&str, &str> = BTreeMap::new();
        for monitor in monitors {
            if !monitor.id.is_empty() && self.assignments.contains_key(&monitor.key) {
                live.entry(monitor.id.as_str())
                    .or_insert(monitor.key.as_str());
            }
        }
        let duplicates: Vec<String> = self
            .assignments
            .iter()
            .filter(|(key, assignment)| {
                key.as_str() != "*"
                    && !current_keys.contains_key(key.as_str())
                    && !assignment.monitor_id.is_empty()
                    && live.contains_key(assignment.monitor_id.as_str())
            })
            .map(|(key, _)| key.clone())
            .collect();
        for key in duplicates {
            log::info!("dropping duplicate assignment for disconnected {key}");
            self.assignments.remove(&key);
            self.originals.remove(&key);
            changed = true;
        }

        changed
    }

    #[allow(dead_code)]
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
    fn accepts_utf8_bom_from_notepad() {
        let parsed = parse_config("\u{feff}{\"language\":\"fr\"}").expect("BOM config");
        assert_eq!(parsed.language, Lang::Fr);
        assert_eq!(parsed.version, CURRENT_VERSION);
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

    fn monitor(key: &str, id: &str) -> MonitorInfo {
        MonitorInfo {
            key: key.to_string(),
            id: id.to_string(),
            ordinal: 0,
            name: String::new(),
            rect: crate::platform::win::RectI {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
            work: crate::platform::win::RectI {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
            primary: key == r"\\.\DISPLAY6",
            dpi: 96,
        }
    }

    const LG: &str = r"\\?\DISPLAY#GSM5C8A#5&D89A054&1&UID4353";
    const MSI: &str = r"\\?\DISPLAY#MSI3BA4#5&D89A054&1&UID4352";

    #[test]
    fn renumbers_follow_the_panel_identity() {
        let mut cfg = Config::default();
        let mut video = Assignment::new(PathBuf::from("C:/v.mp4"), Scaling::Fill);
        video.monitor_id = LG.to_string();
        cfg.assignments
            .insert(r"\\.\DISPLAY1".to_string(), video);
        cfg.originals
            .insert(r"\\.\DISPLAY1".to_string(), "C:/old.jpg".to_string());

        let monitors = vec![monitor(r"\\.\DISPLAY6", LG), monitor(r"\\.\DISPLAY7", MSI)];
        assert!(cfg.reconcile_monitors(&monitors));

        assert!(!cfg.assignments.contains_key(r"\\.\DISPLAY1"));
        assert_eq!(
            cfg.assignments[r"\\.\DISPLAY6"].path,
            PathBuf::from("C:/v.mp4")
        );
        assert_eq!(cfg.originals[r"\\.\DISPLAY6"], "C:/old.jpg");
        // Already reconciled: nothing left to do.
        assert!(!cfg.reconcile_monitors(&monitors));
    }

    #[test]
    fn legacy_assignments_learn_their_identity() {
        let mut cfg = Config::default();
        cfg.assignments.insert(
            r"\\.\DISPLAY6".to_string(),
            Assignment::new(PathBuf::from("C:/a.jpg"), Scaling::Fill),
        );
        let monitors = vec![monitor(r"\\.\DISPLAY6", LG)];
        assert!(cfg.reconcile_monitors(&monitors));
        assert_eq!(cfg.assignments[r"\\.\DISPLAY6"].monitor_id, LG);
        assert!(!cfg.reconcile_monitors(&monitors));
    }

    #[test]
    fn duplicate_keys_for_one_panel_are_dropped() {
        let mut cfg = Config::default();
        cfg.assignments.insert(
            r"\\.\DISPLAY6".to_string(),
            Assignment::new(PathBuf::from("C:/a.jpg"), Scaling::Fill),
        );
        let mut stale = Assignment::new(PathBuf::from("C:/b.jpg"), Scaling::Fill);
        stale.monitor_id = LG.to_string();
        cfg.assignments.insert(r"\\.\DISPLAY1".to_string(), stale);

        let monitors = vec![monitor(r"\\.\DISPLAY6", LG)];
        assert!(cfg.reconcile_monitors(&monitors));
        assert!(!cfg.assignments.contains_key(r"\\.\DISPLAY1"));
        assert_eq!(
            cfg.assignments[r"\\.\DISPLAY6"].path,
            PathBuf::from("C:/a.jpg")
        );
    }

    #[test]
    fn disconnected_panels_keep_their_assignment() {
        // An unplugged screen must still get its wallpaper back when it
        // returns; only entries that can be re-linked are touched.
        let mut cfg = Config::default();
        let mut away = Assignment::new(PathBuf::from("C:/away.jpg"), Scaling::Fill);
        away.monitor_id = MSI.to_string();
        cfg.assignments
            .insert(r"\\.\DISPLAY2".to_string(), away);

        let monitors = vec![monitor(r"\\.\DISPLAY6", LG)];
        assert!(!cfg.reconcile_monitors(&monitors));
        assert!(cfg.assignments.contains_key(r"\\.\DISPLAY2"));
    }
}

