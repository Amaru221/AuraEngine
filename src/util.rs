//! Small cross-cutting helpers: paths, logging, hashing, console output.

use std::io::Write;
use std::os::windows::io::FromRawHandle;
use std::path::{Path, PathBuf};
use std::sync::{Once, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_WRITE, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Console::{
    AttachConsole, GetStdHandle, ATTACH_PARENT_PROCESS, STD_OUTPUT_HANDLE,
};

static CONFIG_ROOT: OnceLock<PathBuf> = OnceLock::new();
static DATA_ROOT: OnceLock<PathBuf> = OnceLock::new();

/// Directory that holds user configuration (`%APPDATA%\Aura`).
pub fn config_root() -> PathBuf {
    CONFIG_ROOT
        .get_or_init(|| {
            dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("Aura")
        })
        .clone()
}

/// Directory that holds application data and cache (`%LOCALAPPDATA%\Aura`).
pub fn data_root() -> PathBuf {
    DATA_ROOT
        .get_or_init(|| {
            dirs::data_local_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("Aura")
        })
        .clone()
}

pub fn cache_root() -> PathBuf {
    data_root().join("cache")
}

pub fn thumbs_dir() -> PathBuf {
    cache_root().join("thumbs")
}

pub fn wallpapers_dir() -> PathBuf {
    data_root().join("wallpapers")
}

pub fn online_dir() -> PathBuf {
    wallpapers_dir().join("online")
}

pub fn bin_dir() -> PathBuf {
    data_root().join("bin")
}

pub fn logs_dir() -> PathBuf {
    data_root().join("logs")
}

pub fn log_path() -> PathBuf {
    logs_dir().join("aura.log")
}

pub fn config_path() -> PathBuf {
    config_root().join("config.json")
}

pub fn ensure_dirs() {
    for dir in [
        config_root(),
        data_root(),
        cache_root(),
        thumbs_dir(),
        wallpapers_dir(),
        online_dir(),
        bin_dir(),
        logs_dir(),
    ] {
        let _ = std::fs::create_dir_all(dir);
    }
}

/// Installs a file logger. Safe to call once; later calls are ignored.
pub fn init_logging() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        ensure_dirs();
        let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path())
        else {
            return;
        };

        let config = simplelog::ConfigBuilder::new()
            .set_time_offset_to_local()
            .unwrap_or_else(|b| b)
            .build();
        let _ = simplelog::WriteLogger::init(log::LevelFilter::Info, config, file);
    });
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Stable cache key for a file based on path + size + modification time.
pub fn file_cache_key(path: &Path, size: u64, modified_ms: u128) -> String {
    let seed = format!("{}|{}|{}", path.to_string_lossy(), size, modified_ms);
    sha256_hex(seed.as_bytes())
}

pub fn cache_key_for_text(text: &str) -> String {
    sha256_hex(text.as_bytes())
}

pub fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

#[allow(dead_code)]
pub fn modified_ms(path: &Path) -> u128 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

#[allow(dead_code)]
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

pub fn sanitize_filename(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ' ') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    let trimmed = out.trim().trim_matches('.').trim().to_string();
    if trimmed.is_empty() {
        "wallpaper".to_string()
    } else {
        trimmed.chars().take(96).collect()
    }
}

pub fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn pcwstr(buf: &[u16]) -> PCWSTR {
    PCWSTR(buf.as_ptr())
}

pub unsafe fn from_wide_ptr(ptr: *const u16) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let mut len = 0usize;
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len))
    }
}

pub fn from_wide_buf(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

pub fn reveal_in_explorer(path: &Path) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut cmd = std::process::Command::new("explorer.exe");
        if path.is_dir() {
            cmd.arg(path);
        } else {
            cmd.raw_arg(format!("/select,\"{}\"", path.display()));
        }
        let _ = cmd.spawn();
    }
}

pub fn open_url(url: &str) {
    let _ = open::that(url);
}

pub fn open_path(path: &Path) {
    let _ = open::that(path);
}

pub fn spawn_detached(exe: &Path, args: &[String], hidden: bool) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut cmd = std::process::Command::new(exe);
    cmd.args(args);
    if hidden {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = cmd.spawn();
}

// ---------------------------------------------------------------------------
// Console support for a GUI-subsystem executable
// ---------------------------------------------------------------------------

fn console_handle() -> Option<std::fs::File> {
    unsafe {
        let name = to_wide("CONOUT$");
        let handle = CreateFileW(
            pcwstr(&name),
            FILE_GENERIC_WRITE.0,
            FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
        .ok()?;
        Some(std::fs::File::from_raw_handle(handle.0 as _))
    }
}

/// Attaches to the parent console when the process was started from a shell.
pub fn attach_console() {
    unsafe {
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

/// Writes to stdout when it exists, otherwise to the attached console.
pub fn console_out(text: &str) {
    #[cfg(windows)]
    {
        // Prefer the inherited stdout so pipes, terminals and test harnesses
        // capture the output.
        if let Ok(handle) = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) } {
            if !handle.is_invalid() {
                use std::mem::ManuallyDrop;
                let mut file = ManuallyDrop::new(unsafe {
                    std::fs::File::from_raw_handle(handle.0 as _)
                });
                let mut line = String::with_capacity(text.len() + 2);
                line.push_str(text);
                line.push_str("\r\n");
                let _ = file.write_all(line.as_bytes());
                let _ = file.flush();
                return;
            }
        }
        if let Some(mut file) = console_handle() {
            let _ = file.write_all(text.as_bytes());
            let _ = file.write_all(b"\r\n");
            return;
        }
    }
    println!("{text}");
    let _ = std::io::stdout().flush();
}

pub fn console_err(text: &str) {
    eprintln!("{text}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_names() {
        assert_eq!(sanitize_filename("my video: 01.mp4"), "my video_ 01.mp4");
        assert_eq!(sanitize_filename("..."), "wallpaper");
        assert_eq!(sanitize_filename("ok_name-1"), "ok_name-1");
    }

    #[test]
    fn hashes_are_stable() {
        assert_eq!(sha256_hex(b"aura"), sha256_hex(b"aura"));
        assert_ne!(sha256_hex(b"aura"), sha256_hex(b"aura2"));
        assert_eq!(sha256_hex(b"aura").len(), 64);
    }

    #[test]
    fn sizes_are_human_readable() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(2048), "2.0 KB");
    }
}
