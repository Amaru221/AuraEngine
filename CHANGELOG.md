# Changelog

All notable changes to Aura Engine are documented here.

## [Unreleased]

### Fixed

- Display state was lost after updating the GPU driver while Aura was running.
  Windows renumbers the display devices (`\\.\DISPLAY1` → `\\.\DISPLAY6`) and the
  saved per-monitor assignments kept pointing at names that no longer existed,
  so the wallpapers were not restored and the Library showed several display
  numbers on the same card (a stale `1` next to the real `6`). Assignments now
  store the stable PnP identity of the panel (`Assignment.monitor_id`) and are
  re-linked to the current device name at startup and whenever the topology
  changes. Library badges and the "applied" state only consider connected
  screens, so disconnected entries can no longer add phantom numbers.
- Static wallpapers could be applied to the wrong monitor because
  `IDesktopWallpaper` was indexed with the `EnumDisplayMonitors` position.
  Both lists have different orders and the COM one can keep a phantom monitor
  from the previous driver. The panel is now resolved by identity, ignoring
  phantom entries.
- `EnumDisplayMonitors` results are deduplicated and sorted (primary first),
  preventing duplicated entries for one panel after a driver update from
  drawing two numbers on the same screen.
- `aura doctor` prints each monitor's stable identity and the number of monitors
  exposed by `IDesktopWallpaper`, which makes ghost displays easy to spot.

## [1.1.1] - 2026-10-28

### Fixed

- White "other theme" frame flash when opening the window. The palette was
  applied before egui knew the system theme, and `set_visuals`/`set_style` only
  write the style slot of the *currently resolved* theme. On a light Windows
  theme the first frames could therefore render with egui's default light
  style (white surfaces and a white native frame). The theme is now pinned with
  `ThemePreference`, the palette is written to both style slots, the native DWM
  theme follows the app palette, and DWM's 1 px system border is removed with
  `DWMWA_BORDER_COLOR`. The window is revealed after three painted frames while
  the clear color stays opaque, so a stale buffer can never show white.
- White system frame on every window activation (Alt+Tab, restore from
  minimized, unhide from the tray). winit keeps `WS_CAPTION`/`WS_THICKFRAME` on
  undecorated windows, and `DefWindowProc` paints the default non-client frame
  and caption buttons over the borderless UI when it receives `WM_NCACTIVATE`.
  The main window procedure is now subclassed to answer `WM_NCACTIVATE` /
  `WM_NCPAINT` itself, so the OS frame is never drawn. Verified with a
  frame-by-frame capture: the white 7 px ring plus caption buttons that appeared
  on activation are gone, while focus, resize, minimize and maximize keep
  working.

## [1.1.0] - 2026-10-26

### Changed

- Per-display applied badges are now shown **only in Library**. Explore cards
  return to their discovery-focused layout: they no longer probe local
  assignments or draw applied-state overlays and accent borders.
- The downloader keeps using the deterministic local filename helper, so
  catalog downloads continue to be stored and reused predictably.

## [1.0.19] - 2026-10-26

### Added

- Per-display applied badges in the **Library** and **Explore** cards. Each
  card now shows the Windows display number (`1`, `2`, …) where that image or
  video is currently assigned, including the same wallpaper on several
  displays. Detection uses the real per-monitor assignments instead of the
  single global `last_applied` value.
- Explore locates already-downloaded catalog items with the same deterministic
  filename used by the downloader, so their badges survive navigation.

### Fixed

- White flash while opening the app. A timer forced the window visible after
  300 ms even when the first OpenGL frame had not been presented yet. The
  window is now revealed only after two painted frames, the first frame is
  cleared with the opaque theme background, the default white window-class
  brush is removed and DWM dark rendering is requested.

### Changed

- Repository `.gitignore` now excludes local agent/editor tooling, build and
  packaging artifacts, logs, temporary files, secrets and OS noise, keeping
  version control focused on the application source and release metadata.

## [1.0.18] - 2026-10-26

### Fixed

- Pause/resume/stop could stop reaching mpv after a failed IPC write. The
  persistent connection kept a cloned reader handle alive, so mpv never
  accepted a reconnection and the video stayed paused (including the Windows
  media controls) until the app was restarted. Commands now use a short-lived
  connection and wait for mpv's acknowledgement.
- The play/pause button now toggles the real playback state instead of only
  the manual pause flag, so a video paused by fullscreen/occlusion can be
  resumed with one click.
- Pause state is reasserted for a few ticks after a change and then stops,
  avoiding both lost commands and fighting the Windows media controls.
- Application shutdown now waits briefly for mpv and kills it if necessary,
  preventing orphan mpv processes when the quit command cannot be delivered.

## [1.0.17] - 2026-10-25

### Added

- Visible language picker in **Settings -> General** for Spanish, English and
  French. The window, tray menu, status text and errors update immediately.
- README screenshots for the library, Explore and Settings.

### Performance

- Library search and filters are cached and only rebuilt when the query,
  filter, item list or favorites change. Item names/folders are pre-lowercased
  during the scan, so a 25k-item collection is not re-scanned every frame.
- Favorites use a `HashSet`, making favorite filters and card badges O(1).
- Thumbnails are generated/downloaded by four parallel workers with a shared
  HTTP client, and uploaded to the GPU in small per-frame batches.
