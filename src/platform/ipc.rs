//! Single-instance IPC through a named pipe.
//!
//! The server runs on its own thread so commands keep flowing even while the
//! UI thread is busy starting mpv or decoding a thumbnail.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, GENERIC_WRITE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, WriteFile, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_MODE, OPEN_EXISTING,
    PIPE_ACCESS_INBOUND,
};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
    PIPE_WAIT,
};

use crate::util;

const PIPE_NAME: &str = r"\\.\pipe\aura-engine-ipc";

static QUEUE: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

fn queue() -> &'static Mutex<Vec<String>> {
    QUEUE.get_or_init(|| Mutex::new(Vec::new()))
}

/// Starts the pipe server. Safe to call once per process.
pub fn start_server() {
    std::thread::Builder::new()
        .name("aura-ipc".into())
        .spawn(|| unsafe { server_loop() })
        .ok();
}

unsafe fn server_loop() {
    let name = util::to_wide(PIPE_NAME);
    loop {
        let handle = unsafe {
            CreateNamedPipeW(
                PCWSTR(name.as_ptr()),
                PIPE_ACCESS_INBOUND,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                1,
                8192,
                8192,
                0,
                None,
            )
        };
        if handle.is_invalid() {
            std::thread::sleep(Duration::from_millis(250));
            continue;
        }

        // Blocks until a client connects. ERROR_PIPE_CONNECTED means it is
        // already connected, in which case reading is valid too.
        let _ = unsafe { ConnectNamedPipe(handle, None) };

        let mut buffer = vec![0u8; 8192];
        let mut read = 0u32;
        if unsafe { ReadFile(handle, Some(&mut buffer), Some(&mut read), None) }.is_ok()
            && read > 0
        {
            buffer.truncate(read as usize);
            let text = String::from_utf8_lossy(&buffer)
                .trim_matches('\0')
                .trim()
                .to_string();
            if !text.is_empty() {
                log::info!("IPC command: {text}");
                queue()
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(text);
            }
        }

        unsafe {
            let _ = DisconnectNamedPipe(handle);
            let _ = CloseHandle(handle);
        }
    }
}

fn open_pipe() -> Option<windows::Win32::Foundation::HANDLE> {
    let name = util::to_wide(PIPE_NAME);
    unsafe {
        CreateFileW(
            PCWSTR(name.as_ptr()),
            GENERIC_WRITE.0,
            FILE_SHARE_MODE(0),
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
        .ok()
    }
}

/// Sends a command to the running instance. Returns false when none is up.
pub fn send(command: &str) -> bool {
    // A normal launch used to spend three seconds probing for another
    // instance before showing anything. Half a second is enough to catch a
    // simultaneous second launch and keeps startup snappy.
    let deadline = Instant::now() + Duration::from_millis(500);
    loop {
        if let Some(handle) = open_pipe() {
            let mut written = 0u32;
            let result = unsafe {
                WriteFile(
                    handle,
                    Some(command.as_bytes()),
                    Some(&mut written),
                    None,
                )
            };
            unsafe {
                let _ = CloseHandle(handle);
            }
            return result.is_ok();
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(60));
    }
}

pub fn exists() -> bool {
    match open_pipe() {
        Some(handle) => {
            unsafe {
                let _ = CloseHandle(handle);
            }
            true
        }
        None => false,
    }
}

/// Drains the queue of commands received by the server thread.
pub fn poll() -> Vec<String> {
    let mut queue = queue().lock().unwrap_or_else(|e| e.into_inner());
    std::mem::take(&mut *queue)
}

