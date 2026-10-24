//! Global shortcuts.

use std::collections::HashMap;

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyAction {
    Switcher,
    Next,
    Prev,
    TogglePause,
    Stop,
}

pub struct Hotkeys {
    _manager: GlobalHotKeyManager,
    map: HashMap<u32, HotkeyAction>,
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
        } else {
            log::warn!("could not register hotkey {:?}", code);
        }
    }

    Some(Hotkeys {
        _manager: manager,
        map,
    })
}

pub fn poll(hotkeys: &Hotkeys) -> Vec<HotkeyAction> {
    let mut actions = Vec::new();
    while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
        if let Some(action) = hotkeys.map.get(&event.id) {
            actions.push(*action);
        }
    }
    actions
}


