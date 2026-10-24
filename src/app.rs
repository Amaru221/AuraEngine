//! Application state, orchestration and event handling.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use eframe::egui;
use egui::{Color32, TextureHandle};

use crate::config::{Assignment, Config, MediaKind, Scaling};
use crate::engine::installer::{self, InstallEvent};
use crate::engine::{Engine, PauseEnv};
use crate::hotkeys::{self, HotkeyAction, Hotkeys};
use crate::i18n::{self, Strings};
use crate::online::{self, MotionBgsCategory, Provider, RemoteItem};
use crate::platform::{ipc, static_wallpaper, win};
use crate::scanner::thumbs::{self, ThumbRequest, ThumbResult};
use crate::scanner::{self, MediaItem, ScanEvent};
use crate::theme;
use crate::tray::{self, Tray, TrayAction};
use crate::util;

pub const HUD_COLS: usize = 5;
pub const HUD_ROWS: usize = 3;
pub const HUD_PAGE_SIZE: usize = HUD_COLS * HUD_ROWS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav {
    Library,
    Explore,
    Monitors,
    Settings,
    About,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    All,
    Live,
    Images,
    Favorites,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Error,
}

pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
    pub until: Instant,
}

pub struct ThumbEntry {
    pub texture: TextureHandle,
    pub aspect: f32,
}

pub struct RemoteSearchResult {
    pub provider: Provider,
    pub page: u32,
    pub result: Result<Vec<RemoteItem>, String>,
}

pub struct DownloadResult {
    pub id: String,
    pub path: Result<PathBuf, String>,
    pub apply: bool,
    pub title: String,
}

pub struct AuraApp {
    pub cfg: Config,
    pub strings: &'static Strings,
    pub nav: Nav,

    pub engine: Engine,
    pub monitors: Vec<win::MonitorInfo>,

    pub items: Vec<MediaItem>,
    pub scanning: bool,
    pub scan_tx: Sender<ScanEvent>,
    pub scan_rx: Receiver<ScanEvent>,
    pub query: String,
    pub filter: Filter,

    pub thumb_tx: Sender<ThumbRequest>,
    pub thumb_rx: Receiver<ThumbResult>,
    pub thumbs: HashMap<String, ThumbEntry>,
    pub thumb_requests: HashSet<String>,

    pub remote: Vec<RemoteItem>,
    pub remote_provider: Provider,
    pub motionbgs_category: MotionBgsCategory,
    pub remote_query: String,
    pub remote_page: u32,
    pub remote_loading: bool,
    pub remote_error: Option<String>,
    pub remote_tx: Sender<RemoteSearchResult>,
    pub remote_rx: Receiver<RemoteSearchResult>,

    pub download_tx: Sender<DownloadResult>,
    pub download_rx: Receiver<DownloadResult>,
    pub downloading: HashSet<String>,

    pub install_rx: Option<Receiver<InstallEvent>>,
    pub installing: bool,
    pub install_stage: String,
    pub install_progress: (u64, u64),

    pub accent: Color32,
    pub toast: Option<Toast>,

    pub hud_open: bool,
    pub hud_index: usize,
    pub hud_items: Vec<MediaItem>,
    pub hud_hold: bool,
    pub hud_opened_at: Instant,
    pub hud_page_anim: f32,

    pub tray: Option<Tray>,
    pub hotkeys: Option<Hotkeys>,

    pub last_tick: Instant,
    pub pause_rx: Receiver<PauseEnv>,
    pub dirty: bool,
    pub last_edit: Instant,

    pub hidden: bool,
    /// True once the parked window has lost the foreground at least once.
    /// A later foreground transition is then a real "Switch to" request.
    pub hidden_lost_foreground: bool,
    pub quitting: bool,
    pub startup_done: bool,
    pub initial_action: Option<String>,
    pub main_hwnd: Option<isize>,
    pub parked: Option<bool>,
    pub applied_accent: Color32,
    pub mpv_version: Option<String>,
    pub restore_done: bool,

    pub monitor_scaling: HashMap<String, Scaling>,
    pub retry_counts: HashMap<String, u32>,
    pub confirm_reset: bool,
}

/// State machine for the parked (hidden to tray) window.
///
/// Returns `(restore, lost_foreground)`. A foreground transition only restores
/// the window after it has lost the foreground at least once, so the window's
/// own activation during a `--hidden` startup does not pop it on screen.
fn parked_step(hidden: bool, lost_foreground: bool, is_foreground: bool) -> (bool, bool) {
    if !hidden {
        return (false, false);
    }
    if is_foreground {
        (lost_foreground, false)
    } else {
        (false, true)
    }
}

