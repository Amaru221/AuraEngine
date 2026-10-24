# Changelog

All notable changes to Aura Engine are documented here.

## [1.0.11] - 2026-10-24

### Added

- Global keyboard shortcuts are suspended while a fullscreen app (usually a
  game) is in the foreground, so game key combinations reach the game instead
  of being swallowed by Windows. They are registered again as soon as the app
  closes or leaves fullscreen. The tray menu keeps working.

### Changed

- The fullscreen/occlusion watcher now drives both auto-pause and shortcut
  suspension. The quick switcher viewport is excluded, so its own shortcuts
  keep working while it is open.

## [1.0.10] - 2026-10-24

### Fixed

- The quick switcher no longer closes (and flickers) when `Ctrl+Alt+W` is
  released. `global-hotkey` emits both Pressed and Released events and the app
  was acting on both; only Pressed is handled now, so all shortcuts fire once.
- `Ctrl+Alt+W` now toggles the HUD: press to open, press again to close.
- Opening the quick switcher no longer brings the parked main window back, and
  closing it no longer pops the main window on screen.
- Moving the mouse over the Cinematic/Dock carousel no longer changes the
  selection; only the keyboard and wheel navigate.

### Changed

- Cinematic and Dock are now smoothly animated, frame-rate independent
  carousels: arrows/WASD move one card and `PageUp`/`PageDown` move a page.
- Cinematic cards have more spacing and no overlapping labels; the Dock has
  wider gaps.
- The page counter and redundant labels are hidden in carousel modes.
- Updated the HUD hint text for the new toggle and paging behaviour.

## [1.0.9] - 2026-10-24

### Added

- Monitor numbers everywhere: "Pantalla 1", "Pantalla 2"... on the Displays
  map, the monitor cards and the library "apply to" menu, matching the numbers
  shown by **Windows Settings → Display → Identify**.
- Large number badges on the virtual desktop map.

### Fixed

- Clicking an entry in a library or Explore context menu (for example a
  monitor in "Todas las pantallas") no longer also clicks the card underneath
  the popup.

## [1.0.8] - 2026-10-24

### Added

- Proper Windows installer (Inno Setup) with welcome, license, tasks and
  language pages (Spanish/English).
- Start Menu group (`Aura Engine`), optional desktop shortcut and optional
  start-with-Windows task.
- Apps & Features entry with publisher, version, icon and uninstaller.
- `App Paths` registration so `aura` works from Win+R and the Start menu.
- Optional trust task for the public self-signed certificate.
- Uninstall prompt to remove settings, cache and downloaded wallpapers.
- Installer bundles mpv, so video wallpapers work offline after installing.
- Release workflow builds and publishes the installer next to the `.exe`
  and `.zip`; the CI ZIP now includes mpv as well.

### Fixed

- Task Manager "End task" now closes Aura instead of hiding it to the tray.
- Task Manager "Switch to" brings the parked window back on-screen.
- First run adopts the autostart state created by the installer.
- The installer closes a running instance before upgrading or uninstalling,
  including silent installs.
- The autostart value is rewritten to the installed location and removed on
  uninstall, replacing the legacy PowerShell value.
- A named mutex lets the installer detect a running instance.
- The parked window no longer pops up on a `--hidden` startup when Windows
  gives it the foreground.

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
