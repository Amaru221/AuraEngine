//! mpv process management through named-pipe IPC.

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde_json::json;
use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::System::Pipes::PeekNamedPipe;

use crate::config::Scaling;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

static PIPE_COUNTER: AtomicU64 = AtomicU64::new(1);

struct Ipc {
    /// Commands are queued immediately and written by a background thread once
    /// mpv's IPC pipe is available. The UI thread never waits for mpv.
    commands: crossbeam_channel::Sender<serde_json::Value>,
}

pub struct MpvInstance {
    child: Option<Child>,
    ipc: Option<Ipc>,
    pub path: PathBuf,
    pub scaling: Scaling,
    pub volume: u8,
    pub muted: bool,
    pub paused: bool,
    /// Number of engine ticks left to re-send the last pause state. This
    /// covers a command lost while mpv was starting without forcing playback
    /// forever (which would fight the Windows media controls).
    pause_reassert: u8,
    pub key: String,
    pub last_error: Option<String>,
}

fn pipe_name(key: &str) -> String {
    let safe: String = key
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let seq = PIPE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        r"\\.\pipe\aura-{}-{}-{}",
        safe.trim_matches('_'),
        std::process::id(),
        seq
    )
}

impl MpvInstance {
    pub fn spawn(
        mpv: &Path,
        hwnd: HWND,
        media: &Path,
        key: &str,
        scaling: Scaling,
        volume: u8,
        muted: bool,
    ) -> Result<Self> {
        let pipe = pipe_name(key);
        let log_file = crate::util::logs_dir().join(format!(
            "mpv-{}.log",
            pipe.rsplit('-').next().unwrap_or("0")
        ));
        let _ = std::fs::create_dir_all(crate::util::logs_dir());

        let mut cmd = Command::new(mpv);
        cmd.arg("--no-config")
            .arg("--idle=yes")
            .arg("--force-window=no")
            .arg("--loop-file=inf")
            .arg("--keep-open=yes")
            .arg("--image-display-duration=inf")
            .arg("--no-terminal")
            .arg("--really-quiet")
            .arg("--no-osc")
            .arg("--no-osd-bar")
            .arg("--osd-level=0")
            .arg("--no-input-default-bindings")
            .arg("--input-terminal=no")
            .arg("--stop-screensaver=no")
            .arg("--cursor-autohide=no")
            .arg("--no-window-dragging")
            .arg("--no-border")
            .arg("--sub-auto=no")
            .arg("--audio-file-auto=no")
            .arg("--vo=gpu")
            .arg("--gpu-context=d3d11")
            .arg("--hwdec=auto-safe")
            .arg("--video-align-x=0")
            .arg("--video-align-y=0")
            .arg("--background-color=#000000")
            .arg(format!("--wid={}", hwnd.0 as usize))
            .arg(format!("--input-ipc-server={pipe}"))
            .arg(format!("--log-file={}", log_file.display()))
            .arg(format!("--volume={volume}"))
            .arg(if muted { "--mute=yes" } else { "--mute=no" });

        for (name, value) in scaling.mpv_properties() {
            cmd.arg(format!("--{name}={value}"));
        }

        cmd.arg(media);

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let child = cmd
            .spawn()
            .with_context(|| format!("failed to start {}", mpv.display()))?;

        // Connecting to the IPC pipe happens on a background thread: clicking a
        // wallpaper must never freeze the window.
        let ipc = Some(Ipc::connect(pipe));

        let instance = Self {
            child: Some(child),
            ipc,
            path: media.to_path_buf(),
            scaling,
            volume,
            muted,
            paused: false,
            pause_reassert: 0,
            key: key.to_string(),
            last_error: None,
        };
        instance.send_command(json!(["set_property", "pause", false]));
        Ok(instance)
    }

    fn send_json(&self, value: serde_json::Value) {
        if let Some(ipc) = &self.ipc {
            log::debug!("mpv queue [{}]: {value}", self.key);
            let _ = ipc.commands.send(value);
        } else {
            log::warn!("mpv queue [{}]: no IPC", self.key);
        }
    }

