//! Application state, orchestration and event handling.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
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
use crate::online::{self, MotionBgsCategory, Provider, RemoteItem, SearchError};
use crate::platform::{ipc, static_wallpaper, win};
use crate::update::{self, UpdateEvent, UpdateInfo};
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
    /// Monotonic use counter, used to evict textures that have not been
    /// visible for a while instead of keeping every decoded frame forever.
    pub last_used: u64,
}

pub struct RemoteSearchResult {
    pub page: u32,
    pub seq: u64,
    pub result: Result<Vec<RemoteItem>, SearchError>,
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
    /// Cached result of `(query, filter, items, favorites)`. Rebuilt only when
    /// one of those inputs changes, not on every frame.
    filtered: Arc<Vec<usize>>,
    filter_key: (String, Filter, usize, usize),
    filter_dirty: bool,
    /// O(1) favorite lookups for cards and filters.
    pub favorites: HashSet<PathBuf>,

    pub thumb_tx: Sender<ThumbRequest>,
    pub thumb_rx: Receiver<ThumbResult>,
    pub thumbs: HashMap<String, ThumbEntry>,
    pub thumb_requests: HashSet<String>,
    thumb_tick: u64,

    pub remote: Arc<Vec<RemoteItem>>,
    pub remote_provider: Provider,
    pub motionbgs_category: MotionBgsCategory,
    pub remote_query: String,
    pub remote_page: u32,
    pub remote_loading: bool,
    pub remote_error: Option<SearchError>,
    pub remote_tx: Sender<RemoteSearchResult>,
    pub remote_rx: Receiver<RemoteSearchResult>,
    remote_seq: u64,
    /// Explore is fetched on first visit instead of during startup.
    pub explore_loaded: bool,

    pub download_tx: Sender<DownloadResult>,
    pub download_rx: Receiver<DownloadResult>,
    pub downloading: HashSet<String>,

    pub install_rx: Option<Receiver<InstallEvent>>,
    pub installing: bool,
    pub install_stage: String,
    pub install_progress: (u64, u64),

    pub accent: Color32,
    /// Wallpaper accent extraction runs off the UI thread; `accent_seq`
    /// discards results for wallpapers that were replaced in the meantime.
    accent_tx: Sender<(u64, Option<Color32>)>,
    accent_rx: Receiver<(u64, Option<Color32>)>,
    accent_seq: u64,
    /// Bitmask shared with the pause watcher: 1 fullscreen, 2 occlusion,
    /// 4 battery. Disabled checks are never queried.
    pause_flags: Arc<AtomicU8>,
    pub toast: Option<Toast>,
    /// Startup wallpaper restores emit "wallpaper applied" toasts that would
    /// replace the update notice; ignore informational toasts briefly.
    pub suppress_info_until: Option<Instant>,

    pub hud_open: bool,
    pub hud_index: usize,
    pub hud_items: Vec<MediaItem>,
    pub hud_page_anim: f32,
    /// Smoothly animated position of the Cinematic/Dock carousel.
    pub hud_carousel: f32,

    pub update_available: Option<UpdateInfo>,
    pub update_checked: bool,
    pub update_checking: bool,
    pub update_downloading: bool,
    pub update_progress: (u64, u64),
    pub update_ready: Option<PathBuf>,
    pub update_error: Option<String>,
    pub update_manual: bool,
    pub update_notified: bool,
    pub update_toast_at: Option<Instant>,
    pub update_rx: Option<Receiver<UpdateEvent>>,

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
    /// The window is created hidden and revealed only after a couple of painted
    /// frames, which removes the white/transparent flash on startup.
    pub revealed: bool,
    /// Frames fully presented since startup. `clear_color` runs before
    /// `update`, so it reads the number of frames already on screen.
    startup_frames: u8,
    /// Set once `theme::apply` has run with the system theme already known.
    /// Before that, egui may resolve the other (default) theme slot.
    theme_synced: bool,
    pub initial_action: Option<String>,
    pub main_hwnd: Option<isize>,
    pub parked: Option<bool>,
    pub applied_accent: Color32,
    pub mpv_version: Option<String>,
    mpv_version_rx: Receiver<Option<String>>,
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

/// Global shortcuts are suspended while a fullscreen app is foreground so the
/// game receives its own key combinations. The HUD owns the foreground while
/// it is open, so shortcuts stay enabled then.
fn hotkeys_enabled(fullscreen: bool, hud_open: bool) -> bool {
    !fullscreen || hud_open
}

const PAUSE_FULLSCREEN: u8 = 1;
const PAUSE_OCCLUSION: u8 = 2;
const PAUSE_BATTERY: u8 = 4;

/// Number of frames that must be on screen before revealing the window.
/// Viewport commands are applied before the current frame is painted, so the
/// window shows a buffer that is already painted.
const REVEAL_AFTER_FRAMES: u8 = 3;

/// Frames cleared with the opaque theme background. One extra frame after the
/// reveal guarantees that any buffer DWM composites while showing the window is
/// dark, never the white default surface.
const OPAQUE_CLEAR_FRAMES: u8 = REVEAL_AFTER_FRAMES + 1;

fn pause_flags(cfg: &Config) -> u8 {
    let mut flags = 0;
    if cfg.pause_on_fullscreen {
        flags |= PAUSE_FULLSCREEN;
    }
    if cfg.pause_on_occlusion {
        flags |= PAUSE_OCCLUSION;
    }
    if cfg.pause_on_battery {
        flags |= PAUSE_BATTERY;
    }
    flags
}

/// Assignment that drives `monitor`: the device name, the wildcard fallback or
/// the stable panel identity (after Windows renumbered the device).
fn monitor_assignment<'a>(
    assignments: &'a BTreeMap<String, Assignment>,
    monitor: &win::MonitorInfo,
) -> Option<&'a Assignment> {
    assignments.get(&monitor.key).or_else(|| {
        if monitor.id.is_empty() {
            return assignments.get("*");
        }
        assignments
            .values()
            .find(|assignment| {
                !assignment.monitor_id.is_empty()
                    && assignment.monitor_id.eq_ignore_ascii_case(&monitor.id)
            })
            .or_else(|| assignments.get("*"))
    })
}