impl AuraApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        hidden: bool,
        initial_action: Option<String>,
    ) -> Self {
        util::ensure_dirs();
        let first_run = !util::config_path().exists();
        let mut cfg = Config::load();
        if first_run {
            // Adopt the autostart state created by the installer.
            cfg.start_with_windows = win::autostart_enabled();
        }
        let strings = i18n::strings(cfg.language);

        theme::install_fonts(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx, theme::DEFAULT_ACCENT);

        let mut engine = Engine::new(cfg.mpv_path.clone());
        // Probe mpv once. Doing this from the Settings page on every frame
        // spawned a process continuously and pinned a CPU core.
        let mpv_version = engine
            .mpv_path()
            .and_then(crate::engine::mpv::version);
        let monitors = win::monitors();
        let _ = engine.tick(&cfg);

        let (scan_tx, scan_rx) = crossbeam_channel::unbounded();
        let (thumb_tx, thumb_rx) = thumbs::spawn_worker();
        let (remote_tx, remote_rx) = crossbeam_channel::unbounded();
        let (download_tx, download_rx) = crossbeam_channel::unbounded();
        let (pause_tx, pause_rx) = crossbeam_channel::bounded::<PauseEnv>(1);

        // Fullscreen/occlusion/battery detection enumerates every top-level
        // window, so it runs off the UI thread.
        std::thread::Builder::new()
            .name("aura-pause-watch".into())
            .spawn(move || loop {
                std::thread::sleep(Duration::from_millis(700));
                let env = PauseEnv {
                    fullscreen: win::is_fullscreen_app(),
                    occluded: win::monitors()
                        .iter()
                        .any(|monitor| win::is_occluded(monitor.rect)),
                    battery: win::on_battery(),
                };
                let _ = pause_tx.try_send(env);
            })
            .ok();

        let mut app = Self {
            engine,
            monitors,
            items: Vec::new(),
            scanning: true,
            scan_tx,
            scan_rx,
            query: String::new(),
            filter: Filter::All,
            thumb_tx,
            thumb_rx,
            thumbs: HashMap::new(),
            thumb_requests: HashSet::new(),
            remote: Vec::new(),
            remote_provider: Provider::Bing,
            motionbgs_category: MotionBgsCategory::All,
            remote_query: String::new(),
            remote_page: 1,
            remote_loading: false,
            remote_error: None,
            remote_tx,
            remote_rx,
            download_tx,
            download_rx,
            downloading: HashSet::new(),
            install_rx: None,
            installing: false,
            install_stage: String::new(),
            install_progress: (0, 0),
            accent: theme::DEFAULT_ACCENT,
            toast: None,
            hud_open: false,
            hud_index: 0,
            hud_items: Vec::new(),
            hud_hold: false,
            hud_opened_at: Instant::now(),
            hud_page_anim: 0.0,
            tray: None,
            hotkeys: hotkeys::create(),
            last_tick: Instant::now(),
            pause_rx,
            dirty: false,
            last_edit: Instant::now(),
            hidden,
            hidden_lost_foreground: false,
            quitting: false,
            startup_done: false,
            initial_action,
            main_hwnd: None,
            parked: None,
            applied_accent: theme::DEFAULT_ACCENT,
            mpv_version,
            restore_done: false,
            monitor_scaling: HashMap::new(),
            retry_counts: HashMap::new(),
            confirm_reset: false,
            cfg,
            strings,
            nav: Nav::Library,
        };

        app.tray = tray::create(app.strings);

        // Keep the event loop alive while the window is hidden in the tray.
        let repaint_ctx = cc.egui_ctx.clone();
        std::thread::Builder::new()
            .name("aura-ticker".into())
            .spawn(move || loop {
                std::thread::sleep(Duration::from_millis(200));
                repaint_ctx.request_repaint();
            })
            .ok();

        app.rescan();
        app.search_online(true);

        if app.cfg.start_with_windows != win::autostart_enabled() {
            let _ = win::set_autostart(app.cfg.start_with_windows);
        }

        app
    }

    // -----------------------------------------------------------------------
    // Persistence & toasts
    // -----------------------------------------------------------------------

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
        self.last_edit = Instant::now();
    }

    pub fn save_now(&mut self) {
        if let Err(err) = self.cfg.save() {
            log::error!("save config: {err:#}");
        }
        self.dirty = false;
    }

    pub fn toast(&mut self, text: impl Into<String>, kind: ToastKind) {
        self.toast = Some(Toast {
            text: text.into(),
            kind,
            until: Instant::now() + Duration::from_secs(4),
        });
    }

    pub fn info(&mut self, text: impl Into<String>) {
        self.toast(text, ToastKind::Info);
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.toast(text, ToastKind::Error);
    }

    pub fn set_language(&mut self, lang: crate::config::Lang) {
        self.cfg.language = lang;
        self.strings = i18n::strings(lang);
        self.tray = tray::create(self.strings);
        self.mark_dirty();
    }

    // -----------------------------------------------------------------------
    // Library
    // -----------------------------------------------------------------------

    pub fn rescan(&mut self) {
        self.scanning = true;
        scanner::scan_async(self.cfg.library_dirs.clone(), self.scan_tx.clone());
    }

    /// Filtered indices into `self.items`. Returning indices avoids cloning
    /// the whole library on every frame.
    pub fn filtered_indices(&self) -> Vec<usize> {
        let query = self.query.trim().to_lowercase();
        self.items
            .iter()
            .enumerate()
            .filter(|(_, item)| match self.filter {
                Filter::All => true,
                Filter::Live => item.kind == MediaKind::Video,
                Filter::Images => item.kind == MediaKind::Image,
                Filter::Favorites => self.cfg.is_favorite(&item.path),
            })
            .filter(|(_, item)| {
                query.is_empty()
                    || item.name.to_lowercase().contains(&query)
                    || item.folder.to_lowercase().contains(&query)
            })
            .map(|(index, _)| index)
            .collect()
    }

    /// Full filtered snapshot. Used when opening the quick switcher.
    pub fn filtered_items(&self) -> Vec<MediaItem> {
        self.filtered_indices()
            .into_iter()
            .map(|index| self.items[index].clone())
            .collect()
    }

    // -----------------------------------------------------------------------
    // Thumbnails
    // -----------------------------------------------------------------------

    pub fn thumb_key_for(item: &MediaItem) -> String {
        item.thumb_key.clone()
    }

    pub fn request_thumb(&mut self, item: &MediaItem) {
        self.request_thumb_by_key(item.thumb_key.clone(), &item.path, item.kind);
    }

    pub fn request_thumb_by_key(&mut self, key: String, path: &Path, kind: MediaKind) {
        if key.is_empty()
            || self.thumbs.contains_key(&key)
            || !self.thumb_requests.insert(key.clone())
        {
            return;
        }
        let _ = self.thumb_tx.send(ThumbRequest::Local {
            key,
            path: path.to_path_buf(),
            kind,
            mpv: self.engine.mpv_path().map(|p| p.to_path_buf()),
        });
    }

    pub fn request_remote_thumb(&mut self, item: &RemoteItem) {
        let key = item.thumb_key();
        if self.thumbs.contains_key(&key) || !self.thumb_requests.insert(key.clone()) {
            return;
        }
        let _ = self.thumb_tx.send(ThumbRequest::Remote {
            key,
            url: item.thumb_url.clone(),
        });
    }

    fn poll_thumbs(&mut self, ctx: &egui::Context) {
        while let Ok(result) = self.thumb_rx.try_recv() {
            self.thumb_requests.remove(&result.key);
            let Some(raw) = result.image else {
                continue;
            };
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [raw.width.max(1), raw.height.max(1)],
                &raw.rgba,
            );
            let texture =
                ctx.load_texture(result.key.clone(), image, egui::TextureOptions::LINEAR);
            let aspect = raw.width as f32 / raw.height.max(1) as f32;
            self.thumbs.insert(
                result.key,
                ThumbEntry { texture, aspect },
            );
        }
    }

    // -----------------------------------------------------------------------
    // Online catalogs
    // -----------------------------------------------------------------------

    pub fn search_online(&mut self, reset: bool) {
        if self.remote_loading {
            return;
        }
        if reset {
            self.remote_page = 1;
            self.remote.clear();
        }
        self.remote_loading = true;
        self.remote_error = None;

        let provider = self.remote_provider;
        let category = self.motionbgs_category;
        let query = self.remote_query.clone();
        let page = self.remote_page;
        let key = if provider == Provider::Pexels {
            self.cfg.pexels_api_key.clone()
        } else {
            self.cfg.wallhaven_api_key.clone()
        };
        let tx = self.remote_tx.clone();

        std::thread::Builder::new()
            .name("aura-online".into())
            .spawn(move || {
                let result = online::search(provider, &query, page, &key, category)
                    .map_err(|e| format!("{e:#}"));
                let _ = tx.send(RemoteSearchResult {
                    provider,
                    page,
                    result,
                });
            })
            .ok();
    }

    pub fn download_remote(&mut self, item: RemoteItem, apply: bool) {
        if !self.downloading.insert(item.id.clone()) {
            return;
        }
        let tx = self.download_tx.clone();
        let dest = util::online_dir();
        let id = item.id.clone();
        let title = item.title.clone();
        std::thread::Builder::new()
            .name("aura-download".into())
            .spawn(move || {
                let path = online::download(&item, &dest, None).map_err(|e| format!("{e:#}"));
                let _ = tx.send(DownloadResult {
                    id,
                    path,
                    apply,
                    title,
                });
            })
            .ok();
    }

    // -----------------------------------------------------------------------
    // Engine / wallpaper application
    // -----------------------------------------------------------------------

    pub fn apply_media(&mut self, path: &Path, target: Option<String>) {
        if !path.exists() {
            self.error(format!("{}: file not found", path.display()));
            return;
        }
        let kind = MediaKind::from_path(path);
        let item = scanner::item_for(path);
        let keys: Vec<String> = match target.as_deref() {
            Some(key) if key != "*" && !key.is_empty() => vec![key.to_string()],
            _ => self.monitors.iter().map(|m| m.key.clone()).collect(),
        };
        if keys.is_empty() {
            self.error("No displays detected");
            return;
        }

        let mut failures = Vec::new();
        let mut applied_any = false;
        for key in &keys {
            let Some(index) = self
                .monitors
                .iter()
                .position(|m| &m.key == key)
            else {
                continue;
            };
            let scaling = self
                .cfg
                .assignment_for(key)
                .map(|a| a.scaling)
                .unwrap_or(Scaling::Fill);

            // Remember the static wallpaper that was active before Aura.
            if !self.cfg.originals.contains_key(key) {
                if let Ok(wall) = static_wallpaper::get_wallpaper(Some(index)) {
                    if !wall.is_empty() && Path::new(&wall) != path {
                        self.cfg.originals.insert(key.clone(), wall);
                    }
                }
            }

            let success = match kind {
                MediaKind::Video => {
                    match self.engine.start_video(
                        key,
                        path,
                        scaling,
                        self.cfg.volume,
                        self.cfg.muted,
                    ) {
                        Ok(()) => {
                            self.retry_counts.remove(key);
                            true
                        }
                        Err(err) => {
                            let message = format!("{err:#}");
                            log::error!("start video on {key}: {message}");
                            if message.contains("MPV_MISSING") {
                                failures.push(self.strings.mpv_not_found.to_string());
                            } else {
                                failures.push(message);
                            }
                            false
                        }
                    }
                }
                MediaKind::Image => {
                    self.engine.stop_monitor(key);
                    match static_wallpaper::set_wallpaper(Some(index), path, scaling) {
                        Ok(()) => {
                            self.retry_counts.remove(key);
                            true
                        }
                        Err(err) => {
                            log::error!("set static wallpaper on {key}: {err:#}");
                            failures.push(format!("{err:#}"));
                            false
                        }
                    }
                }
            };

            if success {
                let mut assignment = Assignment::new(path.to_path_buf(), scaling);
                if let Some(item) = &item {
                    assignment.thumb_key = item.thumb_key.clone();
                }
                self.cfg.set_assignment(key, assignment);
                applied_any = true;
            } else {
                // Never leave a broken assignment behind.
                self.cfg.clear_assignment(key);
            }
        }

        if applied_any {
            self.cfg.last_applied = Some(path.to_path_buf());
        }
        self.mark_dirty();

        if let Some(err) = failures.first() {
            self.error(err.clone());
        } else {
            self.info(self.strings.toast_applied);
        }

        // A remote/home video becomes part of the library immediately.
        if let Some(item) = item {
            if !self.items.iter().any(|i| i.path == item.path) {
                self.items.insert(0, item);
            }
        }

        if applied_any {
            self.update_accent_from(path);
        }
    }

    pub fn apply_to_monitor(&mut self, path: &Path, key: String) {
        self.apply_media(path, Some(key));
    }

    /// "Pantalla 1" / "Display 1", using the Windows display number so it
    /// matches Settings -> Display -> Identify.
    pub fn monitor_title(&self, monitor: &win::MonitorInfo) -> String {
        match monitor.number() {
            Some(number) => format!("{} {number}", self.strings.display),
            None => monitor.label(),
        }
    }

    /// Numbered title plus the friendly adapter name and resolution.
    pub fn monitor_menu_label(&self, monitor: &win::MonitorInfo) -> String {
        let title = self.monitor_title(monitor);
        let name = monitor.label();
        if name == title {
            format!("{title} ({})", monitor.resolution())
        } else {
            format!("{title} - {name} ({})", monitor.resolution())
        }
    }

    pub fn stop_all(&mut self) {
        self.engine.stop_all();
        self.restore_originals();
        self.cfg.originals.clear();
        self.cfg.assignments.clear();
        self.mark_dirty();
        self.info(self.strings.toast_stopped);
    }

    pub fn restore_originals(&mut self) {
        for (key, path) in self.cfg.originals.clone() {
            if let Some(index) = self.monitors.iter().position(|m| m.key == key) {
                if Path::new(&path).exists() {
                    let _ = static_wallpaper::set_wallpaper(Some(index), Path::new(&path), Scaling::Fill);
                }
            }
        }
    }

    fn restore_assignments(&mut self) {
        // Drop assignments whose files disappeared (deleted downloads, temp
        // files, moved library folders) instead of creating black hosts.
        let missing: Vec<String> = self
            .cfg
            .assignments
            .iter()
            .filter(|(_, assignment)| !assignment.path.exists())
            .map(|(key, _)| key.clone())
            .collect();
        for key in missing {
            log::warn!("removing missing assignment for {key}");
            self.cfg.assignments.remove(&key);
            self.mark_dirty();
        }
        if self
            .cfg
            .last_applied
            .as_ref()
            .is_some_and(|path| !path.exists())
        {
            self.cfg.last_applied = None;
            self.mark_dirty();
        }

        let monitors = self.monitors.clone();
        for monitor in &monitors {
            if let Some(assignment) = self.cfg.assignment_for(&monitor.key).cloned() {
                if assignment.path.exists() {
                    self.apply_media(&assignment.path, Some(monitor.key.clone()));
                } else {
                    log::warn!("missing assigned wallpaper: {}", assignment.path.display());
                    self.cfg.clear_assignment(&monitor.key);
                    self.mark_dirty();
                }
            }
        }
    }

    fn reapply_changed(&mut self, keys: &[String]) {
        if keys.is_empty() {
            return;
        }
        for key in keys {
            let Some(assignment) = self.cfg.assignment_for(key).cloned() else {
                continue;
            };
            // Static images belong to Explorer: re-applying them would make
            // Explorer rebuild WorkerW and start an endless loop.
            if assignment.kind != MediaKind::Video {
                continue;
            }
            if !assignment.path.exists() {
                continue;
            }
            if self.engine.running().iter().any(|(k, _, _)| k == key) {
                continue;
            }
            let attempts = self.retry_counts.entry(key.clone()).or_insert(0);
            if *attempts >= 3 {
                continue;
            }
            *attempts += 1;
            self.apply_media(&assignment.path, Some(key.clone()));
        }
    }

    pub fn toggle_pause(&mut self) {
        let paused = self.engine.toggle_user_paused();
        if paused {
            self.info(self.strings.toast_paused);
        } else {
            self.info(self.strings.toast_resumed);
        }
    }

    pub fn next_wallpaper(&mut self, delta: isize) {
        if self.items.is_empty() {
            self.error(self.strings.empty_library);
            return;
        }
        let current = self
            .cfg
            .last_applied
            .as_ref()
            .and_then(|current| self.items.iter().position(|i| &i.path == current));
        let len = self.items.len() as isize;
        let index = match current {
            Some(index) => (index as isize + delta).rem_euclid(len) as usize,
            None => 0,
        };
        let path = self.items[index].path.clone();
        self.apply_media(&path, None);
    }

    pub fn update_accent_from(&mut self, path: &Path) {
        if !self.cfg.accent_from_wallpaper {
            return;
        }
        let accent = theme::accent_from_path(path).map(|(color, _)| color).or_else(|| {
            let item = self.items.iter().find(|i| i.path == path)?;
            let key = Self::thumb_key_for(item);
            let thumb = util::thumbs_dir().join(format!("{key}.jpg"));
            theme::accent_from_path(&thumb).map(|(color, _)| color)
        });
        if let Some(accent) = accent {
            self.set_accent(accent);
        }
    }

    fn update_accent_from_last(&mut self) {
        if let Some(path) = self.cfg.last_applied.clone() {
            self.update_accent_from(&path);
        }
    }

    pub fn set_accent(&mut self, accent: Color32) {
        self.accent = accent;
        if self.cfg.apply_windows_accent {
            let [r, g, b, _] = accent.to_array();
            if let Err(err) = win::apply_windows_accent(r, g, b) {
                log::warn!("accent: {err:#}");
            }
        }
    }

    pub fn start_mpv_install(&mut self) {
        if self.installing {
            return;
        }
        let (tx, rx) = crossbeam_channel::unbounded();
        self.install_rx = Some(rx);
        self.installing = true;
        self.install_stage = self.strings.downloading_mpv.to_string();
        self.install_progress = (0, 0);
        std::thread::Builder::new()
            .name("aura-mpv-install".into())
            .spawn(move || installer::install_mpv(tx))
            .ok();
    }

    fn poll_install(&mut self) {
        let events: Vec<InstallEvent> = match &self.install_rx {
            Some(rx) => {
                let mut events = Vec::new();
                while let Ok(event) = rx.try_recv() {
                    events.push(event);
                }
                events
            }
            None => return,
        };
        let mut done = false;
        for event in events {
            match event {
                InstallEvent::Stage(stage) => self.install_stage = stage,
                InstallEvent::Progress { downloaded, total } => {
                    self.install_progress = (downloaded, total)
                }
                InstallEvent::Done(path) => {
                    self.cfg.mpv_path = Some(path);
                    self.engine.refresh_mpv(self.cfg.mpv_path.clone());
                    self.mpv_version = self
                        .engine
                        .mpv_path()
                        .and_then(crate::engine::mpv::version);
                    self.installing = false;
                    self.install_rx = None;
                    self.mark_dirty();
                    self.info(self.strings.toast_mpv_installed);
                    done = true;
                }
                InstallEvent::Failed(error) => {
                    self.installing = false;
                    self.install_rx = None;
                    self.error(format!("{}: {error}", self.strings.download_failed));
                    done = true;
                }
            }
        }
        if done {
            self.install_rx = None;
        }
    }

    // -----------------------------------------------------------------------
    // Commands (IPC / tray / hotkeys)
    // -----------------------------------------------------------------------

    pub fn handle_command(&mut self, command: &str) {
        let command = command.trim();
        if let Some(category) = command.strip_prefix("motionbgs|") {
            self.remote_provider = Provider::MotionBgs;
            self.motionbgs_category =
                MotionBgsCategory::from_id(category).unwrap_or(MotionBgsCategory::All);
            self.remote_query.clear();
            self.nav = Nav::Explore;
            self.show_window();
            self.search_online(true);
            return;
        }

        if let Some(rest) = command.strip_prefix("apply|") {
            let mut parts = rest.splitn(2, '|');
            let path = parts.next().unwrap_or_default();
            let target = parts.next().unwrap_or("*");
            if !path.is_empty() {
                self.apply_media(Path::new(path), Some(target.to_string()));
            }
            return;
        }

        match command {
            "next" => self.next_wallpaper(1),
            "prev" => self.next_wallpaper(-1),
            "pause" => {
                self.engine.set_user_paused(true);
                self.info(self.strings.toast_paused);
            }
            "resume" => {
                self.engine.set_user_paused(false);
                self.info(self.strings.toast_resumed);
            }
            "toggle-pause" => self.toggle_pause(),
            "stop" => self.stop_all(),
            "rescan" => {
                self.rescan();
                self.info(self.strings.scanning);
            }
            "switcher" => self.open_switcher(),
            "show" => self.show_window(),
            "settings" => {
                self.nav = Nav::Settings;
                self.show_window();
            }
            "library" => {
                self.nav = Nav::Library;
                self.show_window();
            }
            "explore" => {
                self.nav = Nav::Explore;
                self.show_window();
            }
            "monitors" => {
                self.nav = Nav::Monitors;
                self.show_window();
            }
            "about" => {
                self.nav = Nav::About;
                self.show_window();
            }
            "quit" => self.request_quit(),
            other => log::warn!("unknown command: {other}"),
        }
    }

    pub fn show_window(&mut self) {
        self.hidden = false;
        self.hidden_lost_foreground = false;
    }

    pub fn open_switcher(&mut self) {
        self.open_switcher_inner(false);
    }

    /// Hold-to-show variant used by Ctrl+Alt+W: releasing the combo closes it.
    pub fn open_switcher_hold(&mut self) {
        if self.hud_open {
            self.hud_open = false;
            self.hud_hold = false;
            return;
        }
        self.open_switcher_inner(true);
    }

    fn open_switcher_inner(&mut self, hold: bool) {
        if self.hud_open {
            self.hud_open = false;
            self.hud_hold = false;
            return;
        }
        let items = self.filtered_items();
        if items.is_empty() {
            self.error(self.strings.hud_empty);
            return;
        }
        self.hud_index = self
            .cfg
            .last_applied
            .as_ref()
            .and_then(|last| items.iter().position(|i| &i.path == last))
            .unwrap_or(0);
        self.hud_page_anim = (self.hud_index / HUD_PAGE_SIZE) as f32;
        self.hud_items = items;
        self.hud_hold = hold;
        self.hud_opened_at = Instant::now();
        self.hud_open = true;
    }

    pub fn request_quit(&mut self) {
        self.quitting = true;
    }

    fn poll_tray(&mut self) {
        for action in tray::poll() {
            match action {
                TrayAction::Show => self.show_window(),
                TrayAction::Next => self.next_wallpaper(1),
                TrayAction::Prev => self.next_wallpaper(-1),
                TrayAction::TogglePause => self.toggle_pause(),
                TrayAction::Switcher => self.open_switcher(),
                TrayAction::Stop => self.stop_all(),
                TrayAction::Quit => self.request_quit(),
            }
        }
    }

    fn poll_hotkeys(&mut self) {
        let Some(hotkeys) = &self.hotkeys else {
            return;
        };
        let actions = hotkeys::poll(hotkeys);
        for action in actions {
            match action {
                HotkeyAction::Switcher => self.open_switcher_hold(),
                HotkeyAction::Next => self.next_wallpaper(1),
                HotkeyAction::Prev => self.next_wallpaper(-1),
                HotkeyAction::TogglePause => self.toggle_pause(),
                HotkeyAction::Stop => self.stop_all(),
            }
        }
    }

    fn poll_ipc(&mut self) {
        for command in ipc::poll() {
            self.handle_command(&command);
        }
    }

    // -----------------------------------------------------------------------
    // Periodics
    // -----------------------------------------------------------------------

    fn tick_engine(&mut self) {
        if self.last_tick.elapsed() < Duration::from_millis(1200) {
            return;
        }
        self.last_tick = Instant::now();

        let changed = self.engine.tick(&self.cfg);

        let monitors = win::monitors();
        if monitors.len() != self.monitors.len()
            || monitors
                .iter()
                .zip(self.monitors.iter())
                .any(|(a, b)| a.key != b.key || a.rect != b.rect)
        {
            self.monitors = monitors;
            // Keep per-monitor scaling selections in sync with reality.
            for monitor in &self.monitors {
                self.monitor_scaling
                    .entry(monitor.key.clone())
                    .or_insert_with(|| {
                        self.cfg
                            .assignment_for(&monitor.key)
                            .map(|a| a.scaling)
                            .unwrap_or(Scaling::Fill)
                    });
            }
        }

        if !changed.is_empty() {
            self.reapply_changed(&changed);
        }
    }

    fn poll_pause(&mut self) {
        // The watcher thread does the expensive window enumeration; the UI
        // only applies the latest snapshot.
        while let Ok(mut env) = self.pause_rx.try_recv() {
            if self.hud_open {
                env.fullscreen = false;
            }
            self.engine.set_env(env, &self.cfg);
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for file in dropped {
            if let Some(path) = file.path {
                if crate::config::is_supported_media(&path) {
                    self.apply_media(&path, None);
                }
            }
        }
    }

    fn handle_close_request(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) {
            // The X button leaves Aura in the tray, but Task Manager's
            // "End task" (and a system shutdown) must actually close it.
            // When the close request arrives while another window is in the
            // foreground, treat it as a system close and exit.
            let foreground_is_ours = self
                .main_hwnd
                .map(|hwnd| {
                    win::is_foreground(windows::Win32::Foundation::HWND(hwnd as *mut _))
                })
                .unwrap_or(false);

            if self.cfg.close_to_tray && !self.quitting && foreground_is_ours {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.hidden = true;
                self.hidden_lost_foreground = false;
            } else {
                self.quitting = true;
            }
        }
    }

    fn apply_window_state(&mut self, frame: &eframe::Frame) {
        if self.main_hwnd.is_none() {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(handle) = frame.window_handle() {
                if let RawWindowHandle::Win32(win32) = handle.as_raw() {
                    self.main_hwnd = Some(win32.hwnd.get());
                }
            }
        }
        // Task Manager's "Switch to" brings the parked window to the
        // foreground; bring it back on-screen when that happens. Ignore the
        // window's own startup activation: only a foreground transition that
        // happens after the parked window lost the foreground counts.
        if self.hidden {
            let is_fg = self
                .main_hwnd
                .map(|hwnd| win::is_foreground(windows::Win32::Foundation::HWND(hwnd as *mut _)))
                .unwrap_or(false);
            let (restore, lost) = parked_step(true, self.hidden_lost_foreground, is_fg);
            self.hidden_lost_foreground = lost;
            if restore {
                self.hidden = false;
            }
        }

        let want_parked = self.hidden;
        if self.parked != Some(want_parked) {
            if let Some(hwnd) = self.main_hwnd {
                win::park_window(
                    windows::Win32::Foundation::HWND(hwnd as *mut _),
                    want_parked,
                );
                self.parked = Some(want_parked);
            }
        }
    }
}