    pub fn send_command(&self, command: serde_json::Value) {
        self.send_json(json!({ "command": command }));
    }

    pub fn set_scaling(&mut self, scaling: Scaling) {
        self.scaling = scaling;
        for (name, value) in scaling.mpv_properties() {
            let parsed: serde_json::Value = if *value == "yes" || *value == "no" {
                json!(*value == "yes")
            } else if let Ok(number) = value.parse::<f64>() {
                json!(number)
            } else {
                json!(value)
            };
            self.send_command(json!(["set_property", name, parsed]));
        }
    }

    pub fn set_volume(&mut self, volume: u8) {
        self.volume = volume.min(100);
        self.send_command(json!(["set_property", "volume", self.volume]));
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
        self.send_command(json!(["set_property", "mute", muted]));
    }

    pub fn set_paused(&mut self, paused: bool) {
        log::debug!("set_paused [{}] {} (was {})", self.key, paused, self.paused);
        if self.paused != paused {
            self.paused = paused;
            // Keep retrying for a few engine ticks in case the pipe was not
            // ready yet, but stop afterwards so external media controls work.
            self.pause_reassert = 8;
            self.send_command(json!(["set_property", "pause", paused]));
        }
    }

    /// Writes the pause property even when the cached value already matches.
    /// Used for explicit user actions, which must never be a no-op.
    pub fn set_paused_force(&mut self, paused: bool) {
        log::debug!("set_paused_force [{}] {}", self.key, paused);
        self.paused = paused;
        self.pause_reassert = 8;
        self.send_command(json!(["set_property", "pause", paused]));
    }

    /// Re-sends the desired pause state for a short window after a change.
    /// Not permanent: forcing the state every tick would override the Windows
    /// media controls and mask external pause/resume actions.
    pub fn reassert_pause(&mut self) {
        if self.pause_reassert == 0 {
            return;
        }
        self.pause_reassert -= 1;
        self.send_command(json!(["set_property", "pause", self.paused]));
    }

    pub fn is_alive(&mut self) -> bool {
        let status = match self.child.as_mut() {
            Some(child) => child.try_wait(),
            None => return false,
        };
        match status {
            Ok(None) => true,
            Ok(Some(status)) => {
                self.last_error = Some(format!("mpv exited with {status}"));
                self.child = None;
                false
            }
            Err(_) => false,
        }
    }

    pub fn stop(&mut self) {
        self.stop_internal(false);
    }

    /// Used while shutting the application down. The process exit would kill
    /// background reaper threads before they can kill a stuck mpv, leaving an
    /// orphan video process behind.
    pub fn stop_blocking(&mut self) {
        self.stop_internal(true);
    }

    fn stop_internal(&mut self, blocking: bool) {
        log::debug!("stopping mpv for {}", self.key);
        self.send_command(json!(["quit"]));
        self.pause_reassert = 0;

        let Some(mut child) = self.child.take() else {
            return;
        };
        let key = self.key.clone();
        if blocking {
            if reap_child(&mut child, &key) {
                return;
            }
            let _ = child.kill();
            let _ = child.wait();
            return;
        }

        // Reap the process on a background thread so the UI stays responsive.
        std::thread::Builder::new()
            .name("aura-mpv-reaper".into())
            .spawn(move || {
                if reap_child(&mut child, &key) {
                    return;
                }
                let _ = child.kill();
                let _ = child.wait();
            })
            .ok();
    }
}

/// Waits briefly for mpv to quit, returning true when it exited cleanly.
fn reap_child(child: &mut Child, key: &str) -> bool {
    let deadline = Instant::now() + Duration::from_millis(900);
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(_) => return true,
        }
    }
    log::warn!("mpv for {key} did not exit gracefully; killing it");
    false
}