/// Windows display numbers where `path` is assigned **on a connected screen**.
///
/// Device names of disconnected monitors are deliberately ignored: after a GPU
/// driver update Windows renumbers the displays, so a stale `\\.\DISPLAY1`
/// entry would render a number for a screen that no longer exists (and could
/// now belong to another panel).
fn assigned_display_numbers(
    assignments: &BTreeMap<String, Assignment>,
    monitors: &[win::MonitorInfo],
    path: &Path,
) -> Vec<u32> {
    let mut numbers = Vec::new();
    for monitor in monitors {
        let Some(assignment) = monitor_assignment(assignments, monitor) else {
            continue;
        };
        if assignment.path.as_path() != path {
            continue;
        }
        if let Some(number) = monitor.number() {
            numbers.push(number);
        }
    }
    numbers.sort_unstable();
    numbers.dedup();
    numbers
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

        // Fill legacy assignments with their thumbnail keys once, instead of
        // calling `metadata` + SHA-256 from the Displays page every frame.
        for assignment in cfg.assignments.values_mut() {
            if assignment.thumb_key.is_empty() && assignment.path.exists() {
                if let Some(item) = scanner::item_for(&assignment.path) {
                    assignment.thumb_key = item.thumb_key;
                }
            }
        }

        let configured_accent = if cfg.accent_from_wallpaper {
            theme::DEFAULT_ACCENT
        } else {
            cfg.accent_color
                .map(|[r, g, b]| Color32::from_rgb(r, g, b))
                .unwrap_or(theme::DEFAULT_ACCENT)
        };

        theme::install_fonts(&cc.egui_ctx);
        theme::set_opacities(
            cfg.window_opacity,
            cfg.nav_opacity,
            cfg.title_opacity,
        );
        theme::set_preset(cfg.theme_preset, cfg.theme_bg);
        theme::apply(&cc.egui_ctx, configured_accent);

        let mut engine = Engine::new(cfg.mpv_path.clone());
        // Probe mpv once, on a worker thread: spawning `mpv --version` is fast
        // but it delayed the first frame (and it used to run every frame from
        // the Settings page, pinning a CPU core).
        let (mpv_version_tx, mpv_version_rx) = crossbeam_channel::bounded(1);
        if let Some(mpv) = engine.mpv_path().map(Path::to_path_buf) {
            std::thread::Builder::new()
                .name("aura-mpv-version".into())
                .spawn(move || {
                    let _ = mpv_version_tx.send(crate::engine::mpv::version(&mpv));
                })
                .ok();
        }
        // Enumerate the desktop once at startup. `Engine::tick` will refresh
        // the list later; calling it here as well created every WorkerW host
        // twice before the first frame.
        let monitors = win::monitors();
        // Windows renumbers display devices after a GPU driver update or a
        // port change. Re-link the saved assignments to the panels before
        // restoring them, otherwise the app forgets which screen had a
        // wallpaper and shows numbers that no longer exist.
        let monitors_reconciled = cfg.reconcile_monitors(&monitors);
        engine.set_monitors(monitors.clone());

        let (scan_tx, scan_rx) = crossbeam_channel::unbounded();
        let (thumb_tx, thumb_rx) = thumbs::spawn_worker();
        let (remote_tx, remote_rx) = crossbeam_channel::unbounded();
        let (download_tx, download_rx) = crossbeam_channel::unbounded();
        let (pause_tx, pause_rx) = crossbeam_channel::bounded::<PauseEnv>(1);
        let (accent_tx, accent_rx) = crossbeam_channel::unbounded();
        let pause_flags = Arc::new(AtomicU8::new(pause_flags(&cfg)));

        // Fullscreen/occlusion/battery detection enumerates every top-level
        // window, so it runs off the UI thread. Disabled checks (and checks
        // that are irrelevant for the current configuration) are skipped
        // entirely instead of enumerating all windows for nothing.
        let watcher_flags = pause_flags.clone();
        std::thread::Builder::new()
            .name("aura-pause-watch".into())
            .spawn(move || loop {
                std::thread::sleep(Duration::from_millis(700));
                let flags = watcher_flags.load(Ordering::Relaxed);
                // Fullscreen is always measured because it also suspends the
                // global shortcuts; occlusion/battery are config-gated.
                let monitors = win::monitors();
                let env = PauseEnv {
                    fullscreen: win::is_fullscreen_app_on(&monitors),
                    occluded: flags & PAUSE_OCCLUSION != 0
                        && win::any_occluded(&monitors),
                    battery: flags & PAUSE_BATTERY != 0 && win::on_battery(),
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
            filtered: Arc::new(Vec::new()),
            filter_key: (String::new(), Filter::All, 0, 0),
            filter_dirty: true,
            favorites: cfg.favorites.iter().cloned().collect(),
            thumb_tx,
            thumb_rx,
            thumbs: HashMap::new(),
            thumb_requests: HashSet::new(),
            thumb_tick: 0,
            remote: Arc::new(Vec::new()),
            remote_provider: Provider::Bing,
            motionbgs_category: MotionBgsCategory::All,
            remote_query: String::new(),
            remote_page: 1,
            remote_loading: false,
            remote_error: None,
            remote_tx,
            remote_rx,
            remote_seq: 0,
            explore_loaded: false,
            download_tx,
            download_rx,
            downloading: HashSet::new(),
            install_rx: None,
            installing: false,
            install_stage: String::new(),
            install_progress: (0, 0),
            accent: configured_accent,
            accent_tx,
            accent_rx,
            accent_seq: 0,
            pause_flags,
            toast: None,
            suppress_info_until: Some(Instant::now() + Duration::from_secs(5)),
            hud_open: false,
            hud_index: 0,
            hud_items: Vec::new(),
            hud_page_anim: 0.0,
            hud_carousel: 0.0,
            update_available: None,
            update_checked: false,
            update_checking: false,
            update_downloading: false,
            update_progress: (0, 0),
            update_ready: None,
            update_error: None,
            update_manual: false,
            update_notified: false,
            update_toast_at: None,
            update_rx: None,
            tray: None,
            hotkeys: hotkeys::create(),
            last_tick: Instant::now(),
            pause_rx,
            dirty: monitors_reconciled,
            last_edit: Instant::now(),
            hidden,
            hidden_lost_foreground: false,
            quitting: false,
            startup_done: false,
            revealed: false,
            startup_frames: 0,
            theme_synced: false,
            initial_action,
            main_hwnd: None,
            parked: None,
            applied_accent: configured_accent,
            mpv_version: None,
            mpv_version_rx,
            restore_done: false,
            monitor_scaling: HashMap::new(),
            retry_counts: HashMap::new(),
            confirm_reset: false,
            cfg,
            strings,
            nav: Nav::Library,
        };

        app.tray = tray::create(app.strings);

        // Low-frequency safety net: keeps tray/IPC polling alive even when a
        // minimized window stops receiving redraw requests. The normal frame
        // pacing below is much cheaper than the old unconditional 5 Hz wakeup.
        let repaint_ctx = cc.egui_ctx.clone();
        std::thread::Builder::new()
            .name("aura-ticker".into())
            .spawn(move || loop {
                std::thread::sleep(Duration::from_millis(1000));
                repaint_ctx.request_repaint();
            })
            .ok();

        // The window is revealed from `update` once a fully themed frame has
        // been presented. Do not force it visible from a timer: on a cold
        // start that raced the first OpenGL frame and flashed the white
        // default window background.

        app.rescan();
        app.init_updates(&cc.egui_ctx);

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
        if let Some(until) = self.suppress_info_until {
            if Instant::now() < until {
                return;
            }
        }
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

    pub fn invalidate_filter(&mut self) {
        self.filter_dirty = true;
    }

    /// Filtered indices into `self.items`, cached between frames. Returning
    /// indices avoids cloning the whole library; the cache means a 25k-item
    /// collection is not re-scanned and re-lower-cased on every repaint.
    pub fn filtered_indices(&mut self) -> Arc<Vec<usize>> {
        let key = (
            self.query.clone(),
            self.filter,
            self.items.len(),
            self.cfg.favorites.len(),
        );
        if self.filter_dirty || self.filter_key != key {
            let query = self.query.trim().to_lowercase();
            let empty = query.is_empty();
            let mut indices = Vec::with_capacity(self.items.len());
            for (index, item) in self.items.iter().enumerate() {
                let kind_ok = match self.filter {
                    Filter::All => true,
                    Filter::Live => item.kind == MediaKind::Video,
                    Filter::Images => item.kind == MediaKind::Image,
                    Filter::Favorites => self.favorites.contains(&item.path),
                };
                if kind_ok && (empty || item.search_key.contains(&query)) {
                    indices.push(index);
                }
            }
            self.filtered = Arc::new(indices);
            self.filter_key = key;
            self.filter_dirty = false;
        }
        self.filtered.clone()
    }

    /// Full filtered snapshot. Used when opening the quick switcher.
    pub fn filtered_items(&mut self) -> Vec<MediaItem> {
        self.filtered_indices()
            .iter()
            .map(|&index| self.items[index].clone())
            .collect()
    }

    /// Display numbers (1, 2, …) where `path` is currently applied. The
    /// library and explore cards use this to show the real multi-monitor
    /// state instead of the single global `last_applied` value.
    pub fn applied_display_numbers(&self, path: &Path) -> Vec<u32> {
        assigned_display_numbers(&self.cfg.assignments, &self.monitors, path)
    }

    /// Assignment that drives `monitor`, resolving device names that Windows
    /// renumbered through the panel identity.
    pub fn assignment_for_monitor(&self, monitor: &win::MonitorInfo) -> Option<&Assignment> {
        monitor_assignment(&self.cfg.assignments, monitor)
    }

    /// True when a **connected** monitor is currently running `path`. Stale
    /// assignments for disconnected screens must not mark a card as applied.
    pub fn is_media_applied(&self, path: &Path) -> bool {
        self.monitors.iter().any(|monitor| {
            monitor_assignment(&self.cfg.assignments, monitor)
                .is_some_and(|assignment| assignment.path.as_path() == path)
        })
    }

    pub fn toggle_favorite(&mut self, path: &Path) {
        self.cfg.toggle_favorite(path);
        if self.favorites.contains(path) {
            self.favorites.remove(path);
        } else {
            self.favorites.insert(path.to_path_buf());
        }
        self.invalidate_filter();
        self.mark_dirty();
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
        if key.is_empty() {
            return;
        }
        self.touch_thumb(&key);
        if self.thumbs.contains_key(&key) || !self.thumb_requests.insert(key.clone()) {
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
        self.touch_thumb(key);
        if self.thumbs.contains_key(key) || !self.thumb_requests.insert(key.to_string()) {
            return;
        }
        let _ = self.thumb_tx.send(ThumbRequest::Remote {
            key: key.to_string(),
            url: item.thumb_url.clone(),
        });
    }

    fn touch_thumb(&mut self, key: &str) {
        self.thumb_tick = self.thumb_tick.wrapping_add(1);
        let tick = self.thumb_tick;
        if let Some(entry) = self.thumbs.get_mut(key) {
            entry.last_used = tick;
        }
    }

    /// Bounds GPU memory: browsing several large catalogs used to keep every
    /// decoded texture alive for the rest of the session.
    fn trim_thumbs(&mut self) {
        const MAX_THUMBS: usize = 160;
        while self.thumbs.len() > MAX_THUMBS {
            let Some(oldest) = self
                .thumbs
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            self.thumbs.remove(&oldest);
        }
    }

    fn poll_thumbs(&mut self, ctx: &egui::Context) {
        // Convert a few textures per frame so a burst of finished thumbnails
        // cannot stall one frame with dozens of uploads.
        let mut budget = 6;
        while budget > 0 {
            let Ok(result) = self.thumb_rx.try_recv() else {
                break;
            };
            budget -= 1;
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
            self.thumb_tick = self.thumb_tick.wrapping_add(1);
            self.thumbs.insert(
                result.key,
                ThumbEntry {
                    texture,
                    aspect,
                    last_used: self.thumb_tick,
                },
            );
        }
        self.trim_thumbs();
    }

    // -----------------------------------------------------------------------
    // Online catalogs
    // -----------------------------------------------------------------------

    /// Catalog search. Cached results are reused when available.
    pub fn search_online(&mut self, reset: bool) {
        self.search_online_opts(reset, false);
    }

    /// Explicit refresh: clears the current page and bypasses the cache.
    pub fn refresh_online(&mut self) {
        self.search_online_opts(true, true);
    }

    /// Manual retry after a failure. Keeps the current page but bypasses the
    /// cache; nothing is retried automatically.
    pub fn retry_online(&mut self) {
        self.search_online_opts(false, true);
    }

    fn search_online_opts(&mut self, reset: bool, force: bool) {
        // A new request is always allowed to supersede an in-flight one.
        // Previously switching providers while a search was loading left
        // `remote_loading` stuck forever and the new grid empty.
        self.remote_seq = self.remote_seq.wrapping_add(1);
        let seq = self.remote_seq;
        if reset {
            self.remote_page = 1;
            self.remote = Arc::new(Vec::new());
        }
        self.remote_loading = true;
        self.remote_error = None;
        self.explore_loaded = true;

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
                let result = online::search(
                    provider, &query, page, &key, category, force,
                );
                let _ = tx.send(RemoteSearchResult { page, seq, result });
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
            let monitor = self.monitors[index].clone();
            let scaling = self
                .cfg
                .assignment_for(key)
                .map(|a| a.scaling)
                .unwrap_or(Scaling::Fill);

            // Remember the static wallpaper that was active before Aura.
            if !self.cfg.originals.contains_key(key) {
                if let Ok(wall) = static_wallpaper::get_wallpaper_for(&monitor, Some(index)) {
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
                    match static_wallpaper::set_wallpaper_for(&monitor, Some(index), path, scaling) {
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
                assignment.monitor_id = monitor.id.clone();
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
                self.invalidate_filter();
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
                    let monitor = self.monitors[index].clone();
                    let _ = static_wallpaper::set_wallpaper_for(
                        &monitor,
                        Some(index),
                        Path::new(&path),
                        Scaling::Fill,
                    );
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
            if let Some(assignment) = monitor_assignment(&self.cfg.assignments, monitor).cloned() {
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

    /// Localized description of why auto-pause is active.
    pub fn pause_reason(&self) -> String {
        let env = self.engine.env();
        let mut parts = Vec::new();
        if env.fullscreen {
            parts.push(self.strings.pause_fullscreen_reason);
        }
        if env.occluded {
            parts.push(self.strings.pause_occluded_reason);
        }
        if env.battery {
            parts.push(self.strings.pause_battery_reason);
        }
        if parts.is_empty() {
            self.strings.pause_none_reason.to_string()
        } else {
            parts.join(", ")
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
        // Decoding a 4K image (or probing a video) took long enough to stall
        // the click that applied the wallpaper, so do it on a worker thread.
        self.accent_seq = self.accent_seq.wrapping_add(1);
        let seq = self.accent_seq;
        let path = path.to_path_buf();
        let thumb = self
            .items
            .iter()
            .find(|item| item.path == path)
            .map(|item| {
                util::thumbs_dir().join(format!("{}.jpg", Self::thumb_key_for(item)))
            });
        let tx = self.accent_tx.clone();
        std::thread::Builder::new()
            .name("aura-accent".into())
            .spawn(move || {
                let accent = theme::accent_from_path(&path)
                    .map(|(color, _)| color)
                    .or_else(|| {
                        let thumb = thumb?;
                        theme::accent_from_path(&thumb).map(|(color, _)| color)
                    });
                let _ = tx.send((seq, accent));
            })
            .ok();
    }

    fn poll_accent(&mut self) {
        while let Ok((seq, accent)) = self.accent_rx.try_recv() {
            if seq == self.accent_seq {
                if let Some(accent) = accent {
                    self.set_accent(accent);
                }
            }
        }
    }

    pub fn update_accent_from_last(&mut self) {
        if let Some(path) = self.cfg.last_applied.clone() {
            self.update_accent_from(&path);
        }
    }

    pub fn set_accent(&mut self, accent: Color32) {
        self.accent = accent;
        if !self.cfg.accent_from_wallpaper {
            let rgb = [accent.r(), accent.g(), accent.b()];
            if self.cfg.accent_color != Some(rgb) {
                self.cfg.accent_color = Some(rgb);
                self.mark_dirty();
            }
        }
        if self.cfg.apply_windows_accent {
            let [r, g, b, _] = accent.to_array();
            if let Err(err) = win::apply_windows_accent(r, g, b) {
                log::warn!("accent: {err:#}");
            }
        }
    }

    /// Re-applies all three opacity values and refreshes the egui visuals.
    pub fn apply_opacities(&mut self, ctx: &egui::Context) {
        theme::set_opacities(
            self.cfg.window_opacity,
            self.cfg.nav_opacity,
            self.cfg.title_opacity,
        );
        theme::apply(ctx, self.accent);
    }

    // -----------------------------------------------------------------------
    // Updates
    // -----------------------------------------------------------------------

    fn init_updates(&mut self, ctx: &egui::Context) {
        if let Some(cache) = update::UpdateCache::load() {
            self.update_checked = true;
            if let Some(info) = cache.info() {
                if update::is_newer(&info.version, env!("CARGO_PKG_VERSION")) {
                    self.update_available = Some(info);
                    // Shown shortly after the first wallpaper restore,
                    // otherwise the "wallpaper applied" toast replaces it.
                    self.update_toast_at =
                        Some(Instant::now() + Duration::from_millis(1500));
                }
            }
            if cache.is_fresh() {
                return;
            }
        }
        self.start_update_check(ctx.clone(), false);
    }

    fn notify_update(&mut self) {
        if self.update_notified {
            return;
        }
        let Some(info) = self.update_available.clone() else {
            return;
        };
        self.update_notified = true;
        self.update_toast_at = None;
        self.toast = Some(Toast {
            text: format!("{}: v{}", self.strings.update_available, info.version),
            kind: ToastKind::Info,
            until: Instant::now() + Duration::from_secs(6),
        });
        // The toast is drawn by the main window; bring it back when Aura was
        // started minimized so the user actually sees the notification.
        if self.hidden {
            self.show_window();
        }
    }

    pub fn start_update_check(&mut self, ctx: egui::Context, manual: bool) {
        if self.update_checking {
            return;
        }
        self.update_checking = true;
        self.update_manual = manual;
        self.update_error = None;
        let (tx, rx) = crossbeam_channel::unbounded();
        self.update_rx = Some(rx);
        let spawned = std::thread::Builder::new()
            .name("aura-update".into())
            .spawn(move || {
                let checked = update::check_latest();
                let _ = tx.send(UpdateEvent::Checked(checked));
                ctx.request_repaint();
            });
        if let Err(err) = spawned {
            self.update_checking = false;
            self.update_error = Some(format!("{err}"));
            if manual {
                self.error(format!("{}: {err}", self.strings.update_failed));
            }
        }
    }

    pub fn start_update_download(&mut self, ctx: egui::Context) {
        if self.update_downloading || self.update_ready.is_some() {
            return;
        }
        let Some(info) = self.update_available.clone() else {
            return;
        };
        self.update_downloading = true;
        self.update_progress = (0, 0);
        self.update_error = None;
        let (tx, rx) = crossbeam_channel::unbounded();
        self.update_rx = Some(rx);
        let progress_tx = tx.clone();
        let spawned = std::thread::Builder::new()
            .name("aura-update-download".into())
            .spawn(move || {
                let downloaded = update::download_and_verify(&info, &progress_tx);
                let _ = tx.send(UpdateEvent::Downloaded(downloaded));
                ctx.request_repaint();
            });
        if let Err(err) = spawned {
            self.update_downloading = false;
            self.update_error = Some(format!("{err}"));
            self.error(format!("{}: {err}", self.strings.update_failed));
        }
    }

    pub fn install_update(&mut self) {
        let Some(path) = self.update_ready.clone() else {
            return;
        };
        if !path.is_file() {
            self.update_ready = None;
            self.error(self.strings.update_failed);
            return;
        }
        match std::process::Command::new(&path).spawn() {
            Ok(_) => {
                log::info!("launching installer: {}", path.display());
                self.info(self.strings.update_install);
                // The installer asks the running instance to quit before it
                // replaces the executable; start winding down now.
                self.request_quit();
            }
            Err(err) => {
                self.error(format!("{}: {err}", self.strings.update_failed));
            }
        }
    }

    fn poll_updates(&mut self, ctx: &egui::Context) {
        let events: Vec<UpdateEvent> = {
            let Some(rx) = self.update_rx.as_ref() else {
                return;
            };
            let mut events = Vec::new();
            while let Ok(event) = rx.try_recv() {
                events.push(event);
            }
            events
        };

        let finished = events
            .iter()
            .any(|event| matches!(event, UpdateEvent::Checked(_) | UpdateEvent::Downloaded(_)));
        if finished {
            self.update_rx = None;
        }

        for event in events {
            match event {
                UpdateEvent::Checked(Ok(Some(info))) => {
                    self.update_checking = false;
                    self.update_checked = true;
                    self.update_available = Some(info.clone());
                    self.update_ready = None;
                    self.update_downloading = false;
                    update::UpdateCache::record(Some(&info)).save();
                    if !self.update_notified {
                        // Let the wallpaper restore/applied toast finish first.
                        self.update_toast_at =
                            Some(Instant::now() + Duration::from_millis(1200));
                    }
                    self.update_manual = false;
                }
                UpdateEvent::Checked(Ok(None)) => {
                    self.update_checking = false;
                    self.update_checked = true;
                    self.update_available = None;
                    self.update_ready = None;
                    update::UpdateCache::record(None).save();
                    if self.update_manual {
                        self.update_manual = false;
                        self.info(self.strings.update_up_to_date);
                    }
                }
                UpdateEvent::Checked(Err(err)) => {
                    self.update_checking = false;
                    self.update_error = Some(err.clone());
                    log::warn!("update check failed: {err}");
                    if self.update_manual {
                        self.update_manual = false;
                        self.error(format!("{}: {err}", self.strings.update_failed));
                    }
                }
                UpdateEvent::Progress { downloaded, total } => {
                    self.update_progress = (downloaded, total);
                }
                UpdateEvent::Downloaded(Ok(path)) => {
                    self.update_downloading = false;
                    self.update_ready = Some(path);
                    self.info(self.strings.update_ready);
                }
                UpdateEvent::Downloaded(Err(err)) => {
                    self.update_downloading = false;
                    self.update_error = Some(err.clone());
                    self.error(format!("{}: {err}", self.strings.update_failed));
                }
            }
        }

        if self.update_checking || self.update_downloading {
            ctx.request_repaint_after(Duration::from_millis(100));
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
        if self.hud_open {
            self.hud_open = false;
            self.hidden_lost_foreground = false;
            return;
        }
        let items = self.filtered_items();
        log::info!("quick switcher: opening with {} items", items.len());
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
        self.hud_carousel = self.hud_index as f32;
        self.hud_items = items;
        self.hud_open = true;
        // Opening the HUD must not make the parked main window pop up when it
        // closes; a real "Switch to" will lose the foreground again first.
        self.hidden_lost_foreground = false;
    }

    pub fn request_quit(&mut self) {
        if self.quitting {
            return;
        }
        self.quitting = true;
        log::info!("quit requested; watchdog will force exit in 3s if shutdown hangs");
        // The engine shutdown / wallpaper restore can block on a stuck mpv
        // process. Guarantee a real process exit, especially from the tray.
        let _ = std::thread::Builder::new()
            .name("aura-quit-watchdog".into())
            .spawn(|| {
                std::thread::sleep(Duration::from_secs(3));
                log::warn!("graceful quit timed out; forcing process exit");
                std::process::exit(0);
            });
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
                HotkeyAction::Switcher => self.open_switcher(),
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

        // Engine::tick already enumerated the desktop; reuse that snapshot
        // instead of calling EnumDisplayMonitors a second time in this tick.
        let mut changed = self.engine.tick(&self.cfg);

        let fresh = self.engine.monitors().to_vec();
        if fresh.len() != self.monitors.len()
            || fresh.iter().zip(self.monitors.iter()).any(|(a, b)| {
                a.key != b.key || a.id != b.id || a.rect != b.rect || a.dpi != b.dpi
            })
        {
            self.monitors = fresh;
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
            // A GPU driver update (or a port/dock change) renumbers the
            // display devices while Aura is running. Re-link the assignments
            // to the panels that are present now so the wallpaper follows the
            // screen instead of becoming an orphan entry.
            if self.cfg.reconcile_monitors(&self.monitors) {
                self.mark_dirty();
                // The tick above used the old keys, so the engine may not have
                // created the host for the re-linked monitor yet. Sync again
                // and restart that wallpaper within this same tick.
                changed.extend(self.engine.tick(&self.cfg));
                changed.sort();
                changed.dedup();
            }
        }

        if !changed.is_empty() {
            self.reapply_changed(&changed);
        }
    }

    fn poll_pause(&mut self) {
        self.pause_flags
            .store(pause_flags(&self.cfg), Ordering::Relaxed);

        // The watcher thread does the expensive window enumeration; only the
        // newest snapshot matters, so drain without applying stale states.
        let mut latest = None;
        while let Ok(env) = self.pause_rx.try_recv() {
            latest = Some(env);
        }
        if let Some(mut env) = latest {
            // While a fullscreen app (usually a game) is in the foreground,
            // unregister the global shortcuts so they reach the game instead
            // of being swallowed by Windows. They are restored when the app
            // closes or loses fullscreen.
            if let Some(hotkeys) = self.hotkeys.as_mut() {
                hotkeys.set_enabled(hotkeys_enabled(env.fullscreen, self.hud_open));
            }
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
                self.request_quit();
            }
        }
    }

    fn apply_window_state(&mut self, frame: &eframe::Frame) {
        if self.main_hwnd.is_none() {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(handle) = frame.window_handle() {
                if let RawWindowHandle::Win32(win32) = handle.as_raw() {
                    let raw = win32.hwnd.get();
                    self.main_hwnd = Some(raw);
                    win::prepare_window(windows::Win32::Foundation::HWND(raw as *mut _));
                }
            }
        }
        // Task Manager's "Switch to" brings the parked window to the
        // foreground; bring it back on-screen when that happens. Ignore the
        // window's own startup activation: only a foreground transition that
        // happens after the parked window lost the foreground counts.
        // While the quick switcher is open it owns the foreground, so do not
        // restore the main window behind it.
        if self.hidden && !self.hud_open {
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
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // A transparent surface lets the desktop show through the configured
        // opacity. Every buffer rendered before the window is revealed must be
        // opaque instead: if DWM composites a stale one while showing the
        // window, it would otherwise flash the white default surface.
        if self.startup_frames < OPAQUE_CLEAR_FRAMES {
            let [r, g, b, _] = theme::palette().bg.to_array();
            [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
        } else {
            [0.0, 0.0, 0.0, 0.0]
        }
    }

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

        if let Some(at) = self.update_toast_at {
            if Instant::now() >= at {
                self.notify_update();
            }
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
        if let Ok(version) = self.mpv_version_rx.try_recv() {
            self.mpv_version = version;
        }
        self.handle_dropped_files(ctx);
        self.handle_close_request(ctx);

        while let Ok(event) = self.scan_rx.try_recv() {
            match event {
                ScanEvent::Progress => {}
                ScanEvent::Done(items) => {
                    self.items = items;
                    self.scanning = false;
                    self.invalidate_filter();
                }
                ScanEvent::Failed(error) => {
                    self.scanning = false;
                    self.error(error);
                }
            }
        }

        while let Ok(result) = self.remote_rx.try_recv() {
            // Ignore results from a search the user already replaced.
            if result.seq != self.remote_seq {
                continue;
            }
            self.remote_loading = false;
            match result.result {
                Ok(mut items) => {
                    if result.page <= 1 {
                        self.remote = Arc::new(Vec::new());
                    }
                    Arc::make_mut(&mut self.remote).append(&mut items);
                    self.remote_page = result.page;
                }
                Err(error) => {
                    log::warn!("catalog search failed: {error}");
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
                            self.invalidate_filter();
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

        if self.nav == Nav::Explore && !self.explore_loaded {
            self.search_online(true);
        }

        self.tick_engine();
        self.poll_pause();
        self.poll_accent();
        self.poll_updates(ctx);

        // Re-apply the palette on the first frame: `AuraApp::new` runs before
        // egui knows the system theme. `theme::apply` is idempotent and also
        // sends the native window theme, which is only applied once the
        // viewport exists.
        if !self.theme_synced || self.accent != self.applied_accent {
            theme::apply(ctx, self.accent);
            self.applied_accent = self.accent;
            self.theme_synced = true;
        }

        if self.hud_open {
            self.hud_viewport(ctx);
        }

        super::ui::main_ui(self, ctx);
        self.apply_window_state(_frame);

        if let Some(hwnd) = self.main_hwnd {
            let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
            win::apply_rounded_corners(
                windows::Win32::Foundation::HWND(hwnd as *mut _),
                maximized,
            );
        }

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

        // Reveal the window only after several fully themed frames have been
        // presented. Viewport commands are applied before the current frame is
        // painted, so the window shows an already painted buffer; the extra
        // frame (vs. the old two) covers slow first swaps (shader compilation).
        if !self.revealed {
            if self.startup_frames >= REVEAL_AFTER_FRAMES {
                self.revealed = true;
                if !self.hidden {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                }
            } else {
                ctx.request_repaint();
            }
        }

        // Frames already presented. `clear_color` runs before `update`, so it
        // sees the number of frames that are on screen, not the current one.
        if self.startup_frames <= OPAQUE_CLEAR_FRAMES {
            self.startup_frames = self.startup_frames.saturating_add(1);
        }

        // Animate only when something is actually moving. egui repaints on
        // input by itself, so a static page can idle at 5 Hz instead of 60.
        let busy = self.scanning
            || self.remote_loading
            || self.installing
            || self.update_checking
            || self.update_downloading
            || self.toast.is_some();
        let interval = if self.hud_open {
            16
        } else if self.hidden {
            500
        } else if busy {
            50
        } else {
            200
        };
        ctx.request_repaint_after(Duration::from_millis(interval));
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
    // `start_hidden` affects how the viewport is created, so read it before
    // building the window. Avoids briefly flashing a normal window on screen
    // when the user asked Aura to start minimized.
    let hidden = hidden || crate::config::Config::load().start_hidden;
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
        .with_decorations(false)
        .with_transparent(true)
        // A normal launch stays hidden until the first themed frame has been
        // rendered; `--hidden` keeps the window visible but parked off-screen
        // so eframe keeps pumping its event loop for the tray and engine.
        .with_visible(hidden);
    if hidden {
        viewport = viewport.with_position([-32000.0, -32000.0]);
    }

    let options = eframe::NativeOptions {
        viewport,
        centered: !hidden,
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
    use super::{
        assigned_display_numbers, pause_flags, parked_step, PAUSE_BATTERY, PAUSE_FULLSCREEN,
        PAUSE_OCCLUSION,
    };
    use crate::config::{Assignment, Config, Scaling};
    use crate::platform::win::{MonitorInfo, RectI};
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    fn monitor(key: &str) -> MonitorInfo {
        MonitorInfo {
            key: key.to_string(),
            id: String::new(),
            name: String::new(),
            rect: RectI {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
            work: RectI {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
            primary: false,
            dpi: 96,
        }
    }

    const LG: &str = r"\\?\DISPLAY#GSM5C8A#5&D89A054&1&UID4353";

    #[test]
    fn display_badges_follow_each_monitor_assignment() {
        let mut assignments = BTreeMap::new();
        assignments.insert(
            r"\\.\DISPLAY1".to_string(),
            Assignment::new(PathBuf::from("C:/a.jpg"), Scaling::Fill),
        );
        assignments.insert(
            r"\\.\DISPLAY2".to_string(),
            Assignment::new(PathBuf::from("C:/b.jpg"), Scaling::Fill),
        );
        let monitors = vec![monitor(r"\\.\DISPLAY1"), monitor(r"\\.\DISPLAY2")];
        assert_eq!(
            assigned_display_numbers(&assignments, &monitors, Path::new("C:/a.jpg")),
            vec![1]
        );
        assert_eq!(
            assigned_display_numbers(&assignments, &monitors, Path::new("C:/b.jpg")),
            vec![2]
        );
    }

    #[test]
    fn display_badges_expand_wildcards_and_ignore_disconnected_keys() {
        let mut assignments = BTreeMap::new();
        assignments.insert(
            "*".to_string(),
            Assignment::new(PathBuf::from("C:/both.jpg"), Scaling::Fill),
        );
        assignments.insert(
            r"\\.\DISPLAY7".to_string(),
            Assignment::new(PathBuf::from("C:/away.jpg"), Scaling::Fill),
        );
        let monitors = vec![monitor(r"\\.\DISPLAY1"), monitor(r"\\.\DISPLAY2")];
        assert_eq!(
            assigned_display_numbers(&assignments, &monitors, Path::new("C:/both.jpg")),
            vec![1, 2]
        );
        // A disconnected device name is not a valid screen any more: after a
        // driver update its number may belong to another panel.
        assert!(
            assigned_display_numbers(&assignments, &monitors, Path::new("C:/away.jpg")).is_empty()
        );
        assert!(
            assigned_display_numbers(&assignments, &monitors, Path::new("C:/none.jpg")).is_empty()
        );
    }

    #[test]
    fn display_badges_match_a_renumbered_panel_by_identity() {
        let mut assignments = BTreeMap::new();
        let mut assignment = Assignment::new(PathBuf::from("C:/a.jpg"), Scaling::Fill);
        assignment.monitor_id = LG.to_string();
        // Saved before the GPU driver update renamed DISPLAY1 -> DISPLAY6.
        assignments.insert(r"\\.\DISPLAY1".to_string(), assignment);

        let mut renumbered = monitor(r"\\.\DISPLAY6");
        renumbered.id = LG.to_string();
        let monitors = vec![renumbered, monitor(r"\\.\DISPLAY7")];
        assert_eq!(
            assigned_display_numbers(&assignments, &monitors, Path::new("C:/a.jpg")),
            vec![6]
        );
        // Only the screen that actually has the wallpaper is marked.
        assert!(
            assigned_display_numbers(&assignments, &monitors, Path::new("C:/b.jpg")).is_empty()
        );
    }

    #[test]
    fn pause_flags_only_include_enabled_checks() {
        let mut cfg = Config::default();
        cfg.pause_on_fullscreen = true;
        cfg.pause_on_occlusion = false;
        cfg.pause_on_battery = true;
        assert_eq!(pause_flags(&cfg), PAUSE_FULLSCREEN | PAUSE_BATTERY);
        cfg.pause_on_fullscreen = false;
        cfg.pause_on_battery = false;
        assert_eq!(pause_flags(&cfg), 0);
        cfg.pause_on_occlusion = true;
        assert_eq!(pause_flags(&cfg), PAUSE_OCCLUSION);
    }

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

#[cfg(test)]
mod hotkey_tests {
    use super::hotkeys_enabled;

    #[test]
    fn shortcuts_are_suspended_for_fullscreen_apps() {
        assert!(!hotkeys_enabled(true, false));
        assert!(hotkeys_enabled(false, false));
        // The quick switcher is our own fullscreen viewport.
        assert!(hotkeys_enabled(true, true));
    }
}
