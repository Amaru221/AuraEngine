# Changelog

All notable changes to Aura Engine are documented here.

## [1.0.7] - 2026-10-24

### Added

- MotionBGS category chips in **Explore**: All, Anime, Games, 4K, Superhero and Nature.
- `aura motionbgs <category>` to open Explore directly in a MotionBGS category.
- `--category` support for `search-motionbgs` and `download-motionbgs`.

### Fixed

- Missing wallpaper assignments (deleted downloads, temp files) no longer create black host windows; they are removed on startup.
- A failed video/image application no longer persists a broken assignment.

## [1.0.6] - 2026-10-24

### Added

- MotionBGS provider with search, pagination, thumbnails, 4K/1080p selection and one-click download/apply.
- MotionBGS CLI helpers (`search-motionbgs`, `download-motionbgs`).

## [1.0.5] - 2026-10-24

### Added

- Honeycomb quick switcher: 5 columns x 3 rows per page, separated hexagons, larger thumbnails, shadows and accent glow.
- Page navigation with side arrows, mouse wheel, arrows/WASD, PageUp/PageDown, Home/End and page dots.
- Hold-to-show mode for `Ctrl+Alt+W`; releasing the shortcut closes the HUD.
- Manual resume overrides the current auto-pause condition until it disappears.

### Fixed

- Pause/resume commands were delayed or lost because the persistent mpv IPC connection could stall; each command now uses a short-lived connection.
- Video-to-video switching no longer hides the host window.

## [1.0.4] - 2026-10-24

### Added

- MotionBGS online catalog (HTML card scraping, 4K/HD endpoints).

## [1.0.3] - 2026-10-24

### Fixed

- **About** page ran `mpv --version` on every frame, pinning a CPU core.
- Library, Displays, Explore and the HUD no longer clone the whole collection or hash thumbnails every frame.
- Cached config/data paths and thumbnail keys.

## [1.0.2] - 2026-10-24

### Fixed

- **Settings** page ran `mpv --version` on every frame; the version is now cached and refreshed only when the mpv path changes.

## [1.0.1] - 2026-10-24

### Fixed

- Infinite feedback loop between `IDesktopWallpaper` and `WorkerW`: applying a static wallpaper recreated the desktop layer and re-applied the image every few seconds (screen flicker and UI stalls).
- Static wallpapers no longer create or manage host windows.
- mpv start/stop moved off the UI thread; pause detection moved to a background thread.

## [1.0.0] - 2026-10-24

### Added

- Initial native Windows port of Aura (COSMIC Desktop live wallpaper engine).
- `WorkerW` host windows behind desktop icons and `mpv --wid` playback.
- Multi-monitor support with DPI awareness and hotplug/Explorer restart recovery.
- Native static wallpapers through `IDesktopWallpaper`.
- Auto-pause on fullscreen games, desktop occlusion and battery.
- egui interface: Library, Explore, Displays, Settings and About.
- Local library scanning, thumbnails (mpv frame extraction + Shell fallback), search, filters and favorites.
- Wallhaven, Bing Daily and Pexels catalogs.
- Tray icon, global hotkeys and single-instance named-pipe IPC.
- CLI (`next`, `prev`, `apply`, `pause`, `stop`, `switcher`, `status`, `doctor`, `install-mpv`, ...).
- Portable mpv auto-download and packaging scripts.