- Thumbnail textures are capped and evicted with an LRU policy (bounded GPU
  memory when browsing several catalogs).
- Explore loads on first visit instead of during startup, reuses one HTTP
  client, borrows remote items instead of cloning them every frame, and caches
  remote thumbnail keys.
- The pause watcher skips disabled battery checks and enumerates all windows
  once for every monitor instead of once per monitor.
- Adaptive repaint pacing: 60 fps only for HUD animations, 20 fps while
  loading, 5 Hz when idle and 2 Hz while parked in the tray.
- Window creation is now hidden until the first themed frame is ready, which
  removes the startup white/transparent flash.
- Startup no longer waits three seconds for another instance: the
  single-instance mutex makes the first launch skip the pipe probe, the mpv
  version probe runs in the background and monitor enumeration happens once.
- Applying a wallpaper no longer decodes the image for its accent color on the
  UI thread.

### Fixed

- Switching providers while Explore was loading could leave the catalog stuck
  on a spinner forever. Requests now carry a generation and stale results are
  discarded.
- Pause/resume is re-asserted every engine tick and explicit button presses
  force an IPC write, so a command lost while mpv was starting no longer
  desynchronizes the button.
- mpv IPC keeps a persistent connection with a drain thread instead of
  reconnecting for every command, cutting pause/resume latency.
- A UTF-8 BOM in `config.json` (for example after editing it with Notepad)
  no longer discards the configuration.
- The legacy per-monitor thumbnail key is filled once at startup instead of
  calling `metadata`/SHA-256 from the Displays page every frame.
- MotionBGS failures are now classified: IP/Cloudflare block, rate limit,
  network outage or empty search. A block shows a clear message recommending
  a VPN/proxy, a button to open the site in the browser and a manual retry
  only. Nothing is retried automatically.
- Successful MotionBGS pages are cached in memory and on disk for 20 minutes.
  Switching back to a category or repeating a search no longer hits the site;
  **Refresh** and **Retry** bypass the cache explicitly.

## [1.0.16] - 2026-10-25

### Fixed

- Removed the full-window resize overlay introduced in 1.0.14 that could
  block clicks and leave the UI unmanageable. Resizing is now done from a
  small bottom-right grip, and the rest of the window is interactive again.
- The main window now has rounded corners (10 px) through a window region;
  maximized windows stay square.
- Tray **Quit** now guarantees a complete process exit: a watchdog forces
  `exit(0)` after 3 seconds if the graceful engine shutdown hangs.

## [1.0.15] - 2026-10-25

### Changed

- The official installer and ZIP no longer bundle mpv; Aura downloads it on
  first use. The distributed binaries now contain only Aura's own GPL code
  (plus documentation and the public certificate), removing the
  mpv/FFmpeg redistribution obligation.
- Add `THIRD_PARTY_LICENSES.txt` with the full license texts of the Rust
  crates, the embedded egui fonts and the component-specific licenses.
- Add `SOURCE.txt` with the exact source-code tag and license information.
  Release assets now include `LICENSE`, `NOTICE`,
  `THIRD_PARTY_LICENSES.txt` and `SOURCE.txt`.
- Add an unofficial-port disclaimer, the mpv download notice and an About
  button that opens the third-party licenses.
- Update the release workflow to build the installer and ZIP without mpv and
  to link the exact source tag.

## [1.0.14] - 2026-10-25

### Added

- Custom, theme-aware title bar with the app logo, drag-to-move,
  double-click to maximize, and minimize/maximize/close buttons.
- Separate opacity controls for the main container, sidebar and title bar in
  **Settings -> Appearance**, plus a "same opacity for all three" link and a
  "match main" shortcut.
- Edge and corner resize handles for the borderless window.
- A subtle window border so the app keeps its shape on the desktop.

### Changed

- The title bar follows the selected theme and opacity instead of the native
  Windows chrome.
- Window decoration is client-side; the native title bar is gone.

## [1.0.13] - 2026-10-25

### Added

- Translucent main window with a configurable opacity in
  **Settings -> Appearance** (40-100%).
- The left navigation bar uses a darker, slightly more opaque surface than
  the content so it contrasts with the main container.

### Changed

- The window is created with a transparent surface and eframe clears with a
  transparent color; the content panel paints the configured alpha so the
  desktop shows through behind it.
- Cards, dialogs and text remain opaque for readability.

## [1.0.12] - 2026-10-24

### Added

- Automatic update checks against GitHub Releases (cached for 6 hours). A
  toast announces a new version at startup, and the About page shows an update
  badge, the release notes and a download button.
- In-app update: downloads the installer, verifies its SHA-256 against the
  published checksum and launches it. The installer closes Aura and
  relaunches the new version.
- Theme personalization in **Settings -> Appearance**: Aura Dark, Midnight,
  Light and Custom presets, a custom background color, a manual accent color
  with presets, and the existing wallpaper extraction toggle.
- Theme preset and accent color are persisted in the configuration.

### Changed

- Every custom-painted surface uses the active theme palette, so the light
  theme is readable (page titles, toasts, banners, filter chips, keycaps and
  badges).

### Fixed

- Startup "wallpaper applied" toasts no longer hide the update notification.

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
