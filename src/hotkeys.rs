//! Global shortcuts.

use std::collections::HashMap;

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyAction {
    Switcher,
    Next,
    Prev,
    TogglePause,
    Stop,
}

pub struct Hotkeys {
    manager: GlobalHotKeyManager,
    map: HashMap<u32, HotkeyAction>,
    registered: Vec<HotKey>,
    enabled: bool,
}

pub const SHORTCUTS: [(&str, &str, HotkeyAction); 5] = [
    ("Ctrl + Alt + W", "Quick switcher", HotkeyAction::Switcher),
    ("Ctrl + Alt + Right", "Next wallpaper", HotkeyAction::Next),
    ("Ctrl + Alt + Left", "Previous wallpaper", HotkeyAction::Prev),
    ("Ctrl + Alt + Space", "Pause / resume", HotkeyAction::TogglePause),
    ("Ctrl + Alt + S", "Stop wallpaper", HotkeyAction::Stop),
];

pub fn create() -> Option<Hotkeys> {
    let manager = GlobalHotKeyManager::new().ok()?;
    let mut map = HashMap::new();
    let mut registered = Vec::new();

    let definitions = [
        (Code::KeyW, HotkeyAction::Switcher),
        (Code::ArrowRight, HotkeyAction::Next),
        (Code::ArrowLeft, HotkeyAction::Prev),
        (Code::Space, HotkeyAction::TogglePause),
        (Code::KeyS, HotkeyAction::Stop),
    ];

    for (code, action) in definitions {
        let hotkey = HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), code);
        if manager.register(hotkey).is_ok() {
            map.insert(hotkey.id(), action);
            registered.push(hotkey);
        } else {
            log::warn!("could not register hotkey {:?}", code);
        }
    }

    Some(Hotkeys {
        manager,
        map,
        registered,
        enabled: true,
    })
}

impl Hotkeys {
    /// Registers or unregisters every shortcut. Disabling is required while a
    /// fullscreen game is foreground, otherwise Windows swallows the combos
    /// before the game can see them.
    pub fn set_enabled(&mut self, enabled: bool) {
        if enabled == self.enabled {
            return;
        }

        if enabled {
            for hotkey in &self.registered {
                if let Err(err) = self.manager.register(*hotkey) {
                    log::warn!("could not re-register hotkey {:?}: {err}", hotkey.key);
                }
            }
            log::info!("global hotkeys enabled");
        } else {
            for hotkey in &self.registered {
                if let Err(err) = self.manager.unregister(*hotkey) {
                    log::warn!("could not unregister hotkey {:?}: {err}", hotkey.key);
                }
            }
            log::info!("global hotkeys disabled (fullscreen app)");
        }

        self.enabled = enabled;
    }
}

pub fn poll(hotkeys: &Hotkeys) -> Vec<HotkeyAction> {
    let mut actions = Vec::new();
    while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
        // global-hotkey emits both Pressed and Released. Acting on Released
        // made every shortcut fire twice and made the quick switcher close
        // as soon as the keys were released.
        if event.state != HotKeyState::Pressed {
            continue;
        }
        if let Some(action) = hotkeys.map.get(&event.id) {
            actions.push(*action);
        }
    }
    actions
}