impl Ipc {
    fn connect(pipe: String) -> Self {
        let (commands, rx) = crossbeam_channel::unbounded::<serde_json::Value>();

        // Short-lived connection per command. A persistent connection with a
        // cloned reader handle kept the pipe alive after a write failure, so
        // mpv never accepted a reconnection and all later pause/quit commands
        // were lost. Opening a fresh connection also lets us wait for mpv's
        // acknowledgement before considering the command delivered.
        std::thread::Builder::new()
            .name("aura-mpv-writer".into())
            .spawn(move || {
                while let Ok(value) = rx.recv() {
                    let mut text = value.to_string();
                    text.push('\n');

                    let mut last_error = None;
                    for attempt in 0..3 {
                        match send_pipe_command(&pipe, &text) {
                            Ok(()) => {
                                log::debug!("mpv IPC write: {}", text.trim_end());
                                last_error = None;
                                break;
                            }
                            Err(err) => {
                                last_error = Some(err);
                                if attempt < 2 {
                                    std::thread::sleep(Duration::from_millis(40));
                                }
                            }
                        }
                    }
                    if let Some(err) = last_error {
                        log::warn!("mpv IPC command lost after 3 attempts: {err}");
                    }
                }
            })
            .ok();

        Self { commands }
    }
}

/// Writes one command and waits briefly for mpv's reply. The reply is not
/// parsed (commands are idempotent), but waiting makes a failed write visible
/// and prevents the UI state from claiming success when nothing was sent.
fn send_pipe_command(pipe: &str, text: &str) -> Result<()> {
    let mut file = connect_pipe(pipe, Duration::from_millis(700))?;
    file.write_all(text.as_bytes())
        .and_then(|_| file.flush())?;

    let handle = HANDLE(file.as_raw_handle());
    let deadline = Instant::now() + Duration::from_millis(450);
    let mut buffer = [0u8; 4096];
    loop {
        let mut available = 0u32;
        let peek = unsafe {
            PeekNamedPipe(handle, None, 0, None, Some(&mut available), None)
        };
        if peek.is_err() {
            return Err(anyhow::anyhow!("mpv IPC closed before acknowledging"));
        }
        if available > 0 {
            let _ = file.read(&mut buffer);
            return Ok(());
        }
        if Instant::now() >= deadline {
            // The command was written; never treat a slow acknowledgement as
            // a hard failure because mpv may still have applied it.
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn connect_pipe(pipe: &str, timeout: Duration) -> Result<std::fs::File> {
    let deadline = Instant::now() + timeout;
    loop {
        match OpenOptions::new().read(true).write(true).open(pipe) {
            Ok(file) => return Ok(file),
            Err(err) => {
                if Instant::now() >= deadline {
                    return Err(anyhow::anyhow!("{err}"));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

pub fn resolve_mpv(configured: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = configured {
        if path.is_file() {
            return Some(path.to_path_buf());
        }
    }
    // Portable mode: mpv.exe next to aura.exe.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join("mpv.exe");
            if sibling.is_file() {
                return Some(sibling);
            }
        }
    }
    for candidate in [
        crate::util::bin_dir().join("mpv.exe"),
        crate::util::bin_dir().join("mpv").join("mpv.exe"),
    ] {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    which("mpv.exe")
}

pub fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub fn version(mpv: &Path) -> Option<String> {
    use std::os::windows::process::CommandExt;
    let output = Command::new(mpv)
        .arg("--version")
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines().next().map(|s| s.trim().to_string())
}

/// Extracts a single frame with mpv and stores it as a PNG. This is used for
/// video thumbnails and removes the need for a separate ffmpeg install.
pub fn extract_frame(mpv: &Path, video: &Path, output: &Path, at_seconds: u64) -> bool {
    use std::os::windows::process::CommandExt;
    let _ = std::fs::remove_file(output);
    let status = Command::new(mpv)
        .arg("--no-config")
        .arg("--no-audio")
        .arg("--no-terminal")
        .arg("--really-quiet")
        .arg(format!("--start={at_seconds}"))
        .arg("--frames=1")
        .arg("--vo=image")
        .arg("--ovc=png")
        .arg(format!("--o={}", output.display()))
        .arg(video)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    matches!(status, Ok(s) if s.success()) && output.is_file()
}

