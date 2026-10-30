pub mod installer;
pub mod mpv;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

use crate::config::{Config, MediaKind, ParentMode, Scaling};
use crate::platform::host::WallpaperHosts;
use crate::platform::win::{self, MonitorInfo};

#[derive(Clone, Debug, Default)]
pub struct PauseEnv {
    /// At least one monitor is covered by a fullscreen app. Also drives the
    /// global-hotkey suspension.
    pub fullscreen: bool,
    /// Monitor keys (`\\.\DISPLAYn`) covered by a fullscreen app.
    pub fullscreen_monitors: Vec<String>,
    /// At least one monitor is covered by a normal window (UI reason).
    pub occluded: bool,
    /// Monitor keys covered by a normal window.
    pub occluded_monitors: Vec<String>,
    /// The laptop is running on battery (whole desktop).
    pub battery: bool,
}

impl PauseEnv {
    /// True when the environment should pause `monitor`.
    fn per_monitor_active_for(&self, monitor: &str) -> bool {
        self.fullscreen_monitors.iter().any(|key| key == monitor)
            || self.occluded_monitors.iter().any(|key| key == monitor)
    }

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

/// Explicit user resumes that must win over the auto-pause conditions until
/// those conditions disappear. Battery is global; fullscreen and occlusion are
/// tracked per monitor so resuming on one screen does not keep another busy
/// screen playing.
#[derive(Clone, Debug, Default)]
struct PauseOverrides {
    monitors: HashSet<String>,
    battery: bool,
}

impl PauseOverrides {
    fn paused(&self, env: &PauseEnv, monitor: &str, paused_user: bool) -> bool {
        if paused_user {
            return true;
        }
        let battery = env.battery && !self.battery;
        let per_monitor = env.per_monitor_active_for(monitor) && !self.monitors.contains(monitor);
        battery || per_monitor
    }

    /// Records the conditions the user just resumed from.
    fn record(&mut self, env: &PauseEnv) {
        self.monitors = env
            .fullscreen_monitors
            .iter()
            .chain(env.occluded_monitors.iter())
            .cloned()
            .collect();
        self.battery = env.battery;
    }

    /// Forgets overrides whose condition is no longer active.
    fn retain_current(&mut self, env: &PauseEnv) {
        if !env.battery {
            self.battery = false;
        }
        self.monitors
            .retain(|monitor| env.per_monitor_active_for(monitor));
    }

    fn clear(&mut self) {
        self.monitors.clear();
        self.battery = false;
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
    overrides: PauseOverrides,
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
            overrides: PauseOverrides::default(),
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
                .any(|(a, b)| a.key != b.key || a.id != b.id || a.rect != b.rect || a.dpi != b.dpi)
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
        if self.overrides.paused(&self.paused_env, key, self.paused_user) {
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
        let keys: Vec<String> = self.instances.keys().cloned().collect();
        for key in keys {
            if let Some(mut instance) = self.instances.remove(&key) {
                instance.stop_blocking();
            }
        }
        self.hosts.show_all(false);
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
            self.overrides.clear();
        } else {
            // An explicit resume wins over the current auto-pause conditions
            // (fullscreen game, occlusion, battery). Fullscreen overrides are
            // per monitor; global conditions are overridden as a whole. Both
            // are forgotten as soon as the condition disappears.
            self.overrides.record(&self.paused_env);
        }
        // Force the write even when the effective state did not change: the
        // user pressed the button, so mpv must receive the command now.
        self.apply_pause(true);
    }

    pub fn toggle_user_paused(&mut self) -> bool {
        // Toggle the state the user can actually see/hear. Using only the
        // manual flag made the first click a no-op when auto-pause was already
        // active (fullscreen/occlusion): the video stayed paused and the
        // button appeared broken.
        let paused = !self.effective_pause;
        self.set_user_paused(paused);
        paused
    }

    pub fn is_paused(&self) -> bool {
        self.effective_pause
    }

    pub fn env(&self) -> PauseEnv {
        self.paused_env.clone()
    }

    pub fn set_env(&mut self, mut env: PauseEnv, cfg: &Config) {
        if !cfg.pause_on_fullscreen {
            env.fullscreen = false;
            env.fullscreen_monitors.clear();
        }
        if !cfg.pause_on_occlusion {
            env.occluded = false;
        }
        if !cfg.pause_on_battery {
            env.battery = false;
        }
        if env.fullscreen_monitors != self.paused_env.fullscreen_monitors
            || env.occluded_monitors != self.paused_env.occluded_monitors
        {
            log::info!(
                "auto-pause: fullscreen {:?}, occluded {:?}",
                env.fullscreen_monitors,
                env.occluded_monitors
            );
        }
        self.paused_env = env;
        self.apply_pause(false);
    }

