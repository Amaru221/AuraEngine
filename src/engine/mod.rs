pub mod installer;
pub mod mpv;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

use crate::config::{Config, MediaKind, ParentMode, Scaling};
use crate::platform::host::WallpaperHosts;
use crate::platform::win::{self, MonitorInfo};

#[derive(Clone, Copy, Debug, Default)]
pub struct PauseEnv {
    pub fullscreen: bool,
    pub occluded: bool,
    pub battery: bool,
}

impl PauseEnv {
    #[allow(dead_code)]
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if self.fullscreen {
            parts.push("fullscreen");
        }
        if self.occluded {
            parts.push("occluded");
        }
        if self.battery {
            parts.push("battery");
        }
        if parts.is_empty() {
            "none".into()
        } else {
            parts.join(", ")
        }
    }
}

pub struct Engine {
    hosts: WallpaperHosts,
    instances: HashMap<String, mpv::MpvInstance>,
    monitors: Vec<MonitorInfo>,
    mpv_path: Option<PathBuf>,
    paused_user: bool,
    paused_env: PauseEnv,
    effective_pause: bool,
    auto_pause_override: bool,
    last_error: Option<String>,
    parent_mode: ParentMode,
}

impl Engine {
    pub fn new(mpv_path: Option<PathBuf>) -> Self {
        let mpv_path = mpv::resolve_mpv(mpv_path.as_deref());
        Self {
            hosts: WallpaperHosts::new(),
            instances: HashMap::new(),
            monitors: Vec::new(),
            mpv_path,
            paused_user: false,
            paused_env: PauseEnv::default(),
            effective_pause: false,
            auto_pause_override: false,
            last_error: None,
            parent_mode: ParentMode::Auto,
        }
    }

    pub fn refresh_mpv(&mut self, configured: Option<PathBuf>) {
        self.mpv_path = mpv::resolve_mpv(configured.as_deref());
    }

    pub fn mpv_path(&self) -> Option<&Path> {
        self.mpv_path.as_deref()
    }

    pub fn mpv_available(&self) -> bool {
        self.mpv_path.is_some()
    }

    /// Monitors were enumerated by the last `tick`; reuse them from the UI
    /// instead of enumerating the desktop a second time on every tick.
    pub fn monitors(&self) -> &[MonitorInfo] {
        &self.monitors
    }

    /// Seeds the monitor list before the first `tick`, so applying a saved
    /// wallpaper during startup already knows the display geometry.
    pub fn set_monitors(&mut self, monitors: Vec<MonitorInfo>) {
        self.monitors = monitors;
    }

    /// Periodic housekeeping. Returns monitor keys whose host window was
    /// recreated (or whose mpv died) and therefore need to be re-applied.
    pub fn tick(&mut self, cfg: &Config) -> Vec<String> {
        let fresh = win::monitors();
        if fresh.len() != self.monitors.len()
            || fresh
                .iter()
                .zip(self.monitors.iter())
                .any(|(a, b)| a.key != b.key || a.rect != b.rect || a.dpi != b.dpi)
        {
            self.monitors = fresh;
        }

        self.parent_mode = cfg.parent_mode;

        // Hosts are only needed for monitors that run (or will run) a video.
        // Static wallpapers are owned by Explorer and must not touch WorkerW,
        // otherwise SetWallpaper recreates the desktop layer and we would
        // re-apply the image forever.
        let needed: Vec<String> = self
            .monitors
            .iter()
            .filter(|monitor| {
                self.instances.contains_key(&monitor.key)
                    || cfg
                        .assignment_for(&monitor.key)
                        .is_some_and(|assignment| {
                            assignment.kind == MediaKind::Video && assignment.path.exists()
                        })
            })
            .map(|monitor| monitor.key.clone())
            .collect();

        let mut changed = self.hosts.sync(&self.monitors, &needed, cfg.parent_mode);

        // A recreated host means the mpv window became invalid.
        for key in &changed {
            if let Some(mut instance) = self.instances.remove(key) {
                instance.stop();
            }
            self.hosts.set_visible(key, false);
        }

        let dead: Vec<String> = self
            .instances
            .iter_mut()
            .filter_map(|(key, instance)| {
                if instance.is_alive() {
                    None
                } else {
                    if let Some(err) = &instance.last_error {
                        self.last_error = Some(err.clone());
                    }
                    Some(key.clone())
                }
            })
            .collect();
        for key in dead {
            if let Some(mut instance) = self.instances.remove(&key) {
                instance.stop();
            }
            self.hosts.set_visible(&key, false);
            changed.push(key);
        }

        // Re-assert the desired pause state periodically. A single IPC write
        // can be lost while mpv is still creating its pipe (especially right
        // after switching wallpapers), which used to leave the pause button
        // desynchronized. Re-sending the idempotent property every tick makes
        // the UI and playback converge again within ~1 second.
        for instance in self.instances.values_mut() {
            instance.reassert_pause();
        }

        changed.sort();
        changed.dedup();
        changed
    }

