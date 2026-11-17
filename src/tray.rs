//! System tray icon and menu.

use std::sync::OnceLock;
use std::time::Duration;

use crossbeam_channel::Receiver;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::i18n::Strings;
use crate::platform::win;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayAction {
    Show,
    Next,
    Prev,
    TogglePause,
    Switcher,
    Stop,
    Quit,
}

/// Everything the menu reflects about playback and the library. The tray only
/// rebuilds labels/state when this changes, so syncing it every frame is cheap
/// (setting a Win32 menu item's text forces the native menu to be recreated).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrayState {
    /// A wallpaper is playing (or paused). Enables pause/resume and stop.
    pub running: bool,
    /// Playback is paused, so the toggle item reads "Resume".
    pub paused: bool,
    /// The library has at least one item, enabling next/previous/switcher.
    pub library: bool,
}

pub struct Tray {
    icon: TrayIcon,
    _menu: Menu,
    toggle: MenuItem,
    next: MenuItem,
    prev: MenuItem,
    switcher: MenuItem,
    stop: MenuItem,
    state: Option<TrayState>,
}

pub fn create(strings: &Strings, state: TrayState) -> Option<Tray> {
    // Start collecting events immediately, even if the window starts hidden and
    // eframe does not run a frame for a while.
    start_forwarder();

    let menu = Menu::new();
    let show = MenuItem::with_id("show", strings.tray_show, true, None);
    let next = MenuItem::with_id("next", strings.tray_next, true, None);
    let prev = MenuItem::with_id("prev", strings.tray_prev, true, None);
    let toggle = MenuItem::with_id(
        "toggle",
        if state.paused {
            strings.tray_resume
        } else {
            strings.tray_pause
        },
        state.running,
        None,
    );
    let switcher = MenuItem::with_id("switcher", strings.tray_switcher, true, None);
    let stop = MenuItem::with_id("stop", strings.tray_stop, state.running, None);
    let quit = MenuItem::with_id("quit", strings.tray_quit, true, None);

    let _ = menu.append(&show);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&next);
    let _ = menu.append(&prev);
    let _ = menu.append(&toggle);
    let _ = menu.append(&switcher);
    let _ = menu.append(&stop);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&quit);

    let icon = Icon::from_rgba(icon_rgba(), 32, 32).ok()?;

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu.clone()))
        .with_menu_on_left_click(false)
        .with_tooltip(tooltip(strings, state.paused))
        .with_icon(icon)
        .build()
        .ok()?;

    let mut tray = Tray {
        icon: tray,
        _menu: menu,
        toggle,
        next,
        prev,
        switcher,
        stop,
        state: None,
    };
    tray.sync(state, strings);
    Some(tray)
}

impl Tray {
    /// Refreshes the menu labels, enabled states and tooltip. Cheap when the
    /// state did not change, which is the common case for every frame.
    pub fn sync(&mut self, state: TrayState, strings: &Strings) {
        if self.state == Some(state) {
            return;
        }

        self.toggle
            .set_text(if state.paused { strings.tray_resume } else { strings.tray_pause });
        self.toggle.set_enabled(state.running);
        self.stop.set_enabled(state.running);
        self.next.set_enabled(state.library);
        self.prev.set_enabled(state.library);
        self.switcher.set_enabled(state.library);

        let _ = self
            .icon
            .set_tooltip(Some(tooltip(strings, state.paused)));

        self.state = Some(state);
    }
}

fn tooltip(strings: &Strings, paused: bool) -> String {
    let status = if paused {
        strings.paused
    } else {
        strings.playing
    };
    format!("{} - {}", strings.app_name, status)
}

/// Actions collected by the background tray forwarder. `poll` drains this
/// instead of reading muda/tray-icon receivers directly, so tray clicks and
/// menu selections are never lost while the window is parked and idle.
static ACTION_RX: OnceLock<Receiver<TrayAction>> = OnceLock::new();

fn start_forwarder() -> &'static Receiver<TrayAction> {
    ACTION_RX.get_or_init(|| {
        let (tx, rx) = crossbeam_channel::unbounded();
        let _ = std::thread::Builder::new()
            .name("aura-tray-events".into())
            .spawn(move || loop {
                for action in drain_raw() {
                    let _ = tx.send(action);
                    // The UI loop may be asleep while the window is parked. A
                    // real resize wakes winit/eframe even when the parked
                    // window stops receiving WM_PAINT.
                    win::wake_main_window();
                }
                std::thread::sleep(Duration::from_millis(50));
            });
        rx
    })
}

fn drain_raw() -> Vec<TrayAction> {
    let mut actions = Vec::new();

    while let Ok(event) = TrayIconEvent::receiver().try_recv() {
        match event {
            // A left click (single or double) on the notification icon brings
            // the window back, like most tray applications.
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }
            | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } => actions.push(TrayAction::Show),
            _ => {}
        }
    }

    while let Ok(event) = MenuEvent::receiver().try_recv() {
        let action = match event.id.0.as_str() {
            "show" => Some(TrayAction::Show),
            "next" => Some(TrayAction::Next),
            "prev" => Some(TrayAction::Prev),
            "toggle" => Some(TrayAction::TogglePause),
            "switcher" => Some(TrayAction::Switcher),
            "stop" => Some(TrayAction::Stop),
            "quit" => Some(TrayAction::Quit),
            _ => None,
        };
        if let Some(action) = action {
            actions.push(action);
        }
    }

    actions
}

pub fn poll() -> Vec<TrayAction> {
    start_forwarder().try_iter().collect()
}

pub fn icon_rgba() -> Vec<u8> {
    let size = 32usize;
    let mut rgba = vec![0u8; size * size * 4];
    let center = (size as f32 - 1.0) / 2.0;
    let radius = size as f32 / 2.0 - 1.0;

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let dist = (dx * dx + dy * dy).sqrt();
            let index = (y * size + x) * 4;
            let coverage = (radius + 0.75 - dist).clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }
            let t = (y as f32 / size as f32).clamp(0.0, 1.0);
            let r = (122.0 + (187.0 - 122.0) * t) as u8;
            let g = (162.0 + (154.0 - 162.0) * t) as u8;
            let b = (247.0 + (247.0 - 247.0) * t) as u8;

            // White play triangle.
            let in_triangle = x >= 11 && x <= 23 && {
                let half = (23 - x) as f32 * 0.62;
                (y as f32) >= 16.0 - half && (y as f32) <= 16.0 + half
            };

            rgba[index] = if in_triangle { 255 } else { r };
            rgba[index + 1] = if in_triangle { 255 } else { g };
            rgba[index + 2] = if in_triangle { 255 } else { b };
            rgba[index + 3] = (255.0 * coverage) as u8;
        }
    }
    rgba
}