impl eframe::App for AuraApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.startup_done {
            self.startup_done = true;
            if self.cfg.start_hidden {
                self.hidden = true;
                self.hidden_lost_foreground = false;
            }
        }

        if !self.restore_done {
            self.restore_done = true;
            self.restore_assignments();
            self.update_accent_from_last();
        }

        // Defer match-based commands until the library has been scanned.
        if let Some(action) = self.initial_action.clone() {
            let needs_library = action == "next" || action == "prev";
            if !needs_library || !self.scanning {
                self.initial_action = None;
                self.handle_command(&action);
            }
        }

        self.poll_ipc();
        self.poll_tray();
        self.poll_hotkeys();
        self.poll_thumbs(ctx);
        self.poll_install();
        self.handle_dropped_files(ctx);
        self.handle_close_request(ctx);

        while let Ok(event) = self.scan_rx.try_recv() {
            match event {
                ScanEvent::Progress => {}
                ScanEvent::Done(items) => {
                    self.items = items;
                    self.scanning = false;
                }
                ScanEvent::Failed(error) => {
                    self.scanning = false;
                    self.error(error);
                }
            }
        }

        while let Ok(result) = self.remote_rx.try_recv() {
            if result.provider != self.remote_provider {
                continue;
            }
            self.remote_loading = false;
            match result.result {
                Ok(mut items) => {
                    if result.page <= 1 {
                        self.remote.clear();
                    }
                    self.remote.append(&mut items);
                    self.remote_page = result.page;
                }
                Err(error) => {
                    self.remote_error = Some(error);
                }
            }
        }

        while let Ok(result) = self.download_rx.try_recv() {
            match &result.path {
                Ok(path) => {
                    let path = path.clone();
                    self.info(format!("{}: {}", self.strings.toast_download_done, result.title));
                    if let Some(item) = scanner::item_for(&path) {
                        if !self.items.iter().any(|i| i.path == item.path) {
                            self.items.insert(0, item);
                        }
                    }
                    if result.apply {
                        self.apply_media(&path, None);
                    }
                }
                Err(error) => {
                    self.error(format!("{}: {error}", self.strings.download_failed));
                }
            }
            self.downloading.remove(&result.id);
        }

        self.tick_engine();
        self.poll_pause();

        if self.accent != self.applied_accent {
            theme::apply(ctx, self.accent);
            self.applied_accent = self.accent;
        }

        if self.hud_open
            && self.hud_hold
            && self.hud_opened_at.elapsed() > Duration::from_millis(250)
            && !win::switcher_combo_held()
        {
            self.hud_open = false;
            self.hud_hold = false;
        }

        if self.hud_open {
            self.hud_viewport(ctx);
        }

        super::ui::main_ui(self, ctx);
        self.apply_window_state(_frame);

        if self.dirty && self.last_edit.elapsed() > Duration::from_millis(800) {
            self.save_now();
        }

        if self.quitting {
            log::info!("quit: stopping engine");
            self.engine.shutdown();
            log::info!("quit: engine stopped");
            if self.cfg.restore_on_exit {
                self.restore_originals();
            }
            self.save_now();
            log::info!("quit: exiting");
            std::process::exit(0);
        }

        if let Some(toast) = &self.toast {
            if Instant::now() > toast.until {
                self.toast = None;
            }
        }

        // Keep the UI smooth while visible, and only poll occasionally while
        // parked in the tray.
        let interval = if self.hidden { 250 } else { 60 };
        ctx.request_repaint_after(Duration::from_millis(interval));
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.04, 0.043, 0.06, 1.0]
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.engine.shutdown();
        if self.cfg.restore_on_exit {
            self.restore_originals();
        }
        self.save_now();
    }
}