    fn apply_pause(&mut self, force: bool) {
        self.overrides.retain_current(&self.paused_env);
        let paused_user = self.paused_user;
        let overrides = &self.overrides;
        let env = &self.paused_env;
        let mut running = 0usize;
        let mut all_paused = true;
        for (key, instance) in self.instances.iter_mut() {
            let paused = overrides.paused(env, key, paused_user);
            if force {
                instance.set_paused_force(paused);
            } else {
                instance.set_paused(paused);
            }
            running += 1;
            all_paused &= paused;
        }
        // Aggregate state for the UI: playback counts as paused only when the
        // user paused it or every running wallpaper is paused. A fullscreen app
        // on one screen must not make the other (still animated) screen look
        // paused.
        self.effective_pause = paused_user || (running > 0 && all_paused);
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

#[cfg(test)]
mod tests {
    use super::{PauseEnv, PauseOverrides};

    fn fullscreen_on(keys: &[&str]) -> PauseEnv {
        PauseEnv {
            fullscreen: true,
            fullscreen_monitors: keys.iter().map(|key| key.to_string()).collect(),
            ..PauseEnv::default()
        }
    }

    fn paused(env: &PauseEnv, monitor: &str, paused_user: bool) -> bool {
        PauseOverrides::default().paused(env, monitor, paused_user)
    }

    fn occluded_on(keys: &[&str]) -> PauseEnv {
        PauseEnv {
            occluded: true,
            occluded_monitors: keys.iter().map(|key| key.to_string()).collect(),
            ..PauseEnv::default()
        }
    }

    #[test]
    fn fullscreen_only_pauses_the_covered_monitors() {
        let env = fullscreen_on(&[r"\\.\DISPLAY2"]);
        assert!(!paused(&env, r"\\.\DISPLAY1", false));
        assert!(paused(&env, r"\\.\DISPLAY2", false));
    }

    #[test]
    fn fullscreen_on_every_monitor_pauses_every_monitor() {
        let env = fullscreen_on(&[r"\\.\DISPLAY1", r"\\.\DISPLAY2"]);
        assert!(paused(&env, r"\\.\DISPLAY1", false));
        assert!(paused(&env, r"\\.\DISPLAY2", false));
    }

    #[test]
    fn no_fullscreen_pauses_nothing() {
        let env = PauseEnv::default();
        assert!(!env.fullscreen && !env.occluded && !env.battery);
        assert!(!paused(&env, r"\\.\DISPLAY1", false));
        assert!(!paused(&env, r"\\.\DISPLAY2", false));
    }

    #[test]
    fn occlusion_only_pauses_the_covered_monitors() {
        // Regression: a maximized window on DISPLAY2 used to pause DISPLAY1
        // through the global occlusion check.
        let env = occluded_on(&[r"\\.\DISPLAY2"]);
        assert!(!paused(&env, r"\\.\DISPLAY1", false));
        assert!(paused(&env, r"\\.\DISPLAY2", false));
    }

    #[test]
    fn battery_still_pauses_every_monitor() {
        let env = PauseEnv {
            battery: true,
            ..PauseEnv::default()
        };
        assert!(paused(&env, r"\\.\DISPLAY1", false));
        assert!(paused(&env, r"\\.\DISPLAY2", false));
    }

    #[test]
    fn occlusion_and_fullscreen_add_up_per_monitor() {
        let mut env = fullscreen_on(&[r"\\.\DISPLAY1"]);
        env.occluded = true;
        env.occluded_monitors = vec![r"\\.\DISPLAY2".to_string()];
        assert!(paused(&env, r"\\.\DISPLAY1", false));
        assert!(paused(&env, r"\\.\DISPLAY2", false));
    }

    #[test]
    fn manual_pause_wins_over_the_environment() {
        let env = occluded_on(&[r"\\.\DISPLAY2"]);
        assert!(paused(&env, r"\\.\DISPLAY1", true));
        assert!(paused(&env, r"\\.\DISPLAY2", true));
    }

    #[test]
    fn resume_override_is_per_monitor() {
        let env = fullscreen_on(&[r"\\.\DISPLAY2"]);
        // The user resumed playback while DISPLAY2 was fullscreen: only that
        // screen is overridden.
        let mut overrides = PauseOverrides::default();
        overrides.record(&env);
        assert!(!overrides.paused(&env, r"\\.\DISPLAY2", false));
        assert!(!overrides.paused(&env, r"\\.\DISPLAY1", false));

        // A new fullscreen app on DISPLAY1 must still pause it.
        let both = fullscreen_on(&[r"\\.\DISPLAY1", r"\\.\DISPLAY2"]);
        overrides.retain_current(&both);
        assert!(overrides.paused(&both, r"\\.\DISPLAY1", false));
        assert!(!overrides.paused(&both, r"\\.\DISPLAY2", false));

        // Once the game closes the override is forgotten.
        let none = PauseEnv::default();
        overrides.retain_current(&none);
        assert!(!overrides.paused(&none, r"\\.\DISPLAY2", false));
        // A later fullscreen app on that screen pauses it again.
        let later = fullscreen_on(&[r"\\.\DISPLAY2"]);
        assert!(overrides.paused(&later, r"\\.\DISPLAY2", false));
    }

    #[test]
    fn resume_override_covers_occlusion_too() {
        let env = occluded_on(&[r"\\.\DISPLAY2"]);
        let mut overrides = PauseOverrides::default();
        overrides.record(&env);
        assert!(!overrides.paused(&env, r"\\.\DISPLAY2", false));
        assert!(!overrides.paused(&env, r"\\.\DISPLAY1", false));

        // The occlusion disappears: the override is forgotten.
        let none = PauseEnv::default();
        overrides.retain_current(&none);
        assert!(overrides.paused(&occluded_on(&[r"\\.\DISPLAY2"]), r"\\.\DISPLAY2", false));
    }
}