    pub fn start_video(
        &mut self,
        key: &str,
        path: &Path,
        scaling: Scaling,
        volume: u8,
        muted: bool,
    ) -> Result<()> {
        let mpv = self
            .mpv_path
            .clone()
            .ok_or_else(|| anyhow!("MPV_MISSING"))?;
        let monitor = self
            .monitors
            .iter()
            .find(|monitor| monitor.key == key)
            .cloned()
            .ok_or_else(|| anyhow!("unknown monitor {key}"))?;
        let hwnd = self
            .hosts
            .ensure(&monitor, self.parent_mode)
            .ok_or_else(|| anyhow!("no wallpaper host for monitor {key}"))?;

        // Keep the host visible while swapping videos to avoid a black flash.
        if let Some(mut previous) = self.instances.remove(key) {
            previous.stop();
        }
        self.hosts.set_visible(key, true);

        let mut instance = mpv::MpvInstance::spawn(&mpv, hwnd, path, key, scaling, volume, muted)?;
        if self.effective_pause {
            instance.set_paused(true);
        }
        self.instances.insert(key.to_string(), instance);
        self.last_error = None;
        Ok(())
    }

    pub fn stop_monitor(&mut self, key: &str) {
        log::debug!("stop_monitor {key}");
        if let Some(mut instance) = self.instances.remove(key) {
            instance.stop();
        }
        log::debug!("stop_monitor {key}: hiding host");
        self.hosts.set_visible(key, false);
        log::debug!("stop_monitor {key}: done");
    }

    pub fn stop_all(&mut self) {
        let keys: Vec<String> = self.instances.keys().cloned().collect();
        for key in keys {
            self.stop_monitor(&key);
        }
        self.hosts.show_all(false);
    }

    pub fn shutdown(&mut self) {
        self.stop_all();
    }

    pub fn set_scaling(&mut self, key: &str, scaling: Scaling) {
        if let Some(instance) = self.instances.get_mut(key) {
            instance.set_scaling(scaling);
        }
    }

    pub fn set_volume(&mut self, volume: u8) {
        for instance in self.instances.values_mut() {
            instance.set_volume(volume);
        }
    }

    pub fn set_muted(&mut self, muted: bool) {
        for instance in self.instances.values_mut() {
            instance.set_muted(muted);
        }
    }

    pub fn set_user_paused(&mut self, paused: bool) {
        self.paused_user = paused;
        if paused {
            self.auto_pause_override = false;
        } else {
            // An explicit resume wins over the current auto-pause condition
            // (fullscreen game, occlusion, battery). The override is cleared
            // as soon as the condition disappears.
            self.auto_pause_override = true;
        }
        // Force the write even when the effective state did not change: the
        // user pressed the button, so mpv must receive the command now.
        self.apply_pause(true);
    }

    pub fn toggle_user_paused(&mut self) -> bool {
        let paused = !self.paused_user;
        self.set_user_paused(paused);
        paused
    }

    pub fn is_user_paused(&self) -> bool {
        self.paused_user
    }

    pub fn is_paused(&self) -> bool {
        self.effective_pause
    }

    pub fn env(&self) -> PauseEnv {
        self.paused_env
    }

    pub fn set_env(&mut self, env: PauseEnv, cfg: &Config) {
        self.paused_env = env;
        if !cfg.pause_on_fullscreen {
            self.paused_env.fullscreen = false;
        }
        if !cfg.pause_on_occlusion {
            self.paused_env.occluded = false;
        }
        if !cfg.pause_on_battery {
            self.paused_env.battery = false;
        }
        self.apply_pause(false);
    }

    fn apply_pause(&mut self, force: bool) {
        let env_active = self.paused_env.fullscreen
            || self.paused_env.occluded
            || self.paused_env.battery;
        if !env_active {
            self.auto_pause_override = false;
        }
        let effective =
            self.paused_user || (env_active && !self.auto_pause_override);
        if force || effective != self.effective_pause {
            self.effective_pause = effective;
            for instance in self.instances.values_mut() {
                if force {
                    instance.set_paused_force(effective);
                } else {
                    instance.set_paused(effective);
                }
            }
        }
    }

    pub fn running(&self) -> Vec<(String, PathBuf, bool)> {
        let mut out: Vec<(String, PathBuf, bool)> = self
            .instances
            .iter()
            .map(|(key, instance)| (key.clone(), instance.path.clone(), instance.paused))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    pub fn running_count(&self) -> usize {
        self.instances.len()
    }


}