impl Drop for AuraApp {
    fn drop(&mut self) {
        self.engine.shutdown();
        let _ = self.cfg.save();
    }
}

pub fn run(hidden: bool, action: Option<String>) -> anyhow::Result<()> {
    let icon = egui::IconData {
        rgba: tray::icon_rgba(),
        width: 32,
        height: 32,
    };

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Aura Engine")
        .with_inner_size([1240.0, 800.0])
        .with_min_inner_size([980.0, 620.0])
        .with_icon(Arc::new(icon))
        .with_visible(true);
    if hidden {
        // Start off-screen; the window stays "visible" so eframe keeps running.
        viewport = viewport.with_position([-32000.0, -32000.0]);
    }

    let options = eframe::NativeOptions {
        viewport,
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        "Aura Engine",
        options,
        Box::new(move |cc| Ok(Box::new(AuraApp::new(cc, hidden, action)))),
    )
    .map_err(|err| anyhow::anyhow!("{err}"))
}


#[cfg(test)]
mod tests {
    use super::parked_step;

    #[test]
    fn hidden_startup_activation_does_not_restore() {
        // The window is activated while starting with --hidden; keep it parked.
        assert_eq!(parked_step(true, false, true), (false, false));
    }

    #[test]
    fn background_startup_allows_a_later_switch_to() {
        assert_eq!(parked_step(true, false, false), (false, true));
        // Task Manager's "Switch to" focuses the parked window.
        assert_eq!(parked_step(true, true, true), (true, false));
    }

    #[test]
    fn hidden_window_stays_parked_while_in_background() {
        assert_eq!(parked_step(true, true, false), (false, true));
    }

    #[test]
    fn visible_window_never_restores_from_parked() {
        assert_eq!(parked_step(false, true, true), (false, false));
        assert_eq!(parked_step(false, false, false), (false, false));
    }
}
